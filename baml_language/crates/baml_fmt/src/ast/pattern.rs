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
    trivia_classifier::TriviaSliceExt,
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
            // Class destructure patterns are emitted by the parser
            // (`Foo { a, b: <pat>, ... }`) and lowered by the compiler, but
            // the formatter doesn't have a printer for them yet. Surface a
            // clear panic so we don't silently bottom out in the catch-all
            // `UnexpectedKindDesc` branch.
            SyntaxKind::DESTRUCTURE_PATTERN => {
                todo!("formatter support for class destructure patterns")
            }
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
        // Preserve trivia between the parens and the inner pattern. Without
        // this, comments like `( /* hint */ Foo )` or `( Foo /* trail */ )`
        // are silently dropped — data loss and an idempotence break for
        // re-formatting. Mirrors the trivia handling in `ChainPattern`.
        printer.print_raw_token(&self.open_paren);
        let (_, open_trailing) = printer.trivia.get_for_range_split(self.open_paren.span());
        printer.print_trivia_squished(open_trailing);
        let pat_leading = printer.trivia.get_leading_for_element(&*self.pattern);
        printer.print_trivia_squished(pat_leading);
        let info = printer.print(&*self.pattern, shape);
        let pat_trailing = printer.trivia.get_trailing_for_element(&*self.pattern);
        printer.print_trivia_squished(pat_trailing);
        printer.print_raw_token(&self.close_paren);
        info
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
    /// Print as `A | B | C` on a single line, preserving trivia adjacent to
    /// each `|` and to every member boundary. Mirrors `UnionType`'s
    /// trivia-aware single-line printer: block comments like `A /* hint */
    /// | B`, `A | /* hint */ B`, and `A /* end */` after the last member
    /// all round-trip cleanly. Bails to multi-line whenever any trivia
    /// would itself span lines, which keeps formatting idempotent.
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        if printer
            .print(&*self.first, Shape::unlimited_single_line())
            .multi_lined
        {
            return None;
        }
        let first_trailing = printer.trivia.get_trailing_for_element(&*self.first);
        let mut pre_pipe_len = printer.try_print_trivia_single_line_squished(first_trailing)?;

        for (i, (pipe, pat)) in self.rest.iter().enumerate() {
            if printer.output.len() > shape.width {
                return None;
            }
            let (pipe_leading, pipe_trailing) = printer.trivia.get_for_range_split(pipe.span());
            let (pat_leading, pat_trailing) = printer.trivia.get_for_element(pat);
            pre_pipe_len += printer.print_trivia_squished(pipe_leading);
            if pre_pipe_len == 0 {
                printer.print_spaces(1); // no block comments between previous member and `|`
            }

            printer.print_raw_token(pipe);

            let mut post_pipe_len = printer.print_trivia_squished(pipe_trailing);
            post_pipe_len += printer.print_trivia_squished(pat_leading);
            if post_pipe_len == 0 {
                printer.print_spaces(1); // no block comments between `|` and next member
            }

            if printer
                .print(pat, Shape::unlimited_single_line())
                .multi_lined
            {
                return None;
            }
            if i + 1 < self.rest.len() {
                pre_pipe_len = printer.try_print_trivia_single_line_squished(pat_trailing)?;
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
        let mut info = printer.print(&*self.first, shape.clone());
        // Emit any line/block comments hanging off the first member's
        // closing token before we break to the next line — keeps trailing
        // comments attached to the right alternative.
        printer.print_trivia_all_trailing_for(self.first.rightmost_token());
        let inner_indent = shape.indent + printer.config.indent_width;

        for (i, (pipe, pat)) in self.rest.iter().enumerate() {
            info.multi_lined = true;
            let (pipe_leading, pipe_trailing) = printer.trivia.get_for_range_split(pipe.span());
            let (pat_leading, pat_trailing) = printer.trivia.get_for_element(pat);

            printer.print_newline();
            // Pre-pipe leading comments (e.g. `/* note */` directly before
            // `|` on its own line) get re-emitted with a hard newline so
            // they don't collide with the indented `|`.
            printer.print_trivia_with_newline(pipe_leading.trim_blanks(), inner_indent);

            printer.print_spaces(inner_indent);
            printer.print_raw_token(pipe);

            let mut post_pipe_len = printer.print_trivia_squished(pipe_trailing);
            post_pipe_len += printer.print_trivia_squished(pat_leading);
            if post_pipe_len == 0 {
                printer.print_spaces(1); // only add space if there are no block comments between
            }
            printer.print(pat, shape.clone());
            if i + 1 < self.rest.len() {
                printer.print_trivia_trailing(pat_trailing);
            }
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
        // Accumulate multi-line state from every link so callers like
        // `LetStmt` / `ForIteratorArgs` don't mistakenly treat the chain as
        // single-line after a child emitted newlines.
        let mut info = printer.print(&*self.first, shape.clone());
        for (i, (colon, pat)) in self.rest.iter().enumerate() {
            printer.print_raw_token(colon);
            printer.print_str(" ");
            // Preserve trivia between `:` and the next pattern (block
            // comments, etc.) — mirrors the trivia handling on let-stmt
            // type annotations.
            let (_, colon_trailing) = printer.trivia.get_for_range_split(colon.span());
            printer.print_trivia_squished(colon_trailing);
            let pat_leading = printer.trivia.get_leading_for_element(pat);
            printer.print_trivia_squished(pat_leading);
            info.multi_lined |= printer.print(pat, shape.clone()).multi_lined;
            // Trailing trivia on each link except the LAST — for the last
            // link, the trailing trivia belongs to the surrounding context
            // (e.g. the match-arm body) and printing it here would duplicate.
            if i + 1 < self.rest.len() {
                let pat_trailing = printer.trivia.get_trailing_for_element(pat);
                printer.print_trivia_squished(pat_trailing);
            }
        }
        info
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
