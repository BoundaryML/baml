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
    /// task's own token was cancelled (`Local`), or the thread computing a
    /// query this task was blocked on unwound first (`PropagatedPanic` -
    /// which, despite the name, is not evidence of a panic; see the
    /// conversion below).
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
            // `PropagatedPanic` is a misnomer at our layer. Salsa raises it
            // whenever the thread computing a query we blocked on released
            // its claim by unwinding for anything other than that thread's
            // OWN `$/cancelRequest` token (`release_panicking` in
            // `function/sync.rs` reads only the local flag), and the common
            // cause here is the ordinary one: a mutation landed and
            // cancelled the producer mid-query. So it says the same thing
            // `PendingWrite` says, and it gets the same answer - which is
            // also what rust-analyzer does, mapping every `Cancelled` to
            // `ContentModified` without inspecting the variant.
            //
            // Nothing is lost if a real panic WAS the cause: the thread that
            // actually panicked reports it, with the panic hook's message,
            // location, and backtrace, and the client's retry runs the same
            // query again on a thread that will panic in its own right.
            TaskFailure::Cancelled(salsa::Cancelled::PropagatedPanic) => LspError::ContentModified(
                "sources changed while the request was computing".to_owned(),
            ),
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Every salsa unwind that is not this task's own `$/cancelRequest`
    /// answers `ContentModified`, so the client retries.
    ///
    /// `PropagatedPanic` is the one that reads wrong: salsa names it for a
    /// panic, but raises it whenever the thread computing a query we blocked
    /// on unwound for a reason other than ITS OWN local token — which a
    /// mutation cancelling that thread satisfies. Reporting it as an
    /// internal error put a red toast on ordinary typing.
    #[test]
    fn a_salsa_unwind_asks_the_client_to_retry() {
        assert!(matches!(
            LspError::from(TaskFailure::Cancelled(salsa::Cancelled::PendingWrite)),
            LspError::ContentModified(_)
        ));
        assert!(matches!(
            LspError::from(TaskFailure::Cancelled(salsa::Cancelled::PropagatedPanic)),
            LspError::ContentModified(_)
        ));
        assert!(matches!(
            LspError::from(TaskFailure::Cancelled(salsa::Cancelled::Local)),
            LspError::RequestCanceled(_)
        ));
        // A real panic is reported by the thread that panicked, and stays an
        // internal error.
        assert!(matches!(
            LspError::from(TaskFailure::Panicked("boom".to_owned())),
            LspError::Internal(_)
        ));
    }
}
