//! Shared call context - session-scoped state.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::types::BamlValue;

/// Session-scoped context, shared across multiple calls.
#[derive(Debug, Clone, Default)]
pub struct SharedCallContext {
    /// Call stack for nested function tracking.
    call_stack: Arc<Mutex<Vec<CallStackEntry>>>,
    /// Tags that persist across all calls.
    global_tags: Arc<Mutex<HashMap<String, BamlValue>>>,
}

/// Entry in the call stack.
#[derive(Debug, Clone)]
pub struct CallStackEntry {
    pub call_uuid: uuid::Uuid,
    pub function_name: String,
    pub tags: HashMap<String, BamlValue>,
}

impl SharedCallContext {
    /// Create a new shared context.
    pub fn new() -> Self {
        Self::default()
    }

    /// Push a new call onto the stack.
    pub fn push_call(&self, function_name: impl Into<String>) -> uuid::Uuid {
        let call_uuid = uuid::Uuid::new_v4();
        let entry = CallStackEntry {
            call_uuid,
            function_name: function_name.into(),
            tags: HashMap::new(),
        };

        if let Ok(mut stack) = self.call_stack.lock() {
            stack.push(entry);
        }

        call_uuid
    }

    /// Pop the current call from the stack.
    pub fn pop_call(&self) {
        if let Ok(mut stack) = self.call_stack.lock() {
            stack.pop();
        }
    }

    /// Get the current call depth.
    pub fn call_depth(&self) -> usize {
        self.call_stack.lock().map(|s| s.len()).unwrap_or(0)
    }

    /// Set a global tag.
    pub fn set_global_tag(&self, key: impl Into<String>, value: BamlValue) {
        if let Ok(mut tags) = self.global_tags.lock() {
            tags.insert(key.into(), value);
        }
    }

    /// Get a global tag.
    pub fn get_global_tag(&self, key: &str) -> Option<BamlValue> {
        self.global_tags
            .lock()
            .ok()
            .and_then(|tags| tags.get(key).cloned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_call_stack() {
        let ctx = SharedCallContext::new();
        assert_eq!(ctx.call_depth(), 0);

        let uuid1 = ctx.push_call("func1");
        assert_eq!(ctx.call_depth(), 1);

        let uuid2 = ctx.push_call("func2");
        assert_eq!(ctx.call_depth(), 2);
        assert_ne!(uuid1, uuid2);

        ctx.pop_call();
        assert_eq!(ctx.call_depth(), 1);

        ctx.pop_call();
        assert_eq!(ctx.call_depth(), 0);
    }

    #[test]
    fn test_global_tags() {
        let ctx = SharedCallContext::new();

        ctx.set_global_tag("user_id", BamlValue::from("123"));
        assert_eq!(
            ctx.get_global_tag("user_id").and_then(|v| v.as_str().map(String::from)),
            Some("123".to_string())
        );
    }
}
