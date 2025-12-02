<p align="center">
  <img src="https://github.com/user-attachments/assets/9c3a9645-7e21-47d9-8bd2-59ad5ae3d3bc" alt="UTCP Logo" width="256" height="256">
</p>

<h1 align="center">rs-utcp</h1>

<p align="center">
  <strong>Universal Tool Calling Protocol Client for Rust</strong>
</p>

<p align="center">
  <a href="https://crates.io/crates/rs-utcp"><img src="https://img.shields.io/crates/v/rs-utcp.svg" alt="Crates.io"></a>
  <a href="https://docs.rs/rs-utcp"><img src="https://docs.rs/rs-utcp/badge.svg" alt="Documentation"></a>
  <a href="https://github.com/universal-tool-calling-protocol/rs-utcp/blob/main/LICENSE"><img src="https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg" alt="License"></a>
  <a href="https://github.com/universal-tool-calling-protocol/rs-utcp/actions"><img src="https://github.com/universal-tool-calling-protocol/rs-utcp/workflows/CI/badge.svg" alt="CI"></a>
</p>

<p align="center">
  A powerful, async-first Rust implementation of the <a href="https://www.utcp.io">Universal Tool Calling Protocol (UTCP)</a>
</p>

---

## 🌟 Features

- **🔌 12 Communication Protocols (formerly transports)** - HTTP, MCP, WebSocket, gRPC, CLI, GraphQL, TCP, UDP, SSE, WebRTC, HTTP Streams, and Text-based
- **🚀 Async/Await Native** - Built with Tokio for high-performance concurrent operations
- **📦 Config-Driven** - Load tool providers from JSON with automatic discovery and registration
- **🔍 Smart Tool Discovery** - Tag-based semantic search across all registered tools
- **🤖 LLM Integration** - Built-in Codemode orchestrator for AI-driven workflows
- **🔄 Auto-Migration** - Seamless compatibility with UTCP v0.1 and v1.0 formats
- **📝 OpenAPI Support** - Automatic tool generation from OpenAPI 3.0 specifications
- **🔐 Multi-Auth** - Support for API keys, Basic Auth, OAuth2, and custom authentication
- **💾 Streaming** - First-class support for streaming responses across compatible communication protocols
- **🧪 Well-Tested** - 90+ tests ensuring reliability and correctness

## 📦 Installation

Add `rs-utcp` to your `Cargo.toml`:

```toml
[dependencies]
rs-utcp = "0.1.8"
tokio = { version = "1.0", features = ["full"] }
```

Or use `cargo add`:

```bash
cargo add rs-utcp
cargo add tokio --features full
```

## 🚀 Quick Start

### Basic Usage

```rust
use rs_utcp::{
    config::UtcpClientConfig,
    repository::in_memory::InMemoryToolRepository,
    tag::tag_search::TagSearchStrategy,
    UtcpClient, UtcpClientInterface,
};
use std::{collections::HashMap, sync::Arc};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. Configure the client
    let config = UtcpClientConfig::new()
        .with_manual_path("providers.json".into());
    
    // 2. Set up repository and search
    let repo = Arc::new(InMemoryToolRepository::new());
    let search = Arc::new(TagSearchStrategy::new(repo.clone(), 1.0));
    
    // 3. Create the client
    let client = UtcpClient::create(config, repo, search).await?;
    
    // 4. Discover tools
    let tools = client.search_tools("weather", 10).await?;
    println!("Found {} tools", tools.len());
    
    // 5. Call a tool
    let mut args = HashMap::new();
    args.insert("city".to_string(), serde_json::json!("London"));
    
    let result = client.call_tool("weather.get_forecast", args).await?;
    println!("Result: {}", serde_json::to_string_pretty(&result)?);
    
    Ok(())
}
```

### Configuration File (`providers.json`)

```json
{
  "manual_version": "1.0.0",
  "utcp_version": "0.3.0",
  "allowed_communication_protocols": ["http", "mcp"],
  "info": {
    "title": "Example UTCP Manual",
    "version": "1.0.0",
    "description": "Manual v1.0 with tools"
  },
  "tools": [
    {
      "name": "get_forecast",
      "description": "Get current weather for a city",
      "inputs": {
        "type": "object",
        "properties": {
          "city": { "type": "string", "description": "City name" },
          "units": { "type": "string", "enum": ["metric", "imperial"] }
        },
        "required": ["city"]
      },
      "outputs": { "type": "object" },
      "tool_call_template": {
        "call_template_type": "http",
        "name": "weather_api",
        "url": "https://api.weather.example.com/tools",
        "http_method": "GET",
        "headers": { "Accept": "application/json" }
      },
      "tags": ["weather", "demo"]
    },
    {
      "name": "read_file",
      "description": "Read a text file via MCP stdio",
      "inputs": {
        "type": "object",
        "properties": {
          "path": { "type": "string", "description": "File path" }
        },
        "required": ["path"]
      },
      "outputs": { "type": "object" },
      "tool_call_template": {
        "call_template_type": "mcp",
        "name": "file_tools",
        "command": "python3",
        "args": ["mcp_server.py"]
      },
      "tags": ["mcp", "filesystem"]
    }
  ],
  "load_variables_from": [
    {
      "variable_loader_type": "dotenv",
      "env_file_path": ".env"
    }
  ]
}
```

## 🔌 Supported Communication Protocols

rs-utcp supports a comprehensive range of communication protocols, each with full async support:

### Production-Ready Protocols

| Protocol | Description | Status | Streaming |
|-----------|-------------|--------|-----------|
| **HTTP** | REST APIs with UTCP manifest or OpenAPI | ✅ Stable | ❌ |
| **MCP** | Model Context Protocol (stdio & SSE) | ✅ Stable | ✅ |
| **WebRTC** | P2P data channels with signaling | ✅ Stable | ✅ |
| **WebSocket** | Real-time bidirectional communication | ✅ Stable | ✅ |
| **CLI** | Execute local binaries as tools | ✅ Stable | ❌ |
| **gRPC** | High-performance RPC with TLS & auth metadata | ✅ Stable | ✅ |
| **GraphQL** | Query-based tool calling with type-aware variables | ✅ Stable | ❌ |
| **SSE** | Server-Sent Events | ✅ Stable | ✅ |
| **HTTP Streams** | Streaming HTTP responses | ✅ Stable | ✅ |
| **TCP** | Low-level socket transport (framed JSON) | ✅ Stable | ✅ |
| **UDP** | Low-level datagram transport | ✅ Stable | ❌ |
| **Text** | File-based tool providers (JS/SH/Python/exe) | ✅ Stable | ❌ |

## 💡 Examples

### HTTP Provider with OpenAPI

```rust
use rs_utcp::openapi::OpenApiConverter;

// Automatically convert OpenAPI spec to UTCP tools
let converter = OpenApiConverter::new_from_url(
    "https://petstore.swagger.io/v2/swagger.json",
    Some("petstore".to_string())
).await?;

let manual = converter.convert();
println!("Discovered {} tools from OpenAPI spec", manual.tools.len());
```

### MCP Stdio Provider

```rust
let config = serde_json::json!({
    "manual_call_templates": [{
        "call_template_type": "mcp",
        "name": "calculator",
        "command": "python3",
        "args": ["calculator_server.py"],
        "env_vars": {
            "DEBUG": "1"
        }
    }]
});

let client = create_client_from_config(config).await?;
let result = client.call_tool("calculator.add", 
    HashMap::from([
        ("a".to_string(), json!(5)),
        ("b".to_string(), json!(3))
    ])
).await?;
```

### Streaming Tools

```rust
// Call a streaming tool
let mut stream = client.call_tool_stream(
    "sse_provider.events",
    HashMap::new()
).await?;

// Process stream results
while let Some(item) = stream.next().await {
    match item {
        Ok(value) => println!("Received: {}", value),
        Err(e) => eprintln!("Error: {}", e),
    }
}

stream.close().await?;
```

### WebRTC Peer-to-Peer

WebRTC enables direct peer-to-peer tool calling:

```bash
# Terminal 1: Start WebRTC server with signaling
cargo run --example webrtc_server

# Terminal 2: Connect and call tools
cargo run --example webrtc_client
```

See [`examples/webrtc_server/`](examples/webrtc_server/) for the complete implementation.

## 🤖 Codemode & LLM Orchestration

rs-utcp includes a powerful **Codemode** feature that enables dynamic script execution with full access to registered tools. This is perfect for LLM-driven workflows.

### Codemode Basics

```rust
use rs_utcp::plugins::codemode::{CodeModeUtcp, CodeModeArgs};

let codemode = CodeModeUtcp::new(client);

// Execute a Rhai script that calls tools
let script = r#"
    let weather = call_tool("weather.get_forecast", #{
        "city": "Tokyo"
    });
    
    let summary = call_tool("ai.summarize", #{
        "text": weather.to_string()
    });
    
    summary
"#;

let result = codemode.execute(CodeModeArgs {
    code: script.to_string(),
    timeout: Some(30_000),
}).await?;

println!("Result: {:?}", result.value);
```

### LLM Orchestration

The `CodemodeOrchestrator` provides a 4-step AI-driven workflow:

1. **Decide** - LLM determines if tools are needed
2. **Select** - LLM chooses relevant tools
3. **Generate** - LLM writes a Rhai script
4. **Execute** - Script runs in sandboxed environment

```rust
use rs_utcp::plugins::codemode::CodemodeOrchestrator;

let codemode = Arc::new(CodeModeUtcp::new(client));
let llm_model = Arc::new(YourLLMModel::new());
let orchestrator = CodemodeOrchestrator::new(codemode, llm_model);

// Let the LLM figure out how to accomplish the task
let result = orchestrator
    .call_prompt("Get the weather in Paris and summarize it")
    .await?;

match result {
    Some(value) => println!("LLM completed task: {}", value),
    None => println!("No tools needed for this request"),
}
```

See the [Gemini example](examples/orchestrator_gemini.rs) for a complete LLM integration.

### Codemode Security

Codemode executes scripts in a **hardened sandbox** with comprehensive security measures:

- ✅ **Code Validation** - Pre-execution checks for dangerous patterns and size limits
- ✅ **Timeout Enforcement** - Strict timeouts (5s default, 30s max) prevent runaway scripts
- ✅ **Resource Limits** - Memory, CPU, and output size constraints
- ✅ **Sandboxed Execution** - Rhai scripts run isolated from the file system and OS

See [SECURITY.md](SECURITY.md) for complete security documentation.

## 🎯 Use Cases

### 1. **Multi-Protocol API Gateway**
Call tools across HTTP, gRPC, and MCP from a single unified interface.

### 2. **LLM Agent Toolkit**
Provide language models with a consistent way to execute tools regardless of their implementation.

### 3. **Microservices Orchestration**
Coordinate calls across heterogeneous services using different protocols.

### 4. **Plugin System**
Build extensible applications where plugins can be added via configuration.

### 5. **Testing & Mocking**
Easily swap implementations (e.g., HTTP → CLI) for testing without code changes.

## 📚 Documentation

- **[API Documentation](https://docs.rs/rs-utcp)** - Complete API reference
- **[UTCP Specification](https://www.utcp.io)** - Protocol specification
- **[Examples](/examples)** - Working examples for all transports
- **[SECURITY](SECURITY.md)** - Security features and best practices
- **[CHANGELOG](CHANGELOG.md)** - Version history and changes

## 🧪 Testing

Run the comprehensive test suite:

```bash
# Run all tests
cargo test

# Run with output
cargo test -- --nocapture

# Run specific test
cargo test test_http_transport

# Run examples
cargo run --example basic_usage
cargo run --example all_providers
```

## 🏗️ Architecture

```
┌─────────────────────────────────────────────────────┐
│                   UtcpClient                        │
│  (Unified interface for all tool operations)       │
└─────────────────┬───────────────────────────────────┘
                  │
         ┌────────┴──────────┐
         │                   │
  ┌──────▼──────┐   ┌────────▼────────┐
  │  Repository │   │ Communication   │
  │             │   │ Protocols       │
  │  - Tools    │   │  - HTTP         │
  │  - Search   │   │  - MCP          │
  └─────────────┘   │  - gRPC         │
                    │  - WebSocket    │
                    │  - CLI          │
                    │  - etc.         │
                    └─────────────────┘
```

### Key Components

- **UtcpClient** - Main entry point for all operations
- **CommunicationProtocolRegistry** (was TransportRegistry) - Manages all communication protocol implementations
- **Call template handlers** - Registry that maps `call_template_type` to provider builders
- **ToolRepository** - Stores and indexes discovered tools
- **SearchStrategy** - Semantic search across tools
- **Codemode** - Script execution environment
- **Loader** - Configuration and provider loading

### Plugin registration (custom protocols)

Register new communication protocols and call template handlers before constructing your client:

```rust
use std::sync::Arc;
use rs_utcp::call_templates::register_call_template_handler;
use rs_utcp::transports::register_communication_protocol;

fn myproto_template_handler(template: serde_json::Value) -> anyhow::Result<serde_json::Value> {
    // normalize/augment the template into a provider config
    Ok(template)
}

register_call_template_handler("myproto", myproto_template_handler);
register_communication_protocol("myproto", Arc::new(MyProtocol::new())); // implements CommunicationProtocol
```

## 🔧 Advanced Configuration

### Authentication

```json
{
  "manual_call_templates": [{
    "call_template_type": "http",
    "name": "secure_api",
    "url": "https://api.example.com",
    "auth": {
      "auth_type": "api_key",
      "api_key": "${API_KEY}",
      "var_name": "X-API-Key",
      "location": "header"
    }
  }]
}
```

### Environment Variables

```json
{
  "load_variables_from": [
    {
      "variable_loader_type": "dotenv",
      "env_file_path": ".env"
    }
  ],
  "variables": {
    "DEFAULT_TIMEOUT": "30000"
  }
}
```

### Protocol Restrictions

You can restrict which communication protocols are allowed for a manual or provider using the `allowed_communication_protocols` field. This provides a secure-by-default mechanism where tools can only use their own protocol unless explicitly allowed.

```json
{
  "manual_version": "1.0.0",
  "info": { "title": "Restricted Manual", "version": "1.0.0" },
  "allowed_communication_protocols": ["http", "cli"],
  "tools": [
    {
      "name": "http_tool",
      "tool_call_template": {
        "call_template_type": "http",
        "url": "http://example.com"
      }
    },
    {
      "name": "cli_tool",
      "tool_call_template": {
        "call_template_type": "cli",
        "command": "echo"
      }
    }
  ]
}
```

If `allowed_communication_protocols` is not specified, it defaults to only allowing the tool's own protocol type. Tools attempting to use disallowed protocols will be filtered out during registration, and calls will fail validation.

### Custom Search Strategy

```rust
use rs_utcp::tools::ToolSearchStrategy;
use async_trait::async_trait;

struct MySearchStrategy;

#[async_trait]
impl ToolSearchStrategy for MySearchStrategy {
    async fn search_tools(&self, query: &str, limit: usize) 
        -> anyhow::Result<Vec<Tool>> 
    {
        // Your custom search logic
        Ok(vec![])
    }
}
```

## 🤝 Contributing

Contributions are welcome! Here's how you can help:

1. **Found a bug?** Open an issue
2. **Have a feature idea?** Start a discussion
3. **Want to contribute code?** Submit a PR

### Development Setup

```bash
# Clone the repository
git clone https://github.com/universal-tool-calling-protocol/rs-utcp.git
cd rs-utcp

# Run tests
cargo test

# Format code
cargo fmt

# Run lints
cargo clippy

# Build all examples
cargo build --examples
```

## 📜 License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## 🙏 Acknowledgments

- Inspired by [go-utcp](https://github.com/universal-tool-calling-protocol/go-utcp)
- Built on the [UTCP specification](https://www.utcp.io)
- Powered by the amazing Rust async ecosystem

## 📬 Contact & Support

- **Issues**: [GitHub Issues](https://github.com/universal-tool-calling-protocol/rs-utcp/issues)
- **Discussions**: [GitHub Discussions](https://github.com/universal-tool-calling-protocol/rs-utcp/discussions)
- **UTCP Community**: [utcp.io](https://www.utcp.io)

---

<p align="center">
  Made with ❤️ by the UTCP community
</p>
