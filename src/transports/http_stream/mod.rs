// Streamable HTTP Transport (for chunked/streaming HTTP responses)
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use futures::StreamExt;
use reqwest::{header, Client};
use serde_json::{de::Deserializer, Value};
use std::collections::HashMap;
use tokio::sync::mpsc;

use crate::auth::AuthConfig;
use crate::providers::base::Provider;
use crate::providers::http_stream::StreamableHttpProvider;
use crate::tools::Tool;
use crate::transports::{
    stream::{boxed_channel_stream, StreamResult},
    ClientTransport,
};

/// Transport for HTTP endpoints that stream newline-delimited JSON or chunked bodies.
pub struct StreamableHttpTransport {
    client: Client,
}

impl StreamableHttpTransport {
    /// Create a streaming HTTP transport with a default client.
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }

    /// Attach authentication headers or query params to the request builder.
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
                "OAuth2 auth is not yet supported by the HTTP stream transport"
            )),
        }
    }
}

#[async_trait]
impl ClientTransport for StreamableHttpTransport {
    async fn register_tool_provider(&self, _prov: &dyn Provider) -> Result<Vec<Tool>> {
        // Streamable HTTP often shares the same discovery endpoint as HTTP providers.
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
        // Fallback: perform a standard request and aggregate the full response.
        let http_prov = prov
            .as_any()
            .downcast_ref::<StreamableHttpProvider>()
            .ok_or_else(|| anyhow!("Provider is not a StreamableHttpProvider"))?;

        let call_name = tool_name
            .strip_prefix(&format!("{}.", http_prov.base.name))
            .unwrap_or(tool_name);
        let url = format!("{}/{}", http_prov.url.trim_end_matches('/'), call_name);
        let method_upper = http_prov.http_method.to_uppercase();
        let mut request_builder = match method_upper.as_str() {
            "GET" => self.client.get(&url).query(&args),
            "POST" => self.client.post(&url).json(&args),
            "PUT" => self.client.put(&url).json(&args),
            "DELETE" => self.client.delete(&url).json(&args),
            "PATCH" => self.client.patch(&url).json(&args),
            other => return Err(anyhow!("Unsupported HTTP method: {}", other)),
        };

        if let Some(headers) = &http_prov.headers {
            for (k, v) in headers {
                request_builder = request_builder.header(k, v);
            }
        }

        if let Some(auth) = &http_prov.base.auth {
            request_builder = self.apply_auth(request_builder, auth)?;
        }

        let response = request_builder.send().await?;

        if !response.status().is_success() {
            return Err(anyhow!(
                "HTTP request failed with status: {}",
                response.status()
            ));
        }

        let value: Value = response.json().await?;
        Ok(value)
    }

    async fn call_tool_stream(
        &self,
        tool_name: &str,
        args: HashMap<String, Value>,
        prov: &dyn Provider,
    ) -> Result<Box<dyn StreamResult>> {
        let http_prov = prov
            .as_any()
            .downcast_ref::<StreamableHttpProvider>()
            .ok_or_else(|| anyhow!("Provider is not a StreamableHttpProvider"))?;

        let call_name = tool_name
            .strip_prefix(&format!("{}.", http_prov.base.name))
            .unwrap_or(tool_name);
        let url = format!("{}/{}", http_prov.url.trim_end_matches('/'), call_name);
        let method_upper = http_prov.http_method.to_uppercase();
        let mut req = match method_upper.as_str() {
            "GET" => self.client.get(url).query(&args),
            "POST" => self.client.post(url).json(&args),
            "PUT" => self.client.put(url).json(&args),
            "DELETE" => self.client.delete(url).json(&args),
            "PATCH" => self.client.patch(url).json(&args),
            other => return Err(anyhow!("Unsupported HTTP method: {}", other)),
        };

        if let Some(headers) = &http_prov.headers {
            for (k, v) in headers {
                req = req.header(k, v);
            }
        }

        if let Some(auth) = &http_prov.base.auth {
            req = self.apply_auth(req, auth)?;
        }

        let response = req.send().await?;

        if !response.status().is_success() {
            return Err(anyhow!(
                "HTTP request failed with status: {}",
                response.status()
            ));
        }

        // Stream response chunks and parse them as JSON values.
        let mut byte_stream = response.bytes_stream();
        let (tx, rx) = mpsc::channel(16);

        tokio::spawn(async move {
            let mut buffer: Vec<u8> = Vec::new();
            while let Some(chunk_result) = byte_stream.next().await {
                match chunk_result {
                    Ok(bytes) => {
                        buffer.extend_from_slice(&bytes);
                        let deserializer = Deserializer::from_slice(&buffer);
                        let mut stream = deserializer.into_iter::<Value>();
                        let mut offset = 0usize;

                        loop {
                            match stream.next() {
                                Some(Ok(value)) => {
                                    offset = stream.byte_offset();
                                    if tx.send(Ok(value)).await.is_err() {
                                        return;
                                    }
                                }
                                Some(Err(e)) => {
                                    if e.is_eof() {
                                        break;
                                    }
                                    let _ = tx
                                        .send(Err(anyhow!(
                                            "Failed to parse JSON from stream: {}",
                                            e
                                        )))
                                        .await;
                                    return;
                                }
                                None => break,
                            }
                        }

                        if offset > 0 && offset <= buffer.len() {
                            buffer.drain(0..offset);
                        }
                    }
                    Err(err) => {
                        let _ = tx
                            .send(Err(anyhow!("Error reading bytes from stream: {}", err)))
                            .await;
                        return;
                    }
                }
            }

            if !buffer.is_empty() {
                let _ = tx
                    .send(Err(anyhow!("Stream ended with incomplete JSON frame")))
                    .await;
            }
        });

        Ok(boxed_channel_stream(rx, None))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{ApiKeyAuth, AuthType, BasicAuth, OAuth2Auth};
    use crate::providers::base::{BaseProvider, ProviderType};
    use crate::providers::http_stream::StreamableHttpProvider;
    use axum::{body::Body, extract::Json, http::Response, routing::post, Router};
    use bytes::Bytes;
    use serde_json::json;
    use std::net::TcpListener;

    #[test]
    fn apply_auth_sets_expected_headers_and_query() {
        let transport = StreamableHttpTransport::new();

        let header_auth = AuthConfig::ApiKey(ApiKeyAuth {
            auth_type: AuthType::ApiKey,
            api_key: "secret".to_string(),
            var_name: "X-Stream-Key".to_string(),
            location: "header".to_string(),
        });
        let request = transport
            .apply_auth(
                reqwest::Client::new().get("http://example.com"),
                &header_auth,
            )
            .unwrap()
            .build()
            .unwrap();
        assert_eq!(request.headers().get("X-Stream-Key").unwrap(), "secret");

        let query_auth = AuthConfig::ApiKey(ApiKeyAuth {
            auth_type: AuthType::ApiKey,
            api_key: "abc".to_string(),
            var_name: "token".to_string(),
            location: "query".to_string(),
        });
        let request = transport
            .apply_auth(
                reqwest::Client::new().get("http://example.com"),
                &query_auth,
            )
            .unwrap()
            .build()
            .unwrap();
        assert_eq!(request.url().query(), Some("token=abc"));

        let basic_auth = AuthConfig::Basic(BasicAuth {
            auth_type: AuthType::Basic,
            username: "user".to_string(),
            password: "pass".to_string(),
        });
        let request = transport
            .apply_auth(
                reqwest::Client::new().get("http://example.com"),
                &basic_auth,
            )
            .unwrap()
            .build()
            .unwrap();
        assert_eq!(
            request.headers().get(header::AUTHORIZATION).unwrap(),
            "Basic dXNlcjpwYXNz"
        );
    }

    #[test]
    fn apply_auth_rejects_oauth2() {
        let transport = StreamableHttpTransport::new();
        let auth = AuthConfig::OAuth2(OAuth2Auth {
            auth_type: AuthType::OAuth2,
            token_url: "https://auth.example.com/token".to_string(),
            client_id: "client".to_string(),
            client_secret: "secret".to_string(),
            scope: None,
        });

        let err = transport
            .apply_auth(reqwest::Client::new().get("http://example.com"), &auth)
            .unwrap_err();
        assert!(err.to_string().contains("OAuth2 auth is not yet supported"));
    }

    #[tokio::test]
    async fn register_call_and_stream_http_stream_transport() {
        async fn aggregate(Json(payload): Json<Value>) -> Json<Value> {
            Json(json!({ "received": payload }))
        }

        async fn stream(Json(_payload): Json<Value>) -> Response<Body> {
            let chunks: Vec<Result<Bytes, std::convert::Infallible>> = vec![
                Ok(Bytes::from_static(br#"{"chunk":"#)),
                Ok(Bytes::from_static(br#"1}"#)),
                Ok(Bytes::from_static(b"\n{\"chunk\":2}")),
            ];
            Response::builder()
                .header("content-type", "application/json")
                .body(Body::wrap_stream(tokio_stream::iter(chunks)))
                .unwrap()
        }

        let app = Router::new()
            .route("/aggregate", post(aggregate))
            .route("/stream", post(stream));
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::Server::from_tcp(listener)
                .unwrap()
                .serve(app.into_make_service())
                .await
                .unwrap();
        });

        let base_url = format!("http://{}", addr);
        let provider = StreamableHttpProvider {
            base: BaseProvider {
                name: "http-stream".to_string(),
                provider_type: ProviderType::HttpStream,
                auth: None,
            },
            url: base_url.clone(),
            http_method: "POST".to_string(),
            headers: None,
        };

        let transport = StreamableHttpTransport::new();
        let tools = transport
            .register_tool_provider(&provider)
            .await
            .expect("register");
        assert!(tools.is_empty());

        let mut args = HashMap::new();
        args.insert("payload".into(), Value::String("data".into()));

        let aggregate_value = transport
            .call_tool("aggregate", args.clone(), &provider)
            .await
            .expect("call tool");
        assert_eq!(aggregate_value, json!({ "received": json!(args) }));

        let mut stream = transport
            .call_tool_stream("stream", args, &provider)
            .await
            .expect("call tool stream");
        let mut items = Vec::new();
        while let Some(item) = stream.next().await.unwrap() {
            items.push(item);
            if items.len() == 2 {
                break;
            }
        }
        stream.close().await.unwrap();

        assert_eq!(items, vec![json!({"chunk": 1}), json!({"chunk": 2})]);
    }

    #[tokio::test]
    async fn http_stream_strips_provider_prefix() {
        async fn echo(Json(_payload): Json<Value>) -> Json<Value> {
            Json(json!({"ok": true}))
        }

        let app = Router::new().route("/echo", post(echo));
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::Server::from_tcp(listener)
                .unwrap()
                .serve(app.into_make_service())
                .await
                .unwrap();
        });

        let base_url = format!("http://{}", addr);
        let provider = StreamableHttpProvider {
            base: BaseProvider {
                name: "http-stream".to_string(),
                provider_type: ProviderType::HttpStream,
                auth: None,
            },
            url: base_url.clone(),
            http_method: "POST".to_string(),
            headers: None,
        };

        let transport = StreamableHttpTransport::new();
        let value = transport
            .call_tool("http-stream.echo", HashMap::new(), &provider)
            .await
            .expect("call tool");
        assert_eq!(value, json!({"ok": true}));
    }
}
