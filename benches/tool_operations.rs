use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use rs_utcp::{
    config::UtcpClientConfig, repository::in_memory::InMemoryToolRepository,
    tag::tag_search::TagSearchStrategy, UtcpClient, UtcpClientInterface,
};
use serde_json::json;
use std::fs;
use std::{collections::HashMap, sync::Arc};
use tempfile::NamedTempFile;
use tokio::runtime::Runtime;

/// Helper function to create a client with tools from config
async fn create_client_with_tools(tool_count: usize) -> Arc<UtcpClient> {
    let mut tools = vec![];
    for i in 0..tool_count {
        tools.push(json!({
            "name": format!("tool_{}", i),
            "description": format!("Description for tool {}", i),
            "inputs": {"type": "object"},
            "outputs": {"type": "object"},
            "tags": [
                format!("category_{}", i % 5),
                format!("type_{}", i % 3),
                "common"
            ]
        }));
    }

    let config_content = json!({
        "manual_call_templates": [{
            "call_template_type": "cli",
            "name": "test_provider",
            "command": "echo",
            "tools": tools
        }]
    });

    let temp_file = NamedTempFile::new().unwrap();
    fs::write(
        temp_file.path(),
        serde_json::to_vec(&config_content).unwrap(),
    )
    .unwrap();

    let config = UtcpClientConfig::new().with_providers_file(temp_file.path().to_path_buf());
    let repo = Arc::new(InMemoryToolRepository::new());
    let search = Arc::new(TagSearchStrategy::new(repo.clone(), 1.0));

    Arc::new(UtcpClient::new(config, repo, search).await.unwrap())
}

/// Benchmark tool search performance with different repository sizes
fn bench_tool_search(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("tool_search");

    for tool_count in [10, 50, 100, 500].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(tool_count),
            tool_count,
            |b, &count| {
                let client = rt.block_on(create_client_with_tools(count));

                b.to_async(&rt).iter(|| async {
                    let results = client
                        .search_tools(black_box("category_2"), black_box(10))
                        .await
                        .unwrap();
                    black_box(results)
                });
            },
        );
    }

    group.finish();
}

/// Benchmark client initialization with different configurations
fn bench_client_initialization(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("client_init_empty", |b| {
        b.to_async(&rt).iter(|| async {
            let config = UtcpClientConfig::new();
            let repo = Arc::new(InMemoryToolRepository::new());
            let search = Arc::new(TagSearchStrategy::new(repo.clone(), 1.0));

            let client = UtcpClient::create(black_box(config), black_box(repo), black_box(search))
                .await
                .unwrap();

            black_box(client)
        });
    });
}

/// Benchmark tool calling with different argument sizes
fn bench_tool_call_overhead(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("tool_call_overhead");

    for arg_count in [0, 5, 10, 20].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(arg_count),
            arg_count,
            |b, &count| {
                let client = rt.block_on(async {
                    let config_content = json!({
                        "manual_call_templates": [{
                            "call_template_type": "cli",
                            "name": "test_provider",
                            "command": "echo",
                            "tools": [{
                                "name": "echo",
                                "description": "Echo tool",
                                "inputs": {"type": "object"},
                                "outputs": {"type": "object"},
                                "tags": ["test"]
                            }]
                        }]
                    });

                    let temp_file = NamedTempFile::new().unwrap();
                    fs::write(
                        temp_file.path(),
                        serde_json::to_vec(&config_content).unwrap(),
                    )
                    .unwrap();

                    let config =
                        UtcpClientConfig::new().with_providers_file(temp_file.path().to_path_buf());
                    let repo = Arc::new(InMemoryToolRepository::new());
                    let search = Arc::new(TagSearchStrategy::new(repo.clone(), 1.0));

                    Arc::new(UtcpClient::new(config, repo, search).await.unwrap())
                });

                b.to_async(&rt).iter(|| async {
                    let mut args = HashMap::new();
                    for i in 0..count {
                        args.insert(
                            format!("arg_{}", i),
                            serde_json::json!(format!("value_{}", i)),
                        );
                    }

                    // Call the echo tool
                    let _ = client
                        .call_tool(black_box("test_provider.echo"), black_box(args))
                        .await;
                });
            },
        );
    }

    group.finish();
}

/// Benchmark tag matching algorithm performance
fn bench_tag_matching(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("tag_matching");

    for tag_count in [2, 5, 10, 20].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(tag_count),
            tag_count,
            |b, &count| {
                let client = rt.block_on(async {
                    // Create 100 tools with varying tag counts
                    let mut tools = vec![];
                    for i in 0..100 {
                        let mut tags = vec![];
                        for j in 0..count {
                            tags.push(format!("tag_{}_{}", i % 10, j));
                        }

                        tools.push(json!({
                            "name": format!("tool_{}", i),
                            "description": format!("Tool {}", i),
                            "inputs": {"type": "object"},
                            "outputs": {"type": "object"},
                            "tags": tags
                        }));
                    }

                    let config_content = json!({
                        "manual_call_templates": [{
                            "call_template_type": "cli",
                            "name": "test_provider",
                            "command": "echo",
                            "tools": tools
                        }]
                    });

                    let temp_file = NamedTempFile::new().unwrap();
                    fs::write(
                        temp_file.path(),
                        serde_json::to_vec(&config_content).unwrap(),
                    )
                    .unwrap();

                    let config =
                        UtcpClientConfig::new().with_providers_file(temp_file.path().to_path_buf());
                    let repo = Arc::new(InMemoryToolRepository::new());
                    let search = Arc::new(TagSearchStrategy::new(repo.clone(), 1.0));

                    Arc::new(UtcpClient::new(config, repo, search).await.unwrap())
                });

                b.to_async(&rt).iter(|| async {
                    let results = client
                        .search_tools(black_box("tag_5"), black_box(10))
                        .await
                        .unwrap();
                    black_box(results)
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_tool_search,
    bench_client_initialization,
    bench_tool_call_overhead,
    bench_tag_matching,
);
criterion_main!(benches);
