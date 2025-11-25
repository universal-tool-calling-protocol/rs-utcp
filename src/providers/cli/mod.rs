use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::auth::AuthConfig;
use crate::providers::base::{BaseProvider, Provider, ProviderType};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliProvider {
    #[serde(flatten)]
    pub base: BaseProvider,
    pub command_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env_vars: Option<HashMap<String, String>>,
}

impl Provider for CliProvider {
    fn type_(&self) -> ProviderType {
        ProviderType::Cli
    }

    fn name(&self) -> String {
        self.base.name.clone()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl CliProvider {
    pub fn new(name: String, command_name: String, auth: Option<AuthConfig>) -> Self {
        Self {
            base: BaseProvider {
                name,
                provider_type: ProviderType::Cli,
                auth,
            },
            command_name,
            working_dir: None,
            env_vars: None,
        }
    }
}
