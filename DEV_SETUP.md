# Development Setup with Hot Reloading

This guide explains how to set up a development environment with hot reloading for both Rust and TypeScript/JavaScript files.

## Quick Setup

Run the setup script to install all development dependencies:

```bash
pnpm setup:dev
```

This will:
- Install `cargo-watch` for Rust hot reloading
- Install `wasm-bindgen-cli` for WASM builds
- Add the `wasm32-unknown-unknown` target
- Install all Node dependencies
- Build the BAML CLI

## Manual Setup (if needed)

If you prefer to set up manually or the script fails:

1. **Install cargo-watch** for Rust hot reloading:
   ```bash
   cargo install cargo-watch
   ```

2. **Install wasm-bindgen-cli**:
   ```bash
   cargo install wasm-bindgen-cli --version 0.2.92
   ```

3. **Add WASM target**:
   ```bash
   rustup target add wasm32-unknown-unknown
   ```

4. **Install dependencies**:
   ```bash
   pnpm install
   ```

## Development Commands

### Full Development Mode (Recommended)

To run everything with hot reloading:

```bash
pnpm dev
```

This runs all dev tasks in parallel, including:
- Rust CLI with hot reloading
- Rust Language Server with hot reloading
- TypeScript/JavaScript file watching
- Vite dev servers

### Specific Development Modes

#### VSCode Extension Development

To develop the VSCode extension with all its dependencies:

```bash
pnpm dev:vscode-full
```

This runs:
- `@baml/cli` - Rust CLI with cargo-watch
- `@baml/language-server` - Rust Language Server with cargo-watch
- `baml-extension` - VSCode extension with TypeScript watching

#### Individual Package Development

```bash
# Just the VSCode extension (TypeScript only)
pnpm dev:vscode

# Just the language server (Rust)
pnpm dev:language-server

# Just the playground
pnpm dev:playground

# Everything needed for VSCode development
pnpm dev:all
```

## How It Works

### Rust Hot Reloading

The Rust packages (`@baml/cli` and `@baml/language-server`) use `cargo-watch` to:
1. Watch for changes in `.rs` files
2. Automatically rebuild when changes are detected
3. For the CLI, also copy the binary to the `bin/` directory

### TypeScript/JavaScript Hot Reloading

- VSCode extension uses `tsup --watch`
- Playground uses Vite's built-in HMR
- All watching is coordinated through Turbo

### Port Configuration

- **Vite Dev Server**: Port 5173 (playground development)
- **BAML Playground Server**: Port 3030 (language server's embedded playground)
- **Proxy Server**: Port 3031 (for handling CORS)

## Debugging VSCode Extension

1. Run the dev command:
   ```bash
   pnpm dev:vscode-full
   ```

2. In VSCode, press `F5` or go to Run > Start Debugging

3. A new VSCode window will open with the extension loaded

4. Make changes to:
   - **Rust files**: Will auto-rebuild and update the CLI/Language Server
   - **TypeScript files**: Will auto-rebuild the extension
   - **Playground files**: Will hot-reload in the browser

5. Reload the extension host window (`Cmd+R` / `Ctrl+R`) to see Rust changes take effect

## Troubleshooting

### cargo-watch not found

Install it with:
```bash
cargo install cargo-watch
```

### Port conflicts

If you get port conflicts:
- Vite runs on 5173
- Language server playground runs on 3030
- Make sure nothing else is using these ports

### Rust changes not reflecting

After Rust rebuilds, you may need to:
1. Reload the VSCode extension host window
2. Restart the language server: Command Palette > "BAML: Restart Language Server"

## Architecture

```
pnpm dev
├── @baml/cli (cargo watch)
│   └── Watches: engine/cli/src/**/*.rs
│   └── Outputs: engine/cli/bin/baml-cli
├── @baml/language-server (cargo watch)
│   └── Watches: engine/language_server/src/**/*.rs
│   └── Outputs: engine/target/debug/baml-language-server
├── baml-extension (tsup watch)
│   └── Watches: typescript/apps/vscode-ext/src/**/*.ts
│   └── Outputs: typescript/apps/vscode-ext/dist/
└── @baml/playground (vite)
    └── Watches: typescript/apps/playground/src/**/*
    └── Serves: http://localhost:5173
```

This setup ensures that any changes to Rust or TypeScript code are automatically rebuilt and available for testing!