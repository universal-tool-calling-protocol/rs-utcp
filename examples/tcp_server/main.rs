use std::net::SocketAddr;

use rs_utcp::UtcpClientInterface;
use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[path = "../common/mod.rs"]
mod common;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let addr = spawn_tcp_server().await?;
    println!("Started TCP demo at {addr}");

    let client = common::client_from_providers(serde_json::json!({
        "manual_version": "1.0.0",
        "utcp_version": "0.3.0",
        "allowed_communication_protocols": ["tcp"],
        "info": {
            "title": "TCP Demo",
            "version": "1.0.0",
            "description": "TCP Demo Manual"
        },
        "tools": [{
            "name": "echo",
            "description": "TCP Echo",
            "inputs": { "type": "object" },
            "outputs": { "type": "object" },
            "tool_call_template": {
                "call_template_type": "tcp",
                "name": "tcp_demo",
                "host": addr.ip().to_string(),
                "port": addr.port()
            }
        }]
    }))
    .await?;

    let mut args = std::collections::HashMap::new();
    args.insert("message".into(), serde_json::json!("hello tcp"));
    let res = client.call_tool("tcp_demo.echo", args).await?;
    println!("Result: {}", serde_json::to_string_pretty(&res)?);
    Ok(())
}

async fn spawn_tcp_server() -> anyhow::Result<SocketAddr> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            tokio::spawn(async move {
                let mut buf = Vec::new();
                if socket.read_to_end(&mut buf).await.is_ok() {
                    let val: Value = serde_json::from_slice(&buf).unwrap_or(Value::Null);
                    let _ = socket.write_all(val.to_string().as_bytes()).await;
                }
            });
        }
    });
    Ok(addr)
}
