use std::net::SocketAddr;
use std::sync::Arc;

use rs_utcp::{
    config::UtcpClientConfig, providers::udp::UdpProvider,
    repository::in_memory::InMemoryToolRepository, tag::tag_search::TagSearchStrategy, UtcpClient,
    UtcpClientInterface,
};
use serde_json::Value;
use tokio::net::UdpSocket;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let addr = spawn_udp_server().await?;
    println!("Started UDP demo at {addr}");

    let repo = Arc::new(InMemoryToolRepository::new());
    let search = Arc::new(TagSearchStrategy::new(repo.clone(), 1.0));
    let client = UtcpClient::new(UtcpClientConfig::default(), repo, search);

    let provider = UdpProvider::new("udp_demo".into(), addr.ip().to_string(), addr.port(), None);
    client.register_tool_provider(Arc::new(provider)).await?;

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
