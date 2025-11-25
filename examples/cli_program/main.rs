use std::collections::HashMap;
use std::env;
use std::sync::Arc;

use rs_utcp::{
    config::UtcpClientConfig, providers::cli::CliProvider,
    repository::in_memory::InMemoryToolRepository, tag::tag_search::TagSearchStrategy, tools::Tool,
    UtcpClient, UtcpClientInterface,
};
use serde_json::json;
use tokio::io::AsyncReadExt;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Support being invoked as the CLI provider itself
    if env::args().nth(1) == Some("--cli-mode".to_string()) {
        return run_cli_mode().await;
    }

    // Normal example: spawn self as CLI provider
    let self_path = env::current_exe()?;
    let command_name = format!("{} --cli-mode", self_path.display());

    let repo = Arc::new(InMemoryToolRepository::new());
    let search = Arc::new(TagSearchStrategy::new(repo.clone(), 1.0));
    let client = UtcpClient::new(UtcpClientConfig::default(), repo, search);

    let provider = CliProvider::new("cli_demo".into(), command_name, None);
    let tools = client.register_tool_provider(Arc::new(provider)).await?;
    println!(
        "Tools: {:?}",
        tools.iter().map(|t| &t.name).collect::<Vec<_>>()
    );

    let mut args = HashMap::new();
    args.insert("message".into(), serde_json::json!("hello cli"));
    let res: serde_json::Value = client.call_tool("cli_demo.echo", args).await?;
    println!("Result: {}", serde_json::to_string_pretty(&res)?);
    Ok(())
}

async fn run_cli_mode() -> anyhow::Result<()> {
    let mut args = env::args().skip(2).collect::<Vec<_>>();
    if args.is_empty() {
        // discovery mode: print manifest
        let tool = Tool {
            name: "echo".to_string(),
            description: "Echo args".to_string(),
            inputs: rs_utcp::tools::ToolInputOutputSchema {
                type_: "object".to_string(),
                properties: None,
                required: None,
                description: None,
                title: None,
                items: None,
                enum_: None,
                minimum: None,
                maximum: None,
                format: None,
            },
            outputs: rs_utcp::tools::ToolInputOutputSchema {
                type_: "object".to_string(),
                properties: None,
                required: None,
                description: None,
                title: None,
                items: None,
                enum_: None,
                minimum: None,
                maximum: None,
                format: None,
            },
            tags: vec!["cli".to_string()],
            average_response_size: None,
            provider: None,
        };
        println!("{}", json!({ "tools": [tool] }));
        return Ok(());
    }

    // call mode: call <provider> <tool>
    if args.len() < 2 {
        eprintln!("usage: <bin> --cli-mode call <provider> <tool>");
        std::process::exit(1);
    }
    let tool_name = args.pop().unwrap();
    let provider = args.pop().unwrap();
    if provider.is_empty() || tool_name.is_empty() {
        eprintln!("invalid call");
        std::process::exit(1);
    }

    // read args from stdin
    let mut stdin_data = String::new();
    tokio::io::stdin().read_to_string(&mut stdin_data).await?;
    let args_json: serde_json::Value = serde_json::from_str(&stdin_data).unwrap_or(json!({}));

    println!("{}", args_json);
    Ok(())
}
