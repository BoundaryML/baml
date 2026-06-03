# BAML Toolchain System

## Product Boundary

`baml-wrapper` contains only `bin/baml`. It is installed by package managers and curl installers.

`baml-toolchain` contains `bin/baml-cli`, `bin/baml-pack-host`, and `assets/baml-vscode.vsix`. Normal commands are forwarded by the wrapper to the selected toolchain.

## Local Layout

```text
~/.baml/
  config.toml
  state.toml
  bin/baml
  manifest-cache/
  toolchains/<version>/
    VERSION
    install.json
    bin/baml-cli
    bin/baml-pack-host
    assets/baml-vscode.vsix
```

`config.toml` stores user intent:

```toml
[default]
selector = "canary"

[update]
auto_check = false
```

`state.toml` stores active channel resolutions. Manifest cache is not authoritative for normal command resolution.

## Resolution

Precedence:

1. `BAML_VERSION`
2. nearest `baml.toml [toolchain]`
3. `~/.baml/config.toml [default].selector`
4. hardcoded `canary`

Normal commands never hit the network and never auto-install. Toolchain commands are the allowlisted network surface.

`toolchain use` installs if missing and selects the default. `toolchain install` downloads without selecting. `toolchain update` advances channel selectors only and is the explicit freshness check. `toolchain list` is local-only.

## Manifests

Toolchain and wrapper manifests use schema `1`, HTTPS artifact URLs, lowercase SHA-256 checksums, and exact target sets. `BAML_MANIFEST_BASE_URL` can point wrapper validation at dry-run or mirror manifests and is not persisted.

Manifest cache entries are namespaced by manifest base URL: production uses `manifest-cache/prod`, while overrides use `manifest-cache/override/<hash>`. Channel `toolchain use` has a 24-hour cache TTL so selecting an already-known channel does not block on the network unnecessarily. `toolchain update` bypasses that TTL and checks the remote channel pointer before mutating state. Normal pass-through commands never read remote metadata; they use `config.toml`, `state.toml`, and installed `VERSION` files only. Channel state records the manifest base URL that produced it, so dry-run or mirror state is not silently reused under production.

## IDE And Compatibility

The VSIX launches `baml lsp` through the wrapper. It starts one lazy LSP client per nearest BAML project root, so sibling projects can select different toolchains.

LSP compatibility metadata is returned on `initialize` under `capabilities.experimental.baml`. Playground compatibility is checked when the WebSocket receives `hello`; the server sends `hello` and then the legacy `ready`.

## Direct Internal CLI

Direct `baml-cli` invocation prints a stderr warning unless `BAML_WRAPPER_EXEC=1` or `BAML_CLI_ALLOW_DIRECT=1` is set. The warning never goes to stdout.

## Pack Fetcher

`baml_release` owns shared target naming, checksums, manifest schema structs, retrying downloads, install locks, and archive extraction. `baml pack` keeps existing `BAML_PACK_HOST_RELEASE_*` overrides while sharing release artifact resolution with the wrapper.
