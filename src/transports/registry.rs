use std::collections::HashMap;
use std::sync::Arc;

use crate::transports::ClientTransport;

/// Simple plugin-style registry for transports keyed by call_template_type/provider_type.
#[derive(Clone, Default)]
pub struct TransportRegistry {
    map: HashMap<String, Arc<dyn ClientTransport>>,
}

impl TransportRegistry {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }

    pub fn with_default_transports() -> Self {
        let mut reg = Self::new();
        reg.register(
            "http",
            Arc::new(crate::transports::http::HttpClientTransport::new()),
        );
        reg.register("cli", Arc::new(crate::transports::cli::CliTransport::new()));
        reg.register(
            "websocket",
            Arc::new(crate::transports::websocket::WebSocketTransport::new()),
        );
        reg.register(
            "grpc",
            Arc::new(crate::transports::grpc::GrpcTransport::new()),
        );
        reg.register(
            "graphql",
            Arc::new(crate::transports::graphql::GraphQLTransport::new()),
        );
        reg.register("tcp", Arc::new(crate::transports::tcp::TcpTransport::new()));
        reg.register("udp", Arc::new(crate::transports::udp::UdpTransport::new()));
        reg.register("sse", Arc::new(crate::transports::sse::SseTransport::new()));
        reg.register("mcp", Arc::new(crate::transports::mcp::McpTransport::new()));
        reg.register(
            "webrtc",
            Arc::new(crate::transports::webrtc::WebRtcTransport::new()),
        );
        reg.register(
            "http_stream",
            Arc::new(crate::transports::http_stream::StreamableHttpTransport::new()),
        );
        reg.register(
            "text",
            Arc::new(crate::transports::text::TextTransport::new()),
        );
        reg
    }

    pub fn register(&mut self, key: &str, transport: Arc<dyn ClientTransport>) {
        self.map.insert(key.to_string(), transport);
    }

    pub fn get(&self, key: &str) -> Option<Arc<dyn ClientTransport>> {
        self.map.get(key).cloned()
    }

    pub fn as_map(&self) -> HashMap<String, Arc<dyn ClientTransport>> {
        self.map.clone()
    }
}
