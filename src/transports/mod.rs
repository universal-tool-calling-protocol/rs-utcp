pub mod cli;
pub mod graphql;
pub mod grpc;
pub mod http;
pub mod http_stream;
pub mod mcp;
pub mod registry;
pub mod sse;
pub mod stream;
pub mod tcp;
pub mod text;
pub mod udp;
pub mod webrtc;
pub mod websocket;

use crate::providers::base::Provider;
use crate::tools::Tool;
use crate::transports::stream::StreamResult;
use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;

/// Core transport abstraction all communication protocols implement.
#[async_trait]
pub trait ClientTransport: Send + Sync {
    /// Register a tool provider with the underlying transport, returning discovered tools.
    async fn register_tool_provider(&self, prov: &dyn Provider) -> Result<Vec<Tool>>;
    /// Deregister a tool provider and release any associated resources.
    async fn deregister_tool_provider(&self, prov: &dyn Provider) -> Result<()>;
    /// Invoke a tool over the transport and return the result payload.
    async fn call_tool(
        &self,
        tool_name: &str,
        args: HashMap<String, Value>,
        prov: &dyn Provider,
    ) -> Result<Value>;
    /// Invoke a tool and stream incremental responses back to the caller.
    async fn call_tool_stream(
        &self,
        tool_name: &str,
        args: HashMap<String, Value>,
        prov: &dyn Provider,
    ) -> Result<Box<dyn StreamResult>>;
}

// CommunicationProtocol is the new name for transports; kept as a re-export for backwards
// compatibility so plugins can implement the updated terminology without breaking old code.
pub use ClientTransport as CommunicationProtocol;

pub use registry::{
    communication_protocols_snapshot, register_communication_protocol,
    CommunicationProtocolRegistry,
};
