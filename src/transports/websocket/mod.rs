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

pub struct WebSocketTransport;

impl WebSocketTransport {
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

        let mut base_url = ws_prov.url.trim_end_matches('/').to_string();
        if base_url.ends_with("/tools") {
            base_url = base_url.trim_end_matches("/tools").to_string();
        }
        let url = format!("{}/{}", base_url, tool_name);

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

        let mut base_url = ws_prov.url.trim_end_matches('/').to_string();
        if base_url.ends_with("/tools") {
            base_url = base_url.trim_end_matches("/tools").to_string();
        }
        let url = format!("{}/{}", base_url, tool_name);

        let req = self.build_request(ws_prov, &url)?;
        let (mut ws_stream, _) = connect_async(req).await?;

        ws_stream
            .send(Message::Text(serde_json::to_string(&args)?))
            .await?;

        let (tx, rx) = mpsc::channel(16);
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
