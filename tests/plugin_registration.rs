use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::{json, Value};

use rs_utcp::call_templates::{
    call_template_to_provider, register_call_template_handler, CALL_TEMPLATE_HANDLERS,
};
use rs_utcp::providers::base::{Provider, ProviderType};
use rs_utcp::transports::registry::GLOBAL_COMMUNICATION_PROTOCOLS;
use rs_utcp::transports::stream::boxed_vec_stream;
use rs_utcp::transports::{
    communication_protocols_snapshot, register_communication_protocol, CommunicationProtocol,
};

fn myproto_template_handler(template: Value) -> anyhow::Result<Value> {
    let mut obj = template.as_object().cloned().unwrap_or_default();
    obj.insert(
        "marker".to_string(),
        Value::String("handled by myproto".into()),
    );
    Ok(Value::Object(obj))
}

#[derive(Debug, Default)]
struct CountingProtocol {
    call_count: AtomicUsize,
    stream_count: AtomicUsize,
    captured_args: Mutex<Vec<HashMap<String, Value>>>,
}

#[async_trait]
impl CommunicationProtocol for CountingProtocol {
    async fn register_tool_provider(
        &self,
        _prov: &dyn Provider,
    ) -> anyhow::Result<Vec<rs_utcp::tools::Tool>> {
        Ok(vec![])
    }

    async fn deregister_tool_provider(&self, _prov: &dyn Provider) -> anyhow::Result<()> {
        Ok(())
    }

    async fn call_tool(
        &self,
        _tool_name: &str,
        args: HashMap<String, Value>,
        _prov: &dyn Provider,
    ) -> anyhow::Result<Value> {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        self.captured_args.lock().unwrap().push(args.clone());
        Ok(json!({ "echo": args }))
    }

    async fn call_tool_stream(
        &self,
        _tool_name: &str,
        args: HashMap<String, Value>,
        _prov: &dyn Provider,
    ) -> anyhow::Result<Box<dyn rs_utcp::transports::stream::StreamResult>> {
        self.stream_count.fetch_add(1, Ordering::SeqCst);
        self.captured_args.lock().unwrap().push(args.clone());
        Ok(boxed_vec_stream(vec![json!({ "stream": args })]))
    }
}

#[derive(Debug, Clone)]
struct DummyProvider {
    name: String,
}

impl DummyProvider {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
        }
    }
}

impl Provider for DummyProvider {
    fn type_(&self) -> ProviderType {
        ProviderType::Unknown
    }

    fn name(&self) -> String {
        self.name.clone()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[test]
fn registering_custom_plugins_makes_them_available() {
    let key = "myproto_test";
    let registry_before = communication_protocols_snapshot();

    register_call_template_handler(key, myproto_template_handler);
    let provider = call_template_to_provider(json!({ "call_template_type": key })).unwrap();
    assert_eq!(
        provider,
        json!({ "call_template_type": key, "marker": "handled by myproto" })
    );

    register_communication_protocol(key, Arc::new(CountingProtocol::default()));
    let snapshot = communication_protocols_snapshot();
    assert!(
        snapshot.get(key).is_some(),
        "custom communication protocol should be registered"
    );

    // Clean up globals to avoid leaking state across tests.
    if let Ok(mut handlers) = CALL_TEMPLATE_HANDLERS.write() {
        handlers.remove(key);
    }
    if let Ok(mut reg) = GLOBAL_COMMUNICATION_PROTOCOLS.write() {
        *reg = registry_before;
    }
}

#[tokio::test]
async fn custom_protocol_call_tool_and_stream_are_invoked() {
    let key = "myproto_calls";
    let registry_before = communication_protocols_snapshot();
    let protocol = Arc::new(CountingProtocol::default());
    register_communication_protocol(key, protocol.clone());

    let snapshot = communication_protocols_snapshot();
    let proto = snapshot
        .get(key)
        .expect("custom protocol should be visible in snapshot");

    let provider = DummyProvider::new("dummy");
    let mut args = HashMap::new();
    args.insert("foo".into(), json!(1));

    let call_response = proto
        .call_tool("demo.tool", args.clone(), &provider)
        .await
        .unwrap();
    assert_eq!(call_response, json!({ "echo": args.clone() }));
    assert_eq!(protocol.call_count.load(Ordering::SeqCst), 1);

    let mut stream = proto
        .call_tool_stream("demo.stream", args.clone(), &provider)
        .await
        .unwrap();
    assert_eq!(
        stream.next().await.unwrap(),
        Some(json!({ "stream": args.clone() }))
    );
    assert_eq!(stream.next().await.unwrap(), None);
    assert_eq!(protocol.stream_count.load(Ordering::SeqCst), 1);

    let captured = protocol.captured_args.lock().unwrap();
    assert_eq!(
        captured.len(),
        2,
        "call_tool and call_tool_stream should capture args"
    );

    if let Ok(mut reg) = GLOBAL_COMMUNICATION_PROTOCOLS.write() {
        *reg = registry_before;
    }
}
