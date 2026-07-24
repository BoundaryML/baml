use std::fmt::{self, Write};

use borsh::{BorshDeserialize, BorshSerialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub enum DiagnosticIdentifierKind {
    Type,
    Function,
    Field,
    Variable,
    EnumVariant,
    Attribute,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub enum DiagnosticMessageKind {
    Identifier(DiagnosticIdentifierKind),
    TypeExpression,
    Code,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub struct DiagnosticMessageHighlight {
    pub start: u32,
    pub end: u32,
    pub kind: DiagnosticMessageKind,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiagnosticText {
    text: String,
    highlights: Vec<DiagnosticMessageHighlight>,
}

impl DiagnosticText {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_inline_code(text: impl Into<String>) -> Self {
        let text = text.into();
        let mut highlights = Vec::new();
        let mut cursor = 0;
        while let Some(open) = text[cursor..].find('`').map(|offset| cursor + offset) {
            let content_start = open + 1;
            let Some(close) = text[content_start..]
                .find('`')
                .map(|offset| content_start + offset)
            else {
                break;
            };
            if content_start < close {
                highlights.push(DiagnosticMessageHighlight {
                    start: u32::try_from(content_start).expect("diagnostic text exceeds 4 GiB"),
                    end: u32::try_from(close).expect("diagnostic text exceeds 4 GiB"),
                    kind: DiagnosticMessageKind::Code,
                });
            }
            cursor = close + 1;
        }
        Self { text, highlights }
    }

    #[must_use]
    pub fn text(mut self, value: impl fmt::Display) -> Self {
        write!(self.text, "{value}").expect("writing to a String cannot fail");
        self
    }

    #[must_use]
    pub fn identifier(self, value: impl fmt::Display, kind: DiagnosticIdentifierKind) -> Self {
        self.fragment(value, DiagnosticMessageKind::Identifier(kind))
    }

    #[must_use]
    pub fn type_expr(self, value: impl fmt::Display) -> Self {
        self.fragment(value, DiagnosticMessageKind::TypeExpression)
    }

    #[must_use]
    pub fn code(self, value: impl fmt::Display) -> Self {
        self.fragment(value, DiagnosticMessageKind::Code)
    }

    pub fn as_str(&self) -> &str {
        &self.text
    }

    pub fn highlights(&self) -> &[DiagnosticMessageHighlight] {
        &self.highlights
    }

    pub fn into_parts(self) -> (String, Vec<DiagnosticMessageHighlight>) {
        (self.text, self.highlights)
    }

    fn fragment(mut self, value: impl fmt::Display, kind: DiagnosticMessageKind) -> Self {
        self.text.push('`');
        let start = u32::try_from(self.text.len()).expect("diagnostic text exceeds 4 GiB");
        write!(self.text, "{value}").expect("writing to a String cannot fail");
        let end = u32::try_from(self.text.len()).expect("diagnostic text exceeds 4 GiB");
        self.text.push('`');
        self.highlights
            .push(DiagnosticMessageHighlight { start, end, kind });
        self
    }
}

impl From<String> for DiagnosticText {
    fn from(text: String) -> Self {
        Self::from_inline_code(text)
    }
}

impl From<&str> for DiagnosticText {
    fn from(text: &str) -> Self {
        Self::from_inline_code(text)
    }
}

impl From<&String> for DiagnosticText {
    fn from(text: &String) -> Self {
        Self::from_inline_code(text)
    }
}

impl fmt::Display for DiagnosticText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_preserves_plain_text_and_records_fragment_ranges() {
        let text = DiagnosticText::new()
            .text("expected ")
            .type_expr("map<string, int>")
            .text(", found ")
            .identifier("value", DiagnosticIdentifierKind::Variable);

        assert_eq!(text.as_str(), "expected `map<string, int>`, found `value`");
        assert_eq!(
            &text.as_str()[text.highlights()[0].start as usize..text.highlights()[0].end as usize],
            "map<string, int>"
        );
        assert_eq!(
            &text.as_str()[text.highlights()[1].start as usize..text.highlights()[1].end as usize],
            "value"
        );
    }

    #[test]
    fn string_conversion_highlights_backticked_code() {
        let text = DiagnosticText::from("unknown variable `value`");

        assert_eq!(text.highlights().len(), 1);
        assert_eq!(
            &text.as_str()[text.highlights()[0].start as usize..text.highlights()[0].end as usize],
            "value"
        );
        assert_eq!(text.highlights()[0].kind, DiagnosticMessageKind::Code);
    }
}
