//! Prompt AST types for representing structured prompts.

use baml_base::Span;
use bex_vm_types::MediaValue;
use serde::Deserialize;

/// A prompt AST node with an optional source span.
/// Also allows us to attach metadata to nodes if we so want.
#[derive(Debug, Clone, Deserialize)]
pub struct PromptAst {
    #[serde(default, skip_deserializing)]
    pub span: Option<Span>,
    #[serde(flatten)]
    pub node: PromptAstNode,
}

impl PromptAst {
    /// Create a new `PromptAst` with a span.
    pub fn new(span: Span, node: PromptAstNode) -> Self {
        Self {
            span: Some(span),
            node,
        }
    }

    /// Create a new `PromptAst` without a span (for runtime-generated nodes).
    pub fn without_span(node: PromptAstNode) -> Self {
        Self { span: None, node }
    }
}

/// The different kinds of nodes in a prompt AST.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum PromptAstNode {
    /// A plain string.
    Str(String),
    /// A media value (image, audio, video, etc.).
    #[serde(skip_deserializing)]
    Media(MediaValue),
    /// A message with a role, content, and optional metadata.
    Message {
        role: String,
        content: Box<PromptAst>,
        metadata: serde_json::Map<String, serde_json::Value>,
    },
    /// A sequence of prompt nodes.
    Vec(Vec<PromptAst>),
}
