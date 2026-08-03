//! Shared BAML types exposed to every code generator.
//!
//! The representation itself belongs to `baml_type`: generators consume the
//! same compiler-owned qualified names and the same structurally narrowed type
//! family instead of maintaining a parallel enum with lossy conversions.

pub use baml_type::{
    CodegenFunctionParamTy as CallableParam, CodegenTy as Ty, Freshness,
    FunctionParamMode as CodegenFunctionParamMode, ParamTy, QualifiedTypeName as Name,
};
