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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_mcp_provider_http_deserialization() {
        let json = json!({
            "name": "test-mcp-http",
            "provider_type": "mcp",
            "url": "http://localhost:3000/mcp"
        });

        let provider: McpProvider = serde_json::from_value(json).unwrap();
        assert_eq!(provider.base.name, "test-mcp-http");
        assert_eq!(provider.url.as_deref(), Some("http://localhost:3000/mcp"));
        assert!(provider.is_http());
        assert!(!provider.is_stdio());
    }

    #[test]
    fn test_mcp_provider_stdio_deserialization() {
        let json = json!({
            "name": "test-mcp-stdio",
            "provider_type": "mcp",
            "command": "python",
            "args": ["server.py"],
            "env_vars": {
                "DEBUG": "1"
            }
        });

        let provider: McpProvider = serde_json::from_value(json).unwrap();
        assert_eq!(provider.base.name, "test-mcp-stdio");
        assert_eq!(provider.command.as_deref(), Some("python"));
        assert_eq!(provider.args.as_ref().unwrap()[0], "server.py");
        assert_eq!(provider.env_vars.as_ref().unwrap().get("DEBUG").map(|s| s.as_str()), Some("1"));
        assert!(provider.is_stdio());
        assert!(!provider.is_http());
    }
}
