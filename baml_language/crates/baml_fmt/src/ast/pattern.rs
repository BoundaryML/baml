//! Formatter AST for PATTERN nodes.
//!
//! The new parser produces patterns with the following CST shape:
//!
//! ```text
//! PATTERN := KW_LET? (WORD | TYPE_EXPR) (COLON KW_LET? (WORD | TYPE_EXPR))*
//! ```
//!
//! Types, literals, unions, and paths are all inside `TYPE_EXPR` nodes,
//! which the formatter's existing `Type` handles. `FmtPattern` only
//! needs to handle the pattern-level structure: optional `let`, the
//! binding name or type, and colon-separated chains.

use baml_db::baml_compiler_syntax::{SyntaxElement, SyntaxKind};
use rowan::TextRange;

use crate::{
    ast::{FromCST, KnownKind, StrongAstError, SyntaxNodeIter, Token, Type, tokens as t},
    printer::{PrintInfo, Printable, Printer, Shape},
};

/// A single position in the colon-separated chain.
#[derive(Debug)]
pub enum PatternPosition {
    /// A bare WORD token (binding name after `let`, or bare identifier).
    Word(t::Word),
    /// A `TYPE_EXPR` node (type, literal, union, path, etc.).
    TypeExpr(Type),
}

impl Printable for PatternPosition {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            PatternPosition::Word(word) => {
                printer.print_raw_token(word);
                PrintInfo::default_single_line()
            }
            PatternPosition::TypeExpr(ty) => printer.print(ty, shape),
        }
    }
    fn leftmost_token(&self) -> TextRange {
        match self {
            PatternPosition::Word(word) => word.span(),
            PatternPosition::TypeExpr(ty) => ty.leftmost_token(),
        }
    }
    fn rightmost_token(&self) -> TextRange {
        match self {
            PatternPosition::Word(word) => word.span(),
            PatternPosition::TypeExpr(ty) => ty.rightmost_token(),
        }
    }
}

/// Corresponds to a [`SyntaxKind::PATTERN`] node.
///
/// The pattern is a sequence of colon-separated positions, each optionally
/// preceded by `let`. The first position may be a binding name (WORD) or
/// a type expression (`TYPE_EXPR`).
#[derive(Debug)]
pub struct FmtPattern {
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

fn take_position(it: &mut SyntaxNodeIter) -> Result<PatternPosition, StrongAstError> {
    let elem = it.expect_next("WORD or TYPE_EXPR")?;
    match elem.kind() {
        SyntaxKind::WORD => Ok(PatternPosition::Word(t::Word::new_from_span(
            elem.text_range(),
        ))),
        SyntaxKind::TYPE_EXPR => Ok(PatternPosition::TypeExpr(Type::from_cst(elem)?)),
        found => Err(StrongAstError::UnexpectedKindDesc {
            expected_desc: "WORD or TYPE_EXPR".into(),
            found,
            at: elem.text_range(),
        }),
    }
}

impl FromCST for FmtPattern {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::PATTERN)?;

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

        Ok(FmtPattern {
            kw_let,
            first,
            chain,
        })
    }
}

impl KnownKind for FmtPattern {
    fn kind() -> SyntaxKind {
        SyntaxKind::PATTERN
    }
}

impl Printable for FmtPattern {
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
