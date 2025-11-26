use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::auth::AuthConfig;
use crate::providers::base::{BaseProvider, Provider, ProviderType};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphqlProvider {
    #[serde(flatten)]
    pub base: BaseProvider,
    pub url: String,
    #[serde(default = "GraphqlProvider::default_operation")]
    pub operation_type: String, // query | mutation | subscription
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<HashMap<String, String>>,
}

impl Provider for GraphqlProvider {
    fn type_(&self) -> ProviderType {
        ProviderType::Graphql
    }

    fn name(&self) -> String {
        self.base.name.clone()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl GraphqlProvider {
    pub fn new(name: String, url: String, auth: Option<AuthConfig>) -> Self {
        Self {
            base: BaseProvider {
                name,
                provider_type: ProviderType::Graphql,
                auth,
            },
            url,
            operation_type: Self::default_operation(),
            operation_name: None,
            headers: None,
        }
    }

    fn default_operation() -> String {
        "query".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn graphql_provider_defaults_to_query() {
        let json = json!({
            "name": "test-graphql",
            "provider_type": "graphql",
            "url": "http://localhost:4000/graphql"
        });

        let provider: GraphqlProvider = serde_json::from_value(json).unwrap();
        assert_eq!(provider.base.name, "test-graphql");
        assert_eq!(provider.operation_type, "query");
        assert!(provider.operation_name.is_none());
        assert!(provider.headers.is_none());
    }

    #[test]
    fn graphql_provider_accepts_custom_operation_and_headers() {
        let json = json!({
            "name": "test-graphql-full",
            "provider_type": "graphql",
            "url": "https://example.com/graphql",
            "operation_type": "mutation",
            "operation_name": "CreateUser",
            "headers": {
                "Authorization": "Bearer token"
            }
        });

        let provider: GraphqlProvider = serde_json::from_value(json).unwrap();
        assert_eq!(provider.operation_type, "mutation");
        assert_eq!(provider.operation_name.as_deref(), Some("CreateUser"));
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
    fn graphql_provider_new_sets_defaults() {
        let provider =
            GraphqlProvider::new("new-graphql".to_string(), "https://example.com/graphql".to_string(), None);

        assert_eq!(provider.base.provider_type, ProviderType::Graphql);
        assert_eq!(provider.operation_type, "query");
        assert!(provider.operation_name.is_none());
    }
}
