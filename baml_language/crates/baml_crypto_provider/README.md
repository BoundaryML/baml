# baml_crypto_provider

Supplies the crypto for BAML builds that use `external-crypto`.

BAML runs all of its security-relevant crypto on one rustls crypto provider:
TLS (LLM calls, WebSockets, the HTTPS server, telemetry, the remote cache),
Vertex and Bedrock request signing, download checksums, `baml.crypto.Sha256`
and `baml.random.SystemRandom`. A build picks where that provider comes from:

| Feature | Provider |
|---|---|
| `aws-crypto` (default, all published packages except iOS) | AWS-LC, bundled |
| `ring-crypto` | `ring`, bundled |
| `external-crypto` | none bundled: yours |

With `external-crypto`, neither AWS-LC nor `ring` is linked (CI checks this,
see `scripts/check-crypto-backends.sh`). Use it to run BAML's crypto on a
module you choose, such as AWS-LC in FIPS mode, a FIPS-validated OpenSSL,
BoringSSL or SymCrypt.

## Build

```sh
# Python extension
cargo build -p bridge_python --no-default-features --features external-crypto
# C library
cargo build -p bridge_cffi --no-default-features --features bundle-http,external-crypto
# Node addon
cargo build -p bridge_typescript --no-default-features --features bundle-http,external-crypto
# CLI, and the host binary `baml pack` embeds
cargo build -p baml_cli -p baml_pack_host --no-default-features --features external-crypto
# `baml` wrapper
cargo build -p baml --no-default-features --features self-update,external-crypto
```

Every artifact takes the same three features, so one build can't mix
libraries. The Rust SDK (`baml_bridge`) has them too; with `external-crypto`
it only downloads the BAML library after the host has installed a provider.

## Supply the provider

BAML uses the first of these that exists, the first time it needs crypto:

1. A process-wide provider the host already installed. A Rust program that
   embeds BAML can install one before its first BAML call:

   ```rust
   my_provider().install_default().expect("no provider installed yet");
   ```

2. The provider the `baml_crypto_provider` dependency returns. This crate
   returns none. To supply one, write a crate with a `provider()` function and
   point the dependency at it in `baml_language/Cargo.toml`, keeping the
   dependency name (`package` is your crate's name):

   ```toml
   baml_crypto_provider = { path = "../my_crypto_provider", package = "my_crypto_provider" }
   ```

   ```toml
   # my_crypto_provider/Cargo.toml
   [package]
   name = "my_crypto_provider"
   version = "0.1.0"
   edition = "2021"

   [dependencies]
   rustls = { version = "0.23", default-features = false, features = ["std", "fips"] }
   ```

   ```rust
   // my_crypto_provider/src/lib.rs: AWS-LC in FIPS mode
   pub fn provider() -> Option<rustls::crypto::CryptoProvider> {
       Some(rustls::crypto::default_fips_provider())
   }
   ```

   Any rustls 0.23 provider crate works the same way (for example
   `rustls-openssl`, `rustls-symcrypt`, a BoringSSL provider). With Bazel or
   Buck, swap the target that provides `baml_crypto_provider`.

   The provider must have a TLS 1.3 SHA-256 cipher suite (every mainstream
   and FIPS provider does): BAML takes SHA-256 and HMAC-SHA256 from it.

If neither exists, BAML does not panic and does not fall back to another
library: each operation that needs crypto fails with an error saying no crypto
provider is installed. Code that needs no crypto keeps working.

### AWS-LC FIPS notes

The example above is tested end to end (HTTPS, SHA-256, HMAC-SHA256, RS256,
secure random, AWS's SigV4 test vectors). Two things to know:

- Building `aws-lc-fips-sys` needs CMake and Go.
- rustls's `fips` feature also compiles the regular `aws-lc-sys` into the
  build; `aws-lc-rs` uses the FIPS module at runtime. On macOS the FIPS
  module is a shared library (`libaws_lc_fips_*_crypto.dylib`) that must be
  shipped alongside the binary; on Linux it links statically.

## What runs on the provider

| Where | What |
|---|---|
| Every TLS connection | TLS |
| Google Vertex auth | RS256 signing of the service-account JWT, SHA-256 token-cache keys |
| AWS Bedrock auth | SigV4: HMAC-SHA256 and SHA-256 |
| Self-update, wrapper and Rust SDK downloads | SHA-256 checksum checks |
| Telemetry | Upload content hashes, the salted project-id hash |
| `baml.crypto.Sha256` | SHA-256 |
| `baml.random.SystemRandom` | Secure random bytes |

`baml.crypto`'s ciphers (AES-GCM-SIV, ChaCha20-Poly1305, XChaCha20-Poly1305)
are not FIPS-approved and no provider offers them, so an `external-crypto`
build refuses to create them (`Unsupported`).

## What doesn't, and why that's fine

SHA-256 used as a fingerprint, not to protect anything, stays on the `sha2`
crate: compile-cache keys, program identity, file signatures, codegen output
tracking, profiler stores. Those crates also build for wasm, where there is
no provider. FIPS covers crypto used for a security function; confirm with
your auditor that these are out of scope.
