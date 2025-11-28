# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.2]

### Added
- Apache 2.0 `LICENSE` file to make the MIT/Apache-2.0 licensing explicit
- CI runs `cargo-llvm-cov` and uploads an HTML coverage report artifact
- Regression tests covering plugin registration for call template handlers and custom communication protocols

### Changed
- Communication protocol registry now uses an internal `RwLock` so protocols can be registered globally without mutable access and snapshots stay thread-safe

### Fixed
- Strip provider prefixes before invoking GraphQL, HTTP stream, WebSocket, and SSE tools so prefixed tool names hit the correct endpoints

## [0.2.1]

### Added
- Call template handler registry so plugins can register new `call_template_type` converters
- Global communication protocol registry (`register_communication_protocol`) for pluggable protocols
- Unit tests covering provider-prefix stripping for HTTP stream, SSE, WebSocket, and GraphQL protocols

### Changed
- Transports renamed to communication protocols throughout the client API while keeping backward compatibility
- Documentation refreshed to describe the new registries and plugin extension points
- Dependency update: added `once_cell` for global registry initialization

### Fixed
- Strip provider prefixes when calling HTTP stream, SSE, WebSocket, and GraphQL protocols to avoid 404s (e.g., `http_stream_server` example)

## [0.1.8] - 2025-11-26

### Changed
- Version bumped to `1.0.0` to align with UTCP v1.0.0 specification
- Updated all examples to use v1.0 configuration format (`manual_call_templates` instead of `providers`)
- CLI transport: Fixed and improved implementation
- Code refactoring for better maintainability

### Added
- v1.0.0 UTCP specification support with automatic migration from v0.1
- `manual_call_templates` configuration option (replaces `providers`)
- `call_template_type` field (replaces `provider_type`)
- `UtcpClient::create()` async factory method
- `with_manual_path()` configuration helper
- Comprehensive migration documentation

## [0.1.7] - 2025-11-26

### Added
- Unit tests for all provider types
- Comprehensive test coverage for transports
- More tests for WebRTC, WebSocket, MCP, and other transports (#9)

### Changed
- Updated README with latest features and examples
- Improved documentation structure

### Fixed
- Various bug fixes and improvements

## [0.1.6] - 2025-11-26

### Added
- Orchestrator example demonstrating LLM integration (#7)
- Example showing codemode orchestration with Gemini

### Changed
- Updated Cargo.lock

## [0.1.5] - 2025-11-26

### Added
- OpenAPI converter for automatic tool discovery from OpenAPI specs (#6)
- Automatic conversion of OpenAPI endpoints to UTCP tools
- Support for OpenAPI 3.0 specification parsing

### Changed
- Improved codemode orchestrator with better error handling (#5)
- Enhanced LLM integration capabilities
- Updated Cargo.toml and Cargo.lock

## [0.1.4] - 2025-11-26

### Fixed
- Bug fixes in codemode orchestrator
- Improved error handling in orchestrator flow

## [0.1.2] - 2025-11-26

### Added
- MCP SSE (Server-Sent Events) transport support (#3)
- Streaming capabilities for MCP providers
- SSE event handling and parsing

### Changed
- Updated Cargo.lock
- Enhanced README documentation

## [0.1.1] - 2025-11-26

### Changed
- Refactored `UtcpClient` constructor (#2)
  - Unified constructor into single async `new` function
  - Automatic provider loading from config
  - Removed old synchronous constructor
  - Updated all examples to use new unified constructor
- Updated Cargo.lock
- Improved README documentation

## [0.1.0] - 2025-11-25

### Added
- Initial implementation of UTCP client for Rust
- Support for multiple transport protocols:
  - **HTTP**: Full HTTP provider with UTCP manifest support
  - **MCP (Model Context Protocol)**: stdio-based transport
  - **WebSocket**: Real-time bidirectional communication
  - **gRPC**: High-performance RPC protocol
  - **CLI**: Execute local binaries as tools
  - **GraphQL**: Query-based tool calling
  - **TCP/UDP**: Low-level network transports
  - **SSE**: Server-Sent Events for streaming
  - **WebRTC**: Peer-to-peer data channels with signaling
  - **HTTP Stream**: Streaming HTTP responses
  - **Text**: File-based tool provider
- **Codemode** scripting environment powered by Rhai
  - Execute Rhai scripts with access to registered tools
  - `call_tool()` and `call_tool_stream()` functions in scripts
  - Sandboxed execution environment
- **CodemodeOrchestrator** for LLM-driven workflows
  - 4-step orchestration: Decide → Select → Generate → Execute
  - Integration with LLM for dynamic script generation
  - Tool selection and discovery
- **Tag-based tool search strategy**
  - Semantic search across registered tools
  - Configurable search scoring
- **In-memory tool repository**
  - Fast tool lookup and management
  - Provider and tool registration
- **Configuration management**
  - JSON-based provider configuration
  - Auto-loading providers from file
  - Variable substitution support
  - Environment variable integration
- **Streaming support**
  - Stream results for applicable transports
  - Async stream handling with `StreamResult` trait
- **Comprehensive examples**
  - Basic usage examples
  - Provider demonstrations for all transports
  - Server examples for testing
  - Codemode evaluation examples
  - Orchestrator integration examples

### Infrastructure
- Build system with protocol buffer support (gRPC)
- Extensive test coverage (90+ tests)
- Example servers for testing various transports
- GitHub Actions CI/CD pipeline
- Comprehensive documentation and README

### Initial Commits (2025-11-25)
- `43d162c`: First commit - Project initialization
- `53ff07f`: Initialize rs-utcp structure
- `ff02864`: Add examples for all transports
- `45754a1`: Fix Cargo.toml configuration
- `7bd7d73`: Update README with usage instructions
- `039478d`: Refactoring session for code organization
- `de67148`: Update README documentation
- `03434b3`: Major refactoring of rs-utcp architecture
- `3135524`: Various fixes
- `972336b`: Additional improvements
- `95f1a02`: Add stdio MCP transport (#1)

## Project History

### Development Timeline
- **2025-11-25**: Project inception and initial development
  - Core architecture established
  - Multiple transport protocols implemented
  - Example infrastructure created
  
- **2025-11-26**: Feature expansion and testing
  - MCP SSE transport added
  - OpenAPI converter integration
  - Codemode orchestrator improvements
  - WebRTC transport enhancements
  - Comprehensive test coverage
  - Migration to v1.0.0 specification

### Key Milestones
- **v0.1.0** (2025-11-25): Initial release with 12 transport types and codemode support
- **v0.1.1** (2025-11-26): Unified async constructor API
- **v0.1.2** (2025-11-26): MCP SSE transport support
- **v0.1.4** (2025-11-26): Orchestrator bug fixes
- **v0.1.5** (2025-11-26): OpenAPI converter integration
- **v0.1.6** (2025-11-26): Orchestrator examples
- **v0.1.7** (2025-11-26): Comprehensive testing and documentation
- **v1.0.0** (In Progress): UTCP v1.0.0 specification alignment

## Migration Guide

For upgrading from v0.1.x to v1.0.0, the library provides automatic migration:
- Configuration files using `providers` are automatically converted to `manual_call_templates`
- All v0.1 code continues to work without changes
- See the official [UTCP Migration Guide](https://www.utcp.io/migration-v0.1-to-v1.0) for details

## Links

- **UTCP Specification**: https://www.utcp.io
- **Official Migration Guide**: https://www.utcp.io/migration-v0.1-to-v1.0
- **GitHub Repository**: https://github.com/universal-tool-calling-protocol/rs-utcp
- **Documentation**: https://docs.rs/rs-utcp

---

*Note: This changelog is generated from git history and follows the Keep a Changelog format.*
