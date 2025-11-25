use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::auth::AuthConfig;
use crate::providers::base::{BaseProvider, Provider, ProviderType};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpProvider {
    #[serde(flatten)]
    pub base: BaseProvider,
    pub http_method: String,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_field: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub header_fields: Option<Vec<String>>,
}

impl Provider for HttpProvider {
    fn type_(&self) -> ProviderType {
        ProviderType::Http
    }

    fn name(&self) -> String {
        self.base.name.clone()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl HttpProvider {
    pub fn new(name: String, url: String, http_method: String, auth: Option<AuthConfig>) -> Self {
        Self {
            base: BaseProvider {
                name,
                provider_type: ProviderType::Http,
                auth,
            },
            http_method,
            url,
            content_type: Some("application/json".to_string()),
            headers: None,
            body_field: None,
            header_fields: None,
        }
    }
}
