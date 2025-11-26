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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn sse_provider_deserializes_minimal_config() {
        let json = json!({
            "name": "test-sse",
            "provider_type": "sse",
            "url": "https://example.com/sse"
        });

        let provider: SseProvider = serde_json::from_value(json).unwrap();
        assert_eq!(provider.base.name, "test-sse");
        assert_eq!(provider.url, "https://example.com/sse");
        assert!(provider.headers.is_none());
        assert!(provider.body_field.is_none());
        assert!(provider.header_fields.is_none());
    }

    #[test]
    fn sse_provider_accepts_headers_and_fields() {
        let json = json!({
            "name": "test-sse-full",
            "provider_type": "sse",
            "url": "https://example.com/stream",
            "headers": {
                "Authorization": "Bearer token"
            },
            "body_field": "data",
            "header_fields": ["Content-Type", "X-Trace-Id"]
        });

        let provider: SseProvider = serde_json::from_value(json).unwrap();
        assert_eq!(
            provider
                .headers
                .unwrap()
                .get("Authorization")
                .map(|s| s.as_str()),
            Some("Bearer token")
        );
        assert_eq!(provider.body_field.as_deref(), Some("data"));
        assert_eq!(
            provider.header_fields.as_ref().unwrap(),
            &vec!["Content-Type".to_string(), "X-Trace-Id".to_string()]
        );
    }

    #[test]
    fn sse_provider_new_sets_defaults() {
        let provider = SseProvider::new("new-sse".to_string(), "http://localhost:8080/sse".to_string(), None);

        assert_eq!(provider.base.provider_type, ProviderType::Sse);
        assert!(provider.headers.is_none());
        assert!(provider.body_field.is_none());
        assert!(provider.header_fields.is_none());
    }
}
