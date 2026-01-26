<div align="center">
  <h1>cargo-stow</h1>
  <p><strong>Workspace linting and structure validation for Rust monorepos</strong></p>

  <a href="https://crates.io/crates/cargo-stow"><img src="https://img.shields.io/crates/v/cargo-stow.svg" alt="crates.io"></a>
  <a href="https://crates.io/crates/cargo-stow"><img src="https://img.shields.io/crates/d/cargo-stow.svg" alt="downloads"></a>
  <a href="LICENSE"><img src="https://img.shields.io/crates/l/cargo-stow.svg" alt="license"></a>
</div>

## Features

- **Dependency sorting** - Keep deps organized (internal first, then external)
- **Structure validation** - Enforce flat crate layout, naming conventions
- **Dependency rules** - Control who can depend on what
- **Dependency graph** - Visualize workspace structure as SVG
- **Auto-fix** - Automatically fix sortable issues

## Installation

```bash
cargo install cargo-stow
```

## Quick Start

```bash
cargo stow init        # Generate config
cargo stow             # Validate (default)
cargo stow --fix       # Auto-fix sortable issues
cargo stow --graph deps.svg  # Generate dependency graph
```

## Why cargo-stow?

Rust workspaces are powerful, but as they grow, they need governance:

| Feature | cargo-stow | cargo-deny | cargo-machete |
|---------|:----------:|:----------:|:-------------:|
| Dependency sorting | ✅ | ❌ | ❌ |
| Workspace structure validation | ✅ | ❌ | ❌ |
| Crate naming conventions | ✅ | ❌ | ❌ |
| Dependency graph visualization | ✅ | ❌ | ❌ |
| Namespace/prefix enforcement | ✅ | ❌ | ❌ |
| License checking | ❌ | ✅ | ❌ |
| Unused dependency detection | ❌ | ❌ | ✅ |

**cargo-stow** fills the gap for workspace structure linting - use it alongside cargo-deny and cargo-machete for comprehensive workspace hygiene.

## Configuration

Run `cargo stow init` to generate a `stow.toml` configuration file:

```toml
# Define namespace(s) for your crates
[[namespaces]]
name = "myapp"
approved_prefixes = ["api", "cli"]
test_crate_exceptions = []

# Control which crates can depend on what
[[dependency_rules]]
pattern = "anyhow"
allowed_crates = ["*_cli"]
regular_deps_only = true
reason = "Use thiserror for library crates."
```

Alternatively, configure via `[workspace.metadata.stow]` in your `Cargo.toml`.

## Validation Rules

### 1. Flat Crate Structure
All crates must be directly under your crates directory - no nested crates.

### 2. Crate Name Matches Folder
The `name` field in `Cargo.toml` must match the folder name.

### 3. Naming Convention
Crate names must follow: `<namespace>_<word>` or `<namespace>_<prefix>_<word>`

- `myapp_core` - simple crate name
- `myapp_api_client` - prefixed name (requires "api" in `approved_prefixes`)
- `myapp_core_types` - auto-allowed `_types` suffix
- `myapp_core_tests` - auto-allowed `_tests` suffix

### 4. Test Crate Pairing
Crates ending in `_test` or `_tests` must have a corresponding base crate.

### 5. Workspace Dependencies
All dependencies must use `{ workspace = true }` format.

### 6. Dependency Restrictions
Configure rules to restrict which crates can use specific dependencies.

### 7. Dependency Sorting
Dependencies are sorted: internal deps first (alphabetically), then external deps (alphabetically).

## What Gets Fixed

Running `cargo stow --fix`:
- Converts dependencies to `{ workspace = true }` format
- Sorts dependencies (internal first, then external)
- Formats TOML files using taplo

## CI Integration

Add to your GitHub Actions workflow:

```yaml
- name: Install cargo-stow
  run: cargo install cargo-stow

- name: Validate workspace structure
  run: cargo stow
```

## Exit Codes

- `0` - All validations passed
- `1` - Validation errors found

## License

MIT
