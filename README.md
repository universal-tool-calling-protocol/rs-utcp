# rs-utcp

Hey! 👋 This is the Rust client for the **Universal Tool Calling Protocol (UTCP)**.

Basically, I wanted a way to discover and call tools across a whole bunch of different protocols—HTTP, CLI, WebSocket, gRPC, MCP, you name it—without having to write custom glue code for every single one. This library gives you a **single, unified API** to handle all of that.

It's heavily inspired by the [go-utcp](https://github.com/universal-tool-calling-protocol/go-utcp) project, but built from the ground up for Rust. 🦀

## Why use this?

*   **One API for everything**: You don't care if a tool is a local Python script, a remote gRPC service, or an MCP server. You just ask for the tool by name, and `rs-utcp` handles the transport.
*   **Config-driven**: You can load your tool providers from a JSON file. This is huge because it means you can add or change endpoints without recompiling your app.
*   **Codemode**: This is the really cool part. 🚀 It includes a scripting environment (powered by [Rhai](https://rhai.rs/)) that lets you orchestrate complex workflows. You can even hook up an LLM to generate these scripts on the fly.

## Quick Start

First, add it to your project:

```bash
cargo add rs-utcp
```

(Or clone it locally if you're hacking on it).

### Try the demo

I've included a bunch of examples to get you started. The easiest way to see it in action is the basic usage demo:

```bash
cargo run --example basic_usage
```

This spins up a mock HTTP provider and shows you how to call a tool.

### Minimal Setup

Here's what it looks like to use it in your code:

```rust
use rs_utcp::{
    config::UtcpClientConfig,
    repository::in_memory::InMemoryToolRepository,
    tag::tag_search::TagSearchStrategy,
    UtcpClient, UtcpClientInterface,
};
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. Load your providers (or define them in code)
    let config = UtcpClientConfig::new().with_providers_file("examples/providers.json".into());
    
    // 2. Set up the repo and search strategy
    let repo = Arc::new(InMemoryToolRepository::new());
    let search = Arc::new(TagSearchStrategy::new(repo.clone(), 1.0));
    
    // 3. Create the client
    let client = UtcpClient::new(config, repo, search).await?;

    // 4. Find and use tools!
    let tools = client.search_tools("echo", 10).await?;
    println!("Found tools: {:?}", tools.iter().map(|t| &t.name).collect::<Vec<_>>());
    
    Ok(())
}
```

## Supported Transports

We support a lot of protocols out of the box. Some are more mature than others, but here's the list:

*   **HTTP** (The most battle-tested)
*   **MCP** (Model Context Protocol - supports both stdio and SSE!)
*   **WebSocket**
*   **gRPC**
*   **CLI** (Run local binaries as tools)
*   **GraphQL**
*   **TCP / UDP**
*   **SSE** (Server-Sent Events)
*   **WebRTC** (Experimental)

Check out the `examples/` folder for a working server/client demo of almost every transport.

## Codemode & Orchestration

If you want to get fancy, you can use "Codemode". It allows you to execute Rhai scripts that have access to your registered tools.

```rust
// Inside a Rhai script
let result = call_tool("http_demo.echo", #{"message": "Hello from Rhai!"});
print(result);
```

You can run the evaluator demo to play with this:
```bash
cargo run --example codemode_eval
```

## Status

*   **HTTP**: Solid and feature-complete.
*   **MCP**: Working well (stdio & SSE).
*   **Others**: Mostly functional skeletons. They work for the happy path, but might need some hardening.

If you find a bug or want to add a new transport, PRs are super welcome!

## Development

*   **Format**: `cargo fmt`
*   **Check**: `cargo check --examples`
*   **Test**: `cargo test`

## License

MIT (or whichever license you prefer, just update this).
