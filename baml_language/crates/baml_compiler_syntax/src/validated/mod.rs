use std::{borrow::Cow, path::Path};

use rowan::ast::AstNode;

use crate::{
    BamlLanguage, SyntaxElement, SyntaxKind, SyntaxNode, SyntaxNodeExt as _, SyntaxToken, TextRange,
};

mod arena;
mod generated_schema;
mod generated_tokens;
pub mod nodes;

pub use arena::{
    NodeId, Validated, ValidatedChildren, ValidatedDirectElements, ValidatedElement,
    ValidatedElements, ValidatedSyntaxToken, ValidatedTree,
};
pub use generated_schema::*;

pub mod tokens {
    pub use super::{
        AssignmentOp, BacktickString, BinaryOp, BindingKeyword, ByteString, HeaderComment,
        KeywordLiteral, QuotedString, RawString, UnaryOp, ValidatedToken as Token,
        generated_tokens::*, is_word_like,
    };
}

use generated_tokens::{
    And, AndAnd, AndEquals, Caret, CaretEquals, Const, Equals, EqualsEquals, Greater,
    GreaterEquals, GreaterGreater, GreaterGreaterEquals, Instanceof, Less, LessEquals, LessLess,
    LessLessEquals, Let, Minus, MinusEquals, Not, NotEquals, OrOr, Percent, PercentEquals, Pipe,
    PipeEquals, Plus, PlusEquals, QuestionQuestion, Slash, SlashEquals, Star, StarEquals,
};

pub trait ValidatedToken {
    fn span(&self) -> TextRange;
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum BindingKeyword {
    Let(Let),
    Const(Const),
}

impl ValidatedToken for BindingKeyword {
    fn span(&self) -> TextRange {
        match self {
            Self::Let(token) => token.span(),
            Self::Const(token) => token.span(),
        }
    }
}

impl FromCST for BindingKeyword {
    fn from_cst(element: SyntaxElement) -> Result<Self, StrongAstError> {
        match element.kind() {
            SyntaxKind::KW_LET => Let::from_cst(element).map(Self::Let),
            SyntaxKind::KW_CONST => Const::from_cst(element).map(Self::Const),
            found => Err(StrongAstError::UnexpectedKindDesc {
                expected_desc: "KW_LET or KW_CONST".into(),
                found,
                at: element.text_range(),
            }),
        }
    }
}

impl std::fmt::Display for BindingKeyword {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Let(token) => token.fmt(formatter),
            Self::Const(token) => token.fmt(formatter),
        }
    }
}

#[derive(Debug)]
pub enum AssignmentOp {
    Equals(Equals),
    PlusEquals(PlusEquals),
    MinusEquals(MinusEquals),
    StarEquals(StarEquals),
    SlashEquals(SlashEquals),
    PercentEquals(PercentEquals),
    AndEquals(AndEquals),
    PipeEquals(PipeEquals),
    CaretEquals(CaretEquals),
    LessLessEquals(LessLessEquals),
    GreaterGreaterEquals(GreaterGreaterEquals),
}

#[derive(Debug)]
pub enum BinaryOp {
    EqualsEquals(EqualsEquals),
    NotEquals(NotEquals),
    Less(Less),
    Greater(Greater),
    LessEquals(LessEquals),
    GreaterEquals(GreaterEquals),
    AndAnd(AndAnd),
    OrOr(OrOr),
    And(And),
    Pipe(Pipe),
    Caret(Caret),
    Instanceof(Instanceof),
    LessLess(LessLess),
    GreaterGreater(GreaterGreater),
    Plus(Plus),
    Minus(Minus),
    Star(Star),
    Slash(Slash),
    Percent(Percent),
    Equals(Equals),
    PlusEquals(PlusEquals),
    MinusEquals(MinusEquals),
    StarEquals(StarEquals),
    SlashEquals(SlashEquals),
    PercentEquals(PercentEquals),
    AndEquals(AndEquals),
    PipeEquals(PipeEquals),
    CaretEquals(CaretEquals),
    LessLessEquals(LessLessEquals),
    GreaterGreaterEquals(GreaterGreaterEquals),
    QuestionQuestion(QuestionQuestion),
}

impl ValidatedToken for BinaryOp {
    fn span(&self) -> TextRange {
        match self {
            Self::EqualsEquals(token) => token.span(),
            Self::NotEquals(token) => token.span(),
            Self::Less(token) => token.span(),
            Self::Greater(token) => token.span(),
            Self::LessEquals(token) => token.span(),
            Self::GreaterEquals(token) => token.span(),
            Self::AndAnd(token) => token.span(),
            Self::OrOr(token) => token.span(),
            Self::And(token) => token.span(),
            Self::Pipe(token) => token.span(),
            Self::Caret(token) => token.span(),
            Self::Instanceof(token) => token.span(),
            Self::LessLess(token) => token.span(),
            Self::GreaterGreater(token) => token.span(),
            Self::Plus(token) => token.span(),
            Self::Minus(token) => token.span(),
            Self::Star(token) => token.span(),
            Self::Slash(token) => token.span(),
            Self::Percent(token) => token.span(),
            Self::Equals(token) => token.span(),
            Self::PlusEquals(token) => token.span(),
            Self::MinusEquals(token) => token.span(),
            Self::StarEquals(token) => token.span(),
            Self::SlashEquals(token) => token.span(),
            Self::PercentEquals(token) => token.span(),
            Self::AndEquals(token) => token.span(),
            Self::PipeEquals(token) => token.span(),
            Self::CaretEquals(token) => token.span(),
            Self::LessLessEquals(token) => token.span(),
            Self::GreaterGreaterEquals(token) => token.span(),
            Self::QuestionQuestion(token) => token.span(),
        }
    }
}

impl FromCST for BinaryOp {
    fn from_cst(element: SyntaxElement) -> Result<Self, StrongAstError> {
        let token = StrongAstError::assert_is_token(element)?;
        let span = token.text_range();
        match token.kind() {
            SyntaxKind::EQUALS_EQUALS => Ok(Self::EqualsEquals(EqualsEquals::new_from_span(span))),
            SyntaxKind::NOT_EQUALS => Ok(Self::NotEquals(NotEquals::new_from_span(span))),
            SyntaxKind::LESS => Ok(Self::Less(Less::new_from_span(span))),
            SyntaxKind::GREATER => Ok(Self::Greater(Greater::new_from_span(span))),
            SyntaxKind::LESS_EQUALS => Ok(Self::LessEquals(LessEquals::new_from_span(span))),
            SyntaxKind::GREATER_EQUALS => {
                Ok(Self::GreaterEquals(GreaterEquals::new_from_span(span)))
            }
            SyntaxKind::AND_AND => Ok(Self::AndAnd(AndAnd::new_from_span(span))),
            SyntaxKind::OR_OR => Ok(Self::OrOr(OrOr::new_from_span(span))),
            SyntaxKind::AND => Ok(Self::And(And::new_from_span(span))),
            SyntaxKind::PIPE => Ok(Self::Pipe(Pipe::new_from_span(span))),
            SyntaxKind::CARET => Ok(Self::Caret(Caret::new_from_span(span))),
            SyntaxKind::KW_INSTANCEOF => Ok(Self::Instanceof(Instanceof::new_from_span(span))),
            SyntaxKind::LESS_LESS => Ok(Self::LessLess(LessLess::new_from_span(span))),
            SyntaxKind::GREATER_GREATER => {
                Ok(Self::GreaterGreater(GreaterGreater::new_from_span(span)))
            }
            SyntaxKind::PLUS => Ok(Self::Plus(Plus::new_from_span(span))),
            SyntaxKind::MINUS => Ok(Self::Minus(Minus::new_from_span(span))),
            SyntaxKind::STAR => Ok(Self::Star(Star::new_from_span(span))),
            SyntaxKind::SLASH => Ok(Self::Slash(Slash::new_from_span(span))),
            SyntaxKind::PERCENT => Ok(Self::Percent(Percent::new_from_span(span))),
            SyntaxKind::EQUALS => Ok(Self::Equals(Equals::new_from_span(span))),
            SyntaxKind::PLUS_EQUALS => Ok(Self::PlusEquals(PlusEquals::new_from_span(span))),
            SyntaxKind::MINUS_EQUALS => Ok(Self::MinusEquals(MinusEquals::new_from_span(span))),
            SyntaxKind::STAR_EQUALS => Ok(Self::StarEquals(StarEquals::new_from_span(span))),
            SyntaxKind::SLASH_EQUALS => Ok(Self::SlashEquals(SlashEquals::new_from_span(span))),
            SyntaxKind::PERCENT_EQUALS => {
                Ok(Self::PercentEquals(PercentEquals::new_from_span(span)))
            }
            SyntaxKind::AND_EQUALS => Ok(Self::AndEquals(AndEquals::new_from_span(span))),
            SyntaxKind::PIPE_EQUALS => Ok(Self::PipeEquals(PipeEquals::new_from_span(span))),
            SyntaxKind::CARET_EQUALS => Ok(Self::CaretEquals(CaretEquals::new_from_span(span))),
            SyntaxKind::LESS_LESS_EQUALS => {
                Ok(Self::LessLessEquals(LessLessEquals::new_from_span(span)))
            }
            SyntaxKind::GREATER_GREATER_EQUALS => Ok(Self::GreaterGreaterEquals(
                GreaterGreaterEquals::new_from_span(span),
            )),
            SyntaxKind::QUESTION_QUESTION => Ok(Self::QuestionQuestion(
                QuestionQuestion::new_from_span(span),
            )),
            found => Err(StrongAstError::UnexpectedKindDesc {
                expected_desc: "binary operator".into(),
                found,
                at: span,
            }),
        }
    }
}

#[derive(Debug)]
pub enum UnaryOp {
    Not(Not),
    Minus(Minus),
}

impl FromCST for UnaryOp {
    fn from_cst(element: SyntaxElement) -> Result<Self, StrongAstError> {
        match element.kind() {
            SyntaxKind::NOT => Not::from_cst(element).map(Self::Not),
            SyntaxKind::MINUS => Minus::from_cst(element).map(Self::Minus),
            found => Err(StrongAstError::UnexpectedKindDesc {
                expected_desc: "unary operator".into(),
                found,
                at: element.text_range(),
            }),
        }
    }
}

impl ValidatedToken for UnaryOp {
    fn span(&self) -> TextRange {
        match self {
            Self::Not(token) => token.span(),
            Self::Minus(token) => token.span(),
        }
    }
}

#[must_use]
pub fn is_word_like(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::WORD
            | SyntaxKind::KW_CLIENT
            | SyntaxKind::KW_CLASS
            | SyntaxKind::KW_ENUM
            | SyntaxKind::KW_FUNCTION
            | SyntaxKind::KW_IMPLEMENTS
            | SyntaxKind::KW_IMPLEMENT
            | SyntaxKind::KW_EXTENDS
            | SyntaxKind::KW_REQUIRES
            | SyntaxKind::KW_INTERFACE
            | SyntaxKind::KW_SPAWN
            | SyntaxKind::KW_AWAIT
    )
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeywordLiteral {
    pub token_span: TextRange,
}

impl KeywordLiteral {
    #[must_use]
    pub const fn new_from_span(token_span: TextRange) -> Self {
        Self { token_span }
    }
}

impl FromCST for KeywordLiteral {
    fn from_cst(element: SyntaxElement) -> Result<Self, StrongAstError> {
        let token = StrongAstError::assert_is_token(element)?;
        match token.kind() {
            SyntaxKind::KW_TRUE | SyntaxKind::KW_FALSE | SyntaxKind::KW_NULL => {
                Ok(Self::new_from_span(token.text_range()))
            }
            found => Err(StrongAstError::UnexpectedKindDesc {
                expected_desc: "boolean or null literal".into(),
                found,
                at: token.text_range(),
            }),
        }
    }
}

impl ValidatedToken for KeywordLiteral {
    fn span(&self) -> TextRange {
        self.token_span
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct QuotedString {
    pub token_span: TextRange,
}

impl QuotedString {
    #[must_use]
    pub const fn new_from_span(token_span: TextRange) -> Self {
        Self { token_span }
    }
}

impl FromCST for QuotedString {
    fn from_cst(element: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(element)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::STRING_LITERAL)?;
        let start = node
            .first_child_or_token_by_kind(&|kind| kind == SyntaxKind::QUOTE)
            .ok_or_else(|| StrongAstError::missing(SyntaxKind::QUOTE, node.text_range()))?;
        Ok(Self::new_from_span(TextRange::new(
            start.text_range().start(),
            node.text_range().end(),
        )))
    }
}

impl ValidatedToken for QuotedString {
    fn span(&self) -> TextRange {
        self.token_span
    }
}

impl KnownKind for QuotedString {
    fn kind() -> SyntaxKind {
        SyntaxKind::STRING_LITERAL
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RawString {
    pub token_span: TextRange,
}

impl FromCST for RawString {
    fn from_cst(element: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(element)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::RAW_STRING_LITERAL)?;
        let start = node
            .first_child_token_of_kind(SyntaxKind::HASH)
            .ok_or_else(|| StrongAstError::missing(SyntaxKind::HASH, node.text_range()))?;
        Ok(Self {
            token_span: TextRange::new(start.text_range().start(), node.text_range().end()),
        })
    }
}

impl ValidatedToken for RawString {
    fn span(&self) -> TextRange {
        self.token_span
    }
}

impl KnownKind for RawString {
    fn kind() -> SyntaxKind {
        SyntaxKind::RAW_STRING_LITERAL
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BacktickString {
    pub token_span: TextRange,
    pub dedent_safe: bool,
}

impl FromCST for BacktickString {
    fn from_cst(element: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(element)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::BACKTICK_STRING_LITERAL)?;
        let start = node
            .first_child_token_of_kind(SyntaxKind::BACKTICK)
            .ok_or_else(|| StrongAstError::missing(SyntaxKind::BACKTICK, node.text_range()))?;
        let dedent_safe = !node.children().any(|child| match child.kind() {
            SyntaxKind::BACKTICK_FOR_OPEN
            | SyntaxKind::BACKTICK_ENDFOR
            | SyntaxKind::BACKTICK_IF_OPEN
            | SyntaxKind::BACKTICK_ELSE_IF
            | SyntaxKind::BACKTICK_ELSE
            | SyntaxKind::BACKTICK_ENDIF => true,
            SyntaxKind::BACKTICK_INTERPOLATION => child.text().to_string().contains('\n'),
            _ => false,
        });
        Ok(Self {
            token_span: TextRange::new(start.text_range().start(), node.text_range().end()),
            dedent_safe,
        })
    }
}

impl ValidatedToken for BacktickString {
    fn span(&self) -> TextRange {
        self.token_span
    }
}

impl KnownKind for BacktickString {
    fn kind() -> SyntaxKind {
        SyntaxKind::BACKTICK_STRING_LITERAL
    }
}

#[derive(Debug)]
pub struct ByteString {
    pub token_span: TextRange,
}

impl FromCST for ByteString {
    fn from_cst(element: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(element)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::BYTE_STRING_LITERAL)?;
        let start = node
            .first_child_or_token_by_kind(&|kind| kind == SyntaxKind::WORD)
            .ok_or_else(|| StrongAstError::missing(SyntaxKind::WORD, node.text_range()))?;
        Ok(Self {
            token_span: TextRange::new(start.text_range().start(), node.text_range().end()),
        })
    }
}

impl ValidatedToken for ByteString {
    fn span(&self) -> TextRange {
        self.token_span
    }
}

impl KnownKind for ByteString {
    fn kind() -> SyntaxKind {
        SyntaxKind::BYTE_STRING_LITERAL
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HeaderComment {
    pub token_span: TextRange,
}

impl HeaderComment {
    #[must_use]
    pub const fn new_from_span(token_span: TextRange) -> Self {
        Self { token_span }
    }
}

impl FromCST for HeaderComment {
    fn from_cst(element: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(element)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::HEADER_COMMENT)?;
        let first = node
            .first_child_token_of_kind(SyntaxKind::SLASH)
            .ok_or_else(|| StrongAstError::missing(SyntaxKind::SLASH, node.text_range()))?;
        Ok(Self::new_from_span(TextRange::new(
            first.text_range().start(),
            node.text_range().end(),
        )))
    }
}

impl ValidatedToken for HeaderComment {
    fn span(&self) -> TextRange {
        self.token_span
    }
}

impl KnownKind for HeaderComment {
    fn kind() -> SyntaxKind {
        SyntaxKind::HEADER_COMMENT
    }
}

pub trait FromCST: Sized {
    fn from_cst(element: SyntaxElement) -> Result<Self, StrongAstError>;
}

impl<T> FromCST for T
where
    T: AstNode<Language = BamlLanguage>,
{
    fn from_cst(element: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(element)?;
        let found = node.kind();
        let at = node.text_range();
        T::cast(node).ok_or_else(|| StrongAstError::UnexpectedKindDesc {
            expected_desc: std::any::type_name::<T>().into(),
            found,
            at,
        })
    }
}

pub trait KnownKind {
    fn kind() -> SyntaxKind;
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StrongAstError {
    #[error("expected {expected:?}, found {found:?} at {at:?}")]
    UnexpectedKind {
        expected: SyntaxKind,
        found: SyntaxKind,
        at: TextRange,
    },
    #[error("expected {expected_desc}, found {found:?} at {at:?}")]
    UnexpectedKindDesc {
        expected_desc: Cow<'static, str>,
        found: SyntaxKind,
        at: TextRange,
    },
    #[error("expected {expected:?}, but it was missing from {parent:?}")]
    MissingExpectedElement {
        expected: SyntaxKind,
        parent: TextRange,
    },
    #[error("expected {desc}, but it was missing from {parent:?}")]
    MissingExpectedElementDesc {
        desc: Cow<'static, str>,
        parent: TextRange,
    },
    #[error("unexpected element at {at:?} in {parent:?}")]
    UnexpectedAdditionalElement { parent: TextRange, at: TextRange },
    #[error("invalid {kind:?} structure at {at:?}")]
    InvalidStructure { kind: SyntaxKind, at: TextRange },
    #[error("expected a token, found a node at {at:?}")]
    ShouldBeToken { at: TextRange },
    #[error("expected a node, found a token at {at:?}")]
    ShouldBeNode { at: TextRange },
}

impl StrongAstError {
    pub fn assert_kind_node(node: &SyntaxNode, expected: SyntaxKind) -> Result<(), Self> {
        if node.kind() == expected {
            Ok(())
        } else {
            Err(Self::UnexpectedKind {
                expected,
                found: node.kind(),
                at: node.text_range(),
            })
        }
    }

    pub fn assert_kind_token(token: &SyntaxToken, expected: SyntaxKind) -> Result<(), Self> {
        if token.kind() == expected {
            Ok(())
        } else {
            Err(Self::UnexpectedKind {
                expected,
                found: token.kind(),
                at: token.text_range(),
            })
        }
    }

    #[must_use]
    pub fn missing_desc(desc: impl Into<Cow<'static, str>>, parent: TextRange) -> Self {
        Self::MissingExpectedElementDesc {
            desc: desc.into(),
            parent,
        }
    }

    #[must_use]
    pub const fn missing(expected: SyntaxKind, parent: TextRange) -> Self {
        Self::MissingExpectedElement { expected, parent }
    }

    pub fn assert_is_node(element: SyntaxElement) -> Result<SyntaxNode, Self> {
        match element {
            SyntaxElement::Node(node) => Ok(node),
            SyntaxElement::Token(token) => Err(Self::ShouldBeNode {
                at: token.text_range(),
            }),
        }
    }

    pub fn assert_is_token(element: SyntaxElement) -> Result<SyntaxToken, Self> {
        match element {
            SyntaxElement::Node(node) => Err(Self::ShouldBeToken {
                at: node.text_range(),
            }),
            SyntaxElement::Token(token) => Ok(token),
        }
    }

    #[must_use]
    pub fn print_with_file_context(&self, file_path: impl AsRef<Path>, source: &str) -> String {
        fn line_and_column(source: &str, byte_offset: usize) -> Option<(usize, usize)> {
            let (before, _) = source.split_at_checked(byte_offset)?;
            Some((before.lines().count(), before.lines().last()?.len() + 1))
        }

        let location = |range: TextRange| {
            line_and_column(source, range.start().into())
                .map(|(line, column)| format!("{}:{line}:{column}", file_path.as_ref().display()))
        };

        match self {
            Self::UnexpectedKind {
                expected,
                found,
                at,
            } => location(*at).map_or_else(
                || self.to_string(),
                |location| {
                    format!(
                        "Expected token/node of kind {expected:?}, but found {found:?} at {location}"
                    )
                },
            ),
            Self::UnexpectedKindDesc {
                expected_desc,
                found,
                at,
            } => location(*at).map_or_else(
                || self.to_string(),
                |location| {
                    format!(
                        "Expected token/node {expected_desc}, but found {found:?} at {location}"
                    )
                },
            ),
            Self::MissingExpectedElement { expected, parent } => location(*parent).map_or_else(
                || self.to_string(),
                |location| {
                    format!(
                        "Expected token/node {expected:?}, but was unable to find it in {location}"
                    )
                },
            ),
            Self::MissingExpectedElementDesc { desc, parent } => location(*parent).map_or_else(
                || self.to_string(),
                |location| {
                    format!("Expected token/node {desc}, but was unable to find it in {location}")
                },
            ),
            Self::UnexpectedAdditionalElement { at, .. } => location(*at).map_or_else(
                || self.to_string(),
                |location| format!("Unexpected additional element at {location}"),
            ),
            Self::InvalidStructure { kind, at } => location(*at).map_or_else(
                || self.to_string(),
                |location| format!("Invalid {kind:?} structure at {location}"),
            ),
            Self::ShouldBeNode { at } => location(*at).map_or_else(
                || self.to_string(),
                |location| {
                    format!("An element at {location} was a node when it should have been a token.")
                },
            ),
            Self::ShouldBeToken { at } => location(*at).map_or_else(
                || self.to_string(),
                |location| {
                    format!("An element at {location} was a token when it should have been a node.")
                },
            ),
        }
    }
}

pub struct SyntaxNodeIter {
    elements: Box<dyn Iterator<Item = SyntaxElement>>,
    pub parent: TextRange,
    peeked: Option<SyntaxElement>,
}

impl SyntaxNodeIter {
    #[must_use]
    pub fn new(parent_node: &SyntaxNode) -> Self {
        Self {
            elements: Box::new(
                parent_node
                    .children_with_tokens()
                    .by_kind(|kind| !kind.is_trivia()),
            ),
            parent: parent_node.text_range(),
            peeked: None,
        }
    }

    pub fn expect_next(
        &mut self,
        description: impl Into<Cow<'static, str>>,
    ) -> Result<SyntaxElement, StrongAstError> {
        self.next()
            .ok_or_else(|| StrongAstError::missing_desc(description, self.parent))
    }

    pub fn expect_node(
        &mut self,
        description: impl Into<Cow<'static, str>>,
    ) -> Result<SyntaxNode, StrongAstError> {
        StrongAstError::assert_is_node(self.expect_next(description)?)
    }

    pub fn expect_token(
        &mut self,
        description: impl Into<Cow<'static, str>>,
    ) -> Result<SyntaxToken, StrongAstError> {
        StrongAstError::assert_is_token(self.expect_next(description)?)
    }

    pub fn expect_parse<T: KnownKind + FromCST>(&mut self) -> Result<T, StrongAstError> {
        let element = self
            .next()
            .ok_or_else(|| StrongAstError::missing(T::kind(), self.parent))?;
        T::from_cst(element)
    }

    pub fn expect_end(&mut self) -> Result<(), StrongAstError> {
        match self.next() {
            None => Ok(()),
            Some(element) => Err(StrongAstError::UnexpectedAdditionalElement {
                parent: self.parent,
                at: element.text_range(),
            }),
        }
    }

    pub fn peek(&mut self) -> Option<&SyntaxElement> {
        if self.peeked.is_none() {
            self.peeked = self.elements.next();
        }
        self.peeked.as_ref()
    }

    pub fn next_if(
        &mut self,
        predicate: impl FnOnce(&SyntaxElement) -> bool,
    ) -> Option<SyntaxElement> {
        if self.peek().is_some_and(predicate) {
            self.peeked.take()
        } else {
            None
        }
    }

    pub fn next_if_kind(&mut self, kind: SyntaxKind) -> Option<SyntaxElement> {
        self.next_if(|element| element.kind() == kind)
    }
}

impl Iterator for SyntaxNodeIter {
    type Item = SyntaxElement;

    fn next(&mut self) -> Option<Self::Item> {
        self.peeked.take().or_else(|| self.elements.next())
    }
}
