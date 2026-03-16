//! Type and field attributes.
//!
//! Currently only contains SAP metadata (i.e. controls for the schema-aligned parser).
//!
//! These live in `baml_base` b/c they're shared by `baml_compiler_tir::Ty`
//! (TIR) and `baml_type::Ty` (VIR+).

/// A SAP attribute value. Shared across all `@sap.*` attributes.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SapAttrValue<N> {
    Never,

    /// A constant value expression (literal, null, empty container, etc.)
    ConstValueExpr(SapConstValue<N>),
}
impl<N> SapAttrValue<N> {
    pub fn try_map_name<M, F: FnOnce(&N) -> Option<M>>(self, f: F) -> Option<SapAttrValue<M>> {
        match self {
            SapAttrValue::Never => Some(SapAttrValue::Never),
            SapAttrValue::ConstValueExpr(v) => v.try_map_name(f).map(SapAttrValue::ConstValueExpr),
        }
    }
    pub fn expect_map_name<M, F: FnOnce(&N) -> Option<M>>(
        self,
        f: F,
    ) -> Result<SapAttrValue<M>, N> {
        match self {
            SapAttrValue::Never => Ok(SapAttrValue::Never),
            SapAttrValue::ConstValueExpr(v) => {
                v.expect_map_name(f).map(SapAttrValue::ConstValueExpr)
            }
        }
    }
}

/// A constant value for SAP annotations.
///
/// Represents SAP attr args, e.g. `@sap.class_completed_field_missing("Loading...")`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SapConstValue<N> {
    Null,
    String(String),
    Int(i64),
    Float(String), // String to avoid f64 Eq/Hash issues
    Bool(bool),
    EmptyList,
    EmptyMap,
    EnumValue { enum_name: N, variant_name: String },
}
impl<N> SapConstValue<N> {
    pub fn try_map_name<M, F: FnOnce(&N) -> Option<M>>(self, f: F) -> Option<SapConstValue<M>> {
        match self {
            SapConstValue::Null => Some(SapConstValue::Null),
            SapConstValue::String(s) => Some(SapConstValue::String(s)),
            SapConstValue::Int(i) => Some(SapConstValue::Int(i)),
            SapConstValue::Float(s) => Some(SapConstValue::Float(s)),
            SapConstValue::Bool(b) => Some(SapConstValue::Bool(b)),
            SapConstValue::EmptyList => Some(SapConstValue::EmptyList),
            SapConstValue::EmptyMap => Some(SapConstValue::EmptyMap),
            SapConstValue::EnumValue {
                enum_name,
                variant_name,
            } => f(&enum_name).map(|enum_name| SapConstValue::EnumValue {
                enum_name,
                variant_name,
            }),
        }
    }
    pub fn expect_map_name<M, F: FnOnce(&N) -> Option<M>>(
        self,
        f: F,
    ) -> Result<SapConstValue<M>, N> {
        match self {
            SapConstValue::Null => Ok(SapConstValue::Null),
            SapConstValue::String(s) => Ok(SapConstValue::String(s)),
            SapConstValue::Int(i) => Ok(SapConstValue::Int(i)),
            SapConstValue::Float(s) => Ok(SapConstValue::Float(s)),
            SapConstValue::Bool(b) => Ok(SapConstValue::Bool(b)),
            SapConstValue::EmptyList => Ok(SapConstValue::EmptyList),
            SapConstValue::EmptyMap => Ok(SapConstValue::EmptyMap),
            SapConstValue::EnumValue {
                enum_name,
                variant_name,
            } => match f(&enum_name) {
                Some(enum_name) => Ok(SapConstValue::EnumValue {
                    enum_name,
                    variant_name,
                }),
                None => Err(enum_name),
            },
        }
    }
}

/// Non-default type attributes.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TyAttrInner<N> {
    pub sap_in_progress: SapAttrValue<N>,
}
impl<N> TyAttrInner<N> {
    pub fn try_map_name<M, F: FnOnce(&N) -> Option<M>>(self, f: F) -> Option<TyAttrInner<M>> {
        Some(TyAttrInner {
            sap_in_progress: self.sap_in_progress.try_map_name(f)?,
        })
    }
    pub fn expect_map_name<M, F: FnOnce(&N) -> Option<M>>(self, f: F) -> Result<TyAttrInner<M>, N> {
        Ok(TyAttrInner {
            sap_in_progress: self.sap_in_progress.expect_map_name(f)?,
        })
    }
}

/// Attributes intrinsic to a type expression.
///
/// Carried on every `Ty` variant from HIR through runtime.
/// Describes how values of this type behave during streaming.
///
/// `Option<Box<...>>` makes the default (`None`) memory-cheap (8 bytes).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TyAttr<N>(pub Option<Box<TyAttrInner<N>>>);

impl<N> Default for TyAttr<N> {
    fn default() -> Self {
        Self(None)
    }
}
impl<N> TyAttr<N> {
    /// Returns true if all attributes are at their default values.
    pub fn is_default(&self) -> bool {
        self.0.is_none()
    }
    pub fn try_map_name<M, F: FnOnce(&N) -> Option<M>>(self, f: F) -> Option<TyAttr<M>> {
        match self.0 {
            None => Some(TyAttr(None)),
            Some(inner) => Some(TyAttr(Some(Box::new(inner.try_map_name(f)?)))),
        }
    }
    /// Like `try_map_name`, but returns an error if the name is not mapped.
    pub fn expect_map_name<M, F: FnOnce(&N) -> Option<M>>(self, f: F) -> Result<TyAttr<M>, N> {
        match self.0 {
            None => Ok(TyAttr(None)),
            Some(inner) => Ok(TyAttr(Some(Box::new(inner.expect_map_name(f)?)))),
        }
    }
}

/// Non-default field attributes.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FieldAttrInner<N> {
    pub sap_class_completed_field_missing: SapAttrValue<N>,
    pub sap_class_in_progress_field_missing: SapAttrValue<N>,
}
impl<N> FieldAttrInner<N> {
    pub fn try_map_name<M, F: Fn(&N) -> Option<M>>(self, f: F) -> Option<FieldAttrInner<M>> {
        Some(FieldAttrInner {
            sap_class_completed_field_missing: self
                .sap_class_completed_field_missing
                .try_map_name(&f)?,
            sap_class_in_progress_field_missing: self
                .sap_class_in_progress_field_missing
                .try_map_name(f)?,
        })
    }
    pub fn expect_map_name<M, F: Fn(&N) -> Option<M>>(self, f: F) -> Result<FieldAttrInner<M>, N> {
        Ok(FieldAttrInner {
            sap_class_completed_field_missing: self
                .sap_class_completed_field_missing
                .expect_map_name(&f)?,
            sap_class_in_progress_field_missing: self
                .sap_class_in_progress_field_missing
                .expect_map_name(f)?,
        })
    }
}

/// Attributes intrinsic to a field (not its type).
///
/// `Option<Box<...>>` makes the default (`None`) memory-cheap (8 bytes).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FieldAttr<N>(pub Option<Box<FieldAttrInner<N>>>);

impl<N> Default for FieldAttr<N> {
    fn default() -> Self {
        Self(None)
    }
}
impl<N> FieldAttr<N> {
    pub fn try_map_name<M, F: Fn(&N) -> Option<M>>(self, f: F) -> Option<FieldAttr<M>> {
        match self.0 {
            None => Some(FieldAttr(None)),
            Some(inner) => Some(FieldAttr(Some(Box::new(inner.try_map_name(f)?)))),
        }
    }
    pub fn expect_map_name<M, F: Fn(&N) -> Option<M>>(self, f: F) -> Result<FieldAttr<M>, N> {
        match self.0 {
            None => Ok(FieldAttr(None)),
            Some(inner) => Ok(FieldAttr(Some(Box::new(inner.expect_map_name(f)?)))),
        }
    }
}
