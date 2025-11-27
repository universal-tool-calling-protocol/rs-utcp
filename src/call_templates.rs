use std::collections::HashMap;
use std::sync::RwLock;

use anyhow::{anyhow, Result};
use once_cell::sync::Lazy;
use serde_json::{Map, Value};

/// Function that converts a call template value into a provider value.
pub type CallTemplateHandler = fn(Value) -> Result<Value>;

/// Global registry of call template handlers keyed by call_template_type.
pub static CALL_TEMPLATE_HANDLERS: Lazy<RwLock<HashMap<String, CallTemplateHandler>>> =
    Lazy::new(|| {
        let mut handlers: HashMap<String, CallTemplateHandler> = HashMap::new();
        handlers.insert("http".to_string(), http_call_template_handler);
        handlers.insert("cli".to_string(), cli_call_template_handler);
        RwLock::new(handlers)
    });

/// Register or override a call template handler for a call_template_type.
pub fn register_call_template_handler(key: &str, handler: CallTemplateHandler) {
    let mut handlers = CALL_TEMPLATE_HANDLERS
        .write()
        .expect("call template handler registry poisoned");
    handlers.insert(key.to_string(), handler);
}

/// Convert a call template into a provider representation using registered handlers.
pub fn call_template_to_provider(template: Value) -> Result<Value> {
    let call_template_type = template
        .as_object()
        .and_then(|obj| obj.get("call_template_type"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Missing call_template_type"))?
        .to_string();

    let handler = {
        let handlers = CALL_TEMPLATE_HANDLERS
            .read()
            .expect("call template handler registry poisoned");
        handlers.get(&call_template_type).copied()
    }
    .unwrap_or(default_call_template_handler);

    handler(template)
}

fn normalize_common_template(mut template: Value) -> Result<(String, Map<String, Value>)> {
    let mut obj = template
        .as_object_mut()
        .ok_or_else(|| anyhow!("call template must be an object"))?
        .clone();

    let ctype = obj
        .get("call_template_type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Missing call_template_type"))?
        .to_string();

    let name = obj
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or(&ctype)
        .to_string();
    obj.entry("name").or_insert(Value::String(name.clone()));

    obj.insert("provider_type".to_string(), Value::String(ctype.clone()));
    obj.insert("type".to_string(), Value::String(ctype.clone()));

    Ok((ctype, obj))
}

fn default_call_template_handler(template: Value) -> Result<Value> {
    let (_, obj) = normalize_common_template(template)?;
    Ok(Value::Object(obj))
}

fn http_call_template_handler(template: Value) -> Result<Value> {
    let (_, mut obj) = normalize_common_template(template)?;

    if let Some(method) = obj.remove("method").or_else(|| obj.remove("http_method")) {
        obj.insert("http_method".to_string(), method);
    }
    if !obj.contains_key("http_method") {
        obj.insert("http_method".to_string(), Value::String("GET".to_string()));
    }
    if let Some(body_field) = obj.remove("body_field") {
        obj.insert("body_field".to_string(), body_field);
    }
    if let Some(headers) = obj.remove("headers") {
        obj.insert("headers".to_string(), headers);
    }
    if let Some(url) = obj.remove("url") {
        obj.insert("url".to_string(), url);
    }
    if !obj.contains_key("url") {
        obj.insert(
            "url".to_string(),
            Value::String("http://localhost".to_string()),
        );
    }

    Ok(Value::Object(obj))
}

fn cli_call_template_handler(template: Value) -> Result<Value> {
    let (_, mut obj) = normalize_common_template(template)?;

    if let Some(cmd) = obj.remove("command") {
        obj.insert("command_name".to_string(), cmd);
    } else if let Some(commands) = obj.get("commands").and_then(|v| v.as_array()) {
        let first = commands
            .get(0)
            .and_then(|c| c.get("command"))
            .and_then(|v| v.as_str())
            .unwrap_or("bash -c \"\"");
        obj.insert("command_name".to_string(), Value::String(first.to_string()));
    }

    if let Some(env) = obj.remove("env_vars") {
        obj.insert("env_vars".to_string(), env);
    }
    if let Some(cwd) = obj.remove("working_dir") {
        obj.insert("working_dir".to_string(), cwd);
    }

    Ok(Value::Object(obj))
}
