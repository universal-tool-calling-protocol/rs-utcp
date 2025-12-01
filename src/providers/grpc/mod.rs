use serde::{Deserialize, Serialize};

use crate::auth::AuthConfig;
use crate::providers::base::{BaseProvider, Provider, ProviderType};

/// Provider definition for gRPC services.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrpcProvider {
    #[serde(flatten)]
    pub base: BaseProvider,
    pub host: String,
    pub port: u16,
    #[serde(default)]
    pub use_ssl: bool,
}

impl Provider for GrpcProvider {
    fn type_(&self) -> ProviderType {
        ProviderType::Grpc
    }

    fn name(&self) -> String {
        self.base.name.clone()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl GrpcProvider {
    /// Construct a gRPC provider with host/port and optional auth.
    pub fn new(name: String, host: String, port: u16, auth: Option<AuthConfig>) -> Self {
        Self {
            base: BaseProvider {
                name,
                provider_type: ProviderType::Grpc,
                auth,
                allowed_communication_protocols: None,
            },
            host,
            port,
            use_ssl: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn grpc_provider_defaults_use_ssl_to_false() {
        let json = json!({
            "name": "test-grpc",
            "provider_type": "grpc",
            "host": "localhost",
            "port": 50051
        });

        let provider: GrpcProvider = serde_json::from_value(json).unwrap();
        assert_eq!(provider.base.name, "test-grpc");
        assert_eq!(provider.host, "localhost");
        assert_eq!(provider.port, 50051);
        assert!(!provider.use_ssl);
    }

    #[test]
    fn grpc_provider_allows_ssl_flag() {
        let json = json!({
            "name": "secure-grpc",
            "provider_type": "grpc",
            "host": "example.com",
            "port": 443,
            "use_ssl": true
        });

        let provider: GrpcProvider = serde_json::from_value(json).unwrap();
        assert!(provider.use_ssl);
        assert_eq!(provider.host, "example.com");
        assert_eq!(provider.port, 443);
    }

    #[test]
    fn grpc_provider_new_sets_defaults() {
        let provider =
            GrpcProvider::new("new-grpc".to_string(), "localhost".to_string(), 1234, None);

        assert_eq!(provider.base.provider_type, ProviderType::Grpc);
        assert_eq!(provider.host, "localhost");
        assert_eq!(provider.port, 1234);
        assert!(!provider.use_ssl);
    }
}
