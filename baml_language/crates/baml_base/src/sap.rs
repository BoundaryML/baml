//! SAP (Streaming Annotation Protocol) attribute types.
//!
//! These types represent compiler-internal streaming annotations that are
//! populated by Phase 3 (stream type generation) and consumed by the
//! deserializer at runtime.
//!
//! # Compact representation
//!
//! `TyAttr` and `FieldAttr` use `Option<Box<...>>` internally so that the
//! common default case is a single null pointer (8 bytes) with no heap
//! allocation. Non-default attributes (populated by Phase 3) are
//! heap-allocated. This keeps the `Ty` enum small enough to avoid stack
//! overflows in deeply recursive type operations.

/// A SAP attribute value. Shared across all three attributes.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SapAttrValue {
    /// Use the default behavior for this type/field.
    ///
    /// What "default" means depends on which attribute this appears in:
    /// - `sap_in_progress`: type streams normally (partial values visible as they arrive)
    /// - `sap_on_error`: type uses its natural error fallback
    /// - `sap_missing`: field uses default missing behavior
    DefaultForType,

    /// The bottom value — semantics depend on which attribute:
    /// - `sap_in_progress(never)`: type has no in-progress representation
    ///   (only appears when complete)
    /// - `sap_on_error(never)`: errors are not recoverable — propagate failure
    /// - `sap_missing(never)`: field is absent until streaming begins
    Never,

    /// A constant value expression (literal, null, empty container, etc.)
    ConstValueExpr(SapConstValue),
}

/// A constant value for SAP annotations.
///
/// These are the values that can appear in @sap.missing("Loading..."),
/// @`sap.on_error(0)`, etc.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SapConstValue {
    Null,
    String(String),
    Int(i64),
    Float(String), // String to avoid f64 Eq/Hash issues
    Bool(bool),
    EmptyList,
    EmptyMap,
}

/// Non-default type attributes (heap-allocated).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct TyAttrInner {
    sap_in_progress: SapAttrValue,
    sap_on_error: SapAttrValue,
}

/// Attributes intrinsic to a type expression.
///
/// Carried on every `Ty` variant from HIR through runtime.
/// Describes how values of this type behave during streaming and on error.
///
/// Default values are represented as `None` (8 bytes, no allocation).
/// Non-default values are heap-allocated behind a `Box`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct TyAttr(Option<Box<TyAttrInner>>);

impl TyAttr {
    /// Returns true if all attributes are at their default values.
    pub fn is_default(&self) -> bool {
        self.0.is_none()
    }

    /// The value this type takes while it is still being streamed.
    /// `Never` means the type has no in-progress representation — it only
    /// appears when fully parsed.
    pub fn sap_in_progress(&self) -> &SapAttrValue {
        self.0
            .as_ref()
            .map_or(&SapAttrValue::DefaultForType, |inner| {
                &inner.sap_in_progress
            })
    }

    /// The fallback value for this type when a parse error occurs during
    /// streaming. `Never` means errors are not recoverable.
    pub fn sap_on_error(&self) -> &SapAttrValue {
        self.0
            .as_ref()
            .map_or(&SapAttrValue::DefaultForType, |inner| &inner.sap_on_error)
    }

    /// Construct a `TyAttr` from explicit values.
    ///
    /// Returns the compact default representation when both values are
    /// `DefaultForType`.
    pub fn new(sap_in_progress: SapAttrValue, sap_on_error: SapAttrValue) -> Self {
        if sap_in_progress == SapAttrValue::DefaultForType
            && sap_on_error == SapAttrValue::DefaultForType
        {
            TyAttr(None)
        } else {
            TyAttr(Some(Box::new(TyAttrInner {
                sap_in_progress,
                sap_on_error,
            })))
        }
    }
}

/// Non-default field attributes (heap-allocated).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct FieldAttrInner {
    sap_missing: SapAttrValue,
}

/// Attributes intrinsic to a field (not its type).
///
/// Carried on field definitions. Describes what happens when the field's
/// key is absent from the partial JSON object during streaming.
///
/// Default values are represented as `None` (8 bytes, no allocation).
/// Non-default values are heap-allocated behind a `Box`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct FieldAttr(Option<Box<FieldAttrInner>>);

impl FieldAttr {
    /// Returns true if all attributes are at their default values.
    pub fn is_default(&self) -> bool {
        self.0.is_none()
    }

    /// The value of this field before it begins streaming (when the
    /// field's JSON key is absent from the partial object).
    /// `Never` means the field is absent until it starts.
    pub fn sap_missing(&self) -> &SapAttrValue {
        self.0
            .as_ref()
            .map_or(&SapAttrValue::DefaultForType, |inner| &inner.sap_missing)
    }

    /// Construct a `FieldAttr` from an explicit value.
    ///
    /// Returns the compact default representation when the value is
    /// `DefaultForType`.
    pub fn new(sap_missing: SapAttrValue) -> Self {
        if sap_missing == SapAttrValue::DefaultForType {
            FieldAttr(None)
        } else {
            FieldAttr(Some(Box::new(FieldAttrInner { sap_missing })))
        }
    }
}
