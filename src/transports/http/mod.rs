use anyhow::{anyhow, Result};
use async_trait::async_trait;
use reqwest::{header, Client};
use serde_json::Value;
use std::collections::HashMap;
use std::time::Duration;

use crate::auth::AuthConfig;
use crate::providers::base::Provider;
use crate::providers::http::HttpProvider;
use crate::tools::Tool;
use crate::transports::{stream::StreamResult, ClientTransport};

pub struct HttpClientTransport {
    pub client: Client,
}

impl HttpClientTransport {
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to build HTTP client");

        Self { client }
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
                "OAuth2 auth is not yet supported by the HTTP transport"
            )),
        }
    }
}

#[async_trait]
impl ClientTransport for HttpClientTransport {
    async fn register_tool_provider(&self, prov: &dyn Provider) -> Result<Vec<Tool>> {
        // Downcast to HttpProvider using as_any
        let http_prov = prov
            .as_any()
            .downcast_ref::<HttpProvider>()
            .ok_or_else(|| anyhow!("Provider is not an HttpProvider"))?;

        // Fetch tool definitions from the HTTP endpoint
        // The endpoint should return a UTCP manifest or OpenAPI spec
        let mut request_builder = self.client.get(&http_prov.url);

        if let Some(headers) = &http_prov.headers {
            for (key, value) in headers {
                request_builder = request_builder.header(key, value);
            }
        }

        if let Some(auth) = &http_prov.base.auth {
            request_builder = self.apply_auth(request_builder, auth)?;
        }

        let response = request_builder.send().await?;

        if !response.status().is_success() {
            return Err(anyhow!(
                "Failed to fetch tools from {}: {}",
                http_prov.url,
                response.status()
            ));
        }

        // Try to parse as UTCP manifest first
        let body_text = response.text().await?;

        // Try parsing as JSON
        if let Ok(manifest) = serde_json::from_str::<Value>(&body_text) {
            // Check if it's a UTCP manifest (has "tools" array)
            if let Some(tools_array) = manifest.get("tools").and_then(|v| v.as_array()) {
                let mut tools = Vec::new();
                for tool_value in tools_array {
                    if let Ok(tool) = serde_json::from_value::<Tool>(tool_value.clone()) {
                        tools.push(tool);
                    }
                }
                return Ok(tools);
            }
        }

        // If no tools found, return empty vec
        // In a full implementation, we would also parse OpenAPI specs here
        Ok(vec![])
    }

    async fn deregister_tool_provider(&self, _prov: &dyn Provider) -> Result<()> {
        // HTTP transport is stateless, so nothing to do
        Ok(())
    }

    async fn call_tool(
        &self,
        _tool_name: &str,
        args: HashMap<String, Value>,
        prov: &dyn Provider,
    ) -> Result<Value> {
        // Downcast to HttpProvider using as_any
        let http_prov = prov
            .as_any()
            .downcast_ref::<HttpProvider>()
            .ok_or_else(|| anyhow!("Provider is not an HttpProvider"))?;

        // Handle URL path parameters (e.g., {id} in URL)
        let mut url = http_prov.url.clone();
        for (key, value) in &args {
            let placeholder = format!("{{{}}}", key);
            if url.contains(&placeholder) {
                url = url.replace(&placeholder, &value.to_string());
            }
        }

        let method_upper = http_prov.http_method.to_uppercase();
        let mut request_builder = match method_upper.as_str() {
            "GET" => self.client.get(&url),
            "POST" => self.client.post(&url),
            "PUT" => self.client.put(&url),
            "DELETE" => self.client.delete(&url),
            "PATCH" => self.client.patch(&url),
            method => return Err(anyhow!("Unsupported HTTP method: {}", method)),
        };

        // Add headers
        if let Some(headers) = &http_prov.headers {
            for (key, value) in headers {
                request_builder = request_builder.header(key, value);
            }
        }

        if let Some(auth) = &http_prov.base.auth {
            request_builder = self.apply_auth(request_builder, auth)?;
        }

        // Determine how to send remaining args
        if method_upper == "POST" || method_upper == "PUT" || method_upper == "PATCH" {
            // Send as JSON body
            request_builder = request_builder.json(&args);
        } else {
            // Send as query parameters
            for (key, value) in &args {
                request_builder = request_builder.query(&[(key, value.to_string())]);
            }
        }

        // Send request
        let response = request_builder.send().await?;

        if !response.status().is_success() {
            return Err(anyhow!(
                "HTTP request failed with status: {}",
                response.status()
            ));
        }

        let result: Value = response.json().await?;
        Ok(result)
    }

    async fn call_tool_stream(
        &self,
        _tool_name: &str,
        _args: HashMap<String, Value>,
        _prov: &dyn Provider,
    ) -> Result<Box<dyn StreamResult>> {
        Err(anyhow!("Streaming not supported by HttpClientTransport"))
    }
}
