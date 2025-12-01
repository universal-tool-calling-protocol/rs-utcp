use serde::{Deserialize, Serialize};

use crate::auth::AuthConfig;
use crate::providers::base::{BaseProvider, Provider, ProviderType};

/// Provider definition for UDP datagram endpoints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UdpProvider {
    #[serde(flatten)]
    pub base: BaseProvider,
    pub host: String,
    pub port: u16,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

impl Provider for UdpProvider {
    fn type_(&self) -> ProviderType {
        ProviderType::Udp
    }

    fn name(&self) -> String {
        self.base.name.clone()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl UdpProvider {
    /// Create a UDP provider with host/port and optional timeout.
    pub fn new(name: String, host: String, port: u16, auth: Option<AuthConfig>) -> Self {
        Self {
            base: BaseProvider {
                name,
                provider_type: ProviderType::Udp,
                auth,
                allowed_communication_protocols: None,
            },
            host,
            port,
            timeout_ms: Some(30_000),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn udp_provider_defaults_timeout_to_none_when_not_provided() {
        let json = json!({
            "name": "test-udp",
            "provider_type": "udp",
            "host": "127.0.0.1",
            "port": 8081
        });

        let provider: UdpProvider = serde_json::from_value(json).unwrap();
        assert_eq!(provider.base.name, "test-udp");
        assert_eq!(provider.host, "127.0.0.1");
        assert_eq!(provider.port, 8081);
        assert_eq!(provider.timeout_ms, None);
    }

    #[test]
    fn udp_provider_respects_configured_timeout() {
        let json = json!({
            "name": "test-udp-timeout",
            "provider_type": "udp",
            "host": "localhost",
            "port": 9001,
            "timeout_ms": 7000
        });

        let provider: UdpProvider = serde_json::from_value(json).unwrap();
        assert_eq!(provider.timeout_ms, Some(7000));
    }

    #[test]
    fn udp_provider_new_sets_default_timeout() {
        let provider = UdpProvider::new("new-udp".to_string(), "localhost".to_string(), 53, None);

        assert_eq!(provider.base.provider_type, ProviderType::Udp);
        assert_eq!(provider.timeout_ms, Some(30_000));
    }
}
