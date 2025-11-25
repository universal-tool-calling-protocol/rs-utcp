// Provider loading from JSON files
use anyhow::{anyhow, Result};
use serde_json::Value;
use std::path::Path;
use std::sync::Arc;

use crate::config::UtcpClientConfig;
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
    let contents = tokio::fs::read_to_string(path).await?;
    let json: Value = serde_json::from_str(&contents)?;

    let provider_values = parse_providers_json(json)?;

    let mut providers = Vec::new();
    for (index, mut provider_value) in provider_values.into_iter().enumerate() {
        // Perform variable substitution
        substitute_variables(&mut provider_value, config);

        // Create provider
        let provider = create_provider_from_value(provider_value, index)?;
        providers.push(provider);
    }

    Ok(providers)
}

fn parse_providers_json(json: Value) -> Result<Vec<Value>> {
    match json {
        // Direct array of providers
        Value::Array(arr) => Ok(arr),

        // Object that might contain providers
        Value::Object(obj) => {
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

fn create_provider_from_value(mut value: Value, index: usize) -> Result<Arc<dyn Provider>> {
    // Normalize type field: accept both "type" and "provider_type"
    let provider_type = {
        let obj = value
            .as_object_mut()
            .ok_or_else(|| anyhow!("Provider must be an object"))?;

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
}
