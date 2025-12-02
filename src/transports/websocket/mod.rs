// WebSocket Transport - bidirectional communication
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use base64::Engine;
use futures::{SinkExt, StreamExt};
use reqwest::Url;
use serde_json::Value;
use std::collections::HashMap;
use std::str::FromStr;
use tokio::sync::mpsc;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{
        client::IntoClientRequest,
        http::{HeaderName, HeaderValue, Request},
        protocol::Message,
    },
};

use crate::auth::AuthConfig;
use crate::providers::base::Provider;
use crate::providers::websocket::WebSocketProvider;
use crate::tools::{Tool, ToolInputOutputSchema};
use crate::transports::{
    stream::{boxed_channel_stream, StreamResult},
    ClientTransport,
};

/// Transport that communicates with tools over WebSocket connections.
pub struct WebSocketTransport;

impl WebSocketTransport {
    /// Create a WebSocket transport.
    pub fn new() -> Self {
        Self
    }

    fn default_schema() -> ToolInputOutputSchema {
        ToolInputOutputSchema {
            type_: "object".to_string(),
            properties: None,
            required: None,
            description: None,
            title: None,
            items: None,
            enum_: None,
            minimum: None,
            maximum: None,
            format: None,
        }
    }

    fn apply_auth_to_url(&self, url: &str, auth: &AuthConfig) -> Result<String> {
        match auth {
            AuthConfig::ApiKey(api_key) => {
                let location = api_key.location.to_ascii_lowercase();
                if location == "query" {
                    let mut parsed = Url::parse(url)?;
                    parsed
                        .query_pairs_mut()
                        .append_pair(&api_key.var_name, &api_key.api_key);
                    Ok(parsed.to_string())
                } else {
                    Ok(url.to_string())
                }
            }
            _ => Ok(url.to_string()),
        }
    }

    fn apply_auth_headers(&self, req: &mut Request<()>, auth: &AuthConfig) -> Result<()> {
        match auth {
            AuthConfig::ApiKey(api_key) => {
                if api_key.location.to_ascii_lowercase() == "header" {
                    let name = HeaderName::from_str(&api_key.var_name)?;
                    req.headers_mut()
                        .insert(name, HeaderValue::from_str(&api_key.api_key)?);
                } else if api_key.location.to_ascii_lowercase() == "cookie" {
                    let cookie_val = format!("{}={}", api_key.var_name, api_key.api_key);
                    req.headers_mut()
                        .insert("cookie", HeaderValue::from_str(&cookie_val)?);
                }
                Ok(())
            }
            AuthConfig::Basic(basic) => {
                let encoded = base64::engine::general_purpose::STANDARD
                    .encode(format!("{}:{}", basic.username, basic.password));
                req.headers_mut().insert(
                    "authorization",
                    HeaderValue::from_str(&format!("Basic {}", encoded))?,
                );
                Ok(())
            }
            AuthConfig::OAuth2(_) => Err(anyhow!(
                "OAuth2 auth is not yet supported by the WebSocket transport"
            )),
        }
    }

    fn build_request(&self, prov: &WebSocketProvider, url: &str) -> Result<Request<()>> {
        let mut url = url.to_string();
        if let Some(auth) = &prov.base.auth {
            url = self.apply_auth_to_url(&url, auth)?;
        }

        let mut req = url.into_client_request()?;
        if let Some(headers) = &prov.headers {
            for (k, v) in headers {
                let name = HeaderName::from_str(k)?;
                req.headers_mut().insert(name, HeaderValue::from_str(v)?);
            }
        }
        if let Some(proto) = &prov.protocol {
            req.headers_mut()
                .insert("Sec-WebSocket-Protocol", HeaderValue::from_str(proto)?);
        }
        if let Some(auth) = &prov.base.auth {
            self.apply_auth_headers(&mut req, auth)?;
        }
        Ok(req)
    }
}

#[async_trait]
impl ClientTransport for WebSocketTransport {
    async fn register_tool_provider(&self, prov: &dyn Provider) -> Result<Vec<Tool>> {
        let ws_prov = prov
            .as_any()
            .downcast_ref::<WebSocketProvider>()
            .ok_or_else(|| anyhow!("Provider is not a WebSocketProvider"))?;

        let req = self.build_request(ws_prov, &ws_prov.url)?;
        let (mut ws_stream, _) = connect_async(req).await?;

        // Request manual/tool list
        ws_stream.send(Message::Text("manual".to_string())).await?;
        if let Some(msg) = ws_stream.next().await {
            if let Ok(Message::Text(text)) = msg {
                if let Ok(manifest) = serde_json::from_str::<Value>(&text) {
                    if let Some(tools) = manifest.get("tools").and_then(|v| v.as_array()) {
                        let mut parsed = Vec::new();
                        for t in tools {
                            if let Ok(tool) = serde_json::from_value::<Tool>(t.clone()) {
                                parsed.push(tool);
                            } else if let Some(name) = t.get("name").and_then(|v| v.as_str()) {
                                let schema = Self::default_schema();
                                parsed.push(Tool {
                                    name: name.to_string(),
                                    description: t
                                        .get("description")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or_default()
                                        .to_string(),
                                    inputs: schema.clone(),
                                    outputs: schema,
                                    tags: vec![],
                                    average_response_size: None,
                                    provider: None,
                                });
                            }
                        }
                        return Ok(parsed);
                    }
                }
            }
        }

        Ok(vec![])
    }

    async fn deregister_tool_provider(&self, _prov: &dyn Provider) -> Result<()> {
        Ok(())
    }

    async fn call_tool(
        &self,
        tool_name: &str,
        args: HashMap<String, Value>,
        prov: &dyn Provider,
    ) -> Result<Value> {
        let ws_prov = prov
            .as_any()
            .downcast_ref::<WebSocketProvider>()
            .ok_or_else(|| anyhow!("Provider is not a WebSocketProvider"))?;

        let call_name = tool_name
            .strip_prefix(&format!("{}.", ws_prov.base.name))
            .unwrap_or(tool_name);

        let mut base_url = ws_prov.url.trim_end_matches('/').to_string();
        if base_url.ends_with("/tools") {
            base_url = base_url.trim_end_matches("/tools").to_string();
        }
        let url = format!("{}/{}", base_url, call_name);

        let req = self.build_request(ws_prov, &url)?;
        let (mut ws_stream, _) = connect_async(req).await?;

        let payload = serde_json::to_string(&args)?;
        ws_stream.send(Message::Text(payload)).await?;

        let mut results = Vec::new();
        while let Some(msg) = ws_stream.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    let value = serde_json::from_str::<Value>(&text)
                        .unwrap_or_else(|_| Value::String(text));
                    results.push(value);
                }
                Ok(Message::Binary(bin)) => {
                    if let Ok(text) = String::from_utf8(bin) {
                        let value = serde_json::from_str::<Value>(&text)
                            .unwrap_or_else(|_| Value::String(text));
                        results.push(value);
                    }
                }
                Ok(Message::Close(_)) | Err(_) => break,
                _ => {}
            }
        }

        Ok(Value::Array(results))
    }

    async fn call_tool_stream(
        &self,
        tool_name: &str,
        args: HashMap<String, Value>,
        prov: &dyn Provider,
    ) -> Result<Box<dyn StreamResult>> {
        let ws_prov = prov
            .as_any()
            .downcast_ref::<WebSocketProvider>()
            .ok_or_else(|| anyhow!("Provider is not a WebSocketProvider"))?;

        let call_name = tool_name
            .strip_prefix(&format!("{}.", ws_prov.base.name))
            .unwrap_or(tool_name);

        let mut base_url = ws_prov.url.trim_end_matches('/').to_string();
        if base_url.ends_with("/tools") {
            base_url = base_url.trim_end_matches("/tools").to_string();
        }
        let url = format!("{}/{}", base_url, call_name);

        let req = self.build_request(ws_prov, &url)?;
        let (mut ws_stream, _) = connect_async(req).await?;

        ws_stream
            .send(Message::Text(serde_json::to_string(&args)?))
            .await?;

        let (tx, rx) = mpsc::channel(256);
        tokio::spawn(async move {
            while let Some(msg) = ws_stream.next().await {
                match msg {
                    Ok(Message::Text(text)) => {
                        let parsed = serde_json::from_str::<Value>(&text)
                            .map_err(|e| anyhow!("Failed to parse WebSocket message: {}", e));
                        if tx.send(parsed).await.is_err() {
                            return;
                        }
                    }
                    Ok(Message::Binary(bin)) => {
                        if let Ok(text) = String::from_utf8(bin) {
                            let parsed = serde_json::from_str::<Value>(&text)
                                .map_err(|e| anyhow!("Failed to parse WebSocket message: {}", e));
                            if tx.send(parsed).await.is_err() {
                                return;
                            }
                        }
                    }
                    Ok(Message::Close(_)) => break,
                    Ok(_) => {}
                    Err(err) => {
                        let _ = tx
                            .send(Err(anyhow!("WebSocket receive error: {}", err)))
                            .await;
                        break;
                    }
                }
            }
        });

        Ok(boxed_channel_stream(rx, None))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{ApiKeyAuth, AuthType, BasicAuth};
    use crate::providers::base::{BaseProvider, ProviderType};
    use serde_json::json;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    };
    use tokio::net::TcpListener;
    use tokio_tungstenite::tungstenite::Message;

    #[test]
    fn apply_auth_to_url_appends_query_param() {
        let transport = WebSocketTransport::new();
        let auth = AuthConfig::ApiKey(ApiKeyAuth {
            auth_type: AuthType::ApiKey,
            api_key: "token".to_string(),
            var_name: "auth".to_string(),
            location: "query".to_string(),
        });

        let url = transport
            .apply_auth_to_url("ws://example.com/socket", &auth)
            .unwrap();
        assert!(url.contains("auth=token"));
    }

    #[test]
    fn apply_auth_headers_supports_basic_and_cookie() {
        let transport = WebSocketTransport::new();

        let basic_auth = AuthConfig::Basic(BasicAuth {
            auth_type: AuthType::Basic,
            username: "user".to_string(),
            password: "pass".to_string(),
        });
        let mut req = "ws://example.com".into_client_request().unwrap();
        transport
            .apply_auth_headers(&mut req, &basic_auth)
            .expect("basic auth applied");
        assert_eq!(
            req.headers().get("authorization").unwrap(),
            "Basic dXNlcjpwYXNz"
        );

        let cookie_auth = AuthConfig::ApiKey(ApiKeyAuth {
            auth_type: AuthType::ApiKey,
            api_key: "secret".to_string(),
            var_name: "session".to_string(),
            location: "cookie".to_string(),
        });
        let mut req = "ws://example.com".into_client_request().unwrap();
        transport
            .apply_auth_headers(&mut req, &cookie_auth)
            .expect("cookie auth applied");
        assert_eq!(req.headers().get("cookie").unwrap(), "session=secret");
    }

    #[test]
    fn build_request_includes_provider_headers_and_protocol() {
        let transport = WebSocketTransport::new();
        let prov = WebSocketProvider {
            base: BaseProvider {
                name: "ws".to_string(),
                provider_type: ProviderType::Websocket,
                auth: Some(AuthConfig::ApiKey(ApiKeyAuth {
                    auth_type: AuthType::ApiKey,
                    api_key: "abc".to_string(),
                    var_name: "X-Key".to_string(),
                    location: "header".to_string(),
                })),
                allowed_communication_protocols: None,
            },
            url: "ws://example.com/socket".to_string(),
            protocol: Some("json".to_string()),
            keep_alive: false,
            headers: Some(HashMap::from([("X-Custom".to_string(), "1".to_string())])),
        };

        let req = transport.build_request(&prov, &prov.url).unwrap();
        assert_eq!(req.uri().to_string(), prov.url);
        assert_eq!(req.headers().get("X-Custom").unwrap(), "1");
        assert_eq!(req.headers().get("Sec-WebSocket-Protocol").unwrap(), "json");
        assert_eq!(req.headers().get("X-Key").unwrap(), "abc");
    }

    #[tokio::test]
    async fn register_call_and_stream_over_websocket() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();

        tokio::spawn(async move {
            for _ in 0..3 {
                let (stream, _) = listener.accept().await.unwrap();
                let idx = counter_clone.fetch_add(1, Ordering::SeqCst);
                tokio::spawn(async move {
                    let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
                    match idx {
                        0 => {
                            // Manual lookup
                            let _ = ws.next().await;
                            let manifest = json!({
                                "tools": [{
                                    "name": "echo",
                                    "description": "echo tool",
                                    "inputs": { "type": "object" },
                                    "outputs": { "type": "object" },
                                    "tags": []
                                }]
                            });
                            let _ = ws.send(Message::Text(manifest.to_string())).await;
                        }
                        1 => {
                            if let Some(Ok(Message::Text(text))) = ws.next().await {
                                let parsed: Value =
                                    serde_json::from_str(&text).unwrap_or_else(|_| Value::Null);
                                let reply = json!({ "echo": parsed });
                                let _ = ws.send(Message::Text(reply.to_string())).await;
                                let _ = ws.close(None).await;
                            }
                        }
                        _ => {
                            let _ = ws.next().await;
                            let _ = ws
                                .send(Message::Text(json!({ "idx": 1 }).to_string()))
                                .await;
                            let _ = ws
                                .send(Message::Text(json!({ "idx": 2 }).to_string()))
                                .await;
                            let _ = ws.close(None).await;
                        }
                    }
                });
            }
        });

        let prov = WebSocketProvider {
            base: BaseProvider {
                name: "ws".to_string(),
                provider_type: ProviderType::Websocket,
                auth: None,
                allowed_communication_protocols: None,
            },
            url: format!("ws://{}/tools", addr),
            protocol: None,
            keep_alive: false,
            headers: None,
        };

        let transport = WebSocketTransport::new();

        let tools = transport
            .register_tool_provider(&prov)
            .await
            .expect("register tools");
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "echo");

        let mut args = HashMap::new();
        args.insert("msg".into(), Value::String("hello".into()));

        let call_value = transport
            .call_tool("echo", args.clone(), &prov)
            .await
            .expect("call tool");
        assert_eq!(call_value, json!([json!({ "echo": json!(args) })]));

        let mut stream = transport
            .call_tool_stream("stream", args, &prov)
            .await
            .expect("call tool stream");
        assert_eq!(stream.next().await.unwrap().unwrap(), json!({ "idx": 1 }));
        assert_eq!(stream.next().await.unwrap().unwrap(), json!({ "idx": 2 }));
        stream.close().await.unwrap();
    }

    #[tokio::test]
    async fn websocket_strips_provider_prefix() {
        use tokio_tungstenite::tungstenite::handshake::server::{Request, Response};

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let seen_paths = Arc::new(Mutex::new(Vec::new()));
        let seen_paths_clone = seen_paths.clone();

        tokio::spawn(async move {
            for idx in 0..2 {
                let (stream, _) = listener.accept().await.unwrap();
                let seen_paths = seen_paths_clone.clone();
                tokio::spawn(async move {
                    let cb = |req: &Request, resp: Response| {
                        if let Ok(mut guard) = seen_paths.lock() {
                            guard.push(req.uri().path().to_string());
                        }
                        Ok(resp)
                    };
                    let mut ws = tokio_tungstenite::accept_hdr_async(stream, cb)
                        .await
                        .unwrap();

                    if idx == 0 {
                        if let Some(Ok(Message::Text(msg))) = ws.next().await {
                            if msg == "manual" {
                                let manifest = json!({
                                    "tools": [{
                                        "name": "echo",
                                        "description": "echo tool",
                                        "inputs": { "type": "object" },
                                        "outputs": { "type": "object" },
                                        "tags": []
                                    }]
                                });
                                let _ = ws.send(Message::Text(manifest.to_string())).await;
                            }
                        }
                    } else {
                        if let Some(Ok(Message::Text(text))) = ws.next().await {
                            let val: Value =
                                serde_json::from_str(&text).unwrap_or_else(|_| Value::Null);
                            let _ = ws
                                .send(Message::Text(json!({ "echo": val }).to_string()))
                                .await;
                            let _ = ws.close(None).await;
                        }
                    }
                });
            }
        });

        let prov = WebSocketProvider {
            base: BaseProvider {
                name: "wsdemo".to_string(),
                provider_type: ProviderType::Websocket,
                auth: None,
                allowed_communication_protocols: None,
            },
            url: format!("ws://{}/tools", addr),
            protocol: None,
            keep_alive: false,
            headers: None,
        };

        let transport = WebSocketTransport::new();
        let tools = transport
            .register_tool_provider(&prov)
            .await
            .expect("register tools");
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "echo");

        let mut args = HashMap::new();
        args.insert("msg".into(), Value::String("hi".into()));
        let value = transport
            .call_tool("wsdemo.echo", args.clone(), &prov)
            .await
            .expect("prefixed call");
        assert_eq!(value, json!([json!({ "echo": json!(args) })]));

        let paths = seen_paths.lock().unwrap().clone();
        assert_eq!(paths, vec!["/tools".to_string(), "/echo".to_string()]);
    }
}
