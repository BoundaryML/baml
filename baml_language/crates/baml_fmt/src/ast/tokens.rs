use crate::printer::*;
use rowan::TextRange;

pub trait Token {
    fn span(&self) -> TextRange;
}

pub trait KeywordToken: Token {}
macro_rules! define_keyword_tokens {
    ($($keyword:literal => $name:ident;)*) => {
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
    "class" => Class;
    "enum" => Enum;
    "function" => Function;
    "client" => Client;
    "generator" => Generator;
    "test" => Test;
    "retry_policy" => RetryPolicy;
    "template_string" => TemplateString;
    "type_builder" => TypeBuilder;
    "if" => If;
    "else" => Else;
    "for" => For;
    "while" => While;
    "let" => Let;
    "in" => In;
    "break" => Break;
    "continue" => Continue;
    "return" => Return;
    "match" => Match;
    "assert" => Assert;
    "watch" => Watch;
    "instanceof" => Instanceof;
    "env" => Env;
    "dynamic" => Dynamic;
}

pub trait PunctuationToken: Token {}
macro_rules! define_punctuation_tokens {
    ($($punct:literal => $name:ident;)*) => {
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
    "{" => LBrace;
    "}" => RBrace;
    "(" => LParen;
    ")" => RParen;
    "[" => LBracket;
    "]" => RBracket;
    ":" => Colon;
    "::" => DoubleColon;
    "," => Comma;
    ";" => Semicolon;
    "..." => DotDotDot;
    "." => Dot;
    "$" => Dollar;
    "->" => Arrow;
    "=" => Equals;
    "+=" => PlusEquals;
    "-=" => MinusEquals;
    "*=" => StarEquals;
    "/=" => SlashEquals;
    "%=" => PercentEquals;
    "&=" => AndEquals;
    "|=" => PipeEquals;
    "^=" => CaretEquals;
    "<<=" => LessLessEquals;
    ">>=" => GreaterGreaterEquals;
    "=>" => FatArrow;
    "@@" => AtAt;
    "@" => At;
    "|" => Pipe;
    "?" => Question;
    "<=" => LessEquals;
    ">=" => GreaterEquals;
    "<<" => LessLess;
    ">>" => GreaterGreater;
    "<" => Less;
    ">" => Greater;
    "&&" => AndAnd;
    "||" => OrOr;
    "!" => Not;
    "&" => And;
    "^" => Caret;
    "~" => Tilde;
    "++" => PlusPlus;
    "--" => MinusMinus;
    "+" => Plus;
    "-" => Minus;
    "*" => Star;
    "/" => Slash;
    "%" => Percent;
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
    // EqualsEquals(EqualsEquals),
    // NotEquals(NotEquals),
    Less(Less),
    Greater(Greater),
    LessEquals(LessEquals),
    GreaterEquals(GreaterEquals),
    AndAnd(AndAnd),
    OrOr(OrOr),
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
}

impl BinaryOp {
    pub fn from_cst_token(
        token: baml_compiler_syntax::SyntaxToken,
    ) -> Result<Self, super::StrongAstError> {
        use baml_compiler_syntax::SyntaxKind;
        let span = token.text_range();

        match token.kind() {
            SyntaxKind::LESS => Ok(BinaryOp::Less(Less::new_from_span(span))),
            SyntaxKind::GREATER => Ok(BinaryOp::Greater(Greater::new_from_span(span))),
            SyntaxKind::LESS_EQUALS => Ok(BinaryOp::LessEquals(LessEquals::new_from_span(span))),
            SyntaxKind::GREATER_EQUALS => {
                Ok(BinaryOp::GreaterEquals(GreaterEquals::new_from_span(span)))
            }
            SyntaxKind::AND_AND => Ok(BinaryOp::AndAnd(AndAnd::new_from_span(span))),
            SyntaxKind::OR_OR => Ok(BinaryOp::OrOr(OrOr::new_from_span(span))),
            SyntaxKind::PIPE => Ok(BinaryOp::Pipe(Pipe::new_from_span(span))),
            SyntaxKind::CARET => Ok(BinaryOp::Caret(Caret::new_from_span(span))),
            SyntaxKind::KW_INSTANCEOF => Ok(BinaryOp::Instanceof(Instanceof::new_from_span(span))),
            SyntaxKind::LESS_LESS => Ok(BinaryOp::LessLess(LessLess::new_from_span(span))),
            SyntaxKind::GREATER_GREATER => Ok(BinaryOp::GreaterGreater(
                GreaterGreater::new_from_span(span),
            )),
            SyntaxKind::PLUS => Ok(BinaryOp::Plus(Plus::new_from_span(span))),
            SyntaxKind::MINUS => Ok(BinaryOp::Minus(Minus::new_from_span(span))),
            SyntaxKind::STAR => Ok(BinaryOp::Star(Star::new_from_span(span))),
            SyntaxKind::SLASH => Ok(BinaryOp::Slash(Slash::new_from_span(span))),
            SyntaxKind::PERCENT => Ok(BinaryOp::Percent(Percent::new_from_span(span))),
            _ => Err(super::StrongAstError::UnexpectedKindDesc {
                expected_desc: "binary operator".into(),
                found: token.kind(),
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

impl UnaryOp {
    pub fn from_cst_token(
        token: baml_compiler_syntax::SyntaxToken,
    ) -> Result<Self, super::StrongAstError> {
        use baml_compiler_syntax::SyntaxKind;
        let span = token.text_range();

        match token.kind() {
            SyntaxKind::NOT => Ok(UnaryOp::Not(Not::new_from_span(span))),
            SyntaxKind::MINUS => Ok(UnaryOp::Minus(Minus::new_from_span(span))),
            _ => Err(super::StrongAstError::UnexpectedKindDesc {
                expected_desc: "unary operator".into(),
                found: token.kind(),
                at: span,
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IntegerLiteral {
    pub token_span: TextRange,
}
impl IntegerLiteral {
    /// Does not verify that the span is actually a integer literal token.
    pub fn new_from_span(token_span: TextRange) -> Self {
        Self { token_span }
    }
}
impl Token for IntegerLiteral {
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
    pub fn new_from_span(token_span: TextRange) -> Self {
        Self { token_span }
    }
}
impl Token for FloatLiteral {
    fn span(&self) -> TextRange {
        self.token_span
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Word {
    pub token_span: TextRange,
}
impl Word {
    /// Does not verify that the span is actually a word token.
    pub fn new_from_span(token_span: TextRange) -> Self {
        Self { token_span }
    }
}
impl Token for Word {
    fn span(&self) -> TextRange {
        self.token_span
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct QuotedString {
    pub token_span: TextRange,
}
impl QuotedString {
    /// Does not verify that the span is actually a quoted string token.
    pub fn new_from_span(token_span: TextRange) -> Self {
        Self { token_span }
    }
}
impl Token for QuotedString {
    fn span(&self) -> TextRange {
        self.token_span
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RawString {
    pub token_span: TextRange,
}
impl RawString {
    /// Does not verify that the span is actually a raw string token.
    pub fn new_from_span(token_span: TextRange) -> Self {
        Self { token_span }
    }
}
impl Token for RawString {
    fn span(&self) -> TextRange {
        self.token_span
    }
}
impl Printable for RawString {
    fn print(&self, _shape: Shape, printer: &mut Printer) -> PrintInfo {
        let text = &printer.input[self.span()];
        let multi_lined = text.contains('\n');
        printer.print_raw_token(self);
        PrintInfo { multi_lined }
    }
}

impl Printable for BinaryOp {
    fn print(&self, _shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            BinaryOp::Less(t) => printer.print_raw_token(t),
            BinaryOp::Greater(t) => printer.print_raw_token(t),
            BinaryOp::LessEquals(t) => printer.print_raw_token(t),
            BinaryOp::GreaterEquals(t) => printer.print_raw_token(t),
            BinaryOp::AndAnd(t) => printer.print_raw_token(t),
            BinaryOp::OrOr(t) => printer.print_raw_token(t),
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
        }
        PrintInfo::default_single_line()
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
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HeaderComment {
    pub token_span: TextRange,
}
impl HeaderComment {
    /// Does not verify that the span is actually a header comment token.
    pub fn new_from_span(token_span: TextRange) -> Self {
        Self { token_span }
    }
}
impl Token for HeaderComment {
    fn span(&self) -> TextRange {
        self.token_span
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MaybeQuotedString {
    Quoted(QuotedString),
    Unquoted(Word),
}
impl Token for MaybeQuotedString {
    fn span(&self) -> TextRange {
        match self {
            MaybeQuotedString::Quoted(qs) => qs.span(),
            MaybeQuotedString::Unquoted(w) => w.span(),
        }
    }
}
