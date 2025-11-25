// MCP (Model Context Protocol) Transport
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use reqwest::{header, Client};
use serde_json::Value;
use std::collections::HashMap;

use crate::auth::AuthConfig;
use crate::providers::base::Provider;
use crate::providers::mcp::McpProvider;
use crate::tools::Tool;
use crate::transports::{stream::StreamResult, ClientTransport};

pub struct McpTransport {
    client: Client,
}

impl McpTransport {
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
                "OAuth2 auth is not yet supported by the MCP transport"
            )),
        }
    }

    async fn mcp_request(&self, prov: &McpProvider, method: &str, params: Value) -> Result<Value> {
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
            "id": 1,
        });

        let mut req = self.client.post(&prov.url).json(&request);
        if let Some(headers) = &prov.headers {
            for (k, v) in headers {
                req = req.header(k, v);
            }
        }
        if let Some(auth) = &prov.base.auth {
            req = self.apply_auth(req, auth)?;
        }

        let response = req.send().await?;

        if !response.status().is_success() {
            return Err(anyhow!("MCP request failed: {}", response.status()));
        }

        let result: Value = response.json().await?;

        // Check for JSON-RPC error
        if let Some(error) = result.get("error") {
            return Err(anyhow!("MCP error: {}", error));
        }

        result
            .get("result")
            .cloned()
            .ok_or_else(|| anyhow!("No result in MCP response"))
    }
}

#[async_trait]
impl ClientTransport for McpTransport {
    async fn register_tool_provider(&self, _prov: &dyn Provider) -> Result<Vec<Tool>> {
        let mcp_prov = _prov
            .as_any()
            .downcast_ref::<McpProvider>()
            .ok_or_else(|| anyhow!("Provider is not an McpProvider"))?;

        let params = serde_json::json!({ "cursor": null });
        let result = self.mcp_request(mcp_prov, "tools/list", params).await?;

        if let Some(tools) = result.get("tools").and_then(|v| v.as_array()) {
            let mut parsed = Vec::new();
            for tool in tools {
                if let Ok(t) = serde_json::from_value::<Tool>(tool.clone()) {
                    parsed.push(t);
                }
            }
            return Ok(parsed);
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
        let mcp_prov = prov
            .as_any()
            .downcast_ref::<McpProvider>()
            .ok_or_else(|| anyhow!("Provider is not an McpProvider"))?;

        // MCP tool call format
        let params = serde_json::json!({
            "name": tool_name,
            "arguments": args,
        });

        // Call the tool via MCP request
        self.mcp_request(mcp_prov, "tools/call", params).await
    }

    async fn call_tool_stream(
        &self,
        _tool_name: &str,
        _args: HashMap<String, Value>,
        _prov: &dyn Provider,
    ) -> Result<Box<dyn StreamResult>> {
        // MCP can support streaming via Server-Sent Events or WebSocket
        Err(anyhow!("MCP streaming requires SSE or WebSocket transport"))
    }
}
