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
