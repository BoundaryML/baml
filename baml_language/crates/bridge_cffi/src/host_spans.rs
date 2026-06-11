//! Bridge-agnostic HostSpanManager — no-op span-depth tracker for `@trace`.
//!
//! Event production has been removed; this struct now only tracks call
//! depth so the PyO3/napi wrappers stay buildable. `@trace` is inert.

use std::collections::HashMap;

/// Manages host-side span tracking for `@trace`.
///
/// Each instance tracks a single async task or thread's span stack.
/// `enter()` / `exit_ok()` / `exit_error()` drive the lifecycle but no
/// longer emit any events.
#[derive(Clone, Default)]
pub struct HostSpanManager {
    /// Names of the active spans (used only for `context_depth`).
    stack: Vec<String>,
}

impl HostSpanManager {
    pub fn new() -> Self {
        Self { stack: Vec::new() }
    }

    /// Deep clone for async context forking.
    pub fn deep_clone(&self) -> Self {
        self.clone()
    }

    /// Enter a new host-language span (`@trace` function start). No-op push.
    pub fn enter(&mut self, name: String, _args: serde_json::Value) {
        self.stack.push(name);
    }

    /// Exit the current span successfully.
    pub fn exit_ok(&mut self) {
        let _ = self.stack.pop();
    }

    /// Exit the current span with an error.
    pub fn exit_error(&mut self, _error_message: String) {
        let _ = self.stack.pop();
    }

    /// Merge tags into the current span. No-op.
    pub fn upsert_tags(&mut self, _tags: HashMap<String, String>) {}

    /// Number of active spans (call depth).
    pub fn context_depth(&self) -> usize {
        self.stack.len()
    }

    /// Build a `HostSpanContext` for passing to `call_function`.
    ///
    /// Always `None` now that no real spans exist.
    pub fn host_span_context(&self) -> Option<bex_events::HostSpanContext> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enter_exit_depth() {
        let mut mgr = HostSpanManager::new();
        assert_eq!(mgr.context_depth(), 0);

        mgr.enter("outer".into(), serde_json::json!({}));
        assert_eq!(mgr.context_depth(), 1);

        mgr.enter("inner".into(), serde_json::json!({}));
        assert_eq!(mgr.context_depth(), 2);

        mgr.exit_ok();
        assert_eq!(mgr.context_depth(), 1);

        mgr.exit_ok();
        assert_eq!(mgr.context_depth(), 0);
    }

    #[test]
    fn test_deep_clone_independent() {
        let mut mgr = HostSpanManager::new();
        mgr.enter("func".into(), serde_json::json!({}));

        let clone = mgr.deep_clone();
        assert_eq!(clone.context_depth(), 1);

        mgr.exit_ok();
        assert_eq!(mgr.context_depth(), 0);
        assert_eq!(clone.context_depth(), 1); // clone unaffected
    }
}
