//! Resolved type representation — the output of type resolution.
//!
//! The resolved type `Ty` is `baml_type::Ty` — TIR no longer defines its own.
//! This module re-exports the shared type vocabulary so existing
//! `crate::ty::…` / `baml_compiler2_tir::ty::…` paths keep working.

pub use baml_base::{Name, attr::TyAttr};
// `Package`, `QualifiedTypeName`, the reserved-package / synthetic-effect-param
// constants, `is_synthetic_effect_param`, the resolved type `Ty`, its
// function-parameter types, and the rendering strategy now all live in
// `baml_type` (the single home for the shared type vocabulary). Re-exported
// here so existing `crate::ty::…` paths keep working.
pub use baml_type::{
    CanonicalTyRender, Freshness, FunctionParamMode, FunctionParamTy, MediaKind, Package,
    PrimitiveType, QualifiedTypeName, RESERVED_USER_PACKAGE, SYNTHETIC_EFFECT_PARAM_PREFIX, Ty,
    TyRenderStrategy, is_synthetic_effect_param,
};

/// Re-export `baml_base::Literal` as `LiteralValue` for backward compatibility.
pub type LiteralValue = baml_base::Literal;
