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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_http_provider_deserialization() {
        let json = json!({
            "name": "test-http",
            "provider_type": "http",
            "url": "http://example.com",
            "http_method": "POST"
        });

        let provider: HttpProvider = serde_json::from_value(json).unwrap();
        assert_eq!(provider.base.name, "test-http");
        assert_eq!(provider.url, "http://example.com");
        assert_eq!(provider.http_method, "POST");
        assert!(provider.content_type.is_none()); // Defaults are handled in new(), but serde doesn't use new() unless default impl exists
    }

    #[test]
    fn test_http_provider_full_config() {
        let json = json!({
            "name": "test-http-full",
            "provider_type": "http",
            "url": "http://example.com/api",
            "http_method": "GET",
            "content_type": "application/xml",
            "headers": {
                "X-Custom": "value"
            }
        });

        let provider: HttpProvider = serde_json::from_value(json).unwrap();
        assert_eq!(provider.content_type.as_deref(), Some("application/xml"));
        assert_eq!(provider.headers.unwrap().get("X-Custom").map(|s| s.as_str()), Some("value"));
    }
}
