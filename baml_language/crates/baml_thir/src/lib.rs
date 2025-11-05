//! Typed High-level Intermediate Representation.
//!
//! Provides type checking and inference for BAML.

use std::collections::HashMap;

use baml_base::Name;
use baml_hir::{ClassId, EnumId, FunctionId};

mod types;
pub use types::*;

/// Type inference result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InferenceResult {
    pub return_type: Ty,
    pub param_types: HashMap<Name, Ty>,
    pub errors: Vec<TypeError>,
}

/// Type errors that can occur during type checking.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TypeError {
    TypeMismatch {
        expected: Ty,
        found: Ty,
        span: baml_base::Span,
    },
    UnknownType {
        name: String,
        span: baml_base::Span,
    },
    // Add more variants as needed
}

impl baml_base::Diagnostic for TypeError {
    fn message(&self) -> String {
        match self {
            TypeError::TypeMismatch {
                expected, found, ..
            } => {
                format!("Type mismatch: expected {expected:?}, found {found:?}")
            }
            TypeError::UnknownType { name, .. } => {
                format!("Unknown type: {name}")
            }
        }
    }

    fn span(&self) -> Option<baml_base::Span> {
        match self {
            TypeError::TypeMismatch { span, .. } | TypeError::UnknownType { span, .. } => {
                Some(*span)
            }
        }
    }

    fn severity(&self) -> baml_base::Severity {
        baml_base::Severity::Error
    }
}

/// Helper function for type inference (non-tracked for now)
/// In a real implementation, this would use tracked functions with proper salsa types
pub fn infer_function(db: &dyn salsa::Database, func: FunctionId) -> InferenceResult {
    // TODO: Implement type inference
    // Get function data from HIR
    let _data = baml_hir::function_data(db, func);

    InferenceResult {
        return_type: Ty::Unknown,
        param_types: HashMap::new(),
        errors: vec![],
    }
}

/// Helper function for class type resolution (non-tracked for now)
pub fn class_type(db: &dyn salsa::Database, class: ClassId) -> Ty {
    // TODO: Resolve class type
    let _data = baml_hir::class_data(db, class);
    Ty::Unknown
}

/// Helper function for enum type resolution (non-tracked for now)
pub fn enum_type(_db: &dyn salsa::Database, _enum_id: EnumId) -> Ty {
    // TODO: Resolve enum type
    Ty::Unknown
}
