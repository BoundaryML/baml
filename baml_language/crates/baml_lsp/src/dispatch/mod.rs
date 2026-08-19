//! The protocol dispatch tables.
//!
//! Requests come in two kinds, declared once in [`define_request_tables!`]:
//!
//! - **owner-inline** — served on the owner thread from session/root
//!   bookkeeping alone (`initialize`, `shutdown`, …); they never run a
//!   tracked query and respond before `dispatch_request` returns;
//! - **snapshot** — everything document- or position-based: the owner mints
//!   a [`crate::snapshot::Snapshot`] for the session and hands the typed
//!   handler to the executor; the answer comes back as
//!   [`OwnerEvent::RequestDone`], and [`GlobalState::handle_event`] responds.
//!
//! Notifications are all owner-inline and reduce to
//! [`crate::mutation::SourceMutation`] batches plus executor jobs
//! (discovery, reloads); [`define_notification_table!`] declares them. Every
//! method string is spelled exactly once, next to a handler whose parameter
//! and result types are the `lsp_types` ones for that method — the tables
//! type-check the wire contract. Anything not listed is
//! [`LspError::RequestNotSupported`] / [`LspError::NotificationNotSupported`];
//! the host decides whether to log.

mod events;
mod notifications;
mod requests;

pub use requests::{initialize_result, server_capabilities};

use crate::{
    error::LspError,
    executor::spawn_read,
    state::{GlobalState, OwnerEvent, Responder, SessionKey, SessionLifecycle},
};

macro_rules! lsp_request_method {
    ($name:tt) => {
        <lsp_types::lsp_request!($name) as lsp_types::request::Request>::METHOD
    };
}
macro_rules! lsp_request_params {
    ($name:tt) => {
        <lsp_types::lsp_request!($name) as lsp_types::request::Request>::Params
    };
}
macro_rules! lsp_request_result {
    ($name:tt) => {
        <lsp_types::lsp_request!($name) as lsp_types::request::Request>::Result
    };
}
macro_rules! lsp_notification_method {
    ($name:tt) => {
        <lsp_types::lsp_notification!($name) as lsp_types::notification::Notification>::METHOD
    };
}
macro_rules! lsp_notification_params {
    ($name:tt) => {
        <lsp_types::lsp_notification!($name) as lsp_types::notification::Notification>::Params
    };
}

fn to_value<T: serde::Serialize>(value: T) -> Result<serde_json::Value, LspError> {
    serde_json::to_value(value).map_err(LspError::RequestSerializeError)
}

/// Declare the request tables. Owner handlers are
/// `fn(&mut GlobalState, SessionKey, Params) -> Result<Result, LspError>`;
/// snapshot handlers are `fn(&Snapshot, Params) -> Result<Result, LspError>`
/// and must be `Send` (they run on the executor).
macro_rules! define_request_tables {
    (
        owner { $($owner_method:tt => $owner_fn:ident),* $(,)? }
        snapshot { $($snap_method:tt => $snap_fn:ident),* $(,)? }
    ) => {
        impl GlobalState {
            /// Route one request. Owner-inline requests respond before this
            /// returns; snapshot requests respond from
            /// [`GlobalState::handle_event`] once their job reports back.
            /// `respond` is called exactly once either way.
            pub fn dispatch_request(
                &mut self,
                session: SessionKey,
                req: lsp_server::Request,
                respond: Responder,
            ) {
                if let Err(error) = self.admit_request(session, &req.method) {
                    respond(Err(error));
                    return;
                }
                match req.method.as_str() {
                    $(
                        lsp_request_method!($owner_method) => {
                            let (_, params): (_, lsp_request_params!($owner_method)) =
                                match req.extract(lsp_request_method!($owner_method)) {
                                    Ok(extracted) => extracted,
                                    Err(error) => {
                                        respond(Err(LspError::RequestExtractError(error)));
                                        return;
                                    }
                                };
                            let result: Result<lsp_request_result!($owner_method), LspError> =
                                requests::$owner_fn(self, session, params);
                            respond(result.and_then(to_value));
                        }
                    )*
                    $(
                        lsp_request_method!($snap_method) => {
                            let (_, params): (_, lsp_request_params!($snap_method)) =
                                match req.extract(lsp_request_method!($snap_method)) {
                                    Ok(extracted) => extracted,
                                    Err(error) => {
                                        respond(Err(LspError::RequestExtractError(error)));
                                        return;
                                    }
                                };
                            let cx = match self.request_cx(session) {
                                Ok(cx) => cx,
                                Err(error) => {
                                    respond(Err(error));
                                    return;
                                }
                            };
                            let snap = self.snapshot(cx);
                            let handle = self.handle();
                            spawn_read(
                                self.executor(),
                                snap,
                                move |snap| {
                                    let result: Result<lsp_request_result!($snap_method), LspError> =
                                        requests::$snap_fn(snap, params);
                                    result.and_then(to_value)
                                },
                                move |outcome| handle.post(OwnerEvent::RequestDone { respond, outcome }),
                            );
                        }
                    )*
                    other => respond(Err(LspError::RequestNotSupported(other.to_owned()))),
                }
            }
        }
    };
}

/// Declare the notification table. Handlers are
/// `fn(&mut GlobalState, SessionKey, Params) -> Result<(), LspError>`.
macro_rules! define_notification_table {
    ( $($method:tt => $handler:ident),* $(,)? ) => {
        impl GlobalState {
            /// Route one notification. `Err` means the notification was not
            /// applied (unknown method, malformed params, a rejected
            /// mutation); the host decides whether to log it. Nothing is
            /// sent to the client from here.
            pub fn dispatch_notification(
                &mut self,
                session: SessionKey,
                notif: lsp_server::Notification,
            ) -> Result<(), LspError> {
                match notif.method.as_str() {
                    $(
                        lsp_notification_method!($method) => {
                            let params: lsp_notification_params!($method) = notif
                                .extract(lsp_notification_method!($method))
                                .map_err(LspError::NotificationExtractError)?;
                            notifications::$handler(self, session, params)
                        }
                    )*
                    other => Err(LspError::NotificationNotSupported(other.to_owned())),
                }
            }
        }
    };
}

define_request_tables! {
    owner {
        "initialize" => initialize,
        "shutdown" => shutdown,
        "workspace/executeCommand" => execute_command,
        "codeLens/resolve" => code_lens_resolve,
    }
    snapshot {
        "textDocument/formatting" => formatting,
    }
}

define_notification_table! {
    "initialized" => initialized,
    "exit" => exit,
    "$/cancelRequest" => cancel_request,
    "$/setTrace" => set_trace,
    "textDocument/didOpen" => did_open,
    "textDocument/didChange" => did_change,
    "textDocument/willSave" => will_save,
    "textDocument/didSave" => did_save,
    "textDocument/didClose" => did_close,
    "workspace/didChangeConfiguration" => did_change_configuration,
    "workspace/didChangeWatchedFiles" => did_change_watched_files,
    "workspace/didChangeWorkspaceFolders" => did_change_workspace_folders,
    "workspace/didCreateFiles" => did_create_files,
    "workspace/didRenameFiles" => did_rename_files,
    "workspace/didDeleteFiles" => did_delete_files,
}

impl GlobalState {
    /// The lifecycle gate every request passes: `initialize` exactly once,
    /// everything else only between `initialize` and `shutdown`.
    fn admit_request(&self, session: SessionKey, method: &str) -> Result<(), LspError> {
        let lifecycle = self.session(session)?.lifecycle;
        let is_initialize = method == lsp_request_method!("initialize");
        match (lifecycle, is_initialize) {
            (SessionLifecycle::Uninitialized, true) | (SessionLifecycle::Initialized, false) => {
                Ok(())
            }
            (SessionLifecycle::Uninitialized, false) => Err(LspError::ServerNotInitialized(
                format!("{method} before initialize"),
            )),
            (SessionLifecycle::Initialized | SessionLifecycle::ShuttingDown, true) => Err(
                LspError::RequestFailed("initialize was already received".to_owned()),
            ),
            (SessionLifecycle::ShuttingDown, false) => {
                Err(LspError::RequestFailed(format!("{method} after shutdown")))
            }
        }
    }
}
