use serde_json::{json, Map, Value};
use anyhow::{anyhow, Result};

/// Best-effort migration of a v0.1 configuration object to the v1.0 shape.
/// - providers -> manual_call_templates
/// - provider_type -> call_template_type
/// - carries over variables/load_variables_from if present
pub fn migrate_v01_config(config: &Value) -> Value {
    // If already v1.0-ish object, start with a clone to avoid dropping keys.
    let mut out = match config {
        Value::Object(obj) => obj.clone(),
        _ => Map::new(),
    };

    // Copy through variables and loaders if they exist
    if let Some(vars) = config.get("variables") {
        out.insert("variables".to_string(), vars.clone());
    }
    if let Some(loaders) = config.get("load_variables_from") {
        out.insert("load_variables_from".to_string(), loaders.clone());
    }

    let mut templates = Vec::new();
    if let Some(providers) = config.get("providers") {
        match providers {
            Value::Array(arr) => {
                for prov in arr {
                    if let Some(tmpl) = provider_to_call_template(prov) {
                        templates.push(tmpl);
                    }
                }
            }
            Value::Object(_) => {
                if let Some(tmpl) = provider_to_call_template(providers) {
                    templates.push(tmpl);
                }
            }
            _ => {}
        }
    }

    if !templates.is_empty() {
        out.insert("manual_call_templates".to_string(), Value::Array(templates));
    }

    if out.is_empty() {
        config.clone()
    } else {
        Value::Object(out)
    }
}

/// Best-effort migration of a v0.1 manual to v1.0 structure.
/// - Adds manual_version/utcp_version/info
/// - Moves parameters -> inputs, sets a default outputs schema
/// - Moves provider -> tool_call_template (provider_type -> call_template_type)
pub fn migrate_v01_manual(manual: &Value) -> Value {
    let mut out = Map::new();
    out.insert("manual_version".to_string(), Value::String("1.0.0".to_string()));
    out.insert("utcp_version".to_string(), Value::String("0.2.0".to_string()));

    // Info block from provider_info if present
    if let Some(info) = manual.get("provider_info") {
        let mut info_map = Map::new();
        if let Some(name) = info.get("name") {
            info_map.insert("title".to_string(), name.clone());
        }
        if let Some(version) = info.get("version") {
            info_map.insert("version".to_string(), version.clone());
        }
        if let Some(desc) = info.get("description") {
            info_map.insert("description".to_string(), desc.clone());
        }
        out.insert("info".to_string(), Value::Object(info_map));
    }

    // Tools
    if let Some(tools) = manual.get("tools").and_then(|t| t.as_array()) {
        let mut migrated_tools = Vec::new();
        for tool in tools {
            if let Some(mut tool_obj) = tool.as_object().cloned() {
                // parameters -> inputs
                if let Some(params) = tool_obj.remove("parameters") {
                    tool_obj.insert("inputs".to_string(), params);
                }
                // default outputs to object if missing
                tool_obj.entry("outputs".to_string()).or_insert_with(|| {
                    json!({"type": "object"})
                });

                // provider -> tool_call_template
                if let Some(provider) = tool_obj.remove("provider") {
                    if let Some(mut tmpl_obj) = provider.as_object().cloned() {
                        if let Some(ptype) = tmpl_obj
                            .get("provider_type")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string())
                        {
                            tmpl_obj.insert("call_template_type".to_string(), Value::String(ptype));
                        }
                        tool_obj.insert(
                            "tool_call_template".to_string(),
                            Value::Object(tmpl_obj),
                        );
                    }
                }

                migrated_tools.push(Value::Object(tool_obj));
            }
        }
        out.insert("tools".to_string(), Value::Array(migrated_tools));
    }

    Value::Object(out)
}

pub fn provider_to_call_template(provider: &Value) -> Option<Value> {
    let mut obj = provider.as_object()?.clone();
    let ptype = obj
        .get("provider_type")
        .or_else(|| obj.get("type"))
        .and_then(|v| v.as_str())
        .unwrap_or("http")
        .to_string();
    obj.insert(
        "call_template_type".to_string(),
        Value::String(ptype.clone()),
    );
    // Normalize fields that differ between v0.1 providers and v1.0 templates
    if ptype == "http" && obj.get("http_method").is_none() {
        if let Some(method) = obj.get("method").cloned() {
            obj.insert("http_method".to_string(), method);
        }
    }
    Some(Value::Object(obj))
}

/// Convert a call template into a provider representation for backward compatibility.
pub fn call_template_to_provider(template: &Value) -> Option<Value> {
    provider_to_call_template(template)
}

/// Basic validation for a v1.0 config. Ensures manual_call_templates exist when no providers.
pub fn validate_v1_config(config: &Value) -> Result<()> {
    let obj = config
        .as_object()
        .ok_or_else(|| anyhow!("config must be an object"))?;

    if obj.get("manual_call_templates").is_none() && obj.get("providers").is_none() {
        return Err(anyhow!(
            "config must include manual_call_templates (v1.0) or providers (legacy)"
        ));
    }
    Ok(())
}

/// Basic validation for a v1.0 manual: requires manual_version/utcp_version, tools array,
/// and each tool must have name/description/inputs/outputs and a tool_call_template or provider.
pub fn validate_v1_manual(manual: &Value) -> Result<()> {
    let obj = manual
        .as_object()
        .ok_or_else(|| anyhow!("manual must be an object"))?;
    if !obj.contains_key("manual_version") {
        return Err(anyhow!("manual_version is required"));
    }
    if !obj.contains_key("utcp_version") {
        return Err(anyhow!("utcp_version is required"));
    }

    let tools = obj
        .get("tools")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow!("manual.tools must be an array"))?;

    for tool in tools {
        let t = tool
            .as_object()
            .ok_or_else(|| anyhow!("tool must be an object"))?;
        if !t.contains_key("name") {
            return Err(anyhow!("tool missing name"));
        }
        if !t.contains_key("description") {
            return Err(anyhow!("tool missing description"));
        }
        if !t.contains_key("inputs") {
            return Err(anyhow!("tool missing inputs schema"));
        }
        if !t.contains_key("outputs") {
            return Err(anyhow!("tool missing outputs schema"));
        }
        if !(t.contains_key("tool_call_template") || t.contains_key("provider")) {
            return Err(anyhow!(
                "tool missing tool_call_template (v1.0) or provider (legacy)"
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrate_config_providers_to_call_templates() {
        let config = json!({
            "providers": [
                { "provider_type": "http", "url": "http://example.com", "http_method": "GET" },
                { "provider_type": "cli", "command_name": "echo hi" }
            ],
            "variables": { "API_KEY": "x" }
        });

        let migrated = migrate_v01_config(&config);
        assert!(migrated.get("manual_call_templates").is_some());
        let templates = migrated
            .get("manual_call_templates")
            .and_then(|v| v.as_array())
            .unwrap();
        assert_eq!(
            templates[0]
                .get("call_template_type")
                .and_then(|v| v.as_str())
                .unwrap(),
            "http"
        );
        assert_eq!(
            templates[1]
                .get("call_template_type")
                .and_then(|v| v.as_str())
                .unwrap(),
            "cli"
        );
        assert_eq!(
            templates[1]
                .get("command_name")
                .and_then(|v| v.as_str())
                .unwrap(),
            "echo hi"
        );
    }

    #[test]
    fn migrate_manual_sets_versions_and_templates() {
        let manual = json!({
            "utcp_version": "0.1.0",
            "provider_info": { "name": "Weather", "version": "1.0.0" },
            "tools": [{
                "name": "get_weather",
                "description": "Get weather data",
                "parameters": { "type": "object" },
                "provider": { "provider_type": "http", "url": "http://api" }
            }]
        });

        let migrated = migrate_v01_manual(&manual);
        assert_eq!(migrated.get("manual_version").unwrap(), "1.0.0");
        assert_eq!(migrated.get("utcp_version").unwrap(), "0.2.0");
        let tools = migrated.get("tools").and_then(|v| v.as_array()).unwrap();
        let tool = tools[0].as_object().unwrap();
        assert!(tool.get("inputs").is_some());
        assert!(tool.get("outputs").is_some());
        let tmpl = tool
            .get("tool_call_template")
            .and_then(|v| v.as_object())
            .unwrap();
        assert_eq!(
            tmpl.get("call_template_type")
                .and_then(|v| v.as_str())
                .unwrap(),
            "http"
        );
    }

    #[test]
    fn validate_manual_and_config() {
        let config = json!({
            "manual_call_templates": [{
                "call_template_type": "http",
                "url": "http://example.com",
                "http_method": "GET"
            }]
        });
        validate_v1_config(&config).unwrap();

        let manual = json!({
            "manual_version": "1.0.0",
            "utcp_version": "0.2.0",
            "info": { "title": "x", "version": "1.0" },
            "tools": [{
                "name": "t",
                "description": "d",
                "inputs": { "type": "object" },
                "outputs": { "type": "object" },
                "tool_call_template": {
                    "call_template_type": "http",
                    "url": "http://example.com",
                    "http_method": "GET"
                }
            }]
        });
        validate_v1_manual(&manual).unwrap();
    }
}
