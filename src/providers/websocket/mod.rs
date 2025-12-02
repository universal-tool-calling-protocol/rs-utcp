use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::auth::AuthConfig;
use crate::providers::base::{BaseProvider, Provider, ProviderType};

/// Provider configuration for WebSocket endpoints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSocketProvider {
    #[serde(flatten)]
    pub base: BaseProvider,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol: Option<String>,
    #[serde(default)]
    pub keep_alive: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<HashMap<String, String>>,
}

impl Provider for WebSocketProvider {
    fn type_(&self) -> ProviderType {
        ProviderType::Websocket
    }

    fn name(&self) -> String {
        self.base.name.clone()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl WebSocketProvider {
    /// Create a WebSocket provider pointing at a URL.
    pub fn new(name: String, url: String, auth: Option<AuthConfig>) -> Self {
        Self {
            base: BaseProvider {
                name,
                provider_type: ProviderType::Websocket,
                auth,
                allowed_communication_protocols: None,
            },
            url,
            protocol: None,
            keep_alive: false,
            headers: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_websocket_provider_deserialization() {
        let json = json!({
            "name": "test-ws",
            "provider_type": "websocket",
            "url": "ws://localhost:8080"
        });

        let provider: WebSocketProvider = serde_json::from_value(json).unwrap();
        assert_eq!(provider.base.name, "test-ws");
        assert_eq!(provider.url, "ws://localhost:8080");
        assert!(!provider.keep_alive);
        assert!(provider.protocol.is_none());
    }

    #[test]
    fn test_websocket_provider_full_config() {
        let json = json!({
            "name": "test-ws-full",
            "provider_type": "websocket",
            "url": "wss://example.com/ws",
            "protocol": "json-rpc",
            "keep_alive": true,
            "headers": {
                "Authorization": "Bearer token"
            }
        });

        let provider: WebSocketProvider = serde_json::from_value(json).unwrap();
        assert_eq!(provider.url, "wss://example.com/ws");
        assert_eq!(provider.protocol.as_deref(), Some("json-rpc"));
        assert!(provider.keep_alive);
        assert_eq!(
            provider
                .headers
                .unwrap()
                .get("Authorization")
                .map(|s| s.as_str()),
            Some("Bearer token")
        );
    }
}
