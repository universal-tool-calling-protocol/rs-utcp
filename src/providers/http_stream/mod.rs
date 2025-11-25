use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::auth::AuthConfig;
use crate::providers::base::{BaseProvider, Provider, ProviderType};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamableHttpProvider {
    #[serde(flatten)]
    pub base: BaseProvider,
    pub url: String,
    #[serde(default = "StreamableHttpProvider::default_method")]
    pub http_method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<HashMap<String, String>>,
}

impl Provider for StreamableHttpProvider {
    fn type_(&self) -> ProviderType {
        ProviderType::HttpStream
    }

    fn name(&self) -> String {
        self.base.name.clone()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl StreamableHttpProvider {
    pub fn new(name: String, url: String, auth: Option<AuthConfig>) -> Self {
        Self {
            base: BaseProvider {
                name,
                provider_type: ProviderType::HttpStream,
                auth,
            },
            url,
            http_method: Self::default_method(),
            headers: None,
        }
    }

    fn default_method() -> String {
        "POST".to_string()
    }
}
