use internal_baml_diagnostics::Span;

use super::{Expression, FieldType, WithSpan};

#[derive(Debug, Clone)]
pub struct Adapter {
    pub from: FieldType,
    pub to: FieldType,
    pub converter: Expression,

    pub(crate) span: Span,
}

impl WithSpan for Adapter {
    fn span(&self) -> &Span {
        &self.span
    }
}
