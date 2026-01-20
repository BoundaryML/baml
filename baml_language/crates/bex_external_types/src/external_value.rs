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

impl BexValue {
    /// Check if this value is null.
    pub fn is_null(&self) -> bool {
        matches!(self, BexValue::External(BexExternalValue::Null))
    }

    /// Try to get as an integer.
    pub fn as_int(&self) -> Option<i64> {
        match self {
            BexValue::External(BexExternalValue::Int(i)) => Some(*i),
            _ => None,
        }
    }

    /// Try to get as a float.
    pub fn as_float(&self) -> Option<f64> {
        match self {
            BexValue::External(BexExternalValue::Float(f)) => Some(*f),
            _ => None,
        }
    }

    /// Try to get as a boolean.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            BexValue::External(BexExternalValue::Bool(b)) => Some(*b),
            _ => None,
        }
    }

    /// Try to get as an object handle.
    pub fn as_object(&self) -> Option<&Handle> {
        match self {
            BexValue::Opaque(handle) => Some(handle),
            _ => None,
        }
    }

    /// Get the type name for error messages.
    pub fn type_name(&self) -> &'static str {
        match self {
            BexValue::Opaque(_) => "object",
            BexValue::External(s) => s.type_name(),
        }
    }

    /// Try to get as a snapshot reference.
    pub fn as_snapshot(&self) -> Option<&BexExternalValue> {
        match self {
            BexValue::External(s) => Some(s),
            _ => None,
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_external_value_primitives() {
        assert!(BexValue::default().is_null());
        assert_eq!(BexValue::from(42i64).as_int(), Some(42));
        assert_eq!(BexValue::from(1.23f64).as_float(), Some(1.23));
        assert_eq!(BexValue::from(true).as_bool(), Some(true));
    }

    #[test]
    fn test_external_value_from() {
        let v: BexValue = 42i64.into();
        assert_eq!(v.as_int(), Some(42));

        let v: BexValue = 1.23f64.into();
        assert_eq!(v.as_float(), Some(1.23));

        let v: BexValue = true.into();
        assert_eq!(v.as_bool(), Some(true));

        let v: BexValue = "hello".into();
        assert!(matches!(v, BexValue::External(BexExternalValue::String(_))));
    }

    #[test]
    fn test_external_value_object() {
        let handle = Handle::new_detached(42);
        let v = BexValue::Opaque(handle);

        assert!(v.as_object().is_some());
        assert_eq!(v.as_object().unwrap().slab_key(), 42);
    }

    #[test]
    fn test_external_value_type_name() {
        assert_eq!(BexValue::default().type_name(), "null");
        assert_eq!(BexValue::from(0i64).type_name(), "int");
        assert_eq!(BexValue::from(0.0f64).type_name(), "float");
        assert_eq!(BexValue::from(false).type_name(), "bool");

        let handle = Handle::new_detached(0);
        assert_eq!(BexValue::Opaque(handle).type_name(), "object");
    }

    #[test]
    fn test_external_value_equality() {
        assert_eq!(BexValue::default(), BexValue::default());
        assert_eq!(BexValue::from(42i64), BexValue::from(42i64));
        assert_ne!(BexValue::from(42i64), BexValue::from(43i64));
        assert_ne!(BexValue::from(42i64), BexValue::from(42.0f64));
    }

    #[test]
    fn test_external_value_default() {
        let v: BexValue = Default::default();
        assert!(v.is_null());
    }
}
