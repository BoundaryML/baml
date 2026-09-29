//! The network transport BAML uses for every connection it makes.
//!
//! This default is BAML's native transport: HTTP requests use reqwest,
//! servers hyper, WebSocket connections tungstenite, and TLS rustls.
//! TLS backend selection is local to this transport. Point the workspace's
//! `baml_http_provider` dependency at your own crate to replace it; see
//! `VENDOR.md`.

mod client;
mod server;
mod ws;

use std::sync::Arc;

use baml_http_types::{
    BoxFuture, Error, HttpProvider, Listener, Request, Response, TlsConfig, WebSocket,
    WsConnectRequest,
};

/// The transport. Called once per process.
pub fn provider() -> Arc<dyn HttpProvider> {
    Arc::new(NativeHttp)
}

/// The native transport.
#[derive(Debug, Default, Clone, Copy)]
struct NativeHttp;

impl HttpProvider for NativeHttp {
    fn send(&self, request: Request) -> BoxFuture<Result<Response, Error>> {
        client::send(request)
    }

    fn connect_websocket(&self, request: WsConnectRequest) -> BoxFuture<Result<WebSocket, Error>> {
        ws::connect(request)
    }

    fn bind(&self, addr: String) -> BoxFuture<Result<Arc<dyn Listener>, Error>> {
        server::bind(addr)
    }

    fn check_tls(&self, tls: &TlsConfig) -> Result<(), Error> {
        server::check_tls(tls)
    }
}

/// rustls and reqwest panic without a process crypto provider.
fn ensure_crypto_provider() -> Result<(), Error> {
    use rustls::crypto::CryptoProvider;
    if CryptoProvider::get_default().is_none() {
        #[cfg(target_os = "ios")]
        let provider = rustls::crypto::ring::default_provider();
        #[cfg(not(target_os = "ios"))]
        let provider = rustls::crypto::aws_lc_rs::default_provider();
        // Keep a provider installed by the host, including a racing installer.
        let _ = provider.install_default();
    }
    CryptoProvider::get_default()
        .map(|_| ())
        .ok_or_else(|| Error::io("HTTP transport has no TLS crypto provider"))
}
