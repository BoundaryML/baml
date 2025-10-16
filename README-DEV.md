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

### What is mise?

mise is a tool version manager that can handle multiple programming languages and tools in one place. It replaces the need for nvm, rbenv, pyenv, rustup, and other version managers.

### Configuration

Our tool versions are defined in `mise.toml`:

```toml
[tools]
rust = "1.88.0"
go = "1.23"
python = "3.12"
ruby = "3.2.2"
node = "lts"
# ... and more
```

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

## Manual Setup (Not Recommended)

If you prefer to install tools manually or need to understand what the setup script does:

### Required Tools

1. **Rust** (1.88.0)
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   rustup install 1.88.0
   rustup default 1.88.0
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
- Rust: `~/.local/share/mise/installs/rust/1.88.0/`
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