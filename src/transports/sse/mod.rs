// SSE (Server-Sent Events) Transport
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use bytes::Bytes;
use futures::StreamExt;
use reqwest::{header, Client};
use serde_json::{json, Value};
use std::collections::HashMap;
use tokio::sync::mpsc;

use crate::auth::AuthConfig;
use crate::providers::base::Provider;
use crate::providers::sse::SseProvider;
use crate::tools::Tool;
use crate::transports::{
    stream::{boxed_channel_stream, StreamResult},
    ClientTransport,
};

pub struct SseTransport {
    client: Client,
}

impl SseTransport {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }

    fn build_payload(&self, prov: &SseProvider, args: HashMap<String, Value>) -> Value {
        if let Some(body_field) = &prov.body_field {
            json!({ body_field: args })
        } else {
            json!(args)
        }
    }

    fn value_to_header(value: &Value) -> Option<String> {
        match value {
            Value::String(s) => Some(s.clone()),
            Value::Number(n) => Some(n.to_string()),
            Value::Bool(b) => Some(b.to_string()),
            _ => None,
        }
    }

    fn split_headers_from_args(
        &self,
        prov: &SseProvider,
        mut args: HashMap<String, Value>,
    ) -> (HashMap<String, String>, HashMap<String, Value>) {
        let mut headers = HashMap::new();
        if let Some(header_fields) = &prov.header_fields {
            for field in header_fields {
                if let Some(value) = args.remove(field) {
                    if let Some(header_value) = Self::value_to_header(&value) {
                        headers.insert(field.clone(), header_value);
                    }
                }
            }
        }
        (headers, args)
    }

    fn apply_headers(
        &self,
        request: reqwest::RequestBuilder,
        prov: &SseProvider,
        extra_accept: Option<&str>,
        dynamic_headers: &HashMap<String, String>,
    ) -> reqwest::RequestBuilder {
        let mut builder = request;
        builder = builder.header("Accept", extra_accept.unwrap_or("application/json"));
        if let Some(headers) = &prov.headers {
            for (k, v) in headers {
                builder = builder.header(k, v);
            }
        }
        for (k, v) in dynamic_headers {
            builder = builder.header(k, v);
        }
        builder
    }

    fn apply_auth(
        &self,
        builder: reqwest::RequestBuilder,
        auth: &AuthConfig,
    ) -> Result<reqwest::RequestBuilder> {
        match auth {
            AuthConfig::ApiKey(api_key) => {
                let location = api_key.location.to_ascii_lowercase();
                match location.as_str() {
                    "header" => Ok(builder.header(&api_key.var_name, &api_key.api_key)),
                    "query" => {
                        Ok(builder.query(&[(api_key.var_name.clone(), api_key.api_key.clone())]))
                    }
                    "cookie" => {
                        let cookie_value = format!("{}={}", api_key.var_name, api_key.api_key);
                        Ok(builder.header(header::COOKIE, cookie_value))
                    }
                    other => Err(anyhow!("Unsupported API key location: {}", other)),
                }
            }
            AuthConfig::Basic(basic) => {
                Ok(builder.basic_auth(&basic.username, Some(&basic.password)))
            }
            AuthConfig::OAuth2(_) => Err(anyhow!(
                "OAuth2 auth is not yet supported by the SSE transport"
            )),
        }
    }

    fn parse_tools_from_body(&self, body: &str) -> Vec<Tool> {
        if let Ok(manifest) = serde_json::from_str::<Value>(body) {
            if let Some(tools) = manifest.get("tools").and_then(|v| v.as_array()) {
                let mut parsed = Vec::new();
                for t in tools {
                    if let Ok(tool) = serde_json::from_value::<Tool>(t.clone()) {
                        parsed.push(tool);
                    }
                }
                return parsed;
            }
        }
        vec![]
    }

    fn spawn_sse_reader(
        &self,
        mut stream: impl futures::Stream<Item = Result<Bytes, reqwest::Error>> + Send + Unpin + 'static,
    ) -> mpsc::Receiver<Result<Value>> {
        let (tx, rx) = mpsc::channel(16);
        tokio::spawn(async move {
            let mut buffer = String::new();
            let mut data_buf = String::new();

            while let Some(chunk_res) = stream.next().await {
                match chunk_res {
                    Ok(bytes) => {
                        buffer.push_str(&String::from_utf8_lossy(&bytes));
                        loop {
                            if let Some(pos) = buffer.find('\n') {
                                let mut line = buffer[..pos].to_string();
                                buffer.drain(..=pos);
                                line = line.trim_end_matches('\r').to_string();

                                if line.starts_with("data: ") {
                                    if !data_buf.is_empty() {
                                        data_buf.push('\n');
                                    }
                                    data_buf.push_str(&line[6..]);
                                } else if line.is_empty() {
                                    if !data_buf.is_empty() {
                                        let parsed = serde_json::from_str::<Value>(&data_buf)
                                            .map_err(|e| {
                                                anyhow!("Failed to parse SSE data: {}", e)
                                            });
                                        if tx.send(parsed).await.is_err() {
                                            return;
                                        }
                                        data_buf.clear();
                                    }
                                }
                            } else {
                                break;
                            }
                        }
                    }
                    Err(err) => {
                        let _ = tx
                            .send(Err(anyhow!("Error reading SSE stream: {}", err)))
                            .await;
                        return;
                    }
                }
            }

            // Flush trailing data if present
            if !data_buf.is_empty() {
                let _ = tx
                    .send(
                        serde_json::from_str::<Value>(&data_buf)
                            .map_err(|e| anyhow!("Failed to parse SSE data: {}", e)),
                    )
                    .await;
            }
        });
        rx
    }
}

#[async_trait]
impl ClientTransport for SseTransport {
    async fn register_tool_provider(&self, prov: &dyn Provider) -> Result<Vec<Tool>> {
        let sse_prov = prov
            .as_any()
            .downcast_ref::<SseProvider>()
            .ok_or_else(|| anyhow!("Provider is not an SseProvider"))?;

        let mut request = self
            .client
            .get(&sse_prov.url)
            .header("Accept", "application/json");
        request = self.apply_headers(request, sse_prov, None, &HashMap::new());
        if let Some(auth) = &sse_prov.base.auth {
            request = self.apply_auth(request, auth)?;
        }

        let response = request.send().await?;
        if !response.status().is_success() {
            return Err(anyhow!(
                "Failed to fetch tools from {}: {}",
                sse_prov.url,
                response.status()
            ));
        }

        let body = response.text().await?;
        Ok(self.parse_tools_from_body(&body))
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
        // Use streaming parser and eagerly collect values.
        let mut stream = self.call_tool_stream(tool_name, args, prov).await?;
        let mut items = Vec::new();
        while let Some(item) = stream.next().await? {
            items.push(item);
        }
        stream.close().await?;
        Ok(Value::Array(items))
    }

    async fn call_tool_stream(
        &self,
        tool_name: &str,
        args: HashMap<String, Value>,
        prov: &dyn Provider,
    ) -> Result<Box<dyn StreamResult>> {
        let sse_prov = prov
            .as_any()
            .downcast_ref::<SseProvider>()
            .ok_or_else(|| anyhow!("Provider is not an SseProvider"))?;

        let call_name = tool_name
            .strip_prefix(&format!("{}.", sse_prov.base.name))
            .unwrap_or(tool_name);
        let url = format!("{}/{}", sse_prov.url.trim_end_matches('/'), call_name);
        let (header_args, payload_args) = self.split_headers_from_args(sse_prov, args);
        let payload = self.build_payload(sse_prov, payload_args);

        let mut request = self
            .client
            .post(url)
            .header("Content-Type", "application/json");
        request = self.apply_headers(request, sse_prov, Some("text/event-stream"), &header_args);
        if let Some(auth) = &sse_prov.base.auth {
            request = self.apply_auth(request, auth)?;
        }
        let response = request.json(&payload).send().await?;

        if !response.status().is_success() {
            return Err(anyhow!("SSE request failed: {}", response.status()));
        }

        let rx = self.spawn_sse_reader(response.bytes_stream());
        Ok(boxed_channel_stream(rx, None))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::base::{BaseProvider, ProviderType};
    use axum::{body::Body, extract::Json, http::Response, routing::get, routing::post, Router};
    use bytes::Bytes;
    use serde_json::json;
    use std::net::TcpListener;

    #[test]
    fn build_payload_respects_body_field() {
        let transport = SseTransport::new();
        let mut args = HashMap::new();
        args.insert("message".to_string(), json!("hi"));

        let prov = SseProvider {
            base: BaseProvider {
                name: "sse".to_string(),
                provider_type: ProviderType::Sse,
                auth: None,
            },
            url: "http://example.com".to_string(),
            headers: None,
            body_field: Some("data".to_string()),
            header_fields: None,
        };

        let payload = transport.build_payload(&prov, args.clone());
        assert_eq!(payload, json!({ "data": args }));

        let prov_no_field =
            SseProvider::new("sse".to_string(), "http://example.com".to_string(), None);
        let payload = transport.build_payload(&prov_no_field, args.clone());
        assert_eq!(payload, json!(args));
    }

    #[test]
    fn apply_headers_adds_accept_and_custom_headers() {
        let transport = SseTransport::new();
        let prov = SseProvider {
            base: BaseProvider {
                name: "sse".to_string(),
                provider_type: ProviderType::Sse,
                auth: None,
            },
            url: "http://example.com".to_string(),
            headers: Some(HashMap::from([("X-Test".to_string(), "123".to_string())])),
            body_field: None,
            header_fields: None,
        };

        let request = transport
            .apply_headers(
                reqwest::Client::new().get("http://example.com"),
                &prov,
                Some("text/event-stream"),
                &HashMap::new(),
            )
            .build()
            .unwrap();

        assert_eq!(
            request.headers().get("accept").unwrap(),
            "text/event-stream"
        );
        assert_eq!(request.headers().get("x-test").unwrap(), "123");
    }

    #[test]
    fn parse_tools_from_body_reads_manifest() {
        let transport = SseTransport::new();
        let body = json!({
            "tools": [{
                "name": "stream-tool",
                "description": "streams",
                "inputs": { "type": "object" },
                "outputs": { "type": "object" },
                "tags": []
            }]
        })
        .to_string();

        let tools = transport.parse_tools_from_body(&body);
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "stream-tool");
    }

    #[test]
    fn header_fields_move_args_into_headers() {
        let transport = SseTransport::new();
        let prov = SseProvider {
            base: BaseProvider {
                name: "sse".to_string(),
                provider_type: ProviderType::Sse,
                auth: None,
            },
            url: "http://example.com".to_string(),
            headers: None,
            body_field: None,
            header_fields: Some(vec!["X-Token".into(), "trace".into()]),
        };

        let mut args = HashMap::new();
        args.insert("X-Token".into(), json!("abc"));
        args.insert("trace".into(), json!(123));
        args.insert("message".into(), json!("hi"));

        let (headers, remaining) = transport.split_headers_from_args(&prov, args);
        assert_eq!(headers.get("X-Token").map(|s| s.as_str()), Some("abc"));
        assert_eq!(headers.get("trace").map(|s| s.as_str()), Some("123"));
        assert!(remaining.contains_key("message"));
        assert!(!remaining.contains_key("trace"));
    }

    #[tokio::test]
    async fn register_call_and_stream_sse_transport() {
        async fn manifest() -> Json<Value> {
            Json(json!({
                "tools": [{
                    "name": "tool1",
                    "description": "sse tool",
                    "inputs": { "type": "object" },
                    "outputs": { "type": "object" },
                    "tags": []
                }]
            }))
        }

        async fn sse_handler(
            headers: axum::http::HeaderMap,
            Json(payload): Json<Value>,
        ) -> Response<Body> {
            assert_eq!(payload.get("msg").and_then(|v| v.as_str()), Some("hello"));
            assert!(
                payload.get("X-Trace").is_none(),
                "header field should be stripped from payload"
            );
            assert!(
                headers.get("x-trace").is_some(),
                "trace header should be present"
            );
            let stream = tokio_stream::iter(vec![
                Ok::<Bytes, std::convert::Infallible>(Bytes::from_static(b"data: {\"idx\":1}\n\n")),
                Ok(Bytes::from_static(b"data: {\"idx\":2}\n\n")),
            ]);

            Response::builder()
                .header("content-type", "text/event-stream")
                .body(Body::wrap_stream(stream))
                .unwrap()
        }

        let app = Router::new()
            .route("/", get(manifest))
            .route("/tool1", post(sse_handler));
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::Server::from_tcp(listener)
                .unwrap()
                .serve(app.into_make_service())
                .await
                .unwrap();
        });

        let prov = SseProvider {
            base: BaseProvider {
                name: "sse".to_string(),
                provider_type: ProviderType::Sse,
                auth: None,
            },
            url: format!("http://{}", addr),
            headers: None,
            body_field: None,
            header_fields: Some(vec!["X-Trace".into()]),
        };

        let transport = SseTransport::new();
        let tools = transport
            .register_tool_provider(&prov)
            .await
            .expect("register");
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "tool1");

        let mut args = HashMap::new();
        args.insert("msg".into(), Value::String("hello".into()));
        args.insert("X-Trace".into(), Value::String("trace-1".into()));

        let value = transport
            .call_tool("tool1", args.clone(), &prov)
            .await
            .expect("call");
        assert_eq!(value, json!([json!({"idx":1}), json!({"idx":2})]));

        let mut stream = transport
            .call_tool_stream("tool1", args, &prov)
            .await
            .expect("stream");
        assert_eq!(stream.next().await.unwrap().unwrap(), json!({"idx":1}));
        assert_eq!(stream.next().await.unwrap().unwrap(), json!({"idx":2}));
        stream.close().await.unwrap();

        // Provider-prefixed names should still resolve to the same endpoint.
        let mut args = HashMap::new();
        args.insert("msg".into(), Value::String("hello".into()));
        args.insert("X-Trace".into(), Value::String("trace-2".into()));
        let mut prefixed_stream = transport
            .call_tool_stream("sse.tool1", args, &prov)
            .await
            .expect("prefixed stream");
        assert_eq!(
            prefixed_stream.next().await.unwrap().unwrap(),
            json!({"idx":1})
        );
        let _ = prefixed_stream.close().await;
    }
}
