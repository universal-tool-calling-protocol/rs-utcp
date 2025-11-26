// Streamable HTTP Transport (for chunked/streaming HTTP responses)
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use futures::StreamExt;
use reqwest::{header, Client};
use serde_json::Value;
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

pub struct StreamableHttpTransport {
    client: Client,
}

impl StreamableHttpTransport {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
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

        let url = format!("{}/{}", http_prov.url.trim_end_matches('/'), tool_name);
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

        let url = format!("{}/{}", http_prov.url.trim_end_matches('/'), tool_name);
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
            while let Some(chunk_result) = byte_stream.next().await {
                match chunk_result {
                    Ok(bytes) => {
                        let parsed = serde_json::from_slice::<Value>(&bytes)
                            .map_err(|e| anyhow!("Failed to parse JSON from stream: {}", e));
                        if tx.send(parsed).await.is_err() {
                            break;
                        }
                    }
                    Err(err) => {
                        let _ = tx
                            .send(Err(anyhow!("Error reading bytes from stream: {}", err)))
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
    use crate::auth::{ApiKeyAuth, AuthType, BasicAuth, OAuth2Auth};

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
            .apply_auth(reqwest::Client::new().get("http://example.com"), &header_auth)
            .unwrap()
            .build()
            .unwrap();
        assert_eq!(
            request.headers().get("X-Stream-Key").unwrap(),
            "secret"
        );

        let query_auth = AuthConfig::ApiKey(ApiKeyAuth {
            auth_type: AuthType::ApiKey,
            api_key: "abc".to_string(),
            var_name: "token".to_string(),
            location: "query".to_string(),
        });
        let request = transport
            .apply_auth(reqwest::Client::new().get("http://example.com"), &query_auth)
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
            .apply_auth(reqwest::Client::new().get("http://example.com"), &basic_auth)
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
}
