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
