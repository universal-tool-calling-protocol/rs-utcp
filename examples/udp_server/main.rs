use std::net::SocketAddr;

use rs_utcp::UtcpClientInterface;
use serde_json::Value;
use tokio::net::UdpSocket;

#[path = "../common/mod.rs"]
mod common;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let addr = spawn_udp_server().await?;
    println!("Started UDP demo at {addr}");

    let client = common::client_from_providers(serde_json::json!({
        "manual_call_templates": [{
            "call_template_type": "udp",
            "name": "udp_demo",
            "host": addr.ip().to_string(),
            "port": addr.port(),
            "allowed_communication_protocols": ["udp"]
        }]
    }))
    .await?;

    let mut args = std::collections::HashMap::new();
    args.insert("message".into(), serde_json::json!("hello udp"));
    let res: serde_json::Value = client.call_tool("udp_demo.echo", args).await?;
    println!("Result: {}", serde_json::to_string_pretty(&res)?);
    Ok(())
}

async fn spawn_udp_server() -> anyhow::Result<SocketAddr> {
    let socket = UdpSocket::bind("127.0.0.1:0").await?;
    let addr = socket.local_addr()?;
    tokio::spawn(async move {
        let mut buf = vec![0u8; 2048];
        loop {
            let Ok((len, peer)) = socket.recv_from(&mut buf).await else {
                break;
            };
            let val: Value = serde_json::from_slice(&buf[..len]).unwrap_or(Value::Null);
            let _ = socket.send_to(val.to_string().as_bytes(), peer).await;
        }
    });
    Ok(addr)
}
