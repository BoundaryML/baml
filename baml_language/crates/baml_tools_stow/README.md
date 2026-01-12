# baml_tools_stow

A cargo subcommand that validates and fixes `Cargo.toml` files in the BAML workspace.

## Installation

```bash
cargo install --path crates/baml_tools_stow
```

## Usage

```bash
# Check for validation errors (default)
cargo stow --check

# Automatically fix issues
cargo stow --fix

# Verbose output
cargo stow --check --verbose
```

## Configuration

Stow can be configured via a `stow.toml` file in the workspace root, or via `[workspace.metadata.stow]` in `Cargo.toml`. If both exist, `stow.toml` takes precedence.

### stow.toml Example

```toml
# Approved prefixes for multi-word crate names (e.g., baml_compiler_*)
approved_prefixes = ["lsp", "tools", "compiler", "builtins", "vm", "ide", "playground"]

# Test crates exempt from "must have prefix crate" rule
test_crate_exceptions = ["baml_tests"]

# Dependency restriction rules
[[dependency_rules]]
pattern = "anyhow"
allowed_prefixes = ["lsp"]
allowed_crates = ["baml_cli"]
regular_deps_only = true
reason = "Use thiserror for proper error types in library crates."

[[dependency_rules]]
pattern = "baml_compiler*"
allowed_prefixes = ["compiler", "lsp"]
allowed_crates = ["baml_db", "baml_project"]
regular_deps_only = true
reason = "Use baml_db or baml_project to access compiler interfaces."
```

### Cargo.toml Metadata Example

```toml
[workspace.metadata.stow]
approved_prefixes = ["lsp", "tools", "compiler", "builtins", "vm", "ide", "playground"]
test_crate_exceptions = ["baml_tests"]

[[workspace.metadata.stow.dependency_rules]]
pattern = "anyhow"
allowed_prefixes = ["lsp"]
allowed_crates = ["baml_cli"]
regular_deps_only = true
reason = "Use thiserror for proper error types in library crates."
```

If no configuration is found, sensible defaults are used.

## Validation Rules

### 1. Flat Crate Structure
All crates must be directly under `crates/` - no nested crates allowed.

### 2. Crate Name Matches Folder
The `name` field in `Cargo.toml` must match the folder name.

### 3. Naming Convention
Crate names must follow one of these patterns:
- `baml_<word>` - simple crate name (e.g., `baml_cli`, `baml_db`)
- `baml_<prefix>_<word>` - prefixed crate name with approved prefix

**Approved prefixes:** `lsp`, `tools`, `compiler`, `builtins`, `vm`, `ide`

Test crates can use `_test` or `_tests` suffix (e.g., `baml_ide_tests`).

### 4. Test Crate Pairing
Crates ending in `_test` or `_tests` must have a corresponding base crate (e.g., `baml_ide_tests` requires `baml_ide`).

**Exceptions:** `baml_tests`

### 5. Workspace Dependencies
All dependencies must use `{ workspace = true }` format to ensure version consistency.

### 6. Dependency Restrictions
Certain dependencies are restricted to specific crates:

| Dependency | Allowed In | Reason |
|------------|------------|--------|
| `anyhow` | `baml_lsp_*`, `baml_cli` | Use `thiserror` for proper error types in libraries |
| `baml_compiler_*` | `baml_compiler_*`, `baml_lsp_*`, `baml_db`, `baml_project` | Use `baml_db` or `baml_project` for compiler access |

Test crates (`*_test`, `*_tests`) and tool crates (`baml_tools_*`) are exempt from these restrictions.

### 7. Dependency Sorting
Dependencies must be sorted:
1. Internal (`baml_*`) dependencies first, alphabetically sorted
2. External dependencies second, alphabetically sorted

## What Gets Fixed

When running `cargo stow --fix`:
- Dependencies are converted to `{ workspace = true }` format
- Dependencies are sorted according to the rules above
- TOML files are formatted using taplo

## Exit Codes

- `0` - All validations passed
- `1` - Validation errors found
