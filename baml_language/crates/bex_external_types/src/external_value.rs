//! ExternalValue type for FFI boundary.
//!
//! `ExternalValue` is a self-contained value type that can be passed
//! across the FFI boundary without requiring heap access to inspect.
//! Two variants for clear semantics: references vs owned data.

use crate::{Handle, Snapshot};

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
pub enum ExternalValue {
    /// Handle to any heap-allocated object (string, array, map, instance, variant, etc.).
    /// Resolve via `BexEngine::to_snapshot()` to get owned data.
    Object(Handle),

    /// Owned data (primitives and complex types).
    /// Use this variant when you want to pass data to a function or when
    /// you've already converted a handle to owned data.
    Snapshot(Snapshot),
}

impl Default for ExternalValue {
    fn default() -> Self {
        ExternalValue::Snapshot(Snapshot::Null)
    }
}

impl ExternalValue {
    /// Check if this value is null.
    pub fn is_null(&self) -> bool {
        matches!(self, ExternalValue::Snapshot(Snapshot::Null))
    }

    /// Try to get as an integer.
    pub fn as_int(&self) -> Option<i64> {
        match self {
            ExternalValue::Snapshot(Snapshot::Int(i)) => Some(*i),
            _ => None,
        }
    }

    /// Try to get as a float.
    pub fn as_float(&self) -> Option<f64> {
        match self {
            ExternalValue::Snapshot(Snapshot::Float(f)) => Some(*f),
            _ => None,
        }
    }

    /// Try to get as a boolean.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            ExternalValue::Snapshot(Snapshot::Bool(b)) => Some(*b),
            _ => None,
        }
    }

    /// Try to get as an object handle.
    pub fn as_object(&self) -> Option<&Handle> {
        match self {
            ExternalValue::Object(handle) => Some(handle),
            _ => None,
        }
    }

    /// Get the type name for error messages.
    pub fn type_name(&self) -> &'static str {
        match self {
            ExternalValue::Object(_) => "object",
            ExternalValue::Snapshot(s) => s.type_name(),
        }
    }

    /// Try to get as a snapshot reference.
    pub fn as_snapshot(&self) -> Option<&Snapshot> {
        match self {
            ExternalValue::Snapshot(s) => Some(s),
            _ => None,
        }
    }
}

impl From<i64> for ExternalValue {
    fn from(value: i64) -> Self {
        ExternalValue::Snapshot(Snapshot::Int(value))
    }
}

impl From<f64> for ExternalValue {
    fn from(value: f64) -> Self {
        ExternalValue::Snapshot(Snapshot::Float(value))
    }
}

impl From<bool> for ExternalValue {
    fn from(value: bool) -> Self {
        ExternalValue::Snapshot(Snapshot::Bool(value))
    }
}

impl From<Handle> for ExternalValue {
    fn from(value: Handle) -> Self {
        ExternalValue::Object(value)
    }
}

impl From<Snapshot> for ExternalValue {
    fn from(value: Snapshot) -> Self {
        ExternalValue::Snapshot(value)
    }
}

impl From<String> for ExternalValue {
    fn from(value: String) -> Self {
        ExternalValue::Snapshot(Snapshot::String(value))
    }
}

impl From<&str> for ExternalValue {
    fn from(value: &str) -> Self {
        ExternalValue::Snapshot(Snapshot::String(value.to_string()))
    }
}

impl PartialEq for ExternalValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            // Object handles compare by slab_key (identity)
            (ExternalValue::Object(a), ExternalValue::Object(b)) => a.slab_key() == b.slab_key(),
            // Snapshots compare by value
            (ExternalValue::Snapshot(a), ExternalValue::Snapshot(b)) => a == b,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_external_value_primitives() {
        assert!(ExternalValue::default().is_null());
        assert_eq!(ExternalValue::from(42i64).as_int(), Some(42));
        assert_eq!(ExternalValue::from(1.23f64).as_float(), Some(1.23));
        assert_eq!(ExternalValue::from(true).as_bool(), Some(true));
    }

    #[test]
    fn test_external_value_from() {
        let v: ExternalValue = 42i64.into();
        assert_eq!(v.as_int(), Some(42));

        let v: ExternalValue = 1.23f64.into();
        assert_eq!(v.as_float(), Some(1.23));

        let v: ExternalValue = true.into();
        assert_eq!(v.as_bool(), Some(true));

        let v: ExternalValue = "hello".into();
        assert!(matches!(v, ExternalValue::Snapshot(Snapshot::String(_))));
    }

    #[test]
    fn test_external_value_object() {
        let handle = Handle::new_detached(42);
        let v = ExternalValue::Object(handle);

        assert!(v.as_object().is_some());
        assert_eq!(v.as_object().unwrap().slab_key(), 42);
    }

    #[test]
    fn test_external_value_type_name() {
        assert_eq!(ExternalValue::default().type_name(), "null");
        assert_eq!(ExternalValue::from(0i64).type_name(), "int");
        assert_eq!(ExternalValue::from(0.0f64).type_name(), "float");
        assert_eq!(ExternalValue::from(false).type_name(), "bool");

        let handle = Handle::new_detached(0);
        assert_eq!(ExternalValue::Object(handle).type_name(), "object");
    }

    #[test]
    fn test_external_value_equality() {
        assert_eq!(ExternalValue::default(), ExternalValue::default());
        assert_eq!(ExternalValue::from(42i64), ExternalValue::from(42i64));
        assert_ne!(ExternalValue::from(42i64), ExternalValue::from(43i64));
        assert_ne!(ExternalValue::from(42i64), ExternalValue::from(42.0f64));
    }

    #[test]
    fn test_external_value_default() {
        let v: ExternalValue = Default::default();
        assert!(v.is_null());
    }
}
