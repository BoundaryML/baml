//! `baml_lsp` — the BAML language-server protocol layer.
//!
//! Rust-analyzer's shape: one owner thread holds [`state::GlobalState`] (the
//! unique database handle plus all session/document/fence bookkeeping) and
//! applies every mutation; reads run on an [`executor::Executor`] against
//! [`snapshot::Snapshot`]s inside a panic/cancellation guard. A mutation that
//! lands while a read is in flight cancels it (Salsa unwinding →
//! `ContentModified`); a query panic becomes `InternalError` and the server
//! keeps running. Hosts (`baml_lsp_server` natively, the wasm bridge in the
//! browser) supply the transport, the executor, a [`discovery::ProjectFs`],
//! and drain [`state::OwnerEvent`]s.
//!
//! The host's loop, in order of what it calls here:
//!
//! 1. [`GlobalState::with_fs`] once; [`GlobalState::open_session`] per client.
//! 2. [`GlobalState::dispatch_request`] / [`GlobalState::dispatch_notification`]
//!    for every incoming message (the tables live in [`dispatch`]).
//! 3. [`GlobalState::handle_event`] for every event from
//!    [`GlobalState::events`]; [`GlobalState::on_tick`] when
//!    [`GlobalState::next_deadline`] passes.
//! 4. [`GlobalState::close_session`] when a client goes away.
//!
//! This crate is wasm-clean: no threads, no transport, and the filesystem
//! only behind [`discovery::NativeFs`] on native targets.

pub mod diagnostics;
pub mod discovery;
pub mod dispatch;
pub mod error;
pub mod executor;
pub mod mutation;
pub mod paths;
pub mod position_codec;
pub mod roots;
pub mod snapshot;
pub mod state;

#[cfg(not(target_arch = "wasm32"))]
pub use discovery::NativeFs;
pub use discovery::{DiscoveredRoot, LoadedRoot, NoFs, ProjectFs};
pub use dispatch::{initialize_result, server_capabilities};
pub use error::LspError;
pub use state::{
    ClientSender, GlobalState, OwnerEvent, OwnerHandle, Responder, SessionKey, SourceRevision,
};
