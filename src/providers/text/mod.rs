use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::auth::AuthConfig;
use crate::providers::base::{BaseProvider, Provider, ProviderType};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextProvider {
    #[serde(flatten)]
    pub base: BaseProvider,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_path: Option<PathBuf>,
}

impl Provider for TextProvider {
    fn type_(&self) -> ProviderType {
        ProviderType::Text
    }

    fn name(&self) -> String {
        self.base.name.clone()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl TextProvider {
    pub fn new(name: String, base_path: Option<PathBuf>, auth: Option<AuthConfig>) -> Self {
        Self {
            base: BaseProvider {
                name,
                provider_type: ProviderType::Text,
                auth,
            },
            base_path,
        }
    }
}
