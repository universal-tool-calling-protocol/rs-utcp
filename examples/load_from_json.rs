// Example: Loading providers from JSON file

use rs_utcp::config::UtcpClientConfig;
use rs_utcp::repository::in_memory::InMemoryToolRepository;
use rs_utcp::tag::tag_search::TagSearchStrategy;
use rs_utcp::{UtcpClient, UtcpClientInterface};
use std::path::PathBuf;
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("🚀 Loading UTCP Client from providers.json\n");

    // Create configuration with providers file
    let config = UtcpClientConfig::new()
        .with_providers_file(PathBuf::from("examples/providers.json"))
        .with_variable("API_KEY".to_string(), "my-secret-api-key".to_string());

    // Create repository and search strategy
    let repo = Arc::new(InMemoryToolRepository::new());
    let search = Arc::new(TagSearchStrategy::new(repo.clone(), 1.0));

    // Create client and auto-load providers from JSON
    let client = UtcpClient::new_with_providers(config, repo, search).await?;

    println!("\n📋 Listing all available tools:");
    match client.search_tools("", 100).await {
        Ok(tools) => {
            if tools.is_empty() {
                println!("  No tools found. The providers.json file may not contain valid tool definitions.");
            } else {
                for tool in &tools {
                    println!("  - {}", tool.name);
                    println!("    Description: {}", tool.description);
                    println!("    Tags: {:?}", tool.tags);
                    println!();
                }
            }
        }
        Err(e) => {
            eprintln!("  Error listing tools: {}", e);
        }
    }

    println!("✨ Demo complete!");

    Ok(())
}
