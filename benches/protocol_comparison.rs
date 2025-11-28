use criterion::{black_box, criterion_group, criterion_main, Criterion};
use rs_utcp::{
    config::UtcpClientConfig,
    repository::in_memory::InMemoryToolRepository,
    tag::tag_search::TagSearchStrategy,
    UtcpClient, UtcpClientInterface,
};
use std::{collections::HashMap, sync::Arc};
use tokio::runtime::Runtime;
use serde_json::json;
use tempfile::NamedTempFile;
use std::fs;

/// Helper to create a client from a config JSON
async fn create_client_from_config(config_json: serde_json::Value) -> Arc<UtcpClient> {
    let temp_file = NamedTempFile::new().unwrap();
    fs::write(temp_file.path(), serde_json::to_vec(&config_json).unwrap()).unwrap();
    
    let config = UtcpClientConfig::new().with_providers_file(temp_file.path().to_path_buf());
    let repo = Arc::new(InMemoryToolRepository::new());
    let search = Arc::new(TagSearchStrategy::new(repo.clone(), 1.0));
    
    Arc::new(UtcpClient::new(config, repo, search).await.unwrap())
}

/// Benchmark CLI tool calling (actual execution)
fn bench_cli_tool_call(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    c.bench_function("cli_echo_call", |b| {
        let client = rt.block_on(async {
            let config_json = json!({
                "manual_call_templates": [{
                    "call_template_type": "cli",
                    "name": "echo_provider",
                    "command": "echo",
                    "tools": [{
                        "name": "echo",
                        "description": "Echo a message",
                        "inputs": {"type": "object"},
                        "outputs": {"type": "object"},
                        "tags": ["utility"],
                        "tool_call_template": {
                            "call_template_type": "cli",
                            "command": "echo",
                            "args": ["{{message}}"]
                        }
                    }]
                }]
            });
            
            create_client_from_config(config_json).await
        });
        
        b.to_async(&rt).iter(|| async {
            let mut args = HashMap::new();
            args.insert("message".to_string(), json!("benchmark"));
            
            let _ = client.call_tool(black_box("echo_provider.echo"), black_box(args)).await;
        });
    });
}

/// Compare provider initialization overhead across different types
fn bench_provider_comparison(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("provider_initialization");
    
    // HTTP Provider
    group.bench_function("http", |b| {
        b.to_async(&rt).iter(|| async {
            let config_json = json!({
                "manual_call_templates": [{
                    "call_template_type": "http",
                    "name": "http_test",
                    "url": "http://localhost:9999/tools",
                    "http_method": "GET"
                }]
            });
            
            let client = create_client_from_config(black_box(config_json)).await;
            black_box(client)
        });
    });
    
    // CLI Provider
    group.bench_function("cli", |b| {
        b.to_async(&rt).iter(|| async {
            let config_json = json!({
                "manual_call_templates": [{
                    "call_template_type": "cli",
                    "name": "cli_test",
                    "command": "echo"
                }]
            });
            
            let client = create_client_from_config(black_box(config_json)).await;
            black_box(client)
        });
    });
    
    // WebSocket Provider (registration only, no connection)
    group.bench_function("websocket", |b| {
        b.to_async(&rt).iter(|| async {
            let config_json = json!({
                "manual_call_templates": [{
                    "call_template_type": "websocket",
                    "name": "ws_test",
                    "url": "ws://localhost:9999"
                }]
            });
            
            let client = create_client_from_config(black_box(config_json)).await;
            black_box(client)
        });
    });
    
    // MCP Provider
    group.bench_function("mcp", |b| {
        b.to_async(&rt).iter(|| async {
            let config_json = json!({
                "manual_call_templates": [{
                    "call_template_type": "mcp",
                    "name": "mcp_test",
                    "command": "python3",
                    "args": ["server.py"]
                }]
            });
            
            let client = create_client_from_config(black_box(config_json)).await;
            black_box(client)
        });
    });
    
    // gRPC Provider
    group.bench_function("grpc", |b| {
        b.to_async(&rt).iter(|| async {
            let config_json = json!({
                "manual_call_templates": [{
                    "call_template_type": "grpc",
                    "name": "grpc_test",
                    "url": "http://localhost:9999"
                }]
            });
            
            let client = create_client_from_config(black_box(config_json)).await;
            black_box(client)
        });
    });
    
    // SSE Provider
    group.bench_function("sse", |b| {
        b.to_async(&rt).iter(|| async {
            let config_json = json!({
                "manual_call_templates": [{
                    "call_template_type": "sse",
                    "name": "sse_test",
                    "url": "http://localhost:9999/events"
                }]
            });
            
            let client = create_client_from_config(black_box(config_json)).await;
            black_box(client)
        });
    });
    
    group.finish();
}

/// Benchmark loading multiple providers at once
fn bench_multi_provider_loading(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    c.bench_function("load_6_providers", |b| {
        b.to_async(&rt).iter(|| async {
            let config_json = json!({
                "manual_call_templates": [
                    {
                        "call_template_type": "http",
                        "name": "http_provider",
                        "url": "http://localhost:8001/tools"
                    },
                    {
                        "call_template_type": "cli",
                        "name": "cli_provider",
                        "command": "echo"
                    },
                    {
                        "call_template_type": "websocket",
                        "name": "ws_provider",
                        "url": "ws://localhost:8002"
                    },
                    {
                        "call_template_type": "mcp",
                        "name": "mcp_provider",
                        "command": "python3",
                        "args": ["server.py"]
                    },
                    {
                        "call_template_type": "grpc",
                        "name": "grpc_provider",
                        "url": "http://localhost:8003"
                    },
                    {
                        "call_template_type": "sse",
                        "name": "sse_provider",
                        "url": "http://localhost:8004/events"
                    }
                ]
            });
            
            let client = create_client_from_config(black_box(config_json)).await;
            black_box(client)
        });
    });
}

criterion_group!(
    benches,
    bench_cli_tool_call,
    bench_provider_comparison,
    bench_multi_provider_loading,
);
criterion_main!(benches);
