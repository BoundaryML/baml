# BAML on Nix

The repository flake builds BAML from the pinned Rust toolchain and
`baml_language/Cargo.lock`. It exposes two packages with intentionally different
responsibilities.

## Packages

### `baml` (default)

The real rustup-style BAML wrapper:

```console
nix run .#baml -- --version
# or
nix run . -- --version
```

Nix owns the wrapper, so it is built with the `no-self-update` feature. The
wrapper still resolves project manifests and may install or update BAML language
toolchains under `BAML_HOME` when requested. This runtime download behavior is
intentional.

### `baml-cli`

A fixed, Nix-managed language toolchain for CI, containers, and other
reproducible environments:

```console
BAML_CLI_ALLOW_DIRECT=1 nix run .#baml-cli -- --version
nix build .#baml-cli
```

The output contains both binaries:

```text
bin/baml-cli
bin/baml-pack-host
```

The sibling pack host lets native `baml-cli pack` operations run without
fetching the host from a BAML release. Direct invocation may print a warning;
set `BAML_CLI_ALLOW_DIRECT=1` when deliberately using this package instead of
the wrapper.

## Supported systems

| Flake system | Rust target |
| --- | --- |
| `x86_64-linux` | `x86_64-unknown-linux-musl` |
| `aarch64-linux` | `aarch64-unknown-linux-musl` |
| `aarch64-darwin` | `aarch64-apple-darwin` |

Linux outputs are static PIE musl executables. This allows the wrapper to select
upstream musl toolchains on NixOS and makes the default native `baml pack` output
portable outside a particular Nix store.

Only `x86_64-linux` is currently built and smoke-tested locally. The other
systems evaluate successfully and require their native CI runners before being
claimed as build-verified.

## Formatting

```console
nix fmt
```

The treefmt wrapper applies `nixfmt` to the root `flake.nix` and files under
`packaging/nix/`. Existing Rust workspace and TypeScript Biome formatting remain
under their established repository commands and CI jobs.

## Checks

```console
nix flake check
nix flake check --all-systems --no-build
```

The local-system checks verify:

- wrapper version, local toolchain listing, and disabled self-update;
- CLI version and help;
- a minimal BAML compiler check;
- TypeScript SDK generation without invoking Node.js;
- native packing through the installed sibling `baml-pack-host`;
- static Linux linkage.

Checks use isolated `HOME`, `BAML_HOME`, and `BAML_CACHE_DIR` directories and
disable CLI telemetry. Nix builds and checks do not download BAML release
artifacts or managed toolchains.

## State and current limitations

- `BAML_HOME` stores wrapper configuration, manifests, credentials, and managed
  toolchains. It defaults to `~/.baml`.
- `BAML_CACHE_DIR` overrides the fixed CLI's compiler/bytecode cache location.
- `baml playground` requires packaged playground assets, which are not yet in
  the fixed CLI output.
- `baml ide install` requires the managed toolchain's VS Code extension asset.
- A non-native `baml pack --target ...` may download that target's pack host at
  runtime.
- Language SDK native runtimes (`baml-cffi`, Python, Node, and others) are
  separate future packages.
