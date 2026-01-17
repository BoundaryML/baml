# Development Scripts

## Pre-commit Hook

The `pre-commit` hook runs automated checks before allowing commits.

### Installation

```bash
# From baml_language directory
ln -sf ../../baml_language/scripts/pre-commit ../.git/hooks/pre-commit
```

Or install it manually:

```bash
cp scripts/pre-commit ../.git/hooks/pre-commit
chmod +x ../.git/hooks/pre-commit
```

### What it checks

1. **Cargo Stow**: Validates Cargo.toml dependencies are in sync
2. **Rig Templates**: Validates test crate templates are in sync

### Bypassing the hook

If you need to bypass the hook (not recommended):

```bash
git commit --no-verify
```

### Manual checks

You can run the checks manually:

```bash
# Check cargo stow
cargo run -p baml_tools_stow -- stow --check

# Check rig templates
cargo run -p baml_tools_rig -- --check
```
