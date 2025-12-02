use async_trait::async_trait;
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use rs_utcp::{
    config::UtcpClientConfig,
    plugins::codemode::{CodeModeArgs, CodeModeUtcp},
    providers::base::{BaseProvider, Provider, ProviderType},
    repository::in_memory::InMemoryToolRepository,
    tag::tag_search::TagSearchStrategy,
    tools::{Tool, ToolInputOutputSchema},
    transports::{
        registry::register_communication_protocol,
        stream::{boxed_vec_stream, StreamResult},
        CommunicationProtocol,
    },
    UtcpClient, UtcpClientInterface,
};
use serde_json::{json, Value};
use std::{collections::HashMap, sync::Arc};
use tokio::runtime::Runtime;

// --- Mock Protocol for Benchmarking ---

#[derive(Debug)]
struct BenchmarkProtocol;

#[async_trait]
impl CommunicationProtocol for BenchmarkProtocol {
    async fn register_tool_provider(&self, _prov: &dyn Provider) -> anyhow::Result<Vec<Tool>> {
        Ok(vec![
            Tool {
                name: "echo".to_string(),
                description: "Echo tool".to_string(),
                inputs: ToolInputOutputSchema {
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
                },
                outputs: ToolInputOutputSchema {
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
                },
                tags: vec![],
                average_response_size: None,
                provider: None,
            },
            Tool {
                name: "stream".to_string(),
                description: "Streaming tool".to_string(),
                inputs: ToolInputOutputSchema {
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
                },
                outputs: ToolInputOutputSchema {
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
                },
                tags: vec![],
                average_response_size: None,
                provider: None,
            },
        ])
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
        // Simple echo
        Ok(json!(args))
    }

    async fn call_tool_stream(
        &self,
        _tool_name: &str,
        _args: HashMap<String, Value>,
        _prov: &dyn Provider,
    ) -> anyhow::Result<Box<dyn StreamResult>> {
        // Return a stream of 10 items
        let items: Vec<Value> = (0..10).map(|i| json!({"chunk": i})).collect();
        Ok(boxed_vec_stream(items))
    }
}

// --- Helper to setup client ---

async fn create_bench_client() -> Arc<UtcpClient> {
    // Register our benchmark protocol under "http_stream" to hijack it
    // We use HttpStream provider type because it maps to "http_stream" key
    register_communication_protocol("http_stream", Arc::new(BenchmarkProtocol));

    let config = UtcpClientConfig::new();
    let repo = Arc::new(InMemoryToolRepository::new());
    let search = Arc::new(TagSearchStrategy::new(repo.clone(), 1.0));
    let client = UtcpClient::create(config, repo, search).await.unwrap();

    // Register a provider that uses our hijacked protocol
    let provider = BaseProvider {
        name: "bench".to_string(),
        provider_type: ProviderType::HttpStream,
        auth: None,
        allowed_communication_protocols: vec!["http_stream".to_string()].into(),
    };

    client
        .register_tool_provider(Arc::new(provider))
        .await
        .unwrap();

    Arc::new(client)
}

// --- Benchmarks ---

fn bench_call_tool_comparison(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("call_tool_comparison");

    // Setup client once
    let client = rt.block_on(create_bench_client());
    let codemode = CodeModeUtcp::new(client.clone());

    group.bench_function("native", |b| {
        b.to_async(&rt).iter(|| async {
            let mut args = HashMap::new();
            args.insert("msg".to_string(), json!("hello"));
            client
                .call_tool(black_box("bench.echo"), black_box(args))
                .await
                .unwrap()
        });
    });

    group.bench_function("codemode", |b| {
        let script = r#"call_tool("bench.echo", #{ "msg": "hello" })"#;
        b.to_async(&rt).iter(|| async {
            let args = CodeModeArgs {
                code: black_box(script.to_string()),
                timeout: Some(5000),
            };
            codemode.execute(black_box(args)).await.unwrap()
        });
    });

    group.finish();
}

fn bench_call_tool_stream_comparison(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("call_tool_stream_comparison");

    let client = rt.block_on(create_bench_client());
    let codemode = CodeModeUtcp::new(client.clone());

    group.bench_function("native", |b| {
        b.to_async(&rt).iter(|| async {
            let args = HashMap::new();
            let mut stream = client
                .call_tool_stream(black_box("bench.stream"), black_box(args))
                .await
                .unwrap();

            // Consume the stream
            let mut count = 0;
            while let Ok(Some(_)) = stream.next().await {
                count += 1;
            }
            black_box(count)
        });
    });

    group.bench_function("codemode", |b| {
        // Rhai script to consume stream
        let script = r#"
            let stream = call_tool_stream("bench.stream", #{});
            stream.len()
        "#;
        b.to_async(&rt).iter(|| async {
            let args = CodeModeArgs {
                code: black_box(script.to_string()),
                timeout: Some(5000),
            };
            codemode.execute(black_box(args)).await.unwrap()
        });
    });

    group.finish();
}

fn bench_call_many_tools_comparison(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("call_many_tools_comparison");

    let client = rt.block_on(create_bench_client());
    let codemode = CodeModeUtcp::new(client.clone());
    let tool_count = 50;

    group.bench_function("native", |b| {
        b.to_async(&rt).iter(|| async {
            for i in 0..tool_count {
                let mut args = HashMap::new();
                args.insert("msg".to_string(), json!(format!("hello {}", i)));
                client
                    .call_tool(black_box("bench.echo"), black_box(args))
                    .await
                    .unwrap();
            }
        });
    });

    group.bench_function("codemode", |b| {
        let script = format!(
            r#"
            for i in 0..{} {{
                call_tool("bench.echo", #{{ "msg": "hello " + i }});
            }}
        "#,
            tool_count
        );

        b.to_async(&rt).iter(|| async {
            let args = CodeModeArgs {
                code: black_box(script.clone()),
                timeout: Some(10000),
            };
            codemode.execute(black_box(args)).await.unwrap()
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_call_tool_comparison,
    bench_call_tool_stream_comparison,
    bench_call_many_tools_comparison,
);
criterion_main!(benches);
