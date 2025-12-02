use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use rs_utcp::{
    config::UtcpClientConfig,
    plugins::codemode::{CodeModeArgs, CodeModeUtcp},
    repository::in_memory::InMemoryToolRepository,
    tag::tag_search::TagSearchStrategy,
    UtcpClient,
};
use serde_json::json;
use std::fs;
use std::sync::Arc;
use tempfile::NamedTempFile;
use tokio::runtime::Runtime;

/// Helper to create a client from a config JSON
async fn create_client_from_config(config_json: serde_json::Value) -> Arc<UtcpClient> {
    let temp_file = NamedTempFile::new().unwrap();
    fs::write(temp_file.path(), serde_json::to_vec(&config_json).unwrap()).unwrap();

    let config = UtcpClientConfig::new().with_providers_file(temp_file.path().to_path_buf());
    let repo = Arc::new(InMemoryToolRepository::new());
    let search = Arc::new(TagSearchStrategy::new(repo.clone(), 1.0));

    Arc::new(UtcpClient::new(config, repo, search).await.unwrap())
}

/// Benchmark basic Rhai script execution
fn bench_simple_script_execution(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("codemode_simple_script", |b| {
        let codemode = rt.block_on(async {
            let config = UtcpClientConfig::new();
            let repo = Arc::new(InMemoryToolRepository::new());
            let search = Arc::new(TagSearchStrategy::new(repo.clone(), 1.0));
            let client = UtcpClient::create(config, repo, search).await.unwrap();

            CodeModeUtcp::new(Arc::new(client))
        });

        b.to_async(&rt).iter(|| async {
            let script = "42 + 58";
            let args = CodeModeArgs {
                code: black_box(script.to_string()),
                timeout: Some(5000),
            };

            let result = codemode.execute(black_box(args)).await.unwrap();
            black_box(result)
        });
    });
}

/// Benchmark script execution with different complexity levels
fn bench_script_complexity(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("codemode_complexity");

    let codemode = rt.block_on(async {
        let config = UtcpClientConfig::new();
        let repo = Arc::new(InMemoryToolRepository::new());
        let search = Arc::new(TagSearchStrategy::new(repo.clone(), 1.0));
        let client = UtcpClient::create(config, repo, search).await.unwrap();

        CodeModeUtcp::new(Arc::new(client))
    });

    // Simple arithmetic
    group.bench_function("arithmetic", |b| {
        b.to_async(&rt).iter(|| async {
            let script = "let x = 10; let y = 20; x + y * 2";
            let args = CodeModeArgs {
                code: black_box(script.to_string()),
                timeout: Some(5000),
            };

            let result = codemode.execute(black_box(args)).await.unwrap();
            black_box(result)
        });
    });

    // Loop execution
    group.bench_function("loop_100", |b| {
        b.to_async(&rt).iter(|| async {
            let script = r#"
                let sum = 0;
                for i in 0..100 {
                    sum += i;
                }
                sum
            "#;
            let args = CodeModeArgs {
                code: black_box(script.to_string()),
                timeout: Some(5000),
            };

            let result = codemode.execute(black_box(args)).await.unwrap();
            black_box(result)
        });
    });

    // Array operations
    group.bench_function("array_operations", |b| {
        b.to_async(&rt).iter(|| async {
            let script = r#"
                let arr = [];
                for i in 0..50 {
                    arr.push(i * 2);
                }
                arr.len()
            "#;
            let args = CodeModeArgs {
                code: black_box(script.to_string()),
                timeout: Some(5000),
            };

            let result = codemode.execute(black_box(args)).await.unwrap();
            black_box(result)
        });
    });

    // Map operations
    group.bench_function("map_operations", |b| {
        b.to_async(&rt).iter(|| async {
            let script = r#"
                let map = #{};
                for i in 0..50 {
                    map[`key_${i}`] = i * 2;
                }
                map.len()
            "#;
            let args = CodeModeArgs {
                code: black_box(script.to_string()),
                timeout: Some(5000),
            };

            let result = codemode.execute(black_box(args)).await.unwrap();
            black_box(result)
        });
    });

    group.finish();
}

/// Benchmark codemode call_tool function (key feature!)
fn bench_codemode_call_tool(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("codemode_call_tool_cli", |b| {
        let codemode = rt.block_on(async {
            let config_json = json!({
                "manual_call_templates": [{
                    "call_template_type": "cli",
                    "name": "test_provider",
                    "command": "echo",
                    "tools": [{
                        "name": "greet",
                        "description": "Greet someone",
                        "inputs": {"type": "object"},
                        "outputs": {"type": "object"},
                        "tags": ["greeting"],
                        "tool_call_template": {
                            "call_template_type": "cli",
                            "command": "echo",
                            "args": ["Hello, {{name}}!"]
                        }
                    }]
                }]
            });

            let client = create_client_from_config(config_json).await;
            CodeModeUtcp::new(client)
        });

        b.to_async(&rt).iter(|| async {
            // Script that calls a tool
            let script = r#"
                let result = call_tool("test_provider.greet", #{
                    "name": "World"
                });
                result
            "#;
            let args = CodeModeArgs {
                code: black_box(script.to_string()),
                timeout: Some(5000),
            };

            let result = codemode.execute(black_box(args)).await.unwrap();
            black_box(result)
        });
    });
}

/// Benchmark multiple tool calls in sequence
fn bench_codemode_multiple_tool_calls(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("codemode_multiple_calls");

    for call_count in [1, 3, 5].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(call_count),
            call_count,
            |b, &count| {
                let codemode = rt.block_on(async {
                    let config_json = json!({
                        "manual_call_templates": [{
                            "call_template_type": "cli",
                            "name": "test_provider",
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

                    let client = create_client_from_config(config_json).await;
                    CodeModeUtcp::new(client)
                });
                b.to_async(&rt).iter(|| async {
                    // Generate script with multiple tool calls
                    let mut script = String::from("let results = [];\n");
                    for i in 0..count {
                        script.push_str(&format!(
                            "results.push(call_tool(\"test_provider.echo\", #{{\"message\": \"msg_{}\"}}));\n",
                            i
                        ));
                    }
                    script.push_str("results.len()");

                    let args = CodeModeArgs {
                        code: black_box(script),
                        timeout: Some(10000),
                    };

                    let result = codemode.execute(black_box(args)).await.unwrap();
                    black_box(result)
                });
            },
        );
    }

    group.finish();
}

/// Benchmark script parsing overhead
fn bench_script_sizes(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("codemode_script_size");

    for line_count in [10, 50, 100, 200].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(line_count),
            line_count,
            |b, &count| {
                let codemode = rt.block_on(async {
                    let config = UtcpClientConfig::new();
                    let repo = Arc::new(InMemoryToolRepository::new());
                    let search = Arc::new(TagSearchStrategy::new(repo.clone(), 1.0));
                    let client = UtcpClient::create(config, repo, search).await.unwrap();

                    CodeModeUtcp::new(Arc::new(client))
                });

                // Generate a script with many lines
                let mut script = String::from("let sum = 0;\n");
                for i in 0..count {
                    script.push_str(&format!("sum += {};\n", i));
                }
                script.push_str("sum");

                b.to_async(&rt).iter(|| async {
                    let args = CodeModeArgs {
                        code: black_box(script.clone()),
                        timeout: Some(10000),
                    };

                    let result = codemode.execute(black_box(args)).await.unwrap();
                    black_box(result)
                });
            },
        );
    }

    group.finish();
}

/// Benchmark string operations in Rhai
fn bench_string_operations(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("codemode_string_concat", |b| {
        let codemode = rt.block_on(async {
            let config = UtcpClientConfig::new();
            let repo = Arc::new(InMemoryToolRepository::new());
            let search = Arc::new(TagSearchStrategy::new(repo.clone(), 1.0));
            let client = UtcpClient::create(config, repo, search).await.unwrap();

            CodeModeUtcp::new(Arc::new(client))
        });

        b.to_async(&rt).iter(|| async {
            let script = r#"
                let result = "";
                for i in 0..20 {
                    result += `item_${i}_`;
                }
                result
            "#;
            let args = CodeModeArgs {
                code: black_box(script.to_string()),
                timeout: Some(5000),
            };

            let result = codemode.execute(black_box(args)).await.unwrap();
            black_box(result)
        });
    });
}

/// Benchmark function definitions and calls
fn bench_function_calls(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("codemode_function_calls", |b| {
        let codemode = rt.block_on(async {
            let config = UtcpClientConfig::new();
            let repo = Arc::new(InMemoryToolRepository::new());
            let search = Arc::new(TagSearchStrategy::new(repo.clone(), 1.0));
            let client = UtcpClient::create(config, repo, search).await.unwrap();

            CodeModeUtcp::new(Arc::new(client))
        });

        b.to_async(&rt).iter(|| async {
            let script = r#"
                fn fibonacci(n) {
                    if n <= 1 {
                        return n;
                    }
                    return fibonacci(n - 1) + fibonacci(n - 2);
                }
                fibonacci(10)
            "#;
            let args = CodeModeArgs {
                code: black_box(script.to_string()),
                timeout: Some(5000),
            };

            let result = codemode.execute(black_box(args)).await.unwrap();
            black_box(result)
        });
    });
}

criterion_group!(
    benches,
    bench_simple_script_execution,
    bench_script_complexity,
    bench_codemode_call_tool,
    bench_codemode_multiple_tool_calls,
    bench_script_sizes,
    bench_string_operations,
    bench_function_calls,
);
criterion_main!(benches);
