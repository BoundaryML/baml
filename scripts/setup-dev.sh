#!/bin/bash
set -e

# Every path below is relative to the repo root, so go there first instead of
# assuming the caller did.
cd "$(dirname "${BASH_SOURCE[0]}")/.."

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

# Rust is not managed by mise. Each Rust workspace pins its compiler in its own
# rust-toolchain.toml (engine/, baml_language/), and only rustup reads those
# files — a Homebrew or system rustc ignores the pin and ships no wasm32
# target, so playground builds fail much later with a confusing error. Set Rust
# up before `mise install`, which builds cargo-hawk against a pinned toolchain.
if ! command -v rustup &> /dev/null; then
    echo -e "${YELLOW}📦 Installing rustup (rust-toolchain.toml pins the compiler)...${NC}"
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --no-modify-path
    export PATH="$HOME/.cargo/bin:$PATH"
    echo -e "${GREEN}✅ rustup installed${NC}"
else
    echo -e "${GREEN}✅ rustup already installed${NC}"
fi

# Materialize every workspace's pinned toolchain plus the components and
# targets its rust-toolchain.toml lists. The root has no pin of its own, so
# this has to run inside each workspace.
for pin in */rust-toolchain.toml; do
    ws="${pin%/rust-toolchain.toml}"
    echo -e "${YELLOW}📦 Installing pinned Rust toolchain for ${ws}/...${NC}"
    (cd "$ws" && rustup show active-toolchain > /dev/null \
        && echo -e "${GREEN}✅ ${ws}/: Rust $(rustc --version | cut -d' ' -f2)${NC}")
done

# cargo-hawk is a rustc driver: it links against compiler internals, so it needs
# the rustc-dev component on the exact toolchain mise.toml pins it to. Without
# it `mise install` fails, and `set -e` aborts the rest of this script. Other
# cargo tools pin toolchains too, so read the version off the cargo-hawk line.
HAWK_TOOLCHAIN="$(grep 'cargo-hawk' mise.toml | grep -o 'RUSTUP_TOOLCHAIN = "[^"]*"' | head -1 | cut -d'"' -f2)"
if [ -n "$HAWK_TOOLCHAIN" ]; then
    if ! rustup component list --toolchain "$HAWK_TOOLCHAIN" --installed 2>/dev/null | grep -q '^rustc-dev'; then
        echo -e "${YELLOW}📦 Installing Rust $HAWK_TOOLCHAIN with rustc-dev (for cargo-hawk)...${NC}"
        rustup toolchain install "$HAWK_TOOLCHAIN" \
            --component rustc-dev,llvm-tools-preview,rust-src --profile minimal
        echo -e "${GREEN}✅ Rust $HAWK_TOOLCHAIN ready${NC}"
    else
        echo -e "${GREEN}✅ Rust $HAWK_TOOLCHAIN already has rustc-dev${NC}"
    fi
fi

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

# Register the git-lfs filters and fetch the real file contents. Without this,
# every LFS-tracked path (.lfs.baml test fixtures, website fonts and images)
# stays a small text pointer instead of the file it stands for, and anything
# reading them gets nonsense.
if command -v git-lfs &> /dev/null; then
    if [ -z "$(git config --get filter.lfs.process)" ]; then
        echo -e "${YELLOW}📦 Registering git-lfs filters...${NC}"
        git lfs install --local
        echo -e "${GREEN}✅ git-lfs filters registered${NC}"
    fi
    # Never let this abort the script. Fetching depends on the remote's LFS
    # budget, which is the upstream account's to manage — a contributor who
    # can't download the objects should still get a working environment, just
    # without the large fixtures.
    echo -e "${YELLOW}📦 Fetching git-lfs file contents...${NC}"
    if git lfs pull; then
        echo -e "${GREEN}✅ git-lfs files ready${NC}"
    else
        echo -e "${RED}⚠️  git-lfs fetch failed — LFS-tracked files stay as pointer stubs.${NC}"
        echo -e "${RED}    If the error mentions an exceeded LFS budget, it is upstream's${NC}"
        echo -e "${RED}    quota, not your setup; rerun 'git lfs pull' once it is restored.${NC}"
        echo -e "${RED}    Tests over engine/playground-server big-files will fail until then.${NC}"
    fi
else
    echo -e "${RED}⚠️  git-lfs not found — install it (brew install git-lfs), then rerun.${NC}"
    echo -e "${RED}    Without it, LFS-tracked fixtures and assets stay as pointer stubs.${NC}"
fi

# Install Python project dependencies.
# Pin the interpreter to the version in mise.toml. pyproject.toml only says
# `requires-python = "~=3.9"`, so uv otherwise grabs the newest Python it can
# find — and on a version with no prebuilt wheels, deps like jiter fall back to
# compiling from source and fail.
if [ -f "integ-tests/python/pyproject.toml" ]; then
    PYTHON_VERSION="$(grep -E '^python = ' mise.toml | head -1 | cut -d'"' -f2)"
    echo -e "${YELLOW}📦 Installing Python project dependencies (Python ${PYTHON_VERSION:-default})...${NC}"
    cd integ-tests/python
    mise exec -- uv sync --dev ${PYTHON_VERSION:+--python "$PYTHON_VERSION"}
    cd - > /dev/null
    echo -e "${GREEN}✅ Python dependencies installed${NC}"
fi

# Install Ruby bundler into mise's Ruby. A bare `ruby`/`gem` here resolves to
# the system Ruby, which ships its own bundler — the check would pass while
# mise's Ruby still has none.
if mise which ruby &> /dev/null; then
    if ! mise exec ruby -- gem list bundler -i &> /dev/null; then
        echo -e "${YELLOW}📦 Installing bundler...${NC}"
        mise exec ruby -- gem install bundler
        echo -e "${GREEN}✅ bundler installed${NC}"
    fi
fi

# Install node dependencies
if mise which pnpm &> /dev/null; then
    echo -e "${YELLOW}📦 Installing node dependencies...${NC}"
    mise exec -- pnpm install
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
echo -e "  rustup:      $(rustup --version 2>/dev/null | head -1 | cut -d' ' -f2 || echo 'not installed')"
for pin in */rust-toolchain.toml; do
    ws="${pin%/rust-toolchain.toml}"
    printf '  Rust %-15s %s (%s)\n' "${ws}/:" \
        "$(cd "$ws" && rustc --version 2>/dev/null | cut -d' ' -f2 || echo 'not installed')" \
        "$(cd "$ws" && rustup target list --installed 2>/dev/null | tr '\n' ' ' | sed 's/ $//')"
done
echo -e "  Go:          $(go version 2>/dev/null | cut -d' ' -f3 | sed 's/go//' || echo 'not installed')"
echo -e "  Python:      $(python --version 2>/dev/null | cut -d' ' -f2 || echo 'not installed')"
echo -e "  Ruby:        $(ruby -v 2>/dev/null | cut -d' ' -f2 || echo 'not installed')"
echo -e "  Node:        $(node --version 2>/dev/null || echo 'not installed')"
echo -e "  pnpm:        $(pnpm --version 2>/dev/null || echo 'not installed')"
echo -e "  cargo-watch: $(cargo-watch --version 2>/dev/null | cut -d' ' -f2 || echo 'not installed')"
echo -e "  wasm-pack:   $(wasm-pack --version 2>/dev/null | cut -d' ' -f2 || echo 'not installed')"
echo -e "  cargo-hawk:  $(cargo-hawk --version 2>/dev/null | cut -d' ' -f3 || echo 'not installed')"
echo -e "  git-lfs:     $(git lfs version 2>/dev/null | cut -d' ' -f1 | cut -d'/' -f2 || echo 'not installed')"
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