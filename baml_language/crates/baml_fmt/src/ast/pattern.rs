//! Formatter AST for unified patterns.
//!
//! Mirrors the parser's pattern grammar:
//!
//! ```text
//!   PATTERN     := CHAIN
//!   CHAIN       := UNION (':' UNION)*
//!   UNION       := ATOM ('|' ATOM)*
//!   ATOM        := BINDING_PATTERN
//!                | DESTRUCTURE_PATTERN  (currently parser-gated)
//!                | TYPE_PATTERN
//!                | PAREN_PATTERN
//!                | WILDCARD_PATTERN
//! ```
//!
//! `:` (chain narrow) is split before `|` (union alternation), so
//! `let x: int | string` parses as `let x : (int | string)`.

use baml_db::baml_compiler_syntax::{SyntaxElement, SyntaxKind};
use rowan::TextRange;

use crate::{
    ast::{FromCST, KnownKind, StrongAstError, SyntaxNodeIter, Token, Type, tokens as t},
    printer::{PrintInfo, PrintMultiLine, Printable, Printer, Shape},
};

/// Top-level pattern AST node — corresponds to a [`SyntaxKind::PATTERN`].
#[derive(Debug)]
pub enum MatchPattern {
    Wildcard(WildcardPattern),
    Binding(BindingPattern),
    Type(TypePattern),
    Paren(ParenPattern),
    Union(UnionPattern),
    Chain(ChainPattern),
}

impl FromCST for MatchPattern {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::PATTERN)?;

        let mut it = SyntaxNodeIter::new(&node);
        let inner = it.expect_next("pattern body")?;
        it.expect_end()?;
        MatchPattern::from_inner(inner)
    }
}

impl MatchPattern {
    /// Convert one of the inner pattern kinds (an atom, `UNION_PATTERN`, or
    /// `CHAIN_PATTERN`) into the rich enum.
    fn from_inner(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        match node.kind() {
            SyntaxKind::WILDCARD_PATTERN => {
                WildcardPattern::from_node(&node).map(MatchPattern::Wildcard)
            }
            SyntaxKind::BINDING_PATTERN => {
                BindingPattern::from_node(&node).map(MatchPattern::Binding)
            }
            SyntaxKind::TYPE_PATTERN => TypePattern::from_node(&node).map(MatchPattern::Type),
            SyntaxKind::PAREN_PATTERN => ParenPattern::from_node(&node).map(MatchPattern::Paren),
            SyntaxKind::UNION_PATTERN => UnionPattern::from_node(&node).map(MatchPattern::Union),
            SyntaxKind::CHAIN_PATTERN => ChainPattern::from_node(&node).map(MatchPattern::Chain),
            found => Err(StrongAstError::UnexpectedKindDesc {
                expected_desc: "a pattern kind".into(),
                found,
                at: node.text_range(),
            }),
        }
    }
}

impl KnownKind for MatchPattern {
    fn kind() -> SyntaxKind {
        SyntaxKind::PATTERN
    }
}

impl Printable for MatchPattern {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            MatchPattern::Wildcard(p) => p.print(shape, printer),
            MatchPattern::Binding(p) => p.print(shape, printer),
            MatchPattern::Type(p) => p.print(shape, printer),
            MatchPattern::Paren(p) => p.print(shape, printer),
            MatchPattern::Union(p) => p.print(shape, printer),
            MatchPattern::Chain(p) => p.print(shape, printer),
        }
    }
    fn leftmost_token(&self) -> TextRange {
        match self {
            MatchPattern::Wildcard(p) => p.leftmost_token(),
            MatchPattern::Binding(p) => p.leftmost_token(),
            MatchPattern::Type(p) => p.leftmost_token(),
            MatchPattern::Paren(p) => p.leftmost_token(),
            MatchPattern::Union(p) => p.leftmost_token(),
            MatchPattern::Chain(p) => p.leftmost_token(),
        }
    }
    fn rightmost_token(&self) -> TextRange {
        match self {
            MatchPattern::Wildcard(p) => p.rightmost_token(),
            MatchPattern::Binding(p) => p.rightmost_token(),
            MatchPattern::Type(p) => p.rightmost_token(),
            MatchPattern::Paren(p) => p.rightmost_token(),
            MatchPattern::Union(p) => p.rightmost_token(),
            MatchPattern::Chain(p) => p.rightmost_token(),
        }
    }
}

// ─── Atoms ────────────────────────────────────────────────────────────────────

/// `_` or `let _`.
#[derive(Debug)]
pub struct WildcardPattern {
    pub let_keyword: Option<t::Let>,
    pub underscore: t::Word,
}

impl WildcardPattern {
    fn from_node(node: &baml_db::baml_compiler_syntax::SyntaxNode) -> Result<Self, StrongAstError> {
        let mut it = SyntaxNodeIter::new(node);
        let let_keyword = it
            .next_if_kind(SyntaxKind::KW_LET)
            .map(t::Let::from_cst)
            .transpose()?;
        let underscore_elem = it.expect_next("`_`")?;
        let underscore = t::Word::from_cst(underscore_elem)?;
        it.expect_end()?;
        Ok(Self {
            let_keyword,
            underscore,
        })
    }
}

impl Printable for WildcardPattern {
    fn print(&self, _shape: Shape, printer: &mut Printer) -> PrintInfo {
        if let Some(let_kw) = &self.let_keyword {
            printer.print_raw_token(let_kw);
            printer.print_str(" ");
        }
        printer.print_raw_token(&self.underscore);
        PrintInfo::default_single_line()
    }
    fn leftmost_token(&self) -> TextRange {
        self.let_keyword
            .as_ref()
            .map(super::tokens::Token::span)
            .unwrap_or_else(|| self.underscore.span())
    }
    fn rightmost_token(&self) -> TextRange {
        self.underscore.span()
    }
}

/// `let WORD` — introduces a name binding.
#[derive(Debug)]
pub struct BindingPattern {
    pub let_keyword: t::Let,
    pub name: t::Word,
}

impl BindingPattern {
    fn from_node(node: &baml_db::baml_compiler_syntax::SyntaxNode) -> Result<Self, StrongAstError> {
        let mut it = SyntaxNodeIter::new(node);
        let let_keyword = it.expect_parse()?;
        let name = it.expect_parse()?;
        it.expect_end()?;
        Ok(Self { let_keyword, name })
    }
}

impl Printable for BindingPattern {
    fn print(&self, _shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.let_keyword);
        printer.print_str(" ");
        printer.print_raw_token(&self.name);
        PrintInfo::default_single_line()
    }
    fn leftmost_token(&self) -> TextRange {
        self.let_keyword.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.name.span()
    }
}

/// Bare type-expression pattern (literals, paths, generics, function types,
/// arrays, etc).
#[derive(Debug)]
pub struct TypePattern {
    pub ty: Type,
}

impl TypePattern {
    fn from_node(node: &baml_db::baml_compiler_syntax::SyntaxNode) -> Result<Self, StrongAstError> {
        let mut it = SyntaxNodeIter::new(node);
        let ty = it.expect_parse()?;
        it.expect_end()?;
        Ok(Self { ty })
    }
}

impl Printable for TypePattern {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print(&self.ty, shape)
    }
    fn leftmost_token(&self) -> TextRange {
        self.ty.leftmost_token()
    }
    fn rightmost_token(&self) -> TextRange {
        self.ty.rightmost_token()
    }
}

/// `( PATTERN )` — explicit grouping.
#[derive(Debug)]
pub struct ParenPattern {
    pub open_paren: t::LParen,
    pub pattern: Box<MatchPattern>,
    pub close_paren: t::RParen,
}

impl ParenPattern {
    fn from_node(node: &baml_db::baml_compiler_syntax::SyntaxNode) -> Result<Self, StrongAstError> {
        let mut it = SyntaxNodeIter::new(node);
        let open_paren = it.expect_parse()?;
        let pattern = it.expect_parse()?;
        let close_paren = it.expect_parse()?;
        it.expect_end()?;
        Ok(Self {
            open_paren,
            pattern: Box::new(pattern),
            close_paren,
        })
    }
}

impl Printable for ParenPattern {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.open_paren);
        printer.print(&*self.pattern, shape);
        printer.print_raw_token(&self.close_paren);
        PrintInfo::default_single_line()
    }
    fn leftmost_token(&self) -> TextRange {
        self.open_paren.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.close_paren.span()
    }
}

// ─── Combinators ──────────────────────────────────────────────────────────────

/// Union alternation: `A | B | C`. Each member is a pattern (typically an atom,
/// since `|` binds tighter than `:`).
#[derive(Debug)]
pub struct UnionPattern {
    pub first: Box<MatchPattern>,
    pub rest: Vec<(t::Pipe, MatchPattern)>,
}

impl UnionPattern {
    fn from_node(node: &baml_db::baml_compiler_syntax::SyntaxNode) -> Result<Self, StrongAstError> {
        let mut it = SyntaxNodeIter::new(node);
        let first_elem = it.expect_next("a pattern atom")?;
        let first = MatchPattern::from_inner(first_elem)?;
        let mut rest = Vec::new();
        while let Some(pipe_elem) = it.next() {
            let pipe = t::Pipe::from_cst(pipe_elem)?;
            let next_elem = it.expect_next("a pattern atom after `|`")?;
            let next = MatchPattern::from_inner(next_elem)?;
            rest.push((pipe, next));
        }
        Ok(Self {
            first: Box::new(first),
            rest,
        })
    }
}

impl UnionPattern {
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        if printer
            .print(&*self.first, Shape::unlimited_single_line())
            .multi_lined
        {
            return None;
        }
        for (pipe, pat) in &self.rest {
            if printer.output.len() > shape.width {
                return None;
            }
            printer.print_str(" ");
            printer.print_raw_token(pipe);
            printer.print_str(" ");
            if printer
                .print(pat, Shape::unlimited_single_line())
                .multi_lined
            {
                return None;
            }
        }
        if printer.output.len() > shape.width {
            None
        } else {
            Some(PrintInfo::default_single_line())
        }
    }
}

impl PrintMultiLine for UnionPattern {
    /// Multi-line layout: first member on the current line, each subsequent
    /// member starts with `|` on its own indented line.
    ///
    /// ```baml
    /// FirstPattern
    ///     | SecondPattern
    ///     | ThirdPattern
    /// ```
    fn print_multi_line(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let inner_indent = shape.indent + printer.config.indent_width;
        let mut info = printer.print(&*self.first, shape.clone());
        for (pipe, pat) in &self.rest {
            info.multi_lined = true;
            printer.print_newline();
            printer.print_spaces(inner_indent);
            printer.print_raw_token(pipe);
            printer.print_str(" ");
            printer.print(pat, shape.clone());
        }
        info
    }
}

impl Printable for UnionPattern {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|p| self.try_print_single_line(&shape, p))
            .unwrap_or_else(|| self.print_multi_line(shape, printer))
    }
    fn leftmost_token(&self) -> TextRange {
        self.first.leftmost_token()
    }
    fn rightmost_token(&self) -> TextRange {
        self.rest
            .last()
            .map(|(_, p)| p.rightmost_token())
            .unwrap_or_else(|| self.first.rightmost_token())
    }
}

/// Type-narrowing chain: `A : B : C`. Each link is a pattern (atom or union).
#[derive(Debug)]
pub struct ChainPattern {
    pub first: Box<MatchPattern>,
    pub rest: Vec<(t::Colon, MatchPattern)>,
}

impl ChainPattern {
    fn from_node(node: &baml_db::baml_compiler_syntax::SyntaxNode) -> Result<Self, StrongAstError> {
        let mut it = SyntaxNodeIter::new(node);
        let first_elem = it.expect_next("a pattern atom")?;
        let first = MatchPattern::from_inner(first_elem)?;
        let mut rest = Vec::new();
        while let Some(colon_elem) = it.next() {
            let colon = t::Colon::from_cst(colon_elem)?;
            let next_elem = it.expect_next("a pattern atom after `:`")?;
            let next = MatchPattern::from_inner(next_elem)?;
            rest.push((colon, next));
        }
        Ok(Self {
            first: Box::new(first),
            rest,
        })
    }
}

impl Printable for ChainPattern {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print(&*self.first, shape.clone());
        for (colon, pat) in &self.rest {
            printer.print_raw_token(colon);
            printer.print_str(" ");
            // Preserve trivia between `:` and the next pattern (block
            // comments, etc.) — mirrors the trivia handling on let-stmt
            // type annotations.
            let (_, colon_trailing) = printer.trivia.get_for_range_split(colon.span());
            printer.print_trivia_squished(colon_trailing);
            let pat_leading = printer.trivia.get_leading_for_element(pat);
            printer.print_trivia_squished(pat_leading);
            printer.print(pat, shape.clone());
            // Trailing trivia on each link (except the last) — preserves
            // block comments after the type annotation.
            let pat_trailing = printer.trivia.get_trailing_for_element(pat);
            printer.print_trivia_squished(pat_trailing);
        }
        PrintInfo::default_single_line()
    }
    fn leftmost_token(&self) -> TextRange {
        self.first.leftmost_token()
    }
    fn rightmost_token(&self) -> TextRange {
        self.rest
            .last()
            .map(|(_, p)| p.rightmost_token())
            .unwrap_or_else(|| self.first.rightmost_token())
    }
}
