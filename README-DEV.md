# Development Setup Guide

This guide provides detailed instructions for setting up your BAML development environment.

## Quick Start

```bash
# Clone the repository
git clone https://github.com/BoundaryML/baml.git
cd baml

mise trust
pnpm setup-dev
pnpm clean:ws
pnpm install
pnpm typecheck
pnpm build

# Start developing!
pnpm dev
```

## Tool Management with mise

We use [mise](https://mise.jdx.dev/) (formerly rtx) as our polyglot tool version manager. This ensures all developers use the exact same versions of tools, preventing "works on my machine" issues.

### Toolchain

Three tools manage the dev environment:

- **mise** — versions of every tool except Rust, declared in `mise.toml`.
- **direnv** — loads `.envrc`, which activates mise and sets the build
  environment.
- **rustup** — the Rust toolchain. Rust is not in `mise.toml`; each workspace
  pins its own version in `rust-toolchain.toml` and rustup installs it on
  first use. See [Rust workspace toolchains](#rust-workspace-toolchains).

### What is mise?

mise is a tool version manager that can handle multiple programming languages and tools in one place. It replaces the need for nvm, rbenv, pyenv, and other version managers, except rustup.

### Configuration

Tool versions are declared in [`mise.toml`](./mise.toml); see that file for the
current set.

### Common mise Commands

```bash
# List all installed tools
mise list

# Install/update all tools to match mise.toml
mise install

# Show current tool versions
mise current

# Upgrade tools to latest versions (respecting version constraints)
mise upgrade

# Trust the configuration file (required after changes)
mise trust
```

### Rust workspace toolchains

The engine and BAML Language workspaces have independent Rust pins in `engine/rust-toolchain.toml` and `baml_language/rust-toolchain.toml`. Run commands that rely on rustup directory discovery from the appropriate workspace directory. Commands run from the repository root must select an explicit toolchain or use a setup action that provides one.

## Manual Setup (Not Recommended)

If you prefer to install tools manually or need to understand what the setup script does:

### Required Tools

1. **Rust** — install rustup; each workspace's `rust-toolchain.toml` pins
   the version.
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

2. **Go** (1.23)
   - Download from https://golang.org/dl/
   - Install protoc-gen-go: `go install google.golang.org/protobuf/cmd/protoc-gen-go@v1.36.6`

3. **Python** (3.12)
   - Install Python 3.12
   - Install uv: `curl -LsSf https://astral.sh/uv/install.sh | sh`

4. **Ruby** (3.2.2)
   - Install Ruby 3.2.2
   - Install bundler: `gem install bundler`

5. **Node.js** (LTS)
   - Install Node.js LTS
   - pnpm will be installed via mise

### Platform-Specific Dependencies

**macOS:**
```bash
brew install libyaml openssl@3
```

**Linux:**
Dependencies vary by distribution. The setup script will guide you.

## Development Workflow

### Running Everything

```bash
# Start all services with hot reloading
pnpm dev

# Run only specific components
pnpm dev:vscode       # VSCode extension
pnpm dev:playground   # Web playground
pnpm dev:language-server  # Language server
```

### Building Specific Components

After the TypeScript refactor, use these commands:

```bash
# Build everything
pnpm build

# Build specific apps
pnpm build:fiddle-web-app  # Web playground app
pnpm build:vscode         # VSCode extension
pnpm build:playground     # Playground package
pnpm build:cli           # CLI tool

# Release commands
pnpm release:fiddle-web-app  # Release web app
pnpm release:vscode         # Release VSCode extension
pnpm release:cli           # Release CLI
```

### TypeScript Project Structure

The TypeScript codebase follows a monorepo structure:

```
typescript/
├── apps/                    # All applications
│   ├── fiddle-web-app/     # Web playground
│   └── vscode-ext/         # VSCode extension
├── packages/               # All reusable packages
│   ├── ui/                # Shared UI components
│   ├── common/            # Common utilities
│   ├── playground-common/ # Playground shared code
│   ├── language-server/   # Language server
│   └── ...               # Other packages
└── workspace-tools/       # Build and config tools
```

### Running Tests

```bash
# Run all tests
./run-tests.sh

# Run specific language tests
cd integ-tests/typescript && pnpm test
cd integ-tests/python && uv run pytest
cd integ-tests/ruby && rake test
```

### Rust Workspace (`baml_language/`)

The compiler, engine, and LSP server live in the `baml_language/` Cargo
workspace. Common commands (run from `baml_language/`):

```bash
# Unit tests for the whole workspace (always run these after Rust changes)
cargo test --lib

# Unit tests for one crate
cargo test --lib -p bex_project

# Format with the repo's import style (fixes most lints)
cargo fmt -- --config imports_granularity="Crate" --config group_imports="StdExternalCrate"

# The LSP/runtime crates must also compile for the browser playground
cargo check --target wasm32-unknown-unknown -p bridge_wasm
```

Start with `baml_language/ARCHITECTURE.md` for the compiler pipeline and
`baml_language/TEST_INSTRUCTIONS.md` for the snapshot-test workflow. Design
documents for larger subsystems (e.g. the LSP server's locking and rebuild
pipeline) live in `docs/design/`.

### Building

```bash
# Build everything
pnpm build

# Build specific components
cargo build --release    # Rust components
pnpm build              # TypeScript components
```

## Troubleshooting

### mise Issues

**"mise: command not found"**
- The setup script installs mise to `~/.local/bin`. Make sure this is in your PATH.
- Try: `source ~/.bashrc` or `source ~/.zshrc`

**"mise trust required"**
- Run: `mise trust` in the project root

**Tool version conflicts**
- Run: `mise doctor` to diagnose issues
- Try: `mise install --force` to reinstall tools

### Language-Specific Issues

**Rust compilation errors**
- Ensure you're using the correct Rust version: `rustc --version`
- Clear cargo cache: `cargo clean`

**Go module errors**
- Clear module cache: `go clean -modcache`
- Ensure GOPATH is set correctly

**Python/uv issues**
- Clear uv cache: `uv cache clean`
- Reinstall dependencies: `uv sync --reinstall`

**Ruby/bundler issues**
- Clear bundler cache: `bundle clean --force`
- Reinstall gems: `bundle install --force`

### Getting Help

1. Check the [CONTRIBUTING.md](./CONTRIBUTING.md) guide
2. Search existing [GitHub issues](https://github.com/BoundaryML/baml/issues)
3. Ask in our [Discord #contributing channel](https://discord.gg/BTNBeXGuaS)

### IDE Setup

**VSCode:**
- Install recommended extensions when prompted
- mise tools will be automatically detected

**IntelliJ/RustRover:**
- Configure SDK paths to use mise-installed versions
- Go: `~/.local/share/mise/installs/go/1.23/`
- Rust: managed by rustup, not mise (`rustup show home`)
- Python: `~/.local/share/mise/installs/python/3.12/`
- Ruby: `~/.local/share/mise/installs/ruby/3.2.2/`

**Other IDEs:**
- Point to tool installations in `~/.local/share/mise/installs/`

## Keeping Your Environment Updated

When other developers update tool versions:

1. Pull the latest changes
2. Run: `mise install`
3. Restart your terminal/IDE if needed

The setup script can be run anytime to ensure your environment is up to date.
