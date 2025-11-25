// Example: Using MCP stdio transport
//
// This example demonstrates how to use the MCP transport with stdio communication.
// It spawns an MCP server as a child process and communicates with it over stdin/stdout.

use rs_utcp::UtcpClientInterface;
use serde_json::json;
use std::collections::HashMap;

#[path = "common/mod.rs"]
mod common;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("🚀 MCP Stdio Transport Example\n");
    
    // Example 1: Using MCP stdio with node (if you have an MCP server)
    // Uncomment this if you have a real MCP server installed
    /*
    let client = common::client_from_providers(json!({
        "providers": [{
            "provider_type": "mcp",
            "name": "filesystem",
            "command": "node",
            "args": ["/path/to/mcp-server/index.js"],
            "env_vars": {
                "MCP_DEBUG": "1"
            }
        }]
    }))
    .await?;
    */
    
    // Example 2: Using the example stdio MCP server (Python)
    // First, run: python examples/mcp_stdio_server.py
    println!("📡 Creating UTCP client with MCP stdio provider");
    let client = common::client_from_providers(json!({
        "providers": [{
            "provider_type": "mcp",
            "name": "calculator",
            "command": "python3",
            "args": ["examples/mcp_stdio_server.py"]
        }]
    }))
    .await?;
    
    println!("✓ UTCP Client initialized with MCP stdio transport\n");
    
    // List available tools from the stdio MCP server
    println!("📋 Listing available tools:");
    let tools = client.search_tools("", 10).await?;
    for tool in &tools {
        println!("  • {}: {}", tool.name, tool.description);
    }
    
    // Example 3: Call a tool via stdio
    println!("\n⚡ Calling 'add' tool:");
    let mut args = HashMap::new();
    args.insert("a".to_string(), json!(5));
    args.insert("b".to_string(), json!(3));
    
    let result = client.call_tool("calculator.add", args).await?;
    println!("  Result: {}", serde_json::to_string_pretty(&result)?);
    
    // Example 4: Call another tool
    println!("\n⚡ Calling 'multiply' tool:");
    let mut args2 = HashMap::new();
    args2.insert("a".to_string(), json!(7));
    args2.insert("b".to_string(), json!(6));
    
    let result2 = client.call_tool("calculator.multiply", args2).await?;
    println!("  Result: {}", serde_json::to_string_pretty(&result2)?);
    
    println!("\n✨ Demo complete!");
    println!("\nNote: MCP stdio transport communicates with processes via stdin/stdout");
    println!("      This is useful for local tools, sandboxed environments, and");
    println!("      language-agnostic tool providers.");
    
    Ok(())
}
