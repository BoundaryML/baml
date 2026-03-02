//! Type and field attributes.
//!
//! Currently only contains SAP metadata (i.e. controls for the schema-aligned parser).
//!
//! These live in `baml_base` b/c they're shared by `baml_compiler_tir::Ty`
//! (TIR) and `baml_type::Ty` (VIR+).

/// A SAP attribute value. Shared across all `@sap.*` attributes.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SapAttrValue {
    Never,

    /// A constant value expression (literal, null, empty container, etc.)
    ConstValueExpr(SapConstValue),
}

/// A constant value for SAP annotations.
///
/// Represents SAP attr args, e.g. `@sap.class_completed_field_missing("Loading...")`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SapConstValue {
    Null,
    String(String),
    Int(i64),
    Float(String), // String to avoid f64 Eq/Hash issues
    Bool(bool),
    EmptyList,
    EmptyMap,
    EnumValue {
        enum_name: String,
        variant_name: String,
    },
}

/// Non-default type attributes.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TyAttrInner {
    pub sap_in_progress: SapAttrValue,
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

/// Non-default field attributes.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FieldAttrInner {
    pub sap_class_completed_field_missing: SapAttrValue,
    pub sap_class_in_progress_field_missing: SapAttrValue,
}

/// Attributes intrinsic to a field (not its type).
///
/// `Option<Box<...>>` makes the default (`None`) memory-cheap (8 bytes).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct FieldAttr(pub Option<Box<FieldAttrInner>>);
