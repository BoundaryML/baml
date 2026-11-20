//! The state of a `baml.http.Server` and of a `baml.http.TlsConfig`: plain
//! data the platform's server reads, with no heap values; and how a server
//! takes a response body to send.

use std::sync::Mutex;

use rustls_pki_types::{CertificateDer, PrivateKeyDer};

/// A `baml.http.Server`'s bound listener.
///
/// The listener lives here between serves, so cancelling one keeps the port
/// held and the same server can serve again. A serve takes it out for as long
/// as it runs ([`Self::take_listener`]) and puts it back when it ends
/// ([`Self::return_listener`]).
pub struct ServerState {
    slot: Mutex<ListenerSlot>,
}

enum ListenerSlot {
    /// Bound, nonblocking, and waiting for a serve.
    Idle(std::net::TcpListener),
    /// A serve has it.
    Serving,
    /// A serve could not hand it back: the port is released, and the server
    /// cannot serve again.
    Lost,
}

/// Why a server has no listener for a new serve.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListenerUnavailable {
    /// Another serve has it.
    Serving,
    /// An earlier serve could not hand it back.
    Lost,
}

impl ServerState {
    /// A server over `listener`, which is bound and nonblocking.
    pub fn new(listener: std::net::TcpListener) -> Self {
        Self {
            slot: Mutex::new(ListenerSlot::Idle(listener)),
        }
    }

    /// The listener, for a serve to run on.
    ///
    /// # Errors
    ///
    /// The server has no listener to give: see [`ListenerUnavailable`].
    pub fn take_listener(&self) -> Result<std::net::TcpListener, ListenerUnavailable> {
        let mut slot = self.slot();
        match std::mem::replace(&mut *slot, ListenerSlot::Serving) {
            ListenerSlot::Idle(listener) => Ok(listener),
            ListenerSlot::Serving => Err(ListenerUnavailable::Serving),
            ListenerSlot::Lost => {
                *slot = ListenerSlot::Lost;
                Err(ListenerUnavailable::Lost)
            }
        }
    }

    /// Give the listener back once a serve has ended.
    pub fn return_listener(&self, listener: std::net::TcpListener) {
        let mut slot = self.slot();
        debug_assert!(
            matches!(*slot, ListenerSlot::Serving),
            "only the serve that took the listener returns it"
        );
        *slot = ListenerSlot::Idle(listener);
    }

    /// The serve that took the listener could not keep it.
    pub fn lose_listener(&self) {
        let mut slot = self.slot();
        debug_assert!(
            matches!(*slot, ListenerSlot::Serving),
            "only the serve that took the listener loses it"
        );
        *slot = ListenerSlot::Lost;
    }

    fn slot(&self) -> std::sync::MutexGuard<'_, ListenerSlot> {
        // Nothing panics while holding the lock, so a poisoned slot is still
        // a whole state.
        self.slot
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// A `baml.http.TlsConfig`'s parsed certificate chain (leaf first) and private
/// key. Deliberately not `Debug`: nothing prints a key.
pub struct TlsKeys {
    certificates: Vec<CertificateDer<'static>>,
    private_key: PrivateKeyDer<'static>,
}

impl TlsKeys {
    pub fn new(
        certificates: Vec<CertificateDer<'static>>,
        private_key: PrivateKeyDer<'static>,
    ) -> Self {
        Self {
            certificates,
            private_key,
        }
    }

    pub fn certificates(&self) -> &[CertificateDer<'static>] {
        &self.certificates
    }

    pub fn private_key(&self) -> &PrivateKeyDer<'static> {
        &self.private_key
    }

    /// The memory the keys hold. The key counts at a bound that covers any
    /// key in use (an RSA-8192 PKCS#8 blob is under 5 KiB), so nothing about
    /// the key is derived from the secret.
    pub fn retained_bytes(&self) -> usize {
        self.certificates
            .iter()
            .map(|cert| cert.len())
            .sum::<usize>()
            + 8 * 1024
    }
}

/// How a server takes a response body to send
/// ([`HttpBodyResource::take_stream`](crate::resource::HttpBodyResource::take_stream)).
#[derive(Debug)]
pub enum StreamTake {
    /// Not written incrementally: the server reads it whole.
    Whole,
    /// The chunks the handler writes, in order, ending when it ends the body
    /// or drops it. Only one server takes them: holding the receiver is what
    /// lets the writer see a client hang up.
    Chunks(tokio::sync::mpsc::Receiver<bytes::Bytes>),
    /// Written incrementally, and already taken by a server.
    Taken,
}
