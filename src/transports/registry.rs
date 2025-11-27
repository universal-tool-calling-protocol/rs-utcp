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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::base::ProviderType;
    use crate::transports::stream::boxed_vec_stream;
    use crate::transports::CommunicationProtocol;
    use async_trait::async_trait;
    use serde_json::Value;

    #[derive(Debug)]
    struct DummyProtocol;

    #[async_trait]
    impl CommunicationProtocol for DummyProtocol {
        async fn register_tool_provider(
            &self,
            _prov: &dyn crate::providers::base::Provider,
        ) -> anyhow::Result<Vec<crate::tools::Tool>> {
            Ok(vec![])
        }

        async fn deregister_tool_provider(
            &self,
            _prov: &dyn crate::providers::base::Provider,
        ) -> anyhow::Result<()> {
            Ok(())
        }

        async fn call_tool(
            &self,
            _tool_name: &str,
            _args: HashMap<String, Value>,
            _prov: &dyn crate::providers::base::Provider,
        ) -> anyhow::Result<Value> {
            Ok(Value::Null)
        }

        async fn call_tool_stream(
            &self,
            _tool_name: &str,
            _args: HashMap<String, Value>,
            _prov: &dyn crate::providers::base::Provider,
        ) -> anyhow::Result<Box<dyn crate::transports::stream::StreamResult>> {
            Ok(boxed_vec_stream(vec![Value::Null]))
        }
    }

    #[test]
    fn default_protocol_registry_contains_all_builtins() {
        let reg = CommunicationProtocolRegistry::with_default_protocols();
        let expected = vec![
            "http",
            "cli",
            "websocket",
            "grpc",
            "graphql",
            "tcp",
            "udp",
            "sse",
            "mcp",
            "webrtc",
            "http_stream",
            "text",
        ];
        for key in &expected {
            assert!(
                reg.get(key).is_some(),
                "missing built-in protocol {key}"
            );
        }
        assert_eq!(reg.as_map().len(), expected.len());
    }

    #[test]
    fn transport_alias_builds_default_protocols() {
        let reg = TransportRegistry::with_default_transports();
        // Reuse provider type keys to ensure mapping doesn't drift.
        let provider_keys = vec![
            ProviderType::Http,
            ProviderType::Cli,
            ProviderType::Websocket,
            ProviderType::Grpc,
            ProviderType::Graphql,
            ProviderType::Tcp,
            ProviderType::Udp,
            ProviderType::Sse,
            ProviderType::Mcp,
            ProviderType::Webrtc,
            ProviderType::HttpStream,
            ProviderType::Text,
        ]
        .into_iter()
        .map(|p| p.as_key().to_string())
        .collect::<Vec<_>>();

        for key in provider_keys {
            assert!(reg.get(&key).is_some(), "missing protocol for {key}");
        }
    }

    #[test]
    fn register_global_protocol_exposes_it_in_snapshot() {
        let key = "dummy_protocol_test";
        register_communication_protocol(key, Arc::new(DummyProtocol));

        let snapshot = communication_protocols_snapshot();
        assert!(snapshot.get(key).is_some(), "global registry missing {key}");

        // Clean up to avoid leaking state between tests.
        if let Ok(mut guard) = GLOBAL_COMMUNICATION_PROTOCOLS.write() {
            guard.map.remove(key);
        }
    }
}
