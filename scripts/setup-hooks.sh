#!/bin/bash

# Script to set up Git hooks for the BAML project

set -e

SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
REPO_ROOT="$( cd "$SCRIPT_DIR/.." && pwd )"
HOOKS_DIR="$REPO_ROOT/.git/hooks"

echo "Setting up Git hooks for BAML project..."

# Create pre-commit hook
cat > "$HOOKS_DIR/pre-commit" << 'EOF'
#!/bin/bash

# Git pre-commit hook for BAML Language project
# Runs cargo fmt and cargo clippy before allowing commits
#
# To skip this hook, use one of these methods:
# 1. Add "[skip-checks]" to your commit message
# 2. Use --no-verify flag: git commit --no-verify
# 3. Set environment variable: SKIP_CHECKS=1 git commit

set -e

# Color codes for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Check if we should skip checks
if [ "$SKIP_CHECKS" = "1" ]; then
    echo -e "${YELLOW}Skipping pre-commit checks (SKIP_CHECKS=1)${NC}"
    exit 0
fi

# Check if commit message contains skip marker
# Note: This checks the prepared commit message file if it exists
if [ -f ".git/COMMIT_EDITMSG" ]; then
    if grep -q "\[skip-checks\]" .git/COMMIT_EDITMSG; then
        echo -e "${YELLOW}Skipping pre-commit checks ([skip-checks] in commit message)${NC}"
        exit 0
    fi
fi

# Change to baml_language directory
cd baml_language 2>/dev/null || {
    echo -e "${RED}Error: baml_language directory not found${NC}"
    exit 1
}

echo -e "${GREEN}Running pre-commit checks in baml_language...${NC}"

# Run cargo fmt to auto-fix formatting
echo -e "${YELLOW}Running cargo fmt to auto-fix formatting...${NC}"
cargo fmt --all -- --config imports_granularity=Crate --config group_imports=StdExternalCrate
echo -e "${GREEN}✓ Formatting applied${NC}"

# Add any formatted files to the commit
git add -u

# Run cargo clippy with auto-fix
echo -e "${YELLOW}Running cargo clippy with auto-fix...${NC}"
# First try to auto-fix what we can
cargo clippy --workspace --all-targets --all-features --fix --allow-dirty --allow-staged -- -D warnings 2>/dev/null || true

# Add any fixed files to the commit
git add -u

# Now check if there are any remaining warnings that couldn't be auto-fixed
if ! cargo clippy --workspace --all-targets --all-features -- -D warnings; then
    echo -e "${RED}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${RED}✗ Clippy warnings detected that couldn't be auto-fixed!${NC}"
    echo -e "${RED}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo ""
    echo "Please fix the warnings above, or if you need to commit anyway:"
    echo ""
    echo -e "${YELLOW}To skip these checks (for WIP commits):${NC}"
    echo "  • git commit --no-verify"
    echo "  • SKIP_CHECKS=1 git commit"
    echo "  • git commit -m \"your message [skip-checks]\""
    echo ""
    echo -e "${RED}⚠️  Warning: CI/CD will still enforce these checks before merging!${NC}"
    echo -e "${RED}    You'll need to fix these issues before your PR can be merged.${NC}"
    echo ""
    exit 1
fi
echo -e "${GREEN}✓ Clippy checks passed${NC}"

# Run cargo stow to validate Cargo.toml files
echo -e "${YELLOW}Running cargo stow --check...${NC}"
if ! cargo stow --check; then
    echo -e "${RED}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${RED}✗ Cargo.toml validation failed!${NC}"
    echo -e "${RED}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo ""
    echo "Run 'cargo stow --fix' to auto-fix some issues, or fix manually."
    echo ""
    echo -e "${YELLOW}To skip these checks (for WIP commits):${NC}"
    echo "  • git commit --no-verify"
    echo "  • SKIP_CHECKS=1 git commit"
    echo ""
    exit 1
fi
echo -e "${GREEN}✓ Cargo.toml validation passed${NC}"

# Run bep:readme to keep BEP index up to date
echo -e "${YELLOW}Checking BEP index...${NC}"
cd "$REPO_ROOT" || exit 1
if command -v mise >/dev/null 2>&1; then
    mise run bep:readme
    git add beps/README.md 2>/dev/null || true
    echo -e "${GREEN}✓ BEP index updated${NC}"
else
    echo -e "${YELLOW}Warning: mise not found, skipping BEP index update${NC}"
fi

echo ""
echo -e "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${GREEN}✓ All pre-commit checks passed!${NC}"
echo -e "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo "Auto-fixes were applied for formatting and simple clippy issues."
echo "These changes have been staged in your commit."
EOF

# Make hook executable
chmod +x "$HOOKS_DIR/pre-commit"

echo "✓ Git hooks installed successfully!"
echo ""
echo "The pre-commit hook will now run cargo fmt and cargo clippy before each commit."
echo ""
echo "To skip the checks for a specific commit, you can:"
echo "  1. Use --no-verify flag: git commit --no-verify"
echo "  2. Add [skip-checks] to your commit message"
echo "  3. Set environment variable: SKIP_CHECKS=1 git commit"