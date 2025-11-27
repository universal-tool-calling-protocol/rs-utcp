use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use once_cell::sync::Lazy;

use crate::transports::CommunicationProtocol;

/// Plugin-style registry for communication protocols (formerly transports) keyed by call_template_type/provider_type.
#[derive(Clone, Default)]
pub struct CommunicationProtocolRegistry {
    map: HashMap<String, Arc<dyn CommunicationProtocol>>,
}

impl CommunicationProtocolRegistry {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }

    /// Build a registry pre-populated with the built-in communication protocols.
    pub fn with_default_protocols() -> Self {
        let mut reg = Self::new();
        reg.register_default_protocols();
        reg
    }

    /// Backwards-compatible helper matching the old transport terminology.
    pub fn with_default_transports() -> Self {
        Self::with_default_protocols()
    }

    pub fn register_default_protocols(&mut self) {
        self.register(
            "http",
            Arc::new(crate::transports::http::HttpClientTransport::new()),
        );
        self.register("cli", Arc::new(crate::transports::cli::CliTransport::new()));
        self.register(
            "websocket",
            Arc::new(crate::transports::websocket::WebSocketTransport::new()),
        );
        self.register(
            "grpc",
            Arc::new(crate::transports::grpc::GrpcTransport::new()),
        );
        self.register(
            "graphql",
            Arc::new(crate::transports::graphql::GraphQLTransport::new()),
        );
        self.register("tcp", Arc::new(crate::transports::tcp::TcpTransport::new()));
        self.register("udp", Arc::new(crate::transports::udp::UdpTransport::new()));
        self.register("sse", Arc::new(crate::transports::sse::SseTransport::new()));
        self.register("mcp", Arc::new(crate::transports::mcp::McpTransport::new()));
        self.register(
            "webrtc",
            Arc::new(crate::transports::webrtc::WebRtcTransport::new()),
        );
        self.register(
            "http_stream",
            Arc::new(crate::transports::http_stream::StreamableHttpTransport::new()),
        );
        self.register(
            "text",
            Arc::new(crate::transports::text::TextTransport::new()),
        );
    }

    pub fn register(&mut self, key: &str, protocol: Arc<dyn CommunicationProtocol>) {
        self.map.insert(key.to_string(), protocol);
    }

    pub fn get(&self, key: &str) -> Option<Arc<dyn CommunicationProtocol>> {
        self.map.get(key).cloned()
    }

    pub fn as_map(&self) -> HashMap<String, Arc<dyn CommunicationProtocol>> {
        self.map.clone()
    }
}

/// Backwards-compatible alias for the previous registry name.
pub type TransportRegistry = CommunicationProtocolRegistry;

/// Global, plugin-extensible registry that holds every registered communication protocol.
pub static GLOBAL_COMMUNICATION_PROTOCOLS: Lazy<RwLock<CommunicationProtocolRegistry>> =
    Lazy::new(|| {
        let mut reg = CommunicationProtocolRegistry::new();
        reg.register_default_protocols();
        RwLock::new(reg)
    });

/// Register a new communication protocol (transport) implementation globally so all clients can use it.
pub fn register_communication_protocol(key: &str, protocol: Arc<dyn CommunicationProtocol>) {
    let mut reg = GLOBAL_COMMUNICATION_PROTOCOLS
        .write()
        .expect("communication protocol registry poisoned");
    reg.register(key, protocol);
}

/// Snapshot the current set of registered communication protocols.
pub fn communication_protocols_snapshot() -> CommunicationProtocolRegistry {
    GLOBAL_COMMUNICATION_PROTOCOLS
        .read()
        .expect("communication protocol registry poisoned")
        .clone()
}
