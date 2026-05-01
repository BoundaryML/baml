//! Formatter AST for PATTERN nodes.
//!
//! CST shape:
//!
//! ```text
//! PATTERN := PATTERN_ALT (PIPE PATTERN_ALT)*
//! PATTERN_ALT := KW_LET? position (COLON KW_LET? position)*
//! position := WORD | TYPE_EXPR ({fields})? (PIPE position)*    -- chain-local Or
//! ```
//!
//! Top-level `|` between `PATTERN_ALT` siblings always precedes `let` (the
//! parser only emits a top-level pipe boundary when the next token is `let`).
//! Plain `|` inside a `PATTERN_ALT` is chain-local — it extends the most
//! recent position with Or-alternatives.

use baml_db::baml_compiler_syntax::{SyntaxElement, SyntaxKind};
use rowan::TextRange;

use crate::{
    ast::{FromCST, KnownKind, StrongAstError, SyntaxNodeIter, Token, Type, tokens as t},
    printer::{PrintInfo, Printable, Printer, Shape},
};

/// A single position with optional chain-local Or alternatives.
#[derive(Debug)]
pub struct PatternPosition {
    pub first: PatternPositionAtom,
    pub or_alts: Vec<(t::Pipe, PatternPositionAtom)>,
}

#[derive(Debug)]
pub enum PatternPositionAtom {
    /// A bare WORD token (binding name after `let`).
    Word(t::Word),
    /// A `TYPE_EXPR` node (type, literal, union, path, etc.).
    TypeExpr(Type),
}

impl Printable for PatternPositionAtom {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            PatternPositionAtom::Word(word) => {
                printer.print_raw_token(word);
                PrintInfo::default_single_line()
            }
            PatternPositionAtom::TypeExpr(ty) => printer.print(ty, shape),
        }
    }
    fn leftmost_token(&self) -> TextRange {
        match self {
            PatternPositionAtom::Word(word) => word.span(),
            PatternPositionAtom::TypeExpr(ty) => ty.leftmost_token(),
        }
    }
    fn rightmost_token(&self) -> TextRange {
        match self {
            PatternPositionAtom::Word(word) => word.span(),
            PatternPositionAtom::TypeExpr(ty) => ty.rightmost_token(),
        }
    }
}

impl PatternPosition {
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        if printer
            .print(&self.first, Shape::unlimited_single_line())
            .multi_lined
        {
            return None;
        }
        for (pipe, atom) in &self.or_alts {
            printer.print_str(" ");
            printer.print_raw_token(pipe);
            printer.print_str(" ");
            if printer
                .print(atom, Shape::unlimited_single_line())
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

    fn print_multi_line(&self, shape: &Shape, printer: &mut Printer) -> PrintInfo {
        printer.print(&self.first, shape.clone());
        let alt_indent = shape.indent + printer.config.indent_width;
        for (pipe, atom) in &self.or_alts {
            printer.print_newline();
            printer.print_spaces(alt_indent);
            printer.print_raw_token(pipe);
            printer.print_str(" ");
            printer.print(atom, shape.clone());
        }
        PrintInfo::default_multi_lined()
    }
}

impl Printable for PatternPosition {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        if self.or_alts.is_empty() {
            return printer.print(&self.first, shape);
        }
        printer
            .try_sub_printer(|p| self.try_print_single_line(&shape, p))
            .unwrap_or_else(|| self.print_multi_line(&shape, printer))
    }
    fn leftmost_token(&self) -> TextRange {
        self.first.leftmost_token()
    }
    fn rightmost_token(&self) -> TextRange {
        self.or_alts
            .last()
            .map(|(_, a)| a.rightmost_token())
            .unwrap_or_else(|| self.first.rightmost_token())
    }
}

/// One `PATTERN_ALT`: a colon-chain of positions, optionally led by `let`.
#[derive(Debug)]
pub struct FmtPatternAlt {
    pub kw_let: Option<t::Let>,
    pub first: PatternPosition,
    pub chain: Vec<ChainedPosition>,
}

/// A single `: <position>` in the chain.
#[derive(Debug)]
pub struct ChainedPosition {
    pub colon: t::Colon,
    pub kw_let: Option<t::Let>,
    pub position: PatternPosition,
}

/// Corresponds to a [`SyntaxKind::PATTERN`] node — one or more
/// `PATTERN_ALT` alternatives separated by top-level `| let` boundaries.
#[derive(Debug)]
pub struct FmtPattern {
    pub first: FmtPatternAlt,
    pub alts: Vec<(t::Pipe, FmtPatternAlt)>,
}

fn take_atom(it: &mut SyntaxNodeIter) -> Result<PatternPositionAtom, StrongAstError> {
    let elem = it.expect_next("WORD or TYPE_EXPR")?;
    match elem.kind() {
        SyntaxKind::WORD => Ok(PatternPositionAtom::Word(t::Word::new_from_span(
            elem.text_range(),
        ))),
        SyntaxKind::TYPE_EXPR => Ok(PatternPositionAtom::TypeExpr(Type::from_cst(elem)?)),
        found => Err(StrongAstError::UnexpectedKindDesc {
            expected_desc: "WORD or TYPE_EXPR".into(),
            found,
            at: elem.text_range(),
        }),
    }
}

fn take_position(it: &mut SyntaxNodeIter) -> Result<PatternPosition, StrongAstError> {
    let first = take_atom(it)?;
    let mut or_alts = Vec::new();
    while let Some(pipe_elem) = it.next_if_kind(SyntaxKind::PIPE) {
        let pipe = t::Pipe::from_cst(pipe_elem)?;
        let atom = take_atom(it)?;
        or_alts.push((pipe, atom));
    }
    Ok(PatternPosition { first, or_alts })
}

impl FromCST for FmtPatternAlt {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::PATTERN_ALT)?;

        let mut it = SyntaxNodeIter::new(&node);

        let kw_let = it
            .next_if_kind(SyntaxKind::KW_LET)
            .map(t::Let::from_cst)
            .transpose()?;

        let first = take_position(&mut it)?;

        let mut chain = Vec::new();
        while let Some(colon_elem) = it.next_if_kind(SyntaxKind::COLON) {
            let colon = t::Colon::from_cst(colon_elem)?;
            let chain_let = it
                .next_if_kind(SyntaxKind::KW_LET)
                .map(t::Let::from_cst)
                .transpose()?;
            let position = take_position(&mut it)?;
            chain.push(ChainedPosition {
                colon,
                kw_let: chain_let,
                position,
            });
        }

        Ok(FmtPatternAlt {
            kw_let,
            first,
            chain,
        })
    }
}

impl KnownKind for FmtPatternAlt {
    fn kind() -> SyntaxKind {
        SyntaxKind::PATTERN_ALT
    }
}

impl FromCST for FmtPattern {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::PATTERN)?;

        let mut it = SyntaxNodeIter::new(&node);

        let first_elem = it.expect_next("PATTERN_ALT")?;
        let first = FmtPatternAlt::from_cst(first_elem)?;

        let mut alts = Vec::new();
        while let Some(pipe_elem) = it.next_if_kind(SyntaxKind::PIPE) {
            let pipe = t::Pipe::from_cst(pipe_elem)?;
            let alt_elem = it.expect_next("PATTERN_ALT")?;
            let alt = FmtPatternAlt::from_cst(alt_elem)?;
            alts.push((pipe, alt));
        }

        Ok(FmtPattern { first, alts })
    }
}

impl KnownKind for FmtPattern {
    fn kind() -> SyntaxKind {
        SyntaxKind::PATTERN
    }
}

impl Printable for FmtPatternAlt {
    fn print(&self, mut shape: Shape, printer: &mut Printer) -> PrintInfo {
        if let Some(kw) = &self.kw_let {
            printer.print_raw_token(kw);
            printer.print_str(" ");
            shape.first_line_offset += 4; // "let "
            shape.width = shape.width.saturating_sub(4);
        }

        let mut info = printer.print(&self.first, shape.clone());

        for chained in &self.chain {
            printer.print_raw_token(&chained.colon);
            let (_, colon_trailing) = printer.trivia.get_for_range_split(chained.colon.span());
            printer.print_str(" ");
            printer.print_trivia_squished(colon_trailing);

            if let Some(kw) = &chained.kw_let {
                printer.print_raw_token(kw);
                printer.print_str(" ");
            }

            let pos_leading = printer.trivia.get_leading_for_element(&chained.position);
            printer.print_trivia_squished(pos_leading);
            let sub_info = printer.print(&chained.position, shape.clone());
            info.multi_lined |= sub_info.multi_lined;
        }

        info
    }
    fn leftmost_token(&self) -> TextRange {
        if let Some(kw) = &self.kw_let {
            kw.span()
        } else {
            self.first.leftmost_token()
        }
    }
    fn rightmost_token(&self) -> TextRange {
        if let Some(last) = self.chain.last() {
            last.position.rightmost_token()
        } else {
            self.first.rightmost_token()
        }
    }
}

impl Printable for FmtPattern {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let mut info = printer.print(&self.first, shape.clone());
        for (pipe, alt) in &self.alts {
            printer.print_str(" ");
            printer.print_raw_token(pipe);
            printer.print_str(" ");
            let sub = printer.print(alt, shape.clone());
            info.multi_lined |= sub.multi_lined;
        }
        info
    }
    fn leftmost_token(&self) -> TextRange {
        self.first.leftmost_token()
    }
    fn rightmost_token(&self) -> TextRange {
        self.alts
            .last()
            .map(|(_, a)| a.rightmost_token())
            .unwrap_or_else(|| self.first.rightmost_token())
    }
}
