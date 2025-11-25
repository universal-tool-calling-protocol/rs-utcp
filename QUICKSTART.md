# Quick Start Guide - rust-utcp

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
rust-utcp = { path = "../rust-utcp" }  # Or from crates.io when published
tokio = { version = "1.0", features = ["full"] }
serde_json = "1.0"
anyhow = "1.0"
```

## Basic Usage

### 1. Initialize the Client

```rust
use rust_utcp::*;
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Create configuration
    let config = config::UtcpClientConfig::default();
    
    // Create repository and search strategy
    let repo = Arc::new(repository::in_memory::InMemoryToolRepository::new());
    let search = Arc::new(tag::tag_search::TagSearchStrategy::new(repo.clone(), 1.0));
    
    // Create client with all transports
    let client = UtcpClient::new(config, repo, search);
    
    println!("✓ Client initialized!");
    Ok(())
}
```

### 2. Register an HTTP Provider

```rust
use rust_utcp::providers::http::HttpProvider;

// Create an HTTP provider pointing to a UTCP-enabled API
let provider = HttpProvider::new(
    "weather_api".to_string(),
    "https://api.example.com/utcp/tools".to_string(),
    "GET".to_string(),
    None,
);

// Register it with the client
let tools = client.register_tool_provider(Arc::new(provider)).await?;

println!("Registered {} tools", tools.len());
for tool in &tools {
    println!("  - {}: {}", tool.name, tool.description);
}
```

### 3. Register a CLI Provider

```rust
use rust_utcp::providers::cli::CliProvider;

// Create a CLI provider for a command-line tool
let mut provider = CliProvider::new(
    "local_tools".to_string(),
    "my-cli-tool".to_string(),
    None,
);

// Optional: set working directory
provider.working_dir = Some("/path/to/working/dir".to_string());

// Optional: set environment variables
let mut env = std::collections::HashMap::new();
env.insert("API_KEY".to_string(), "secret".to_string());
provider.env_vars = Some(env);

// Register it
let tools = client.register_tool_provider(Arc::new(provider)).await?;
```

### 4. Search for Tools

```rust
// Search by keywords
let results = client.search_tools("weather forecast", 10).await?;

for tool in results {
    println!("{}: {}", tool.name, tool.description);
    println!("  Tags: {:?}", tool.tags);
}
```

### 5. Call a Tool

```rust
use std::collections::HashMap;
use serde_json::json;

// Prepare arguments
let mut args = HashMap::new();
args.insert("city".to_string(), json!("London"));
args.insert("units".to_string(), json!("metric"));
args.insert("days".to_string(), json!(5));

// Call the tool (format: "provider_name.tool_name")
let result = client.call_tool("weather_api.get_forecast", args).await?;

// Process result
println!("Result: {}", serde_json::to_string_pretty(&result)?);
```

### 6. Handle Errors

```rust
match client.call_tool("api.some_tool", args).await {
    Ok(result) => {
        println!("Success: {:?}", result);
    }
    Err(e) => {
        eprintln!("Error calling tool: {}", e);
        // Handle error appropriately
    }
}
```

## Advanced Usage

### Custom Provider Types

You can create custom providers by implementing the `Provider` trait:

```rust
use rust_utcp::providers::base::{Provider, ProviderType};

#[derive(Debug, Clone)]
struct MyCustomProvider {
    name: String,
    // ... your fields
}

impl Provider for MyCustomProvider {
    fn type_(&self) -> ProviderType {
        ProviderType::Http  // Or your custom type
    }
    
    fn name(&self) -> String {
        self.name.clone()
    }
    
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
```

### Custom Transport

Implement the `ClientTransport` trait:

```rust
use async_trait::async_trait;
use rust_utcp::transports::ClientTransport;

pub struct MyTransport;

#[async_trait]
impl ClientTransport for MyTransport {
    async fn register_tool_provider(&self, prov: &dyn Provider) -> Result<Vec<Tool>> {
        // Your implementation
    }
    
    async fn call_tool(&self, tool_name: &str, args: HashMap<String, Value>, prov: &dyn Provider) -> Result<Value> {
        // Your implementation
    }
    
    // ... other methods
}
```

## Available Transports

The client initializes all transports automatically:

- `http` - HTTP/HTTPS REST APIs ✅ **Fully Functional**
- `cli` - Command-line tools ✅ **Fully Functional**
- `tcp` - TCP socket communication (basic implementation)
- `websocket` - WebSocket connections (skeleton)
- `grpc` - gRPC services (skeleton)
- `graphql` - GraphQL APIs (skeleton)
- `udp` - UDP datagrams (skeleton)
- `sse` - Server-Sent Events (skeleton)
- `mcp` - Model Context Protocol (skeleton)
- `webrtc` - WebRTC data channels (skeleton)
- `http_stream` - Streaming HTTP (skeleton)
- `text` - File-based tools (skeleton)

## Examples

Run the included example:

```bash
cargo run --example basic_usage
```

## Common Patterns

### Provider Registration Pattern

```rust
async fn register_provider(client: &UtcpClient, provider: Arc<dyn Provider>) -> Result<()> {
    match client.register_tool_provider(provider.clone()).await {
        Ok(tools) => {
            println!("✓ Registered {} with {} tools", provider.name(), tools.len());
            Ok(())
        }
        Err(e) => {
            eprintln!("✗ Failed to register {}: {}", provider.name(), e);
            Err(e)
        }
    }
}
```

### Batch Tool Execution

```rust
async fn execute_batch(client: &UtcpClient, tools: Vec<(&str, HashMap<String, Value>)>) -> Vec<Result<Value>> {
    let mut results = Vec::new();
    
    for (tool_name, args) in tools {
        let result = client.call_tool(tool_name, args).await;
        results.push(result);
    }
    
    results
}
```

### Search and Execute

```rust
async fn find_and_execute(client: &UtcpClient, query: &str, args: HashMap<String, Value>) -> Result<Value> {
    // Find the tool
    let tools = client.search_tools(query, 1).await?;
    
    if tools.is_empty() {
        return Err(anyhow::anyhow!("No tools found matching '{}'", query));
    }
    
    // Execute the first match
    client.call_tool(&tools[0].name, args).await
}
```

## Troubleshooting

### Tool Not Found

```rust
// Make sure the tool name is fully qualified: "provider.tool"
// ✗ Wrong: "get_weather"
// ✓ Correct: "weather_api.get_weather"
```

### Provider Registration Fails

```rust
// Check that the endpoint returns valid UTCP tool definitions
// The response should be JSON with a "tools" array
```

### CLI Tool Execution Times Out

```rust
// Increase timeout if needed (default: 30 seconds)
// Or ensure the CLI tool completes faster
```

## Next Steps

- See `IMPLEMENTATION.md` for architecture details
- Check `examples/` for more usage patterns
- Read the API documentation: `cargo doc --open`

## Support

- Issues: GitHub Issues
- Discussions: GitHub Discussions
- Documentation: `/docs`
