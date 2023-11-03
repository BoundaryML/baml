use std::hash::Hash;

use crate::ast::{Span, WithSpan};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VariableId(pub u32);

impl VariableId {
    /// Used for range bounds when iterating over BTreeMaps.
    pub const MIN: VariableId = VariableId(0);
    /// Used for range bounds when iterating over BTreeMaps.
    pub const MAX: VariableId = VariableId(u32::MAX);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Variable {
    /// Entire unparsed text of the variable.  (input.something.bar)
    pub text: String,
    /// [input, something, bar]
    pub path: Vec<String>,
    pub span: Span,
}

impl Variable {
    /// Unique Key
    pub fn key(&self) -> String {
        format!("{{//BAML_CLIENT_REPLACE_ME_MAGIC_{}//}}", self.text)
    }
}

impl Hash for Variable {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.text.hash(state);
    }
}

impl WithSpan for Variable {
    fn span(&self) -> &Span {
        &self.span
    }
}
