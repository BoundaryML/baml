mod code_block;
mod comment_block;
mod prompt_text;
mod top;
mod traits;
mod variable;

pub use code_block::{CodeBlock, CodeBlockId, CodeType};
pub use comment_block::{CommentBlock, CommentBlockId};
pub use internal_baml_diagnostics::Span;
pub use prompt_text::{PromptText, PromptTextId};
pub use top::Top;
pub use variable::{Variable, VariableId};

pub use traits::{WithDocumentation, WithName, WithSpan};

#[derive(Debug, Clone)]
pub struct PromptAst {
    pub tops: Vec<Top>,
}

impl PromptAst {
    pub fn new() -> Self {
        PromptAst { tops: Vec::new() }
    }

    /// Iterate over all the top-level items in the schema.
    pub fn iter_tops(&self) -> impl Iterator<Item = (TopId, &Top)> {
        self.tops
            .iter()
            .enumerate()
            .map(|(top_idx, top)| (top_idx_to_top_id(top_idx, top), top))
    }
}

/// An identifier for a top-level item in a schema AST. Use the `schema[top_id]`
/// syntax to resolve the id to an `ast::Top`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TopId {
    /// An enum declaration
    CommentBlock(CommentBlockId),

    PromptText(PromptTextId),
    // A class declaration
    CodeBlock(CodeBlockId),
    // EmptyLine(EmptyLineId),

    // PromptText(PromptTextId),
}

impl TopId {
    pub fn as_comment_block_id(self) -> Option<CommentBlockId> {
        match self {
            TopId::CommentBlock(id) => Some(id),
            _ => None,
        }
    }

    pub fn as_prompt_text_id(self) -> Option<PromptTextId> {
        match self {
            TopId::PromptText(id) => Some(id),
            _ => None,
        }
    }

    pub fn as_code_block_id(self) -> Option<CodeBlockId> {
        match self {
            TopId::CodeBlock(id) => Some(id),
            _ => None,
        }
    }
}

impl std::ops::Index<TopId> for PromptAst {
    type Output = Top;

    fn index(&self, index: TopId) -> &Self::Output {
        let idx = match index {
            TopId::CommentBlock(CommentBlockId(idx)) => idx,
            TopId::PromptText(PromptTextId(idx)) => idx,
            TopId::CodeBlock(CodeBlockId(idx)) => idx,
        };

        &self.tops[idx as usize]
    }
}

fn top_idx_to_top_id(top_idx: usize, top: &Top) -> TopId {
    match top {
        Top::CommentBlock(_) => TopId::CommentBlock(CommentBlockId(top_idx as u32)),
        Top::PromptText(_) => TopId::PromptText(PromptTextId(top_idx as u32)),
        Top::CodeBlock(_) => TopId::CodeBlock(CodeBlockId(top_idx as u32)),
    }
}
