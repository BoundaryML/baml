use std::collections::HashMap;

use baml_base::FileId;
use text_size::TextRange;

use crate::DiagnosticMessageKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HighlightColor {
    Green,
    Yellow,
    Magenta,
    Cyan,
    BrightYellow,
    BrightBlue,
    BrightMagenta,
    BrightCyan,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HighlightAttributes(u8);

impl HighlightAttributes {
    pub const BOLD: Self = Self(1 << 0);
    pub const DIM: Self = Self(1 << 1);
    pub const ITALIC: Self = Self(1 << 2);
    pub const STRIKETHROUGH: Self = Self(1 << 3);

    pub const fn empty() -> Self {
        Self(0)
    }

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    pub fn insert(&mut self, other: Self) {
        self.0 |= other.0;
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HighlightStyle {
    pub foreground: Option<HighlightColor>,
    pub attributes: HighlightAttributes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HighlightSpan {
    pub range: TextRange,
    pub style: HighlightStyle,
}

pub type SourceHighlights = HashMap<FileId, Vec<HighlightSpan>>;

pub trait DiagnosticMessageHighlighter {
    fn highlight(&self, kind: DiagnosticMessageKind, text: &str) -> Vec<HighlightSpan>;
}
