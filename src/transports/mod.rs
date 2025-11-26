pub mod cli;
pub mod graphql;
pub mod grpc;
pub mod http;
pub mod http_stream;
pub mod mcp;
pub mod sse;
pub mod stream;
pub mod tcp;
pub mod text;
pub mod udp;
pub mod webrtc;
pub mod websocket;
pub mod registry;

use crate::providers::base::Provider;
use crate::tools::Tool;
use crate::transports::stream::StreamResult;
use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;

#[async_trait]
pub trait ClientTransport: Send + Sync {
    async fn register_tool_provider(&self, prov: &dyn Provider) -> Result<Vec<Tool>>;
    async fn deregister_tool_provider(&self, prov: &dyn Provider) -> Result<()>;
    async fn call_tool(
        &self,
        tool_name: &str,
        args: HashMap<String, Value>,
        prov: &dyn Provider,
    ) -> Result<Value>;
    async fn call_tool_stream(
        &self,
        tool_name: &str,
        args: HashMap<String, Value>,
        prov: &dyn Provider,
    ) -> Result<Box<dyn StreamResult>>;
}
