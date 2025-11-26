// MCP (Model Context Protocol) Transport
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use reqwest::{header, Client};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;

use crate::auth::AuthConfig;
use crate::providers::base::Provider;
use crate::providers::mcp::McpProvider;
use crate::tools::Tool;
use crate::transports::{stream::StreamResult, ClientTransport};

// Stdio process wrapper for MCP transport
struct McpStdioProcess {
    #[allow(dead_code)] // Needed to keep the process alive
    child: Child,
    stdin: Arc<Mutex<ChildStdin>>,
    stdout: Arc<Mutex<BufReader<ChildStdout>>>,
    request_id: Arc<Mutex<u64>>,
}

impl McpStdioProcess {
    async fn new(
        command: &str,
        args: &Option<Vec<String>>,
        env_vars: &Option<HashMap<String, String>>,
    ) -> Result<Self> {
        let mut cmd = Command::new(command);

        if let Some(args_vec) = args {
            cmd.args(args_vec);
        }

        if let Some(env) = env_vars {
            for (k, v) in env {
                cmd.env(k, v);
            }
        }

        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn()?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("Failed to get stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("Failed to get stdout"))?;

        Ok(Self {
            child,
            stdin: Arc::new(Mutex::new(stdin)),
            stdout: Arc::new(Mutex::new(BufReader::new(stdout))),
            request_id: Arc::new(Mutex::new(1)),
        })
    }

    async fn send_request(&self, method: &str, params: Value) -> Result<Value> {
        let mut id_guard = self.request_id.lock().await;
        let id = *id_guard;
        *id_guard += 1;
        drop(id_guard);

        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
            "id": id,
        });

        let request_str = serde_json::to_string(&request)?;

        // Write request to stdin
        let mut stdin = self.stdin.lock().await;
        stdin.write_all(request_str.as_bytes()).await?;
        stdin.write_all(b"\n").await?;
        stdin.flush().await?;
        drop(stdin);

        // Read response from stdout
        let mut stdout = self.stdout.lock().await;
        let mut line = String::new();
        stdout.read_line(&mut line).await?;
        drop(stdout);

        if line.is_empty() {
            return Err(anyhow!("MCP process closed connection"));
        }

        let response: Value = serde_json::from_str(&line)?;

        // Check for JSON-RPC error
        if let Some(error) = response.get("error") {
            return Err(anyhow!("MCP error: {}", error));
        }

        response
            .get("result")
            .cloned()
            .ok_or_else(|| anyhow!("No result in MCP response"))
    }
}

pub struct McpTransport {
    client: Client,
    // Map of provider name to stdio process
    stdio_processes: Arc<Mutex<HashMap<String, Arc<McpStdioProcess>>>>,
}

impl McpTransport {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            stdio_processes: Arc::new(Mutex::new(HashMap::new())),
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

    async fn mcp_http_request(
        &self,
        prov: &McpProvider,
        method: &str,
        params: Value,
    ) -> Result<Value> {
        let url = prov
            .url
            .as_ref()
            .ok_or_else(|| anyhow!("No URL provided for HTTP MCP provider"))?;

        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
            "id": 1,
        });

        let mut req = self.client.post(url).json(&request);
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

    async fn get_or_create_stdio_process(
        &self,
        prov: &McpProvider,
    ) -> Result<Arc<McpStdioProcess>> {
        let mut processes = self.stdio_processes.lock().await;

        if let Some(process) = processes.get(&prov.base.name) {
            return Ok(Arc::clone(process));
        }

        let command = prov
            .command
            .as_ref()
            .ok_or_else(|| anyhow!("No command provided for stdio MCP provider"))?;

        let process = Arc::new(McpStdioProcess::new(command, &prov.args, &prov.env_vars).await?);
        processes.insert(prov.base.name.clone(), Arc::clone(&process));

        Ok(process)
    }

    async fn mcp_request(&self, prov: &McpProvider, method: &str, params: Value) -> Result<Value> {
        if prov.is_http() {
            self.mcp_http_request(prov, method, params).await
        } else if prov.is_stdio() {
            let process = self.get_or_create_stdio_process(prov).await?;
            process.send_request(method, params).await
        } else {
            Err(anyhow!(
                "MCP provider must have either 'url' (HTTP) or 'command' (stdio)"
            ))
        }
    }

    async fn mcp_http_stream(
        &self,
        prov: &McpProvider,
        params: Value,
    ) -> Result<Box<dyn StreamResult>> {
        use eventsource_stream::Eventsource;
        use futures::StreamExt;

        let url = prov
            .url
            .as_ref()
            .ok_or_else(|| anyhow!("No URL provided for HTTP MCP provider"))?;

        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "tools/call",
            "params": params,
            "id": 1,
        });

        let mut req = self.client.post(url).json(&request);

        // Add headers
        if let Some(headers) = &prov.headers {
            for (k, v) in headers {
                req = req.header(k, v);
            }
        }

        // Add authentication
        if let Some(auth) = &prov.base.auth {
            req = self.apply_auth(req, auth)?;
        }

        // Set Accept header for SSE
        req = req.header("Accept", "text/event-stream");

        let response = req.send().await?;

        if !response.status().is_success() {
            return Err(anyhow!("MCP stream request failed: {}", response.status()));
        }

        // Create a channel to stream results
        let (tx, rx) = tokio::sync::mpsc::channel(100);

        // Spawn a task to read SSE events
        tokio::spawn(async move {
            let byte_stream = response.bytes_stream();
            let mut event_stream = byte_stream.eventsource();

            while let Some(event_result) = event_stream.next().await {
                match event_result {
                    Ok(event) => {
                        // Parse the event data as JSON
                        match serde_json::from_str::<Value>(&event.data) {
                            Ok(value) => {
                                if tx.send(Ok(value)).await.is_err() {
                                    break; // Receiver dropped
                                }
                            }
                            Err(e) => {
                                let _ = tx
                                    .send(Err(anyhow!("Failed to parse SSE event: {}", e)))
                                    .await;
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(Err(anyhow!("SSE stream error: {}", e))).await;
                        break;
                    }
                }
            }
        });

        Ok(crate::transports::stream::boxed_channel_stream(rx, None))
    }

    async fn mcp_stdio_stream(
        &self,
        prov: &McpProvider,
        params: Value,
    ) -> Result<Box<dyn StreamResult>> {
        let process = self.get_or_create_stdio_process(prov).await?;

        // Generate request ID
        let mut id_guard = process.request_id.lock().await;
        let id = *id_guard;
        *id_guard += 1;
        drop(id_guard);

        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "tools/call",
            "params": params,
            "id": id,
        });

        let request_str = serde_json::to_string(&request)?;

        // Write request to stdin
        let mut stdin = process.stdin.lock().await;
        stdin.write_all(request_str.as_bytes()).await?;
        stdin.write_all(b"\n").await?;
        stdin.flush().await?;
        drop(stdin);

        // Create a channel to stream results
        let (tx, rx) = tokio::sync::mpsc::channel(100);

        // Clone Arc for the task
        let stdout = Arc::clone(&process.stdout);

        // Spawn a task to read streaming responses
        tokio::spawn(async move {
            let mut stdout_guard = stdout.lock().await;

            loop {
                let mut line = String::new();
                match stdout_guard.read_line(&mut line).await {
                    Ok(0) => break, // EOF
                    Ok(_) => {
                        if line.trim().is_empty() {
                            continue;
                        }

                        // Parse the JSON-RPC response
                        match serde_json::from_str::<Value>(&line) {
                            Ok(response) => {
                                // Check if this is an error response
                                if let Some(error) = response.get("error") {
                                    let _ = tx.send(Err(anyhow!("MCP error: {}", error))).await;
                                    break;
                                }

                                // Check if this is the final result or a stream chunk
                                if let Some(result) = response.get("result") {
                                    // Send the result
                                    if tx.send(Ok(result.clone())).await.is_err() {
                                        break; // Receiver dropped
                                    }

                                    // Check if this is marked as the final response
                                    if response
                                        .get("final")
                                        .and_then(|v| v.as_bool())
                                        .unwrap_or(false)
                                    {
                                        break;
                                    }
                                }
                            }
                            Err(e) => {
                                let _ = tx
                                    .send(Err(anyhow!("Failed to parse response: {}", e)))
                                    .await;
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        let _ = tx
                            .send(Err(anyhow!("Failed to read from stdout: {}", e)))
                            .await;
                        break;
                    }
                }
            }
        });

        Ok(crate::transports::stream::boxed_channel_stream(rx, None))
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
        let mcp_prov = _prov
            .as_any()
            .downcast_ref::<McpProvider>()
            .ok_or_else(|| anyhow!("Provider is not an McpProvider"))?;

        // For stdio processes, terminate the process
        if mcp_prov.is_stdio() {
            let mut processes = self.stdio_processes.lock().await;
            if let Some(process) = processes.remove(&mcp_prov.base.name) {
                // Process will be dropped and killed when Arc count reaches 0
                drop(process);
            }
        }

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
        tool_name: &str,
        args: HashMap<String, Value>,
        prov: &dyn Provider,
    ) -> Result<Box<dyn StreamResult>> {
        let mcp_prov = prov
            .as_any()
            .downcast_ref::<McpProvider>()
            .ok_or_else(|| anyhow!("Provider is not an McpProvider"))?;

        // MCP tool call format
        let params = serde_json::json!({
            "name": tool_name,
            "arguments": args,
        });

        if mcp_prov.is_http() {
            self.mcp_http_stream(mcp_prov, params).await
        } else if mcp_prov.is_stdio() {
            self.mcp_stdio_stream(mcp_prov, params).await
        } else {
            Err(anyhow!(
                "MCP provider must have either 'url' (HTTP) or 'command' (stdio)"
            ))
        }
    }
}
