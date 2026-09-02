use baml_db::baml_compiler_syntax::{SyntaxElement, SyntaxKind, SyntaxNodeExt};
use rowan::{TextRange, TextSize};

use crate::{
    ast::{FromCST, KnownKind, StrongAstError},
    printer::{PrintInfo, Printable, Printer, Shape},
};

pub trait Token {
    fn span(&self) -> TextRange;
}

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
    "interface" => SyntaxKind::KW_INTERFACE => Interface;
    "implements" => SyntaxKind::KW_IMPLEMENTS => Implements;
    "implement" => SyntaxKind::KW_IMPLEMENT => Implement;
    "extends" => SyntaxKind::KW_EXTENDS => Extends;
    "requires" => SyntaxKind::KW_REQUIRES => Requires;
    "function" => SyntaxKind::KW_FUNCTION => Function;
    "client" => SyntaxKind::KW_CLIENT => Client;
    "generator" => SyntaxKind::KW_GENERATOR => Generator;
    "test" => SyntaxKind::KW_TEST => Test;
    "testset" => SyntaxKind::KW_TESTSET => TestSet;
    "retry_policy" => SyntaxKind::KW_RETRY_POLICY => RetryPolicy;
    "template_string" => SyntaxKind::KW_TEMPLATE_STRING => TemplateString;
    "if" => SyntaxKind::KW_IF => If;
    "else" => SyntaxKind::KW_ELSE => Else;
    "for" => SyntaxKind::KW_FOR => For;
    "while" => SyntaxKind::KW_WHILE => While;
    "let" => SyntaxKind::KW_LET => Let;
    "const" => SyntaxKind::KW_CONST => Const;
    "in" => SyntaxKind::KW_IN => In;
    "break" => SyntaxKind::KW_BREAK => Break;
    "continue" => SyntaxKind::KW_CONTINUE => Continue;
    "return" => SyntaxKind::KW_RETURN => Return;
    "throw" => SyntaxKind::KW_THROW => Throw;
    "match" => SyntaxKind::KW_MATCH => Match;
    "catch" => SyntaxKind::KW_CATCH => Catch;
    "catch_all" => SyntaxKind::KW_CATCH_ALL => CatchAll;
    "catch_all_panics" => SyntaxKind::KW_CATCH_ALL_PANICS => CatchAllPanics;
    "instanceof" => SyntaxKind::KW_INSTANCEOF => Instanceof;
    "is" => SyntaxKind::KW_IS => Is;
    "spawn" => SyntaxKind::KW_SPAWN => Spawn;
    "with" => SyntaxKind::KW_WITH => With;
    "throws" => SyntaxKind::KW_THROWS => Throws;
    "type" => SyntaxKind::KW_TYPE => TypeKw;
    "as" => SyntaxKind::KW_AS => As;
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum BindingKeyword {
    Let(Let),
    Const(Const),
}

impl Token for BindingKeyword {
    fn span(&self) -> TextRange {
        match self {
            Self::Let(token) => token.span(),
            Self::Const(token) => token.span(),
        }
    }
}

impl FromCST for BindingKeyword {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        match elem.kind() {
            SyntaxKind::KW_LET => Ok(Self::Let(Let::from_cst(elem)?)),
            SyntaxKind::KW_CONST => Ok(Self::Const(Const::from_cst(elem)?)),
            found => Err(StrongAstError::UnexpectedKindDesc {
                expected_desc: "KW_LET or KW_CONST".into(),
                found,
                at: elem.text_range(),
            }),
        }
    }
}

impl std::fmt::Display for BindingKeyword {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Let(token) => token.fmt(f),
            Self::Const(token) => token.fmt(f),
        }
    }
}

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
    ".." => SyntaxKind::DOT_DOT => DotDot;
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
    "?." => SyntaxKind::QUESTION_DOT => QuestionDot;
    "??" => SyntaxKind::QUESTION_QUESTION => QuestionQuestion;
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
            BinaryOp::QuestionQuestion(t) => t.span(),
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
            SyntaxKind::QUESTION_QUESTION => Ok(BinaryOp::QuestionQuestion(
                QuestionQuestion::new_from_span(token.text_range()),
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
            BinaryOp::QuestionQuestion(t) => printer.print_raw_token(t),
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

/// A boolean / null literal — `true` (`KW_TRUE`), `false` (`KW_FALSE`), or
/// `null` (`KW_NULL`). One token type spanning the three re-lexed kinds.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeywordLiteral {
    pub token_span: TextRange,
}
impl KeywordLiteral {
    /// Does not verify that the span is actually a boolean/null literal token.
    #[must_use]
    pub fn new_from_span(token_span: TextRange) -> Self {
        Self { token_span }
    }
}
impl FromCST for KeywordLiteral {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let token = StrongAstError::assert_is_token(elem)?;
        match token.kind() {
            SyntaxKind::KW_TRUE | SyntaxKind::KW_FALSE | SyntaxKind::KW_NULL => {
                Ok(Self::new_from_span(token.text_range()))
            }
            found => Err(StrongAstError::UnexpectedKindDesc {
                expected_desc: "KW_TRUE, KW_FALSE, or KW_NULL".into(),
                found,
                at: token.text_range(),
            }),
        }
    }
}
impl Token for KeywordLiteral {
    fn span(&self) -> TextRange {
        self.token_span
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

/// True for token kinds the parser accepts as identifiers in name positions.
///
/// The lexer emits dedicated keyword kinds for these words, but the parser
/// keeps them valid as field, parameter, method, and member-access names
/// (e.g. a class field or parameter named `client`, or `x.implements(y)` on
/// the reflection `type` value). The CST therefore contains the keyword kind
/// where the strong AST expects a name, and [`Word::from_cst`] must accept it.
/// Mirrors `at_member_name` and `parse_parameter` in `baml_compiler_parser`.
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
            // Member and function-name position only (`re.match(...)`,
            // `function match(...)`); `match` is never a path segment or a
            // parameter name.
            | SyntaxKind::KW_MATCH
    )
}

impl FromCST for Word {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let token = StrongAstError::assert_is_token(elem)?;
        if !is_word_like(token.kind()) {
            return Err(StrongAstError::UnexpectedKind {
                expected: SyntaxKind::WORD,
                found: token.kind(),
                at: token.text_range(),
            });
        }
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
        let Some(first_line) = lines.next() else {
            // Interior is empty after trim (e.g. `#"\n"#`) — print as-is.
            printer.print_raw_token(self);
            return PrintInfo { multi_lined };
        };
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

/// BEP-049 backtick-interpolated string literal.
///
/// A backtick string is auto-dedented at lower time (BEP-049 §12,
/// `baml_base::dedent::dedent_backtick`) with its `${...}` interpolations
/// replaced by placeholders and §13 whitespace control applied, so its runtime
/// *value* depends on the interior's indentation. The formatter re-indents a
/// multi-line interior to sit one level past the surrounding block (like a raw
/// string) but ONLY when that is provably value-preserving: it strips the same
/// common-prefix the runtime would, re-emits at the block indent, and then
/// re-derives the value of both forms and bails to verbatim if they differ. A
/// literal with a `${for}`/`${if}` block tag or a multi-line interpolation is
/// always printed verbatim (see the `dedent_safe` field), since re-indenting
/// those could change the §13 / placeholder-dedent value.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BacktickString {
    pub token_span: TextRange,
    /// `true` when the literal has no `${for}`/`${if}` block tag and no
    /// multi-line `${...}` interpolation, so re-indenting its text lines cannot
    /// change the runtime value.
    dedent_safe: bool,
}
impl FromCST for BacktickString {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::BACKTICK_STRING_LITERAL)?;

        let start = node
            .first_child_token_of_kind(SyntaxKind::BACKTICK)
            .ok_or_else(|| StrongAstError::missing(SyntaxKind::BACKTICK, node.text_range()))?;

        // Block tags (§13 whitespace control) and multi-line interpolations
        // (placeholdered before the §12 min-indent, so their inner lines are NOT
        // re-indented) make a plain re-indent value-unsafe. Detect them up front.
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

        Ok(BacktickString {
            token_span: TextRange::new(start.text_range().start(), node.text_range().end()),
            dedent_safe,
        })
    }
}
impl Token for BacktickString {
    fn span(&self) -> TextRange {
        self.token_span
    }
}
impl KnownKind for BacktickString {
    fn kind() -> SyntaxKind {
        SyntaxKind::BACKTICK_STRING_LITERAL
    }
}

/// Re-indent a multi-line backtick literal `text` so its interior sits one level
/// past `indent`, or return `None` to print it verbatim (single line, malformed,
/// or the re-indent would change the runtime value).
fn reindent_backtick(text: &str, indent: usize, indent_width: usize) -> Option<String> {
    if !text.contains('\n') {
        return None;
    }
    // The delimiter is a run of N backticks on each side (tick ladder).
    let ticks = text.bytes().take_while(|&c| c == b'`').count();
    if ticks == 0 || text.len() < ticks * 2 {
        return None;
    }
    let inner = &text[ticks..text.len() - ticks];

    // Source-level dedent: strip the common leading-whitespace prefix and the
    // delimiters' own line breaks, exactly as the compiler's §12 dedent does,
    // and on the raw source for the same reason it does — so escapes and
    // `${...}` stay intact and the printed form remains valid source.
    let dedented = baml_db::dedent::dedent_backtick(inner);
    if dedented.is_empty() {
        return None;
    }

    let base = indent + indent_width;
    let mut candidate_inner = String::from("\n");
    for (i, line) in dedented.lines().enumerate() {
        if i > 0 {
            candidate_inner.push('\n');
        }
        if !line.is_empty() {
            candidate_inner.extend(std::iter::repeat_n(' ', base));
            candidate_inner.push_str(line);
        }
    }
    candidate_inner.push('\n');
    candidate_inner.extend(std::iter::repeat_n(' ', indent));

    // Bail to verbatim unless the runtime value (§12 dedented, then escapes
    // decoded — the compiler's order) is byte-identical for the original and
    // re-indented interiors.
    let value = |s: &str| {
        baml_db::escape::unescape_backtick_string_literal(&baml_db::dedent::dedent_backtick(s))
    };
    if value(inner) != value(&candidate_inner) {
        return None;
    }

    Some(format!(
        "{}{}{}",
        &text[..ticks],
        candidate_inner,
        &text[text.len() - ticks..]
    ))
}

impl Printable for BacktickString {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let text = &printer.input[self.span()];
        let multi_lined = text.contains('\n');
        let reindented = if self.dedent_safe {
            reindent_backtick(text, shape.indent, printer.config.indent_width)
        } else {
            None
        };
        match reindented {
            Some(reindented) => printer.print_str(&reindented),
            None => printer.print_raw_token(self),
        }
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

#[derive(Debug)]
pub struct ByteString {
    pub token_span: TextRange,
}
impl FromCST for ByteString {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::BYTE_STRING_LITERAL)?;

        // Find the `b` prefix word token to strip preceding trivia.
        let start = node
            .first_child_or_token_by_kind(&|kind| kind == SyntaxKind::WORD)
            .ok_or_else(|| StrongAstError::missing(SyntaxKind::WORD, node.text_range()))?;

        Ok(ByteString {
            token_span: TextRange::new(start.text_range().start(), node.text_range().end()),
        })
    }
}
impl Token for ByteString {
    fn span(&self) -> TextRange {
        self.token_span
    }
}
impl KnownKind for ByteString {
    fn kind() -> SyntaxKind {
        SyntaxKind::BYTE_STRING_LITERAL
    }
}
impl Printable for ByteString {
    fn print(&self, _shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(self);
        PrintInfo { multi_lined: false }
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
