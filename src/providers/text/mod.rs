use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::auth::AuthConfig;
use crate::providers::base::{BaseProvider, Provider, ProviderType};

/// Provider definition for file-backed text tools.
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
    /// Create a text provider with an optional base directory for scripts and manifests.
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::Path;

    #[test]
    fn text_provider_deserializes_without_base_path() {
        let json = json!({
            "name": "test-text",
            "provider_type": "text"
        });

        let provider: TextProvider = serde_json::from_value(json).unwrap();
        assert_eq!(provider.base.name, "test-text");
        assert!(provider.base_path.is_none());
        assert_eq!(provider.base.provider_type, ProviderType::Text);
    }

    #[test]
    fn text_provider_deserializes_with_base_path() {
        let json = json!({
            "name": "test-text-path",
            "provider_type": "text",
            "base_path": "/tmp/tools"
        });

        let provider: TextProvider = serde_json::from_value(json).unwrap();
        assert_eq!(provider.base_path.as_deref(), Some(Path::new("/tmp/tools")));
    }

    #[test]
    fn text_provider_new_sets_fields() {
        let provider = TextProvider::new("new-text".to_string(), Some("/opt/text".into()), None);

        assert_eq!(provider.base.name, "new-text");
        assert_eq!(provider.base.provider_type, ProviderType::Text);
        assert_eq!(provider.base_path.as_deref(), Some(Path::new("/opt/text")));
    }
}
