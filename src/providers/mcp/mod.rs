use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::auth::AuthConfig;
use crate::providers::base::{BaseProvider, Provider, ProviderType};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpProvider {
    #[serde(flatten)]
    pub base: BaseProvider,
    // HTTP transport fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<HashMap<String, String>>,
    // Stdio transport fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env_vars: Option<HashMap<String, String>>,
}

impl Provider for McpProvider {
    fn type_(&self) -> ProviderType {
        ProviderType::Mcp
    }

    fn name(&self) -> String {
        self.base.name.clone()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl McpProvider {
    // Create HTTP-based MCP provider
    pub fn new(name: String, url: String, auth: Option<AuthConfig>) -> Self {
        Self {
            base: BaseProvider {
                name,
                provider_type: ProviderType::Mcp,
                auth,
            },
            url: Some(url),
            headers: None,
            command: None,
            args: None,
            env_vars: None,
        }
    }

    // Create stdio-based MCP provider
    pub fn new_stdio(
        name: String,
        command: String,
        args: Option<Vec<String>>,
        env_vars: Option<HashMap<String, String>>,
    ) -> Self {
        Self {
            base: BaseProvider {
                name,
                provider_type: ProviderType::Mcp,
                auth: None,
            },
            url: None,
            headers: None,
            command: Some(command),
            args,
            env_vars,
        }
    }

    pub fn is_stdio(&self) -> bool {
        self.command.is_some()
    }

    pub fn is_http(&self) -> bool {
        self.url.is_some()
    }
}
