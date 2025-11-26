// Example: MCP Streaming with SSE
//
// This example demonstrates how to use MCP streaming capabilities
// for both HTTP and stdio transports.

use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Method, Request, Response, Server, StatusCode};
use rs_utcp::config::UtcpClientConfig;
use rs_utcp::providers::mcp::McpProvider;
use rs_utcp::repository::in_memory::InMemoryToolRepository;
use rs_utcp::tag::tag_search::TagSearchStrategy;
use rs_utcp::{UtcpClient, UtcpClientInterface};
use serde_json::json;
use std::collections::HashMap;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("🔄 MCP Streaming Example\n");

    // Start a local MCP server that supports SSE
    let addr = spawn_mcp_server().await?;
    println!("  ✓ Started local MCP server at http://{}", addr);

    // Create repository and search strategy
    let repo = Arc::new(InMemoryToolRepository::new());
    let search = Arc::new(TagSearchStrategy::new(repo.clone(), 1.0));
    let config = UtcpClientConfig::default();

    // Create client
    let client = UtcpClient::new(config, repo.clone(), search).await?;

    // Example 1: HTTP-based MCP provider with SSE streaming
    println!("\n📡 Example 1: HTTP MCP Provider with SSE");
    let http_provider = Arc::new(McpProvider::new(
        "http_mcp".to_string(),
        format!("http://{}/mcp", addr),
        None,
    ));

    match client.register_tool_provider(http_provider.clone()).await {
        Ok(tools) => {
            println!("  ✓ Registered HTTP MCP provider with {} tools", tools.len());
            
            // Try to stream results from a tool
            if let Some(tool) = tools.first() {
                let mut args = HashMap::new();
                args.insert("query".to_string(), serde_json::json!("test"));
                
                println!("  → Streaming results from tool: {}", tool.name);
                match client.call_tool_stream(&tool.name, args).await {
                    Ok(mut stream) => {
                        while let Ok(Some(value)) = stream.next().await {
                            println!("  📦 Received: {}", serde_json::to_string_pretty(&value)?);
                        }
                        stream.close().await?;
                    }
                    Err(e) => println!("  ⚠ Stream error: {}", e),
                }
            }
        }
        Err(e) => println!("  ⚠ Failed to register HTTP provider: {}", e),
    }

    // Example 2: Stdio-based MCP provider with streaming
    println!("\n📝 Example 2: Stdio MCP Provider with Streaming");
    let stdio_provider = Arc::new(McpProvider::new_stdio(
        "stdio_mcp".to_string(),
        "python3".to_string(),
        Some(vec!["examples/mcp_stdio_server.py".to_string()]),
        None,
    ));

    match client.register_tool_provider(stdio_provider.clone()).await {
        Ok(tools) => {
            println!("  ✓ Registered stdio MCP provider with {} tools", tools.len());
            
            // Try to stream results from the 'add' tool
            let mut args = HashMap::new();
            args.insert("a".to_string(), serde_json::json!(10));
            args.insert("b".to_string(), serde_json::json!(20));
            
            println!("  → Streaming results from tool: stdio_mcp.add");
            match client.call_tool_stream("stdio_mcp.add", args).await {
                Ok(mut stream) => {
                    while let Ok(Some(value)) = stream.next().await {
                        println!("  📦 Received: {}", serde_json::to_string_pretty(&value)?);
                    }
                    stream.close().await?;
                }
                Err(e) => println!("  ⚠ Stream error: {}", e),
            }
        }
        Err(e) => println!("  ⚠ Failed to register stdio provider: {}", e),
    }

    println!("\n✨ Demo complete!");
    Ok(())
}

async fn spawn_mcp_server() -> anyhow::Result<SocketAddr> {
    let make_svc = make_service_fn(|_conn| async { Ok::<_, Infallible>(service_fn(handle)) });
    let server = Server::try_bind(&([127, 0, 0, 1], 0).into())?;
    let addr = server.local_addr();
    tokio::spawn(server.serve(make_svc));
    Ok(addr)
}

async fn handle(req: Request<Body>) -> Result<Response<Body>, Infallible> {
    if req.method() != Method::POST {
        return Ok(Response::builder()
            .status(StatusCode::METHOD_NOT_ALLOWED)
            .body(Body::empty())
            .unwrap());
    }

    let accept_header = req.headers().get("Accept").and_then(|h| h.to_str().ok()).unwrap_or("");
    let is_sse = accept_header.contains("text/event-stream");

    let body = hyper::body::to_bytes(req.into_body())
        .await
        .unwrap_or_default();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap_or(json!({}));
    let method = payload.get("method").and_then(|m| m.as_str()).unwrap_or("");

    match method {
        "tools/list" => {
            let resp = json!({
                "jsonrpc": "2.0",
                "result": { "tools": [{
                    "name": "stream_echo",
                    "description": "Echo args with streaming",
                    "inputs": { "type": "object" },
                    "outputs": { "type": "object" },
                    "tags": ["mcp", "stream"]
                }]},
                "id": payload.get("id").cloned().unwrap_or(json!(1))
            });
            Ok(json_response(StatusCode::OK, resp))
        }
        "tools/call" => {
            if is_sse {
                // Return SSE stream
                let (mut tx, body) = Body::channel();
                
                tokio::spawn(async move {
                    let messages = vec!["Hello", "from", "SSE", "stream!"];
                    
                    for msg in messages {
                        let data = json!({
                            "type": "chunk",
                            "content": msg
                        });
                        let event = format!("data: {}\n\n", data.to_string());
                        if tx.send_data(event.into()).await.is_err() {
                            break;
                        }
                        sleep(Duration::from_millis(200)).await;
                    }
                    
                    // Send final event if needed, or just close
                    // For this demo, we'll just close the stream
                });

                Ok(Response::builder()
                    .status(StatusCode::OK)
                    .header("Content-Type", "text/event-stream")
                    .header("Cache-Control", "no-cache")
                    .header("Connection", "keep-alive")
                    .body(body)
                    .unwrap())
            } else {
                let resp = json!({
                    "jsonrpc": "2.0",
                    "result": payload.get("params").cloned().unwrap_or(json!({})),
                    "id": payload.get("id").cloned().unwrap_or(json!(1))
                });
                Ok(json_response(StatusCode::OK, resp))
            }
        }
        _ => Ok(Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::empty())
            .unwrap()),
    }
}

fn json_response(status: StatusCode, body: serde_json::Value) -> Response<Body> {
    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}
