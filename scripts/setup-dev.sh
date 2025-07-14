#!/bin/bash
set -e

echo "🚀 Setting up BAML development environment with mise..."

# Colors for output
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

# Check if mise is installed
if ! command -v mise &> /dev/null; then
    echo -e "${YELLOW}📦 Installing mise (tool version manager)...${NC}"

    # Install mise
    curl https://mise.jdx.dev/install.sh | sh

    # Add mise to PATH for current session
    export PATH="$HOME/.local/bin:$PATH"

    echo -e "${GREEN}✅ mise installed successfully${NC}"
else
    echo -e "${GREEN}✅ mise already installed${NC}"
fi

# Trust the mise config file
echo -e "${YELLOW}📦 Trusting mise configuration...${NC}"
mise trust

# Install all tools defined in mise.toml
echo -e "${YELLOW}📦 Installing all development tools...${NC}"
mise install

# Additional setup for platform-specific dependencies
if command -v brew &> /dev/null; then
    # macOS dependencies for Ruby
    if ! brew list libyaml &> /dev/null; then
        echo -e "${YELLOW}📦 Installing libyaml (required for Ruby psych extension)...${NC}"
        brew install libyaml
        echo -e "${GREEN}✅ libyaml installed${NC}"
    fi

    if ! brew list openssl@3 &> /dev/null; then
        echo -e "${YELLOW}📦 Installing openssl@3 (required for Ruby)...${NC}"
        brew install openssl@3
        echo -e "${GREEN}✅ openssl@3 installed${NC}"
    fi
fi

# Install Python project dependencies
if [ -f "integ-tests/python/pyproject.toml" ] && command -v uv &> /dev/null; then
    echo -e "${YELLOW}📦 Installing Python project dependencies...${NC}"
    cd integ-tests/python
    uv sync --dev
    cd - > /dev/null
    echo -e "${GREEN}✅ Python dependencies installed${NC}"
fi

# Install Ruby bundler
if command -v ruby &> /dev/null; then
    if ! gem list bundler -i &> /dev/null; then
        echo -e "${YELLOW}📦 Installing bundler...${NC}"
        gem install bundler
        echo -e "${GREEN}✅ bundler installed${NC}"
    fi
fi

# Install node dependencies
if command -v pnpm &> /dev/null; then
    echo -e "${YELLOW}📦 Installing node dependencies...${NC}"
    pnpm install
    echo -e "${GREEN}✅ node dependencies installed${NC}"
fi

# Add mise activation commands to shell config files if not already present
# Create activation snippet that checks for mise availability
MISE_BASH_ACTIVATION='# mise activation
if command -v mise &> /dev/null; then
    eval "$(mise activate bash --shims)"
    eval "$(mise activate bash)"
elif [ -f ~/.local/bin/mise ]; then
    export PATH="$HOME/.local/bin:$PATH"
    eval "$(~/.local/bin/mise activate bash --shims)"
    eval "$(~/.local/bin/mise activate bash)"
fi'

MISE_ZSH_ACTIVATION='# mise activation
if command -v mise &> /dev/null; then
    eval "$(mise activate zsh --shims)"
    eval "$(mise activate zsh)"
elif [ -f ~/.local/bin/mise ]; then
    export PATH="$HOME/.local/bin:$PATH"
    eval "$(~/.local/bin/mise activate zsh --shims)"
    eval "$(~/.local/bin/mise activate zsh)"
fi'

# Check and add to .bashrc if not already present
if [ -f ~/.bashrc ]; then
    # Check if any mise activation exists
    if ! grep -q 'mise activate' ~/.bashrc; then
        echo "$MISE_BASH_ACTIVATION" >> ~/.bashrc
        echo -e "${GREEN}✅ Added mise activation to ~/.bashrc${NC}"
    fi
fi

# Check and add to .zshrc if not already present
if [ -f ~/.zshrc ]; then
    # Check if any mise activation exists
    if ! grep -q 'mise activate' ~/.zshrc; then
        echo "$MISE_ZSH_ACTIVATION" >> ~/.zshrc
        echo -e "${GREEN}✅ Added mise activation to ~/.zshrc${NC}"
    fi
fi

# For the current session, activate with shims if mise is available
if command -v mise &> /dev/null; then
    # Detect current shell and activate accordingly
    if [ -n "$BASH_VERSION" ]; then
        eval "$(mise activate bash --shims)"
        eval "$(mise activate bash)"
    elif [ -n "$ZSH_VERSION" ]; then
        eval "$(mise activate zsh --shims)"
        eval "$(mise activate zsh)"
    fi
elif [ -f ~/.local/bin/mise ]; then
    # If mise is installed but not in PATH yet, use the full path
    export PATH="$HOME/.local/bin:$PATH"
    if [ -n "$BASH_VERSION" ]; then
        eval "$(~/.local/bin/mise activate bash --shims)"
        eval "$(~/.local/bin/mise activate bash)"
    elif [ -n "$ZSH_VERSION" ]; then
        eval "$(~/.local/bin/mise activate zsh --shims)"
        eval "$(~/.local/bin/mise activate zsh)"
    fi
fi

# Verify installations
echo ""
echo -e "${GREEN}🔍 Verifying installations:${NC}"
echo -e "  Rust:        $(rustc --version 2>/dev/null | cut -d' ' -f2 || echo 'not installed')"
echo -e "  Go:          $(go version 2>/dev/null | cut -d' ' -f3 | sed 's/go//' || echo 'not installed')"
echo -e "  Python:      $(python --version 2>/dev/null | cut -d' ' -f2 || echo 'not installed')"
echo -e "  Ruby:        $(ruby -v 2>/dev/null | cut -d' ' -f2 || echo 'not installed')"
echo -e "  Node:        $(node --version 2>/dev/null || echo 'not installed')"
echo -e "  pnpm:        $(pnpm --version 2>/dev/null || echo 'not installed')"
echo -e "  cargo-watch: $(cargo-watch --version 2>/dev/null | cut -d' ' -f2 || echo 'not installed')"
echo -e "  wasm-pack:   $(wasm-pack --version 2>/dev/null | cut -d' ' -f2 || echo 'not installed')"
echo -e "  uv:          $(uv --version 2>/dev/null | cut -d' ' -f2 || echo 'not installed')"
echo -e "  ruff:        $(ruff --version 2>/dev/null | cut -d' ' -f2 || echo 'not installed')"
echo -e "  maturin:     $(maturin --version 2>/dev/null | cut -d' ' -f2 || echo 'not installed')"

echo ""
echo -e "${GREEN}🎉 Development environment setup complete!${NC}"
echo ""
echo "You can now run:"
echo "  pnpm dev              # Run everything with hot reloading"
echo "  pnpm dev:vscode       # Run VSCode extension with all dependencies"
echo "  pnpm dev:playground   # Run just the playground"
echo ""
echo "For VSCode extension debugging:"
echo "  1. Run 'pnpm dev:vscode'"
echo "  2. Press F5 in VSCode to launch the extension host"

echo ""
echo -e "${YELLOW}⚠️  IMPORTANT: Please restart your shell or run 'source ~/.zshrc' or 'source ~/.bashrc' before continuing!${NC}"