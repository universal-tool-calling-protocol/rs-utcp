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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn streamable_http_provider_defaults_to_post() {
        let json = json!({
            "name": "test-http-stream",
            "provider_type": "http_stream",
            "url": "https://example.com/stream"
        });

        let provider: StreamableHttpProvider = serde_json::from_value(json).unwrap();
        assert_eq!(provider.base.name, "test-http-stream");
        assert_eq!(provider.http_method, "POST");
        assert!(provider.headers.is_none());
    }

    #[test]
    fn streamable_http_provider_accepts_method_and_headers() {
        let json = json!({
            "name": "test-http-stream-full",
            "provider_type": "http_stream",
            "url": "https://example.com/stream",
            "http_method": "PUT",
            "headers": {
                "Authorization": "Bearer token"
            }
        });

        let provider: StreamableHttpProvider = serde_json::from_value(json).unwrap();
        assert_eq!(provider.http_method, "PUT");
        assert_eq!(
            provider
                .headers
                .unwrap()
                .get("Authorization")
                .map(|s| s.as_str()),
            Some("Bearer token")
        );
    }

    #[test]
    fn streamable_http_provider_new_sets_defaults() {
        let provider =
            StreamableHttpProvider::new("new-http-stream".to_string(), "https://example.com".to_string(), None);

        assert_eq!(provider.base.provider_type, ProviderType::HttpStream);
        assert_eq!(provider.http_method, "POST");
        assert!(provider.headers.is_none());
    }
}
