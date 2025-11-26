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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn deserializes_cli_provider_with_minimal_config() {
        let json = json!({
            "name": "test-cli",
            "provider_type": "cli",
            "command_name": "echo"
        });

        let provider: CliProvider = serde_json::from_value(json).unwrap();
        assert_eq!(provider.base.name, "test-cli");
        assert_eq!(provider.command_name, "echo");
        assert!(provider.working_dir.is_none());
        assert!(provider.env_vars.is_none());
        assert_eq!(provider.type_(), ProviderType::Cli);
    }

    #[test]
    fn cli_provider_new_sets_defaults() {
        let provider = CliProvider::new("builder".to_string(), "make".to_string(), None);

        assert_eq!(provider.base.name, "builder");
        assert_eq!(provider.base.provider_type, ProviderType::Cli);
        assert_eq!(provider.command_name, "make");
        assert!(provider.working_dir.is_none());
        assert!(provider.env_vars.is_none());
    }
}
