//! ExternalValue type for FFI boundary.
//!
//! `ExternalValue` is a self-contained value type that can be passed
//! across the FFI boundary without requiring heap access to inspect.
//! Two variants for clear semantics: references vs owned data.

use crate::{BexExternalValue, Handle};

/// A value that can cross the FFI boundary.
///
/// Two variants for clear semantics:
/// - `Object(Handle)`: Reference to a heap object (lazy access)
/// - `Snapshot(Snapshot)`: Owned data (primitives and complex types)
///
/// # Example
///
/// ```ignore
/// let result: ExternalValue = engine.call_function("get_user", &[]).await?;
///
/// match result {
///     ExternalValue::Object(handle) => {
///         // Convert to owned data when needed
///         let snapshot = engine.to_snapshot(result)?;
///     }
///     ExternalValue::Snapshot(snapshot) => {
///         // Already owned, use directly
///         match snapshot {
///             Snapshot::Int(n) => println!("Got int: {}", n),
///             Snapshot::String(s) => println!("Got string: {}", s),
///             _ => {}
///         }
///     }
/// }
/// ```
#[derive(Clone, Debug)]
pub enum BexValue {
    /// Handle to any heap-allocated object (string, array, map, instance, variant, etc.).
    /// Resolve via `BexEngine::to_snapshot()` to get owned data.
    Opaque(Handle),

    /// Owned data (primitives and complex types).
    /// Use this variant when you want to pass data to a function or when
    /// you've already converted a handle to owned data.
    External(BexExternalValue),
}

impl Default for BexValue {
    fn default() -> Self {
        BexValue::External(BexExternalValue::Null)
    }
}

impl From<i64> for BexValue {
    fn from(value: i64) -> Self {
        BexValue::External(BexExternalValue::Int(value))
    }
}

impl From<f64> for BexValue {
    fn from(value: f64) -> Self {
        BexValue::External(BexExternalValue::Float(value))
    }
}

impl From<bool> for BexValue {
    fn from(value: bool) -> Self {
        BexValue::External(BexExternalValue::Bool(value))
    }
}

impl From<Handle> for BexValue {
    fn from(value: Handle) -> Self {
        BexValue::Opaque(value)
    }
}

impl From<BexExternalValue> for BexValue {
    fn from(value: BexExternalValue) -> Self {
        BexValue::External(value)
    }
}

impl From<String> for BexValue {
    fn from(value: String) -> Self {
        BexValue::External(BexExternalValue::String(value))
    }
}

impl From<&str> for BexValue {
    fn from(value: &str) -> Self {
        BexValue::External(BexExternalValue::String(value.to_string()))
    }
}

impl PartialEq for BexValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            // Object handles compare by slab_key (identity)
            (BexValue::Opaque(a), BexValue::Opaque(b)) => a.slab_key() == b.slab_key(),
            // Snapshots compare by value
            (BexValue::External(a), BexValue::External(b)) => a == b,
            _ => false,
        }
    }
}
