// UDP Transport - connectionless datagram communication
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use tokio::net::UdpSocket;

use crate::providers::base::Provider;
use crate::providers::udp::UdpProvider;
use crate::tools::Tool;
use crate::transports::{stream::StreamResult, ClientTransport};

pub struct UdpTransport;

impl UdpTransport {
    pub fn new() -> Self {
        Self
    }

    async fn send_and_receive(&self, server_addr: &str, data: &[u8]) -> Result<Vec<u8>> {
        // Bind to a random local port
        let socket = UdpSocket::bind("0.0.0.0:0").await?;

        // Send data
        socket.send_to(data, server_addr).await?;

        // Receive response
        let mut buf = vec![0u8; 65535]; // Max UDP packet size
        let (len, _) = socket.recv_from(&mut buf).await?;

        buf.truncate(len);
        Ok(buf)
    }
}

#[async_trait]
impl ClientTransport for UdpTransport {
    async fn register_tool_provider(&self, _prov: &dyn Provider) -> Result<Vec<Tool>> {
        // UDP providers define tools statically
        Ok(vec![])
    }

    async fn deregister_tool_provider(&self, _prov: &dyn Provider) -> Result<()> {
        Ok(())
    }

    async fn call_tool(
        &self,
        tool_name: &str,
        args: HashMap<String, Value>,
        prov: &dyn Provider,
    ) -> Result<Value> {
        let udp_prov = prov
            .as_any()
            .downcast_ref::<UdpProvider>()
            .ok_or_else(|| anyhow!("Provider is not a UdpProvider"))?;

        // Build request
        let request = serde_json::json!({
            "tool": tool_name,
            "args": args,
        });

        let request_bytes = serde_json::to_vec(&request)?;

        // Send request and receive response
        let address = format!("{}:{}", udp_prov.host, udp_prov.port);
        let response_bytes = self.send_and_receive(&address, &request_bytes).await?;

        // Parse response
        let result: Value = serde_json::from_slice(&response_bytes)?;
        Ok(result)
    }

    async fn call_tool_stream(
        &self,
        _tool_name: &str,
        _args: HashMap<String, Value>,
        _prov: &dyn Provider,
    ) -> Result<Box<dyn StreamResult>> {
        Err(anyhow!(
            "Streaming not suitable for UDP - use TCP or WebSocket"
        ))
    }
}
