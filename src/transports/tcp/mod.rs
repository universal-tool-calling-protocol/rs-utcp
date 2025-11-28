// TCP Transport
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::mpsc;

use crate::providers::base::Provider;
use crate::providers::tcp::TcpProvider;
use crate::tools::Tool;
use crate::transports::{
    stream::{boxed_channel_stream, StreamResult},
    ClientTransport,
};

/// TCP transport used for simple length-delimited or line-delimited JSON exchanges.
pub struct TcpTransport;

impl TcpTransport {
    /// Create a TCP transport instance.
    pub fn new() -> Self {
        Self
    }

    async fn send_and_receive(&self, address: &str, data: &[u8]) -> Result<Vec<u8>> {
        let mut stream = TcpStream::connect(address).await?;

        // Send data
        stream.write_all(data).await?;
        stream.flush().await?;

        // Shutdown write half to signal we're done sending
        stream.shutdown().await?;

        // Read response
        let mut buffer = Vec::new();
        stream.read_to_end(&mut buffer).await?;

        Ok(buffer)
    }
}

#[async_trait]
impl ClientTransport for TcpTransport {
    async fn register_tool_provider(&self, _prov: &dyn Provider) -> Result<Vec<Tool>> {
        // TCP providers would define tools statically or via initial handshake
        Ok(vec![])
    }

    async fn deregister_tool_provider(&self, _prov: &dyn Provider) -> Result<()> {
        Ok(())
    }

    async fn call_tool(
        &self,
        _tool_name: &str,
        args: HashMap<String, Value>,
        prov: &dyn Provider,
    ) -> Result<Value> {
        let tcp_prov = prov
            .as_any()
            .downcast_ref::<TcpProvider>()
            .ok_or_else(|| anyhow!("Provider is not a TcpProvider"))?;

        let request = serde_json::to_vec(&json!({
            "tool": _tool_name,
            "args": args
        }))?;
        let address = format!("{}:{}", tcp_prov.host, tcp_prov.port);

        // Optional timeout
        let response = if let Some(timeout) = tcp_prov.timeout_ms {
            tokio::time::timeout(
                std::time::Duration::from_millis(timeout),
                self.send_and_receive(&address, &request),
            )
            .await??
        } else {
            self.send_and_receive(&address, &request).await?
        };

        let result: Value = serde_json::from_slice(&response)?;
        Ok(result)
    }

    async fn call_tool_stream(
        &self,
        _tool_name: &str,
        args: HashMap<String, Value>,
        prov: &dyn Provider,
    ) -> Result<Box<dyn StreamResult>> {
        let tcp_prov = prov
            .as_any()
            .downcast_ref::<TcpProvider>()
            .ok_or_else(|| anyhow!("Provider is not a TcpProvider"))?;

        let request = serde_json::to_vec(&json!({
            "tool": _tool_name,
            "args": args
        }))?;
        let address = format!("{}:{}", tcp_prov.host, tcp_prov.port);
        let mut stream = TcpStream::connect(address).await?;
        stream.write_all(&request).await?;
        stream.write_all(b"\n").await?;
        stream.flush().await?;
        stream.shutdown().await?;

        let timeout = tcp_prov.timeout_ms.map(Duration::from_millis);
        let mut reader = BufReader::new(stream);
        let (tx, rx) = mpsc::channel(32);

        tokio::spawn(async move {
            loop {
                let mut line = String::new();
                let read_future = reader.read_line(&mut line);

                let read_result = if let Some(duration) = timeout {
                    match tokio::time::timeout(duration, read_future).await {
                        Ok(res) => res,
                        Err(_) => {
                            let _ = tx.send(Err(anyhow!("TCP stream timed out"))).await;
                            return;
                        }
                    }
                } else {
                    read_future.await
                };

                match read_result {
                    Ok(0) => return,
                    Ok(_) => {
                        let trimmed = line.trim();
                        if trimmed.is_empty() {
                            continue;
                        }

                        match serde_json::from_str::<Value>(trimmed) {
                            Ok(value) => {
                                if tx.send(Ok(value)).await.is_err() {
                                    return;
                                }
                            }
                            Err(err) => {
                                let _ = tx
                                    .send(Err(anyhow!("Failed to parse TCP stream JSON: {}", err)))
                                    .await;
                                return;
                            }
                        }
                    }
                    Err(err) => {
                        let _ = tx.send(Err(anyhow!("TCP stream error: {}", err))).await;
                        return;
                    }
                }
            }
        });

        Ok(boxed_channel_stream(rx, None))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::base::{BaseProvider, ProviderType};
    use serde_json::json;
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn call_tool_round_trips_over_tcp() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = Vec::new();
            socket.read_to_end(&mut buf).await.unwrap();
            let incoming: Value = serde_json::from_slice(&buf).unwrap();
            let response = serde_json::to_vec(&json!({
                "tool": incoming.get("tool").cloned().unwrap(),
                "args": incoming.get("args").cloned().unwrap()
            }))
            .unwrap();
            socket.write_all(&response).await.unwrap();
        });

        let prov = TcpProvider {
            base: BaseProvider {
                name: "tcp".to_string(),
                provider_type: ProviderType::Tcp,
                auth: None,
            },
            host: addr.ip().to_string(),
            port: addr.port(),
            timeout_ms: None,
        };

        let mut args = HashMap::new();
        args.insert("msg".to_string(), Value::String("hello".to_string()));

        let result = TcpTransport::new()
            .call_tool("echo", args.clone(), &prov)
            .await
            .unwrap();

        assert_eq!(result.get("tool"), Some(&json!("echo")));
        assert_eq!(result.get("args"), Some(&json!(args)));
    }

    #[tokio::test]
    async fn call_tool_stream_reads_newline_delimited_messages() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = Vec::new();
            socket.read_to_end(&mut buf).await.unwrap();

            let messages = vec![json!({"idx": 1}), json!({"idx": 2})];
            for message in messages {
                let line = serde_json::to_vec(&message).unwrap();
                socket.write_all(&line).await.unwrap();
                socket.write_all(b"\n").await.unwrap();
            }
        });

        let prov = TcpProvider {
            base: BaseProvider {
                name: "tcp".to_string(),
                provider_type: ProviderType::Tcp,
                auth: None,
            },
            host: addr.ip().to_string(),
            port: addr.port(),
            timeout_ms: None,
        };

        let mut args = HashMap::new();
        args.insert("value".to_string(), Value::String("v".to_string()));

        let transport = TcpTransport::new();
        let mut stream = transport
            .call_tool_stream("sample", args, &prov)
            .await
            .expect("stream");

        assert_eq!(stream.next().await.unwrap().unwrap(), json!({"idx": 1}));
        assert_eq!(stream.next().await.unwrap().unwrap(), json!({"idx": 2}));
        assert_eq!(stream.next().await.unwrap(), None);
        stream.close().await.unwrap();
    }
}
