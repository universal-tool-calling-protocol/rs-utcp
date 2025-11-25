use std::net::SocketAddr;
use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use rs_utcp::{
    config::UtcpClientConfig, providers::websocket::WebSocketProvider,
    repository::in_memory::InMemoryToolRepository, tag::tag_search::TagSearchStrategy, UtcpClient,
    UtcpClientInterface,
};
use serde_json::json;
use tokio::net::TcpListener;
use tokio_tungstenite::{accept_async, tungstenite::protocol::Message};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let addr = spawn_ws_server().await?;
    println!("Started WS demo at ws://{addr}/tools");

    let repo = Arc::new(InMemoryToolRepository::new());
    let search = Arc::new(TagSearchStrategy::new(repo.clone(), 1.0));
    let client = UtcpClient::new(UtcpClientConfig::default(), repo, search);

    let provider = WebSocketProvider::new("ws_demo".into(), format!("ws://{addr}/tools"), None);
    let tools = client.register_tool_provider(Arc::new(provider)).await?;
    println!(
        "Tools: {:?}",
        tools.iter().map(|t| &t.name).collect::<Vec<_>>()
    );

    let mut args = std::collections::HashMap::new();
    args.insert("message".into(), serde_json::json!("hello ws"));
    let res = client.call_tool("ws_demo.echo", args).await?;
    println!("Result: {}", serde_json::to_string_pretty(&res)?);
    Ok(())
}

async fn spawn_ws_server() -> anyhow::Result<SocketAddr> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            tokio::spawn(async move {
                let mut ws = accept_async(stream).await.expect("ws accept");
                while let Some(msg) = ws.next().await {
                    match msg {
                        Ok(Message::Text(text)) if text == "manual" => {
                            let manifest = json!({
                                "tools": [{
                                    "name": "echo",
                                    "description": "Echo a message",
                                    "inputs": {"type": "object"},
                                    "outputs": {"type": "object"},
                                    "tags": ["ws"]
                                }]
                            });
                            let _ = ws.send(Message::Text(manifest.to_string())).await;
                        }
                        Ok(Message::Text(text)) => {
                            // Echo back parsed JSON
                            let val: serde_json::Value =
                                serde_json::from_str(&text).unwrap_or(json!({}));
                            let _ = ws.send(Message::Text(val.to_string())).await;
                            break;
                        }
                        Ok(Message::Close(_)) | Err(_) => break,
                        _ => {}
                    }
                }
            });
        }
    });
    Ok(addr)
}
