// Example demonstrating GraphQL subscription support with call_tool_stream
use futures::{SinkExt, StreamExt};
use rs_utcp::UtcpClientInterface;
use serde_json::json;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::time::sleep;
use tokio_tungstenite::{accept_async, tungstenite::Message};

#[path = "common/mod.rs"]
mod common;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Spawn a GraphQL subscription server
    let addr = spawn_graphql_subscription_server().await?;
    println!("Started GraphQL subscription server at ws://{}", addr);

    // Give the server time to start
    sleep(Duration::from_millis(100)).await;

    // Create a UTCP client with a GraphQL subscription provider
    let client = common::client_from_providers(json!({
        "providers": [{
            "provider_type": "graphql",
            "name": "stock_sub",
            "url": format!("http://{}", addr),
            "operation_type": "subscription"
        }]
    }))
    .await?;

    println!("Subscribing to stock price updates...");

    // Call the subscription tool and get a stream
    let mut stream = client
        .call_tool_stream("stock_sub.stockPriceUpdates", Default::default())
        .await?;

    // Consume streaming results
    let mut count = 0;
    while let Ok(Some(value)) = stream.next().await {
        println!("📈 Update #{}: {}", count + 1, value);
        count += 1;
        if count >= 5 {
            // Stop after 5 updates
            break;
        }
    }

    println!("\n✅ Received {} stock price updates", count);
    stream.close().await?;
    Ok(())
}

/// Spawns a GraphQL subscription server that implements the graphql-transport-ws protocol
async fn spawn_graphql_subscription_server() -> anyhow::Result<std::net::SocketAddr> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;

    tokio::spawn(async move {
        loop {
            if let Ok((stream, _)) = listener.accept().await {
                tokio::spawn(async move {
                    if let Ok(mut ws) = accept_async(stream).await {
                        // Handle GraphQL subscription protocol
                        while let Some(msg) = ws.next().await {
                            match msg {
                                Ok(Message::Text(text)) => {
                                    let payload: serde_json::Value =
                                        serde_json::from_str(&text).unwrap_or_default();

                                    match payload.get("type").and_then(|v| v.as_str()) {
                                        Some("connection_init") => {
                                            // Send connection_ack
                                            let _ = ws
                                                .send(Message::Text(
                                                    json!({ "type": "connection_ack" }).to_string(),
                                                ))
                                                .await;
                                        }
                                        Some("subscribe") => {
                                            // Send periodic stock price updates
                                            for i in 1..=10 {
                                                let price = 100.0 + (i as f64 * 2.5);
                                                let update = json!({
                                                    "id": "1",
                                                    "type": "next",
                                                    "payload": {
                                                        "data": {
                                                            "stockPriceUpdates": {
                                                                "symbol": "UTCP",
                                                                "price": price,
                                                                "update": i
                                                            }
                                                        }
                                                    }
                                                });

                                                if ws
                                                    .send(Message::Text(update.to_string()))
                                                    .await
                                                    .is_err()
                                                {
                                                    return;
                                                }

                                                sleep(Duration::from_secs(1)).await;
                                            }

                                            // Send complete message
                                            let _ = ws
                                                .send(Message::Text(
                                                    json!({
                                                        "id": "1",
                                                        "type": "complete"
                                                    })
                                                    .to_string(),
                                                ))
                                                .await;
                                            return;
                                        }
                                        _ => {}
                                    }
                                }
                                Ok(Message::Close(_)) => return,
                                Err(_) => return,
                                _ => {}
                            }
                        }
                    }
                });
            }
        }
    });

    Ok(addr)
}
