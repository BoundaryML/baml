use baml_base::Name;
use text_size::TextRange;

use crate::item_tree::FunctionParam;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateString {
    pub name: Name,
    /// Template parameters with optional type annotations and spans.
    pub params: Vec<FunctionParam>,
    /// Template body text (Jinja template).
    pub body: Option<String>,
    /// Full source span of the template string declaration.
    pub span: TextRange,
}
