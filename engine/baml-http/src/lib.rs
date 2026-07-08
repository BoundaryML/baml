//! HTTP client for BAML.
//!
//! BAML code uses `baml_http::...` instead of `reqwest::...`. The crate
//! exposes the subset of the reqwest API that BAML uses:
//!
//! - On native targets it is implemented directly on hyper/hyper-util. This
//!   keeps the dependency tree small and makes the engine vendorable in
//!   monorepos where reqwest is not importable.
//! - On wasm32 it re-exports reqwest, whose browser (fetch) backend is the
//!   only practical option there.
//!
//! ## Native TLS backend (cargo features)
//! - `native-tls` (default): platform TLS (SChannel / Security.framework /
//!   system OpenSSL) via hyper-tls. Uses the OS trust store and avoids the
//!   `ring` crate; connections use HTTP/1.1 (hyper-tls does not forward the
//!   ALPN h2 hint). Best for environments that ban ring or need corporate CAs.
//! - `rustls-tls`: statically-linked rustls + ring with bundled webpki roots,
//!   HTTP/2 enabled. Most portable for prebuilt wheels, but pulls in `ring`.
//! - `native-tls-vendored`: native-tls with a statically-vendored OpenSSL
//!   (portable Linux wheels without a system libssl).
//!
//! Known differences from reqwest on native targets:
//! - Environment proxies (HTTP_PROXY/HTTPS_PROXY) are not supported.
//! - `read_timeout` applies between body chunks (and to whole buffered body
//!   reads) rather than per socket read.

#[cfg(target_arch = "wasm32")]
mod reqwest_client;
#[cfg(target_arch = "wasm32")]
pub use reqwest_client::*;

#[cfg(not(target_arch = "wasm32"))]
mod hyper_client;
#[cfg(not(target_arch = "wasm32"))]
pub use hyper_client::*;
