use serde::{Deserialize, Serialize};

use crate::auth::AuthConfig;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderType {
    Http,
    Sse,
    HttpStream,
    Cli,
    Websocket,
    Grpc,
    Graphql,
    Tcp,
    Udp,
    Webrtc,
    Mcp,
    Text,
    #[serde(other)]
    Unknown,
}

impl ProviderType {
    pub fn as_key(&self) -> &'static str {
        match self {
            ProviderType::Http => "http",
            ProviderType::Sse => "sse",
            ProviderType::HttpStream => "http_stream",
            ProviderType::Cli => "cli",
            ProviderType::Websocket => "websocket",
            ProviderType::Grpc => "grpc",
            ProviderType::Graphql => "graphql",
            ProviderType::Tcp => "tcp",
            ProviderType::Udp => "udp",
            ProviderType::Webrtc => "webrtc",
            ProviderType::Mcp => "mcp",
            ProviderType::Text => "text",
            ProviderType::Unknown => "unknown",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ProviderType;

    #[test]
    fn provider_type_keys_match_transport_names() {
        assert_eq!(ProviderType::Http.as_key(), "http");
        assert_eq!(ProviderType::Sse.as_key(), "sse");
        assert_eq!(ProviderType::HttpStream.as_key(), "http_stream");
        assert_eq!(ProviderType::Cli.as_key(), "cli");
        assert_eq!(ProviderType::Websocket.as_key(), "websocket");
        assert_eq!(ProviderType::Grpc.as_key(), "grpc");
        assert_eq!(ProviderType::Graphql.as_key(), "graphql");
        assert_eq!(ProviderType::Tcp.as_key(), "tcp");
        assert_eq!(ProviderType::Udp.as_key(), "udp");
        assert_eq!(ProviderType::Webrtc.as_key(), "webrtc");
        assert_eq!(ProviderType::Mcp.as_key(), "mcp");
        assert_eq!(ProviderType::Text.as_key(), "text");
        assert_eq!(ProviderType::Unknown.as_key(), "unknown");
    }
}

pub trait Provider: Send + Sync + std::fmt::Debug + std::any::Any {
    fn type_(&self) -> ProviderType;
    fn name(&self) -> String;

    fn as_any(&self) -> &dyn std::any::Any;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaseProvider {
    pub name: String,
    pub provider_type: ProviderType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth: Option<AuthConfig>,
}

impl Provider for BaseProvider {
    fn type_(&self) -> ProviderType {
        self.provider_type.clone()
    }
    fn name(&self) -> String {
        self.name.clone()
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
