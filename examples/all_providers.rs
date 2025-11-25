//! Showcase for every provider/transport. Mirrors go-utcp example set.
//! Each section is opt-in via environment variables so you can run what you have
//! endpoints for without the others failing.
//!
//! Example:
//!   DEMO_HTTP_URL=https://httpbin.org/post \
//!   DEMO_WS_URL=wss://echo.websocket.events \
//!   cargo run --example all_providers

use anyhow::Result;
use rs_utcp::config::UtcpClientConfig;
use rs_utcp::providers::cli::CliProvider;
use rs_utcp::providers::graphql::GraphqlProvider;
use rs_utcp::providers::grpc::GrpcProvider;
use rs_utcp::providers::http::HttpProvider;
use rs_utcp::providers::http_stream::StreamableHttpProvider;
use rs_utcp::providers::mcp::McpProvider;
use rs_utcp::providers::sse::SseProvider;
use rs_utcp::providers::tcp::TcpProvider;
use rs_utcp::providers::text::TextProvider;
use rs_utcp::providers::udp::UdpProvider;
use rs_utcp::providers::webrtc::WebRtcProvider;
use rs_utcp::providers::websocket::WebSocketProvider;
use rs_utcp::repository::in_memory::InMemoryToolRepository;
use rs_utcp::tag::tag_search::TagSearchStrategy;
use rs_utcp::{UtcpClient, UtcpClientInterface};
use serde_json::json;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::time::{timeout, Duration};

fn env(key: &str) -> Option<String> {
    std::env::var(key).ok()
}

#[tokio::main]
async fn main() -> Result<()> {
    let repo = Arc::new(InMemoryToolRepository::new());
    let search = Arc::new(TagSearchStrategy::new(repo.clone(), 1.0));
    let client = UtcpClient::new(UtcpClientConfig::default(), repo, search);

    println!("Running provider demos (set DEMO_* env vars to enable a block):");

    if let Err(e) = demo_http(&client).await {
        eprintln!("HTTP demo error: {e}");
    }
    if let Err(e) = demo_cli(&client).await {
        eprintln!("CLI demo error: {e}");
    }
    if let Err(e) = demo_websocket(&client).await {
        eprintln!("WebSocket demo error: {e}");
    }
    if let Err(e) = demo_graphql(&client).await {
        eprintln!("GraphQL demo error: {e}");
    }
    if let Err(e) = demo_grpc(&client).await {
        eprintln!("gRPC demo error: {e}");
    }
    if let Err(e) = demo_tcp(&client).await {
        eprintln!("TCP demo error: {e}");
    }
    if let Err(e) = demo_udp(&client).await {
        eprintln!("UDP demo error: {e}");
    }
    if let Err(e) = demo_sse(&client).await {
        eprintln!("SSE demo error: {e}");
    }
    if let Err(e) = demo_http_stream(&client).await {
        eprintln!("HTTP stream demo error: {e}");
    }
    if let Err(e) = demo_mcp(&client).await {
        eprintln!("MCP demo error: {e}");
    }
    if let Err(e) = demo_text(&client).await {
        eprintln!("Text demo error: {e}");
    }
    if let Err(e) = demo_webrtc(&client).await {
        eprintln!("WebRTC demo error: {e}");
    }

    Ok(())
}

async fn demo_http(client: &UtcpClient) -> Result<()> {
    let Some(url) = env("DEMO_HTTP_URL") else {
        println!("  ▫️ HTTP: set DEMO_HTTP_URL to run");
        return Ok(());
    };

    println!("  ▶️ HTTP -> {url}");
    let provider = HttpProvider::new("http_demo".into(), url, "POST".into(), None);
    let provider = Arc::new(provider);
    let _ = client.register_tool_provider(provider.clone()).await?;

    let mut args = HashMap::new();
    args.insert("echo".into(), json!("hello from rust-utcp"));
    let value = client.call_tool("http_demo.echo", args).await?;
    println!("    response: {value}");
    Ok(())
}

async fn demo_cli(client: &UtcpClient) -> Result<()> {
    let Some(cmd) = env("DEMO_CLI_CMD") else {
        println!("  ▫️ CLI: set DEMO_CLI_CMD (e.g., echo '{{\"tools\":[]}}') to run");
        return Ok(());
    };
    println!("  ▶️ CLI -> {cmd}");
    let provider = CliProvider::new("cli_demo".into(), cmd, None);
    let provider = Arc::new(provider);
    let _ = client.register_tool_provider(provider.clone()).await?;

    let mut args = HashMap::new();
    args.insert("msg".into(), json!("hello cli"));
    let value = client.call_tool("cli_demo.echo", args).await;
    println!("    call result: {:?}", value);
    Ok(())
}

async fn demo_websocket(client: &UtcpClient) -> Result<()> {
    let Some(url) = env("DEMO_WS_URL") else {
        println!("  ▫️ WebSocket: set DEMO_WS_URL to run (e.g., wss://echo.websocket.events)");
        return Ok(());
    };
    println!("  ▶️ WebSocket -> {url}");
    let provider = WebSocketProvider::new("ws_demo".into(), url, None);
    let provider = Arc::new(provider);
    let _ = client.register_tool_provider(provider.clone()).await?;

    let mut args = HashMap::new();
    args.insert("text".into(), json!("hello websocket"));
    let value = client.call_tool("ws_demo.echo", args).await;
    println!("    call result: {:?}", value);
    Ok(())
}

async fn demo_graphql(client: &UtcpClient) -> Result<()> {
    let Some(url) = env("DEMO_GRAPHQL_URL") else {
        println!("  ▫️ GraphQL: set DEMO_GRAPHQL_URL to run");
        return Ok(());
    };
    println!("  ▶️ GraphQL -> {url}");
    let provider = GraphqlProvider::new("graphql_demo".into(), url, None);
    let provider = Arc::new(provider);
    let _ = client.register_tool_provider(provider.clone()).await?;

    let mut args = HashMap::new();
    args.insert("name".into(), json!("rust-utcp"));
    let value = client.call_tool("graphql_demo.hello", args).await;
    println!("    call result: {:?}", value);
    Ok(())
}

async fn demo_grpc(client: &UtcpClient) -> Result<()> {
    let (Some(host), Some(port)) = (env("DEMO_GRPC_HOST"), env("DEMO_GRPC_PORT")) else {
        println!("  ▫️ gRPC: set DEMO_GRPC_HOST and DEMO_GRPC_PORT to run");
        return Ok(());
    };
    let port: u16 = port.parse().unwrap_or(50051);
    println!("  ▶️ gRPC -> {host}:{port}");
    let provider = GrpcProvider::new("grpc_demo".into(), host, port, None);
    let provider = Arc::new(provider);
    let _ = client.register_tool_provider(provider.clone()).await?;

    let mut args = HashMap::new();
    args.insert("echo".into(), json!("hello grpc"));
    let value = client.call_tool("grpc_demo.echo", args).await;
    println!("    call result: {:?}", value);
    Ok(())
}

async fn demo_tcp(client: &UtcpClient) -> Result<()> {
    let Some(addr) = env("DEMO_TCP_ADDR") else {
        println!("  ▫️ TCP: set DEMO_TCP_ADDR (host:port) to run");
        return Ok(());
    };
    let (host, port) = split_host_port(&addr)?;
    println!("  ▶️ TCP -> {addr}");
    let provider = TcpProvider::new("tcp_demo".into(), host, port, None);
    let provider = Arc::new(provider);
    let _ = client.register_tool_provider(provider.clone()).await?;

    let mut args = HashMap::new();
    args.insert("ping".into(), json!("pong"));
    let value = client.call_tool("tcp_demo.echo", args).await;
    println!("    call result: {:?}", value);
    Ok(())
}

async fn demo_udp(client: &UtcpClient) -> Result<()> {
    let Some(addr) = env("DEMO_UDP_ADDR") else {
        println!("  ▫️ UDP: set DEMO_UDP_ADDR (host:port) to run");
        return Ok(());
    };
    let (host, port) = split_host_port(&addr)?;
    println!("  ▶️ UDP -> {addr}");
    let provider = UdpProvider::new("udp_demo".into(), host, port, None);
    let provider = Arc::new(provider);
    let _ = client.register_tool_provider(provider.clone()).await?;

    let mut args = HashMap::new();
    args.insert("ping".into(), json!("pong"));
    let value = client.call_tool("udp_demo.echo", args).await;
    println!("    call result: {:?}", value);
    Ok(())
}

async fn demo_sse(client: &UtcpClient) -> Result<()> {
    let Some(url) = env("DEMO_SSE_URL") else {
        println!("  ▫️ SSE: set DEMO_SSE_URL to run");
        return Ok(());
    };
    println!("  ▶️ SSE -> {url}");
    let provider = SseProvider::new("sse_demo".into(), url, None);
    let provider = Arc::new(provider);
    let _ = client.register_tool_provider(provider.clone()).await?;

    let mut args = HashMap::new();
    args.insert("topic".into(), json!("demo"));
    let mut stream = client.call_tool_stream("sse_demo.stream", args).await?;
    let item = timeout(Duration::from_secs(5), stream.next()).await;
    println!("    first event: {:?}", item);
    let _ = stream.close().await;
    Ok(())
}

async fn demo_http_stream(client: &UtcpClient) -> Result<()> {
    let Some(url) = env("DEMO_HTTP_STREAM_URL") else {
        println!("  ▫️ HTTP Stream: set DEMO_HTTP_STREAM_URL to run");
        return Ok(());
    };
    println!("  ▶️ HTTP Stream -> {url}");
    let provider = StreamableHttpProvider::new("http_stream_demo".into(), url, None);
    let provider = Arc::new(provider);
    let _ = client.register_tool_provider(provider.clone()).await?;

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

async fn demo_mcp(client: &UtcpClient) -> Result<()> {
    let Some(url) = env("DEMO_MCP_URL") else {
        println!("  ▫️ MCP: set DEMO_MCP_URL to run");
        return Ok(());
    };
    println!("  ▶️ MCP -> {url}");
    let provider = McpProvider::new("mcp_demo".into(), url, None);
    let provider = Arc::new(provider);
    let tools = client.register_tool_provider(provider.clone()).await?;
    println!("    tools: {}", tools.len());

    let mut args = HashMap::new();
    args.insert("name".into(), json!("echo"));
    let value = client.call_tool("mcp_demo.echo", args).await;
    println!("    call result: {:?}", value);
    Ok(())
}

async fn demo_text(client: &UtcpClient) -> Result<()> {
    let Some(path) = env("DEMO_TEXT_PATH") else {
        println!(
            "  ▫️ Text: set DEMO_TEXT_PATH to a folder containing tools.json and scripts to run"
        );
        return Ok(());
    };
    let base_path = PathBuf::from(path);
    println!("  ▶️ Text -> {}", base_path.display());
    let provider = TextProvider::new("text_demo".into(), Some(base_path.clone()), None);
    let provider = Arc::new(provider);
    let tools = client.register_tool_provider(provider.clone()).await?;
    println!("    tools: {}", tools.len());

    let mut args = HashMap::new();
    args.insert("name".into(), json!("world"));
    let value = client.call_tool("text_demo.hello", args).await;
    println!("    call result: {:?}", value);
    Ok(())
}

async fn demo_webrtc(client: &UtcpClient) -> Result<()> {
    let Some(sig) = env("DEMO_WEBRTC_SIGNALING") else {
        println!("  ▫️ WebRTC: set DEMO_WEBRTC_SIGNALING to run (transport currently a stub)");
        return Ok(());
    };
    println!("  ▶️ WebRTC -> {sig}");
    let provider = WebRtcProvider::new("webrtc_demo".into(), None);
    let provider = Arc::new(provider);
    let _ = client.register_tool_provider(provider.clone()).await?;

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
