//! Read-side database views handed to pool tasks.
//!
//! A [`Snapshot`] is a cloned Salsa handle plus the request-immutable context
//! a handler needs. Its lifetime is a *deadlock invariant*, not a convention:
//! Salsa's `set_*` blocks the owner thread until every cloned handle is
//! dropped, so a snapshot parked in a struct, cached, or held by a thread that
//! is itself waiting on the owner would stall every mutation (and on wasm,
//! where the owner is the only thread, hang the tab). The type enforces the
//! shape structurally: no `Clone`, a crate-private constructor, and the only
//! consumer is [`crate::executor::Executor::spawn`], whose wrapper owns the
//! snapshot, lends it to the task as `&Snapshot`, and drops it *before*
//! reporting the result.

use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use baml_db::ProjectDatabase;

use crate::{
    error::LspError,
    position_codec::PositionEncoding,
    roots::RootsView,
    state::{OpenDocuments, SourceRevision, TokenBaseline},
};

/// Request-immutable session context baked into a snapshot at mint time.
#[derive(Debug, Clone)]
pub struct RequestCx {
    /// The session's negotiated position encoding (UTF-16 before/without
    /// negotiation, matching the LSP default).
    pub encoding: PositionEncoding,
    pub snippet_support: bool,
    /// The session's semantic-token baselines at mint time (immutable view;
    /// the owner replaces the map on store). Delta requests diff against
    /// this on the pool.
    pub token_baselines: Arc<std::collections::HashMap<PathBuf, TokenBaseline>>,
}

impl Default for RequestCx {
    fn default() -> Self {
        Self {
            encoding: PositionEncoding::UTF16,
            snippet_support: false,
            token_baselines: Arc::new(std::collections::HashMap::new()),
        }
    }
}

/// A read-only view of the database at one revision.
///
/// See the module docs for the lifetime invariant. `Debug` deliberately omits
/// the database.
pub struct Snapshot {
    db: ProjectDatabase,
    revision: SourceRevision,
    roots: Arc<RootsView>,
    open_documents: Arc<OpenDocuments>,
    cx: RequestCx,
    live: Arc<AtomicUsize>,
}

impl std::fmt::Debug for Snapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Snapshot")
            .field("revision", &self.revision)
            .field("cx", &self.cx)
            .finish_non_exhaustive()
    }
}

impl Snapshot {
    /// Mint a snapshot. Crate-private: only the owner state does this, and
    /// only to hand the result straight to an executor.
    pub(crate) fn mint(
        db: &ProjectDatabase,
        revision: SourceRevision,
        roots: Arc<RootsView>,
        open_documents: Arc<OpenDocuments>,
        cx: RequestCx,
        live: Arc<AtomicUsize>,
    ) -> Self {
        live.fetch_add(1, Ordering::SeqCst);
        Self {
            db: db.clone(),
            revision,
            roots,
            open_documents,
            cx,
            live,
        }
    }

    pub fn db(&self) -> &ProjectDatabase {
        &self.db
    }

    pub fn revision(&self) -> SourceRevision {
        self.revision
    }

    pub fn roots(&self) -> &RootsView {
        &self.roots
    }

    pub fn open_documents(&self) -> &OpenDocuments {
        &self.open_documents
    }

    pub fn cx(&self) -> &RequestCx {
        &self.cx
    }

    /// The Salsa cancellation token for this handle. Cancelling it makes the
    /// next query entry inside the task unwind with `Cancelled::Local`,
    /// which [`run_guarded`] reports as [`TaskFailure::Cancelled`].
    pub fn cancellation_token(&self) -> salsa::CancellationToken {
        salsa::Database::cancellation_token(&self.db)
    }
}

impl Drop for Snapshot {
    fn drop(&mut self) {
        self.live.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Why a guarded task produced no value.
#[derive(Debug)]
pub enum TaskFailure {
    /// Salsa unwound the task: a mutation landed (`PendingWrite`), the
    /// task's own token was cancelled (`Local`), or another thread's query
    /// panicked (`PropagatedPanic`).
    Cancelled(salsa::Cancelled),
    /// A real panic. The message is the payload's `&str`/`String` if it had
    /// one.
    Panicked(String),
}

impl From<TaskFailure> for LspError {
    fn from(failure: TaskFailure) -> Self {
        match failure {
            TaskFailure::Cancelled(salsa::Cancelled::PendingWrite) => LspError::ContentModified(
                "sources changed while the request was computing".to_owned(),
            ),
            TaskFailure::Cancelled(salsa::Cancelled::Local) => {
                LspError::RequestCanceled("request canceled".to_owned())
            }
            TaskFailure::Cancelled(salsa::Cancelled::PropagatedPanic) => {
                tracing::error!("a query on another thread panicked");
                LspError::Internal("a query on another thread panicked".to_owned())
            }
            // `Cancelled` is `#[non_exhaustive]`.
            TaskFailure::Cancelled(other) => LspError::Internal(format!("cancelled: {other}")),
            TaskFailure::Panicked(message) => {
                tracing::error!(%message, "request handler panicked");
                LspError::Internal(format!("handler panicked: {message}"))
            }
        }
    }
}

/// Run `f` against `snap` with the panic/cancellation boundary in place.
///
/// Salsa's own catch is innermost (it re-raises foreign payloads), the real
/// panic catch outermost. Both take `AssertUnwindSafe`: the closure borrows a
/// `!Sync` database handle, which is the rust-analyzer precedent.
pub fn run_guarded<T>(
    snap: &Snapshot,
    f: impl FnOnce(&Snapshot) -> Result<T, LspError>,
) -> Result<Result<T, LspError>, TaskFailure> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        salsa::Cancelled::catch(std::panic::AssertUnwindSafe(|| f(snap)))
    })) {
        Ok(Ok(result)) => Ok(result),
        Ok(Err(cancelled)) => Err(TaskFailure::Cancelled(cancelled)),
        Err(payload) => Err(TaskFailure::Panicked(panic_message(payload.as_ref()))),
    }
}

fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_owned()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "<non-string panic payload>".to_owned()
    }
}
