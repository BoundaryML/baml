use crate::ast::{Expression, FieldType};
use internal_baml_diagnostics::Span;

/// The BAML syntax for a call to std::fetch().
pub struct FetchCall {
    pub output_type: (FieldType, Span),
    pub argument: Expression,
}