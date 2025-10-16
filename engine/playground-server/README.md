# Playground Server

This crate provides shared playground server infrastructure for BAML, eliminating code duplication between the language server and standalone playground implementations.

## What's Included

### Library (`playground-server`)

The core library provides shared infrastructure for playground servers:

- **Generic PlaygroundServer**: Type-safe server implementation that works with any message type
- **WebSocket Handlers**: Generic WebSocket RPC and message handlers 
- **Asset Management**: GitHub release asset downloading and verification
- **Port Management**: Configurable port allocation for playground and proxy servers
- **Shared Types**: Common message types and state structures

### Binary (`playground-barebones`)

A standalone test/repro server for testing playground functionality:

```bash
cargo run --bin playground-barebones
```

This binary:
- Starts a playground server on port 3900-3999 range
- Provides an interactive CLI for sending test messages
- Includes BAML runtime integration for full testing
- Serves as a minimal reproduction environment

## Architecture

The library eliminates ~800 lines of code duplication by extracting common patterns:

```
Before:                          After:
┌─────────────────┐             ┌──────────────────┐
│ language_server │             │ playground-server│
│   playground2/  │────────────▶│    (library)     │
│   - server.rs   │             │                  │
│   - handlers/   │             └──────────────────┘
│   - types       │                       ▲
└─────────────────┘                       │
                                          │
┌─────────────────┐                       │
│   barebones     │             ┌─────────┴────────┐
│   - server.rs   │────────────▶│playground-barebones│
│   - handlers/   │             │    (binary)      │
│   - types       │             └──────────────────┘
└─────────────────┘
```

## Usage

### As a Library

```rust
use playground_server::{
    PlaygroundServer, GitHubReleaseAssetManager, 
    AppState, PortConfiguration, pick_ports
};

let port_picks = pick_ports(PortConfiguration {
    base_port: 3700,
    max_attempts: 100,
}).await?;

let server = PlaygroundServer {
    app_state: AppState { /* ... */ },
    asset_manager: GitHubReleaseAssetManager {
        github_repo: "BoundaryML/baml",
        version_env_var: "CARGO_PKG_VERSION",
    },
};

server.run(port_picks.playground_listener).await?;
```

### Generic Design

The server is generic over message types and asset management:

```rust
impl<T, A> PlaygroundServer<T, A>
where
    T: Send + Sync + Clone + serde::Serialize + 'static,
    A: AssetManager + 'static,
{
    pub async fn run(self, listener: TcpListener) -> Result<(), Box<dyn Error + Send>>
}
```

This allows different implementations to use:
- Different message types (LSP messages, custom types, etc.)
- Different asset sources (GitHub releases, local files, etc.)
- Different port ranges and configurations

## Integration

- **Language Server**: Uses this library for `playground2` integration with LSP
- **Standalone Testing**: The `playground-barebones` binary provides a minimal test environment
- **Custom Implementations**: Can build on this library for other playground use cases