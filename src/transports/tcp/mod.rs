// TCP Transport
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::providers::base::Provider;
use crate::providers::tcp::TcpProvider;
use crate::tools::Tool;
use crate::transports::{stream::StreamResult, ClientTransport};

pub struct TcpTransport;

impl TcpTransport {
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

        let request = serde_json::to_vec(&args)?;
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
        _args: HashMap<String, Value>,
        _prov: &dyn Provider,
    ) -> Result<Box<dyn StreamResult>> {
        Err(anyhow!(
            "TCP streaming requires persistent connection - not fully implemented"
        ))
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
            let args_value: Value = serde_json::from_slice(&buf).unwrap();
            let response = serde_json::to_vec(&json!({ "echo": args_value })).unwrap();
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
            .call_tool("ignored", args.clone(), &prov)
            .await
            .unwrap();

        assert_eq!(result.get("echo"), Some(&json!(args)));
    }

    #[tokio::test]
    async fn register_returns_empty_and_stream_not_supported() {
        let prov = TcpProvider {
            base: BaseProvider {
                name: "tcp".to_string(),
                provider_type: ProviderType::Tcp,
                auth: None,
            },
            host: "127.0.0.1".to_string(),
            port: 0,
            timeout_ms: None,
        };

        let transport = TcpTransport::new();
        assert!(transport
            .register_tool_provider(&prov)
            .await
            .unwrap()
            .is_empty());

        let err = transport
            .call_tool_stream("tool", HashMap::new(), &prov)
            .await
            .err()
            .expect("stream error");
        assert!(err.to_string().contains("not fully implemented"));
    }
}
