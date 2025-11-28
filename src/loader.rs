// Provider loading from JSON files
use anyhow::{anyhow, Result};
use serde_json::Value;
use std::path::Path;
use std::sync::Arc;

use crate::call_templates;
use crate::config::UtcpClientConfig;
use crate::migration::{migrate_v01_config, validate_v1_config, validate_v1_manual};
use crate::providers::base::Provider;
use crate::providers::cli::CliProvider;
use crate::providers::graphql::GraphqlProvider;
use crate::providers::grpc::GrpcProvider;
use crate::providers::http::HttpProvider;
use crate::providers::http_stream::StreamableHttpProvider;
use crate::providers::mcp::McpProvider;
use crate::providers::sse::SseProvider;
use crate::providers::tcp::TcpProvider;
use crate::providers::text::TextProvider;
use crate::providers::udp::UdpProvider;
use crate::providers::webrtc::WebRtcProvider;
use crate::providers::websocket::WebSocketProvider;
use crate::spec::ManualV1;

/// Parse a providers JSON file
/// Supports multiple formats:
/// - Array: [{"provider_type": "http", ...}, ...]
/// - Object with providers array: {"providers": [{...}, ...]}
/// - Object with single provider: {"providers": {...}}
/// - Single provider: {"provider_type": "http", ...}
pub async fn load_providers_from_file(
    path: impl AsRef<Path>,
    config: &UtcpClientConfig,
) -> Result<Vec<Arc<dyn Provider>>> {
    Ok(load_providers_with_tools_from_file(path, config)
        .await?
        .into_iter()
        .map(|p| p.provider)
        .collect())
}

/// LoadedProvider represents a provider that has been loaded from a configuration file,
/// optionally including a list of tools if they were defined in the file (e.g. in a manual).
pub struct LoadedProvider {
    /// The loaded provider instance.
    pub provider: Arc<dyn Provider>,
    /// Optional list of tools associated with this provider.
    pub tools: Option<Vec<crate::tools::Tool>>,
}

/// Load providers or manuals (v0.1 or v1.0), returning providers and any embedded tools.
pub async fn load_providers_with_tools_from_file(
    path: impl AsRef<Path>,
    config: &UtcpClientConfig,
) -> Result<Vec<LoadedProvider>> {
    let contents = tokio::fs::read_to_string(path).await?;
    let json_raw: Value = serde_json::from_str(&contents)?;
    // Apply v0.1 -> v1.0 migration for configs if needed
    let json = migrate_v01_config(&json_raw);

    // Validate v1.0 shapes when applicable
    if let Some(obj) = json.as_object() {
        if obj.contains_key("manual_call_templates") {
            validate_v1_config(&json)?;
        }
        if obj.contains_key("tools") {
            validate_v1_manual(&json)?;
        }
    }

    // If this is a manual with tools, collect tools per provider
    if let Some(obj) = json.as_object() {
        if obj.get("tools").is_some() {
            let _manual: ManualV1 = serde_json::from_value(json.clone())
                .map_err(|e| anyhow!("Invalid v1.0 manual: {}", e))?;

            let (providers, tools) = parse_manual_tools_with_providers(json.clone(), config)?;
            return Ok(providers
                .into_iter()
                .zip(tools.into_iter())
                .map(|(provider, tools)| LoadedProvider {
                    provider,
                    tools: Some(tools),
                })
                .collect());
        }
    }

    let provider_values = parse_providers_json(json)?;

    let mut providers = Vec::new();
    for (index, mut provider_value) in provider_values.into_iter().enumerate() {
        // Perform variable substitution
        substitute_variables(&mut provider_value, config);

        // Create provider
        let provider = create_provider_from_value(provider_value, index)?;
        providers.push(LoadedProvider {
            provider,
            tools: None,
        });
    }

    Ok(providers)
}

/// Parses the raw JSON value into a list of provider JSON objects.
/// Handles various formats: array, object with "providers", object with "manual_call_templates", or single provider object.
fn parse_providers_json(json: Value) -> Result<Vec<Value>> {
    match json {
        // Direct array of providers
        Value::Array(arr) => Ok(arr),

        // Object that might contain providers
        Value::Object(obj) => {
            if obj.get("tools").is_some() {
                return parse_manual_tools(Value::Object(obj));
            }

            // v1.0 migration: manual_call_templates -> providers
            if let Some(templates_value) = obj.get("manual_call_templates") {
                if let Some(arr) = templates_value.as_array() {
                    let mut providers = Vec::new();
                    for template in arr {
                        providers
                            .push(call_templates::call_template_to_provider(template.clone())?);
                    }
                    return Ok(providers);
                }
            }

            if let Some(providers_value) = obj.get("providers") {
                match providers_value {
                    // providers is an array
                    Value::Array(arr) => Ok(arr.clone()),
                    // providers is a single object
                    Value::Object(_) => Ok(vec![providers_value.clone()]),
                    _ => Err(anyhow!("'providers' field must be an array or object")),
                }
            } else {
                // Single provider object (no "providers" wrapper)
                Ok(vec![Value::Object(obj)])
            }
        }

        _ => Err(anyhow!("JSON root must be array or object")),
    }
}

/// Parse a manual (v0.1 or v1.0) into a list of providers by lifting tool_call_templates.
fn parse_manual_tools(json: Value) -> Result<Vec<Value>> {
    let obj = json
        .as_object()
        .ok_or_else(|| anyhow!("Manual must be an object"))?;
    let tools = obj
        .get("tools")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow!("Manual missing tools array"))?;

    let mut providers = Vec::new();
    for tool in tools {
        if let Some(provider) = tool_to_provider(tool)? {
            providers.push(provider);
        }
    }
    Ok(providers)
}

/// Parses a manual JSON object to extract both providers and their associated tools.
/// Returns a tuple of (providers, tools_per_provider).
fn parse_manual_tools_with_providers(
    json: Value,
    config: &UtcpClientConfig,
) -> Result<(Vec<Arc<dyn Provider>>, Vec<Vec<crate::tools::Tool>>)> {
    let obj = json
        .as_object()
        .ok_or_else(|| anyhow!("Manual must be an object"))?;
    let tools = obj
        .get("tools")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow!("Manual missing tools array"))?;

    let mut providers = Vec::new();
    let mut tools_per_provider = Vec::new();

    for (idx, tool_val) in tools.iter().enumerate() {
        if let Some(provider_val) = tool_to_provider(tool_val)? {
            let mut provider_val = provider_val.clone();
            substitute_variables(&mut provider_val, config);
            // If missing provider_type, derive from call_template_type
            let obj = provider_val
                .as_object_mut()
                .ok_or_else(|| anyhow!("Provider must be object"))?;
            if obj
                .get("provider_type")
                .or_else(|| obj.get("type"))
                .is_none()
            {
                if let Some(ct) = obj.get("call_template_type").cloned() {
                    obj.insert("provider_type".to_string(), ct.clone());
                    obj.insert("type".to_string(), ct);
                } else {
                    obj.insert(
                        "provider_type".to_string(),
                        Value::String("http".to_string()),
                    );
                    obj.insert("type".to_string(), Value::String("http".to_string()));
                }
            }
            let provider = create_provider_from_value(provider_val, idx)?;
            let prov_name = provider.name();
            providers.push(provider);

            let mut tool_value = tool_val.clone();
            if let Some(obj) = tool_value.as_object_mut() {
                obj.entry("tags")
                    .or_insert_with(|| Value::Array(Vec::new()));
            }
            let mut tool: crate::tools::Tool = serde_json::from_value(tool_value)?;
            // Prefix tool name with provider to keep existing naming
            if !tool.name.starts_with(&format!("{}.", prov_name)) {
                tool.name = format!("{}.{}", prov_name, tool.name);
            }
            tools_per_provider.push(vec![tool]);
        }
    }

    Ok((providers, tools_per_provider))
}

/// Extracts a provider definition from a tool JSON object.
/// Checks for "tool_call_template" or "provider" fields.
fn tool_to_provider(tool: &Value) -> Result<Option<Value>> {
    let tool_obj = tool
        .as_object()
        .ok_or_else(|| anyhow!("Tool must be an object"))?;

    if let Some(tmpl) = tool_obj.get("tool_call_template") {
        Ok(Some(call_templates::call_template_to_provider(
            tmpl.clone(),
        )?))
    } else if let Some(prov) = tool_obj.get("provider") {
        Ok(Some(call_templates::call_template_to_provider(
            prov.clone(),
        )?))
    } else {
        Ok(None)
    }
}

/// Creates a Provider instance from a JSON value.
/// Handles type normalization and defaults.
fn create_provider_from_value(mut value: Value, index: usize) -> Result<Arc<dyn Provider>> {
    // Normalize type field: accept both "type" and "provider_type"
    let provider_type = {
        let obj = value
            .as_object_mut()
            .ok_or_else(|| anyhow!("Provider must be an object"))?;

        if obj.get("provider_type").is_none() && obj.get("type").is_none() {
            obj.insert(
                "provider_type".to_string(),
                Value::String("http".to_string()),
            );
            obj.insert("type".to_string(), Value::String("http".to_string()));
        }

        let ptype = obj
            .get("provider_type")
            .or_else(|| obj.get("type"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Missing provider_type field"))?
            .to_string();

        // Ensure both fields exist
        obj.insert("type".to_string(), Value::String(ptype.clone()));
        obj.insert("provider_type".to_string(), Value::String(ptype.clone()));

        // Ensure name field exists
        if !obj.contains_key("name") {
            obj.insert(
                "name".to_string(),
                Value::String(format!("{}_{}", ptype, index)),
            );
        }

        ptype
    };

    // Create provider based on type
    match provider_type.as_str() {
        "http" => {
            if !value
                .get("http_method")
                .or_else(|| value.get("method"))
                .is_some()
            {
                if let Some(obj) = value.as_object_mut() {
                    obj.insert("http_method".to_string(), Value::String("GET".to_string()));
                }
            }
            if !value.get("url").is_some() {
                if let Some(obj) = value.as_object_mut() {
                    obj.insert(
                        "url".to_string(),
                        Value::String("http://localhost".to_string()),
                    );
                }
            }
            let provider: HttpProvider = serde_json::from_value(value)?;
            Ok(Arc::new(provider))
        }
        "cli" => {
            let provider: CliProvider = serde_json::from_value(value)?;
            Ok(Arc::new(provider))
        }
        "sse" => {
            let provider: SseProvider = serde_json::from_value(value)?;
            Ok(Arc::new(provider))
        }
        "websocket" => {
            let provider: WebSocketProvider = serde_json::from_value(value)?;
            Ok(Arc::new(provider))
        }
        "grpc" => {
            let provider: GrpcProvider = serde_json::from_value(value)?;
            Ok(Arc::new(provider))
        }
        "graphql" => {
            let provider: GraphqlProvider = serde_json::from_value(value)?;
            Ok(Arc::new(provider))
        }
        "tcp" => {
            let provider: TcpProvider = serde_json::from_value(value)?;
            Ok(Arc::new(provider))
        }
        "udp" => {
            let provider: UdpProvider = serde_json::from_value(value)?;
            Ok(Arc::new(provider))
        }
        "http_stream" => {
            let provider: StreamableHttpProvider = serde_json::from_value(value)?;
            Ok(Arc::new(provider))
        }
        "mcp" => {
            let provider: McpProvider = serde_json::from_value(value)?;
            Ok(Arc::new(provider))
        }
        "webrtc" => {
            let provider: WebRtcProvider = serde_json::from_value(value)?;
            Ok(Arc::new(provider))
        }
        "text" => {
            let provider: TextProvider = serde_json::from_value(value)?;
            Ok(Arc::new(provider))
        }
        // Add more provider types as they are implemented
        _ => Err(anyhow!("Unsupported provider type: {}", provider_type)),
    }
}

/// Substitutes variables in the JSON value using the provided configuration.
/// Replaces ${VAR} and $VAR with values from config or environment.
fn substitute_variables(value: &mut Value, config: &UtcpClientConfig) {
    match value {
        Value::String(s) => {
            // Replace ${VAR} or $VAR patterns
            let mut result = s.clone();

            // Replace from config variables
            for (key, val) in &config.variables {
                result = result.replace(&format!("${{{}}}", key), val);
                result = result.replace(&format!("${}", key), val);
            }

            // Also check environment variables for remaining variables
            if result.contains('$') {
                for key in config.variables.keys() {
                    if let Ok(env_val) = std::env::var(key) {
                        result = result.replace(&format!("${}", key), &env_val);
                        result = result.replace(&format!("${{{}}}", key), &env_val);
                    }
                }
            }

            *s = result;
        }
        Value::Object(obj) => {
            for (_, v) in obj.iter_mut() {
                substitute_variables(v, config);
            }
        }
        Value::Array(arr) => {
            for item in arr.iter_mut() {
                substitute_variables(item, config);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_parse_providers_array() {
        let json = serde_json::json!([
            {"provider_type": "http", "url": "http://example.com"},
            {"provider_type": "cli", "command_name": "ls"}
        ]);

        let result = parse_providers_json(json).unwrap();
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_parse_providers_object_with_array() {
        let json = serde_json::json!({
            "providers": [
                {"provider_type": "http", "url": "http://example.com"}
            ]
        });

        let result = parse_providers_json(json).unwrap();
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_parse_single_provider() {
        let json = serde_json::json!({
            "provider_type": "http",
            "url": "http://example.com"
        });

        let result = parse_providers_json(json).unwrap();
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_parse_manual_call_templates_converts_to_providers() {
        let json = serde_json::json!({
            "manual_call_templates": [
                {
                    "name": "weather_service",
                    "call_template_type": "http",
                    "url": "http://example.com",
                    "http_method": "GET"
                },
                {
                    "name": "cli_tool",
                    "call_template_type": "cli",
                    "command": "echo hi",
                    "working_dir": "/tmp"
                }
            ]
        });

        let result = parse_providers_json(json).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(
            result[0]
                .get("provider_type")
                .and_then(|v| v.as_str())
                .unwrap(),
            "http"
        );
        assert_eq!(
            result[1]
                .get("command_name")
                .and_then(|v| v.as_str())
                .unwrap(),
            "echo hi"
        );
    }

    #[tokio::test]
    async fn load_providers_supports_multiple_types() {
        let mut file = NamedTempFile::new().unwrap();
        write!(
            file,
            r#"{{
                "providers": [
                    {{ "provider_type": "sse", "name": "events", "url": "http://example.com/sse" }},
                    {{ "provider_type": "http_stream", "name": "stream", "url": "http://example.com/stream" }}
                ]
            }}"#
        )
        .unwrap();

        let config = UtcpClientConfig::default();
        let providers = load_providers_from_file(file.path(), &config)
            .await
            .unwrap();
        assert_eq!(providers.len(), 2);
        assert_eq!(
            providers[0].type_(),
            crate::providers::base::ProviderType::Sse
        );
        assert_eq!(
            providers[1].type_(),
            crate::providers::base::ProviderType::HttpStream
        );
    }

    #[tokio::test]
    async fn load_manual_with_tools_returns_tools() {
        let mut file = NamedTempFile::new().unwrap();
        write!(
            file,
            r#"{{
                "manual_version": "1.0.0",
                "utcp_version": "0.2.0",
                "info": {{ "title": "demo", "version": "1.0.0" }},
                "tools": [
                    {{
                        "name": "echo",
                        "description": "Echo",
                        "inputs": {{ "type": "object" }},
                        "outputs": {{ "type": "object" }},
                        "tool_call_template": {{
                            "call_template_type": "http",
                            "name": "http_tool",
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
        assert_eq!(loaded.len(), 1);
        assert!(loaded[0].tools.as_ref().unwrap()[0]
            .name
            .starts_with(&loaded[0].provider.name()));
    }
}
