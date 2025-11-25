use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::auth::AuthConfig;
use crate::providers::base::{BaseProvider, Provider, ProviderType};

/// Provider definition for Server-Sent Events endpoints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SseProvider {
    #[serde(flatten)]
    pub base: BaseProvider,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_field: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub header_fields: Option<Vec<String>>,
}

impl Provider for SseProvider {
    fn type_(&self) -> ProviderType {
        ProviderType::Sse
    }

    fn name(&self) -> String {
        self.base.name.clone()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl SseProvider {
    pub fn new(name: String, url: String, auth: Option<AuthConfig>) -> Self {
        Self {
            base: BaseProvider {
                name,
                provider_type: ProviderType::Sse,
                auth,
            },
            url,
            headers: None,
            body_field: None,
            header_fields: None,
        }
    }
}
