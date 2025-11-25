# rs-utcp

A Rust implementation of the Universal Tool Calling Protocol (UTCP) client, based on `go-utcp`.

## Overview

`rust-utcp` is a comprehensive Rust port of the `go-utcp` project, providing a unified interface for calling tools across different transport protocols. It supports multiple provider types and transports, enabling seamless integration with various APIs and services.

## Features

- **Multi-Transport Support**: HTTP, WebSocket, gRPC, GraphQL, TCP, UDP, SSE, MCP, WebRTC, CLI, and more
- **Async/Await**: Built on Tokio for high-performance async operations
- **Type-Safe**: Leverages Rust's type system for compile-time safety
- **Provider Management**: Register, discover, and manage tool providers
- **Tool Search**: Semantic search across registered tools using tags and descriptions
- **Authentication**: Support for API Key, Basic Auth, and OAuth2

## Architecture

The project structure mirrors the `go-utcp` project:

- **`src/lib.rs`**: Main entry point, defines `UtcpClient` and `UtcpClientInterface`
- **`src/config.rs`**: Configuration structs (`UtcpClientConfig`)
- **`src/tools/`**: Tool definitions, schemas, and search strategies
- **`src/providers/`**: Provider definitions (HTTP, etc.)
- **`src/transports/`**: Transport implementations for different protocols
- **`src/repository/`**: Tool repository interfaces and in-memory implementation
- **`src/auth/`**: Authentication mechanisms
- **`src/tag/`**: Tag-based tool search implementation

## Implemented Transports

All 12 transport types from `go-utcp` have been implemented:

1. ✅ **HTTP** - RESTful HTTP/HTTPS APIs (fully functional)
2. ✅ **CLI** - Command-line tool execution (skeleton)
3. ✅ **WebSocket** - WebSocket-based communication (skeleton)
4. ✅ **gRPC** - gRPC service calls (skeleton)
5. ✅ **GraphQL** - GraphQL queries and mutations (skeleton)
6. ✅ **TCP** - Raw TCP socket communication (skeleton)
7. ✅ **UDP** - UDP datagram communication (skeleton)
8. ✅ **SSE** - Server-Sent Events (skeleton)
9. ✅ **MCP** - Model Context Protocol (skeleton)
10. ✅ **WebRTC** - WebRTC data channels (skeleton)
11. ✅ **HTTP Stream** - Streamable HTTP responses (skeleton)
12. ✅ **Text** - File-based tool definitions (skeleton)

### Transport Status

- **HTTP Transport**: Fully implemented with tool discovery and execution
- **Other Transports**: Skeleton implementations are in place, ready for extension

## Usage

```rust
use rust_utcp::{UtcpClient, UtcpClientInterface};
use rust_utcp::config::UtcpClientConfig;
use rust_utcp::repository::in_memory::InMemoryToolRepository;
use rust_utcp::tag::tag_search::TagSearchStrategy;
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Create configuration
    let config = UtcpClientConfig::default();
    
    // Create repository and search strategy
    let repo = Arc::new(InMemoryToolRepository::new());
    let search = Arc::new(TagSearchStrategy::new(repo.clone(), 1.0));
    
    // Create UTCP client
    let client = UtcpClient::new(config, repo, search);
    
    // Register a provider (example)
    // let provider = HttpProvider::new(..., None);
    // let tools = client.register_tool_provider(Arc::new(provider)).await?;
    
    // Call a tool
    // let result = client.call_tool("provider.tool_name", args).await?;
    
    Ok(())
}
```

## Examples

- Quick start HTTP/CLI: `cargo run --example basic_usage`
- Load providers from JSON: `cargo run --example load_from_json`
- All providers/transport patterns (env-gated):  
  `DEMO_HTTP_URL=https://httpbin.org/post DEMO_WS_URL=wss://echo.websocket.events cargo run --example all_providers`
- Self-hosted demos (spin up server + client in one binary):
  - HTTP: `cargo run --example http_server`
- WebSocket: `cargo run --example websocket_server`
- SSE: `cargo run --example sse_server`
- TCP: `cargo run --example tcp_server`
- UDP: `cargo run --example udp_server`
- HTTP stream: `cargo run --example http_stream_server`
- Codemode (Rust-like DSL): `cargo run --example codemode_eval`

### Codemode (Rust-like DSL)

Codemode lets you orchestrate UTCP tools by evaluating Rust-like snippets (powered by Rhai) with helper functions injected:

- `call_tool("<provider.tool>", #{...args})`
- `search_tools("<query>", <limit>)`
- `sprintf("hello {}", [value])`

Example snippet:

```text
let sum = 2 + 3;
let echoed = call_tool("http_demo.echo", #{"message": "hi"});
sum // returned as __out
```

See `examples/codemode_eval/main.rs` for a runnable demo.

## Core Components

### UtcpClient

The main client that manages providers, transports, and tool calls:

```rust
pub trait UtcpClientInterface: Send + Sync {
    async fn register_tool_provider(&self, prov: Arc<dyn Provider>) -> Result<Vec<Tool>>;
    async fn deregister_tool_provider(&self, provider_name: &str) -> Result<()>;
    async fn call_tool(&self, tool_name: &str, args: HashMap<String, Value>) -> Result<Value>;
    async fn search_tools(&self, query: &str, limit: usize) -> Result<Vec<Tool>>;
    async fn call_tool_stream(&self, tool_name: &str, args: HashMap<String, Value>) -> Result<Box<dyn StreamResult>>;
}
```

### Providers

Providers define how to connect to and interact with tool sources:

- `HttpProvider`: HTTP/HTTPS endpoints
- More providers can be added following the same pattern

### Tool Repository

Manages registered providers and their tools:

- `InMemoryToolRepository`: Simple in-memory storage
- Can be extended for persistent storage (database, file system, etc.)

### Search Strategy

`TagSearchStrategy`: Searches tools by matching query terms against tool tags and descriptions with configurable weighting.

## Development Status

This is a working implementation with the core architecture in place:

- ✅ Core client infrastructure
- ✅ All transport skeletons created
- ✅ HTTP transport fully functional
- ✅ Provider trait and HTTP provider
- ✅ Tool repository and search
- ✅ Authentication framework
- 🚧 OpenAPI spec parsing (planned)
- 🚧 Full transport implementations (in progress)

## Building

```bash
cargo build
```

## Testing

```bash
cargo test
```

## Dependencies

- `tokio` - Async runtime
- `serde` / `serde_json` - Serialization
- `reqwest` - HTTP client
- `async-trait` - Async trait support
- `anyhow` / `thiserror` - Error handling
- `regex` - Pattern matching for search

## Contributing

Contributions are welcome! The project structure is designed to be extensible:

1. Each transport has its own module in `src/transports/`
2. New providers can be added to `src/providers/`
3. Custom search strategies can implement `ToolSearchStrategy`
4. Authentication methods extend the `Auth` trait

## License

[Include appropriate license information]

## Acknowledgments

Based on the [go-utcp](https://github.com/universal-tool-calling-protocol/go-utcp) project.
# rs-utcp
