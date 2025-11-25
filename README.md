# rs-utcp

Rust client for the Universal Tool Calling Protocol (UTCP). It lets you discover and call tools across HTTP, CLI, WebSocket, gRPC, GraphQL, TCP/UDP, SSE, MCP, WebRTC, HTTP stream, and text providers with a single API.

## Quick start

The fastest way to see UTCP working is to run the bundled demo that spins up a mock HTTP provider and loads it through `new_with_providers`:

```bash
cargo run --example basic_usage
```

You should see a local provider start, tools listed, and a sample tool call printed.

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
// Load providers from a JSON file (see providers.json for shape).
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

`providers.json` supports multiple transports. This is enough for a simple HTTP provider:

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

Variables are substituted from `UtcpClientConfig::variables` or the environment.

## Example gallery

- `cargo run --example basic_usage` — spin up a local HTTP provider, load via `new_with_providers`, call a tool.
- `cargo run --example load_from_json` — load providers directly from `examples/providers.json`.
- `cargo run --example http_server` — start a demo HTTP provider and call it.
- `cargo run --example websocket_server` / `sse_server` / `tcp_server` / `udp_server` / `http_stream_server` / `grpc_server` / `graphql_server` / `mcp_server` — self-hosted provider for each transport, then call it.
- `cargo run --example cli_program` — treat the binary as its own CLI provider.
- `cargo run --example codemode_eval` — evaluate Rust-like snippets (via Rhai) that can call UTCP tools.
- `cargo run --example all_providers` — env-driven sampler for every transport (set `DEMO_*` vars to enable blocks).

## Codemode (Rhai-based snippets)

Codemode lets you orchestrate UTCP tools from a Rust-like DSL powered by Rhai. Helpers available inside snippets:

- `call_tool("<provider.tool>", #{...args})` — invoke a UTCP tool and return JSON.
- `search_tools("<query>", <limit>)` — find tools by name/description/tags.
- `sprintf("hello {}", [value])` — string formatting helper.

Example snippet (from `examples/codemode_eval/main.rs`):

```text
let a = 2 + 3;
let echo = call_tool("http_demo.echo", #{"message": "hi"});
echo
```

Run it end-to-end (spins up a demo HTTP provider under the hood):

```bash
cargo run --example codemode_eval
```

## Why UTCP here?

- One client, many transports: providers describe how to talk to tools; transports handle the protocol details.
- Discovery + invocation: load providers, search by tags/descriptions, call tools or stream responses.
- Config-driven: `new_with_providers` reads JSON/YAML-like provider configs so you can ship endpoints without code changes.

## Project layout

- `src/lib.rs` — `UtcpClient` plus `UtcpClientInterface`.
- `src/providers/` — provider definitions (HTTP, CLI, WS, gRPC, GraphQL, TCP, UDP, SSE, MCP, WebRTC, HTTP stream, text).
- `src/transports/` — transport implementations matching provider types.
- `src/repository/` — tool repository abstraction + in-memory impl.
- `src/tag/` — tag-based search (`TagSearchStrategy`).
- `examples/` — runnable demos; most spin up their own provider servers.

## Development

Prereqs: Rust toolchain + `protoc` (for gRPC example builds).

Common tasks:

- Format: `cargo fmt`
- Lint/check examples: `cargo check --examples`
- Tests: `cargo test`
- Run a demo: `cargo run --example http_server` (or any from the gallery above)

## Status

- HTTP is feature-complete; other transports are usable demo skeletons and ready for extension.
- Authentication helpers exist (API key, basic, OAuth2 scaffolding).
- OpenAPI/spec generation is on the roadmap.

## License

TBD — add your license file and update this section.

## Credits

Based on the [go-utcp](https://github.com/universal-tool-calling-protocol/go-utcp) project.
