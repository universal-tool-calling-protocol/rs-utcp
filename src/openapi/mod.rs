use anyhow::{anyhow, Result};
use reqwest::Url;
use serde_json::{Map, Value};
use std::collections::HashMap;

use crate::auth::{ApiKeyAuth, AuthConfig, AuthType, BasicAuth, OAuth2Auth};
use crate::providers::base::{BaseProvider, ProviderType};
use crate::providers::http::HttpProvider;
use crate::tools::{Tool, ToolInputOutputSchema};

pub const VERSION: &str = "1.0";

#[derive(Debug, Clone)]
pub struct UtcpManual {
    pub version: String,
    pub tools: Vec<Tool>,
}

pub struct OpenApiConverter {
    spec: Value,
    spec_url: Option<String>,
    provider_name: String,
}

impl OpenApiConverter {
    pub fn new(
        openapi_spec: Value,
        spec_url: Option<String>,
        provider_name: Option<String>,
    ) -> Self {
        let provider_name = provider_name
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| derive_provider_name(&openapi_spec));

        Self {
            spec: openapi_spec,
            spec_url,
            provider_name,
        }
    }

    pub async fn new_from_url(spec_url: &str, provider_name: Option<String>) -> Result<Self> {
        let (spec, final_url) = load_spec_from_url(spec_url).await?;
        Ok(Self::new(spec, Some(final_url), provider_name))
    }

    pub fn convert(&self) -> UtcpManual {
        let mut tools = Vec::new();
        let base_url = self.base_url();

        if let Some(paths) = self.spec.get("paths").and_then(|v| v.as_object()) {
            for (raw_path, raw_item) in paths {
                if let Some(path_item) = raw_item.as_object() {
                    for (method, raw_op) in path_item {
                        let lower = method.to_ascii_lowercase();
                        if !matches!(lower.as_str(), "get" | "post" | "put" | "delete" | "patch") {
                            continue;
                        }

                        if let Some(op) = raw_op.as_object() {
                            if let Ok(Some(tool)) =
                                self.create_tool(raw_path, &lower, op, &base_url)
                            {
                                tools.push(tool);
                            }
                        }
                    }
                }
            }
        }

        UtcpManual {
            version: VERSION.to_string(),
            tools,
        }
    }

    fn base_url(&self) -> String {
        if let Some(servers) = self.spec.get("servers").and_then(|v| v.as_array()) {
            if let Some(first) = servers.first().and_then(|v| v.as_object()) {
                if let Some(url) = first.get("url").and_then(|v| v.as_str()) {
                    if !url.is_empty() {
                        return url.to_string();
                    }
                }
            }
        }

        if let Some(host) = self.spec.get("host").and_then(|v| v.as_str()) {
            let scheme = self
                .spec
                .get("schemes")
                .and_then(|v| v.as_array())
                .and_then(|arr| arr.first())
                .and_then(|v| v.as_str())
                .unwrap_or("https");
            let base_path = self
                .spec
                .get("basePath")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            return format!("{}://{}{}", scheme, host, base_path);
        }

        if let Some(spec_url) = &self.spec_url {
            if let Ok(parsed) = Url::parse(spec_url) {
                if let Some(host) = parsed.host_str() {
                    return format!("{}://{}", parsed.scheme(), host);
                }
            }
        }

        "/".to_string()
    }

    fn resolve_ref(&self, reference: &str) -> Result<Value> {
        if !reference.starts_with("#/") {
            return Err(anyhow!("only local refs supported, got {}", reference));
        }
        let pointer = &reference[1..];
        self.spec
            .pointer(pointer)
            .cloned()
            .ok_or_else(|| anyhow!("ref {} not found", reference))
    }

    fn resolve_schema(&self, schema: Value) -> Value {
        match schema {
            Value::Object(map) => {
                if let Some(Value::String(reference)) = map.get("$ref").cloned() {
                    if let Ok(resolved) = self.resolve_ref(&reference) {
                        return self.resolve_schema(resolved);
                    }
                    return Value::Object(map);
                }

                let mut out = Map::new();
                for (k, v) in map {
                    out.insert(k, self.resolve_schema(v));
                }
                Value::Object(out)
            }
            Value::Array(arr) => Value::Array(
                arr.into_iter()
                    .map(|item| self.resolve_schema(item))
                    .collect(),
            ),
            other => other,
        }
    }

    fn extract_auth(&self, operation: &Map<String, Value>) -> Option<AuthConfig> {
        let mut reqs = Vec::new();
        if let Some(op_sec) = operation.get("security").and_then(|v| v.as_array()) {
            if !op_sec.is_empty() {
                reqs = op_sec.clone();
            }
        }
        if reqs.is_empty() {
            if let Some(global) = self.spec.get("security").and_then(|v| v.as_array()) {
                reqs = global.clone();
            }
        }
        if reqs.is_empty() {
            return None;
        }

        let schemes = self.get_security_schemes().unwrap_or_default();
        for raw in reqs {
            if let Some(sec_map) = raw.as_object() {
                for name in sec_map.keys() {
                    if let Some(Value::Object(scheme)) = schemes.get(name) {
                        if let Some(auth) = self.create_auth_from_scheme(scheme) {
                            return Some(auth);
                        }
                    }
                }
            }
        }
        None
    }

    fn get_security_schemes(&self) -> Option<Map<String, Value>> {
        if let Some(components) = self.spec.get("components").and_then(|v| v.as_object()) {
            if let Some(security_schemes) = components
                .get("securitySchemes")
                .and_then(|v| v.as_object())
            {
                return Some(security_schemes.clone());
            }
        }
        if let Some(defs) = self
            .spec
            .get("securityDefinitions")
            .and_then(|v| v.as_object())
        {
            return Some(defs.clone());
        }
        None
    }

    fn create_auth_from_scheme(&self, scheme: &Map<String, Value>) -> Option<AuthConfig> {
        let typ = scheme
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_ascii_lowercase();

        match typ.as_str() {
            "apikey" => {
                let location = scheme
                    .get("in")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let name = scheme
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if location.is_empty() || name.is_empty() {
                    return None;
                }
                let auth = ApiKeyAuth {
                    auth_type: AuthType::ApiKey,
                    api_key: format!("${{{}_API_KEY}}", self.provider_name.to_uppercase()),
                    var_name: name,
                    location,
                };
                Some(AuthConfig::ApiKey(auth))
            }
            "basic" => {
                let auth = BasicAuth {
                    auth_type: AuthType::Basic,
                    username: format!("${{{}_USERNAME}}", self.provider_name.to_uppercase()),
                    password: format!("${{{}_PASSWORD}}", self.provider_name.to_uppercase()),
                };
                Some(AuthConfig::Basic(auth))
            }
            "http" => {
                let scheme_name = scheme
                    .get("scheme")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_ascii_lowercase();
                match scheme_name.as_str() {
                    "basic" => {
                        let auth = BasicAuth {
                            auth_type: AuthType::Basic,
                            username: format!(
                                "${{{}_USERNAME}}",
                                self.provider_name.to_uppercase()
                            ),
                            password: format!(
                                "${{{}_PASSWORD}}",
                                self.provider_name.to_uppercase()
                            ),
                        };
                        Some(AuthConfig::Basic(auth))
                    }
                    "bearer" => {
                        let auth = ApiKeyAuth {
                            auth_type: AuthType::ApiKey,
                            api_key: format!(
                                "Bearer ${{{}_API_KEY}}",
                                self.provider_name.to_uppercase()
                            ),
                            var_name: "Authorization".to_string(),
                            location: "header".to_string(),
                        };
                        Some(AuthConfig::ApiKey(auth))
                    }
                    _ => None,
                }
            }
            "oauth2" => {
                if let Some(flows) = scheme.get("flows").and_then(|v| v.as_object()) {
                    for raw_flow in flows.values() {
                        if let Some(flow) = raw_flow.as_object() {
                            if let Some(token_url) = flow.get("tokenUrl").and_then(|v| v.as_str()) {
                                let scope = flow
                                    .get("scopes")
                                    .and_then(|v| v.as_object())
                                    .map(|m| m.keys().cloned().collect::<Vec<_>>().join(" "));
                                let auth = OAuth2Auth {
                                    auth_type: AuthType::OAuth2,
                                    token_url: token_url.to_string(),
                                    client_id: format!(
                                        "${{{}_CLIENT_ID}}",
                                        self.provider_name.to_uppercase()
                                    ),
                                    client_secret: format!(
                                        "${{{}_CLIENT_SECRET}}",
                                        self.provider_name.to_uppercase()
                                    ),
                                    scope: optional_string(scope.unwrap_or_default()),
                                };
                                return Some(AuthConfig::OAuth2(auth));
                            }
                        }
                    }
                }

                if let Some(token_url) = scheme.get("tokenUrl").and_then(|v| v.as_str()) {
                    let scope = scheme
                        .get("scopes")
                        .and_then(|v| v.as_object())
                        .map(|m| m.keys().cloned().collect::<Vec<_>>().join(" "));
                    let auth = OAuth2Auth {
                        auth_type: AuthType::OAuth2,
                        token_url: token_url.to_string(),
                        client_id: format!("${{{}_CLIENT_ID}}", self.provider_name.to_uppercase()),
                        client_secret: format!(
                            "${{{}_CLIENT_SECRET}}",
                            self.provider_name.to_uppercase()
                        ),
                        scope: optional_string(scope.unwrap_or_default()),
                    };
                    return Some(AuthConfig::OAuth2(auth));
                }

                None
            }
            _ => None,
        }
    }

    fn create_tool(
        &self,
        path: &str,
        method: &str,
        op: &Map<String, Value>,
        base_url: &str,
    ) -> Result<Option<Tool>> {
        let op_id = op
            .get("operationId")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| {
                let sanitized_path = path.trim_matches('/').replace('/', "_");
                format!("{}_{}", method.to_ascii_lowercase(), sanitized_path)
            });

        let description = op
            .get("summary")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .or_else(|| {
                op.get("description")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            })
            .unwrap_or_default();

        let tags = op
            .get("tags")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let (input_schema, headers, body_field) = self.extract_inputs(op);
        let output_schema = self.extract_outputs(op);
        let auth = self.extract_auth(op);

        let provider = HttpProvider {
            base: BaseProvider {
                name: self.provider_name.clone(),
                provider_type: ProviderType::Http,
                auth,
            },
            http_method: method.to_ascii_uppercase(),
            url: join_url(base_url, path),
            content_type: Some("application/json".to_string()),
            headers: None,
            body_field,
            header_fields: if headers.is_empty() {
                None
            } else {
                Some(headers)
            },
        };

        let provider_value = serde_json::to_value(provider)?;
        Ok(Some(Tool {
            name: op_id,
            description,
            inputs: input_schema,
            outputs: output_schema,
            tags,
            average_response_size: None,
            provider: Some(provider_value),
        }))
    }

    fn extract_inputs(
        &self,
        op: &Map<String, Value>,
    ) -> (ToolInputOutputSchema, Vec<String>, Option<String>) {
        let mut props: HashMap<String, Value> = HashMap::new();
        let mut required: Vec<String> = Vec::new();
        let mut headers = Vec::new();
        let mut body_field: Option<String> = None;

        if let Some(parameters) = op.get("parameters").and_then(|v| v.as_array()) {
            for raw_param in parameters {
                let param = self.resolve_schema(raw_param.clone());
                if let Some(param_obj) = param.as_object() {
                    let name = param_obj
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let location = param_obj
                        .get("in")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    if name.is_empty() {
                        continue;
                    }
                    if location == "header" {
                        headers.push(name.clone());
                    }
                    if location == "body" {
                        body_field = Some(name.clone());
                    }

                    let schema_val = param_obj
                        .get("schema")
                        .cloned()
                        .unwrap_or_else(|| Value::Object(Map::new()));
                    let schema_obj = self.resolve_schema(schema_val);
                    let schema_map = schema_obj.as_object().cloned().unwrap_or_default();
                    let mut entry = Map::new();

                    if let Some(desc) = param_obj.get("description") {
                        entry.insert("description".to_string(), desc.clone());
                    }
                    if let Some(typ) = schema_map.get("type").or_else(|| param_obj.get("type")) {
                        entry.insert("type".to_string(), typ.clone());
                    }
                    for (k, v) in schema_map {
                        entry.insert(k, v);
                    }
                    if !entry.contains_key("type") {
                        entry.insert("type".to_string(), Value::String("object".to_string()));
                    }

                    props.insert(name.clone(), Value::Object(entry));
                    if param_obj
                        .get("required")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false)
                    {
                        required.push(name);
                    }
                }
            }
        }

        if let Some(request_body) = op.get("requestBody") {
            let rb = self.resolve_schema(request_body.clone());
            if let Some(rb_obj) = rb.as_object() {
                if let Some(content) = rb_obj.get("content").and_then(|v| v.as_object()) {
                    if let Some(app_json) =
                        content.get("application/json").and_then(|v| v.as_object())
                    {
                        if let Some(schema) = app_json.get("schema") {
                            let name = "body".to_string();
                            body_field = Some(name.clone());
                            let schema_obj = self.resolve_schema(schema.clone());
                            let schema_map = schema_obj.as_object().cloned().unwrap_or_default();
                            let mut entry = Map::new();
                            if let Some(desc) = rb_obj.get("description") {
                                entry.insert("description".to_string(), desc.clone());
                            }
                            for (k, v) in schema_map {
                                entry.insert(k, v);
                            }
                            if !entry.contains_key("type") {
                                entry.insert(
                                    "type".to_string(),
                                    Value::String("object".to_string()),
                                );
                            }
                            props.insert(name.clone(), Value::Object(entry));
                            if rb_obj
                                .get("required")
                                .and_then(|v| v.as_bool())
                                .unwrap_or(false)
                            {
                                required.push(name);
                            }
                        }
                    }
                }
            }
        }

        let schema = ToolInputOutputSchema {
            type_: "object".to_string(),
            properties: if props.is_empty() { None } else { Some(props) },
            required: if required.is_empty() {
                None
            } else {
                Some(required)
            },
            description: None,
            title: None,
            items: None,
            enum_: None,
            minimum: None,
            maximum: None,
            format: None,
        };

        (schema, headers, body_field)
    }

    fn extract_outputs(&self, op: &Map<String, Value>) -> ToolInputOutputSchema {
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

        let responses = match op.get("responses").and_then(|v| v.as_object()) {
            Some(r) => r,
            None => return default_schema,
        };
        let resp = match responses
            .get("200")
            .or_else(|| responses.get("201"))
            .cloned()
        {
            Some(r) => r,
            None => return default_schema,
        };

        let resp = self.resolve_schema(resp);
        if let Some(resp_obj) = resp.as_object() {
            if let Some(content) = resp_obj.get("content").and_then(|v| v.as_object()) {
                if let Some(app_json) = content.get("application/json").and_then(|v| v.as_object())
                {
                    if let Some(schema) = app_json.get("schema") {
                        let fallback = resp_obj
                            .get("description")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());
                        return self.build_schema_from_value(schema, fallback);
                    }
                }
            }
            if let Some(schema) = resp_obj.get("schema") {
                let fallback = resp_obj
                    .get("description")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                return self.build_schema_from_value(schema, fallback);
            }
        }

        default_schema
    }

    fn build_schema_from_value(
        &self,
        schema: &Value,
        fallback_description: Option<String>,
    ) -> ToolInputOutputSchema {
        let resolved = self.resolve_schema(schema.clone());
        let map = resolved.as_object().cloned().unwrap_or_default();

        let mut out = ToolInputOutputSchema {
            type_: map
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("object")
                .to_string(),
            properties: map_from_value(map.get("properties")),
            required: string_slice(map.get("required")),
            description: match map
                .get("description")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
            {
                Some(desc) if !desc.is_empty() => Some(desc),
                _ => fallback_description.filter(|s| !s.is_empty()),
            },
            title: map
                .get("title")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            items: None,
            enum_: map.get("enum").and_then(|v| interface_slice(v)),
            minimum: cast_float(map.get("minimum")),
            maximum: cast_float(map.get("maximum")),
            format: map
                .get("format")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
        };

        if out.type_ == "array" {
            out.items = map_from_value(map.get("items"));
        }

        out
    }
}

pub async fn load_spec_from_url(raw_url: &str) -> Result<(Value, String)> {
    let resp = reqwest::get(raw_url).await?;
    let status = resp.status();
    if !status.is_success() {
        return Err(anyhow!("unexpected HTTP status: {}", status));
    }

    let final_url = resp.url().to_string();
    let bytes = resp.bytes().await?;

    if let Ok(json_spec) = serde_json::from_slice::<Value>(&bytes) {
        return Ok((json_spec, final_url));
    }

    let yaml_value: serde_yaml::Value = serde_yaml::from_slice(&bytes)
        .map_err(|err| anyhow!("failed to parse as JSON or YAML: {}", err))?;
    let json_value = serde_json::to_value(yaml_value)?;
    Ok((json_value, final_url))
}

fn derive_provider_name(spec: &Value) -> String {
    let title = spec
        .get("info")
        .and_then(|v| v.as_object())
        .and_then(|info| info.get("title"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let base = if title.is_empty() {
        "openapi_provider".to_string()
    } else {
        title
    };
    let invalid = " -.,!?'\"\\\\/()[]{}#@$%^&*+=~`|;:<>";

    let mut output = String::new();
    for ch in base.chars() {
        if invalid.contains(ch) {
            output.push('_');
        } else {
            output.push(ch);
        }
    }
    if output.is_empty() {
        "openapi_provider".to_string()
    } else {
        output
    }
}

fn optional_string(s: String) -> Option<String> {
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

fn join_url(base: &str, path: &str) -> String {
    let trimmed_base = base.trim_end_matches('/');
    let trimmed_path = path.trim_start_matches('/');
    if trimmed_base.is_empty() {
        format!("/{}", trimmed_path)
    } else if trimmed_path.is_empty() {
        trimmed_base.to_string()
    } else {
        format!("{}/{}", trimmed_base, trimmed_path)
    }
}

fn map_from_value(value: Option<&Value>) -> Option<HashMap<String, Value>> {
    value.and_then(|v| v.as_object()).map(|obj| {
        obj.iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect::<HashMap<_, _>>()
    })
}

fn string_slice(value: Option<&Value>) -> Option<Vec<String>> {
    value.and_then(|v| v.as_array()).map(|arr| {
        let collected = arr
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect::<Vec<_>>();
        if collected.is_empty() {
            None
        } else {
            Some(collected)
        }
    })?
}

fn interface_slice(value: &Value) -> Option<Vec<Value>> {
    value.as_array().map(|arr| {
        if arr.is_empty() {
            None
        } else {
            Some(arr.to_vec())
        }
    })?
}

fn cast_float(value: Option<&Value>) -> Option<f64> {
    value.and_then(|v| {
        if let Some(n) = v.as_f64() {
            Some(n)
        } else if let Some(i) = v.as_i64() {
            Some(i as f64)
        } else {
            None
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn build_test_converter() -> OpenApiConverter {
        let spec = json!({
            "info": {"title": "Test"},
            "components": {
                "schemas": {
                    "Obj": {
                        "type": "object",
                        "properties": { "name": { "type": "string" } }
                    }
                },
                "securitySchemes": {
                    "apiKey": { "type": "apiKey", "name": "X-Token", "in": "header" },
                    "basicAuth": { "type": "http", "scheme": "basic" }
                }
            },
            "security": [ { "apiKey": [] } ]
        });

        OpenApiConverter::new(
            spec,
            Some("https://api.example.com/spec.json".to_string()),
            Some("test".to_string()),
        )
    }

    #[test]
    fn resolve_ref_and_schema() {
        let converter = build_test_converter();
        let obj = converter.resolve_ref("#/components/schemas/Obj").unwrap();
        assert_eq!(obj.get("type").and_then(|v| v.as_str()), Some("object"));
        assert!(converter.resolve_ref("#/bad/ref").is_err());

        let resolved = converter.resolve_schema(json!({"$ref": "#/components/schemas/Obj"}));
        assert_eq!(
            resolved
                .get("properties")
                .and_then(|v| v.get("name"))
                .and_then(|v| v.get("type"))
                .and_then(|v| v.as_str()),
            Some("string")
        );
    }

    #[test]
    fn create_auth_from_scheme_and_extract() {
        let converter = build_test_converter();
        let api_key = converter
            .create_auth_from_scheme(
                &json!({"type": "apiKey", "in": "header", "name": "X"})
                    .as_object()
                    .unwrap(),
            )
            .unwrap();
        match api_key {
            AuthConfig::ApiKey(auth) => {
                assert_eq!(auth.var_name, "X");
                assert_eq!(auth.location, "header");
                assert_eq!(auth.api_key, "${TEST_API_KEY}");
            }
            _ => panic!("expected ApiKey auth"),
        }

        let basic = converter
            .create_auth_from_scheme(
                &json!({"type": "http", "scheme": "basic"})
                    .as_object()
                    .unwrap(),
            )
            .unwrap();
        matches!(basic, AuthConfig::Basic(_));

        let bearer = converter
            .create_auth_from_scheme(
                &json!({"type": "http", "scheme": "bearer"})
                    .as_object()
                    .unwrap(),
            )
            .unwrap();
        match bearer {
            AuthConfig::ApiKey(auth) => {
                assert_eq!(auth.var_name, "Authorization");
                assert!(auth.api_key.contains("${TEST_API_KEY}"));
            }
            _ => panic!("expected bearer api key auth"),
        }

        let mut op = Map::new();
        op.insert("security".to_string(), json!([{"basicAuth": []}]));
        let auth = converter.extract_auth(&op).unwrap();
        matches!(auth, AuthConfig::Basic(_));
    }

    #[test]
    fn inputs_outputs_and_create_tool() {
        let converter = build_test_converter();
        let op_value = json!({
            "operationId": "ping",
            "summary": "Ping",
            "tags": ["t"],
            "parameters": [
                { "name": "id", "in": "query", "required": true, "schema": { "type": "string" }},
                { "name": "X", "in": "header", "schema": { "type": "string" }}
            ],
            "requestBody": {
                "required": true,
                "content": {
                    "application/json": {
                        "schema": {
                            "type": "object",
                            "properties": { "foo": { "type": "string" } }
                        }
                    }
                }
            },
            "responses": {
                "200": {
                    "content": {
                        "application/json": {
                            "schema": {
                                "type": "object",
                                "description": "desc",
                                "properties": { "ok": { "type": "boolean" } }
                            }
                        }
                    }
                }
            }
        });
        let op = op_value.as_object().unwrap().clone();

        let (schema, headers, body) = converter.extract_inputs(&op);
        assert_eq!(schema.properties.as_ref().map(|m| m.len()), Some(3));
        assert_eq!(headers, vec!["X".to_string()]);
        assert_eq!(body.as_deref(), Some("body"));

        let out = converter.extract_outputs(&op);
        assert_eq!(out.type_, "object");
        assert!(out.properties.unwrap().contains_key("ok"));

        let tool = converter
            .create_tool("/ping", "get", &op, "https://api.example.com")
            .unwrap()
            .unwrap();
        assert_eq!(tool.name, "ping");
        let prov: HttpProvider = serde_json::from_value(tool.provider.unwrap()).unwrap();
        assert_eq!(prov.url, "https://api.example.com/ping");
    }

    #[test]
    fn convert_basic() {
        let spec = json!({
            "info": {"title": "Test API"},
            "servers": [{"url": "https://api.example.com"}],
            "paths": {
                "/ping": {
                    "get": {
                        "operationId": "ping",
                        "summary": "Ping",
                        "responses": {
                            "200": {
                                "content": {
                                    "application/json": {
                                        "schema": { "type": "object" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        });
        let converter = OpenApiConverter::new(spec, None, None);
        let manual = converter.convert();
        assert_eq!(manual.version, VERSION);
        assert_eq!(manual.tools.len(), 1);
        assert_eq!(manual.tools[0].name, "ping");
    }
}
