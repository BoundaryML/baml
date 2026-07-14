//! Ambient, dispatch-scoped request cancellation.
//!
//! The ingress runtime dispatches one request at a time per worker thread and
//! hands the operation-owned [`sys_types::CancellationToken`] to
//! `handle_request_with_cancellation`. Handler signatures stay stable: the
//! token is installed thread-locally for the duration of exactly one dispatch
//! and observed only at *safe points* — handler entry, immediately after the
//! bounded source-gate acquisition, and handler completion.
//!
//! Under the abort-profile invariant this token is never connected to Salsa's
//! unwind-based local cancellation: observing it returns a typed
//! `LspError::RequestCanceled`, it does not unwind an in-flight query.

use std::cell::RefCell;

thread_local! {
    static CURRENT_REQUEST_CANCELLATION: RefCell<Option<sys_types::CancellationToken>> =
        const { RefCell::new(None) };
}

/// Installs a token as the current dispatch's cancellation for this thread.
/// The previous value is restored on drop, so a nested dispatch (e.g.
/// `handle_request` delegating through `handle_request_with_cancellation`)
/// stays balanced.
pub(crate) struct RequestCancellationScope {
    previous: Option<sys_types::CancellationToken>,
}

impl RequestCancellationScope {
    pub(crate) fn enter(token: Option<sys_types::CancellationToken>) -> Self {
        let previous = CURRENT_REQUEST_CANCELLATION.with(|current| current.replace(token));
        Self { previous }
    }
}

impl Drop for RequestCancellationScope {
    fn drop(&mut self) {
        CURRENT_REQUEST_CANCELLATION.with(|current| {
            *current.borrow_mut() = self.previous.take();
        });
    }
}

/// Safe-point check for the ambient dispatch token. `false` when no token is
/// installed (plain `handle_request`, notifications, WASM single-thread path).
pub(crate) fn current_request_is_cancelled() -> bool {
    CURRENT_REQUEST_CANCELLATION.with(|current| {
        current
            .borrow()
            .as_ref()
            .is_some_and(sys_types::CancellationToken::is_cancelled)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_installs_observes_and_restores_the_ambient_token() {
        assert!(!current_request_is_cancelled());
        let outer = sys_types::CancellationToken::new();
        let scope = RequestCancellationScope::enter(Some(outer.clone()));
        assert!(!current_request_is_cancelled());

        {
            let inner = sys_types::CancellationToken::new();
            inner.cancel();
            let _nested = RequestCancellationScope::enter(Some(inner));
            assert!(current_request_is_cancelled());
        }

        // The outer (uncancelled) token is restored after the nested scope.
        assert!(!current_request_is_cancelled());
        outer.cancel();
        assert!(current_request_is_cancelled());
        drop(scope);
        assert!(!current_request_is_cancelled());
    }
}
