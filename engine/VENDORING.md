# Vendoring the BAML engine

This workspace supports a **vendor profile**: a minimal, feature-gated build of
the Python bindings (`baml_py`) and the `baml` CLI for environments that vendor
all third-party code (typically large monorepos where every external crate must
be imported and reviewed).

## The vendor profile

```bash
# The Python extension module (includes the CLI entrypoints):
cargo build -p baml-python-ffi --no-default-features --release

# The standalone CLI, same trimmed feature set:
cargo build -p baml-cli --no-default-features --release
```

`--no-default-features` on either crate disables, relative to the published
artifacts:

| Feature   | What it gates                             | Heavy deps dropped                              |
| --------- | ----------------------------------------- | ----------------------------------------------- |
| `lsp`     | `baml-cli lsp` (the language server)       | lsp-server/lsp-types, tonic+prost, axum 0.8, tokio-tungstenite, schemars, tar, console-subscriber |
| `bedrock` | AWS Bedrock provider                       | the entire `aws-*` / smithy SDK tree            |
| `serve`   | `baml serve` (local OpenAPI server)        | axum 0.7, axum-extra                            |
| `dev`     | `baml dev` (serve + file watching)         | notify, notify-debouncer-full                   |
| `repl`    | `baml repl`                                | dirs, supports-color                            |
| `cloud`   | `baml auth/login/deploy` (Boundary Cloud)  | dialoguer, indicatif(-cli), open, jsonwebtoken-adjacent auth stack |
| `vertex`  | Vertex AI (Google Cloud) provider auth     | gcp_auth                                        |
| `tui`     | Interactive terminal UIs (`baml init` stepper) | ratatui, crossterm, cassowary, vte, … (~12 crates) |
| `optimize`| `baml optimize` (GEPA; TUI-driven)          | (via `tui`)                                     |
| `studio`  | Boundary Studio tracing publisher           | baml-rpc (ts-rs, serde_with), blake3, flate2    |

What remains is `baml init / generate / check / test / fmt / dump-hir /
dump-bytecode` plus the full in-process runtime (OpenAI, Anthropic,
Google AI, and any OpenAI-compatible provider). Disabling `vertex` or
`bedrock` does not remove the provider from the language: using it simply
fails at request time with an error explaining the feature was compiled out.
Without `tui`, `baml init` uses its plain-text output path (same behavior as
`BAML_NO_UI=1`).

Every feature is **on by default**, so published artifacts (PyPI, NPM, gems,
the release CLI) are unaffected by the gating.

## Using Vertex AI

Add the `vertex` feature back on top of the minimal profile:

```bash
cargo build -p baml-python-ffi --no-default-features --features vertex --release
```

This adds 8 crates (~289 total): `gcp_auth` (hyper-based, so reqwest stays out
of the tree), `rustls-pemfile`, `async-trait`, `pin-project`(+internal),
`tracing-futures`, and a second `thiserror` (v1, alongside v2).

Auth strategies supported by the vertex-ai provider (via gcp_auth): a service
account JSON file path or JSON string/object passed in client options, or
system default resolution (application-default credentials file → GCE
metadata server → gcloud CLI). `base_url` can be overridden per client if
requests must go through a proxy.

## HTTP stack: baml-http (hyper), no reqwest

All shipping HTTP goes through the internal [`baml-http`](baml-http/) crate:

- **Native targets**: implemented directly on `hyper` + `hyper-util`, with a
  selectable TLS backend (see below). reqwest is not in the native dependency
  tree at all.
- **wasm32**: re-exports reqwest, whose browser-fetch backend is the only
  practical option there.

reqwest is still used by non-shipping/auxiliary code: the language-server's
CORS proxy (gated out of the vendor profile via `lsp`), the playground
server, sandbox, and tests.

Known differences from the old reqwest stack (see `baml-http/src/lib.rs`):
environment proxies (`HTTP_PROXY`/`HTTPS_PROXY`) are not supported, and
`read_timeout` acts as an idle timeout between body chunks.

### TLS backend (and the `ring` question)

baml-http's TLS backend is a cargo feature:

- **`native-tls` (default)**: platform TLS (SChannel / Security.framework /
  system OpenSSL) via hyper-tls. Reads the OS trust store (corporate/internal
  CAs work out of the box) and **does not pull in `ring`**. Uses HTTP/1.1.
- **`rustls-tls`**: statically-linked rustls + ring + bundled webpki roots,
  HTTP/2 enabled. Most portable for prebuilt wheels, but pulls in `ring`.
- **`native-tls-vendored`**: native-tls with a statically-vendored OpenSSL,
  for portable Linux wheels without a system `libssl`.

With the default (`native-tls`), the vendor profile is **ring-free**,
including with `--features vertex`.

Vertex uses a vendored, ring-free fork of `gcp_auth` (see
[`vendored/gcp-auth`](vendored/gcp-auth/)): upstream `gcp_auth 0.12` pulls
`ring` for both TLS (hyper-rustls) and JWT RS256 signing. The fork swaps TLS to
native-tls and the signer to the pure-Rust `rsa` crate. The signer is verified
byte-identical to `openssl dgst -sha256 -sign` (a unit test in
`vendored/gcp-auth/src/types.rs`). To use a different signing backend (e.g.
BoringSSL), replace the ~15-line `Signer` impl in that file.

## Generating the import list

The exact crate set to vendor (note `-e normal,build`: build-dependencies
like `cc` must be imported too):

```bash
cargo tree -p baml-python-ffi --no-default-features -e normal,build --prefix none \
  | sed -E 's/ \(\*\)//; s/ \(proc-macro\)//' \
  | grep -v "$(pwd)" \
  | awk '{print $1" "$2}' | sort -u
```

As of this writing that is ~283 unique external crates (vs ~460+ for the
published wheel). The only crate with native/asm build requirements on Linux
is **`ring`** (rustls' crypto backend). `core-foundation-sys`,
`security-framework-sys`, and `fsevent-sys` in the list are macOS-only cfg
dependencies; `dirs-sys` is pure Rust.

## Build-system notes for consumers

- `baml-python-ffi` is a plain pyo3 `cdylib` using `abi3-py38` — no maturin
  needed inside a monorepo build system; a `rust_shared_library` (or
  equivalent pyo3 rule) plus a `py_library` wrapper suffices.
- `baml-cli` is an ordinary `rust_binary`.
- Vendor from release tags, and regenerate + diff the crate list on each
  upgrade so new imports are an explicit, reviewable event.

## Supply-chain audit coverage

Several organizations publish cargo-vet audit sets; the largest public
aggregation is [google/rust-crate-audits](https://github.com/google/rust-crate-audits)
(Android, Fuchsia, gVisor, ChromeOS…). `scripts/vendor-audit-coverage.py`
cross-references the vendor profile against it. As of 2026-07-07:

- **75 crates (25%)**: exact version covered by a published audit.
- **141 crates**: audited at a different (usually older) version; delta
  review, typically cheap.
- **78 crates**: no published audit. The big-ticket items:
  - **pyo3 family** (6 crates): the Python binding layer itself;
    unsafe-heavy, unavoidable for `baml_py`, and the single largest review.
    Note `pyo3-build-config`/`python3-dll-a` are build-time crates that read
    the environment (they locate the Python toolchain).
  - **rustls 0.23 family + webpki-roots**: older rustls lineages have
    published audits; the 0.23 line does not yet. `webpki-roots` is
    CDLA-Permissive-2.0 (the Mozilla CA bundle license); see the trust-roots
    note above for avoiding it entirely.
  - **askama** (+derive/parser): compile-time templates in the generators;
    its derive macro reads template files from the source tree at build time,
    which strict proc-macro policies will want to look at.

Version multiplicity: 13 crates appear at 2 versions each (all different
semver epochs, e.g. `syn` 1+2, `hashbrown` 0.14+0.15). All but one
(`rustc-hash`, pinned in `internal-baml-parser-database`) come from external
pins.

Licenses: overwhelmingly `MIT OR Apache-2.0`; the rest are
ISC/Unicode-3.0/Unlicense-OR-MIT notice licenses. The only unusual one is
`webpki-roots` (CDLA-Permissive-2.0, see above).

## Keeping it green

CI runs the vendor profile on every PR (`vendor-profile` job in
`.github/workflows/primary.yml`): the `--no-default-features` builds (with
and without `vertex`) must compile, and `reqwest` must not appear in the
native vendor tree. If your change adds a dependency to the vendor profile,
expect the reviewer to ask whether it can live behind one of the feature
gates above instead.
