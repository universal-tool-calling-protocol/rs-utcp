//! Showcase for every provider/transport. Mirrors go-utcp example set.
//! Each section is opt-in via environment variables so you can run what you have
//! endpoints for without the others failing.
//!
//! Example:
//!   DEMO_HTTP_URL=https://httpbin.org/post \
//!   DEMO_WS_URL=wss://echo.websocket.events \
//!   cargo run --example all_providers

use anyhow::Result;
use rs_utcp::UtcpClientInterface;
use serde_json::json;
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::time::{timeout, Duration};

#[path = "common/mod.rs"]
mod common;

fn env(key: &str) -> Option<String> {
    std::env::var(key).ok()
}

#[tokio::main]
async fn main() -> Result<()> {
    println!("Running provider demos (set DEMO_* env vars to enable a block):");

    if let Err(e) = demo_http().await {
        eprintln!("HTTP demo error: {e}");
    }
    if let Err(e) = demo_cli().await {
        eprintln!("CLI demo error: {e}");
    }
    if let Err(e) = demo_websocket().await {
        eprintln!("WebSocket demo error: {e}");
    }
    if let Err(e) = demo_graphql().await {
        eprintln!("GraphQL demo error: {e}");
    }
    if let Err(e) = demo_grpc().await {
        eprintln!("gRPC demo error: {e}");
    }
    if let Err(e) = demo_tcp().await {
        eprintln!("TCP demo error: {e}");
    }
    if let Err(e) = demo_udp().await {
        eprintln!("UDP demo error: {e}");
    }
    if let Err(e) = demo_sse().await {
        eprintln!("SSE demo error: {e}");
    }
    if let Err(e) = demo_http_stream().await {
        eprintln!("HTTP stream demo error: {e}");
    }
    if let Err(e) = demo_mcp().await {
        eprintln!("MCP demo error: {e}");
    }
    if let Err(e) = demo_text().await {
        eprintln!("Text demo error: {e}");
    }
    if let Err(e) = demo_webrtc().await {
        eprintln!("WebRTC demo error: {e}");
    }

    Ok(())
}

async fn demo_http() -> Result<()> {
    let Some(url) = env("DEMO_HTTP_URL") else {
        println!("  ▫️ HTTP: set DEMO_HTTP_URL to run");
        return Ok(());
    };

    println!("  ▶️ HTTP -> {url}");
    let client = common::client_from_providers(json!({
        "providers": [{
            "provider_type": "http",
            "name": "http_demo",
            "url": url,
            "http_method": "POST"
        }]
    }))
    .await?;

    let mut args = HashMap::new();
    args.insert("echo".into(), json!("hello from rust-utcp"));
    let value = client.call_tool("http_demo.echo", args).await?;
    println!("    response: {value}");
    Ok(())
}

async fn demo_cli() -> Result<()> {
    let Some(cmd) = env("DEMO_CLI_CMD") else {
        println!("  ▫️ CLI: set DEMO_CLI_CMD (e.g., echo '{{\"tools\":[]}}') to run");
        return Ok(());
    };
    println!("  ▶️ CLI -> {cmd}");
    let client = common::client_from_providers(json!({
        "providers": [{
            "provider_type": "cli",
            "name": "cli_demo",
            "command_name": cmd
        }]
    }))
    .await?;

    let mut args = HashMap::new();
    args.insert("msg".into(), json!("hello cli"));
    let value = client.call_tool("cli_demo.echo", args).await;
    println!("    call result: {:?}", value);
    Ok(())
}

async fn demo_websocket() -> Result<()> {
    let Some(url) = env("DEMO_WS_URL") else {
        println!("  ▫️ WebSocket: set DEMO_WS_URL to run (e.g., wss://echo.websocket.events)");
        return Ok(());
    };
    println!("  ▶️ WebSocket -> {url}");
    let client = common::client_from_providers(json!({
        "providers": [{
            "provider_type": "websocket",
            "name": "ws_demo",
            "url": url
        }]
    }))
    .await?;

    let mut args = HashMap::new();
    args.insert("text".into(), json!("hello websocket"));
    let value = client.call_tool("ws_demo.echo", args).await;
    println!("    call result: {:?}", value);
    Ok(())
}

async fn demo_graphql() -> Result<()> {
    let Some(url) = env("DEMO_GRAPHQL_URL") else {
        println!("  ▫️ GraphQL: set DEMO_GRAPHQL_URL to run");
        return Ok(());
    };
    println!("  ▶️ GraphQL -> {url}");
    let client = common::client_from_providers(json!({
        "providers": [{
            "provider_type": "graphql",
            "name": "graphql_demo",
            "url": url
        }]
    }))
    .await?;

    let mut args = HashMap::new();
    args.insert("name".into(), json!("rust-utcp"));
    let value = client.call_tool("graphql_demo.hello", args).await;
    println!("    call result: {:?}", value);
    Ok(())
}

async fn demo_grpc() -> Result<()> {
    let (Some(host), Some(port)) = (env("DEMO_GRPC_HOST"), env("DEMO_GRPC_PORT")) else {
        println!("  ▫️ gRPC: set DEMO_GRPC_HOST and DEMO_GRPC_PORT to run");
        return Ok(());
    };
    let port: u16 = port.parse().unwrap_or(50051);
    println!("  ▶️ gRPC -> {host}:{port}");
    let client = common::client_from_providers(json!({
        "providers": [{
            "provider_type": "grpc",
            "name": "grpc_demo",
            "host": host,
            "port": port
        }]
    }))
    .await?;

    let mut args = HashMap::new();
    args.insert("echo".into(), json!("hello grpc"));
    let value = client.call_tool("grpc_demo.echo", args).await;
    println!("    call result: {:?}", value);
    Ok(())
}

async fn demo_tcp() -> Result<()> {
    let Some(addr) = env("DEMO_TCP_ADDR") else {
        println!("  ▫️ TCP: set DEMO_TCP_ADDR (host:port) to run");
        return Ok(());
    };
    let (host, port) = split_host_port(&addr)?;
    println!("  ▶️ TCP -> {addr}");
    let client = common::client_from_providers(json!({
        "providers": [{
            "provider_type": "tcp",
            "name": "tcp_demo",
            "host": host,
            "port": port
        }]
    }))
    .await?;

    let mut args = HashMap::new();
    args.insert("ping".into(), json!("pong"));
    let value = client.call_tool("tcp_demo.echo", args).await;
    println!("    call result: {:?}", value);
    Ok(())
}

async fn demo_udp() -> Result<()> {
    let Some(addr) = env("DEMO_UDP_ADDR") else {
        println!("  ▫️ UDP: set DEMO_UDP_ADDR (host:port) to run");
        return Ok(());
    };
    let (host, port) = split_host_port(&addr)?;
    println!("  ▶️ UDP -> {addr}");
    let client = common::client_from_providers(json!({
        "providers": [{
            "provider_type": "udp",
            "name": "udp_demo",
            "host": host,
            "port": port
        }]
    }))
    .await?;

    let mut args = HashMap::new();
    args.insert("ping".into(), json!("pong"));
    let value = client.call_tool("udp_demo.echo", args).await;
    println!("    call result: {:?}", value);
    Ok(())
}

async fn demo_sse() -> Result<()> {
    let Some(url) = env("DEMO_SSE_URL") else {
        println!("  ▫️ SSE: set DEMO_SSE_URL to run");
        return Ok(());
    };
    println!("  ▶️ SSE -> {url}");
    let client = common::client_from_providers(json!({
        "providers": [{
            "provider_type": "sse",
            "name": "sse_demo",
            "url": url
        }]
    }))
    .await?;

    let mut args = HashMap::new();
    args.insert("topic".into(), json!("demo"));
    let mut stream = client.call_tool_stream("sse_demo.stream", args).await?;
    let item = timeout(Duration::from_secs(5), stream.next()).await;
    println!("    first event: {:?}", item);
    let _ = stream.close().await;
    Ok(())
}

async fn demo_http_stream() -> Result<()> {
    let Some(url) = env("DEMO_HTTP_STREAM_URL") else {
        println!("  ▫️ HTTP Stream: set DEMO_HTTP_STREAM_URL to run");
        return Ok(());
    };
    println!("  ▶️ HTTP Stream -> {url}");
    let client = common::client_from_providers(json!({
        "providers": [{
            "provider_type": "http_stream",
            "name": "http_stream_demo",
            "url": url
        }]
    }))
    .await?;

    let mut args = HashMap::new();
    args.insert("query".into(), json!("stream me"));
    let mut stream = client
        .call_tool_stream("http_stream_demo.stream", args)
        .await?;
    let item = timeout(Duration::from_secs(5), stream.next()).await;
    println!("    first chunk: {:?}", item);
    let _ = stream.close().await;
    Ok(())
}

async fn demo_mcp() -> Result<()> {
    let Some(url) = env("DEMO_MCP_URL") else {
        println!("  ▫️ MCP: set DEMO_MCP_URL to run");
        return Ok(());
    };
    println!("  ▶️ MCP -> {url}");
    let client = common::client_from_providers(json!({
        "providers": [{
            "provider_type": "mcp",
            "name": "mcp_demo",
            "url": url
        }]
    }))
    .await?;
    let tools = client.search_tools("", 10).await?;
    println!("    tools: {}", tools.len());

    let mut args = HashMap::new();
    args.insert("name".into(), json!("echo"));
    let value = client.call_tool("mcp_demo.echo", args).await;
    println!("    call result: {:?}", value);
    Ok(())
}

async fn demo_text() -> Result<()> {
    let Some(path) = env("DEMO_TEXT_PATH") else {
        println!(
            "  ▫️ Text: set DEMO_TEXT_PATH to a folder containing tools.json and scripts to run"
        );
        return Ok(());
    };
    let base_path = PathBuf::from(path);
    println!("  ▶️ Text -> {}", base_path.display());
    let client = common::client_from_providers(json!({
        "providers": [{
            "provider_type": "text",
            "name": "text_demo",
            "base_path": base_path
        }]
    }))
    .await?;
    let tools = client.search_tools("", 10).await?;
    println!("    tools: {}", tools.len());

    let mut args = HashMap::new();
    args.insert("name".into(), json!("world"));
    let value = client.call_tool("text_demo.hello", args).await;
    println!("    call result: {:?}", value);
    Ok(())
}

async fn demo_webrtc() -> Result<()> {
    let Some(sig) = env("DEMO_WEBRTC_SIGNALING") else {
        println!("  ▫️ WebRTC: set DEMO_WEBRTC_SIGNALING to run (transport currently a stub)");
        return Ok(());
    };
    println!("  ▶️ WebRTC -> {sig}");
    let client = common::client_from_providers(json!({
        "providers": [{
            "provider_type": "webrtc",
            "name": "webrtc_demo",
            "signaling_server": sig
        }]
    }))
    .await?;

    let mut args = HashMap::new();
    args.insert("message".into(), json!("hello p2p"));
    let value = client.call_tool("webrtc_demo.echo", args).await;
    println!("    call result: {:?}", value);
    Ok(())
}

fn split_host_port(addr: &str) -> Result<(String, u16)> {
    let mut parts = addr.split(':');
    let host = parts.next().unwrap_or_default().to_string();
    let port = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("Missing port in {addr}"))?
        .parse()?;
    Ok((host, port))
}
