//! Type attributes.
//!
//! Currently only contains SAP metadata (i.e. controls for the schema-aligned parser).
//!
//! These live in `baml_base` b/c they're shared by `baml_compiler_tir::Ty`
//! (TIR) and `baml_type::Ty` (VIR+).

/// Binary present/absent flag for SAP attributes.
///
/// Used instead of `bool` for extensibility — future attributes may
/// need additional states (e.g., `Inherited`, `Explicit`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TyAttrValue {
    #[default]
    Unset,
    Set,
}

impl TyAttrValue {
    /// Merge two flags: Set wins over Unset.
    ///
    /// Used when `@@stream.*` block attributes merge into field attributes
    /// during `stream_expand`.
    #[must_use]
    pub fn or(self, other: TyAttrValue) -> TyAttrValue {
        match (self, other) {
            (TyAttrValue::Set, _) | (_, TyAttrValue::Set) => TyAttrValue::Set,
            _ => TyAttrValue::Unset,
        }
    }
}

/// Non-default type attributes for SAP streaming behavior.
///
/// BEP-006 v12 defines three binary (present/absent) SAP attributes
/// that control how the schema-aligned parser handles each streaming state.
/// These are type-level attributes carried on `baml_type::Ty` and only
/// lowered to field-level when consumed by the SAP runtime.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TyAttrInner {
    /// `@sap.parse_without_null`: during parsing (both in-progress and done
    /// states), exclude `null` from the type's parse candidates. Null is
    /// present in the generated type only as a synthesized pending default,
    /// not as a valid parse result.
    pub sap_parse_without_null: TyAttrValue,

    /// `@sap.pending_never`: no value is yielded for this field while it is
    /// in the pending state (i.e., the JSON key has not yet appeared).
    pub sap_pending_never: TyAttrValue,

    /// `@sap.in_progress_never`: no value is yielded for this field while it
    /// is in the in-progress state (i.e., the JSON value has started but is
    /// not yet complete).
    pub sap_in_progress_never: TyAttrValue,
}

/// Attributes intrinsic to a type expression.
///
/// Carried on every `Ty` variant from HIR through runtime.
/// Describes how values of this type behave during streaming.
///
/// `Option<Box<...>>` makes the default (`None`) memory-cheap (8 bytes).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct TyAttr(pub Option<Box<TyAttrInner>>);

impl TyAttr {
    /// Returns true if all attributes are at their default values.
    pub fn is_default(&self) -> bool {
        self.0.is_none()
    }
}
