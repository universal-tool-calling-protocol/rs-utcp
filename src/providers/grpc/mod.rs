use serde::{Deserialize, Serialize};

use crate::auth::AuthConfig;
use crate::providers::base::{BaseProvider, Provider, ProviderType};

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
    pub fn new(name: String, host: String, port: u16, auth: Option<AuthConfig>) -> Self {
        Self {
            base: BaseProvider {
                name,
                provider_type: ProviderType::Grpc,
                auth,
            },
            host,
            port,
            use_ssl: false,
        }
    }
}
