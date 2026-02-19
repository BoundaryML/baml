use baml_db::baml_compiler_syntax::{SyntaxElement, SyntaxKind, SyntaxNodeExt};
use rowan::{TextRange, TextSize};

use crate::{
    ast::{FromCST, KnownKind, StrongAstError},
    printer::{PrintInfo, Printable, Printer, Shape},
};

pub trait Token {
    fn span(&self) -> TextRange;
}

pub trait KeywordToken: Token {}
macro_rules! define_keyword_tokens {
    ($($keyword:literal => SyntaxKind::$syntax_kind:ident => $name:ident;)*) => {
        $(
            #[derive(Debug, Clone, PartialEq, Eq, Hash)]
            pub struct $name {
                pub token_span: TextRange,
            }
            impl $name {
                /// Does not verify that the span is actually the keyword token.
                pub fn new_from_span(token_span: TextRange) -> Self {
                    Self { token_span }
                }
            }
            impl Token for $name {
                fn span(&self) -> TextRange {
                    self.token_span
                }
            }
            impl KnownKind for $name {
                fn kind() -> SyntaxKind {
                    SyntaxKind::$syntax_kind
                }
            }
            impl FromCST for $name {
                #[inline]
                fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
                    let token = StrongAstError::assert_is_token(elem)?;
                    StrongAstError::assert_kind_token(&token, SyntaxKind::$syntax_kind)?;
                    Ok(Self::new_from_span(token.text_range()))
                }
            }
            impl KeywordToken for $name {}
            impl std::fmt::Display for $name {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    f.write_str($keyword)
                }
            }
        )*
    }
}

define_keyword_tokens! {
    "class" => SyntaxKind::KW_CLASS => Class;
    "enum" => SyntaxKind::KW_ENUM => Enum;
    "function" => SyntaxKind::KW_FUNCTION => Function;
    "client" => SyntaxKind::KW_CLIENT => Client;
    "generator" => SyntaxKind::KW_GENERATOR => Generator;
    "test" => SyntaxKind::KW_TEST => Test;
    "retry_policy" => SyntaxKind::KW_RETRY_POLICY => RetryPolicy;
    "template_string" => SyntaxKind::KW_TEMPLATE_STRING => TemplateString;
    "type_builder" => SyntaxKind::KW_TYPE_BUILDER => TypeBuilder;
    "if" => SyntaxKind::KW_IF => If;
    "else" => SyntaxKind::KW_ELSE => Else;
    "for" => SyntaxKind::KW_FOR => For;
    "while" => SyntaxKind::KW_WHILE => While;
    "let" => SyntaxKind::KW_LET => Let;
    "in" => SyntaxKind::KW_IN => In;
    "break" => SyntaxKind::KW_BREAK => Break;
    "continue" => SyntaxKind::KW_CONTINUE => Continue;
    "return" => SyntaxKind::KW_RETURN => Return;
    "match" => SyntaxKind::KW_MATCH => Match;
    "assert" => SyntaxKind::KW_ASSERT => Assert;
    "watch" => SyntaxKind::KW_WATCH => Watch;
    "instanceof" => SyntaxKind::KW_INSTANCEOF => Instanceof;
    "env" => SyntaxKind::KW_ENV => Env;
    "dynamic" => SyntaxKind::KW_DYNAMIC => Dynamic;
}

pub trait PunctuationToken: Token {}
macro_rules! define_punctuation_tokens {
    ($($punct:literal => SyntaxKind::$syntax_kind:ident => $name:ident;)*) => {
        $(
            #[derive(Debug, Clone, PartialEq, Eq, Hash)]
            pub struct $name {
                pub token_span: TextRange,
            }
            impl $name {
                /// Does not verify that the span is actually the punctuation token.
                pub fn new_from_span(token_span: TextRange) -> Self {
                    Self { token_span }
                }
            }
            impl Token for $name {
                fn span(&self) -> TextRange {
                    self.token_span
                }
            }
            impl KnownKind for $name {
                fn kind() -> SyntaxKind {
                    SyntaxKind::$syntax_kind
                }
            }
            impl FromCST for $name {
                #[inline]
                fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
                    let token = StrongAstError::assert_is_token(elem)?;
                    StrongAstError::assert_kind_token(&token, SyntaxKind::$syntax_kind)?;
                    Ok(Self::new_from_span(token.text_range()))
                }
            }
            impl PunctuationToken for $name {}
            impl std::fmt::Display for $name {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    f.write_str($punct)
                }
            }
        )*
    };
}

define_punctuation_tokens! {
    "{" => SyntaxKind::L_BRACE => LBrace;
    "}" => SyntaxKind::R_BRACE => RBrace;
    "(" => SyntaxKind::L_PAREN => LParen;
    ")" => SyntaxKind::R_PAREN => RParen;
    "[" => SyntaxKind::L_BRACKET => LBracket;
    "]" => SyntaxKind::R_BRACKET => RBracket;
    ":" => SyntaxKind::COLON => Colon;
    "::" => SyntaxKind::DOUBLE_COLON => DoubleColon;
    "," => SyntaxKind::COMMA => Comma;
    ";" => SyntaxKind::SEMICOLON => Semicolon;
    "..." => SyntaxKind::DOT_DOT_DOT => DotDotDot;
    "." => SyntaxKind::DOT => Dot;
    "$" => SyntaxKind::DOLLAR => Dollar;
    "->" => SyntaxKind::ARROW => Arrow;
    "=" => SyntaxKind::EQUALS => Equals;
    "+=" => SyntaxKind::PLUS_EQUALS => PlusEquals;
    "-=" => SyntaxKind::MINUS_EQUALS => MinusEquals;
    "*=" => SyntaxKind::STAR_EQUALS => StarEquals;
    "/=" => SyntaxKind::SLASH_EQUALS => SlashEquals;
    "%=" => SyntaxKind::PERCENT_EQUALS => PercentEquals;
    "&=" => SyntaxKind::AND_EQUALS => AndEquals;
    "|=" => SyntaxKind::PIPE_EQUALS => PipeEquals;
    "^=" => SyntaxKind::CARET_EQUALS => CaretEquals;
    "<<=" => SyntaxKind::LESS_LESS_EQUALS => LessLessEquals;
    ">>=" => SyntaxKind::GREATER_GREATER_EQUALS => GreaterGreaterEquals;
    "=>" => SyntaxKind::FAT_ARROW => FatArrow;
    "@@" => SyntaxKind::AT_AT => AtAt;
    "@" => SyntaxKind::AT => At;
    "|" => SyntaxKind::PIPE => Pipe;
    "?" => SyntaxKind::QUESTION => Question;
    "==" => SyntaxKind::EQUALS_EQUALS => EqualsEquals;
    "!=" => SyntaxKind::NOT_EQUALS => NotEquals;
    "<=" => SyntaxKind::LESS_EQUALS => LessEquals;
    ">=" => SyntaxKind::GREATER_EQUALS => GreaterEquals;
    "<<" => SyntaxKind::LESS_LESS => LessLess;
    ">>" => SyntaxKind::GREATER_GREATER => GreaterGreater;
    "<" => SyntaxKind::LESS => Less;
    ">" => SyntaxKind::GREATER => Greater;
    "&&" => SyntaxKind::AND_AND => AndAnd;
    "||" => SyntaxKind::OR_OR => OrOr;
    "!" => SyntaxKind::NOT => Not;
    "&" => SyntaxKind::AND => And;
    "^" => SyntaxKind::CARET => Caret;
    "~" => SyntaxKind::TILDE => Tilde;
    "++" => SyntaxKind::PLUS_PLUS => PlusPlus;
    "--" => SyntaxKind::MINUS_MINUS => MinusMinus;
    "+" => SyntaxKind::PLUS => Plus;
    "-" => SyntaxKind::MINUS => Minus;
    "*" => SyntaxKind::STAR => Star;
    "/" => SyntaxKind::SLASH => Slash;
    "%" => SyntaxKind::PERCENT => Percent;
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
}

impl BinaryOp {
    #[must_use]
    pub fn span(&self) -> TextRange {
        match self {
            BinaryOp::EqualsEquals(t) => t.span(),
            BinaryOp::NotEquals(t) => t.span(),
            BinaryOp::Less(t) => t.span(),
            BinaryOp::Greater(t) => t.span(),
            BinaryOp::LessEquals(t) => t.span(),
            BinaryOp::GreaterEquals(t) => t.span(),
            BinaryOp::And(t) => t.span(),
            BinaryOp::AndAnd(t) => t.span(),
            BinaryOp::OrOr(t) => t.span(),
            BinaryOp::Pipe(t) => t.span(),
            BinaryOp::Caret(t) => t.span(),
            BinaryOp::Instanceof(t) => t.span(),
            BinaryOp::LessLess(t) => t.span(),
            BinaryOp::GreaterGreater(t) => t.span(),
            BinaryOp::Plus(t) => t.span(),
            BinaryOp::Minus(t) => t.span(),
            BinaryOp::Star(t) => t.span(),
            BinaryOp::Slash(t) => t.span(),
            BinaryOp::Percent(t) => t.span(),
            BinaryOp::Equals(t) => t.span(),
            BinaryOp::PlusEquals(t) => t.span(),
            BinaryOp::MinusEquals(t) => t.span(),
            BinaryOp::StarEquals(t) => t.span(),
            BinaryOp::SlashEquals(t) => t.span(),
            BinaryOp::PercentEquals(t) => t.span(),
            BinaryOp::AndEquals(t) => t.span(),
            BinaryOp::PipeEquals(t) => t.span(),
            BinaryOp::CaretEquals(t) => t.span(),
            BinaryOp::LessLessEquals(t) => t.span(),
            BinaryOp::GreaterGreaterEquals(t) => t.span(),
        }
    }
}

impl FromCST for BinaryOp {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let token = StrongAstError::assert_is_token(elem)?;

        match token.kind() {
            SyntaxKind::EQUALS_EQUALS => Ok(BinaryOp::EqualsEquals(EqualsEquals::new_from_span(
                token.text_range(),
            ))),
            SyntaxKind::NOT_EQUALS => Ok(BinaryOp::NotEquals(NotEquals::new_from_span(
                token.text_range(),
            ))),
            SyntaxKind::LESS => Ok(BinaryOp::Less(Less::new_from_span(token.text_range()))),
            SyntaxKind::GREATER => Ok(BinaryOp::Greater(Greater::new_from_span(
                token.text_range(),
            ))),
            SyntaxKind::LESS_EQUALS => Ok(BinaryOp::LessEquals(LessEquals::new_from_span(
                token.text_range(),
            ))),
            SyntaxKind::GREATER_EQUALS => Ok(BinaryOp::GreaterEquals(
                GreaterEquals::new_from_span(token.text_range()),
            )),
            SyntaxKind::AND => Ok(BinaryOp::And(And::new_from_span(token.text_range()))),
            SyntaxKind::AND_AND => Ok(BinaryOp::AndAnd(AndAnd::new_from_span(token.text_range()))),
            SyntaxKind::OR_OR => Ok(BinaryOp::OrOr(OrOr::new_from_span(token.text_range()))),
            SyntaxKind::PIPE => Ok(BinaryOp::Pipe(Pipe::new_from_span(token.text_range()))),
            SyntaxKind::CARET => Ok(BinaryOp::Caret(Caret::new_from_span(token.text_range()))),
            SyntaxKind::KW_INSTANCEOF => Ok(BinaryOp::Instanceof(Instanceof::new_from_span(
                token.text_range(),
            ))),
            SyntaxKind::LESS_LESS => Ok(BinaryOp::LessLess(LessLess::new_from_span(
                token.text_range(),
            ))),
            SyntaxKind::GREATER_GREATER => Ok(BinaryOp::GreaterGreater(
                GreaterGreater::new_from_span(token.text_range()),
            )),
            SyntaxKind::PLUS => Ok(BinaryOp::Plus(Plus::new_from_span(token.text_range()))),
            SyntaxKind::MINUS => Ok(BinaryOp::Minus(Minus::new_from_span(token.text_range()))),
            SyntaxKind::STAR => Ok(BinaryOp::Star(Star::new_from_span(token.text_range()))),
            SyntaxKind::SLASH => Ok(BinaryOp::Slash(Slash::new_from_span(token.text_range()))),
            SyntaxKind::PERCENT => Ok(BinaryOp::Percent(Percent::new_from_span(
                token.text_range(),
            ))),
            SyntaxKind::EQUALS => Ok(BinaryOp::Equals(Equals::new_from_span(token.text_range()))),
            SyntaxKind::PLUS_EQUALS => Ok(BinaryOp::PlusEquals(PlusEquals::new_from_span(
                token.text_range(),
            ))),
            SyntaxKind::MINUS_EQUALS => Ok(BinaryOp::MinusEquals(MinusEquals::new_from_span(
                token.text_range(),
            ))),
            SyntaxKind::STAR_EQUALS => Ok(BinaryOp::StarEquals(StarEquals::new_from_span(
                token.text_range(),
            ))),
            SyntaxKind::SLASH_EQUALS => Ok(BinaryOp::SlashEquals(SlashEquals::new_from_span(
                token.text_range(),
            ))),
            SyntaxKind::PERCENT_EQUALS => Ok(BinaryOp::PercentEquals(
                PercentEquals::new_from_span(token.text_range()),
            )),
            SyntaxKind::AND_EQUALS => Ok(BinaryOp::AndEquals(AndEquals::new_from_span(
                token.text_range(),
            ))),
            SyntaxKind::PIPE_EQUALS => Ok(BinaryOp::PipeEquals(PipeEquals::new_from_span(
                token.text_range(),
            ))),
            SyntaxKind::CARET_EQUALS => Ok(BinaryOp::CaretEquals(CaretEquals::new_from_span(
                token.text_range(),
            ))),
            SyntaxKind::LESS_LESS_EQUALS => Ok(BinaryOp::LessLessEquals(
                LessLessEquals::new_from_span(token.text_range()),
            )),
            SyntaxKind::GREATER_GREATER_EQUALS => Ok(BinaryOp::GreaterGreaterEquals(
                GreaterGreaterEquals::new_from_span(token.text_range()),
            )),
            _ => Err(StrongAstError::UnexpectedKindDesc {
                expected_desc: "binary operator".into(),
                found: token.kind(),
                at: token.text_range(),
            }),
        }
    }
}

impl Printable for BinaryOp {
    fn print(&self, _shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            BinaryOp::EqualsEquals(t) => printer.print_raw_token(t),
            BinaryOp::NotEquals(t) => printer.print_raw_token(t),
            BinaryOp::Less(t) => printer.print_raw_token(t),
            BinaryOp::Greater(t) => printer.print_raw_token(t),
            BinaryOp::LessEquals(t) => printer.print_raw_token(t),
            BinaryOp::GreaterEquals(t) => printer.print_raw_token(t),
            BinaryOp::AndAnd(t) => printer.print_raw_token(t),
            BinaryOp::OrOr(t) => printer.print_raw_token(t),
            BinaryOp::And(t) => printer.print_raw_token(t),
            BinaryOp::Pipe(t) => printer.print_raw_token(t),
            BinaryOp::Caret(t) => printer.print_raw_token(t),
            BinaryOp::Instanceof(t) => printer.print_raw_token(t),
            BinaryOp::LessLess(t) => printer.print_raw_token(t),
            BinaryOp::GreaterGreater(t) => printer.print_raw_token(t),
            BinaryOp::Plus(t) => printer.print_raw_token(t),
            BinaryOp::Minus(t) => printer.print_raw_token(t),
            BinaryOp::Star(t) => printer.print_raw_token(t),
            BinaryOp::Slash(t) => printer.print_raw_token(t),
            BinaryOp::Percent(t) => printer.print_raw_token(t),
            BinaryOp::Equals(t) => printer.print_raw_token(t),
            BinaryOp::PlusEquals(t) => printer.print_raw_token(t),
            BinaryOp::MinusEquals(t) => printer.print_raw_token(t),
            BinaryOp::StarEquals(t) => printer.print_raw_token(t),
            BinaryOp::SlashEquals(t) => printer.print_raw_token(t),
            BinaryOp::PercentEquals(t) => printer.print_raw_token(t),
            BinaryOp::AndEquals(t) => printer.print_raw_token(t),
            BinaryOp::PipeEquals(t) => printer.print_raw_token(t),
            BinaryOp::CaretEquals(t) => printer.print_raw_token(t),
            BinaryOp::LessLessEquals(t) => printer.print_raw_token(t),
            BinaryOp::GreaterGreaterEquals(t) => printer.print_raw_token(t),
        }
        PrintInfo::default_single_line()
    }
    fn leftmost_token(&self) -> TextRange {
        self.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.span()
    }
}

#[derive(Debug)]
pub enum UnaryOp {
    Not(Not),
    Minus(Minus),
}

impl FromCST for UnaryOp {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        match elem.kind() {
            SyntaxKind::NOT => Not::from_cst(elem).map(UnaryOp::Not),
            SyntaxKind::MINUS => Minus::from_cst(elem).map(UnaryOp::Minus),
            _ => Err(StrongAstError::UnexpectedKindDesc {
                expected_desc: "unary operator".into(),
                found: elem.kind(),
                at: elem.text_range(),
            }),
        }
    }
}

impl Token for UnaryOp {
    fn span(&self) -> TextRange {
        match self {
            UnaryOp::Not(t) => t.span(),
            UnaryOp::Minus(t) => t.span(),
        }
    }
}

impl Printable for UnaryOp {
    fn print(&self, _shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            UnaryOp::Not(t) => printer.print_raw_token(t),
            UnaryOp::Minus(t) => printer.print_raw_token(t),
        }
        PrintInfo::default_single_line()
    }
    fn leftmost_token(&self) -> TextRange {
        self.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.span()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IntegerLiteral {
    pub token_span: TextRange,
}
impl IntegerLiteral {
    /// Does not verify that the span is actually a integer literal token.
    #[must_use]
    pub fn new_from_span(token_span: TextRange) -> Self {
        Self { token_span }
    }
}
impl FromCST for IntegerLiteral {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let token = StrongAstError::assert_is_token(elem)?;
        StrongAstError::assert_kind_token(&token, SyntaxKind::INTEGER_LITERAL)?;
        Ok(Self::new_from_span(token.text_range()))
    }
}
impl Token for IntegerLiteral {
    fn span(&self) -> TextRange {
        self.token_span
    }
}
impl KnownKind for IntegerLiteral {
    fn kind() -> SyntaxKind {
        SyntaxKind::INTEGER_LITERAL
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FloatLiteral {
    pub token_span: TextRange,
}
impl FloatLiteral {
    /// Does not verify that the span is actually a float literal token.
    #[must_use]
    pub fn new_from_span(token_span: TextRange) -> Self {
        Self { token_span }
    }
}
impl FromCST for FloatLiteral {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let token = StrongAstError::assert_is_token(elem)?;
        StrongAstError::assert_kind_token(&token, SyntaxKind::FLOAT_LITERAL)?;
        Ok(Self::new_from_span(token.text_range()))
    }
}
impl Token for FloatLiteral {
    fn span(&self) -> TextRange {
        self.token_span
    }
}
impl KnownKind for FloatLiteral {
    fn kind() -> SyntaxKind {
        SyntaxKind::FLOAT_LITERAL
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Word {
    pub token_span: TextRange,
}
impl Word {
    /// Does not verify that the span is actually a word token.
    #[must_use]
    pub fn new_from_span(token_span: TextRange) -> Self {
        Self { token_span }
    }
}
impl FromCST for Word {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let token = StrongAstError::assert_is_token(elem)?;
        StrongAstError::assert_kind_token(&token, SyntaxKind::WORD)?;
        Ok(Self::new_from_span(token.text_range()))
    }
}
impl Token for Word {
    fn span(&self) -> TextRange {
        self.token_span
    }
}
impl KnownKind for Word {
    fn kind() -> SyntaxKind {
        SyntaxKind::WORD
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct QuotedString {
    pub token_span: TextRange,
}
impl QuotedString {
    /// Does not verify that the span is actually a quoted string token.
    #[must_use]
    pub fn new_from_span(token_span: TextRange) -> Self {
        Self { token_span }
    }
}
impl FromCST for QuotedString {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::STRING_LITERAL)?;

        // Find the opening quote
        let start = node
            .first_child_or_token_by_kind(&|kind| kind == SyntaxKind::QUOTE)
            .ok_or_else(|| StrongAstError::missing(SyntaxKind::QUOTE, node.text_range()))?;

        Ok(Self::new_from_span(TextRange::new(
            start.text_range().start(),
            node.text_range().end(),
        )))
    }
}
impl Token for QuotedString {
    fn span(&self) -> TextRange {
        self.token_span
    }
}
impl KnownKind for QuotedString {
    fn kind() -> SyntaxKind {
        SyntaxKind::STRING_LITERAL
    }
}
impl Printable for QuotedString {
    fn print(&self, _shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(self);
        PrintInfo {
            multi_lined: printer.input[self.span()].contains('\n'),
        }
    }
    fn leftmost_token(&self) -> TextRange {
        TextRange::new(
            self.token_span.start(),
            self.token_span.start() + TextSize::from(1),
        )
    }
    fn rightmost_token(&self) -> TextRange {
        TextRange::new(
            self.token_span.end() - TextSize::from(1),
            self.token_span.end(),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RawString {
    pub token_span: TextRange,
}
impl FromCST for RawString {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::RAW_STRING_LITERAL)?;

        // Find the opening hash token to strip preceding trivia
        let start = node
            .first_child_token_of_kind(SyntaxKind::HASH)
            .ok_or_else(|| StrongAstError::missing(SyntaxKind::HASH, node.text_range()))?;

        Ok(RawString {
            token_span: TextRange::new(start.text_range().start(), node.text_range().end()),
        })
    }
}
impl Token for RawString {
    fn span(&self) -> TextRange {
        self.token_span
    }
}
impl KnownKind for RawString {
    fn kind() -> SyntaxKind {
        SyntaxKind::RAW_STRING_LITERAL
    }
}
impl Printable for RawString {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let text = &printer.input[self.span()];
        let multi_lined = text.contains('\n');
        if !multi_lined {
            // print as-is
            printer.print_raw_token(self);
            return PrintInfo { multi_lined };
        }

        // we need to re-organize the interior
        let (Some(start_quote), Some(end_quote)) = (text.find('"'), text.rfind('"')) else {
            // should never happen, but print as-is if it does
            printer.print_raw_token(self);
            return PrintInfo { multi_lined };
        };
        if end_quote <= start_quote {
            // should never happen, but print as-is if it does
            printer.print_raw_token(self);
            return PrintInfo { multi_lined };
        }

        let interior = &text[start_quote + 1..end_quote].trim();
        let mut lines = interior.lines();
        let first_line = lines
            .next()
            .unwrap_or_else(|| unreachable!("split always has at least one element"));
        let min_indent = lines
            .clone()
            .map(|line| {
                let count = line.bytes().take_while(|c| *c == b' ').count();
                if count == line.len() {
                    // it is all spaces
                    usize::MAX
                } else {
                    count
                }
            })
            .min()
            .unwrap_or(0);

        let inner_base_indent = shape.indent + printer.config.indent_width;
        printer.print_str(&text[..=start_quote]);
        printer.print_newline();
        printer.print_spaces(inner_base_indent);
        printer.print_str(first_line.trim_start_matches(' '));
        for line in lines {
            if line.len() <= min_indent {
                // This line must be all spaces since otherwise it would have affected `min_indent`.
                // So we can print an empty line.
                printer.print_newline();
                continue;
            }

            let (removed_indent, line) = line.split_at(min_indent);
            debug_assert!(
                removed_indent.bytes().all(|c| c == b' '),
                "should not have removed non-indent"
            );
            debug_assert!(!line.is_empty(), "should have been handled above");

            printer.print_newline();
            printer.print_spaces(inner_base_indent);
            printer.print_str(line);
        }
        printer.print_newline();
        printer.print_spaces(shape.indent);
        printer.print_str(&text[end_quote..]);

        PrintInfo { multi_lined }
    }
    fn leftmost_token(&self) -> TextRange {
        TextRange::new(
            self.token_span.start(),
            self.token_span.start() + TextSize::from(1),
        )
    }
    fn rightmost_token(&self) -> TextRange {
        TextRange::new(
            self.token_span.end() - TextSize::from(1),
            self.token_span.end(),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HeaderComment {
    pub token_span: TextRange,
}
impl HeaderComment {
    /// Does not verify that the span is actually a header comment token.
    #[must_use]
    pub fn new_from_span(token_span: TextRange) -> Self {
        Self { token_span }
    }
}
impl FromCST for HeaderComment {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::HEADER_COMMENT)?;

        // find the first non-trivia token
        let first = node
            .first_child_token_of_kind(SyntaxKind::SLASH)
            .ok_or_else(|| StrongAstError::missing(SyntaxKind::SLASH, node.text_range()))?;

        Ok(Self::new_from_span(TextRange::new(
            first.text_range().start(),
            node.text_range().end(),
        )))
    }
}
impl Token for HeaderComment {
    fn span(&self) -> TextRange {
        self.token_span
    }
}
impl KnownKind for HeaderComment {
    fn kind() -> SyntaxKind {
        SyntaxKind::HEADER_COMMENT
    }
}
