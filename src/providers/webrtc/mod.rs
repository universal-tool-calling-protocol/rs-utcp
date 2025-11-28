use serde::{Deserialize, Serialize};

use crate::auth::AuthConfig;
use crate::providers::base::{BaseProvider, Provider, ProviderType};

/// STUN/TURN server configuration passed to the WebRTC stack.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IceServer {
    pub urls: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credential: Option<String>,
}

/// Provider definition for peer-to-peer WebRTC transports.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebRtcProvider {
    #[serde(flatten)]
    pub base: BaseProvider,

    /// Signaling server URL (WebSocket or HTTP)
    pub signaling_server: String,

    /// ICE servers (STUN/TURN)
    #[serde(default = "default_ice_servers")]
    pub ice_servers: Vec<IceServer>,

    /// Data channel label
    #[serde(default = "default_channel_label")]
    pub channel_label: String,

    /// Whether to use ordered delivery
    #[serde(default = "default_true")]
    pub ordered: bool,

    /// Max packet lifetime in milliseconds (for unordered channels)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_packet_life_time: Option<u16>,

    /// Max retransmits (for unordered channels)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_retransmits: Option<u16>,
}

fn default_ice_servers() -> Vec<IceServer> {
    vec![IceServer {
        urls: vec!["stun:stun.l.google.com:19302".to_string()],
        username: None,
        credential: None,
    }]
}

fn default_channel_label() -> String {
    "utcp-data".to_string()
}

fn default_true() -> bool {
    true
}

impl Provider for WebRtcProvider {
    fn type_(&self) -> ProviderType {
        ProviderType::Webrtc
    }

    fn name(&self) -> String {
        self.base.name.clone()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl WebRtcProvider {
    /// Construct a WebRTC provider backed by a signaling server URL.
    pub fn new(name: String, signaling_server: String, auth: Option<AuthConfig>) -> Self {
        Self {
            base: BaseProvider {
                name,
                provider_type: ProviderType::Webrtc,
                auth,
            },
            signaling_server,
            ice_servers: default_ice_servers(),
            channel_label: default_channel_label(),
            ordered: true,
            max_packet_life_time: None,
            max_retransmits: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_webrtc_provider_defaults() {
        let json = json!({
            "name": "test-webrtc",
            "provider_type": "webrtc",
            "signaling_server": "http://localhost:8080"
        });

        let provider: WebRtcProvider = serde_json::from_value(json).unwrap();

        assert_eq!(provider.base.name, "test-webrtc");
        assert_eq!(provider.signaling_server, "http://localhost:8080");
        assert_eq!(provider.channel_label, "utcp-data");
        assert!(provider.ordered);
        assert_eq!(provider.ice_servers.len(), 1);
        assert_eq!(
            provider.ice_servers[0].urls[0],
            "stun:stun.l.google.com:19302"
        );
    }

    #[test]
    fn test_webrtc_provider_full_config() {
        let json = json!({
            "name": "test-webrtc-full",
            "provider_type": "webrtc",
            "signaling_server": "wss://signal.example.com",
            "ice_servers": [
                {
                    "urls": ["turn:turn.example.com"],
                    "username": "user",
                    "credential": "pass"
                }
            ],
            "channel_label": "custom-label",
            "ordered": false,
            "max_packet_life_time": 100,
            "max_retransmits": 5
        });

        let provider: WebRtcProvider = serde_json::from_value(json).unwrap();

        assert_eq!(provider.signaling_server, "wss://signal.example.com");
        assert_eq!(provider.ice_servers.len(), 1);
        assert_eq!(provider.ice_servers[0].username.as_deref(), Some("user"));
        assert_eq!(provider.channel_label, "custom-label");
        assert!(!provider.ordered);
        assert_eq!(provider.max_packet_life_time, Some(100));
        assert_eq!(provider.max_retransmits, Some(5));
    }
}
