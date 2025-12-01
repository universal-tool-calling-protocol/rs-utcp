use crate::config::UtcpClientConfig;
use crate::loader::load_providers_with_tools_from_file;
use crate::providers::base::{BaseProvider, Provider, ProviderType};
use crate::repository::in_memory::InMemoryToolRepository;
use crate::tools::{Tool, ToolInputOutputSchema, ToolSearchStrategy};
use crate::{UtcpClient, UtcpClientInterface};
use anyhow::Result;
use async_trait::async_trait;
use std::io::Write;
use std::sync::Arc;
use tempfile::NamedTempFile;

struct MockSearchStrategy;

#[async_trait]
impl ToolSearchStrategy for MockSearchStrategy {
    async fn search_tools(&self, _query: &str, _limit: usize) -> Result<Vec<Tool>> {
        Ok(vec![])
    }
}

#[tokio::test]
async fn test_manual_default_protocol_restriction() {
    // Manual without allowed_communication_protocols should only register tools matching the tool's own protocol
    let mut file = NamedTempFile::new().unwrap();
    write!(
        file,
        r#"{{
            "manual_version": "1.0.0",
            "utcp_version": "0.2.0",
            "info": {{ "title": "Test Manual", "version": "1.0.0" }},
            "tools": [
                {{
                    "name": "http_tool",
                    "description": "HTTP tool",
                    "inputs": {{ "type": "object" }},
                    "outputs": {{ "type": "object" }},
                    "tool_call_template": {{
                        "call_template_type": "http",
                        "name": "http_provider",
                        "url": "http://example.com",
                        "http_method": "GET"
                    }}
                }}
            ]
        }}"#
    )
    .unwrap();

    let config = UtcpClientConfig::default();
    let loaded = load_providers_with_tools_from_file(file.path(), &config)
        .await
        .unwrap();

    // Should register the HTTP tool since it matches its own protocol
    assert_eq!(loaded.len(), 1);
    assert!(loaded[0].tools.is_some());
    assert_eq!(loaded[0].tools.as_ref().unwrap().len(), 1);

    // Verify provider has default allowed protocols (only http)
    let allowed = loaded[0].provider.allowed_protocols();
    assert_eq!(allowed, vec!["http".to_string()]);
}

#[tokio::test]
async fn test_manual_explicit_multi_protocol_allowlist() {
    // Manual with allowed_communication_protocols should register tools matching allowed protocols
    let mut file = NamedTempFile::new().unwrap();
    write!(
        file,
        r#"{{
            "manual_version": "1.0.0",
            "utcp_version": "0.2.0",
            "info": {{ "title": "Multi Protocol Manual", "version": "1.0.0" }},
            "allowed_communication_protocols": ["http", "cli"],
            "tools": [
                {{
                    "name": "http_tool",
                    "description": "HTTP tool",
                    "inputs": {{ "type": "object" }},
                    "outputs": {{ "type": "object" }},
                    "tool_call_template": {{
                        "call_template_type": "http",
                        "name": "http_provider",
                        "url": "http://example.com",
                        "http_method": "GET"
                    }}
                }},
                {{
                    "name": "cli_tool",
                    "description": "CLI tool",
                    "inputs": {{ "type": "object" }},
                    "outputs": {{ "type": "object" }},
                    "tool_call_template": {{
                        "call_template_type": "cli",
                        "name": "cli_provider",
                        "command": "echo hello"
                    }}
                }}
            ]
        }}"#
    )
    .unwrap();

    let config = UtcpClientConfig::default();
    let loaded = load_providers_with_tools_from_file(file.path(), &config)
        .await
        .unwrap();

    // Should register both HTTP and CLI tools
    assert_eq!(loaded.len(), 2);

    // Verify both providers were created with correct types
    let types: Vec<_> = loaded.iter().map(|l| l.provider.type_()).collect();
    assert!(types.contains(&ProviderType::Http));
    assert!(types.contains(&ProviderType::Cli));
}

#[tokio::test]
async fn test_manual_filters_disallowed_protocols() {
    // Manual should filter out tools with disallowed protocols
    let mut file = NamedTempFile::new().unwrap();
    write!(
        file,
        r#"{{
            "manual_version": "1.0.0",
            "utcp_version": "0.2.0",
            "info": {{ "title": "Filtered Manual", "version": "1.0.0" }},
            "allowed_communication_protocols": ["http"],
            "tools": [
                {{
                    "name": "http_tool",
                    "description": "HTTP tool",
                    "inputs": {{ "type": "object" }},
                    "outputs": {{ "type": "object" }},
                    "tool_call_template": {{
                        "call_template_type": "http",
                        "name": "http_provider",
                        "url": "http://example.com",
                        "http_method": "GET"
                    }}
                }},
                {{
                    "name": "cli_tool",
                    "description": "CLI tool",
                    "inputs": {{ "type": "object" }},
                    "outputs": {{ "type": "object" }},
                    "tool_call_template": {{
                        "call_template_type": "cli",
                        "name": "cli_provider",
                        "command": "echo hello"
                    }}
                }}
            ]
        }}"#
    )
    .unwrap();

    let config = UtcpClientConfig::default();
    let loaded = load_providers_with_tools_from_file(file.path(), &config)
        .await
        .unwrap();

    // Should only register HTTP tool, CLI tool should be filtered out
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].provider.type_(), ProviderType::Http);
}

#[tokio::test]
async fn test_empty_array_defaults_to_own_protocol() {
    // Empty allowed_communication_protocols array should behave same as undefined
    let mut file = NamedTempFile::new().unwrap();
    write!(
        file,
        r#"{{
            "manual_version": "1.0.0",
            "utcp_version": "0.2.0",
            "info": {{ "title": "Empty Array Manual", "version": "1.0.0" }},
            "allowed_communication_protocols": [],
            "tools": [
                {{
                    "name": "http_tool",
                    "description": "HTTP tool",
                    "inputs": {{ "type": "object" }},
                    "outputs": {{ "type": "object" }},
                    "tool_call_template": {{
                        "call_template_type": "http",
                        "name": "http_provider",
                        "url": "http://example.com",
                        "http_method": "GET"
                    }}
                }}
            ]
        }}"#
    )
    .unwrap();

    let config = UtcpClientConfig::default();
    let loaded = load_providers_with_tools_from_file(file.path(), &config)
        .await
        .unwrap();

    // With empty array, should not filter (behaves as if undefined)
    // Actually based on our implementation, empty is treated as "not set"
    assert_eq!(loaded.len(), 1);
}

#[tokio::test]
async fn test_provider_allowed_protocols_method() {
    // Test that Provider trait's allowed_protocols method works correctly
    let provider_with_allowed = BaseProvider {
        name: "test".to_string(),
        provider_type: ProviderType::Http,
        auth: None,
        allowed_communication_protocols: Some(vec!["http".to_string(), "cli".to_string()]),
    };

    let allowed = provider_with_allowed.allowed_protocols();
    assert_eq!(allowed, vec!["http".to_string(), "cli".to_string()]);

    let provider_without_allowed = BaseProvider {
        name: "test2".to_string(),
        provider_type: ProviderType::Cli,
        auth: None,
        allowed_communication_protocols: None,
    };

    let default_allowed = provider_without_allowed.allowed_protocols();
    assert_eq!(default_allowed, vec!["cli".to_string()]);

    let provider_empty_allowed = BaseProvider {
        name: "test3".to_string(),
        provider_type: ProviderType::Tcp,
        auth: None,
        allowed_communication_protocols: Some(vec![]),
    };

    let empty_allowed = provider_empty_allowed.allowed_protocols();
    assert_eq!(empty_allowed, vec!["tcp".to_string()]);
}

#[tokio::test]
async fn test_call_tool_validates_allowed_protocols() {
    // Test that call_tool checks allowed protocols
    let config = UtcpClientConfig::default();
    let repo = Arc::new(InMemoryToolRepository::new());
    let strategy = Arc::new(MockSearchStrategy);

    let client = UtcpClient::new(config, repo.clone(), strategy)
        .await
        .unwrap();

    // Create a provider with restricted protocols
    let provider = Arc::new(BaseProvider {
        name: "test_provider".to_string(),
        provider_type: ProviderType::Http,
        auth: None,
        allowed_communication_protocols: Some(vec!["cli".to_string()]), // Only allow CLI, but this is HTTP
    });

    let default_schema = ToolInputOutputSchema {
        type_: "object".to_string(),
        properties: None,
        required: None,
        description: None,
        title: None,
        items: None,
        enum_: None,
        minimum: None,
        maximum: None,
        format: None,
    };

    let tool = Tool {
        name: "test_provider.test_tool".to_string(),
        description: "Test".to_string(),
        inputs: default_schema.clone(),
        outputs: default_schema,
        tags: vec![],
        average_response_size: None,
        provider: None,
    };

    // Register the provider with tools
    let result = client
        .register_tool_provider_with_tools(provider.clone(), vec![tool])
        .await;

    // Registration should succeed
    assert!(result.is_ok());

    // But calling the tool should fail due to protocol mismatch
    let call_result = client
        .call_tool("test_provider.test_tool", std::collections::HashMap::new())
        .await;

    assert!(call_result.is_err());
    let err_msg = call_result.unwrap_err().to_string();
    assert!(err_msg.contains("not allowed"));
    assert!(err_msg.contains("http"));
    assert!(err_msg.contains("cli"));
}
