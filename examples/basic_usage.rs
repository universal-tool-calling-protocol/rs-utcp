// Example: Using rust-utcp to register and call tools

use rs_utcp::config::UtcpClientConfig;
use rs_utcp::providers::cli::CliProvider;
use rs_utcp::providers::http::HttpProvider;
use rs_utcp::repository::in_memory::InMemoryToolRepository;
use rs_utcp::tag::tag_search::TagSearchStrategy;
use rs_utcp::{UtcpClient, UtcpClientInterface};
use std::collections::HashMap;
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Create configuration
    let config = UtcpClientConfig::default();

    // Create repository and search strategy
    let repo = Arc::new(InMemoryToolRepository::new());
    let search = Arc::new(TagSearchStrategy::new(repo.clone(), 1.0));

    // Create UTCP client (initializes all 12 transports)
    let client = UtcpClient::new(config, repo, search);

    println!("✓ UTCP Client initialized with all transports");
    println!("  Available transports: HTTP, CLI, WebSocket, gRPC, GraphQL, TCP, UDP, SSE, MCP, WebRTC, HTTP Stream, Text");

    // Example 1: Register HTTP provider
    println!("\n📡 Example 1: HTTP Provider");
    let http_provider = HttpProvider::new(
        "weather_api".to_string(),
        "https://api.weather.example.com/tools".to_string(),
        "GET".to_string(),
        None,
    );

    match client.register_tool_provider(Arc::new(http_provider)).await {
        Ok(tools) => {
            println!("  ✓ Registered HTTP provider with {} tools", tools.len());
            for tool in &tools {
                println!("    - {}: {}", tool.name, tool.description);
            }
        }
        Err(e) => println!("  ✗ Failed to register HTTP provider: {}", e),
    }

    // Example 2: Register CLI provider
    println!("\n🖥️  Example 2: CLI Provider");
    let cli_provider = CliProvider::new("git_tools".to_string(), "git".to_string(), None);

    match client.register_tool_provider(Arc::new(cli_provider)).await {
        Ok(tools) => {
            println!("  ✓ Registered CLI provider with {} tools", tools.len());
        }
        Err(e) => println!("  ✗ Failed to register CLI provider: {}", e),
    }

    // Example 3: Search for tools
    println!("\n🔍 Example 3: Search Tools");
    match client.search_tools("weather", 5).await {
        Ok(tools) => {
            println!("  Found {} tools matching 'weather':", tools.len());
            for tool in &tools {
                println!("    - {}", tool.name);
            }
        }
        Err(e) => println!("  ✗ Search failed: {}", e),
    }

    // Example 4: Call a tool
    println!("\n⚡ Example 4: Call Tool");
    let mut args = HashMap::new();
    args.insert("city".to_string(), serde_json::json!("London"));
    args.insert("units".to_string(), serde_json::json!("metric"));

    match client
        .call_tool("weather_api.get_current_weather", args)
        .await
    {
        Ok(result) => {
            println!("  ✓ Tool executed successfully");
            println!("  Result: {}", serde_json::to_string_pretty(&result)?);
        }
        Err(e) => println!("  ✗ Tool call failed: {}", e),
    }

    // Example 5: List available transports
    println!("\n📋 Example 5: Available Transports");
    let transports = client.get_transports();
    println!("  {} transports available:", transports.len());
    for (name, _) in &transports {
        println!("    - {}", name);
    }

    println!("\n✨ Demo complete!");

    Ok(())
}
