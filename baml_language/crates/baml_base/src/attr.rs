//! Type attributes.
//!
//! Contains SAP metadata (controls for the schema-aligned parser).
//!
//! These live in `baml_base` b/c they're shared by `baml_compiler_tir::Ty`
//! (TIR) and `baml_type::Ty` (VIR+).

use serde::{Deserialize, Serialize};

/// Binary present/absent flag for SAP attributes.
///
/// Used instead of `bool` for extensibility — future attributes may
/// need additional states (e.g., `Inherited`, `Explicit`).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default, Serialize, Deserialize,
)]
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

/// Attributes intrinsic to a type expression.
///
/// Carried on every `Ty` variant from HIR through runtime.
/// Describes how values of this type behave during streaming.
///
/// BEP-006 v12 defines three binary (present/absent) SAP attributes
/// that control how the schema-aligned parser handles each streaming state.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Default, Serialize, Deserialize)]
pub struct TyAttr {
    /// `@sap.parse_without_null`: during parsing (both in-progress and done
    /// states), exclude `null` from the type's parse candidates.
    pub sap_parse_without_null: TyAttrValue,

    /// `@sap.pending_never`: no value is yielded for this field while it is
    /// in the pending state (i.e., the JSON key has not yet appeared).
    pub sap_pending_never: TyAttrValue,

    /// `@sap.in_progress_never`: no value is yielded for this field while it
    /// is in the in-progress state (i.e., the JSON value has started but is
    /// not yet complete).
    pub sap_in_progress_never: TyAttrValue,
}

impl TyAttr {
    /// Return the canonical names of all attributes that are `Set`.
    pub fn attr_names(&self) -> Vec<&'static str> {
        let mut names = Vec::new();
        if self.sap_parse_without_null == TyAttrValue::Set {
            names.push("sap.parse_without_null");
        }
        if self.sap_pending_never == TyAttrValue::Set {
            names.push("sap.pending_never");
        }
        if self.sap_in_progress_never == TyAttrValue::Set {
            names.push("sap.in_progress_never");
        }
        names
    }
}
