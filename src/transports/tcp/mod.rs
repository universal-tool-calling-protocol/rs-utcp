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
