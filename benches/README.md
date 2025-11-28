# Performance Benchmarks

This directory contains comprehensive performance benchmarks for the `rs-utcp` library using [Criterion.rs](https://github.com/bheisler/criterion.rs).

## Benchmark Suites

### 1. Tool Operations (`tool_operations.rs`)

Measures core UTCP client operations:

- **Tool Registration**: Performance with varying numbers of tools (10, 50, 100, 500)
- **Tool Search**: Search performance across different repository sizes
- **Client Initialization**: Overhead of creating a new client
- **Tool Call Overhead**: Impact of different argument counts
- **Tag Matching**: Performance of the tag-based search algorithm

### 2. Protocol Comparison (`protocol_comparison.rs`)

Compares different communication protocol implementations:

- **Provider Registration**: Overhead for HTTP, CLI, WebSocket, MCP protocols
- **Serialization**: JSON serialization performance for different provider types
- **CLI Tool Calls**: Actual execution performance for CLI-based tools

### 3. Codemode Execution (`codemode_execution.rs`)

Benchmarks the Rhai script execution engine:

- **Script Complexity**: Simple arithmetic, loops, arrays, maps
- **Script Size**: Performance with varying script lengths (10-200 lines)
- **String Operations**: String concatenation and manipulation
- **Function Calls**: Recursive function execution (e.g., Fibonacci)

## Running Benchmarks

### Run All Benchmarks

```bash
cargo bench
```

### Run Specific Benchmark Suite

```bash
# Tool operations only
cargo bench --bench tool_operations

# Protocol comparison only
cargo bench --bench protocol_comparison

# Codemode execution only
cargo bench --bench codemode_execution
```

### Run Specific Benchmark

```bash
# Run only tool registration benchmarks
cargo bench --bench tool_operations tool_registration

# Run only search benchmarks with 100 tools
cargo bench --bench tool_operations "tool_search/100"
```

### Generate Flamegraphs (Optional)

Install cargo-flamegraph:
```bash
cargo install flamegraph
```

Run with flamegraph:
```bash
cargo bench --bench tool_operations -- --profile-time=5
```

## Interpreting Results

Criterion generates HTML reports in `target/criterion/`. Open `target/criterion/report/index.html` in your browser to view:

- **Throughput**: Operations per second
- **Latency**: Time per operation (mean, median, std dev)
- **Comparison**: Performance changes between runs
- **Violin Plots**: Distribution of measurements
- **Regression**: Historical performance tracking

### Example Output

```
tool_registration/10    time:   [125.43 µs 127.89 µs 130.67 µs]
tool_registration/50    time:   [623.21 µs 631.45 µs 640.89 µs]
tool_registration/100   time:   [1.2534 ms 1.2689 ms 1.2851 ms]
tool_registration/500   time:   [6.3421 ms 6.4123 ms 6.4891 ms]
```

## Performance Baselines

Expected performance on a modern system (Apple M1/M2 or equivalent):

| Operation | Size | Expected Time |
|-----------|------|---------------|
| Tool Registration | 100 tools | ~1-2 ms |
| Tool Search | 100 tools | ~50-100 µs |
| Client Init | Empty | ~100-200 µs |
| Simple Script | 10 lines | ~50-100 µs |
| CLI Tool Call | Echo | ~2-5 ms |

## CI Integration

Benchmarks can be run in CI to detect performance regressions:

```yaml
- name: Run benchmarks
  run: cargo bench --no-fail-fast
  
- name: Upload benchmark results
  uses: actions/upload-artifact@v4
  with:
    name: benchmark-results
    path: target/criterion
```

## Optimizing Performance

If you find slow performance:

1. **Tool Registration**: Consider batching tool registrations
2. **Search**: Use more specific tags to reduce search space
3. **Codemode**: Keep scripts small and avoid deep recursion
4. **Protocols**: Choose the right protocol for your use case:
   - **CLI**: Fast for local tools
   - **HTTP**: Good for REST APIs
   - **gRPC**: Best for high-throughput RPC
   - **MCP**: Ideal for stdio-based tools

## Adding New Benchmarks

To add a new benchmark:

1. Create a new file in `benches/` or add to an existing one
2. Use the Criterion API:

```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn my_benchmark(c: &mut Criterion) {
    c.bench_function("my_operation", |b| {
        b.iter(|| {
            // Your code to benchmark
            black_box(expensive_operation())
        });
    });
}

criterion_group!(benches, my_benchmark);
criterion_main!(benches);
```

3. Add to `Cargo.toml`:

```toml
[[bench]]
name = "my_benchmark"
harness = false
```

## Resources

- [Criterion.rs Documentation](https://bheisler.github.io/criterion.rs/book/)
- [Rust Performance Book](https://nnethercote.github.io/perf-book/)
- [Benchmarking Best Practices](https://easyperf.net/blog/)
