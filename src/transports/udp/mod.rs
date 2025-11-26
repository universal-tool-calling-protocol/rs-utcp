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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::base::{BaseProvider, ProviderType};
    use serde_json::json;
    use tokio::net::UdpSocket;

    #[tokio::test]
    async fn call_tool_sends_and_receives_datagram() {
        let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let addr = socket.local_addr().unwrap();

        tokio::spawn(async move {
            let mut buf = vec![0u8; 1024];
            let (len, peer) = socket.recv_from(&mut buf).await.unwrap();
            let incoming: Value = serde_json::from_slice(&buf[..len]).unwrap();
            let response = serde_json::to_vec(&json!({
                "received_tool": incoming.get("tool").cloned().unwrap(),
                "args": incoming.get("args").cloned().unwrap()
            }))
            .unwrap();
            UdpSocket::bind("0.0.0.0:0")
                .await
                .unwrap()
                .send_to(&response, peer)
                .await
                .unwrap();
        });

        let prov = UdpProvider {
            base: BaseProvider {
                name: "udp".to_string(),
                provider_type: ProviderType::Udp,
                auth: None,
            },
            host: addr.ip().to_string(),
            port: addr.port(),
            timeout_ms: None,
        };

        let mut args = HashMap::new();
        args.insert("value".to_string(), Value::String("ping".to_string()));

        let result = UdpTransport::new()
            .call_tool("echo", args.clone(), &prov)
            .await
            .unwrap();

        assert_eq!(result.get("received_tool"), Some(&json!("echo")));
        assert_eq!(result.get("args"), Some(&json!(args)));
    }
}
