//! Reference: [`baml_db::baml_compiler_syntax::ast::Expr`] and [`baml_db::baml_compiler_hir::body`]

use baml_db::baml_compiler_syntax::{SyntaxElement, SyntaxKind};
use rowan::TextRange;

use crate::{
    ast::{
        BinaryOp, FromCST, KnownKind, MatchPattern, Statement, StrongAstError, SyntaxNodeIter,
        Token, Type, UnaryOp, tokens as t,
    },
    printer::{PrintInfo, PrintMultiLine, Printable, Printer, Shape},
    trivia_classifier::{EmittableTrivia, TriviaInfo, TriviaSliceExt},
};

#[derive(Debug)]
pub enum Expression {
    Literal(Literal),
    /// Includes things like `null`, `true`, `false`, `baml.fs`, etc.
    Path(PathExpr),
    /// A generic instantiation whose base is NOT a plain path — e.g.
    /// `(<T>(x: T) -> T { x })<int>` or `(foo)<int>`. The path-based form
    /// (`foo<int>`, `a.b.foo<int>`) is carried by [`PathExpr::generic_args`];
    /// the parser wraps both in a `PATH_EXPR` node, so this is selected when
    /// that node's first child is not a word/path.
    GenericApply(GenericApplyExpr),
    Paren(ParenExpr),
    Binary(BinaryExpr),
    Is(IsExpr),
    Unary(UnaryExpr),
    If(IfExpr),
    IfLet(IfLetExpr),
    Match(MatchExpr),
    Catch(CatchExpr),
    Call(CallExpr),
    Index(IndexExpr),
    FieldAccess(FieldAccessExpr),
    OptionalFieldAccess(OptionalFieldAccessExpr),
    OptionalIndex(OptionalIndexExpr),
    OptionalCall(OptionalCallExpr),
    EnvAccess(EnvAccessExpr),
    Block(BlockExpr),
    ArrayInitializer(ArrayInitializer),
    MapInitializer(MapLiteral),
    ObjectInitializer(ObjectInitializer),
    RawString(t::RawString),
    BacktickString(t::BacktickString),
    ByteString(t::ByteString),
    Lambda(Box<LambdaExpr>),
    /// A `spawn name? (with opts)? { … }` task-spawn expression (BEP-034).
    Spawn(Box<SpawnExpr>),
    /// A braceless `return …` in expression position (a `RETURN_EXPR`, e.g. a
    /// `catch`/`match` arm value like `_ => return 0`). Printed verbatim, like
    /// [`Expression::Unknown`] and backed by the same [`VerbatimSpan`], but kept
    /// as a distinct variant so the arm printers can recognize it: when they wrap
    /// a braceless arm body into a block they append the `;` that a block-position
    /// `return` requires, so the output round-trips through `RETURN_STMT` (i.e. is
    /// idempotent).
    Return(VerbatimSpan),
    /// A braceless `break` in expression position (a `BREAK_EXPR`, e.g. a
    /// `catch`/`match` arm value like `0 => break`). Handled exactly like
    /// [`Expression::Return`] and backed by the same [`VerbatimSpan`]: when an arm
    /// printer wraps it into a block it appends the `;` that a block-position
    /// `break` requires, so the output round-trips through `BREAK_STMT`.
    Break(VerbatimSpan),
    /// A braceless `continue` in expression position (a `CONTINUE_EXPR`). The
    /// `continue` counterpart of [`Expression::Break`].
    Continue(VerbatimSpan),
    Unknown(VerbatimSpan),
}

/// A node the strong AST does not model and prints verbatim: an unmodeled
/// expression (e.g. `defer { … }`, `throw e`, `await f`,
/// `x.as<T>`) held as [`Expression::Unknown`], or a braceless jump held as
/// [`Expression::Return`], [`Expression::Break`], or [`Expression::Continue`].
///
/// Rather than a single whole-node span, this carries the node's true first and
/// last *token* ranges. The trivia classifier keys leading/trailing comments to
/// individual token ranges, so [`Printable::leftmost_token`] /
/// [`Printable::rightmost_token`] must return those exact token ranges for a
/// comment to attach and emit. A whole-node span never matches a token key, so
/// a trailing comment on the node was silently dropped — the `defer` statement
/// comment-loss bug (B-629), and the same class of bug for a braceless `return`
/// arm. A whole-node span can also begin inside leading trivia (the parser
/// attaches a preceding comment to the node), which would re-print that comment
/// verbatim at the wrong indent; the `content_range` used for printing excludes
/// it.
#[derive(Debug)]
pub struct VerbatimSpan {
    /// Range of the first non-trivia token — the leading-trivia anchor.
    first_token: TextRange,
    /// Range of the last non-trivia token — the trailing-trivia anchor.
    last_token: TextRange,
}

impl VerbatimSpan {
    /// Build from the verbatim-printed syntax element, capturing its first and
    /// last non-trivia token ranges. Any leading/trailing trivia that the CST
    /// attaches inside the node is skipped so the anchors line up with the
    /// classifier's per-token comment keys.
    fn from_element(elem: &SyntaxElement) -> Self {
        if let Some(node) = elem.as_node() {
            let mut tokens = node
                .descendants_with_tokens()
                .filter_map(rowan::NodeOrToken::into_token)
                .filter(|t| !t.kind().is_trivia());
            if let Some(first) = tokens.next() {
                let first_token = first.text_range();
                let last_token = tokens.last().map_or(first_token, |t| t.text_range());
                return VerbatimSpan {
                    first_token,
                    last_token,
                };
            }
        }
        // A bare token, or a node with only trivia: the whole span is the token.
        let whole = elem.text_range();
        VerbatimSpan {
            first_token: whole,
            last_token: whole,
        }
    }

    /// The verbatim source span to print: from the first token to the last,
    /// excluding any leading/trailing trivia the CST folded into the node.
    fn content_range(&self) -> TextRange {
        TextRange::new(self.first_token.start(), self.last_token.end())
    }
}

impl Expression {
    #[must_use]
    pub const fn statement_needs_semicolon(&self) -> bool {
        !matches!(
            self,
            Expression::If(_)
                | Expression::IfLet(_)
                | Expression::Match(_)
                | Expression::Lambda(_)
                | Expression::Spawn(_)
                | Expression::Unknown(_)
        )
    }
}

impl FromCST for Expression {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let expr = match elem.kind() {
            SyntaxKind::STRING_LITERAL => t::QuotedString::from_cst(elem)
                .map(Literal::String)
                .map(Expression::Literal)?,
            SyntaxKind::INTEGER_LITERAL => Expression::Literal(Literal::Integer(
                t::IntegerLiteral::new_from_span(elem.text_range()),
            )),
            SyntaxKind::FLOAT_LITERAL => Expression::Literal(Literal::Float(
                t::FloatLiteral::new_from_span(elem.text_range()),
            )),
            SyntaxKind::KW_TRUE | SyntaxKind::KW_FALSE | SyntaxKind::KW_NULL => {
                Literal::from_cst(elem).map(Expression::Literal)?
            }
            SyntaxKind::WORD | SyntaxKind::KW_CLIENT => {
                PathExpr::from_cst(elem).map(Expression::Path)?
            }
            SyntaxKind::PATH_EXPR => {
                // The parser wraps any postfix `<...>` in a PATH_EXPR. When the
                // base is a plain path (word / nested PATH_EXPR) it is a
                // `PathExpr`; otherwise (a parenthesized expr, lambda, etc.) it
                // is a generic instantiation on a non-path base.
                let node = StrongAstError::assert_is_node(elem.clone())?;
                let base_is_path = SyntaxNodeIter::new(&node).next().is_some_and(|c| {
                    is_path_segment_kind(c.kind()) || c.kind() == SyntaxKind::PATH_EXPR
                });
                if base_is_path {
                    PathExpr::from_cst(elem).map(Expression::Path)?
                } else {
                    GenericApplyExpr::from_cst(elem).map(Expression::GenericApply)?
                }
            }
            SyntaxKind::PAREN_EXPR => ParenExpr::from_cst(elem).map(Expression::Paren)?,
            SyntaxKind::BINARY_EXPR => BinaryExpr::from_cst(elem).map(Expression::Binary)?,
            SyntaxKind::IS_EXPR => IsExpr::from_cst(elem).map(Expression::Is)?,
            SyntaxKind::UNARY_EXPR => UnaryExpr::from_cst(elem).map(Expression::Unary)?,
            SyntaxKind::IF_EXPR => IfExpr::from_cst(elem).map(Expression::If)?,
            SyntaxKind::IF_LET_EXPR => IfLetExpr::from_cst(elem).map(Expression::IfLet)?,
            SyntaxKind::MATCH_EXPR => MatchExpr::from_cst(elem).map(Expression::Match)?,
            SyntaxKind::CATCH_EXPR => CatchExpr::from_cst(elem).map(Expression::Catch)?,
            SyntaxKind::CALL_EXPR => CallExpr::from_cst(elem).map(Expression::Call)?,
            SyntaxKind::INDEX_EXPR => IndexExpr::from_cst(elem).map(Expression::Index)?,
            SyntaxKind::FIELD_ACCESS_EXPR => {
                FieldAccessExpr::from_cst(elem).map(Expression::FieldAccess)?
            }
            SyntaxKind::OPTIONAL_FIELD_ACCESS_EXPR => {
                OptionalFieldAccessExpr::from_cst(elem).map(Expression::OptionalFieldAccess)?
            }
            SyntaxKind::OPTIONAL_INDEX_EXPR => {
                OptionalIndexExpr::from_cst(elem).map(Expression::OptionalIndex)?
            }
            SyntaxKind::OPTIONAL_CALL_EXPR => {
                OptionalCallExpr::from_cst(elem).map(Expression::OptionalCall)?
            }
            SyntaxKind::ENV_ACCESS_EXPR => {
                EnvAccessExpr::from_cst(elem).map(Expression::EnvAccess)?
            }
            SyntaxKind::BLOCK_EXPR => BlockExpr::from_cst(elem).map(Expression::Block)?,
            SyntaxKind::ARRAY_LITERAL => {
                ArrayInitializer::from_cst(elem).map(Expression::ArrayInitializer)?
            }
            SyntaxKind::MAP_LITERAL => {
                MapLiteral::from_cst(elem).map(Expression::MapInitializer)?
            }
            SyntaxKind::OBJECT_LITERAL => {
                ObjectInitializer::from_cst(elem).map(Expression::ObjectInitializer)?
            }
            SyntaxKind::RAW_STRING_LITERAL => {
                t::RawString::from_cst(elem).map(Expression::RawString)?
            }
            SyntaxKind::BACKTICK_STRING_LITERAL => {
                t::BacktickString::from_cst(elem).map(Expression::BacktickString)?
            }
            SyntaxKind::BYTE_STRING_LITERAL => {
                t::ByteString::from_cst(elem).map(Expression::ByteString)?
            }
            SyntaxKind::LAMBDA_EXPR => Expression::Lambda(Box::new(LambdaExpr::from_cst(elem)?)),
            SyntaxKind::SPAWN_EXPR => Expression::Spawn(Box::new(SpawnExpr::from_cst(elem)?)),
            SyntaxKind::RETURN_EXPR => Expression::Return(VerbatimSpan::from_element(&elem)),
            SyntaxKind::BREAK_EXPR => Expression::Break(VerbatimSpan::from_element(&elem)),
            SyntaxKind::CONTINUE_EXPR => Expression::Continue(VerbatimSpan::from_element(&elem)),
            _ => Expression::Unknown(VerbatimSpan::from_element(&elem)),
        };
        Ok(expr)
    }
}

impl Expression {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    pub(crate) fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
        match self {
            Expression::Literal(lit) => lit.single_line_width(input),
            Expression::Path(path) => path.single_line_width(input),
            Expression::GenericApply(ga) => ga.single_line_width(input),
            Expression::Paren(paren) => paren.single_line_width(input),
            Expression::Binary(binary) => binary.single_line_width(input),
            Expression::Is(is) => is.single_line_width(input),
            Expression::Unary(unary) => unary.single_line_width(input),
            Expression::If(_) => None,
            Expression::IfLet(_) => None,
            Expression::Match(_) => None,
            Expression::Catch(_) => None,
            Expression::Call(call) => call.single_line_width(input),
            Expression::Index(index) => index.single_line_width(input),
            Expression::FieldAccess(fa) => fa.single_line_width(input),
            Expression::OptionalFieldAccess(fa) => fa.single_line_width(input),
            Expression::OptionalIndex(index) => index.single_line_width(input),
            Expression::OptionalCall(call) => call.single_line_width(input),
            Expression::EnvAccess(env) => env.single_line_width(input),
            Expression::Block(_) => None,
            Expression::ArrayInitializer(array) => array.single_line_width(input),
            Expression::MapInitializer(map) => map.single_line_width(input),
            Expression::ObjectInitializer(obj) => obj.single_line_width(input),
            Expression::RawString(raw) => {
                if input.input[raw.span()].contains('\n') {
                    None
                } else {
                    Some(usize::from(raw.span().len()))
                }
            }
            Expression::BacktickString(bt) => {
                if input.input[bt.span()].contains('\n') {
                    None
                } else {
                    Some(usize::from(bt.span().len()))
                }
            }
            Expression::ByteString(bs) => Some(usize::from(bs.span().len())),
            Expression::Lambda(_) => None,
            Expression::Spawn(spawn) => spawn.single_line_width(input),
            Expression::Return(_) | Expression::Break(_) | Expression::Continue(_) => None,
            Expression::Unknown(unknown) => {
                // Unmodeled nodes (e.g. `await f`, `x.as<T>`,
                // `throw e`) print their source verbatim (see `print`). When that
                // text is a single line it occupies a known width and can sit
                // inline like any other fitting expression. Reporting `None` here
                // used to force every *enclosing* expression to wrap even when the
                // whole thing fit the width budget (B-231).
                let text = &input.input[unknown.content_range()];
                if text.contains('\n') {
                    None
                } else {
                    Some(text.trim_start().len())
                }
            }
        }
    }
}

impl Printable for Expression {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            Expression::Literal(lit) => lit.print(shape, printer),
            chain @ (Expression::Path(_)
            | Expression::Call(_)
            | Expression::Index(_)
            | Expression::FieldAccess(_)
            | Expression::OptionalFieldAccess(_)
            | Expression::OptionalIndex(_)
            | Expression::OptionalCall(_)) => {
                // These are all chains of postfix expressions
                let chain = PrintChain::new(chain, printer.trivia);
                chain.print(shape, printer)
            }
            Expression::GenericApply(ga) => ga.print(shape, printer),
            Expression::Paren(paren) => paren.print(shape, printer),
            Expression::Binary(binary) => binary.print(shape, printer),
            Expression::Is(is) => is.print(shape, printer),
            Expression::Unary(unary) => unary.print(shape, printer),
            Expression::If(if_expr) => if_expr.print(shape, printer),
            Expression::IfLet(if_let_expr) => if_let_expr.print(shape, printer),
            Expression::Match(match_expr) => match_expr.print(shape, printer),
            Expression::Catch(catch_expr) => catch_expr.print(shape, printer),
            Expression::EnvAccess(env) => env.print(shape, printer),
            Expression::Block(block) => block.print(shape, printer),
            Expression::ArrayInitializer(array) => array.print(shape, printer),
            Expression::MapInitializer(map) => map.print(shape, printer),
            Expression::ObjectInitializer(obj) => obj.print(shape, printer),
            Expression::RawString(raw) => raw.print(shape, printer),
            Expression::BacktickString(bt) => bt.print(shape, printer),
            Expression::ByteString(bs) => bs.print(shape, printer),
            Expression::Lambda(lambda) => lambda.print(shape, printer),
            Expression::Spawn(spawn) => spawn.print(shape, printer),
            // Print the raw `return …` / `break` / `continue` text. The arm
            // printers add the `;` when they wrap this into a block (see
            // `CatchArm`/`MatchArm`). A braceless jump only appears as a whole
            // arm value, never nested inside another expression, so it always
            // reports multi-lined.
            Expression::Return(jump) | Expression::Break(jump) | Expression::Continue(jump) => {
                printer.print_input_range_trimmed_start(jump.content_range());
                PrintInfo::default_multi_lined()
            }
            // Unmodeled nodes print their source verbatim. Report `multi_lined`
            // honestly from whether that text spans multiple lines: a single-line
            // unknown node (`await f`, `x.as<T>`, …) must not claim to be
            // multi-line, or it force-wraps its parents even when everything fits
            // on one line (B-231).
            Expression::Unknown(unknown) => {
                let range = unknown.content_range();
                printer.print_input_range_trimmed_start(range);
                PrintInfo {
                    multi_lined: printer.input[range].contains('\n'),
                }
            }
        }
    }
    fn leftmost_token(&self) -> TextRange {
        match self {
            Expression::Literal(lit) => lit.leftmost_token(),
            Expression::Path(path) => path.leftmost_token(),
            Expression::GenericApply(ga) => ga.leftmost_token(),
            Expression::Paren(paren) => paren.leftmost_token(),
            Expression::Binary(binary) => binary.leftmost_token(),
            Expression::Is(is) => is.leftmost_token(),
            Expression::Unary(unary) => unary.leftmost_token(),
            Expression::If(if_expr) => if_expr.leftmost_token(),
            Expression::IfLet(if_let_expr) => if_let_expr.leftmost_token(),
            Expression::Match(match_expr) => match_expr.leftmost_token(),
            Expression::Catch(catch_expr) => catch_expr.leftmost_token(),
            Expression::Call(call) => call.leftmost_token(),
            Expression::Index(index) => index.leftmost_token(),
            Expression::FieldAccess(fa) => fa.base.leftmost_token(),
            Expression::OptionalFieldAccess(fa) => fa.base.leftmost_token(),
            Expression::OptionalIndex(index) => index.base.leftmost_token(),
            Expression::OptionalCall(call) => call.callee.leftmost_token(),
            Expression::EnvAccess(env) => env.leftmost_token(),
            Expression::Block(block) => block.leftmost_token(),
            Expression::ArrayInitializer(array) => array.leftmost_token(),
            Expression::MapInitializer(map) => map.leftmost_token(),
            Expression::ObjectInitializer(obj) => obj.leftmost_token(),
            Expression::RawString(raw) => raw.leftmost_token(),
            Expression::BacktickString(bt) => bt.leftmost_token(),
            Expression::ByteString(bs) => bs.leftmost_token(),
            Expression::Lambda(lambda) => lambda.leftmost_token(),
            Expression::Spawn(spawn) => spawn.leftmost_token(),
            Expression::Return(span)
            | Expression::Break(span)
            | Expression::Continue(span)
            | Expression::Unknown(span) => span.first_token,
        }
    }
    fn rightmost_token(&self) -> TextRange {
        match self {
            Expression::Literal(lit) => lit.rightmost_token(),
            Expression::Path(path) => path.rightmost_token(),
            Expression::GenericApply(ga) => ga.rightmost_token(),
            Expression::Paren(paren) => paren.rightmost_token(),
            Expression::Binary(binary) => binary.rightmost_token(),
            Expression::Is(is) => is.rightmost_token(),
            Expression::Unary(unary) => unary.rightmost_token(),
            Expression::If(if_expr) => if_expr.rightmost_token(),
            Expression::IfLet(if_let_expr) => if_let_expr.rightmost_token(),
            Expression::Match(match_expr) => match_expr.rightmost_token(),
            Expression::Catch(catch_expr) => catch_expr.rightmost_token(),
            Expression::Call(call) => call.rightmost_token(),
            Expression::Index(index) => index.rightmost_token(),
            Expression::FieldAccess(fa) => fa.field.span(),
            Expression::OptionalFieldAccess(fa) => fa.field.span(),
            Expression::OptionalIndex(index) => index.close_bracket.span(),
            Expression::OptionalCall(call) => call.args.rightmost_token(),
            Expression::EnvAccess(env) => env.rightmost_token(),
            Expression::Block(block) => block.rightmost_token(),
            Expression::ArrayInitializer(array) => array.rightmost_token(),
            Expression::MapInitializer(map) => map.rightmost_token(),
            Expression::ObjectInitializer(obj) => obj.rightmost_token(),
            Expression::RawString(raw) => raw.rightmost_token(),
            Expression::BacktickString(bt) => bt.rightmost_token(),
            Expression::ByteString(bs) => bs.rightmost_token(),
            Expression::Lambda(lambda) => lambda.rightmost_token(),
            Expression::Spawn(spawn) => spawn.rightmost_token(),
            Expression::Return(span)
            | Expression::Break(span)
            | Expression::Continue(span)
            | Expression::Unknown(span) => span.last_token,
        }
    }
}

#[derive(Debug)]
pub enum Literal {
    String(t::QuotedString),
    Integer(t::IntegerLiteral),
    Float(t::FloatLiteral),
    /// `true` / `false` / `null`.
    Keyword(t::KeywordLiteral),
}

impl FromCST for Literal {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        match elem.kind() {
            SyntaxKind::STRING_LITERAL => Ok(Literal::String(t::QuotedString::from_cst(elem)?)),
            SyntaxKind::INTEGER_LITERAL => Ok(Literal::Integer(t::IntegerLiteral::from_cst(elem)?)),
            SyntaxKind::FLOAT_LITERAL => Ok(Literal::Float(t::FloatLiteral::from_cst(elem)?)),
            SyntaxKind::KW_TRUE | SyntaxKind::KW_FALSE | SyntaxKind::KW_NULL => {
                Ok(Literal::Keyword(t::KeywordLiteral::from_cst(elem)?))
            }
            _ => Err(StrongAstError::UnexpectedKindDesc {
                expected_desc: "a literal".into(),
                found: elem.kind(),
                at: elem.text_range(),
            }),
        }
    }
}

impl Literal {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    pub(crate) fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
        match self {
            Literal::String(s) => {
                if input.input[s.span()].contains('\n') {
                    None
                } else {
                    Some(usize::from(s.span().len()))
                }
            }
            Literal::Integer(i) => Some(usize::from(i.span().len())),
            Literal::Float(f) => Some(usize::from(f.span().len())),
            Literal::Keyword(k) => Some(usize::from(k.span().len())),
        }
    }
}

impl Printable for Literal {
    fn print(&self, _shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            Literal::String(s) => printer.print_raw_token(s),
            Literal::Integer(i) => printer.print_raw_token(i),
            Literal::Float(f) => printer.print_raw_token(f),
            Literal::Keyword(k) => printer.print_raw_token(k),
        }
        PrintInfo::default_single_line()
    }
    fn leftmost_token(&self) -> TextRange {
        match self {
            Literal::String(s) => s.leftmost_token(),
            Literal::Integer(i) => i.span(),
            Literal::Float(f) => f.span(),
            Literal::Keyword(k) => k.span(),
        }
    }
    fn rightmost_token(&self) -> TextRange {
        match self {
            Literal::String(s) => s.rightmost_token(),
            Literal::Integer(i) => i.span(),
            Literal::Float(f) => f.span(),
            Literal::Keyword(k) => k.span(),
        }
    }
}

/// Corresponds to either a [`SyntaxKind::PATH_EXPR`] node or single [`SyntaxKind::WORD`] token.
#[derive(Debug)]
pub struct PathExpr {
    pub first: t::Word,
    pub rest: Vec<(t::Dot, t::Word)>,
    /// Trailing generic arguments, e.g. the `<int, string>` in `f<int, string>`
    /// or `baml.fetch_as<Todo>`. Only present at the tail of the path.
    pub generic_args: Option<GenericArgs>,
}

/// Mirrors the `segment` allowlist in `parse_path_or_ident`; a kind the parser
/// admits into a `PATH_EXPR` but this rejects would fail to format.
fn is_path_segment_kind(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::WORD
            | SyntaxKind::KW_CLIENT
            | SyntaxKind::KW_SPAWN
            | SyntaxKind::KW_AWAIT
            | SyntaxKind::KW_CLASS
            | SyntaxKind::KW_ENUM
            | SyntaxKind::KW_INTERFACE
            | SyntaxKind::KW_FUNCTION
            | SyntaxKind::KW_MATCH
    )
}

fn path_segment_from_cst(elem: SyntaxElement) -> Result<t::Word, StrongAstError> {
    let token = StrongAstError::assert_is_token(elem)?;
    if is_path_segment_kind(token.kind()) {
        Ok(t::Word::new_from_span(token.text_range()))
    } else {
        Err(StrongAstError::UnexpectedKindDesc {
            expected_desc: "path segment".into(),
            found: token.kind(),
            at: token.text_range(),
        })
    }
}

impl FromCST for PathExpr {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        if is_path_segment_kind(elem.kind()) {
            let first = path_segment_from_cst(elem)?;
            return Ok(PathExpr {
                first,
                rest: Vec::new(),
                generic_args: None,
            });
        }
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::PATH_EXPR)?;

        let mut it = SyntaxNodeIter::new(&node);

        // First child: either a WORD, or a nested PATH_EXPR (the parser wraps
        // an existing path expr when it adds GENERIC_ARGS as a postfix).
        let next = it
            .next()
            .ok_or_else(|| StrongAstError::missing(SyntaxKind::WORD, it.parent))?;

        let (first, mut rest) = match next.kind() {
            kind if is_path_segment_kind(kind) => (path_segment_from_cst(next)?, Vec::new()),
            SyntaxKind::PATH_EXPR => {
                let nested = PathExpr::from_cst(next)?;
                if nested.generic_args.is_some() {
                    return Err(StrongAstError::UnexpectedAdditionalElement {
                        parent: it.parent,
                        at: nested
                            .generic_args
                            .as_ref()
                            .map_or_else(rowan::TextRange::default, |g| g.open_angle.span()),
                    });
                }
                (nested.first, nested.rest)
            }
            _ => {
                return Err(StrongAstError::UnexpectedAdditionalElement {
                    parent: it.parent,
                    at: next.text_range(),
                });
            }
        };

        let mut generic_args: Option<GenericArgs> = None;

        // Then: DOT WORD pairs, optionally followed by a single GENERIC_ARGS.
        while let Some(elem) = it.next() {
            match elem.kind() {
                SyntaxKind::DOT => {
                    let dot = t::Dot::from_cst(elem)?;
                    let word = path_segment_from_cst(it.expect_next("path segment after `.`")?)?;
                    rest.push((dot, word));
                }
                SyntaxKind::GENERIC_ARGS => {
                    generic_args = Some(GenericArgs::from_cst(elem)?);
                    if let Some(extra) = it.next() {
                        return Err(StrongAstError::UnexpectedAdditionalElement {
                            parent: it.parent,
                            at: extra.text_range(),
                        });
                    }
                    break;
                }
                _ => {
                    return Err(StrongAstError::UnexpectedAdditionalElement {
                        parent: it.parent,
                        at: elem.text_range(),
                    });
                }
            }
        }

        Ok(PathExpr {
            first,
            rest,
            generic_args,
        })
    }
}

impl PathExpr {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    #[allow(clippy::unnecessary_wraps)]
    pub(crate) fn single_line_width(&self, _input: &Printer<'_>) -> Option<usize> {
        let mut len = usize::from(self.first.span().len());
        for (dot, word) in &self.rest {
            len += usize::from(dot.span().len()) + usize::from(word.span().len());
        }
        if let Some(ref ga) = self.generic_args {
            len += ga.formatted_single_line_width();
        }
        Some(len)
    }
}

impl Printable for PathExpr {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        if self.rest.is_empty() {
            printer.print_raw_token(&self.first);
            if let Some(ref ga) = self.generic_args {
                printer.print(ga, shape);
            }
            return PrintInfo::default_single_line();
        }
        let first = Expression::Path(PathExpr {
            first: self.first.clone(),
            rest: Vec::new(),
            generic_args: None,
        });
        let chain_members = self
            .rest
            .iter()
            .map(|(dot, word)| PrintChainItem::FieldAccess(dot, word))
            .collect();
        let chain = PrintChain {
            first: &first,
            chain_members,
        };
        let info = chain.print(shape.clone(), printer);
        if let Some(ref ga) = self.generic_args {
            printer.print(ga, shape);
        }
        info
    }
    fn leftmost_token(&self) -> TextRange {
        self.first.span()
    }
    fn rightmost_token(&self) -> TextRange {
        if let Some(ref ga) = self.generic_args {
            return ga.close_angle.span();
        }
        self.rest
            .last()
            .map_or(&self.first, |(_, word)| word)
            .span()
    }
}

/// A generic instantiation whose base is not a plain path, e.g.
/// `(<T>(x: T) -> T { x })<int>` or `(foo)<int>`. Corresponds to a
/// [`SyntaxKind::PATH_EXPR`] node whose first child is an arbitrary expression
/// followed by `GENERIC_ARGS`.
#[derive(Debug)]
pub struct GenericApplyExpr {
    pub base: Box<Expression>,
    pub generic_args: GenericArgs,
}

impl FromCST for GenericApplyExpr {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::PATH_EXPR)?;

        let mut it = SyntaxNodeIter::new(&node);
        let base_elem = it
            .next()
            .ok_or_else(|| StrongAstError::missing(SyntaxKind::PAREN_EXPR, it.parent))?;
        let base = Box::new(Expression::from_cst(base_elem)?);
        let ga_elem = it
            .next()
            .ok_or_else(|| StrongAstError::missing(SyntaxKind::GENERIC_ARGS, it.parent))?;
        let generic_args = GenericArgs::from_cst(ga_elem)?;
        if let Some(extra) = it.next() {
            return Err(StrongAstError::UnexpectedAdditionalElement {
                parent: it.parent,
                at: extra.text_range(),
            });
        }
        Ok(GenericApplyExpr { base, generic_args })
    }
}

impl GenericApplyExpr {
    pub(crate) fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
        Some(self.base.single_line_width(input)? + self.generic_args.formatted_single_line_width())
    }
}

impl Printable for GenericApplyExpr {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let info = self.base.print(shape.clone(), printer);
        printer.print(&self.generic_args, shape);
        info
    }
    fn leftmost_token(&self) -> TextRange {
        self.base.leftmost_token()
    }
    fn rightmost_token(&self) -> TextRange {
        self.generic_args.close_angle.span()
    }
}

/// Corresponds to a [`SyntaxKind::PAREN_EXPR`] node.
#[derive(Debug)]
pub struct ParenExpr {
    pub open_paren: t::LParen,
    pub expr: Box<Expression>,
    pub close_paren: t::RParen,
}

impl FromCST for ParenExpr {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::PAREN_EXPR)?;

        let mut it = SyntaxNodeIter::new(&node);

        let open_paren = it.expect_parse()?;

        let expr = it.expect_next("an expression")?;
        let expr = Expression::from_cst(expr)?;

        let close_paren = it.expect_parse()?;

        it.expect_end()?;

        Ok(ParenExpr {
            open_paren,
            expr: Box::new(expr),
            close_paren,
        })
    }
}

impl KnownKind for ParenExpr {
    fn kind() -> SyntaxKind {
        SyntaxKind::PAREN_EXPR
    }
}

impl ParenExpr {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    pub(crate) fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
        let inner = self.expr.single_line_width(input)?;
        let (_, open_trailing) = input.trivia.get_for_range_split(self.open_paren.span());
        let (expr_leading, expr_trailing) = input.trivia.get_for_element(&*self.expr);
        let (close_leading, _) = input.trivia.get_for_range_split(self.close_paren.span());
        let trivia_len = open_trailing
            .iter()
            .chain(expr_leading)
            .chain(expr_trailing)
            .chain(close_leading)
            .map(|t| t.single_line_len(input.input))
            .sum::<Option<usize>>()?;
        Some(const { "()".len() } + inner + trivia_len)
    }

    /// Whether no comments are attached to either paren token or to the inner
    /// expression's boundary. Peeling a transparent paren cannot lose trivia:
    /// every span a parent context queries around it is empty.
    fn is_transparent(&self, trivia: &TriviaInfo) -> bool {
        let (open_leading, open_trailing) = trivia.get_for_range_split(self.open_paren.span());
        let (close_leading, close_trailing) = trivia.get_for_range_split(self.close_paren.span());
        let (expr_leading, expr_trailing) = trivia.get_for_element(&*self.expr);
        open_leading.is_empty()
            && open_trailing.is_empty()
            && close_leading.is_empty()
            && close_trailing.is_empty()
            && expr_leading.is_empty()
            && expr_trailing.is_empty()
    }
}

impl PrintMultiLine for ParenExpr {
    /// Multi-line layout: inner expression wraps to an indented new line,
    /// closing paren aligns with the opening context.
    ///
    /// ```baml
    /// (
    ///     some_long_expression
    /// )
    /// ```
    fn print_multi_line(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let inner_shape = Shape {
            width: shape.width.saturating_sub(printer.config.indent_width),
            indent: shape.indent + printer.config.indent_width,
            first_line_offset: 0,
        };
        printer.print_raw_token(&self.open_paren);
        printer.print_trivia_all_trailing_for(self.open_paren.token_span);
        printer.print_newline();

        let (expr_leading, expr_trailing) = printer.trivia.get_for_element(&*self.expr);
        printer.print_trivia_with_newline(expr_leading.trim_blanks(), inner_shape.indent);
        printer.print_spaces(inner_shape.indent);
        printer.print(&*self.expr, inner_shape.clone());
        printer.print_trivia_trailing(expr_trailing);
        printer.print_newline();

        let (close_leading, _) = printer.trivia.get_for_range_split(self.close_paren.span());
        printer.print_trivia_with_newline(close_leading.trim_blanks(), inner_shape.indent);
        printer.print_spaces(shape.indent);
        printer.print_raw_token(&self.close_paren);
        PrintInfo::default_multi_lined()
    }
}

impl ParenExpr {
    /// Should be passed a sub-printer to avoid printing trivia in the outer printer
    /// in the event that the printer is unable to fit the parenthesized expression on a single line.
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        printer.print_raw_token(&self.open_paren);
        let (_, open_trailing) = printer.trivia.get_for_range_split(self.open_paren.span());
        printer.try_print_trivia_single_line_squished(open_trailing)?;

        let (expr_leading, expr_trailing) = printer.trivia.get_for_element(&*self.expr);
        printer.try_print_trivia_single_line_squished(expr_leading)?;
        if printer
            .print(&*self.expr, Shape::unlimited_single_line())
            .multi_lined
        {
            return None;
        }
        printer.try_print_trivia_single_line_squished(expr_trailing)?;

        let (close_leading, _) = printer.trivia.get_for_range_split(self.close_paren.span());
        printer.try_print_trivia_single_line_squished(close_leading)?;
        printer.print_raw_token(&self.close_paren);

        if printer.output.len() > shape.width {
            None
        } else {
            Some(PrintInfo::default_single_line())
        }
    }
}

impl Printable for ParenExpr {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|p| self.try_print_single_line(&shape, p))
            .unwrap_or_else(|| self.print_multi_line(shape, printer))
    }
    fn leftmost_token(&self) -> TextRange {
        self.open_paren.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.close_paren.span()
    }
}

impl Expression {
    /// Strips nested [`ParenExpr`] wrappers that are transparent (no comments
    /// attached to the parens or the inner expression's boundary), returning
    /// the innermost expression. Callers decide per context whether printing
    /// the peeled expression instead of `self` is safe.
    fn peel_transparent_parens(&self, trivia: &TriviaInfo) -> &Expression {
        let mut expr = self;
        while let Expression::Paren(paren) = expr {
            if !paren.is_transparent(trivia) {
                break;
            }
            expr = &paren.expr;
        }
        expr
    }

    /// Whether this expression binds at least as tightly as a postfix
    /// operator, i.e. it can sit directly in a receiver position (`X.f`,
    /// `X(..)`, `X[i]`) or as a unary operand with no parens around it.
    ///
    /// Numeric and keyword literals are excluded: the `.` in `(1).to_string()`
    /// re-lexes as part of a float once the parens come off. Object and map
    /// literals are excluded because a bare leading `{` is ambiguous with a
    /// block, and [`Expression::GenericApply`] because its `<` is ambiguous
    /// with a comparison.
    ///
    /// An optional-chain link anywhere on the spine also disqualifies it —
    /// see [`Self::has_optional_chain_link`].
    fn binds_as_postfix_operand(&self) -> bool {
        match self {
            Expression::Call(_) | Expression::Index(_) | Expression::FieldAccess(_) => {
                !self.has_optional_chain_link()
            }
            Expression::Path(_)
            | Expression::EnvAccess(_)
            | Expression::ArrayInitializer(_)
            | Expression::RawString(_)
            | Expression::BacktickString(_)
            | Expression::ByteString(_) => true,
            Expression::Literal(lit) => matches!(lit, Literal::String(_)),
            _ => false,
        }
    }

    /// Whether this expression's postfix spine contains a `?.` link.
    ///
    /// Parens around such a receiver are load-bearing, not decoration: they
    /// **end the short-circuit region**. `(a?.b).c` evaluates `(null).c` when
    /// `a` is null — a `TypeError` — where `a?.b.c` short-circuits to null. So
    /// peeling them would silently change runtime behavior and these parens
    /// always stay.
    ///
    /// Only the spine counts. A `?.` inside a call argument or index operand
    /// (`f(a?.b).c`) belongs to a separate chain and is unaffected.
    fn has_optional_chain_link(&self) -> bool {
        match self {
            Expression::OptionalFieldAccess(_)
            | Expression::OptionalIndex(_)
            | Expression::OptionalCall(_) => true,
            Expression::Call(call) => call.callee.has_optional_chain_link(),
            Expression::Index(index) => index.base.has_optional_chain_link(),
            Expression::FieldAccess(fa) => fa.base.has_optional_chain_link(),
            Expression::Paren(paren) => paren.expr.has_optional_chain_link(),
            _ => false,
        }
    }

    /// The expression a postfix-receiver or unary-operand position actually
    /// prints: transparent parens peel while what they wrap still stands on
    /// its own here, so the parens delimit nothing.
    /// `((xs).join(` `)).includes(x)` prints as `xs.join(` `).includes(x)`.
    ///
    /// A receiver that binds looser than postfix keeps *one* paren — removing
    /// it would re-parse against a different base (`(a ?? b).length()`) — but
    /// the redundant layers around it still peel, so `((a + b)).f()` prints as
    /// `(a + b).f()` rather than keeping the whole stack.
    pub(crate) fn effective_postfix_operand(&self, trivia: &TriviaInfo) -> &Expression {
        self.peel_to_needed_paren(trivia, false)
    }

    /// [`Self::effective_postfix_operand`] for a unary operand.
    ///
    /// Identical except that literals peel here. The literal restriction exists
    /// only to keep `(1).to_string()` from re-lexing its `.` into a float, and
    /// no `.` follows a unary operand — so `-((1))` prints as `-1` and
    /// `!((true))` as `!true`. A literal that *is* a postfix receiver
    /// (`-(1).to_string()`) sits in the receiver position, not this one, and
    /// still keeps its parens.
    pub(crate) fn effective_unary_operand(&self, trivia: &TriviaInfo) -> &Expression {
        self.peel_to_needed_paren(trivia, true)
    }

    fn peel_to_needed_paren(&self, trivia: &TriviaInfo, unary: bool) -> &Expression {
        let mut expr = self;
        while let Expression::Paren(paren) = expr {
            if !paren.is_transparent(trivia) {
                break;
            }
            // Peel only down to the last paren this position still needs: an
            // inner paren is reconsidered on the next turn, so a stack around
            // a looser-binding receiver collapses to exactly one.
            let stands_alone = paren.expr.binds_as_postfix_operand()
                || matches!(&*paren.expr, Expression::Paren(_))
                || (unary && matches!(&*paren.expr, Expression::Literal(_)));
            if !stands_alone {
                break;
            }
            expr = &paren.expr;
        }
        expr
    }
}

/// Corresponds to a [`SyntaxKind::BINARY_EXPR`] node.
#[derive(Debug)]
pub struct BinaryExpr {
    pub op: BinaryOp,
    pub sides: Box<(Expression, Expression)>,
}

impl FromCST for BinaryExpr {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::BINARY_EXPR)?;

        let mut it = SyntaxNodeIter::new(&node);

        // Get left expression
        let left = it.expect_next("left expression")?;
        let left_expr = Expression::from_cst(left)?;

        // Get operator — handle `??` which appears as two consecutive QUESTION tokens
        let op_elem = it.expect_next("binary operator")?;
        let op = if op_elem.kind() == SyntaxKind::QUESTION {
            // Check for second QUESTION to form `??`
            let first_range = op_elem.text_range();
            if let Some(second) = it.next_if_kind(SyntaxKind::QUESTION) {
                let combined_range = TextRange::new(first_range.start(), second.text_range().end());
                BinaryOp::QuestionQuestion(t::QuestionQuestion::new_from_span(combined_range))
            } else {
                return Err(StrongAstError::UnexpectedKindDesc {
                    expected_desc: "binary operator".into(),
                    found: SyntaxKind::QUESTION,
                    at: first_range,
                });
            }
        } else {
            BinaryOp::from_cst(op_elem)?
        };

        // Get right expression
        let right = it.expect_next("right expression")?;
        let right_expr = Expression::from_cst(right)?;

        it.expect_end()?;

        Ok(BinaryExpr {
            op,
            sides: Box::new((left_expr, right_expr)),
        })
    }
}

impl KnownKind for BinaryExpr {
    fn kind() -> SyntaxKind {
        SyntaxKind::BINARY_EXPR
    }
}

impl BinaryExpr {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    pub(crate) fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
        let left = self.effective_left(input.trivia);
        let right = &self.sides.1;
        let left_width = left.single_line_width(input)?;
        let right_width = right.single_line_width(input)?;
        // Must match trivia handled by try_print_single_line
        let mut trivia_len = 0usize;
        let left_trailing = input.trivia.get_trailing_for_element(left);

        let (op_leading, op_trailing) = input.trivia.get_for_range_split(self.op.span());
        trivia_len += (op_leading.try_squished_len(input.input)?
            + left_trailing.try_squished_len(input.input)?)
        .max(const { " ".len() }); // basically, if not comments then we have the space

        let right_leading = input.trivia.get_leading_for_element(right);
        trivia_len += (right_leading.try_squished_len(input.input)?
            + op_trailing.try_squished_len(input.input)?)
        .max(const { " ".len() }); // basically, if not comments then we have the space

        let len = left_width + usize::from(self.op.span().len()) + right_width + trivia_len;
        Some(len)
    }

    /// The left operand with redundant parens peeled.
    ///
    /// `(a && b) && c` and `a && b && c` parse to different trees but mean
    /// the same thing and print identically, so a transparent paren around
    /// the left operand is dropped when the inner operator sits in the same
    /// precedence row as this one (reparsing the output yields the printed
    /// tree, keeping the formatter idempotent). Right operands are never
    /// peeled: removing those parens would re-associate, as in `a - (b - c)`.
    /// Mixed-precedence parens like `(a * b) + c` are kept: they are
    /// redundant to the parser but carry clarity for the reader.
    fn effective_left(&self, trivia: &TriviaInfo) -> &Expression {
        let Some(row) = BinaryOpPrecedenceRow::row_for_op(&self.op) else {
            return &self.sides.0;
        };
        let peeled = self.sides.0.peel_transparent_parens(trivia);
        match peeled {
            Expression::Binary(inner)
                if BinaryOpPrecedenceRow::row_for_op(&inner.op) == Some(row) =>
            {
                peeled
            }
            _ => &self.sides.0,
        }
    }

    /// Recursively lifts binary expressions in the same chaining group to the top level.
    /// For ops that are not in any chaining groups, return will be the same as the original.
    /// Redundant parens around left operands are peeled (see [`Self::effective_left`])
    /// so a fully parenthesized chain flattens like an unparenthesized one.
    ///
    /// The vec will never be empty.
    fn get_chaining_members(
        &self,
        trivia: &TriviaInfo,
    ) -> (&Expression, Vec<(&BinaryOp, &Expression)>) {
        let mut members = Vec::new();
        let Some(chaining_group) = BinaryOpChainingGroup::group_for_op(&self.op) else {
            members.push((&self.op, &self.sides.1));
            return (&self.sides.0, members);
        };

        match (self.effective_left(trivia), &self.sides.1) {
            (Expression::Binary(left), Expression::Binary(right))
                if BinaryOpChainingGroup::group_for_op(&left.op) == Some(chaining_group)
                    && BinaryOpChainingGroup::group_for_op(&right.op) == Some(chaining_group) =>
            {
                let (left_first, left_rest) = left.get_chaining_members(trivia);
                let (right_first, right_rest) = right.get_chaining_members(trivia);

                members.extend(left_rest);
                members.push((&self.op, right_first));
                members.extend(right_rest);

                (left_first, members)
            }
            (Expression::Binary(left), right)
                if BinaryOpChainingGroup::group_for_op(&left.op) == Some(chaining_group) =>
            {
                let (first, left_rest) = left.get_chaining_members(trivia);

                members.extend(left_rest);
                members.push((&self.op, right));
                (first, members)
            }
            (left, Expression::Binary(right))
                if BinaryOpChainingGroup::group_for_op(&right.op) == Some(chaining_group) =>
            {
                let (right_first, right_rest) = right.get_chaining_members(trivia);

                members.push((&self.op, right_first));
                members.extend(right_rest);
                (left, members)
            }
            (left, right) => {
                members.push((&self.op, right));
                (left, members)
            }
        }
    }
}

impl PrintMultiLine for BinaryExpr {
    /// Multi-line layout: splits at the operator. The operator and right-hand
    /// side wrap to an indented new line. Trailing comments on sub-expressions
    /// are preserved.
    ///
    /// ```baml
    /// left_expression // trailing comment
    ///     + right_expression
    /// ```
    ///
    /// For chainable operators, contained binary ops (of the same group) should be printed at the same indentation.
    /// Groups:
    ///     - Add/Subtract
    ///     - Multiply/Divide/Modulo
    ///     - Bitwise And/Or/Xor
    ///     - Logical And/Or
    ///
    /// ```baml
    /// a
    ///     + b
    ///     + c
    ///     - d * e
    /// ```
    ///
    /// ```baml
    /// // precedence matters:
    /// aaaaaaaaa
    ///     + bbbbbbbbb
    ///         * cccccccc
    ///         / dddddddd
    ///     - eeeeeee
    /// ```
    fn print_multi_line(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let inner_indent = shape.indent + printer.config.indent_width;
        let (first, chain_members) = self.get_chaining_members(printer.trivia);
        printer.print(first, shape);
        printer.print_trivia_all_trailing_for(first.rightmost_token());
        let num_chain_members = chain_members.len();
        for (i, (op, right)) in chain_members.into_iter().enumerate() {
            printer.print_newline();
            printer.print_spaces(inner_indent);
            printer.print(op, Shape::unlimited_single_line());
            printer.print_str(" ");
            let inner_shape = Shape {
                width: printer
                    .config
                    .line_width
                    .saturating_sub(inner_indent + usize::from(op.span().len()) + 1),
                indent: inner_indent,
                first_line_offset: usize::from(op.span().len()) + 1,
            };
            printer.print(right, inner_shape.clone());
            if i + 1 < num_chain_members {
                printer.print_trivia_all_trailing_for(right.rightmost_token());
            }
        }
        PrintInfo::default_multi_lined()
    }
}

impl BinaryExpr {
    /// Should be passed a sub-printer to avoid printing trivia in the outer printer
    /// in the event that the printer is unable to fit the binary expression on a single line.
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        let left = self.effective_left(printer.trivia);
        let right = &self.sides.1;

        if printer
            .print(left, Shape::unlimited_single_line())
            .multi_lined
        {
            return None;
        }
        let left_trailing = printer.trivia.get_trailing_for_element(left);
        let (op_leading, op_trailing) = printer.trivia.get_for_range_split(self.op.span());
        let right_leading = printer.trivia.get_leading_for_element(right);

        let mut left_trivia_len = printer.try_print_trivia_single_line_squished(left_trailing)?;
        left_trivia_len += printer.print_trivia_squished(op_leading);
        if left_trivia_len == 0 {
            printer.print_spaces(1); // only add space if there are no block comments between
        }

        printer.print(&self.op, Shape::unlimited_single_line());

        let mut right_trivia_len = printer.print_trivia_squished(op_trailing);
        right_trivia_len += printer.print_trivia_squished(right_leading);
        if right_trivia_len == 0 {
            printer.print_spaces(1); // only add space if there are no block comments between
        }
        if printer
            .print(right, Shape::unlimited_single_line())
            .multi_lined
        {
            return None;
        }
        // right trailing is the outermost trailing — not printed here

        if printer.output.len() > shape.width {
            None
        } else {
            Some(PrintInfo::default_single_line())
        }
    }
}

impl Printable for BinaryExpr {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|p| self.try_print_single_line(&shape, p))
            .unwrap_or_else(|| self.print_multi_line(shape, printer))
    }
    fn leftmost_token(&self) -> TextRange {
        self.sides.0.leftmost_token()
    }
    fn rightmost_token(&self) -> TextRange {
        self.sides.1.rightmost_token()
    }
}

/// Categories for grouping binary operators for nested chaining
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BinaryOpChainingGroup {
    AddSubtract,
    MultiplyDivide,
    Bitwise,
    Logical,
}
impl BinaryOpChainingGroup {
    fn group_for_op(op: &BinaryOp) -> Option<Self> {
        match op {
            BinaryOp::Plus(_) | BinaryOp::Minus(_) => Some(Self::AddSubtract),
            BinaryOp::Star(_) | BinaryOp::Slash(_) | BinaryOp::Percent(_) => {
                Some(Self::MultiplyDivide)
            }
            BinaryOp::And(_) | BinaryOp::Pipe(_) | BinaryOp::Caret(_) => Some(Self::Bitwise),
            BinaryOp::AndAnd(_) | BinaryOp::OrOr(_) => Some(Self::Logical),
            _ => None,
        }
    }
}

/// Precedence rows whose redundant left-operand parens the formatter strips
/// (see [`BinaryExpr::effective_left`]). Ops within a row share one binding
/// power in the parser (`infix_binding_power`), so `(a OP b) OP c` reparses
/// identically without the parens. Comparisons, equality, shifts, `??`, and
/// assignments are deliberately absent: chains of those are unusual enough
/// that explicit parens read as intent.
///
/// Finer-grained than [`BinaryOpChainingGroup`], which mixes precedence
/// levels (`&&` with `||`, `&` with `|`) because it only groups layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BinaryOpPrecedenceRow {
    AddSubtract,
    MultiplyDivideModulo,
    BitwiseAnd,
    BitwiseOr,
    BitwiseXor,
    LogicalAnd,
    LogicalOr,
}
impl BinaryOpPrecedenceRow {
    fn row_for_op(op: &BinaryOp) -> Option<Self> {
        match op {
            BinaryOp::Plus(_) | BinaryOp::Minus(_) => Some(Self::AddSubtract),
            BinaryOp::Star(_) | BinaryOp::Slash(_) | BinaryOp::Percent(_) => {
                Some(Self::MultiplyDivideModulo)
            }
            BinaryOp::And(_) => Some(Self::BitwiseAnd),
            BinaryOp::Pipe(_) => Some(Self::BitwiseOr),
            BinaryOp::Caret(_) => Some(Self::BitwiseXor),
            BinaryOp::AndAnd(_) => Some(Self::LogicalAnd),
            BinaryOp::OrOr(_) => Some(Self::LogicalOr),
            _ => None,
        }
    }
}

/// Corresponds to a [`SyntaxKind::IS_EXPR`] node.
///
/// `<expr> is <pattern>` — Rust `matches!`-style pattern test. Structure is
/// rigid (an expression LHS, a single keyword, a pattern RHS), so the
/// formatter prints it on a single line whenever it fits and otherwise
/// keeps the keyword glued to the pattern on the next line.
#[derive(Debug)]
pub struct IsExpr {
    pub lhs: Box<Expression>,
    pub keyword: t::Is,
    pub pattern: MatchPattern,
}

impl FromCST for IsExpr {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::IS_EXPR)?;

        let mut it = SyntaxNodeIter::new(&node);
        let lhs_elem = it.expect_next("`is` left expression")?;
        let lhs = Expression::from_cst(lhs_elem)?;
        let kw_elem = it.expect_next("`is` keyword")?;
        let keyword = t::Is::from_cst(kw_elem)?;
        let pat_elem = it.expect_next("`is` pattern")?;
        let pattern = MatchPattern::from_cst(pat_elem)?;
        it.expect_end()?;

        Ok(IsExpr {
            lhs: Box::new(lhs),
            keyword,
            pattern,
        })
    }
}

impl KnownKind for IsExpr {
    fn kind() -> SyntaxKind {
        SyntaxKind::IS_EXPR
    }
}

impl IsExpr {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if the LHS can never be single-lined.
    pub(crate) fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
        let lhs = self.lhs.single_line_width(input)?;
        // The pattern's width is hard to query precisely without
        // reimplementing MatchPattern's own width logic, so use the source
        // span between leftmost and rightmost tokens as an upper bound —
        // overestimates by leading/trailing trivia, which is fine for the
        // line-fit check.
        let pat_left = self.pattern.leftmost_token().start();
        let pat_right = self.pattern.rightmost_token().end();
        let pattern_width = usize::from(pat_right - pat_left);
        // `<lhs> is <pattern>` — lhs + " " + "is" + " " + pattern.
        Some(lhs + 1 + usize::from(self.keyword.span().len()) + 1 + pattern_width)
    }
}

impl Printable for IsExpr {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        // Mirrors `BinaryExpr::try_print_single_line`'s trivia handling so
        // comments around the `is` keyword (e.g. `v /*hint*/ is int`) round-
        // trip instead of being silently dropped.
        let mut multi_lined = false;

        multi_lined |= printer.print(&*self.lhs, shape.clone()).multi_lined;

        let lhs_trailing = printer.trivia.get_trailing_for_element(&*self.lhs);
        let (kw_leading, kw_trailing) = printer.trivia.get_for_range_split(self.keyword.span());

        let mut left_trivia_len = printer.print_trivia_squished(lhs_trailing);
        left_trivia_len += printer.print_trivia_squished(kw_leading);
        if left_trivia_len == 0 {
            printer.print_spaces(1);
        }

        printer.print_raw_token(&self.keyword);

        let pat_leading = printer.trivia.get_leading_for_element(&self.pattern);
        let mut right_trivia_len = printer.print_trivia_squished(kw_trailing);
        right_trivia_len += printer.print_trivia_squished(pat_leading);
        if right_trivia_len == 0 {
            printer.print_spaces(1);
        }

        multi_lined |= printer.print(&self.pattern, shape).multi_lined;
        PrintInfo { multi_lined }
    }
    fn leftmost_token(&self) -> TextRange {
        self.lhs.leftmost_token()
    }
    fn rightmost_token(&self) -> TextRange {
        self.pattern.rightmost_token()
    }
}

/// Corresponds to a [`SyntaxKind::UNARY_EXPR`] node.
#[derive(Debug)]
pub struct UnaryExpr {
    pub op: UnaryOp,
    pub expr: Box<Expression>,
}

impl FromCST for UnaryExpr {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::UNARY_EXPR)?;

        let mut it = SyntaxNodeIter::new(&node);

        // Get operator
        let op = it.expect_next("unary operator")?;
        let op = UnaryOp::from_cst(op)?;

        // Get expression
        let expr_node = it.expect_next("expression")?;
        let expr = Expression::from_cst(expr_node)?;

        it.expect_end()?;

        Ok(UnaryExpr {
            op,
            expr: Box::new(expr),
        })
    }
}

impl KnownKind for UnaryExpr {
    fn kind() -> SyntaxKind {
        SyntaxKind::UNARY_EXPR
    }
}

impl UnaryExpr {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    pub(crate) fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
        let expr = self
            .expr
            .effective_unary_operand(input.trivia)
            .single_line_width(input)?;
        Some(usize::from(self.op.span().len()) + expr)
    }
}

impl Printable for UnaryExpr {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let mut multi_lined = false;
        multi_lined |= printer.print(&self.op, shape.clone()).multi_lined;
        let expr = self.expr.effective_unary_operand(printer.trivia);
        multi_lined |= printer.print(expr, shape).multi_lined;

        PrintInfo { multi_lined }
    }
    fn leftmost_token(&self) -> TextRange {
        self.op.leftmost_token()
    }
    fn rightmost_token(&self) -> TextRange {
        self.expr.rightmost_token()
    }
}

/// Corresponds to a [`SyntaxKind::IF_EXPR`] node.
#[derive(Debug)]
pub struct IfExpr {
    pub keyword: t::If,
    /// The condition expression. Parens are optional in Baml, so this can be
    /// any expression — `if (a == b)` and `if a == b` are both valid.
    pub condition: Box<Expression>,
    pub block: BlockExpr,
    pub else_branch: Option<(t::Else, ElseExpr)>,
}

impl FromCST for IfExpr {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::IF_EXPR)?;

        let mut it = SyntaxNodeIter::new(&node);

        // KW_IF
        let keyword = it.expect_parse()?;

        // Condition: any expression (parens are optional in Baml).
        let condition_elem = it.expect_next("an if condition expression")?;
        let condition = Box::new(Expression::from_cst(condition_elem)?);

        // BLOCK_EXPR
        let block: BlockExpr = it.expect_parse()?;

        // Optional else branch
        let else_branch = if let Some(elem) = it.next() {
            let else_token = t::Else::from_cst(elem)?;

            let else_body_node = it.expect_node("else body (if, if-let, or block)")?;
            let else_body = match else_body_node.kind() {
                SyntaxKind::IF_EXPR => ElseExpr::If(Box::new(IfExpr::from_cst(
                    SyntaxElement::Node(else_body_node),
                )?)),
                SyntaxKind::IF_LET_EXPR => ElseExpr::IfLet(Box::new(IfLetExpr::from_cst(
                    SyntaxElement::Node(else_body_node),
                )?)),
                SyntaxKind::BLOCK_EXPR => ElseExpr::Block(Box::new(BlockExpr::from_cst(
                    SyntaxElement::Node(else_body_node),
                )?)),
                _ => {
                    return Err(StrongAstError::UnexpectedKindDesc {
                        expected_desc: "IF_EXPR, IF_LET_EXPR, or BLOCK_EXPR".into(),
                        found: else_body_node.kind(),
                        at: else_body_node.text_range(),
                    });
                }
            };

            Some((else_token, else_body))
        } else {
            None
        };

        it.expect_end()?;

        Ok(IfExpr {
            keyword,
            condition,
            block,
            else_branch,
        })
    }
}

impl KnownKind for IfExpr {
    fn kind() -> SyntaxKind {
        SyntaxKind::IF_EXPR
    }
}

impl Printable for IfExpr {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.keyword);
        printer.print_str(" ");
        // Always print parens around the condition. Source may omit them
        // (Baml allows `if cond { ... }`), but emitting them keeps formatter
        // output consistent with the canonical form.
        let needs_parens = !matches!(*self.condition, Expression::Paren(_));
        let cond_shape = if needs_parens {
            // Reserve room for the synthetic `( )` so a barely-fitting
            // condition doesn't push the line past the width budget once
            // we wrap parens around it.
            let mut s = shape.clone();
            s.width = s.width.saturating_sub(2);
            s
        } else {
            shape.clone()
        };
        if needs_parens {
            printer.print_str("(");
        }
        printer.print(&*self.condition, cond_shape);
        if needs_parens {
            printer.print_str(")");
        }
        printer.print_str(" ");
        printer.print(&self.block, shape.clone());

        if let Some((else_kw, else_expr)) = &self.else_branch {
            printer.print_str(" ");
            printer.print_raw_token(else_kw);
            printer.print_str(" ");
            printer.print(else_expr, shape);
        }

        PrintInfo::default_multi_lined()
    }
    fn leftmost_token(&self) -> TextRange {
        self.keyword.span()
    }
    fn rightmost_token(&self) -> TextRange {
        if let Some((_, else_expr)) = &self.else_branch {
            else_expr.rightmost_token()
        } else {
            self.block.rightmost_token()
        }
    }
}

/// Used in [`IfExpr`] / [`IfLetExpr`] to represent the else/else-if branch.
#[derive(Debug)]
pub enum ElseExpr {
    /// else if
    If(Box<IfExpr>),
    /// else if let
    IfLet(Box<IfLetExpr>),
    /// final else block
    Block(Box<BlockExpr>),
}

impl Printable for ElseExpr {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            ElseExpr::If(if_expr) => if_expr.print(shape, printer),
            ElseExpr::IfLet(if_let_expr) => if_let_expr.print(shape, printer),
            ElseExpr::Block(block) => block.print(shape, printer),
        }
    }
    fn leftmost_token(&self) -> TextRange {
        match self {
            ElseExpr::If(if_expr) => if_expr.leftmost_token(),
            ElseExpr::IfLet(if_let_expr) => if_let_expr.leftmost_token(),
            ElseExpr::Block(block) => block.leftmost_token(),
        }
    }
    fn rightmost_token(&self) -> TextRange {
        match self {
            ElseExpr::If(if_expr) => if_expr.rightmost_token(),
            ElseExpr::IfLet(if_let_expr) => if_let_expr.rightmost_token(),
            ElseExpr::Block(block) => block.rightmost_token(),
        }
    }
}

/// Corresponds to a [`SyntaxKind::IF_LET_EXPR`] node.
///
/// `if let PATTERN = SCRUTINEE BLOCK (else (BLOCK | IF_EXPR | IF_LET_EXPR))?`
#[derive(Debug)]
pub struct IfLetExpr {
    pub keyword: t::If,
    /// `let PATTERN` — the leading `let` is part of the pattern grammar
    /// (`parse_let_pattern`), so it's stored inside `pattern` rather than
    /// as a separate token.
    pub pattern: MatchPattern,
    pub equals: t::Equals,
    pub scrutinee: Box<Expression>,
    pub block: BlockExpr,
    pub else_branch: Option<(t::Else, ElseExpr)>,
}

impl FromCST for IfLetExpr {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::IF_LET_EXPR)?;

        let mut it = SyntaxNodeIter::new(&node);

        // KW_IF
        let keyword = it.expect_parse()?;

        // PATTERN (consumes its own leading `let` token)
        let pattern = it.expect_parse()?;

        // `=` separator between pattern and scrutinee
        let equals = it.expect_parse()?;

        // Scrutinee: any expression
        let scrutinee_elem = it.expect_next("if-let scrutinee expression")?;
        let scrutinee = Box::new(Expression::from_cst(scrutinee_elem)?);

        // Then block
        let block: BlockExpr = it.expect_parse()?;

        // Optional else / else-if / else-if-let
        let else_branch = if let Some(elem) = it.next() {
            let else_token = t::Else::from_cst(elem)?;
            let else_body_node = it.expect_node("else body (if, if-let, or block)")?;
            let else_body = match else_body_node.kind() {
                SyntaxKind::IF_EXPR => ElseExpr::If(Box::new(IfExpr::from_cst(
                    SyntaxElement::Node(else_body_node),
                )?)),
                SyntaxKind::IF_LET_EXPR => ElseExpr::IfLet(Box::new(IfLetExpr::from_cst(
                    SyntaxElement::Node(else_body_node),
                )?)),
                SyntaxKind::BLOCK_EXPR => ElseExpr::Block(Box::new(BlockExpr::from_cst(
                    SyntaxElement::Node(else_body_node),
                )?)),
                _ => {
                    return Err(StrongAstError::UnexpectedKindDesc {
                        expected_desc: "IF_EXPR, IF_LET_EXPR, or BLOCK_EXPR".into(),
                        found: else_body_node.kind(),
                        at: else_body_node.text_range(),
                    });
                }
            };
            Some((else_token, else_body))
        } else {
            None
        };

        it.expect_end()?;

        Ok(IfLetExpr {
            keyword,
            pattern,
            equals,
            scrutinee,
            block,
            else_branch,
        })
    }
}

impl KnownKind for IfLetExpr {
    fn kind() -> SyntaxKind {
        SyntaxKind::IF_LET_EXPR
    }
}

impl Printable for IfLetExpr {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.keyword);
        printer.print_str(" ");
        // `if let PATTERN = SCRUTINEE { ... }` — pattern carries its own
        // leading `let`. No surrounding parens around the pattern or
        // scrutinee (unlike plain `if`, where parens are canonicalised
        // around the condition).
        printer.print(&self.pattern, shape.clone());
        printer.print_str(" ");
        printer.print_raw_token(&self.equals);
        printer.print_str(" ");
        printer.print(&*self.scrutinee, shape.clone());
        printer.print_str(" ");
        printer.print(&self.block, shape.clone());

        if let Some((else_kw, else_expr)) = &self.else_branch {
            printer.print_str(" ");
            printer.print_raw_token(else_kw);
            printer.print_str(" ");
            printer.print(else_expr, shape);
        }

        PrintInfo::default_multi_lined()
    }
    fn leftmost_token(&self) -> TextRange {
        self.keyword.span()
    }
    fn rightmost_token(&self) -> TextRange {
        if let Some((_, else_expr)) = &self.else_branch {
            else_expr.rightmost_token()
        } else {
            self.block.rightmost_token()
        }
    }
}

/// An element of a match/catch arm list: an arm, or a `//#` header comment
/// appearing between arms. Headers are legal arm-list elements (the parser
/// consumes them there, mirroring statement blocks), so the strong AST must
/// carry them through formatting.
#[derive(Debug)]
pub enum ArmListItem<A> {
    Arm(A),
    Header(t::HeaderComment),
}

impl<A: Printable> Printable for ArmListItem<A> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            Self::Arm(arm) => arm.print(shape, printer),
            Self::Header(header) => {
                printer.print_raw_token(header);
                PrintInfo::default_single_line()
            }
        }
    }

    fn leftmost_token(&self) -> TextRange {
        match self {
            Self::Arm(arm) => arm.leftmost_token(),
            Self::Header(header) => header.span(),
        }
    }

    fn rightmost_token(&self) -> TextRange {
        match self {
            Self::Arm(arm) => arm.rightmost_token(),
            Self::Header(header) => header.span(),
        }
    }
}

/// Corresponds to a [`SyntaxKind::MATCH_EXPR`] node.
#[derive(Debug)]
pub struct MatchExpr {
    pub keyword: t::Match,
    pub open_paren: t::LParen,
    pub scrutinee: Box<Expression>,
    pub close_paren: t::RParen,
    pub open_brace: t::LBrace,
    pub arms: Vec<ArmListItem<MatchArm>>,
    pub close_brace: t::RBrace,
}

impl FromCST for MatchExpr {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::MATCH_EXPR)?;

        let mut it = SyntaxNodeIter::new(&node);

        // KW_MATCH
        let keyword = it.expect_parse()?;

        // L_PAREN
        let open_paren = it.expect_parse()?;

        // Scrutinee expression (can be any node that represents an expression)
        let scrutinee_node = it.expect_next("scrutinee expression")?;
        let scrutinee = Box::new(Expression::from_cst(scrutinee_node)?);

        // R_PAREN
        let close_paren = it.expect_parse()?;

        // L_BRACE
        let open_brace = it.expect_parse()?;

        // Collect match arms
        let mut arms = Vec::new();
        let close_brace = loop {
            let Some(elem) = it.next() else {
                return Err(StrongAstError::missing(SyntaxKind::R_BRACE, it.parent));
            };
            match elem.kind() {
                SyntaxKind::R_BRACE => {
                    break t::RBrace::from_cst(elem)?;
                }
                SyntaxKind::MATCH_ARM => {
                    let arm = MatchArm::from_cst(elem)?;
                    arms.push(ArmListItem::Arm(arm));
                }
                SyntaxKind::HEADER_COMMENT => {
                    arms.push(ArmListItem::Header(t::HeaderComment::from_cst(elem)?));
                }
                _ => {
                    return Err(StrongAstError::UnexpectedKindDesc {
                        expected_desc: "MATCH_ARM, HEADER_COMMENT, or R_BRACE".into(),
                        found: elem.kind(),
                        at: elem.text_range(),
                    });
                }
            }
        };

        it.expect_end()?;

        Ok(MatchExpr {
            keyword,
            open_paren,
            scrutinee,
            close_paren,
            open_brace,
            arms,
            close_brace,
        })
    }
}

impl KnownKind for MatchExpr {
    fn kind() -> SyntaxKind {
        SyntaxKind::MATCH_EXPR
    }
}

impl MatchExpr {
    fn try_print_scrutinee_single_line(
        &self,
        shape: &Shape,
        printer: &mut Printer,
    ) -> Option<PrintInfo> {
        printer.print_raw_token(&self.open_paren);
        let (_, open_trailing) = printer.trivia.get_for_range_split(self.open_paren.span());
        printer.try_print_trivia_single_line_squished(open_trailing)?;

        let (scrutinee_leading, scrutinee_trailing) =
            printer.trivia.get_for_element(&*self.scrutinee);
        printer.try_print_trivia_single_line_squished(scrutinee_leading)?;
        if printer
            .print(&*self.scrutinee, Shape::unlimited_single_line())
            .multi_lined
        {
            return None;
        }
        printer.try_print_trivia_single_line_squished(scrutinee_trailing)?;

        let (close_leading, _) = printer.trivia.get_for_range_split(self.close_paren.span());
        printer.try_print_trivia_single_line_squished(close_leading)?;
        printer.print_raw_token(&self.close_paren);

        if printer.output.len() > shape.width {
            None
        } else {
            Some(PrintInfo::default_single_line())
        }
    }

    fn print_scrutinee_multi_line(&self, shape: &Shape, printer: &mut Printer) {
        let paren_inner_indent = shape.indent + printer.config.indent_width;
        printer.print_raw_token(&self.open_paren);
        printer.print_trivia_all_trailing_for(self.open_paren.span());
        printer.print_newline();

        printer.print_standalone_with_trivia(&*self.scrutinee, paren_inner_indent);
        printer.print_newline();
        printer
            .print_trivia_all_leading_with_newline_for(self.close_paren.span(), paren_inner_indent);
        printer.print_spaces(shape.indent);
        printer.print_raw_token(&self.close_paren);
    }
}

impl Printable for MatchExpr {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let inner_indent = shape.indent + printer.config.indent_width;

        // Print "match" keyword
        printer.print_raw_token(&self.keyword);
        printer.print_str(" ");

        // Print scrutinee: try single-line, fall back to multi-line
        if printer
            .try_sub_printer(|p| self.try_print_scrutinee_single_line(&shape, p))
            .is_none()
        {
            self.print_scrutinee_multi_line(&shape, printer);
        }

        // Print body with block container pattern
        printer.print_str(" ");
        printer.print_raw_token(&self.open_brace);
        printer.print_trivia_all_trailing_for(self.open_brace.span());
        printer.print_newline();

        for arm in &self.arms {
            printer.print_standalone_with_trivia(arm, inner_indent);
            printer.print_newline();
        }

        printer.print_trivia_all_leading_with_newline_for(self.close_brace.span(), inner_indent);
        printer.print_spaces(shape.indent);
        printer.print_raw_token(&self.close_brace);

        PrintInfo::default_multi_lined()
    }
    fn leftmost_token(&self) -> TextRange {
        self.keyword.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.close_brace.span()
    }
}

/// Corresponds to a [`SyntaxKind::MATCH_ARM`] node.
#[derive(Debug)]
pub struct MatchArm {
    pub pattern: MatchPattern,
    pub guard: Option<MatchGuard>,
    pub fat_arrow: t::FatArrow,
    pub body: Expression,
    pub comma: Option<t::Comma>,
}

impl FromCST for MatchArm {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::MATCH_ARM)?;

        let mut it = SyntaxNodeIter::new(&node);

        // MATCH_PATTERN
        let pattern: MatchPattern = it.expect_parse()?;

        // Check for optional guard (if condition)
        let guard = it
            .next_if_kind(SyntaxKind::MATCH_GUARD)
            .map(MatchGuard::from_cst)
            .transpose()?;

        // FAT_ARROW
        let fat_arrow = it.expect_parse()?;

        // Body expression
        let body_node = it.expect_next("match arm body")?;
        let body = Expression::from_cst(body_node)?;

        let comma = it.next().map(t::Comma::from_cst).transpose()?;

        it.expect_end()?;

        Ok(MatchArm {
            pattern,
            guard,
            fat_arrow,
            body,
            comma,
        })
    }
}

impl KnownKind for MatchArm {
    fn kind() -> SyntaxKind {
        SyntaxKind::MATCH_ARM
    }
}

impl MatchArm {
    /// Prints all of the arm except the body/expression (prints up to and including the `=>`)
    fn print_condition(&self, shape: &Shape, printer: &mut Printer) -> PrintInfo {
        let mut multi_lined = false;

        let mut pattern_printer = printer.sub_printer();
        let pattern_info = pattern_printer.print(&self.pattern, shape.clone());
        multi_lined |= pattern_info.multi_lined;
        let pattern_len = pattern_printer.len();
        printer.append_from_printer(pattern_printer);

        if let Some(guard) = &self.guard {
            if pattern_info.multi_lined {
                // Guard goes on new line
                printer.print_newline();
                printer.print_spaces(shape.indent + printer.config.indent_width);
                let offset = usize::from(guard.keyword.token_span.len()) + const { " ".len() };
                let guard_shape = Shape {
                    width: printer.config.line_width.saturating_sub(
                        shape.indent + printer.config.indent_width + offset + const { " =>".len() },
                    ),
                    indent: shape.indent + printer.config.indent_width,
                    first_line_offset: offset,
                };
                guard.print(guard_shape, printer);
            } else if matches!(guard.condition, Expression::Paren(_) | Expression::Block(_)) {
                // we can delegate determining whether or not to multi-line to the guard expression
                // since it will do so nicely
                printer.print_spaces(1);
                let offset = shape.first_line_offset + pattern_len + 1;
                let guard_shape = Shape {
                    width: printer
                        .config
                        .line_width
                        .saturating_sub(shape.indent + offset + const { " => ".len() }),
                    indent: shape.indent,
                    first_line_offset: offset,
                };
                let guard_info = guard.print(guard_shape, printer);
                multi_lined |= guard_info.multi_lined;
            } else {
                // try printing guard single-line
                let mut guard_single_line = printer.sub_printer();
                let guard_info =
                    guard.print(Shape::unlimited_single_line(), &mut guard_single_line);

                let single_line_len = pattern_len
                    + const { " ".len() }
                    + guard_single_line.len()
                    + const { " =>".len() };
                if guard_info.multi_lined || single_line_len > shape.width {
                    // Guard is too long to fit on a single line, so print it on the next line
                    printer.print_newline();
                    printer.print_spaces(shape.indent + printer.config.indent_width);
                    let guard_shape = Shape {
                        width: printer
                            .config
                            .line_width
                            .saturating_sub(shape.indent + const { " => {".len() }),
                        indent: shape.indent,
                        first_line_offset: 0,
                    };
                    guard.print(guard_shape, printer);
                } else {
                    // guard goes on the same line after the pattern
                    printer.print_spaces(1);
                    printer.append_from_printer(guard_single_line);
                }
            }
        }

        printer.print_str(" =>");

        PrintInfo { multi_lined }
    }
}

/// Print an arm body that is being wrapped into a `{ … }` block (the `{` and
/// newline are already emitted; the caller emits the closing `}`).
///
/// `arm_indent` is the arm's own indent; the body is printed one level deeper.
/// A braceless jump body (`return`/`break`/`continue`) additionally gets its
/// statement `;` — and its trailing trivia is deliberately left for the arm
/// level so a same-line comment stays attached to the arm (emitted after the
/// wrapped `},`) instead of being split from the `;` or dropped/duplicated when
/// the arm has no comma (B-629).
fn print_wrapped_arm_body(printer: &mut Printer, body: &Expression, arm_indent: usize) {
    let inner_indent = arm_indent + printer.config.indent_width;
    if matches!(
        body,
        Expression::Return(_) | Expression::Break(_) | Expression::Continue(_)
    ) {
        printer.print_standalone_leading_and_body(body, inner_indent);
        printer.print_str(";");
    } else {
        printer.print_standalone_with_trivia(body, inner_indent);
    }
}

impl Printable for MatchArm {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let condition_info = self.print_condition(&shape, printer);
        let condition_multi_lined = condition_info.multi_lined;

        if condition_multi_lined {
            // the body goes in a block expression on a new line
            printer.print_newline();

            printer.print_spaces(shape.indent);
            if let Expression::Block(block) = &self.body {
                // body is already a block expression
                let body_shape = Shape {
                    width: printer.config.line_width.saturating_sub(shape.indent),
                    indent: shape.indent,
                    first_line_offset: 0,
                };
                printer.print(block, body_shape);
                printer.print_str(",");
            } else {
                // put the body in a block expression
                printer.print_str("{");
                printer.print_newline();
                print_wrapped_arm_body(printer, &self.body, shape.indent);
                printer.print_newline();
                printer.print_spaces(shape.indent);
                printer.print_str("},");
            }
            return PrintInfo::default_multi_lined();
        }

        // condition is single line, see if we can fit the body with it
        // TODO: if the body is a block with only a tail expression, we might be able to un-nest it

        printer.print_spaces(1);
        let line_len_remaining = printer.current_line_remaining_width();
        if let Expression::Block(block) = &self.body {
            // If it is a block expression, we print it directly in front of the ` => `.
            // Since the condition was single-line, the preceding line had no extra indent
            // so we don't need to put the `{` on a new line.
            let body_shape = Shape {
                width: line_len_remaining,
                indent: shape.indent,
                first_line_offset: printer
                    .config
                    .line_width
                    .saturating_sub(shape.indent + line_len_remaining),
            };
            let info = printer.print(block, body_shape);
            printer.print_str(",");
            return info;
        } else if let Expression::Match(match_expr) = &self.body
            && let Some(match_scrutinee_len) = match_expr.scrutinee.single_line_width(printer)
            && const { "match () {".len() } + match_scrutinee_len <= line_len_remaining
        {
            // Match expressions also may go directly on the same line if
            // `match (...) {` fits. The arms can be multi-line.
            let match_shape = Shape {
                width: line_len_remaining,
                indent: shape.indent,
                first_line_offset: printer
                    .config
                    .line_width
                    .saturating_sub(shape.indent + line_len_remaining),
            };
            let info = match_expr.print(match_shape, printer);
            printer.print_str(",");
            return info;
        }

        // try and print the body single-line
        let mut try_body = printer.sub_printer();
        let try_body_info = self
            .body
            .print(Shape::unlimited_single_line(), &mut try_body);

        if try_body_info.multi_lined || try_body.len() > line_len_remaining {
            // create a block expression around it
            printer.print_str("{");
            printer.print_newline();
            print_wrapped_arm_body(printer, &self.body, shape.indent);
            printer.print_newline();
            printer.print_spaces(shape.indent);
            printer.print_str("},");
            PrintInfo::default_multi_lined()
        } else {
            printer.append_from_printer(try_body);
            printer.print_str(",");
            PrintInfo::default_single_line()
        }
    }
    fn leftmost_token(&self) -> TextRange {
        self.pattern.leftmost_token()
    }
    fn rightmost_token(&self) -> TextRange {
        if let Some(comma) = &self.comma {
            comma.span()
        } else {
            self.body.rightmost_token()
        }
    }
}

/// Corresponds to a [`SyntaxKind::MATCH_GUARD`] node.
#[derive(Debug)]
pub struct MatchGuard {
    pub keyword: t::If,
    pub condition: Expression,
}

impl FromCST for MatchGuard {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::MATCH_GUARD)?;

        let mut it = SyntaxNodeIter::new(&node);

        let if_token = it.expect_parse()?;

        let condition = it.expect_next("a condition")?;
        let condition = Expression::from_cst(condition)?;

        it.expect_end()?;

        Ok(MatchGuard {
            keyword: if_token,
            condition,
        })
    }
}

impl KnownKind for MatchGuard {
    fn kind() -> SyntaxKind {
        SyntaxKind::MATCH_GUARD
    }
}

impl Printable for MatchGuard {
    fn print(&self, mut shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.keyword);
        printer.print_str(" ");
        shape.width = shape
            .width
            .saturating_sub(usize::from(self.keyword.token_span.len()) + 1);
        shape.first_line_offset += usize::from(self.keyword.token_span.len()) + 1;
        printer.print(&self.condition, shape)
    }
    fn leftmost_token(&self) -> TextRange {
        self.keyword.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.condition.rightmost_token()
    }
}

/// Corresponds to a [`SyntaxKind::CATCH_EXPR`] node.
#[derive(Debug)]
pub struct CatchExpr {
    pub base: Box<Expression>,
    pub clauses: Vec<CatchClause>,
}

impl FromCST for CatchExpr {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::CATCH_EXPR)?;

        let mut it = SyntaxNodeIter::new(&node);
        let base = Box::new(Expression::from_cst(
            it.expect_next("catch base expression")?,
        )?);

        let mut clauses = Vec::new();
        for elem in it {
            if elem.kind() != SyntaxKind::CATCH_CLAUSE {
                return Err(StrongAstError::UnexpectedKindDesc {
                    expected_desc: "CATCH_CLAUSE".into(),
                    found: elem.kind(),
                    at: elem.text_range(),
                });
            }
            clauses.push(CatchClause::from_cst(elem)?);
        }

        Ok(Self { base, clauses })
    }
}

impl KnownKind for CatchExpr {
    fn kind() -> SyntaxKind {
        SyntaxKind::CATCH_EXPR
    }
}

impl Printable for CatchExpr {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let base_info = printer.print(&*self.base, shape.clone());
        for clause in &self.clauses {
            printer.print_str(" ");
            printer.print(clause, shape.clone());
        }
        PrintInfo {
            multi_lined: base_info.multi_lined || !self.clauses.is_empty(),
        }
    }

    fn leftmost_token(&self) -> TextRange {
        self.base.leftmost_token()
    }

    fn rightmost_token(&self) -> TextRange {
        self.clauses
            .last()
            .map_or_else(|| self.base.rightmost_token(), CatchClause::rightmost_token)
    }
}

/// The `catch`, `catch_all`, or `catch_all_panics` keyword that starts a catch clause.
#[derive(Debug)]
pub enum CatchKeyword {
    Catch(t::Catch),
    CatchAll(t::CatchAll),
    CatchAllPanics(t::CatchAllPanics),
}

impl FromCST for CatchKeyword {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        match elem.kind() {
            SyntaxKind::KW_CATCH => t::Catch::from_cst(elem).map(Self::Catch),
            SyntaxKind::KW_CATCH_ALL => t::CatchAll::from_cst(elem).map(Self::CatchAll),
            SyntaxKind::KW_CATCH_ALL_PANICS => {
                t::CatchAllPanics::from_cst(elem).map(Self::CatchAllPanics)
            }
            found => Err(StrongAstError::UnexpectedKindDesc {
                expected_desc: "KW_CATCH, KW_CATCH_ALL, or KW_CATCH_ALL_PANICS".into(),
                found,
                at: elem.text_range(),
            }),
        }
    }
}

impl Token for CatchKeyword {
    fn span(&self) -> TextRange {
        match self {
            CatchKeyword::Catch(keyword) => keyword.span(),
            CatchKeyword::CatchAll(keyword) => keyword.span(),
            CatchKeyword::CatchAllPanics(keyword) => keyword.span(),
        }
    }
}

/// `catch (binding)` and optional stack-trace bindings use small wrapper nodes.
#[derive(Debug)]
pub struct CatchBinding {
    pub name: t::Word,
}

impl CatchBinding {
    fn from_cst_kind(elem: SyntaxElement, kind: SyntaxKind) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, kind)?;

        let mut it = SyntaxNodeIter::new(&node);
        let name = it.expect_parse()?;
        it.expect_end()?;
        Ok(Self { name })
    }
}

impl Printable for CatchBinding {
    fn print(&self, _shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.name);
        PrintInfo::default_single_line()
    }

    fn leftmost_token(&self) -> TextRange {
        self.name.span()
    }

    fn rightmost_token(&self) -> TextRange {
        self.name.span()
    }
}

/// Corresponds to a [`SyntaxKind::CATCH_CLAUSE`] node.
#[derive(Debug)]
pub struct CatchClause {
    pub keyword: CatchKeyword,
    pub open_paren: t::LParen,
    pub binding: CatchBinding,
    pub stack_trace_binding: Option<(t::Comma, CatchBinding)>,
    pub close_paren: t::RParen,
    pub open_brace: t::LBrace,
    pub arms: Vec<ArmListItem<CatchArm>>,
    pub close_brace: t::RBrace,
}

impl FromCST for CatchClause {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::CATCH_CLAUSE)?;

        let mut it = SyntaxNodeIter::new(&node);
        let keyword = CatchKeyword::from_cst(it.expect_next("catch keyword")?)?;
        let open_paren = it.expect_parse()?;
        let binding = CatchBinding::from_cst_kind(
            it.expect_next("catch binding")?,
            SyntaxKind::CATCH_BINDING,
        )?;
        let stack_trace_binding = it
            .next_if_kind(SyntaxKind::COMMA)
            .map(|comma| {
                Ok::<_, StrongAstError>((
                    t::Comma::from_cst(comma)?,
                    CatchBinding::from_cst_kind(
                        it.expect_next("catch stack trace binding")?,
                        SyntaxKind::CATCH_STACK_TRACE_BINDING,
                    )?,
                ))
            })
            .transpose()?;
        let close_paren = it.expect_parse()?;
        let open_brace = it.expect_parse()?;

        let mut arms = Vec::new();
        let close_brace = loop {
            let Some(elem) = it.next() else {
                return Err(StrongAstError::missing(SyntaxKind::R_BRACE, it.parent));
            };
            match elem.kind() {
                SyntaxKind::R_BRACE => break t::RBrace::from_cst(elem)?,
                SyntaxKind::CATCH_ARM => arms.push(ArmListItem::Arm(CatchArm::from_cst(elem)?)),
                SyntaxKind::HEADER_COMMENT => {
                    arms.push(ArmListItem::Header(t::HeaderComment::from_cst(elem)?));
                }
                found => {
                    return Err(StrongAstError::UnexpectedKindDesc {
                        expected_desc: "CATCH_ARM, HEADER_COMMENT, or R_BRACE".into(),
                        found,
                        at: elem.text_range(),
                    });
                }
            }
        };
        it.expect_end()?;

        Ok(Self {
            keyword,
            open_paren,
            binding,
            stack_trace_binding,
            close_paren,
            open_brace,
            arms,
            close_brace,
        })
    }
}

impl KnownKind for CatchClause {
    fn kind() -> SyntaxKind {
        SyntaxKind::CATCH_CLAUSE
    }
}

impl Printable for CatchClause {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let inner_indent = shape.indent + printer.config.indent_width;

        printer.print_raw_token(&self.keyword);
        printer.print_str(" ");
        printer.print_raw_token(&self.open_paren);
        printer.print(&self.binding, Shape::unlimited_single_line());
        if let Some((comma, stack_trace_binding)) = &self.stack_trace_binding {
            printer.print_raw_token(comma);
            printer.print_str(" ");
            printer.print(stack_trace_binding, Shape::unlimited_single_line());
        }
        printer.print_raw_token(&self.close_paren);
        printer.print_str(" ");
        printer.print_raw_token(&self.open_brace);
        printer.print_trivia_all_trailing_for(self.open_brace.span());
        printer.print_newline();

        for arm in &self.arms {
            printer.print_standalone_with_trivia(arm, inner_indent);
            printer.print_newline();
        }

        printer.print_trivia_all_leading_with_newline_for(self.close_brace.span(), inner_indent);
        printer.print_spaces(shape.indent);
        printer.print_raw_token(&self.close_brace);

        PrintInfo::default_multi_lined()
    }

    fn leftmost_token(&self) -> TextRange {
        self.keyword.span()
    }

    fn rightmost_token(&self) -> TextRange {
        self.close_brace.span()
    }
}

/// Corresponds to a [`SyntaxKind::CATCH_ARM`] node.
#[derive(Debug)]
pub struct CatchArm {
    pub pattern: MatchPattern,
    pub fat_arrow: t::FatArrow,
    pub body: Expression,
    pub comma: Option<t::Comma>,
}

impl FromCST for CatchArm {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::CATCH_ARM)?;

        let mut it = SyntaxNodeIter::new(&node);
        let pattern = it.expect_parse()?;
        let fat_arrow = it.expect_parse()?;
        let body = Expression::from_cst(it.expect_next("catch arm body")?)?;
        let comma = it.next().map(t::Comma::from_cst).transpose()?;
        it.expect_end()?;

        Ok(Self {
            pattern,
            fat_arrow,
            body,
            comma,
        })
    }
}

impl KnownKind for CatchArm {
    fn kind() -> SyntaxKind {
        SyntaxKind::CATCH_ARM
    }
}

impl Printable for CatchArm {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print(&self.pattern, shape.clone());
        printer.print_str(" ");
        printer.print_raw_token(&self.fat_arrow);
        printer.print_str(" ");

        let line_len_remaining = printer.current_line_remaining_width();
        if let Expression::Block(block) = &self.body {
            let body_shape = Shape {
                width: line_len_remaining,
                indent: shape.indent,
                first_line_offset: printer
                    .config
                    .line_width
                    .saturating_sub(shape.indent + line_len_remaining),
            };
            let info = printer.print(block, body_shape);
            if self.comma.is_some() {
                printer.print_str(",");
            }
            return info;
        }

        let mut try_body = printer.sub_printer();
        let try_body_info = self
            .body
            .print(Shape::unlimited_single_line(), &mut try_body);

        if try_body_info.multi_lined || try_body.len() > line_len_remaining {
            printer.print_str("{");
            printer.print_newline();
            print_wrapped_arm_body(printer, &self.body, shape.indent);
            printer.print_newline();
            printer.print_spaces(shape.indent);
            printer.print_str("}");
            if self.comma.is_some() {
                printer.print_str(",");
            }
            PrintInfo::default_multi_lined()
        } else {
            printer.append_from_printer(try_body);
            if self.comma.is_some() {
                printer.print_str(",");
            }
            PrintInfo::default_single_line()
        }
    }

    fn leftmost_token(&self) -> TextRange {
        self.pattern.leftmost_token()
    }

    fn rightmost_token(&self) -> TextRange {
        if let Some(comma) = &self.comma {
            comma.span()
        } else {
            self.body.rightmost_token()
        }
    }
}

/// Corresponds to a [`SyntaxKind::CALL_EXPR`] node.
#[derive(Debug)]
pub struct CallExpr {
    pub callee: Box<Expression>,
    pub args: CallArgs,
}

impl FromCST for CallExpr {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::CALL_EXPR)?;

        let mut it = SyntaxNodeIter::new(&node);

        // Callee expression
        let callee_node = it.expect_next("callee expression")?;
        let callee = Box::new(Expression::from_cst(callee_node)?);

        // CALL_ARGS
        let args: CallArgs = it.expect_parse()?;

        Ok(CallExpr { callee, args })
    }
}

impl KnownKind for CallExpr {
    fn kind() -> SyntaxKind {
        SyntaxKind::CALL_EXPR
    }
}

impl CallExpr {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    pub(crate) fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
        let callee = self
            .callee
            .effective_postfix_operand(input.trivia)
            .single_line_width(input)?;
        let args = self.args.single_line_width(input)?;
        Some(callee + args)
    }
}

impl Printable for CallExpr {
    /// The main way to call this should be through [`PrintChain`]
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let mut multi_lined = false;
        let line_len_before = printer.current_line_len();
        let callee = self.callee.effective_postfix_operand(printer.trivia);
        multi_lined |= printer.print(callee, shape.clone()).multi_lined;
        // Account for the callee on the call line so the args' hug layout
        // (see `CallArgs::try_print_hug`) budgets its first line correctly.
        let args_shape = Shape {
            first_line_offset: shape.first_line_offset
                + printer.current_line_len().saturating_sub(line_len_before),
            ..shape
        };
        multi_lined |= printer.print(&self.args, args_shape).multi_lined;
        PrintInfo { multi_lined }
    }
    fn leftmost_token(&self) -> TextRange {
        self.callee.leftmost_token()
    }
    fn rightmost_token(&self) -> TextRange {
        self.args.rightmost_token()
    }
}

/// Corresponds to a [`SyntaxKind::CALL_ARGS`] node.
#[derive(Debug)]
pub struct CallArgs {
    pub open_paren: t::LParen,
    pub args: Vec<(CallArg, Option<t::Comma>)>,
    pub close_paren: t::RParen,
}
impl FromCST for CallArgs {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::CALL_ARGS)?;

        let mut it = SyntaxNodeIter::new(&node);

        let open_paren = it.expect_parse()?;

        let mut args = Vec::new();
        let close_paren = loop {
            let Some(elem) = it.next() else {
                return Err(StrongAstError::missing(SyntaxKind::R_PAREN, it.parent));
            };

            if elem.kind() == SyntaxKind::R_PAREN {
                break t::RParen::from_cst(elem)?;
            }

            let arg = if elem.kind() == SyntaxKind::CALL_ARG {
                CallArg::from_cst(elem)?
            } else {
                CallArg {
                    label: None,
                    expr: Expression::from_cst(elem)?,
                }
            };
            let comma = it
                .next_if_kind(SyntaxKind::COMMA)
                .map(t::Comma::from_cst)
                .transpose()?;
            args.push((arg, comma));
        };

        it.expect_end()?;

        Ok(CallArgs {
            open_paren,
            args,
            close_paren,
        })
    }
}

/// Corresponds to a [`SyntaxKind::CALL_ARG`] node.
#[derive(Debug)]
pub struct CallArg {
    pub label: Option<(t::Word, t::Equals)>,
    pub expr: Expression,
}

impl FromCST for CallArg {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::CALL_ARG)?;

        let children: Vec<_> = node
            .children_with_tokens()
            .filter(|elem| !elem.kind().is_trivia())
            .collect();

        let (label, expr_elem) = if children.len() >= 3
            && matches!(children[0].kind(), SyntaxKind::WORD | SyntaxKind::KW_CLIENT)
            && children[1].kind() == SyntaxKind::EQUALS
        {
            let name = t::Word::new_from_span(children[0].text_range());
            let equals = t::Equals::from_cst(children[1].clone())?;
            (Some((name, equals)), children[2].clone())
        } else {
            let Some(expr_elem) = children.first().cloned() else {
                return Err(StrongAstError::missing_desc(
                    "call argument",
                    node.text_range(),
                ));
            };
            (None, expr_elem)
        };

        let expr = Expression::from_cst(expr_elem)?;

        Ok(CallArg { label, expr })
    }
}

impl CallArg {
    /// A block-terminal argument (a lambda or a `spawn { … }`) that may hug
    /// the call parens instead of forcing the whole call to break: the
    /// argument's block opens on the call line and its `}` is immediately
    /// followed by `)`.
    const fn is_huggable(&self) -> bool {
        matches!(self.expr, Expression::Lambda(_) | Expression::Spawn(_))
    }

    /// The argument expression with redundant parens peeled: the call's own
    /// parens already delimit the argument, so a transparent paren wrapping
    /// the whole expression carries nothing. Lambdas and `spawn` keep their
    /// parens: peeling one would flip [`Self::is_huggable`] between passes
    /// and break idempotency.
    fn effective_expr(&self, trivia: &TriviaInfo) -> &Expression {
        let peeled = self.expr.peel_transparent_parens(trivia);
        if matches!(peeled, Expression::Lambda(_) | Expression::Spawn(_)) {
            &self.expr
        } else {
            peeled
        }
    }

    pub(crate) fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
        let expr = self.effective_expr(input.trivia);
        let mut len = 0;
        if let Some((name, equals)) = &self.label {
            let (_, name_trailing) = input.trivia.get_for_range_split(name.span());
            let (equals_leading, equals_trailing) = input.trivia.get_for_range_split(equals.span());
            let expr_leading = input.trivia.get_leading_for_element(expr);
            len += usize::from(name.span().len())
                + name_trailing.try_squished_len(input.input)?
                + equals_leading.try_squished_len(input.input)?
                + " = ".len()
                + equals_trailing.try_squished_len(input.input)?
                + expr_leading.try_squished_len(input.input)?;
        }
        len += expr.single_line_width(input)?;
        Some(len)
    }
}

impl Printable for CallArg {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let expr = self.effective_expr(printer.trivia);
        if let Some((name, equals)) = &self.label {
            printer.print_raw_token(name);
            let (_, name_trailing) = printer.trivia.get_for_range_split(name.span());
            let (equals_leading, equals_trailing) =
                printer.trivia.get_for_range_split(equals.span());
            let expr_leading = printer.trivia.get_leading_for_element(expr);
            printer.print_trivia_squished(name_trailing);
            printer.print_trivia_squished(equals_leading);
            printer.print_str(" = ");
            printer.print_trivia_squished(equals_trailing);
            printer.print_trivia_squished(expr_leading);
        }
        printer.print(expr, shape)
    }

    fn leftmost_token(&self) -> TextRange {
        self.label
            .as_ref()
            .map_or_else(|| self.expr.leftmost_token(), |(name, _)| name.span())
    }

    fn rightmost_token(&self) -> TextRange {
        self.expr.rightmost_token()
    }
}

impl KnownKind for CallArgs {
    fn kind() -> SyntaxKind {
        SyntaxKind::CALL_ARGS
    }
}

impl PrintMultiLine for CallArgs {
    /// Always multi-lined, even if there are no arguments it would still be `(\n<indent>)`
    fn print_multi_line(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let inner_indent = shape.indent + printer.config.indent_width;
        let inner_shape = Shape {
            width: printer.config.line_width.saturating_sub(inner_indent),
            indent: inner_indent,
            first_line_offset: 0,
        };

        printer.print_raw_token(&self.open_paren);
        printer.print_trivia_all_trailing_for(self.open_paren.span());
        printer.print_newline();

        for (arg, comma) in &self.args {
            printer.print_trivia_all_leading_with_newline_for(
                arg.leftmost_token(),
                inner_shape.indent,
            );
            printer.print_spaces(inner_shape.indent);
            printer.print(arg, inner_shape.clone());
            if let Some(comma) = comma {
                printer.print_raw_token(comma);
                printer.print_trivia_all_trailing_for(comma.span());
            } else {
                printer.print_str(",");
                printer.print_trivia_all_trailing_for(arg.rightmost_token());
            }
            printer.print_newline();
        }

        printer
            .print_trivia_all_leading_with_newline_for(self.close_paren.span(), inner_shape.indent);
        printer.print_spaces(shape.indent);
        printer.print_raw_token(&self.close_paren);

        PrintInfo::default_multi_lined()
    }
}

impl CallArgs {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    pub(crate) fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
        let mut len = const { "()".len() };
        let (_, open_trailing) = input.trivia.get_for_range_split(self.open_paren.span());
        for t in open_trailing {
            len += t.single_line_len(input.input)?;
        }
        for (i, (arg, comma)) in self.args.iter().enumerate() {
            let (arg_leading, arg_trailing) = input.trivia.get_for_element(arg);
            for t in arg_leading {
                len += t.single_line_len(input.input)?;
            }
            len += arg.single_line_width(input)?;
            for t in arg_trailing {
                len += t.single_line_len(input.input)?;
            }
            if i + 1 < self.args.len() {
                if let Some(comma) = comma {
                    let (comma_leading, comma_trailing) =
                        input.trivia.get_for_range_split(comma.span());
                    for t in comma_leading {
                        len += t.single_line_len(input.input)?;
                    }
                    len += 1; // ","
                    for t in comma_trailing {
                        len += t.single_line_len(input.input)?;
                    }
                } else {
                    len += 1; // ","
                }
                len += 1; // " "
            } else if let Some(comma) = comma {
                // Trailing comma is removed in single-line mode, but check trivia
                let (comma_leading, comma_trailing) =
                    input.trivia.get_for_range_split(comma.span());
                for t in comma_leading {
                    len += t.single_line_len(input.input)?;
                }
                for t in comma_trailing {
                    len += t.single_line_len(input.input)?;
                }
            }
        }
        let (close_leading, _) = input.trivia.get_for_range_split(self.close_paren.span());
        for t in close_leading {
            len += t.single_line_len(input.input)?;
        }
        Some(len)
    }

    /// Should be passed a sub-printer to avoid printing trivia in the outer printer
    /// in the event that the printer is unable to fit the call args on a single line.
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        printer.print_raw_token(&self.open_paren);
        let (_, open_trailing) = printer.trivia.get_for_range_split(self.open_paren.span());
        printer.try_print_trivia_single_line_squished(open_trailing)?;

        for (i, (arg, comma)) in self.args.iter().enumerate() {
            if printer.output.len() > shape.width {
                return None;
            }
            let (arg_leading, arg_trailing) = printer.trivia.get_for_element(arg);
            printer.try_print_trivia_single_line_squished(arg_leading)?;
            if printer
                .print(arg, Shape::unlimited_single_line())
                .multi_lined
            {
                return None;
            }
            printer.try_print_trivia_single_line_squished(arg_trailing)?;
            if i + 1 < self.args.len() {
                if let Some(comma) = comma {
                    let (comma_leading, comma_trailing) =
                        printer.trivia.get_for_range_split(comma.span());
                    printer.try_print_trivia_single_line_squished(comma_leading)?;
                    printer.print_raw_token(comma);
                    printer.try_print_trivia_single_line_squished(comma_trailing)?;
                } else {
                    printer.print_str(",");
                }
                printer.print_str(" ");
            } else if let Some(comma) = comma {
                // Trailing comma is removed in single-line mode, but we still try the comments.
                let (comma_leading, comma_trailing) =
                    printer.trivia.get_for_range_split(comma.span());
                printer.try_print_trivia_single_line_squished(comma_leading)?;
                printer.try_print_trivia_single_line_squished(comma_trailing)?;
            }
        }

        let (close_leading, _) = printer.trivia.get_for_range_split(self.close_paren.span());
        printer.try_print_trivia_single_line_squished(close_leading)?;
        printer.print_raw_token(&self.close_paren);

        if printer.output.len() > shape.width {
            None
        } else {
            Some(PrintInfo::default_single_line())
        }
    }

    /// Whether the hug layout (see [`Self::try_print_hug`]) applies: the last
    /// argument is block-terminal (a lambda or `spawn { … }`).
    fn can_hug(&self) -> bool {
        self.args
            .split_last()
            .is_some_and(|((arg, _), _)| arg.is_huggable())
    }

    /// Hug layout for a trailing block-terminal argument: everything up to
    /// the last argument prints on one line, the last argument's block opens
    /// on that same line and closes at the outer indent, immediately followed
    /// by the closing paren (no trailing comma).
    ///
    /// ```baml
    /// futures.push(spawn {
    ///     work(c)
    /// })
    /// ```
    ///
    /// Should be passed a sub-printer to avoid printing trivia in the outer
    /// printer in the event that the hug layout does not apply.
    fn try_print_hug(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        let ((last_arg, last_comma), init) = self.args.split_last()?;
        if !last_arg.is_huggable() {
            return None;
        }

        printer.print_raw_token(&self.open_paren);
        let (_, open_trailing) = printer.trivia.get_for_range_split(self.open_paren.span());
        printer.try_print_trivia_single_line_squished(open_trailing)?;

        for (arg, comma) in init {
            let (arg_leading, arg_trailing) = printer.trivia.get_for_element(arg);
            printer.try_print_trivia_single_line_squished(arg_leading)?;
            if printer
                .print(arg, Shape::unlimited_single_line())
                .multi_lined
            {
                return None;
            }
            printer.try_print_trivia_single_line_squished(arg_trailing)?;
            if let Some(comma) = comma {
                let (comma_leading, comma_trailing) =
                    printer.trivia.get_for_range_split(comma.span());
                printer.try_print_trivia_single_line_squished(comma_leading)?;
                printer.print_raw_token(comma);
                printer.try_print_trivia_single_line_squished(comma_trailing)?;
            } else {
                printer.print_str(",");
            }
            printer.print_str(" ");
        }

        let (last_leading, last_trailing) = printer.trivia.get_for_element(last_arg);
        printer.try_print_trivia_single_line_squished(last_leading)?;
        // The hugged argument's first line continues the call line, so its
        // single-line budget is what remains after the indent, the call's own
        // offset, and everything printed since the open paren.
        let first_line_offset = shape.first_line_offset + printer.current_line_len();
        let arg_shape = Shape {
            width: printer
                .config
                .line_width
                .saturating_sub(shape.indent + first_line_offset),
            indent: shape.indent,
            first_line_offset,
        };
        printer.print(last_arg, arg_shape);
        // The trailing comma is dropped in the hug layout, but keep any
        // comments attached around it.
        printer.try_print_trivia_single_line_squished(last_trailing)?;
        if let Some(comma) = last_comma {
            let (comma_leading, comma_trailing) = printer.trivia.get_for_range_split(comma.span());
            printer.try_print_trivia_single_line_squished(comma_leading)?;
            printer.try_print_trivia_single_line_squished(comma_trailing)?;
        }
        let (close_leading, _) = printer.trivia.get_for_range_split(self.close_paren.span());
        printer.try_print_trivia_single_line_squished(close_leading)?;
        printer.print_raw_token(&self.close_paren);

        Some(PrintInfo::default_multi_lined())
    }
}

impl Printable for CallArgs {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|p| self.try_print_single_line(&shape, p))
            .or_else(|| printer.try_sub_printer(|p| self.try_print_hug(&shape, p)))
            .unwrap_or_else(|| self.print_multi_line(shape, printer))
    }
    fn leftmost_token(&self) -> TextRange {
        self.open_paren.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.close_paren.span()
    }
}

/// Represents the bracket-enclosed portion of an index expression: `[expr]`.
/// Analogous to [`CallArgs`] for call expressions.
/// Used by both [`IndexExpr`] and [`PrintChain`].
#[derive(Debug)]
pub struct IndexArgs<'a> {
    pub open_bracket: &'a t::LBracket,
    pub index: &'a Expression,
    pub close_bracket: &'a t::RBracket,
}

impl IndexArgs<'_> {
    pub(crate) fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
        let mut len = const { "[]".len() };
        len += self.index.single_line_width(input)?;
        let (_, open_trailing) = input.trivia.get_for_range_split(self.open_bracket.span());
        len += open_trailing.try_squished_len(input.input)?;
        let (index_leading, index_trailing) = input.trivia.get_for_element(self.index);
        len += index_leading.try_squished_len(input.input)?;
        len += index_trailing.try_squished_len(input.input)?;
        let (close_leading, _) = input.trivia.get_for_range_split(self.close_bracket.span());
        len += close_leading.try_squished_len(input.input)?;
        Some(len)
    }

    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        printer.print_raw_token(self.open_bracket);
        let (_, open_trailing) = printer.trivia.get_for_range_split(self.open_bracket.span());
        printer.try_print_trivia_single_line_squished(open_trailing)?;

        let (index_leading, index_trailing) = printer.trivia.get_for_element(self.index);
        printer.try_print_trivia_single_line_squished(index_leading)?;
        if printer
            .print(self.index, Shape::unlimited_single_line())
            .multi_lined
        {
            return None;
        }
        printer.try_print_trivia_single_line_squished(index_trailing)?;

        let (close_leading, _) = printer
            .trivia
            .get_for_range_split(self.close_bracket.span());
        printer.try_print_trivia_single_line_squished(close_leading)?;
        printer.print_raw_token(self.close_bracket);

        if printer.output.len() > shape.width {
            None
        } else {
            Some(PrintInfo::default_single_line())
        }
    }
}

impl PrintMultiLine for IndexArgs<'_> {
    fn print_multi_line(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let inner_indent = shape.indent + printer.config.indent_width;
        printer.print_raw_token(self.open_bracket);
        printer.print_trivia_all_trailing_for(self.open_bracket.span());
        printer.print_newline();

        let (index_leading, index_trailing) = printer.trivia.get_for_element(self.index);
        printer.print_trivia_with_newline(index_leading.trim_blanks(), inner_indent);
        printer.print_spaces(inner_indent);
        let inner_shape = Shape::standalone(printer.config.line_width, inner_indent);
        printer.print(self.index, inner_shape);
        printer.print_trivia_trailing(index_trailing);
        printer.print_newline();

        let (close_leading, _) = printer
            .trivia
            .get_for_range_split(self.close_bracket.span());
        printer.print_trivia_with_newline(close_leading.trim_blanks(), inner_indent);
        printer.print_spaces(shape.indent);
        printer.print_raw_token(self.close_bracket);
        PrintInfo::default_multi_lined()
    }
}

impl Printable for IndexArgs<'_> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|p| self.try_print_single_line(&shape, p))
            .unwrap_or_else(|| self.print_multi_line(shape, printer))
    }
    fn leftmost_token(&self) -> TextRange {
        self.open_bracket.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.close_bracket.span()
    }
}

/// Corresponds to a [`SyntaxKind::INDEX_EXPR`] node.
#[derive(Debug)]
pub struct IndexExpr {
    pub base: Box<Expression>,
    pub open_bracket: t::LBracket,
    pub index: Box<Expression>,
    pub close_bracket: t::RBracket,
}

impl FromCST for IndexExpr {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::INDEX_EXPR)?;

        let mut it = SyntaxNodeIter::new(&node);

        // Base expression
        let base_node = it.expect_next("base expression")?;
        let base = Box::new(Expression::from_cst(base_node)?);

        // L_BRACKET
        let open_bracket = it.expect_parse()?;

        // Index expression
        let index_node = it.expect_next("index expression")?;
        let index = Box::new(Expression::from_cst(index_node)?);

        // R_BRACKET
        let close_bracket = it.expect_parse()?;

        it.expect_end()?;

        Ok(IndexExpr {
            base,
            open_bracket,
            index,
            close_bracket,
        })
    }
}

impl KnownKind for IndexExpr {
    fn kind() -> SyntaxKind {
        SyntaxKind::INDEX_EXPR
    }
}

impl IndexExpr {
    fn args(&self) -> IndexArgs<'_> {
        IndexArgs {
            open_bracket: &self.open_bracket,
            index: &self.index,
            close_bracket: &self.close_bracket,
        }
    }

    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    pub(crate) fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
        let base = self
            .base
            .effective_postfix_operand(input.trivia)
            .single_line_width(input)?;
        Some(base + self.args().single_line_width(input)?)
    }
}

impl PrintMultiLine for IndexExpr {
    fn print_multi_line(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let base = self.base.effective_postfix_operand(printer.trivia);
        printer.print(base, shape.clone());
        self.args().print_multi_line(shape, printer)
    }
}

impl IndexExpr {
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        let base = self.base.effective_postfix_operand(printer.trivia);
        let base_len = base.single_line_width(printer)?;
        let args_len = self.args().single_line_width(printer)?;
        if base_len + args_len > shape.width {
            return None;
        }
        if base
            .print(Shape::unlimited_single_line(), printer)
            .multi_lined
        {
            return None;
        }
        if self
            .args()
            .print(Shape::unlimited_single_line(), printer)
            .multi_lined
        {
            return None;
        }
        Some(PrintInfo::default_single_line())
    }
}

impl Printable for IndexExpr {
    /// The main way to call this should be through [`PrintChain`]
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|p| self.try_print_single_line(&shape, p))
            .unwrap_or_else(|| self.print_multi_line(shape, printer))
    }
    fn leftmost_token(&self) -> TextRange {
        self.base.leftmost_token()
    }
    fn rightmost_token(&self) -> TextRange {
        self.close_bracket.span()
    }
}

/// Corresponds to a [`SyntaxKind::FIELD_ACCESS_EXPR`] node.
#[derive(Debug)]
pub struct FieldAccessExpr {
    pub base: Box<Expression>,
    pub dot: t::Dot,
    pub field: t::Word,
}

impl FromCST for FieldAccessExpr {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::FIELD_ACCESS_EXPR)?;

        let mut it = SyntaxNodeIter::new(&node);

        // Base expression
        let base_node = it.expect_next("base expression")?;
        let base = Box::new(Expression::from_cst(base_node)?);

        // DOT
        let dot = it.expect_parse()?;

        // WORD (field name)
        let field = it.expect_parse()?;

        it.expect_end()?;

        Ok(FieldAccessExpr { base, dot, field })
    }
}

impl KnownKind for FieldAccessExpr {
    fn kind() -> SyntaxKind {
        SyntaxKind::FIELD_ACCESS_EXPR
    }
}

impl FieldAccessExpr {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    pub(crate) fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
        let base = self
            .base
            .effective_postfix_operand(input.trivia)
            .single_line_width(input)?;
        Some(base + usize::from(self.dot.span().len()) + usize::from(self.field.span().len()))
    }
}

/// Corresponds to a [`SyntaxKind::OPTIONAL_FIELD_ACCESS_EXPR`] node: `base?.field`.
#[derive(Debug)]
pub struct OptionalFieldAccessExpr {
    pub base: Box<Expression>,
    pub question_dot: t::QuestionDot,
    pub field: t::Word,
}

impl FromCST for OptionalFieldAccessExpr {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::OPTIONAL_FIELD_ACCESS_EXPR)?;

        let mut it = SyntaxNodeIter::new(&node);

        let base_node = it.expect_next("base expression")?;
        let base = Box::new(Expression::from_cst(base_node)?);

        let question_dot = it.expect_parse()?;

        let field = it.expect_parse()?;

        it.expect_end()?;

        Ok(OptionalFieldAccessExpr {
            base,
            question_dot,
            field,
        })
    }
}

impl KnownKind for OptionalFieldAccessExpr {
    fn kind() -> SyntaxKind {
        SyntaxKind::OPTIONAL_FIELD_ACCESS_EXPR
    }
}

impl OptionalFieldAccessExpr {
    pub(crate) fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
        let base = self
            .base
            .effective_postfix_operand(input.trivia)
            .single_line_width(input)?;
        Some(
            base + usize::from(self.question_dot.span().len())
                + usize::from(self.field.span().len()),
        )
    }
}

/// Corresponds to a [`SyntaxKind::OPTIONAL_INDEX_EXPR`] node: `base?.[index]`.
#[derive(Debug)]
pub struct OptionalIndexExpr {
    pub base: Box<Expression>,
    pub question_dot: t::QuestionDot,
    pub open_bracket: t::LBracket,
    pub index: Box<Expression>,
    pub close_bracket: t::RBracket,
}

impl FromCST for OptionalIndexExpr {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::OPTIONAL_INDEX_EXPR)?;

        let mut it = SyntaxNodeIter::new(&node);

        let base_node = it.expect_next("base expression")?;
        let base = Box::new(Expression::from_cst(base_node)?);

        let question_dot = it.expect_parse()?;

        let open_bracket = it.expect_parse()?;

        let index_node = it.expect_next("index expression")?;
        let index = Box::new(Expression::from_cst(index_node)?);

        let close_bracket = it.expect_parse()?;

        it.expect_end()?;

        Ok(OptionalIndexExpr {
            base,
            question_dot,
            open_bracket,
            index,
            close_bracket,
        })
    }
}

impl KnownKind for OptionalIndexExpr {
    fn kind() -> SyntaxKind {
        SyntaxKind::OPTIONAL_INDEX_EXPR
    }
}

impl OptionalIndexExpr {
    fn args(&self) -> IndexArgs<'_> {
        IndexArgs {
            open_bracket: &self.open_bracket,
            index: &self.index,
            close_bracket: &self.close_bracket,
        }
    }

    pub(crate) fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
        let base = self
            .base
            .effective_postfix_operand(input.trivia)
            .single_line_width(input)?;
        Some(
            base + usize::from(self.question_dot.span().len())
                + self.args().single_line_width(input)?,
        )
    }
}

impl Printable for OptionalIndexExpr {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let mut multi_lined = false;
        let base = self.base.effective_postfix_operand(printer.trivia);
        multi_lined |= printer.print(base, shape.clone()).multi_lined;
        printer.print_raw_token(&self.question_dot);
        multi_lined |= printer.print(&self.args(), shape).multi_lined;
        PrintInfo { multi_lined }
    }
    fn leftmost_token(&self) -> TextRange {
        self.base.leftmost_token()
    }
    fn rightmost_token(&self) -> TextRange {
        self.close_bracket.span()
    }
}

/// Corresponds to a [`SyntaxKind::OPTIONAL_CALL_EXPR`] node: `callee?.(args)`.
#[derive(Debug)]
pub struct OptionalCallExpr {
    pub callee: Box<Expression>,
    pub question_dot: t::QuestionDot,
    pub args: CallArgs,
}

impl FromCST for OptionalCallExpr {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::OPTIONAL_CALL_EXPR)?;

        let mut it = SyntaxNodeIter::new(&node);

        let callee_node = it.expect_next("callee expression")?;
        let callee = Box::new(Expression::from_cst(callee_node)?);

        let question_dot = it.expect_parse()?;

        let args: CallArgs = it.expect_parse()?;

        it.expect_end()?;

        Ok(OptionalCallExpr {
            callee,
            question_dot,
            args,
        })
    }
}

impl KnownKind for OptionalCallExpr {
    fn kind() -> SyntaxKind {
        SyntaxKind::OPTIONAL_CALL_EXPR
    }
}

impl OptionalCallExpr {
    pub(crate) fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
        let callee = self
            .callee
            .effective_postfix_operand(input.trivia)
            .single_line_width(input)?;
        let args = self.args.single_line_width(input)?;
        Some(callee + usize::from(self.question_dot.span().len()) + args)
    }
}

impl Printable for OptionalCallExpr {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let mut multi_lined = false;
        let callee = self.callee.effective_postfix_operand(printer.trivia);
        multi_lined |= printer.print(callee, shape.clone()).multi_lined;
        printer.print_raw_token(&self.question_dot);
        multi_lined |= printer.print(&self.args, shape).multi_lined;
        PrintInfo { multi_lined }
    }
    fn leftmost_token(&self) -> TextRange {
        self.callee.leftmost_token()
    }
    fn rightmost_token(&self) -> TextRange {
        self.args.rightmost_token()
    }
}

/// Corresponds to a [`SyntaxKind::ENV_ACCESS_EXPR`] node.
#[derive(Debug)]
pub struct EnvAccessExpr {
    pub keyword: t::Word,
    pub dot: t::Dot,
    pub field: t::Word,
}

impl FromCST for EnvAccessExpr {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::ENV_ACCESS_EXPR)?;

        let mut it = SyntaxNodeIter::new(&node);

        let keyword = it.expect_parse()?;

        let dot = it.expect_parse()?;

        let field = it.expect_parse()?;

        it.expect_end()?;

        Ok(EnvAccessExpr {
            keyword,
            dot,
            field,
        })
    }
}

impl KnownKind for EnvAccessExpr {
    fn kind() -> SyntaxKind {
        SyntaxKind::ENV_ACCESS_EXPR
    }
}

impl EnvAccessExpr {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    #[allow(clippy::unnecessary_wraps)]
    pub(crate) fn single_line_width(&self, _input: &Printer<'_>) -> Option<usize> {
        Some(
            usize::from(self.keyword.span().len())
                + usize::from(self.dot.span().len())
                + usize::from(self.field.span().len()),
        )
    }
}

impl Printable for EnvAccessExpr {
    fn print(&self, _shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.keyword);
        printer.print_raw_token(&self.dot);
        printer.print_raw_token(&self.field);
        PrintInfo::default_single_line()
    }
    fn leftmost_token(&self) -> TextRange {
        self.keyword.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.field.span()
    }
}

/// Corresponds to a [`SyntaxKind::BLOCK_EXPR`] node.
#[derive(Debug)]
pub struct BlockExpr {
    pub open_brace: t::LBrace,
    pub stmts: Vec<Statement>,
    /// Possible tail expression.
    /// If not in a block that can have a tail expression, this should be treated as a normal [`Statement::Expr`].
    pub expr: Option<Box<Expression>>,
    pub close_brace: t::RBrace,
}

impl FromCST for BlockExpr {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::BLOCK_EXPR)?;

        let mut it = SyntaxNodeIter::new(&node);

        let open_brace = it.expect_parse()?;

        // Collect all statements and optional final expression
        let mut stmts = Vec::new();
        let close_brace = loop {
            let Some(elem) = it.next() else {
                return Err(StrongAstError::missing(SyntaxKind::R_BRACE, it.parent));
            };
            if elem.kind() == SyntaxKind::R_BRACE {
                break t::RBrace::from_cst(elem)?;
            }

            let stmt = Statement::from_cst(elem)?;
            if let Some(Statement::Expr(expr)) = stmts.last_mut()
                && expr.semicolon.is_none()
                && let Statement::EmptySemicolon(semi) = stmt
            {
                // Attach semicolon to preceding expression since expressions don't immediately parse semicolons
                expr.semicolon = Some(semi);
                continue;
            }
            stmts.push(stmt);
        };

        // If final statement is a expression without semicolon, extract it as a tail expression
        let expr = match stmts.pop() {
            Some(Statement::Expr(expr)) if expr.semicolon.is_none() => Some(expr.expr),
            Some(stmt) => {
                stmts.push(stmt);
                None
            }
            None => None,
        };

        it.expect_end()?;

        Ok(BlockExpr {
            open_brace,
            stmts,
            expr: expr.map(Box::new),
            close_brace,
        })
    }
}

impl KnownKind for BlockExpr {
    fn kind() -> SyntaxKind {
        SyntaxKind::BLOCK_EXPR
    }
}

impl Printable for BlockExpr {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        // An empty block with no comment trapped inside collapses to `{}`
        // (e.g. an empty match arm `null => {},` or an empty `if` body).
        if self.stmts.is_empty() && self.expr.is_none() {
            let (_, open_trailing) = printer.trivia.get_for_range_split(self.open_brace.span());
            let (close_leading, _) = printer.trivia.get_for_range_split(self.close_brace.span());
            if !open_trailing.iter().any(EmittableTrivia::is_comment)
                && !close_leading.iter().any(EmittableTrivia::is_comment)
            {
                printer.print_raw_token(&self.open_brace);
                printer.print_raw_token(&self.close_brace);
                return PrintInfo::default_single_line();
            }
        }

        printer.print_raw_token(&self.open_brace);
        printer.print_trivia_all_trailing_for(self.open_brace.span());
        printer.print_newline();

        // body statements
        let inner_indent = shape.indent + printer.config.indent_width;
        if let Some((first, rest)) = self.stmts.split_first() {
            let (first_leading, first_trailing) = printer.trivia.get_for_element(first);
            printer.print_trivia_with_newline(first_leading.trim_leading_blanks(), inner_indent);
            printer.print_spaces(inner_indent);
            let inner_shape = Shape::standalone(printer.config.line_width, inner_indent);
            printer.print(first, inner_shape);
            printer.print_trivia_trailing(first_trailing);
            printer.print_newline();

            for stmt in rest {
                printer.print_standalone_with_trivia(stmt, inner_indent);
                printer.print_newline();
            }
        }

        // tail expression
        if let Some(expr) = self.expr.as_deref() {
            let (expr_leading, expr_trailing) = printer.trivia.get_for_element(expr);
            let expr_leading = if self.stmts.is_empty() {
                expr_leading.trim_leading_blanks()
            } else {
                expr_leading
            };
            printer.print_trivia_with_newline(expr_leading, inner_indent);
            printer.print_spaces(inner_indent);
            let inner_shape = Shape::standalone(printer.config.line_width, inner_indent);
            printer.print(expr, inner_shape);
            printer.print_trivia_trailing(expr_trailing);
            printer.print_newline();
        }

        let (close_brace_leading, _) = printer.trivia.get_for_range_split(self.close_brace.span());
        printer.print_trivia_with_newline(close_brace_leading.trim_trailing_blanks(), inner_indent);
        printer.print_spaces(shape.indent);
        printer.print_raw_token(&self.close_brace);

        PrintInfo { multi_lined: true }
    }
    fn leftmost_token(&self) -> TextRange {
        self.open_brace.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.close_brace.span()
    }
}

/// Corresponds to a [`SyntaxKind::ARRAY_LITERAL`] node.
#[derive(Debug)]
pub struct ArrayInitializer {
    pub open_bracket: t::LBracket,
    /// Commas are optional for all elements.
    /// For example, `[1 2 3]` is equivalent to `[1, 2, 3]` in BAML.
    ///
    /// While this is valid, excluding commas is *strongly* discouraged as it is a crime against software and also more error-prone:
    /// if `[1, -2, 3]` is written as `[1 -2 3]`, it will be parsed as `[1-2, 3]` instead (the `-` will be treated as a binary operator instead of a unary operator).
    pub elements: Vec<(Expression, Option<t::Comma>)>,
    pub close_bracket: t::RBracket,
}

impl FromCST for ArrayInitializer {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::ARRAY_LITERAL)?;

        let mut it = SyntaxNodeIter::new(&node);

        let open_bracket = it.expect_parse()?;

        let mut elements: Vec<(Expression, Option<t::Comma>)> = Vec::new();

        let close_bracket = loop {
            let Some(elem) = it.next() else {
                return Err(StrongAstError::missing(SyntaxKind::R_BRACKET, it.parent));
            };

            if elem.kind() == SyntaxKind::R_BRACKET {
                break t::RBracket::from_cst(elem)?;
            }

            let expr = Expression::from_cst(elem)?;
            let comma = it
                .next_if_kind(SyntaxKind::COMMA)
                .map(t::Comma::from_cst)
                .transpose()?;

            elements.push((expr, comma));
        };

        Ok(ArrayInitializer {
            open_bracket,
            elements,
            close_bracket,
        })
    }
}

impl KnownKind for ArrayInitializer {
    fn kind() -> SyntaxKind {
        SyntaxKind::ARRAY_LITERAL
    }
}

impl PrintMultiLine for ArrayInitializer {
    /// Multi-line layout: each element on its own indented line with trailing comma.
    /// Closing bracket on its own line.
    ///
    /// ```baml
    /// [
    ///     element1,
    ///     element2,
    ///     element3,
    /// ]
    /// ```
    fn print_multi_line(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.open_bracket);
        printer.print_trivia_all_trailing_for(self.open_bracket.span());
        printer.print_newline();

        let inner_indent = shape.indent + printer.config.indent_width;
        for (elem, comma) in &self.elements {
            let (elem_leading, elem_trailing) = printer.trivia.get_for_element(elem);
            printer.print_trivia_with_newline(elem_leading.trim_blanks(), inner_indent);
            printer.print_spaces(inner_indent);
            let inner_shape = Shape::standalone(printer.config.line_width, inner_indent);
            printer.print(elem, inner_shape);
            if let Some(comma) = comma {
                printer.print_trivia_squished(elem_trailing);
                printer.print_raw_token(comma);
                printer.print_trivia_all_trailing_for(comma.span());
            } else {
                printer.print_str(",");
                printer.print_trivia_trailing(elem_trailing);
            }
            printer.print_newline();
        }

        let (close_bracket_leading, _) = printer
            .trivia
            .get_for_range_split(self.close_bracket.span());
        printer
            .print_trivia_with_newline(close_bracket_leading.trim_trailing_blanks(), inner_indent);
        printer.print_spaces(shape.indent);
        printer.print_raw_token(&self.close_bracket);
        PrintInfo::default_multi_lined()
    }
}

impl ArrayInitializer {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    pub(crate) fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
        let mut len = const { "[".len() };
        let (_, open_trailing) = input.trivia.get_for_range_split(self.open_bracket.span());
        len += open_trailing.try_squished_len(input.input)?;

        for (i, (elem, comma)) in self.elements.iter().enumerate() {
            let (el_leading, el_trailing) = input.trivia.get_for_element(elem);

            len += el_leading.try_squished_len(input.input)?;
            len += elem.single_line_width(input)?;

            let is_last = i + 1 >= self.elements.len();
            if let Some(comma) = comma {
                let (comma_leading, comma_trailing) =
                    input.trivia.get_for_range_split(comma.span());
                len += el_trailing.squished_len(input.input); // always squished before the comma
                len += comma_leading.squished_len(input.input); // always squished before the comma
                if !is_last {
                    len += const { ", ".len() };
                }
                len += comma_trailing.try_squished_len(input.input)?;
            } else {
                len += el_trailing.try_squished_len(input.input)?; // if multilined would go after the added comma
                if !is_last {
                    len += const { ", ".len() };
                }
            }
        }

        let (close_leading, _) = input.trivia.get_for_range_split(self.close_bracket.span());
        len += close_leading.try_squished_len(input.input)?;
        len += const { "]".len() };
        Some(len)
    }

    /// Tries to print the array initializer as a single line.
    ///
    /// If successful, returns the info.
    ///
    /// Should be passed a sub-printer to avoid printing trivia in the outer printer
    /// in the event that the printer is unable to fit the array initializer on a single line.
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        printer.print_raw_token(&self.open_bracket);
        let (_, open_trailing) = printer.trivia.get_for_range_split(self.open_bracket.span());
        printer.try_print_trivia_single_line_squished(open_trailing)?;

        for (i, (elem, comma)) in self.elements.iter().enumerate() {
            if printer.output.len() > shape.width {
                return None;
            }

            let (el_leading, el_trailing) = printer.trivia.get_for_element(elem);
            printer.try_print_trivia_single_line_squished(el_leading)?;
            if printer
                .print(elem, Shape::unlimited_single_line())
                .multi_lined
            {
                return None;
            }

            let is_last = i + 1 >= self.elements.len();
            if let Some(comma) = comma {
                let (comma_leading, comma_trailing) =
                    printer.trivia.get_for_range_split(comma.span());
                printer.print_trivia_squished(el_trailing); // always squished before the comma
                printer.print_trivia_squished(comma_leading); // always squished before the comma
                if !is_last {
                    printer.print_str(", ");
                }
                printer.try_print_trivia_single_line_squished(comma_trailing)?;
            } else {
                printer.try_print_trivia_single_line_squished(el_trailing)?; // if multilined would go after the added comma and thus would not be squished
                if !is_last {
                    printer.print_str(", ");
                }
            }
        }

        let (close_leading, _) = printer
            .trivia
            .get_for_range_split(self.close_bracket.span());
        printer.try_print_trivia_single_line_squished(close_leading)?;
        printer.print_raw_token(&self.close_bracket);

        if printer.output.len() > shape.width {
            None
        } else {
            Some(PrintInfo::default_single_line())
        }
    }
}

impl Printable for ArrayInitializer {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|p| self.try_print_single_line(&shape, p))
            .unwrap_or_else(|| self.print_multi_line(shape, printer))
    }
    fn leftmost_token(&self) -> TextRange {
        self.open_bracket.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.close_bracket.span()
    }
}

/// Corresponds to a [`SyntaxKind::OBJECT_LITERAL`] node.
#[derive(Debug)]
pub struct ObjectInitializer {
    pub name: PathExpr,
    pub open_brace: t::LBrace,
    /// Fields and `...spread` members, in source order. Order is significant:
    /// later members win at runtime, so it must be preserved verbatim.
    pub fields: Vec<(ObjectMember, Option<t::Comma>)>,
    pub close_brace: t::RBrace,
}

impl FromCST for ObjectInitializer {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::OBJECT_LITERAL)?;

        let mut it = SyntaxNodeIter::new(&node);

        // WORD (object type name)
        let name = it.expect_next("a WORD or PATH_EXPR")?;
        let name = PathExpr::from_cst(name)?;

        let open_brace = it.expect_parse()?;

        let mut fields = Vec::new();
        let close_brace = loop {
            let Some(elem) = it.next() else {
                return Err(StrongAstError::missing(SyntaxKind::R_BRACE, it.parent));
            };
            match elem.kind() {
                SyntaxKind::R_BRACE => {
                    break t::RBrace::from_cst(elem)?;
                }
                SyntaxKind::OBJECT_FIELD | SyntaxKind::SPREAD_ELEMENT => {
                    let field = ObjectMember::from_cst(elem)?;
                    let comma = it
                        .next_if_kind(SyntaxKind::COMMA)
                        .map(t::Comma::from_cst)
                        .transpose()?;
                    fields.push((field, comma));
                }
                _ => {
                    return Err(StrongAstError::UnexpectedKindDesc {
                        expected_desc: "OBJECT_FIELD, SPREAD_ELEMENT, or R_BRACE".into(),
                        found: elem.kind(),
                        at: elem.text_range(),
                    });
                }
            }
        };

        it.expect_end()?;

        Ok(ObjectInitializer {
            name,
            open_brace,
            fields,
            close_brace,
        })
    }
}

impl KnownKind for ObjectInitializer {
    fn kind() -> SyntaxKind {
        SyntaxKind::OBJECT_LITERAL
    }
}

impl PrintMultiLine for ObjectInitializer {
    /// Multi-line layout: each field on its own indented line with trailing comma.
    /// Closing brace on its own line.
    ///
    ///
    /// ```baml
    /// Name {
    ///     field1: value1,
    ///     field2: value2,
    /// }
    /// ```
    fn print_multi_line(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let inner_shape = Shape {
            width: shape.width.saturating_sub(printer.config.indent_width),
            indent: shape.indent + printer.config.indent_width,
            first_line_offset: 0,
        };

        printer.print(&self.name, Shape::unlimited_single_line());
        printer.print_str(" ");
        printer.print_raw_token(&self.open_brace);
        printer.print_trivia_all_trailing_for(self.open_brace.span());
        printer.print_newline();

        for (field, comma) in &self.fields {
            printer.print_trivia_all_leading_with_newline_for(
                field.leftmost_token(),
                inner_shape.indent,
            );
            printer.print_spaces(inner_shape.indent);
            printer.print(field, inner_shape.clone());
            if let Some(comma) = comma {
                printer.print_raw_token(comma);
                printer.print_trivia_all_trailing_for(comma.span());
            } else {
                printer.print_str(",");
                printer.print_trivia_all_trailing_for(field.rightmost_token());
            }
            printer.print_newline();
        }

        printer.print_spaces(shape.indent);
        printer.print_trivia_all_leading_with_newline_for(self.close_brace.span(), shape.indent);
        printer.print_raw_token(&self.close_brace);
        PrintInfo::default_multi_lined()
    }
}

impl ObjectInitializer {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    pub(crate) fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
        // Name { field1: v1, field2: v2 }
        let mut len = self.name.single_line_width(input)? + const { " {  }".len() };
        let (_, open_trailing) = input.trivia.get_for_range_split(self.open_brace.span());
        len += open_trailing.try_squished_len(input.input)?;
        for (i, (field, comma)) in self.fields.iter().enumerate() {
            let (fld_leading, fld_trailing) = input.trivia.get_for_element(field);
            len += fld_leading.try_squished_len(input.input)?;
            len += field.single_line_width(input)?;
            len += fld_trailing.try_squished_len(input.input)?;
            if i + 1 < self.fields.len() {
                if let Some(comma) = comma {
                    let (comma_leading, comma_trailing) =
                        input.trivia.get_for_range_split(comma.span());
                    len += comma_leading.try_squished_len(input.input)?;
                    len += 1; // ","
                    len += comma_trailing.try_squished_len(input.input)?;
                } else {
                    len += 1; // ","
                }
                len += 1; // " "
            } else if let Some(comma) = comma {
                // Trailing comma is removed in single-line mode, but check trivia
                let (comma_leading, comma_trailing) =
                    input.trivia.get_for_range_split(comma.span());
                len += comma_leading.try_squished_len(input.input)?;
                len += comma_trailing.try_squished_len(input.input)?;
            }
        }
        let (close_leading, _) = input.trivia.get_for_range_split(self.close_brace.span());
        len += close_leading.try_squished_len(input.input)?;
        Some(len)
    }

    /// Tries to print the object initializer as a single line.
    ///
    /// If successful, returns the info.
    ///
    /// Should be passed a sub-printer to avoid printing trivia in the outer printer
    /// in the event that the printer is unable to fit the object initializer on a single line.
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        printer.print(&self.name, Shape::unlimited_single_line());
        printer.print_str(" ");
        printer.print_raw_token(&self.open_brace);
        printer.print_str(" ");
        let (_, open_trailing) = printer.trivia.get_for_range_split(self.open_brace.span());
        printer.try_print_trivia_single_line_squished(open_trailing)?;

        for (i, (field, comma)) in self.fields.iter().enumerate() {
            if printer.output.len() > shape.width {
                return None;
            }
            let (fld_leading, fld_trailing) = printer.trivia.get_for_element(field);
            printer.try_print_trivia_single_line_squished(fld_leading)?;
            if printer
                .print(field, Shape::unlimited_single_line())
                .multi_lined
            {
                return None;
            }
            printer.try_print_trivia_single_line_squished(fld_trailing)?;
            if i + 1 < self.fields.len() {
                if let Some(comma) = comma {
                    let (comma_leading, comma_trailing) =
                        printer.trivia.get_for_range_split(comma.span());
                    printer.try_print_trivia_single_line_squished(comma_leading)?;
                    printer.print_raw_token(comma);
                    printer.try_print_trivia_single_line_squished(comma_trailing)?;
                } else {
                    printer.print_str(",");
                }
                printer.print_str(" ");
            } else if let Some(comma) = comma {
                // Trailing comma is removed in single-line mode, but we still try the comments.
                let (comma_leading, comma_trailing) =
                    printer.trivia.get_for_range_split(comma.span());
                printer.try_print_trivia_single_line_squished(comma_leading)?;
                printer.try_print_trivia_single_line_squished(comma_trailing)?;
            }
        }
        let (close_leading, _) = printer.trivia.get_for_range_split(self.close_brace.span());
        printer.try_print_trivia_single_line_squished(close_leading)?;
        printer.print_str(" ");
        printer.print_raw_token(&self.close_brace);

        if printer.output.len() > shape.width {
            None
        } else {
            Some(PrintInfo::default_single_line())
        }
    }
}

impl Printable for ObjectInitializer {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|p| self.try_print_single_line(&shape, p))
            .unwrap_or_else(|| self.print_multi_line(shape, printer))
    }
    fn leftmost_token(&self) -> TextRange {
        self.name.leftmost_token()
    }
    fn rightmost_token(&self) -> TextRange {
        self.close_brace.span()
    }
}

/// Corresponds to a [`SyntaxKind::MAP_LITERAL`] node.
#[derive(Debug)]
pub struct MapLiteral {
    pub open_brace: t::LBrace,
    pub fields: Vec<(ObjectField, Option<t::Comma>)>,
    pub close_brace: t::RBrace,
}

impl FromCST for MapLiteral {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::MAP_LITERAL)?;

        let mut it = SyntaxNodeIter::new(&node);

        let open_brace = it.expect_parse()?;

        let mut fields = Vec::new();
        let close_brace = loop {
            let Some(elem) = it.next() else {
                return Err(StrongAstError::missing(SyntaxKind::R_BRACE, it.parent));
            };
            match elem.kind() {
                SyntaxKind::R_BRACE => {
                    break t::RBrace::from_cst(elem)?;
                }
                SyntaxKind::OBJECT_FIELD => {
                    let field = ObjectField::from_cst(elem)?;
                    let comma = it
                        .next_if_kind(SyntaxKind::COMMA)
                        .map(t::Comma::from_cst)
                        .transpose()?;
                    fields.push((field, comma));
                }
                _ => {
                    return Err(StrongAstError::UnexpectedKindDesc {
                        expected_desc: "OBJECT_FIELD or R_BRACE".into(),
                        found: elem.kind(),
                        at: elem.text_range(),
                    });
                }
            }
        };

        it.expect_end()?;

        Ok(MapLiteral {
            open_brace,
            fields,
            close_brace,
        })
    }
}

impl KnownKind for MapLiteral {
    fn kind() -> SyntaxKind {
        SyntaxKind::MAP_LITERAL
    }
}

impl PrintMultiLine for MapLiteral {
    /// Multi-line layout: each entry on its own indented line with trailing comma.
    /// Closing brace on its own line.
    ///
    /// ```baml
    /// {
    ///     key1: value1,
    ///     key2: value2,
    /// }
    /// ```
    fn print_multi_line(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let inner_shape = Shape {
            width: shape.width.saturating_sub(printer.config.indent_width),
            indent: shape.indent + printer.config.indent_width,
            first_line_offset: 0,
        };

        printer.print_raw_token(&self.open_brace);
        printer.print_trivia_all_trailing_for(self.open_brace.span());
        printer.print_newline();

        for (field, comma) in &self.fields {
            printer.print_trivia_all_leading_with_newline_for(
                field.leftmost_token(),
                inner_shape.indent,
            );
            printer.print_spaces(inner_shape.indent);
            printer.print(field, inner_shape.clone());
            if let Some(comma) = comma {
                printer.print_raw_token(comma);
                printer.print_trivia_all_trailing_for(comma.span());
            } else {
                printer.print_str(",");
                printer.print_trivia_all_trailing_for(field.rightmost_token());
            }
            printer.print_newline();
        }

        printer
            .print_trivia_all_leading_with_newline_for(self.close_brace.span(), inner_shape.indent);
        printer.print_spaces(shape.indent);
        printer.print_raw_token(&self.close_brace);
        PrintInfo::default_multi_lined()
    }
}

impl MapLiteral {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    pub(crate) fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
        let (_, open_trailing) = input.trivia.get_for_range_split(self.open_brace.span());
        let (close_leading, _) = input.trivia.get_for_range_split(self.close_brace.span());
        // A populated map carries two interior padding spaces (`{ k1: v1 }`);
        // an empty map is just `{}`. Keep this in sync with `try_print_single_line`.
        let has_content = !self.fields.is_empty()
            || open_trailing.iter().any(EmittableTrivia::is_comment)
            || close_leading.iter().any(EmittableTrivia::is_comment);
        let mut len = if has_content {
            const { "{  }".len() }
        } else {
            const { "{}".len() }
        };
        for t in open_trailing {
            len += t.single_line_len(input.input)?;
        }
        for (i, (field, comma)) in self.fields.iter().enumerate() {
            let (fld_leading, fld_trailing) = input.trivia.get_for_element(field);
            for t in fld_leading {
                len += t.single_line_len(input.input)?;
            }
            len += field.single_line_width(input)?;
            for t in fld_trailing {
                len += t.single_line_len(input.input)?;
            }
            if i + 1 < self.fields.len() {
                if let Some(comma) = comma {
                    let (comma_leading, comma_trailing) =
                        input.trivia.get_for_range_split(comma.span());
                    for t in comma_leading {
                        len += t.single_line_len(input.input)?;
                    }
                    len += 1; // ","
                    for t in comma_trailing {
                        len += t.single_line_len(input.input)?;
                    }
                } else {
                    len += 1; // ","
                }
                len += 1; // " "
            } else if let Some(comma) = comma {
                // Trailing comma is removed in single-line mode, but check trivia
                let (comma_leading, comma_trailing) =
                    input.trivia.get_for_range_split(comma.span());
                for t in comma_leading {
                    len += t.single_line_len(input.input)?;
                }
                for t in comma_trailing {
                    len += t.single_line_len(input.input)?;
                }
            }
        }
        for t in close_leading {
            len += t.single_line_len(input.input)?;
        }
        Some(len)
    }

    /// Should be passed a sub-printer to avoid printing trivia in the outer printer
    /// in the event that the printer is unable to fit the map literal on a single line.
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        let (_, open_trailing) = printer.trivia.get_for_range_split(self.open_brace.span());
        let (close_leading, _) = printer.trivia.get_for_range_split(self.close_brace.span());
        // An empty map renders as `{}` with no interior padding. The padding
        // spaces are only added when there is something to surround: fields or
        // an interior comment (the only trivia that prints on a single line).
        let has_content = !self.fields.is_empty()
            || open_trailing.iter().any(EmittableTrivia::is_comment)
            || close_leading.iter().any(EmittableTrivia::is_comment);

        printer.print_raw_token(&self.open_brace);
        if has_content {
            printer.print_str(" ");
        }
        printer.try_print_trivia_single_line_squished(open_trailing)?;

        for (i, (field, comma)) in self.fields.iter().enumerate() {
            if printer.output.len() > shape.width {
                return None;
            }
            let (fld_leading, fld_trailing) = printer.trivia.get_for_element(field);
            printer.try_print_trivia_single_line_squished(fld_leading)?;
            if printer
                .print(field, Shape::unlimited_single_line())
                .multi_lined
            {
                return None;
            }
            printer.try_print_trivia_single_line_squished(fld_trailing)?;
            if i + 1 < self.fields.len() {
                if let Some(comma) = comma {
                    let (comma_leading, comma_trailing) =
                        printer.trivia.get_for_range_split(comma.span());
                    printer.try_print_trivia_single_line_squished(comma_leading)?;
                    printer.print_raw_token(comma);
                    printer.try_print_trivia_single_line_squished(comma_trailing)?;
                } else {
                    printer.print_str(",");
                }
                printer.print_str(" ");
            } else if let Some(comma) = comma {
                // Trailing comma is removed in single-line mode, but we still try the comments.
                let (comma_leading, comma_trailing) =
                    printer.trivia.get_for_range_split(comma.span());
                printer.try_print_trivia_single_line_squished(comma_leading)?;
                printer.try_print_trivia_single_line_squished(comma_trailing)?;
            }
        }
        printer.try_print_trivia_single_line_squished(close_leading)?;
        if has_content {
            printer.print_str(" ");
        }
        printer.print_raw_token(&self.close_brace);

        if printer.output.len() > shape.width {
            None
        } else {
            Some(PrintInfo::default_single_line())
        }
    }
}

impl Printable for MapLiteral {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|p| self.try_print_single_line(&shape, p))
            .unwrap_or_else(|| self.print_multi_line(shape, printer))
    }
    fn leftmost_token(&self) -> TextRange {
        self.open_brace.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.close_brace.span()
    }
}

/// Corresponds to a [`SyntaxKind::OBJECT_FIELD`] node.
#[derive(Debug)]
pub struct ObjectField {
    pub name: ObjectFieldKey,
    /// Absent for property shorthand (`{ options }`). The parser only permits
    /// shorthand for a bare identifier, never for a quoted or qualified key.
    pub colon: Option<t::Colon>,
    pub value: Option<Expression>,
}

impl FromCST for ObjectField {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::OBJECT_FIELD)?;

        let mut it = SyntaxNodeIter::new(&node);

        let name = it.expect_next("WORD or STRING_LITERAL")?;
        let name = ObjectFieldKey::from_cst(name)?;

        let colon = it
            .next_if_kind(SyntaxKind::COLON)
            .map(t::Colon::from_cst)
            .transpose()?;

        let value = if colon.is_some() {
            let value = it.expect_next("value")?;
            Some(Expression::from_cst(value)?)
        } else {
            None
        };

        it.expect_end()?;

        Ok(ObjectField { name, colon, value })
    }
}

impl KnownKind for ObjectField {
    fn kind() -> SyntaxKind {
        SyntaxKind::OBJECT_FIELD
    }
}

impl ObjectField {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    pub(crate) fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
        let name = self.name.single_line_width(input)?;
        let (Some(colon), Some(value)) = (&self.colon, &self.value) else {
            return Some(name);
        };
        let value_width = value.single_line_width(input)?;
        // Must match trivia handled by print: colon_trailing + value_leading
        let mut trivia_len = 0usize;
        let (_, colon_trailing) = input.trivia.get_for_range_split(colon.span());
        for t in colon_trailing {
            trivia_len += t.single_line_len(input.input)?;
        }
        let value_leading = input.trivia.get_leading_for_element(value);
        for t in value_leading {
            trivia_len += t.single_line_len(input.input)?;
        }
        Some(name + const { ": ".len() } + value_width + trivia_len)
    }
}

impl Printable for ObjectField {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let mut multi_lined = false;
        multi_lined |= printer.print(&self.name, shape.clone()).multi_lined;
        let (Some(colon), Some(value)) = (&self.colon, &self.value) else {
            return PrintInfo { multi_lined };
        };
        printer.print_raw_token(colon);
        let (_, colon_trailing) = printer.trivia.get_for_range_split(colon.span());
        printer.print_str(" ");
        printer.print_trivia_squished(colon_trailing);
        let value_leading = printer.trivia.get_leading_for_element(value);
        printer.print_trivia_squished(value_leading);
        multi_lined |= printer.print(value, shape).multi_lined;
        PrintInfo { multi_lined }
    }
    fn leftmost_token(&self) -> TextRange {
        self.name.leftmost_token()
    }
    fn rightmost_token(&self) -> TextRange {
        self.value
            .as_ref()
            .map(Printable::rightmost_token)
            .unwrap_or_else(|| self.name.rightmost_token())
    }
}

/// A member of an [`ObjectInitializer`]: either a `name: value` field or a
/// `...expr` spread element.
///
/// Only [`SyntaxKind::OBJECT_LITERAL`] admits spreads; map literals and array
/// literals keep using [`ObjectField`] directly.
#[derive(Debug)]
pub enum ObjectMember {
    Field(ObjectField),
    Spread(SpreadElement),
}

impl FromCST for ObjectMember {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        match elem.kind() {
            SyntaxKind::OBJECT_FIELD => Ok(ObjectMember::Field(ObjectField::from_cst(elem)?)),
            SyntaxKind::SPREAD_ELEMENT => Ok(ObjectMember::Spread(SpreadElement::from_cst(elem)?)),
            _ => Err(StrongAstError::UnexpectedKindDesc {
                expected_desc: "OBJECT_FIELD or SPREAD_ELEMENT".into(),
                found: elem.kind(),
                at: elem.text_range(),
            }),
        }
    }
}

impl ObjectMember {
    /// Returns the width of the member if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    pub(crate) fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
        match self {
            ObjectMember::Field(field) => field.single_line_width(input),
            ObjectMember::Spread(spread) => spread.single_line_width(input),
        }
    }
}

impl Printable for ObjectMember {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            ObjectMember::Field(field) => field.print(shape, printer),
            ObjectMember::Spread(spread) => spread.print(shape, printer),
        }
    }
    fn leftmost_token(&self) -> TextRange {
        match self {
            ObjectMember::Field(field) => field.leftmost_token(),
            ObjectMember::Spread(spread) => spread.leftmost_token(),
        }
    }
    fn rightmost_token(&self) -> TextRange {
        match self {
            ObjectMember::Field(field) => field.rightmost_token(),
            ObjectMember::Spread(spread) => spread.rightmost_token(),
        }
    }
}

/// Corresponds to a [`SyntaxKind::SPREAD_ELEMENT`] node.
///
/// Struct-update spread inside an object literal: `Type { ...base, field: v }`.
#[derive(Debug)]
pub struct SpreadElement {
    pub dot_dot_dot: t::DotDotDot,
    pub value: Expression,
}

impl FromCST for SpreadElement {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::SPREAD_ELEMENT)?;

        let mut it = SyntaxNodeIter::new(&node);

        let dot_dot_dot = it.expect_parse()?;
        let value = it.expect_next("spread value")?;
        let value = Expression::from_cst(value)?;

        it.expect_end()?;

        Ok(SpreadElement { dot_dot_dot, value })
    }
}

impl KnownKind for SpreadElement {
    fn kind() -> SyntaxKind {
        SyntaxKind::SPREAD_ELEMENT
    }
}

impl SpreadElement {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    pub(crate) fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
        // Must match the trivia handled by `print`: dots_trailing + value_leading.
        let mut trivia_len = 0usize;
        let (_, dots_trailing) = input.trivia.get_for_range_split(self.dot_dot_dot.span());
        for t in dots_trailing {
            trivia_len += t.single_line_len(input.input)?;
        }
        let value_leading = input.trivia.get_leading_for_element(&self.value);
        for t in value_leading {
            trivia_len += t.single_line_len(input.input)?;
        }
        let value_width = self.value.single_line_width(input)?;
        Some(const { "...".len() } + value_width + trivia_len)
    }
}

impl Printable for SpreadElement {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        // No space after `...` — it binds tightly to its operand.
        printer.print_raw_token(&self.dot_dot_dot);
        let (_, dots_trailing) = printer.trivia.get_for_range_split(self.dot_dot_dot.span());
        printer.print_trivia_squished(dots_trailing);
        let value_leading = printer.trivia.get_leading_for_element(&self.value);
        printer.print_trivia_squished(value_leading);
        printer.print(&self.value, shape)
    }
    fn leftmost_token(&self) -> TextRange {
        self.dot_dot_dot.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.value.rightmost_token()
    }
}

/// Represents the a valid key for an [`ObjectField`].
#[derive(Debug)]
pub enum ObjectFieldKey {
    Word(t::Word),
    String(t::QuotedString),
}

impl FromCST for ObjectFieldKey {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        match elem.kind() {
            // `client` (KW_CLIENT) is a keyword but a valid field name, e.g.
            // `Agent { client: ... }` — mirror `parse_object_field`.
            kind if t::is_word_like(kind) => Ok(ObjectFieldKey::Word(t::Word::from_cst(elem)?)),
            SyntaxKind::STRING_LITERAL => {
                Ok(ObjectFieldKey::String(t::QuotedString::from_cst(elem)?))
            }
            _ => Err(StrongAstError::UnexpectedKindDesc {
                expected_desc: "WORD or STRING_LITERAL".into(),
                found: elem.kind(),
                at: elem.text_range(),
            }),
        }
    }
}

impl ObjectFieldKey {
    /// Returns the width of the expression if it fits on a single line.
    /// Returns `None` if it can never be single-lined.
    pub(crate) fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
        match self {
            ObjectFieldKey::Word(word) => Some(usize::from(word.span().len())),
            ObjectFieldKey::String(s) => {
                if input.input[s.span()].contains('\n') {
                    None
                } else {
                    Some(usize::from(s.span().len()))
                }
            }
        }
    }
}

impl Printable for ObjectFieldKey {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            ObjectFieldKey::Word(word) => {
                printer.print_raw_token(word);
                PrintInfo::default_single_line()
            }
            ObjectFieldKey::String(string) => printer.print(string, shape),
        }
    }
    fn leftmost_token(&self) -> TextRange {
        match self {
            ObjectFieldKey::Word(word) => word.span(),
            ObjectFieldKey::String(string) => string.leftmost_token(),
        }
    }
    fn rightmost_token(&self) -> TextRange {
        match self {
            ObjectFieldKey::Word(word) => word.span(),
            ObjectFieldKey::String(string) => string.rightmost_token(),
        }
    }
}

// ─── Lambda Expression ────────────────────────────────────────────────────────

/// Corresponds to a [`SyntaxKind::GENERIC_PARAM_LIST`] node.
///
/// Contains `<T, U>` generic parameter declarations for a lambda expression.
/// Printed as `<T>` or `<K, V>` etc.
#[derive(Debug)]
pub struct GenericParamList {
    pub open_angle: t::Less,
    /// Comma-separated type parameter declarations.
    pub params: Vec<GenericParam>,
    pub close_angle: t::Greater,
}

#[derive(Debug)]
pub struct GenericParam {
    pub name: t::Word,
    pub bounds: Option<GenericParamBounds>,
    pub comma: Option<t::Comma>,
}

#[derive(Debug)]
pub struct GenericParamBounds {
    pub extends: t::Extends,
    pub bounds: Vec<(Type, Option<t::And>)>,
}

impl FromCST for GenericParamList {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::GENERIC_PARAM_LIST)?;

        let mut it = SyntaxNodeIter::new(&node);

        let open_angle: t::Less = it.expect_parse()?;

        let mut params = Vec::new();
        let close_angle = loop {
            let Some(elem) = it.next() else {
                return Err(StrongAstError::missing(SyntaxKind::GREATER, it.parent));
            };
            match elem.kind() {
                SyntaxKind::GREATER => {
                    break t::Greater::from_cst(elem)?;
                }
                SyntaxKind::GENERIC_PARAM => {
                    let param_node = StrongAstError::assert_is_node(elem)?;
                    let mut param_it = SyntaxNodeIter::new(&param_node);
                    let name: t::Word = param_it.expect_parse()?;
                    let bounds = if param_it.peek().map(SyntaxElement::kind)
                        == Some(SyntaxKind::GENERIC_PARAM_BOUNDS)
                    {
                        let elem = param_it.next().expect("peeked");
                        Some(GenericParamBounds::from_cst(elem)?)
                    } else {
                        None
                    };
                    param_it.expect_end()?;
                    let comma = it
                        .next_if_kind(SyntaxKind::COMMA)
                        .map(t::Comma::from_cst)
                        .transpose()?;
                    params.push(GenericParam {
                        name,
                        bounds,
                        comma,
                    });
                }
                _ => {
                    return Err(StrongAstError::UnexpectedAdditionalElement {
                        parent: it.parent,
                        at: elem.text_range(),
                    });
                }
            }
        };

        it.expect_end()?;

        Ok(GenericParamList {
            open_angle,
            params,
            close_angle,
        })
    }
}

impl FromCST for GenericParamBounds {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::GENERIC_PARAM_BOUNDS)?;

        let mut it = SyntaxNodeIter::new(&node);
        let extends: t::Extends = it.expect_parse()?;
        let mut bounds = Vec::new();
        while it.peek().is_some() {
            let ty: Type = it.expect_parse()?;
            let and = it
                .next_if_kind(SyntaxKind::AND)
                .map(t::And::from_cst)
                .transpose()?;
            bounds.push((ty, and));
        }
        it.expect_end()?;

        Ok(GenericParamBounds { extends, bounds })
    }
}

impl KnownKind for GenericParamList {
    fn kind() -> SyntaxKind {
        SyntaxKind::GENERIC_PARAM_LIST
    }
}

impl Printable for GenericParamList {
    fn print(&self, _shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.open_angle);
        for (i, param) in self.params.iter().enumerate() {
            printer.print_raw_token(&param.name);
            if let Some(bounds) = &param.bounds {
                printer.print_str(" ");
                printer.print(bounds, Shape::unlimited_single_line());
            }
            if i + 1 < self.params.len() {
                printer.print_str(", ");
            }
        }
        printer.print_raw_token(&self.close_angle);
        PrintInfo::default_single_line()
    }
    fn leftmost_token(&self) -> TextRange {
        self.open_angle.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.close_angle.span()
    }
}

impl Printable for GenericParamBounds {
    fn print(&self, _shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.extends);
        for (idx, (bound, _and)) in self.bounds.iter().enumerate() {
            if idx == 0 {
                printer.print_str(" ");
            } else {
                printer.print_str(" & ");
            }
            printer.print(bound, Shape::unlimited_single_line());
        }
        PrintInfo::default_single_line()
    }
    fn leftmost_token(&self) -> TextRange {
        self.extends.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.bounds
            .last()
            .map(|(bound, _)| bound.rightmost_token())
            .unwrap_or_else(|| self.extends.span())
    }
}

/// Corresponds to a [`SyntaxKind::GENERIC_ARGS`] node.
///
/// Contains `<Type1, Type2, ...>` generic arguments at a call site
/// or generic-typed path (e.g. `f<int, string>(...)`, `Box<int> { ... }`).
#[derive(Debug)]
pub struct GenericArgs {
    pub open_angle: t::Less,
    /// Comma-separated static or contextual runtime type arguments.
    pub args: Vec<(GenericArg, Option<t::Comma>)>,
    pub close_angle: t::Greater,
}

#[derive(Debug)]
pub enum GenericArg {
    Type(crate::ast::Type),
}

impl FromCST for GenericArgs {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::GENERIC_ARGS)?;

        let mut it = SyntaxNodeIter::new(&node);

        let open_angle: t::Less = it.expect_parse()?;

        let mut args = Vec::new();
        let close_angle = loop {
            let Some(elem) = it.next() else {
                return Err(StrongAstError::missing(SyntaxKind::GREATER, it.parent));
            };
            match elem.kind() {
                SyntaxKind::GREATER => {
                    break t::Greater::from_cst(elem)?;
                }
                SyntaxKind::TYPE_EXPR => {
                    let arg = GenericArg::Type(crate::ast::Type::from_cst(elem)?);
                    let comma = it
                        .next_if_kind(SyntaxKind::COMMA)
                        .map(t::Comma::from_cst)
                        .transpose()?;
                    args.push((arg, comma));
                }
                _ => {
                    return Err(StrongAstError::UnexpectedAdditionalElement {
                        parent: it.parent,
                        at: elem.text_range(),
                    });
                }
            }
        };

        it.expect_end()?;

        Ok(GenericArgs {
            open_angle,
            args,
            close_angle,
        })
    }
}

impl GenericArgs {
    /// Width that the formatter would emit on a single line, ignoring any
    /// internal trivia in the source. Used by single-line-width estimators
    /// upstream to decide whether a host expression fits on one line.
    ///
    /// Format is `<T1, T2, T3>`: 2 chars for `<>`, plus each type argument's
    /// source-text width, plus `, ` (2 chars) between arguments. Source
    /// types may contain whitespace, but for typical cases this is a tight
    /// upper bound and tracks what the printer actually emits.
    pub(crate) fn formatted_single_line_width(&self) -> usize {
        let mut len: usize = 2; // `<` and `>`
        for (i, (arg, _)) in self.args.iter().enumerate() {
            let (left, right) = match arg {
                GenericArg::Type(ty) => (ty.leftmost_token(), ty.rightmost_token()),
            };
            let arg_span = right.end() - left.start();
            len += usize::from(arg_span);
            if i + 1 < self.args.len() {
                len += 2; // `, `
            }
        }
        len
    }
}

impl KnownKind for GenericArgs {
    fn kind() -> SyntaxKind {
        SyntaxKind::GENERIC_ARGS
    }
}

impl Printable for GenericArgs {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.open_angle);
        for (i, (arg, _comma)) in self.args.iter().enumerate() {
            match arg {
                GenericArg::Type(ty) => printer.print(ty, shape.clone()),
            };
            if i + 1 < self.args.len() {
                printer.print_str(", ");
            }
        }
        printer.print_raw_token(&self.close_angle);
        PrintInfo::default_single_line()
    }
    fn leftmost_token(&self) -> TextRange {
        self.open_angle.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.close_angle.span()
    }
}

/// Corresponds to a [`SyntaxKind::THROWS_CLAUSE`] node.
///
/// Contains `throws <type>`.
#[derive(Debug)]
pub struct ThrowsClause {
    pub keyword: t::Throws,
    pub ty: crate::ast::Type,
}

impl FromCST for ThrowsClause {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::THROWS_CLAUSE)?;

        let mut it = SyntaxNodeIter::new(&node);
        let keyword: t::Throws = it.expect_parse()?;
        let ty: crate::ast::Type = it.expect_parse()?;
        it.expect_end()?;

        Ok(ThrowsClause { keyword, ty })
    }
}

impl KnownKind for ThrowsClause {
    fn kind() -> SyntaxKind {
        SyntaxKind::THROWS_CLAUSE
    }
}

impl Printable for ThrowsClause {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let mut multi_lined = false;
        printer.print_raw_token(&self.keyword);
        printer.print_str(" ");
        multi_lined |= printer.print(&self.ty, shape).multi_lined;
        PrintInfo { multi_lined }
    }
    fn leftmost_token(&self) -> TextRange {
        self.keyword.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.ty.rightmost_token()
    }
}

/// Arrow token in a function signature. Accepts either `->` (canonical) or
/// `=>` (accepted permissively for ergonomic parity with JS/TS arrow functions);
/// the formatter always emits `->`. Shared by declarations and lambdas so the
/// compiler's permissive syntax and formatter repair stay in lockstep.
#[derive(Debug)]
pub enum FunctionArrow {
    Arrow(t::Arrow),
    FatArrow(t::FatArrow),
}

impl FunctionArrow {
    #[must_use]
    pub fn span(&self) -> TextRange {
        match self {
            FunctionArrow::Arrow(t) => t.span(),
            FunctionArrow::FatArrow(t) => t.span(),
        }
    }

    /// Returns true if the source used `=>` instead of the canonical `->`.
    #[must_use]
    pub fn is_fat_arrow(&self) -> bool {
        matches!(self, FunctionArrow::FatArrow(_))
    }

    /// Print trivia between the source arrow and the next canonical element.
    /// The arrow spelling may be synthesized, but its source span still owns
    /// comments that must survive normalization.
    pub(crate) fn print_separator_before(
        &self,
        next_leftmost: Option<TextRange>,
        continuation_indent: usize,
        printer: &mut Printer,
    ) {
        let (_, arrow_trailing) = printer.trivia.get_for_range_split(self.span());
        let next_leading = next_leftmost
            .map(|range| printer.trivia.get_for_range_split(range).0)
            .unwrap_or(&[]);
        let mut printed_comment = false;
        let mut continued_on_newline = false;

        for trivia in arrow_trailing.iter().chain(next_leading) {
            if !trivia.is_comment() {
                continue;
            }
            if !continued_on_newline {
                printer.print_spaces(1);
            }
            printer.print_trivia(trivia);
            printed_comment = true;
            continued_on_newline = trivia.single_line_len(printer.input).is_none();
            if continued_on_newline {
                printer.print_newline();
                printer.print_spaces(continuation_indent);
            }
        }

        if !printed_comment || !continued_on_newline {
            printer.print_spaces(1);
        }
    }
}

impl FromCST for FunctionArrow {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let token = StrongAstError::assert_is_token(elem)?;
        match token.kind() {
            SyntaxKind::ARROW => Ok(FunctionArrow::Arrow(t::Arrow::new_from_span(
                token.text_range(),
            ))),
            SyntaxKind::FAT_ARROW => Ok(FunctionArrow::FatArrow(t::FatArrow::new_from_span(
                token.text_range(),
            ))),
            _ => Err(StrongAstError::UnexpectedKindDesc {
                expected_desc: "ARROW or FAT_ARROW".into(),
                found: token.kind(),
                at: token.text_range(),
            }),
        }
    }
}

impl KnownKind for FunctionArrow {
    fn kind() -> SyntaxKind {
        // Primary/canonical kind; `from_cst` also accepts FAT_ARROW.
        SyntaxKind::ARROW
    }
}

/// Corresponds to a [`SyntaxKind::LAMBDA_EXPR`] node.
///
/// Syntax: `[<T, U>] (params) (-> | =>) [RetType] [throws E] { body }`
#[derive(Debug)]
pub struct LambdaExpr {
    pub generic_params: Option<GenericParamList>,
    pub param_list: super::FunctionParamList,
    pub arrow: FunctionArrow,
    pub return_type: Option<crate::ast::Type>,
    pub throws: Option<ThrowsClause>,
    pub block: BlockExpr,
}

#[allow(clippy::redundant_closure_for_method_calls)]
impl FromCST for LambdaExpr {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::LAMBDA_EXPR)?;

        let mut it = SyntaxNodeIter::new(&node);

        // Optional generic params: <T, U>
        let generic_params = if it.peek().map(|e| e.kind()) == Some(SyntaxKind::GENERIC_PARAM_LIST)
        {
            let elem = it.next().expect("peeked");
            Some(GenericParamList::from_cst(elem)?)
        } else {
            None
        };

        // Parameter list: (x: int, y: string) or ()
        let param_list: super::FunctionParamList = it.expect_parse()?;

        // Arrow: `->` or `=>` (formatter normalizes to `->`)
        let arrow: FunctionArrow = it.expect_parse()?;

        // Optional return type: TYPE_EXPR before THROWS_CLAUSE or BLOCK_EXPR
        let return_type = if it.peek().map(|e| e.kind()) == Some(SyntaxKind::TYPE_EXPR) {
            let elem = it.next().expect("peeked");
            Some(crate::ast::Type::from_cst(elem)?)
        } else {
            None
        };

        // Optional throws clause
        let throws = if it.peek().map(|e| e.kind()) == Some(SyntaxKind::THROWS_CLAUSE) {
            let elem = it.next().expect("peeked");
            Some(ThrowsClause::from_cst(elem)?)
        } else {
            None
        };

        // Block body
        let block: BlockExpr = it.expect_parse()?;

        it.expect_end()?;

        Ok(LambdaExpr {
            generic_params,
            param_list,
            arrow,
            return_type,
            throws,
            block,
        })
    }
}

impl KnownKind for LambdaExpr {
    fn kind() -> SyntaxKind {
        SyntaxKind::LAMBDA_EXPR
    }
}

impl Printable for LambdaExpr {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        // Optional generic params: <T>
        if let Some(ref gp) = self.generic_params {
            printer.print(gp, shape.clone());
        }

        // Parameter list
        printer.print(&self.param_list, shape.clone());

        // Space + arrow (always normalize to canonical `->`)
        printer.print_str(" ->");

        // Optional return type
        if let Some(ref ret) = self.return_type {
            self.arrow.print_separator_before(
                Some(ret.leftmost_token()),
                shape.indent + printer.config.indent_width,
                printer,
            );
            printer.print(ret, shape.clone());
            if let Some(ref throws) = self.throws {
                printer.print_str(" ");
                printer.print(throws, shape.clone());
            }
            printer.print_str(" ");
        } else if let Some(ref throws) = self.throws {
            self.arrow.print_separator_before(
                Some(throws.leftmost_token()),
                shape.indent + printer.config.indent_width,
                printer,
            );
            printer.print(throws, shape.clone());
            printer.print_str(" ");
        } else {
            self.arrow.print_separator_before(
                Some(self.block.leftmost_token()),
                shape.indent + printer.config.indent_width,
                printer,
            );
        }

        printer.print(&self.block, shape);

        PrintInfo::default_multi_lined()
    }
    fn leftmost_token(&self) -> TextRange {
        if let Some(ref gp) = self.generic_params {
            gp.leftmost_token()
        } else {
            self.param_list.leftmost_token()
        }
    }
    fn rightmost_token(&self) -> TextRange {
        self.block.rightmost_token()
    }
}

/// The `with` options clause of a [`SpawnExpr`]: the keyword and its
/// comma-separated expressions (in v1 a single `baml.spawn.options(...)`
/// call).
pub type SpawnWithClause = (t::With, Vec<(Expression, Option<t::Comma>)>);

/// Corresponds to a [`SyntaxKind::SPAWN_EXPR`] node.
///
/// `spawn name_expr? (with expr (, expr)*)? { body }` (BEP-034). The name
/// expression and the `with` options clause are both optional; the body is
/// always a brace-delimited block.
#[derive(Debug)]
pub struct SpawnExpr {
    pub keyword: t::Spawn,
    /// Optional task-name expression between `spawn` and `with`/the body.
    pub name: Option<Expression>,
    pub with_clause: Option<SpawnWithClause>,
    pub block: BlockExpr,
}

impl FromCST for SpawnExpr {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::SPAWN_EXPR)?;

        let mut it = SyntaxNodeIter::new(&node);
        let keyword: t::Spawn = it.expect_parse()?;

        let mut name = None;
        let mut with_clause = None;
        let block = loop {
            let elem = it.expect_next("spawn body block")?;
            match elem.kind() {
                SyntaxKind::BLOCK_EXPR => break BlockExpr::from_cst(elem)?,
                SyntaxKind::KW_WITH => {
                    let with_kw = t::With::from_cst(elem)?;
                    let mut options = Vec::new();
                    while let Some(next) = it.peek() {
                        if next.kind() == SyntaxKind::BLOCK_EXPR {
                            break;
                        }
                        let expr = Expression::from_cst(it.next().expect("peeked"))?;
                        let comma = it
                            .next_if_kind(SyntaxKind::COMMA)
                            .map(t::Comma::from_cst)
                            .transpose()?;
                        options.push((expr, comma));
                    }
                    with_clause = Some((with_kw, options));
                }
                _ if name.is_none() && with_clause.is_none() => {
                    name = Some(Expression::from_cst(elem)?);
                }
                _ => {
                    return Err(StrongAstError::UnexpectedAdditionalElement {
                        parent: it.parent,
                        at: elem.text_range(),
                    });
                }
            }
        };
        it.expect_end()?;

        Ok(SpawnExpr {
            keyword,
            name,
            with_clause,
            block,
        })
    }
}

impl KnownKind for SpawnExpr {
    fn kind() -> SyntaxKind {
        SyntaxKind::SPAWN_EXPR
    }
}

impl SpawnExpr {
    /// Source range for the spawn header, excluding the body block's opening
    /// brace. Keeping a commented header verbatim avoids dropping trivia that
    /// sits between the keyword, optional name, `with` options, and commas.
    fn header_range(&self) -> TextRange {
        TextRange::new(
            self.keyword.span().start(),
            self.block.open_brace.span().start(),
        )
    }

    /// Header comments are deliberately kept verbatim. The structured header
    /// layout canonicalizes whitespace and commas, but does not otherwise have
    /// enough information to place a line comment without changing its line.
    /// The trivia classifier catches both line and block comments here.
    fn header_requires_verbatim(&self, input: &Printer<'_>) -> bool {
        let header_start = self.keyword.span().start();
        let block_start = self.block.open_brace.span().start();
        input.trivia.all_trivia().iter().any(|trivia| {
            let attached_at = trivia.attached_to().start();
            trivia.is_comment() && attached_at >= header_start && attached_at <= block_start
        })
    }

    /// Width of the header (`spawn`, optional name, optional `with` clause)
    /// without the body block. `None` if any part can never be single-lined.
    fn header_single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
        if self.header_requires_verbatim(input) {
            return None;
        }
        let mut len = usize::from(self.keyword.span().len());
        if let Some(name) = &self.name {
            len += const { " ".len() } + name.single_line_width(input)?;
        }
        if let Some((with_kw, options)) = &self.with_clause {
            len += const { " ".len() } + usize::from(with_kw.span().len());
            for (i, (expr, _)) in options.iter().enumerate() {
                len += if i == 0 {
                    const { " ".len() }
                } else {
                    const { ", ".len() }
                };
                len += expr.single_line_width(input)?;
            }
        }
        Some(len)
    }

    /// Returns the width of the expression if it fits on a single line —
    /// a simple body (`{}` or `{ tail }`) and a single-lineable header.
    /// Returns `None` if it can never be single-lined.
    pub(crate) fn single_line_width(&self, input: &Printer<'_>) -> Option<usize> {
        if !self.block.stmts.is_empty() {
            return None;
        }
        let header = self.header_single_line_width(input)?;
        let (_, open_trailing) = input
            .trivia
            .get_for_range_split(self.block.open_brace.span());
        let (close_leading, _) = input
            .trivia
            .get_for_range_split(self.block.close_brace.span());
        let body = match self.block.expr.as_deref() {
            Some(tail) => {
                let (tail_leading, tail_trailing) = input.trivia.get_for_element(tail);
                (const { " {  }".len() })
                    + open_trailing.try_squished_len(input.input)?
                    + tail_leading.try_squished_len(input.input)?
                    + tail.single_line_width(input)?
                    + tail_trailing.try_squished_len(input.input)?
                    + close_leading.try_squished_len(input.input)?
            }
            None => {
                if open_trailing.iter().any(EmittableTrivia::is_comment)
                    || close_leading.iter().any(EmittableTrivia::is_comment)
                {
                    return None;
                }
                const { " {}".len() }
            }
        };
        Some(header + body)
    }

    /// Prints the header: `spawn`, then the optional name and `with` clause.
    /// Returns whether any part spilled onto multiple lines.
    fn print_header(&self, shape: &Shape, printer: &mut Printer) -> bool {
        let mut multi_lined = false;
        printer.print_raw_token(&self.keyword);
        if let Some(name) = &self.name {
            printer.print_str(" ");
            multi_lined |= printer.print(name, shape.clone()).multi_lined;
        }
        if let Some((with_kw, options)) = &self.with_clause {
            printer.print_str(" ");
            printer.print_raw_token(with_kw);
            for (i, (expr, _)) in options.iter().enumerate() {
                printer.print_str(if i == 0 { " " } else { ", " });
                multi_lined |= printer.print(expr, shape.clone()).multi_lined;
            }
        }
        multi_lined
    }

    /// Single-line layout: `spawn name? (with opts)? {}` or `… { tail }`.
    /// Only possible when the body has no statements.
    ///
    /// Should be passed a sub-printer to avoid printing trivia in the outer
    /// printer in the event that the expression cannot fit on a single line.
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        if !self.block.stmts.is_empty() || self.header_requires_verbatim(printer) {
            return None;
        }
        if self.print_header(&Shape::unlimited_single_line(), printer) {
            return None;
        }
        printer.print_str(" ");

        let (_, open_trailing) = printer
            .trivia
            .get_for_range_split(self.block.open_brace.span());
        let (close_leading, _) = printer
            .trivia
            .get_for_range_split(self.block.close_brace.span());
        match self.block.expr.as_deref() {
            Some(tail) => {
                printer.print_raw_token(&self.block.open_brace);
                printer.print_str(" ");
                printer.try_print_trivia_single_line_squished(open_trailing)?;
                let (tail_leading, tail_trailing) = printer.trivia.get_for_element(tail);
                printer.try_print_trivia_single_line_squished(tail_leading)?;
                if printer
                    .print(tail, Shape::unlimited_single_line())
                    .multi_lined
                {
                    return None;
                }
                printer.try_print_trivia_single_line_squished(tail_trailing)?;
                printer.try_print_trivia_single_line_squished(close_leading)?;
                printer.print_str(" ");
                printer.print_raw_token(&self.block.close_brace);
            }
            None => {
                if open_trailing.iter().any(EmittableTrivia::is_comment)
                    || close_leading.iter().any(EmittableTrivia::is_comment)
                {
                    return None;
                }
                printer.print_raw_token(&self.block.open_brace);
                printer.print_raw_token(&self.block.close_brace);
            }
        }

        if printer.output.len() > shape.width {
            None
        } else {
            Some(PrintInfo::default_single_line())
        }
    }
}

impl PrintMultiLine for SpawnExpr {
    /// Multi-line layout: the header stays on the current line and the block
    /// opens right after it, closing at the outer indent.
    ///
    /// ```baml
    /// spawn with baml.spawn.options(group = g) {
    ///     compute()
    /// }
    /// ```
    fn print_multi_line(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        if self.header_requires_verbatim(printer) {
            printer.print_input_range(self.header_range());
        } else {
            self.print_header(&shape, printer);
            printer.print_str(" ");
        }
        printer.print(&self.block, shape);
        PrintInfo::default_multi_lined()
    }
}

impl Printable for SpawnExpr {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|p| self.try_print_single_line(&shape, p))
            .unwrap_or_else(|| self.print_multi_line(shape, printer))
    }
    fn leftmost_token(&self) -> TextRange {
        self.keyword.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.block.rightmost_token()
    }
}

// ─── PrintChain ───────────────────────────────────────────────────────────────

/// Only used for printing chained expressions.
///
/// Needed to re-organize before printing from a hierarchical structure to a flat-ish one.
pub struct PrintChain<'a> {
    /// May be a [`PathExpr`] in which case only the first item is used (the rest are included in [`PrintChain::chain_members`]).
    first: &'a Expression,
    /// Will always start with a field access (if not empty), since calls/indexes will be included in `first` if not following a field access.
    chain_members: Vec<PrintChainItem<'a>>,
}
impl<'a> PrintChain<'a> {
    /// Builds the flat chain for a postfix spine.
    ///
    /// Every receiver is taken through `Expression::effective_postfix_operand`
    /// so redundant parens around it peel and the walk continues through them.
    /// A paren that survives (looser-binding receiver, or one carrying a
    /// comment) still terminates the walk and becomes `first`, which is what
    /// puts it on its own indent level.
    #[must_use]
    pub fn new(from: &'a Expression, trivia: &TriviaInfo) -> Self {
        let from = from.effective_postfix_operand(trivia);
        match from {
            Expression::Path(path_expr) => {
                let mut chain_members: Vec<PrintChainItem<'a>> = path_expr
                    .rest
                    .iter()
                    .map(|(dot, word)| PrintChainItem::FieldAccess(dot, word))
                    .collect();
                if let Some(ref ga) = path_expr.generic_args {
                    chain_members.push(PrintChainItem::GenericArgs(ga));
                }
                Self {
                    first: from,
                    chain_members,
                }
            }
            Expression::Call(call_expr) => {
                let mut chain = Self::new(&call_expr.callee, trivia);
                if chain.chain_members.is_empty() {
                    // included in `first` if not following a field access
                    Self {
                        first: from,
                        chain_members: Vec::new(),
                    }
                } else {
                    chain
                        .chain_members
                        .push(PrintChainItem::Call(&call_expr.args));
                    chain
                }
            }
            Expression::Index(index_expr) => {
                let mut chain = Self::new(&index_expr.base, trivia);
                if chain.chain_members.is_empty() {
                    // included in `first` if not following a field access
                    Self {
                        first: from,
                        chain_members: Vec::new(),
                    }
                } else {
                    chain
                        .chain_members
                        .push(PrintChainItem::Index(index_expr.args()));
                    chain
                }
            }
            Expression::FieldAccess(field_access_expr) => {
                let mut chain = Self::new(&field_access_expr.base, trivia);
                chain.chain_members.push(PrintChainItem::FieldAccess(
                    &field_access_expr.dot,
                    &field_access_expr.field,
                ));
                chain
            }
            Expression::OptionalFieldAccess(ofa) => {
                let mut chain = Self::new(&ofa.base, trivia);
                chain
                    .chain_members
                    .push(PrintChainItem::OptionalFieldAccess(
                        &ofa.question_dot,
                        &ofa.field,
                    ));
                chain
            }
            Expression::OptionalIndex(oi) => {
                let mut chain = Self::new(&oi.base, trivia);
                chain
                    .chain_members
                    .push(PrintChainItem::OptionalIndex(&oi.question_dot, oi.args()));
                chain
            }
            Expression::OptionalCall(oc) => {
                let mut chain = Self::new(&oc.callee, trivia);
                chain
                    .chain_members
                    .push(PrintChainItem::OptionalCall(&oc.question_dot, &oc.args));
                chain
            }
            base => Self {
                first: base,
                chain_members: Vec::new(),
            },
        }
    }
}

impl PrintMultiLine for PrintChain<'_> {
    /// Prints the chained expression broken at method-call boundaries,
    /// prettier/rustfmt style.
    ///
    /// Plain member accesses (namespace segments, field accesses, generic
    /// type segments) are atomic with their receiver and never split, no
    /// matter how long the path is. Break points sit before the `.name` of
    /// each call group; the first call group stays glued to the receiver
    /// line when it fits:
    ///
    /// ```baml
    /// root.ai.Agent<Itinerary>.new()
    ///     .with_client(client)
    ///     .run(spec)
    /// ```
    fn print_multi_line(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let first_single_line = match self.first {
            Expression::Path(path_expr) => {
                printer.print_raw_token(&path_expr.first);
                true
            }
            // Call/Index print directly: routing them through
            // `Expression::print` would rebuild this same chain and recurse.
            Expression::Call(call_expr) => {
                let first_info = printer.print(call_expr, shape.clone());
                !first_info.multi_lined
            }
            Expression::Index(index_expr) => {
                let first_info = printer.print(index_expr, shape.clone());
                !first_info.multi_lined
            }
            _ => {
                let first_info = printer.print(self.first, shape.clone());
                !first_info.multi_lined
            }
        };
        let mut multi_lined = !first_single_line;

        let chain_indent = shape.indent + printer.config.indent_width;
        let mut line_remaining_width = printer.current_line_remaining_width();
        let mut rest: &[PrintChainItem<'_>] = &self.chain_members;

        // A call/index applied directly to the receiver (`base?.(x).field`)
        // cannot break away from it: glue it to the receiver's line.
        while let Some((item, tail)) = rest.split_first() {
            if Self::is_plain_access(item) {
                break;
            }
            multi_lined |=
                Self::print_non_field_item(item, chain_indent, &mut line_remaining_width, printer);
            rest = tail;
        }

        // The leading run of plain accesses is the namespace path; it is
        // atomic with the receiver and always stays on its line. When a call
        // follows, the final access of the run is that call's method name and
        // belongs to the call's group instead (`.new` stays glued to `()`).
        let plain_run_len = rest
            .iter()
            .take_while(|item| Self::is_plain_access(item))
            .count();
        let path_len = if plain_run_len == rest.len() {
            plain_run_len
        } else {
            rest[..plain_run_len]
                .iter()
                .rposition(|item| {
                    matches!(
                        item,
                        PrintChainItem::FieldAccess(..) | PrintChainItem::OptionalFieldAccess(..)
                    )
                })
                .unwrap_or(plain_run_len)
        };
        for item in &rest[..path_len] {
            Self::print_plain_item(item, printer);
        }
        rest = &rest[path_len..];
        line_remaining_width = printer.current_line_remaining_width();

        // Split the remaining items into groups: each group is a run of
        // plain accesses (the method name) followed by its calls/indexes.
        let mut is_first_group = true;
        while !rest.is_empty() {
            let group_plain = rest
                .iter()
                .take_while(|item| Self::is_plain_access(item))
                .count();
            let group_callish = rest[group_plain..]
                .iter()
                .take_while(|item| !Self::is_plain_access(item))
                .count();
            let (group, tail) = rest.split_at(group_plain + group_callish);
            rest = tail;

            // A group can only start with a call/index when the path had no
            // field access to serve as its name; such a group cannot move to
            // its own line. Otherwise, the first call group stays glued to
            // the receiver line when it fits; later groups always break.
            let glue = if group_plain == 0 {
                true
            } else if is_first_group && first_single_line {
                Self::group_single_line_width(group, printer)
                    .is_some_and(|width| width <= line_remaining_width)
            } else {
                false
            };
            if !glue {
                printer.print_newline();
                printer.print_spaces(chain_indent);
                line_remaining_width = printer.config.line_width.saturating_sub(chain_indent);
                multi_lined = true;
            }
            for item in group {
                if Self::is_plain_access(item) {
                    Self::print_plain_item(item, printer);
                    line_remaining_width = printer.current_line_remaining_width();
                } else {
                    multi_lined |= Self::print_non_field_item(
                        item,
                        chain_indent,
                        &mut line_remaining_width,
                        printer,
                    );
                }
            }
            is_first_group = false;
        }

        PrintInfo { multi_lined }
    }
}

impl PrintChain<'_> {
    /// Plain (non-call) chain items: member accesses and generic type
    /// segments. These are atomic with their receiver and never move to
    /// their own line.
    const fn is_plain_access(item: &PrintChainItem<'_>) -> bool {
        matches!(
            item,
            PrintChainItem::FieldAccess(..)
                | PrintChainItem::OptionalFieldAccess(..)
                | PrintChainItem::GenericArgs(..)
        )
    }

    /// Prints a plain access glued to whatever precedes it on the line.
    fn print_plain_item(item: &PrintChainItem<'_>, printer: &mut Printer) {
        match *item {
            PrintChainItem::FieldAccess(dot, word) => {
                printer.print_raw_token(dot);
                printer.print_raw_token(word);
            }
            PrintChainItem::OptionalFieldAccess(qd, word) => {
                printer.print_raw_token(qd);
                printer.print_raw_token(word);
            }
            PrintChainItem::GenericArgs(generic_args) => {
                printer.print(generic_args, Shape::unlimited_single_line());
            }
            _ => unreachable!("print_plain_item called with a call/index item"),
        }
    }

    /// Returns the single-line width of one chain item, or `None` if it can
    /// never be single-lined.
    fn item_single_line_width(item: &PrintChainItem<'_>, printer: &Printer<'_>) -> Option<usize> {
        match item {
            PrintChainItem::FieldAccess(dot, word) => {
                Some(usize::from(dot.span().len() + word.span().len()))
            }
            PrintChainItem::OptionalFieldAccess(qd, word) => {
                Some(usize::from(qd.span().len() + word.span().len()))
            }
            PrintChainItem::Index(index_args) => index_args.single_line_width(printer),
            PrintChainItem::OptionalIndex(qd, index_args) => {
                Some(usize::from(qd.span().len()) + index_args.single_line_width(printer)?)
            }
            PrintChainItem::Call(call_args) => call_args.single_line_width(printer),
            PrintChainItem::OptionalCall(qd, call_args) => {
                Some(usize::from(qd.span().len()) + call_args.single_line_width(printer)?)
            }
            PrintChainItem::GenericArgs(generic_args) => {
                Some(generic_args.formatted_single_line_width())
            }
        }
    }

    /// Returns the single-line width of a group of chain items, or `None` if
    /// any of them can never be single-lined.
    fn group_single_line_width(
        group: &[PrintChainItem<'_>],
        printer: &Printer<'_>,
    ) -> Option<usize> {
        group
            .iter()
            .map(|item| Self::item_single_line_width(item, printer))
            .sum()
    }

    /// Prints a call/index item on the current line. Its arguments may wrap.
    ///
    /// Returns whether the printed item spanned multiple lines.
    fn print_non_field_item(
        item: &PrintChainItem<'_>,
        chain_indent: usize,
        line_remaining_width: &mut usize,
        printer: &mut Printer,
    ) -> bool {
        let multi_lined = match item {
            PrintChainItem::Index(index_args) => {
                let index_shape = Shape {
                    width: *line_remaining_width,
                    indent: chain_indent,
                    first_line_offset: printer.current_line_len().saturating_sub(chain_indent),
                };
                printer.print(index_args, index_shape).multi_lined
            }
            PrintChainItem::OptionalIndex(qd, index_args) => {
                printer.print_raw_token(*qd);
                let index_shape = Shape {
                    width: *line_remaining_width,
                    indent: chain_indent,
                    first_line_offset: printer.current_line_len().saturating_sub(chain_indent),
                };
                printer.print(index_args, index_shape).multi_lined
            }
            &PrintChainItem::Call(call_args) => {
                let call_shape = Shape {
                    width: *line_remaining_width,
                    indent: chain_indent,
                    first_line_offset: printer.current_line_len().saturating_sub(chain_indent),
                };
                printer.print(call_args, call_shape).multi_lined
            }
            &PrintChainItem::OptionalCall(qd, call_args) => {
                printer.print_raw_token(qd);
                let call_shape = Shape {
                    width: *line_remaining_width,
                    indent: chain_indent,
                    first_line_offset: printer.current_line_len().saturating_sub(chain_indent),
                };
                printer.print(call_args, call_shape).multi_lined
            }
            _ => unreachable!("print_non_field_item called with a plain access item"),
        };
        *line_remaining_width = printer.current_line_remaining_width();
        multi_lined
    }

    /// Prints `first` followed by `members` in single-line form. Returns
    /// `None` if any element refuses to single-line. The final total-width
    /// check is left to the caller.
    fn try_print_members_single_line(
        &self,
        members: &[PrintChainItem<'_>],
        shape: &Shape,
        printer: &mut Printer,
    ) -> Option<()> {
        match self.first {
            Expression::Path(path_expr) => {
                printer.print_raw_token(&path_expr.first);
            }
            Expression::FieldAccess(..)
            | Expression::OptionalFieldAccess(..)
            | Expression::OptionalIndex(..)
            | Expression::OptionalCall(..) => {
                unreachable!("Should have been unwrapped when the PrintChain was created")
            }
            Expression::Call(call_expr) => {
                if printer
                    .print(call_expr, Shape::unlimited_single_line())
                    .multi_lined
                {
                    return None;
                }
            }
            Expression::Index(index_expr) => {
                if printer
                    .print(index_expr, Shape::unlimited_single_line())
                    .multi_lined
                {
                    return None;
                }
            }
            _ => {
                if self.first.single_line_width(printer)? > shape.width {
                    return None;
                }
                if printer
                    .print(self.first, Shape::unlimited_single_line())
                    .multi_lined
                {
                    return None;
                }
            }
        }
        for item in members {
            if printer.output.len() > shape.width {
                return None;
            }
            match item {
                &PrintChainItem::FieldAccess(dot, word) => {
                    printer.print_raw_token(dot);
                    printer.print_raw_token(word);
                }
                &PrintChainItem::OptionalFieldAccess(qd, word) => {
                    printer.print_raw_token(qd);
                    printer.print_raw_token(word);
                }
                PrintChainItem::Index(index_args) => {
                    index_args.try_print_single_line(shape, printer)?;
                }
                PrintChainItem::OptionalIndex(qd, index_args) => {
                    printer.print_raw_token(*qd);
                    index_args.try_print_single_line(shape, printer)?;
                }
                &PrintChainItem::Call(call_args) => {
                    call_args.try_print_single_line(shape, printer)?;
                }
                &PrintChainItem::OptionalCall(qd, call_args) => {
                    printer.print_raw_token(qd);
                    call_args.try_print_single_line(shape, printer)?;
                }
                &PrintChainItem::GenericArgs(generic_args) => {
                    printer.print(generic_args, Shape::unlimited_single_line());
                }
            }
        }
        Some(())
    }

    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        self.try_print_members_single_line(&self.chain_members, shape, printer)?;
        if printer.output.len() > shape.width {
            None
        } else {
            Some(PrintInfo::default_single_line())
        }
    }

    /// Hug layout: the whole chain prints on one line except the final call,
    /// whose trailing block-terminal argument hugs the parens (see
    /// [`CallArgs::try_print_hug`]).
    ///
    /// ```baml
    /// futures.push(spawn {
    ///     work(c)
    /// });
    /// ```
    ///
    /// Should be passed a sub-printer to avoid printing partial output in the
    /// event that the hug layout does not apply.
    fn try_print_hug(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        let (last, prefix) = self.chain_members.split_last()?;
        let (question_dot, call_args) = match last {
            PrintChainItem::Call(args) => (None, *args),
            PrintChainItem::OptionalCall(qd, args) => (Some(*qd), *args),
            _ => return None,
        };
        if !call_args.can_hug() {
            return None;
        }
        self.try_print_members_single_line(prefix, shape, printer)?;
        if printer.output.len() > shape.width {
            return None;
        }
        if let Some(qd) = question_dot {
            printer.print_raw_token(qd);
        }
        let hug_shape = Shape {
            width: shape.width,
            indent: shape.indent,
            // `try_print_hug` runs in this same sub-printer, so its
            // `current_line_len()` already includes the printed chain prefix.
            // Keep only the offset that existed before this chain began.
            first_line_offset: shape.first_line_offset,
        };
        call_args.try_print_hug(&hug_shape, printer)
    }

    /// Tail-broken layout: the receiver, the namespace path, and every
    /// intermediate call stay on one line, and only the final call/index
    /// wraps its arguments:
    ///
    /// ```baml
    /// root.ai.Agent<Itinerary>.new().run(
    ///     plan_trip_spec(...),
    /// );
    /// ```
    ///
    /// Applies when the whole prefix up to the final call fits the line.
    /// Should be passed a sub-printer to avoid printing partial output in
    /// the event that the layout does not apply.
    fn try_print_tail_call_broken(
        &self,
        shape: &Shape,
        printer: &mut Printer,
    ) -> Option<PrintInfo> {
        let (last, prefix) = self.chain_members.split_last()?;
        let question_dot = match last {
            PrintChainItem::Call(_) | PrintChainItem::Index(_) => None,
            PrintChainItem::OptionalCall(qd, _) | PrintChainItem::OptionalIndex(qd, _) => Some(*qd),
            PrintChainItem::FieldAccess(..)
            | PrintChainItem::OptionalFieldAccess(..)
            | PrintChainItem::GenericArgs(..) => return None,
        };
        self.try_print_members_single_line(prefix, shape, printer)?;
        if let Some(qd) = question_dot {
            printer.print_raw_token(qd);
        }
        if printer.output.len() > shape.width {
            return None;
        }
        // `shape.width` is the remaining line budget measured from the
        // chain's start column (`width + indent + first_line_offset ==
        // line_width`), and this sub-printer's output also starts at that
        // column, so the args' budget is what the prefix left over.
        let args_shape = Shape {
            width: shape.width.saturating_sub(printer.output.len()),
            indent: shape.indent,
            first_line_offset: shape.first_line_offset + printer.output.len(),
        };
        let info = match last {
            PrintChainItem::Call(call_args) | PrintChainItem::OptionalCall(_, call_args) => {
                printer.print(*call_args, args_shape)
            }
            PrintChainItem::Index(index_args) | PrintChainItem::OptionalIndex(_, index_args) => {
                printer.print(index_args, args_shape)
            }
            _ => unreachable!("checked above"),
        };
        // The final call/index may still overflow the prefix line: its
        // multi-line layout keeps the opening bracket (plus any squished
        // trivia) on that line without re-checking the budget. Reject the
        // layout in that case so the chain breaks at call boundaries instead.
        let first_line_len = printer.output.find('\n').unwrap_or(printer.output.len());
        if first_line_len > shape.width {
            return None;
        }
        Some(info)
    }
}

impl Printable for PrintChain<'_> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|p| self.try_print_single_line(&shape, p))
            .or_else(|| printer.try_sub_printer(|p| self.try_print_hug(&shape, p)))
            .or_else(|| printer.try_sub_printer(|p| self.try_print_tail_call_broken(&shape, p)))
            .unwrap_or_else(|| self.print_multi_line(shape, printer))
    }
    fn leftmost_token(&self) -> TextRange {
        self.first.leftmost_token()
    }
    fn rightmost_token(&self) -> TextRange {
        match self.chain_members.last() {
            Some(
                PrintChainItem::FieldAccess(_, word) | PrintChainItem::OptionalFieldAccess(_, word),
            ) => word.span(),
            Some(
                PrintChainItem::Index(index_args) | PrintChainItem::OptionalIndex(_, index_args),
            ) => index_args.close_bracket.span(),
            Some(PrintChainItem::Call(call_args) | PrintChainItem::OptionalCall(_, call_args)) => {
                call_args.rightmost_token()
            }
            Some(PrintChainItem::GenericArgs(ga)) => ga.close_angle.span(),
            None => self.first.rightmost_token(),
        }
    }
}

/// Only used for printing chained expressions. See [`PrintChain`].
enum PrintChainItem<'a> {
    FieldAccess(&'a t::Dot, &'a t::Word),
    OptionalFieldAccess(&'a t::QuestionDot, &'a t::Word),
    Index(IndexArgs<'a>),
    OptionalIndex(&'a t::QuestionDot, IndexArgs<'a>),
    Call(&'a CallArgs),
    OptionalCall(&'a t::QuestionDot, &'a CallArgs),
    GenericArgs(&'a GenericArgs),
}
