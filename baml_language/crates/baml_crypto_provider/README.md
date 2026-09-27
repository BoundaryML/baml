# baml_crypto_provider

Supplies the TLS crypto for BAML builds that use `external-crypto`.

BAML makes every TLS connection (LLM calls, WebSockets, the HTTPS server,
telemetry, the remote cache) through rustls. rustls needs a crypto provider
under it, and a build picks where it comes from:

| Feature | Provider |
|---|---|
| `aws-crypto` (default, all published packages) | AWS-LC, bundled |
| `ring-crypto` | `ring`, bundled |
| `external-crypto` | none bundled: yours |

With `external-crypto`, neither AWS-LC nor `ring` is linked (CI checks this,
see `scripts/check-crypto-backends.sh`). Use it to put TLS on a crypto module
you choose, such as a FIPS-validated OpenSSL, AWS-LC in FIPS mode, BoringSSL
or SymCrypt.

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

BAML uses the first of these that exists, on its first TLS connection:

1. A process-wide provider the host already installed. A Rust program that
   embeds BAML can install one before its first BAML call:

   ```rust
   my_provider().install_default().expect("no provider installed yet");
   ```

2. The provider this crate returns. This default returns none. To supply
   one, write a crate with the same name and function and point the
   workspace at it in `baml_language/Cargo.toml`:

   ```toml
   baml_crypto_provider = { path = "../my_crypto_provider" }
   ```

   ```toml
   # my_crypto_provider/Cargo.toml
   [package]
   name = "baml_crypto_provider"
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

If neither exists, BAML does not panic and does not fall back to another
library: each HTTPS call fails with a `baml.errors.Io` that says no TLS crypto
provider is installed. Plain HTTP keeps working.

## What this covers

Every TLS connection BAML makes. Some crypto BAML runs itself in pure Rust
does not go through the provider yet:

| Where | Algorithm |
|---|---|
| Google Vertex auth | RSA signing of the service-account JWT |
| AWS Bedrock auth | HMAC-SHA256 / SHA-256 request signing (SigV4) |
| `baml.crypto` in BAML code | AES-GCM-SIV, ChaCha20-Poly1305 |
| Internal cache keys and fingerprints | SHA-256, not used for security |

If you use neither Bedrock nor Vertex and your BAML code doesn't use
`baml.crypto`, TLS is all the security-relevant crypto BAML does.
