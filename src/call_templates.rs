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
        handlers.insert("websocket".to_string(), websocket_call_template_handler);
        handlers.insert("grpc".to_string(), grpc_call_template_handler);
        handlers.insert("graphql".to_string(), graphql_call_template_handler);
        handlers.insert("tcp".to_string(), tcp_call_template_handler);
        handlers.insert("udp".to_string(), udp_call_template_handler);
        handlers.insert("sse".to_string(), sse_call_template_handler);
        handlers.insert("mcp".to_string(), mcp_call_template_handler);
        handlers.insert("webrtc".to_string(), webrtc_call_template_handler);
        handlers.insert("http_stream".to_string(), http_stream_call_template_handler);
        handlers.insert("text".to_string(), text_call_template_handler);
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

fn websocket_call_template_handler(template: Value) -> Result<Value> {
    let (_, mut obj) = normalize_common_template(template)?;
    if let Some(url) = obj.remove("url") {
        obj.insert("url".to_string(), url);
    }
    if let Some(protocol) = obj.remove("protocol") {
        obj.insert("protocol".to_string(), protocol);
    }
    if let Some(keep_alive) = obj.remove("keep_alive") {
        obj.insert("keep_alive".to_string(), keep_alive);
    }
    if let Some(headers) = obj.remove("headers") {
        obj.insert("headers".to_string(), headers);
    }
    Ok(Value::Object(obj))
}

fn grpc_call_template_handler(template: Value) -> Result<Value> {
    let (_, mut obj) = normalize_common_template(template)?;
    if let Some(host) = obj.remove("host") {
        obj.insert("host".to_string(), host);
    }
    if let Some(port) = obj.remove("port") {
        obj.insert("port".to_string(), port);
    }
    if let Some(use_ssl) = obj.remove("use_ssl") {
        obj.insert("use_ssl".to_string(), use_ssl);
    }
    Ok(Value::Object(obj))
}

fn graphql_call_template_handler(template: Value) -> Result<Value> {
    let (_, mut obj) = normalize_common_template(template)?;
    if let Some(url) = obj.remove("url") {
        obj.insert("url".to_string(), url);
    }
    if let Some(op_type) = obj.remove("operation_type") {
        obj.insert("operation_type".to_string(), op_type);
    }
    if let Some(op_name) = obj.remove("operation_name") {
        obj.insert("operation_name".to_string(), op_name);
    }
    if let Some(headers) = obj.remove("headers") {
        obj.insert("headers".to_string(), headers);
    }
    Ok(Value::Object(obj))
}

fn tcp_call_template_handler(template: Value) -> Result<Value> {
    let (_, mut obj) = normalize_common_template(template)?;
    if let Some(host) = obj.remove("host") {
        obj.insert("host".to_string(), host);
    }
    if let Some(port) = obj.remove("port") {
        obj.insert("port".to_string(), port);
    }
    if let Some(timeout) = obj.remove("timeout_ms") {
        obj.insert("timeout_ms".to_string(), timeout);
    }
    Ok(Value::Object(obj))
}

fn udp_call_template_handler(template: Value) -> Result<Value> {
    let (_, mut obj) = normalize_common_template(template)?;
    if let Some(host) = obj.remove("host") {
        obj.insert("host".to_string(), host);
    }
    if let Some(port) = obj.remove("port") {
        obj.insert("port".to_string(), port);
    }
    if let Some(timeout) = obj.remove("timeout_ms") {
        obj.insert("timeout_ms".to_string(), timeout);
    }
    Ok(Value::Object(obj))
}

fn sse_call_template_handler(template: Value) -> Result<Value> {
    let (_, mut obj) = normalize_common_template(template)?;
    if let Some(url) = obj.remove("url") {
        obj.insert("url".to_string(), url);
    }
    if let Some(headers) = obj.remove("headers") {
        obj.insert("headers".to_string(), headers);
    }
    if let Some(body_field) = obj.remove("body_field") {
        obj.insert("body_field".to_string(), body_field);
    }
    if let Some(header_fields) = obj.remove("header_fields") {
        obj.insert("header_fields".to_string(), header_fields);
    }
    Ok(Value::Object(obj))
}

fn mcp_call_template_handler(template: Value) -> Result<Value> {
    let (_, mut obj) = normalize_common_template(template)?;
    // HTTP fields
    if let Some(url) = obj.remove("url") {
        obj.insert("url".to_string(), url);
    }
    if let Some(headers) = obj.remove("headers") {
        obj.insert("headers".to_string(), headers);
    }
    // Stdio fields
    if let Some(cmd) = obj.remove("command") {
        obj.insert("command".to_string(), cmd);
    }
    if let Some(args) = obj.remove("args") {
        obj.insert("args".to_string(), args);
    }
    if let Some(env) = obj.remove("env_vars") {
        obj.insert("env_vars".to_string(), env);
    }
    Ok(Value::Object(obj))
}

fn webrtc_call_template_handler(template: Value) -> Result<Value> {
    let (_, mut obj) = normalize_common_template(template)?;
    if let Some(sig) = obj.remove("signaling_server") {
        obj.insert("signaling_server".to_string(), sig);
    }
    if let Some(ice) = obj.remove("ice_servers") {
        obj.insert("ice_servers".to_string(), ice);
    }
    if let Some(label) = obj.remove("channel_label") {
        obj.insert("channel_label".to_string(), label);
    }
    if let Some(ordered) = obj.remove("ordered") {
        obj.insert("ordered".to_string(), ordered);
    }
    if let Some(life) = obj.remove("max_packet_life_time") {
        obj.insert("max_packet_life_time".to_string(), life);
    }
    if let Some(retx) = obj.remove("max_retransmits") {
        obj.insert("max_retransmits".to_string(), retx);
    }
    Ok(Value::Object(obj))
}

fn http_stream_call_template_handler(template: Value) -> Result<Value> {
    let (_, mut obj) = normalize_common_template(template)?;
    if let Some(url) = obj.remove("url") {
        obj.insert("url".to_string(), url);
    }
    if let Some(method) = obj.remove("http_method") {
        obj.insert("http_method".to_string(), method);
    }
    if let Some(headers) = obj.remove("headers") {
        obj.insert("headers".to_string(), headers);
    }
    Ok(Value::Object(obj))
}

fn text_call_template_handler(template: Value) -> Result<Value> {
    let (_, mut obj) = normalize_common_template(template)?;
    if let Some(path) = obj.remove("base_path") {
        obj.insert("base_path".to_string(), path);
    }
    Ok(Value::Object(obj))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_websocket_template() {
        let template = json!({
            "call_template_type": "websocket",
            "name": "ws-tool",
            "url": "ws://localhost:8080",
            "protocol": "json-rpc"
        });
        let result = call_template_to_provider(template).unwrap();
        assert_eq!(result["provider_type"], "websocket");
        assert_eq!(result["url"], "ws://localhost:8080");
        assert_eq!(result["protocol"], "json-rpc");
    }

    #[test]
    fn test_grpc_template() {
        let template = json!({
            "call_template_type": "grpc",
            "name": "grpc-tool",
            "host": "localhost",
            "port": 50051,
            "use_ssl": true
        });
        let result = call_template_to_provider(template).unwrap();
        assert_eq!(result["provider_type"], "grpc");
        assert_eq!(result["host"], "localhost");
        assert_eq!(result["port"], 50051);
        assert_eq!(result["use_ssl"], true);
    }

    #[test]
    fn test_mcp_template() {
        let template = json!({
            "call_template_type": "mcp",
            "name": "mcp-tool",
            "command": "python",
            "args": ["server.py"]
        });
        let result = call_template_to_provider(template).unwrap();
        assert_eq!(result["provider_type"], "mcp");
        assert_eq!(result["command"], "python");
        assert_eq!(result["args"][0], "server.py");
    }
}
