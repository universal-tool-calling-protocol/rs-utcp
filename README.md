# rs-utcp

Rust client for the Universal Tool Calling Protocol (UTCP). Discover and call tools across HTTP, CLI, WebSocket, gRPC, GraphQL, TCP/UDP, SSE, MCP, WebRTC, HTTP stream, and text providers with one API.

## Why use this client?

- One interface for many transports — providers describe endpoints, transports handle protocols.
- Discovery + invocation — load providers, search by tags/descriptions, call tools or stream responses.
- Config-driven — `new_with_providers` loads JSON provider manifests so you can ship endpoints without recompiling.
- Codemode — generate and run Rhai snippets that orchestrate tools; includes an optional LLM-driven orchestrator.

## Install / prereqs

- Rust toolchain (1.70+ recommended)
- `protoc` if you want to build the gRPC example

Add to your project (path/git if not published):

```bash
cargo add rs-utcp
```

## Quick start

Run the bundled demo that spins up a mock HTTP provider and loads it via `new_with_providers`:

```bash
cargo run --example basic_usage
```

You’ll see the provider start, tools listed, and a sample tool call.

## Minimal client setup

```rust
use rs_utcp::{
    config::UtcpClientConfig,
    repository::in_memory::InMemoryToolRepository,
    tag::tag_search::TagSearchStrategy,
    UtcpClient, UtcpClientInterface,
};
use std::sync::Arc;

# #[tokio::main]
# async fn main() -> anyhow::Result<()> {
let config = UtcpClientConfig::new().with_providers_file("examples/providers.json".into());
let repo = Arc::new(InMemoryToolRepository::new());
let search = Arc::new(TagSearchStrategy::new(repo.clone(), 1.0));
let client = UtcpClient::new_with_providers(config, repo, search).await?;

let tools = client.search_tools("echo", 10).await?;
println!("Found tools: {:?}", tools.iter().map(|t| &t.name).collect::<Vec<_>>());
# Ok(())
# }
```

### Provider JSON example

```json
{
  "providers": [
    {
      "provider_type": "http",
      "name": "demo",
      "url": "https://example.com/tools",
      "http_method": "POST",
      "headers": { "Authorization": "Bearer ${API_KEY}" }
    }
  ]
}
```

Variables are substituted from `UtcpClientConfig::variables` or environment variables.

## Example gallery

- `cargo run --example basic_usage` — spin up a local HTTP provider, load via `new_with_providers`, call a tool.
- `cargo run --example load_from_json` — load providers from `examples/providers.json`.
- `cargo run --example http_server` — demo HTTP provider + client.
- `cargo run --example websocket_server` / `sse_server` / `tcp_server` / `udp_server` / `http_stream_server` / `grpc_server` / `graphql_server` / `mcp_server` — self-hosted provider for each transport.
- `cargo run --example cli_program` — treat the binary as its own CLI provider.
- `cargo run --example codemode_eval` — evaluate Rust-like snippets (Rhai) that can call UTCP tools.
- `cargo run --example all_providers` — env-driven sampler for every transport (set `DEMO_*` vars).

## Codemode

Rhai-powered orchestration of UTCP tools:

- Helpers inside snippets: `call_tool("<provider.tool>", #{...})`, `call_tool_stream`, `search_tools`, `sprintf`.
- Example snippet:
  ```text
  let echo = call_tool("http_demo.echo", #{"message": "hi"});
  echo
  ```
- Run the demo: `cargo run --example codemode_eval`

LLM-driven orchestrator: implement `LlmModel::complete(prompt) -> String`, wire it into `CodemodeOrchestrator`, and it will (1) decide if tools are needed, (2) pick tools, (3) ask the model to emit a snippet, (4) execute via CodeMode.

## Architecture notes

- `UtcpClient` caches provider tools and resolved tool → transport bindings for fast calls.
- Transports live in `src/transports/` (http, cli, ws, grpc, graphql, tcp, udp, sse, mcp, webrtc, http_stream, text).
- Providers mirror transports in `src/providers/`.
- Repository abstraction in `src/repository/` (in-memory default).
- Tag-based search in `src/tag/`.

## Development

- Format: `cargo fmt`
- Check: `cargo check --examples`
- Tests: `cargo test`
- Try a demo: `cargo run --example http_server` (or any from the gallery)

## Status

- HTTP is feature-complete; other transports are demo-ready skeletons.
- Auth helpers exist (API key, basic, OAuth2 scaffolding).
- OpenAPI/spec generation is planned.

## License

TBD — add your license file and update this section.

## Credits

Based on the [go-utcp](https://github.com/universal-tool-calling-protocol/go-utcp) project.
