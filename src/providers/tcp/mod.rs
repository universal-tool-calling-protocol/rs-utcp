use serde::{Deserialize, Serialize};

use crate::auth::AuthConfig;
use crate::providers::base::{BaseProvider, Provider, ProviderType};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TcpProvider {
    #[serde(flatten)]
    pub base: BaseProvider,
    pub host: String,
    pub port: u16,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

impl Provider for TcpProvider {
    fn type_(&self) -> ProviderType {
        ProviderType::Tcp
    }

    fn name(&self) -> String {
        self.base.name.clone()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl TcpProvider {
    pub fn new(name: String, host: String, port: u16, auth: Option<AuthConfig>) -> Self {
        Self {
            base: BaseProvider {
                name,
                provider_type: ProviderType::Tcp,
                auth,
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
    fn tcp_provider_defaults_timeout_to_none_when_not_provided() {
        let json = json!({
            "name": "test-tcp",
            "provider_type": "tcp",
            "host": "127.0.0.1",
            "port": 8080
        });

        let provider: TcpProvider = serde_json::from_value(json).unwrap();
        assert_eq!(provider.base.name, "test-tcp");
        assert_eq!(provider.host, "127.0.0.1");
        assert_eq!(provider.port, 8080);
        assert_eq!(provider.timeout_ms, None);
    }

    #[test]
    fn tcp_provider_respects_configured_timeout() {
        let json = json!({
            "name": "test-tcp-timeout",
            "provider_type": "tcp",
            "host": "localhost",
            "port": 9000,
            "timeout_ms": 5000
        });

        let provider: TcpProvider = serde_json::from_value(json).unwrap();
        assert_eq!(provider.timeout_ms, Some(5000));
    }

    #[test]
    fn tcp_provider_new_sets_default_timeout() {
        let provider = TcpProvider::new("new-tcp".to_string(), "localhost".to_string(), 80, None);

        assert_eq!(provider.base.provider_type, ProviderType::Tcp);
        assert_eq!(provider.timeout_ms, Some(30_000));
    }
}
