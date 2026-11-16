<div align="center">
  <h1>cargo-stow</h1>
  <p><strong>Workspace linting and structure validation for Rust monorepos</strong></p>

  <a href="https://crates.io/crates/cargo-stow"><img src="https://img.shields.io/crates/v/cargo-stow.svg" alt="crates.io"></a>
  <a href="https://crates.io/crates/cargo-stow"><img src="https://img.shields.io/crates/d/cargo-stow.svg" alt="downloads"></a>
  <a href="LICENSE"><img src="https://img.shields.io/crates/l/cargo-stow.svg" alt="license"></a>
</div>

`cargo stow` keeps a growing workspace's structure intentional:

- **Layering** - declare which crates may depend on what, and fail CI on violations
- **Naming** - crate names state their namespace and role
- **Hygiene** - every dependency goes through `[workspace.dependencies]`, sorted
- **A picture** - the dependency graph as deterministic [Mermaid](https://mermaid.js.org) text

The exact behaviour is specified in [SPEC.md](SPEC.md).

## Installation

```bash
cargo install cargo-stow
```

## Usage

```bash
cargo stow init                # write a template stow.toml
cargo stow                     # validate
cargo stow --fix               # repair dependency tables, then validate
cargo stow --graph deps.mmd    # write the dependency graph
```

Exit codes: `0` success, `1` validation errors or a failure, `2` usage error.

## Configuration

`cargo stow init` writes a commented `stow.toml` at the workspace root:

```toml
[[namespaces]]
name = "myapp"
approved_prefixes = ["api", "cli"]

[[dependency_rules]]
pattern = "anyhow"
allowed_crates = ["*_cli"]
regular_deps_only = true
reason = "Use thiserror for error types in library crates."
```

The same keys can live under `[workspace.metadata.stow]` in the root `Cargo.toml` instead.
Unknown keys are rejected.

## Checks

Every check covers `dependencies`, `dev-dependencies`, `build-dependencies` and the same
tables under each `target.<cfg>`.

| Id | Rule |
|---|---|
| `nested-crate` | Crates sit directly under a workspace member root. |
| `name-mismatch` | The package name equals its folder name. |
| `naming` | Names are `<namespace>_<word>`, `<namespace>_<approved prefix>_<word>`, an approved suffix, or end in `_types`, `_test` or `_tests`. |
| `orphan-test-crate` | A `_test` or `_tests` crate has a base crate. |
| `non-workspace-dep` | Every dependency uses `workspace = true`. |
| `restricted-dep` | Dependency rules: who may depend on what. |
| `unsorted-deps` | Internal dependencies first, then external, each alphabetical. |
| `unused-workspace-dep` | Every `[workspace.dependencies]` entry is used. |

Errors are printed one per line:

```
error[restricted-dep]: myapp_core: crates/myapp_core/Cargo.toml: [dependencies] `anyhow` is not allowed here: Use thiserror for error types in library crates.
```

## Fix

`cargo stow --fix` repairs `non-workspace-dep` and `unsorted-deps`:

- Entries with a workspace definition are rewritten to `{ workspace = true }`, keeping
  `features`, `optional` and `default-features`.
- Each dependency table is sorted.

It is lossless. A comment directly above an entry moves with it, nothing else in the file
is reformatted, and a file that needs no change is not written.

## Graph

`cargo stow --graph FILE` writes a Mermaid flowchart: one cluster per namespace, dashed
edges across namespaces, and only the edges no other dependency already implies. The
output is byte-for-byte reproducible, so it can be committed and checked in CI.

## How it compares

| Feature | cargo-stow | cargo-deny | cargo-machete |
|---|:---:|:---:|:---:|
| Dependency layering rules | yes | no | no |
| Crate naming conventions | yes | no | no |
| Sorted, workspace-inherited dependencies | yes | no | no |
| Dependency graph | yes | no | no |
| License checking | no | yes | no |
| Unused dependency detection | no | no | yes |

## Tests

`tests/cases` holds one plain-text case per run. The format is documented in
[tests/cases/README.md](tests/cases/README.md) and is not tied to this implementation:
set `STOW_BIN` to test another one.

## License

MIT
