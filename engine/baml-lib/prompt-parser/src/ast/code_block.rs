use std::hash::Hash;

use crate::ast::{Span, WithSpan};

use super::Variable;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CodeBlockId(pub u32);

impl CodeBlockId {
    /// Used for range bounds when iterating over BTreeMaps.
    pub const MIN: CodeBlockId = CodeBlockId(0);
    /// Used for range bounds when iterating over BTreeMaps.
    pub const MAX: CodeBlockId = CodeBlockId(u32::MAX);
}

#[derive(Debug, Clone)]
pub enum CodeBlock {
    PrintEnum(PrinterBlock),
    PrintType(PrinterBlock),
    Variable(Variable),
}

impl CodeBlock {
    pub fn as_str<'a>(&'a self) -> &'a str {
        match self {
            CodeBlock::PrintEnum(printer_block) => printer_block.target.text.as_str(),
            CodeBlock::PrintType(printer_block) => printer_block.target.text.as_str(),
            CodeBlock::Variable(variable) => variable.text.as_str(),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PrinterBlock {
    pub printer: Option<(String, Span)>,
    pub target: Variable,
}

impl PrinterBlock {
    /// Unique Key
    pub fn key(&self) -> String {
        format!("{{//BAML_CLIENT_REPLACE_ME_MAGIC_{}//}}", self.target.text)
    }
}

impl Hash for PrinterBlock {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        if let Some(printer) = &self.printer {
            printer.0.hash(state);
        }
        self.target.text.hash(state);
    }
}

impl WithSpan for PrinterBlock {
    fn span(&self) -> &Span {
        &self.target.span()
    }
}

impl WithSpan for CodeBlock {
    fn span(&self) -> &Span {
        match self {
            CodeBlock::Variable(v) => v.span(),
            CodeBlock::PrintEnum(v) => v.span(),
            CodeBlock::PrintType(v) => v.span(),
        }
    }
}
