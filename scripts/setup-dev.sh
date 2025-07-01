#!/bin/bash
set -e

echo "🚀 Setting up BAML development environment..."

# Colors for output
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Parse command line arguments
SKIP_PNPM=false
SKIP_CARGO_WATCH=false
SKIP_RUST=false
SKIP_GO=false
SKIP_PYTHON=false

for arg in "$@"; do
    case $arg in
        --skip-pnpm)
            SKIP_PNPM=true
            shift
            ;;
        --skip-cargo-watch)
            SKIP_CARGO_WATCH=true
            shift
            ;;
        --skip-rust)
            SKIP_RUST=true
            shift
            ;;
        --skip-go)
            SKIP_GO=true
            shift
            ;;
        --skip-python)
            SKIP_PYTHON=true
            shift
            ;;
        --help|-h)
            echo "Usage: $0 [OPTIONS]"
            echo "Options:"
            echo "  --skip-pnpm         Skip pnpm installation"
            echo "  --skip-cargo-watch  Skip cargo-watch installation"
            echo "  --skip-rust         Skip Rust/Cargo installation"
            echo "  --skip-go           Skip Go installation"
            echo "  --skip-python       Skip Python/uv/ruff installation"
            echo "  --help, -h          Show this help message"
            exit 0
            ;;
        *)
            # Unknown option
            echo "Unknown option: $arg"
            echo "Use --help for usage information"
            exit 1
            ;;
    esac
done

# Check if cargo is installed
if [ "$SKIP_RUST" = false ]; then
    if ! command -v cargo &> /dev/null; then
        echo -e "${YELLOW}⚠️  Rust/Cargo is not installed. Installing Rust...${NC}"

        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain 1.85.0

        # Source cargo environment from the correct location
        if [ -n "$HOME" ]; then
            source $HOME/.cargo/env
        fi

        echo -e "${GREEN}✅ Rust installed successfully${NC}"
    else
        echo -e "${GREEN}✅ Rust/Cargo already installed${NC}"
    fi
else
    echo -e "${YELLOW}⏭️  Skipping Rust installation${NC}"
fi

# Check if Go is installed
if [ "$SKIP_GO" = false ]; then
    if ! command -v go &> /dev/null; then
        echo -e "${YELLOW}⚠️  Go is not installed. Installing Go 1.24...${NC}"

        # Determine architecture and OS for Go installation
        OS=$(uname -s | tr '[:upper:]' '[:lower:]')
        ARCH=$(uname -m)

        case "$ARCH" in
            x86_64) ARCH="amd64" ;;
            aarch64|arm64) ARCH="arm64" ;;
            *)
                echo -e "${YELLOW}⚠️  Unsupported architecture: $ARCH. Please install Go manually.${NC}"
                exit 1
                ;;
        esac

        GO_VERSION="1.23"
        GO_TARBALL="go${GO_VERSION}.${OS}-${ARCH}.tar.gz"
        GO_URL="https://golang.org/dl/${GO_TARBALL}"

        # Download and install Go
        echo -e "${YELLOW}📦 Downloading Go ${GO_VERSION}...${NC}"
        curl -L "$GO_URL" -o "/tmp/${GO_TARBALL}"

        # Remove any existing Go installation
        sudo rm -rf /usr/local/go

        # Extract Go
        sudo tar -C /usr/local -xzf "/tmp/${GO_TARBALL}"

        # Clean up
        rm "/tmp/${GO_TARBALL}"

        # Add Go to PATH if not already there
        if ! echo "$PATH" | grep -q "/usr/local/go/bin"; then
            echo 'export PATH=$PATH:/usr/local/go/bin' >> ~/.bashrc
            echo 'export PATH=$PATH:/usr/local/go/bin' >> ~/.zshrc 2>/dev/null || true
            export PATH=$PATH:/usr/local/go/bin
        fi

        echo -e "${GREEN}✅ Go installed successfully${NC}"
    else
        echo -e "${GREEN}✅ Go already installed${NC}"
                # Check if it's the right version
        GO_VERSION=$(go version | cut -d' ' -f3 | sed 's/go//')
        REQUIRED_VERSION="1.23"

        # Compare versions (only warn if current version is older than required)
        # Using sort -V to compare versions properly
        if [[ "$(printf '%s\n%s\n' "$REQUIRED_VERSION" "$GO_VERSION" | sort -V | head -n1)" == "$GO_VERSION" ]] && [[ "$GO_VERSION" != "$REQUIRED_VERSION"* ]]; then
            echo -e "${YELLOW}⚠️  Go version is $GO_VERSION, but project requires $REQUIRED_VERSION or newer. Consider updating.${NC}"
        fi
    fi

    # Install protoc-gen-go if not already installed
    if command -v go &> /dev/null; then
        if ! command -v protoc-gen-go &> /dev/null; then
            echo -e "${YELLOW}📦 Installing protoc-gen-go...${NC}"
            go install github.com/golang/protobuf/protoc-gen-go@latest
            echo -e "${GREEN}✅ protoc-gen-go installed${NC}"
        else
            echo -e "${GREEN}✅ protoc-gen-go already installed${NC}"
        fi

        if ! command -v goimports &> /dev/null; then
            echo -e "${YELLOW}📦 Installing goimports...${NC}"
            go install golang.org/x/tools/cmd/goimports@latest
            echo -e "${GREEN}✅ goimports installed${NC}"
        else
            echo -e "${GREEN}✅ goimports already installed${NC}"
        fi

        # Ensure Go bin directory is in PATH
        GOPATH_BIN="$(go env GOPATH)/bin"
        if ! echo "$PATH" | grep -q "$GOPATH_BIN"; then
            echo -e "${YELLOW}📦 Adding Go bin directory to PATH...${NC}"
            echo "export PATH=\$PATH:\$(go env GOPATH)/bin" >> ~/.bashrc
            echo "export PATH=\$PATH:\$(go env GOPATH)/bin" >> ~/.zshrc 2>/dev/null || true
            export PATH=$PATH:$GOPATH_BIN
            echo -e "${GREEN}✅ Go bin directory added to PATH${NC}"
        else
            echo -e "${GREEN}✅ Go bin directory already in PATH${NC}"
        fi
    fi
else
    echo -e "${YELLOW}⏭️  Skipping Go installation${NC}"
fi

# Check if Python tooling is installed
if [ "$SKIP_PYTHON" = false ]; then
    # Install uv (modern Python package manager) if not already installed
    if ! command -v uv &> /dev/null; then
        echo -e "${YELLOW}📦 Installing uv (Python package manager)...${NC}"
        curl -LsSf https://astral.sh/uv/install.sh | sh

        # Source uv environment
        if [ -n "$HOME" ]; then
            export PATH="$HOME/.cargo/bin:$PATH"
        fi

        echo -e "${GREEN}✅ uv installed successfully${NC}"
    else
        echo -e "${GREEN}✅ uv already installed${NC}"
    fi

    # Install Python project dependencies including ruff
    if command -v uv &> /dev/null; then
        echo -e "${YELLOW}📦 Installing Python project dependencies (including ruff)...${NC}"

                # Install dependencies from integ-tests/python pyproject.toml
        if [ -f "integ-tests/python/pyproject.toml" ]; then
            cd integ-tests/python
            uv sync --dev

            # Install ruff as a global tool using the version from pyproject.toml
            if ! command -v ruff &> /dev/null; then
                uv tool install ruff
            fi
            cd - > /dev/null

            # Ensure uv tools directory is in PATH
            UV_TOOLS_BIN="$HOME/.local/bin"
            if ! echo "$PATH" | grep -q "$UV_TOOLS_BIN"; then
                echo -e "${YELLOW}📦 Adding uv tools directory to PATH...${NC}"
                echo "export PATH=\$PATH:$UV_TOOLS_BIN" >> ~/.bashrc
                echo "export PATH=\$PATH:$UV_TOOLS_BIN" >> ~/.zshrc 2>/dev/null || true
                export PATH=$PATH:$UV_TOOLS_BIN
                echo -e "${GREEN}✅ uv tools directory added to PATH${NC}"
            fi

            echo -e "${GREEN}✅ Python dependencies and ruff installed${NC}"
        else
            echo -e "${YELLOW}⚠️  integ-tests/python/pyproject.toml not found, skipping Python dependencies${NC}"
        fi
    fi
else
    echo -e "${YELLOW}⏭️  Skipping Python tooling installation${NC}"
fi

# Check if pnpm is installed
if [ "$SKIP_PNPM" = false ]; then
    if ! command -v pnpm &> /dev/null; then
        echo -e "${YELLOW}⚠️  pnpm is not installed. Installing pnpm...${NC}"
        npm install -g pnpm
        echo -e "${GREEN}✅ pnpm installed successfully${NC}"
    fi
else
    echo -e "${YELLOW}⏭️  Skipping pnpm installation${NC}"
fi

# Install cargo-watch if not already installed
if [ "$SKIP_CARGO_WATCH" = false ]; then
    if ! command -v cargo-watch &> /dev/null; then
        echo -e "${YELLOW}📦 Installing cargo-watch for Rust hot reloading...${NC}"
        cargo install cargo-watch
        echo -e "${GREEN}✅ cargo-watch installed${NC}"
    else
        echo -e "${GREEN}✅ cargo-watch already installed${NC}"
    fi
else
    echo -e "${YELLOW}⏭️  Skipping cargo-watch installation${NC}"
fi

# Install wasm-pack if not already installed (needed for building Rust WASM packages)
# Note: wasm-pack automatically manages wasm-bindgen-cli internally
if [ "$SKIP_RUST" = false ]; then
    if ! command -v wasm-pack &> /dev/null; then
        echo -e "${YELLOW}📦 Installing wasm-pack...${NC}"
        cargo install wasm-pack --version 0.13.1
        echo -e "${GREEN}✅ wasm-pack installed${NC}"
    else
        echo -e "${GREEN}✅ wasm-pack already installed${NC}"
    fi

    # Install cross-rs for cross-compilation
    if ! command -v cross &> /dev/null; then
        echo -e "${YELLOW}📦 Installing cross-rs for cross-compilation...${NC}"
        cargo install cross --git https://github.com/cross-rs/cross
        echo -e "${GREEN}✅ cross-rs installed${NC}"
    else
        echo -e "${GREEN}✅ cross-rs already installed${NC}"
    fi
else
    echo -e "${YELLOW}⏭️  Skipping Rust tools installation (Rust installation was skipped)${NC}"
fi

echo ""
echo -e "${GREEN}🎉 Development environment setup complete!${NC}"
echo ""
echo "You can now run:"
echo "  pnpm dev              # Run everything with hot reloading"
echo "  pnpm dev:vscode-full  # Run VSCode extension with all dependencies"
echo "  pnpm dev:playground   # Run just the playground"
echo ""
echo "For VSCode extension debugging:"
echo "  1. Run 'pnpm dev:vscode-full'"
echo "  2. Press F5 in VSCode to launch the extension host"