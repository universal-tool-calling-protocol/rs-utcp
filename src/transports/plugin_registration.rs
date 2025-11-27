use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};

use rs_utcp::call_templates::{
    call_template_to_provider, register_call_template_handler, CALL_TEMPLATE_HANDLERS,
};
use rs_utcp::transports::{
    communication_protocols_snapshot, register_communication_protocol, CommunicationProtocol,
};
use rs_utcp::transports::stream::boxed_vec_stream;

fn myproto_template_handler(template: Value) -> anyhow::Result<Value> {
    let mut obj = template.as_object().cloned().unwrap_or_default();
    obj.insert("marker".to_string(), Value::String("handled by myproto".into()));
    Ok(Value::Object(obj))
}

#[derive(Debug)]
struct MyProtocol;

#[async_trait]
impl CommunicationProtocol for MyProtocol {
    async fn register_tool_provider(
        &self,
        _prov: &dyn rs_utcp::providers::base::Provider,
    ) -> anyhow::Result<Vec<rs_utcp::tools::Tool>> {
        Ok(vec![])
    }

    async fn deregister_tool_provider(
        &self,
        _prov: &dyn rs_utcp::providers::base::Provider,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    async fn call_tool(
        &self,
        _tool_name: &str,
        _args: HashMap<String, Value>,
        _prov: &dyn rs_utcp::providers::base::Provider,
    ) -> anyhow::Result<Value> {
        Ok(json!("ok"))
    }

    async fn call_tool_stream(
        &self,
        _tool_name: &str,
        _args: HashMap<String, Value>,
        _prov: &dyn rs_utcp::providers::base::Provider,
    ) -> anyhow::Result<Box<dyn rs_utcp::transports::stream::StreamResult>> {
        Ok(boxed_vec_stream(vec![json!("ok")]))
    }
}

#[test]
fn registering_custom_plugins_makes_them_available() {
    let key = "myproto_test";

    register_call_template_handler(key, myproto_template_handler);
    let provider = call_template_to_provider(json!({ "call_template_type": key })).unwrap();
    assert_eq!(
        provider,
        json!({ "call_template_type": key, "marker": "handled by myproto" })
    );

    register_communication_protocol(key, Arc::new(MyProtocol));
    let snapshot = communication_protocols_snapshot();
    assert!(
        snapshot.get(key).is_some(),
        "custom communication protocol should be registered"
    );

    // Clean up globals to avoid leaking state across tests.
    if let Ok(mut handlers) = CALL_TEMPLATE_HANDLERS.write() {
        handlers.remove(key);
    }
    if let Ok(mut reg) = rs_utcp::transports::registry::GLOBAL_COMMUNICATION_PROTOCOLS.write() {
        *reg = rs_utcp::transports::CommunicationProtocolRegistry::with_default_protocols();
    }
}
