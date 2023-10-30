use crate::ast::{traits::WithSpan, CodeBlock, CommentBlock, PromptText, Span};

/// Enum for distinguishing between top-level entries
#[derive(Debug, Clone)]
pub enum Top {
    CommentBlock(CommentBlock),
    CodeBlock(CodeBlock),
    PromptText(PromptText),
    WhiteSpace(String, Span),
}

impl Top {
    /// A string saying what kind of item this is.
    pub fn get_type(&self) -> &str {
        match self {
            Top::CommentBlock(_) => "comment_block",
            Top::PromptText(_) => "prompt_text",
            Top::CodeBlock(_) => "code_block",
            Top::WhiteSpace(..) => "white_space",
        }
    }

    // /// The name of the item.
    // pub fn identifier(&self) -> &Identifier {
    //     match self {
    //         // Top::CompositeType(ct) => &ct.name,
    //         Top::CommentBlock(x) => &x.name,
    //         Top::PromptText(x) => &x.name,
    //         Top::CodeBlock(x) => &x.name,
    //     }
    // }

    // /// The name of the item.
    // pub fn name(&self) -> &str {
    //     &self.identifier().name
    // }

    pub fn as_comment_block(&self) -> Option<&CommentBlock> {
        match self {
            Top::CommentBlock(comment_block) => Some(comment_block),
            _ => None,
        }
    }

    pub fn as_prompt_text(&self) -> Option<&PromptText> {
        match self {
            Top::PromptText(prompt_text) => Some(prompt_text),
            _ => None,
        }
    }

    pub fn as_code_block(&self) -> Option<&CodeBlock> {
        match self {
            Top::CodeBlock(code_block) => Some(code_block),
            _ => None,
        }
    }
}

impl WithSpan for Top {
    fn span(&self) -> &Span {
        match self {
            Top::CommentBlock(en) => en.span(),
            Top::PromptText(en) => en.span(),
            Top::CodeBlock(en) => en.span(),
            Top::WhiteSpace(_, span) => span,
        }
    }
}
