//! Formatter AST for unified patterns.
//!
//! Mirrors the parser's pattern grammar:
//!
//! ```text
//!   PATTERN     := CHAIN
//!   CHAIN       := UNION (':' UNION)*
//!   UNION       := ATOM ('|' ATOM)*
//!   ATOM        := BINDING_PATTERN
//!                | DESTRUCTURE_PATTERN
//!                | ARRAY_PATTERN
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
    ast::{
        FromCST, GenericArgs, KnownKind, StrongAstError, SyntaxNodeIter, Token, Type, TypeArgs,
        tokens as t,
    },
    printer::{PrintInfo, PrintMultiLine, Printable, Printer, Shape},
    trivia_classifier::{EmittableTrivia, TriviaSliceExt},
};

fn try_print_trivia_single_line_spaced(
    printer: &mut Printer,
    trivia: &[EmittableTrivia],
    before: bool,
    after: bool,
) -> Option<usize> {
    let trivia_len = trivia
        .iter()
        .map(|t| t.single_line_len(printer.input))
        .sum::<Option<usize>>()?;
    let comments = trivia
        .iter()
        .filter(|t| t.is_comment() && t.single_line_len(printer.input).is_some())
        .collect::<Vec<_>>();
    if !comments.is_empty() && before {
        printer.print_str(" ");
    }
    for (i, t) in comments.iter().enumerate() {
        if i > 0 {
            printer.print_str(" ");
        }
        printer.print_trivia(t);
    }
    if !comments.is_empty() && after {
        printer.print_str(" ");
    }
    Some(trivia_len)
}

fn print_trivia_squished_spaced(
    printer: &mut Printer,
    trivia: &[EmittableTrivia],
    before: bool,
    after: bool,
) -> usize {
    let trivia_len = trivia
        .iter()
        .filter_map(|t| t.single_line_len(printer.input))
        .sum::<usize>();
    let comments = trivia
        .iter()
        .filter(|t| t.is_comment() && t.single_line_len(printer.input).is_some())
        .collect::<Vec<_>>();
    if !comments.is_empty() && before {
        printer.print_str(" ");
    }
    for (i, t) in comments.iter().enumerate() {
        if i > 0 {
            printer.print_str(" ");
        }
        printer.print_trivia(t);
    }
    if !comments.is_empty() && after {
        printer.print_str(" ");
    }
    trivia_len
}

fn print_binding_keyword_with_trailing_trivia(printer: &mut Printer, keyword: &t::BindingKeyword) {
    printer.print_raw_token(keyword);
    printer.print_str(" ");
    let (_, trailing) = printer.trivia.get_for_range_split(keyword.span());
    if print_trivia_squished_spaced(printer, trailing, false, false) > 0 {
        printer.print_str(" ");
    }
}

/// Top-level pattern AST node — corresponds to a [`SyntaxKind::PATTERN`].
#[derive(Debug)]
pub enum MatchPattern {
    Wildcard(WildcardPattern),
    Binding(BindingPattern),
    Destructure(DestructurePattern),
    Array(ArrayPattern),
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
            SyntaxKind::ARRAY_PATTERN => ArrayPattern::from_node(&node).map(MatchPattern::Array),
            SyntaxKind::PAREN_PATTERN => ParenPattern::from_node(&node).map(MatchPattern::Paren),
            SyntaxKind::UNION_PATTERN => UnionPattern::from_node(&node).map(MatchPattern::Union),
            SyntaxKind::CHAIN_PATTERN => ChainPattern::from_node(&node).map(MatchPattern::Chain),
            SyntaxKind::DESTRUCTURE_PATTERN => {
                DestructurePattern::from_node(&node).map(MatchPattern::Destructure)
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
            MatchPattern::Destructure(p) => p.print(shape, printer),
            MatchPattern::Array(p) => p.print(shape, printer),
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
            MatchPattern::Destructure(p) => p.leftmost_token(),
            MatchPattern::Array(p) => p.leftmost_token(),
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
            MatchPattern::Destructure(p) => p.rightmost_token(),
            MatchPattern::Array(p) => p.rightmost_token(),
            MatchPattern::Type(p) => p.rightmost_token(),
            MatchPattern::Paren(p) => p.rightmost_token(),
            MatchPattern::Union(p) => p.rightmost_token(),
            MatchPattern::Chain(p) => p.rightmost_token(),
        }
    }
}

// ─── Atoms ────────────────────────────────────────────────────────────────────

/// `_`, `let _`, or `const _`.
#[derive(Debug)]
pub struct WildcardPattern {
    pub let_keyword: Option<t::BindingKeyword>,
    pub underscore: t::Word,
}

impl WildcardPattern {
    fn from_node(node: &baml_db::baml_compiler_syntax::SyntaxNode) -> Result<Self, StrongAstError> {
        let mut it = SyntaxNodeIter::new(node);
        let let_keyword = it
            .next_if(|elem| matches!(elem.kind(), SyntaxKind::KW_LET | SyntaxKind::KW_CONST))
            .map(t::BindingKeyword::from_cst)
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
            print_binding_keyword_with_trailing_trivia(printer, let_kw);
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

/// `let WORD`/`const WORD` or `let WORD : <pattern>`/`const WORD : <pattern>` — name binding with an optional
/// sub-pattern. The sub-pattern can be a type ascription (`let x: int`),
/// another binding (`let x: let y`), a structural destructure
/// (`let x: [a, b]`, `let x: Class { f }`), etc. The parser folds the
/// `: <pattern>` directly into the [`SyntaxKind::BINDING_PATTERN`] node
/// (no `CHAIN_PATTERN` wrapper).
#[derive(Debug)]
pub struct BindingPattern {
    pub let_keyword: t::BindingKeyword,
    pub name: t::Word,
    pub subpat: Option<(t::Colon, Box<MatchPattern>)>,
}

impl BindingPattern {
    fn from_node(node: &baml_db::baml_compiler_syntax::SyntaxNode) -> Result<Self, StrongAstError> {
        let mut it = SyntaxNodeIter::new(node);
        let let_keyword = t::BindingKeyword::from_cst(it.expect_next("binding introducer")?)?;
        let name = it.expect_parse()?;
        let subpat = if let Some(colon_elem) = it.next_if_kind(SyntaxKind::COLON) {
            let colon = t::Colon::from_cst(colon_elem)?;
            let pattern: MatchPattern = it.expect_parse()?;
            Some((colon, Box::new(pattern)))
        } else {
            None
        };
        it.expect_end()?;
        Ok(Self {
            let_keyword,
            name,
            subpat,
        })
    }
}

impl Printable for BindingPattern {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        print_binding_keyword_with_trailing_trivia(printer, &self.let_keyword);
        printer.print_raw_token(&self.name);
        let mut info = PrintInfo::default_single_line();
        if let Some((colon, pattern)) = &self.subpat {
            let (_, name_trailing) = printer.trivia.get_for_range_split(self.name.span());
            print_trivia_squished_spaced(printer, name_trailing, true, false);
            let (colon_leading, colon_trailing) = printer.trivia.get_for_range_split(colon.span());
            print_trivia_squished_spaced(printer, colon_leading, true, false);
            printer.print_raw_token(colon);
            printer.print_str(" ");
            print_trivia_squished_spaced(printer, colon_trailing, false, true);
            let pattern_leading = printer.trivia.get_leading_for_element(&**pattern);
            print_trivia_squished_spaced(printer, pattern_leading, false, true);
            info.multi_lined |= printer.print(&**pattern, shape).multi_lined;
        }
        info
    }
    fn leftmost_token(&self) -> TextRange {
        self.let_keyword.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.subpat
            .as_ref()
            .map(|(_, p)| p.rightmost_token())
            .unwrap_or_else(|| self.name.span())
    }
}

/// `(let|const)? path.Class { field, renamed: <pattern>, ... }`.
#[derive(Debug)]
pub struct DestructurePattern {
    pub let_keyword: Option<t::BindingKeyword>,
    pub first: t::Word,
    pub rest: Vec<(t::Dot, t::Word)>,
    pub generic_args: Option<DestructureTypeArgs>,
    pub open_brace: t::LBrace,
    pub fields: Vec<(FieldPattern, Option<t::Comma>)>,
    pub close_brace: t::RBrace,
}

#[derive(Debug)]
pub enum DestructureTypeArgs {
    Generic(GenericArgs),
    Type(TypeArgs),
}

impl FromCST for DestructureTypeArgs {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        match elem.kind() {
            SyntaxKind::GENERIC_ARGS => GenericArgs::from_cst(elem).map(Self::Generic),
            SyntaxKind::TYPE_ARGS => TypeArgs::from_cst(elem).map(Self::Type),
            found => Err(StrongAstError::UnexpectedKindDesc {
                expected_desc: "GENERIC_ARGS or TYPE_ARGS".into(),
                found,
                at: elem.text_range(),
            }),
        }
    }
}

impl Printable for DestructureTypeArgs {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            DestructureTypeArgs::Generic(args) => args.print(shape, printer),
            DestructureTypeArgs::Type(args) => args.print(shape, printer),
        }
    }

    fn leftmost_token(&self) -> TextRange {
        match self {
            DestructureTypeArgs::Generic(args) => args.leftmost_token(),
            DestructureTypeArgs::Type(args) => args.leftmost_token(),
        }
    }

    fn rightmost_token(&self) -> TextRange {
        match self {
            DestructureTypeArgs::Generic(args) => args.rightmost_token(),
            DestructureTypeArgs::Type(args) => args.rightmost_token(),
        }
    }
}

impl DestructurePattern {
    fn from_node(node: &baml_db::baml_compiler_syntax::SyntaxNode) -> Result<Self, StrongAstError> {
        let mut it = SyntaxNodeIter::new(node);
        let let_keyword = it
            .next_if(|elem| matches!(elem.kind(), SyntaxKind::KW_LET | SyntaxKind::KW_CONST))
            .map(t::BindingKeyword::from_cst)
            .transpose()?;
        let first = it.expect_parse()?;
        let mut rest = Vec::new();
        while let Some(dot_elem) = it.next_if_kind(SyntaxKind::DOT) {
            let dot = t::Dot::from_cst(dot_elem)?;
            let word = it.expect_parse()?;
            rest.push((dot, word));
        }
        let generic_args = it
            .next_if(|elem| {
                matches!(
                    elem.kind(),
                    SyntaxKind::GENERIC_ARGS | SyntaxKind::TYPE_ARGS
                )
            })
            .map(DestructureTypeArgs::from_cst)
            .transpose()?;
        let open_brace = it.expect_parse()?;
        let mut fields = Vec::new();
        let close_brace = loop {
            let Some(elem) = it.next() else {
                return Err(StrongAstError::missing(SyntaxKind::R_BRACE, it.parent));
            };
            if elem.kind() == SyntaxKind::R_BRACE {
                break t::RBrace::from_cst(elem)?;
            }
            let field = FieldPattern::from_cst(elem)?;
            let comma = it
                .next_if_kind(SyntaxKind::COMMA)
                .map(t::Comma::from_cst)
                .transpose()?;
            fields.push((field, comma));
        };
        it.expect_end()?;
        Ok(Self {
            let_keyword,
            first,
            rest,
            generic_args,
            open_brace,
            fields,
            close_brace,
        })
    }

    fn print_path(&self, printer: &mut Printer) {
        if let Some(let_kw) = &self.let_keyword {
            print_binding_keyword_with_trailing_trivia(printer, let_kw);
        }
        printer.print_raw_token(&self.first);
        for (dot, word) in &self.rest {
            printer.print_raw_token(dot);
            printer.print_raw_token(word);
        }
        if let Some(generic_args) = &self.generic_args {
            printer.print(generic_args, Shape::unlimited_single_line());
        }
    }

    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        // TODO(class-destructure-format): make this line-width decision aware of
        // the surrounding pattern chain / let statement. Right now a destructure
        // pattern can fit by itself but still produce a long full statement.
        self.print_path(printer);
        printer.print_str(" ");
        printer.print_raw_token(&self.open_brace);

        let (_, open_trailing) = printer.trivia.get_for_range_split(self.open_brace.span());
        if self.fields.is_empty() {
            try_print_trivia_single_line_spaced(printer, open_trailing, true, true)?;
            let (close_leading, _) = printer.trivia.get_for_range_split(self.close_brace.span());
            try_print_trivia_single_line_spaced(printer, close_leading, true, false)?;
            printer.print_raw_token(&self.close_brace);
            return (printer.output.len() <= shape.width).then(PrintInfo::default_single_line);
        }

        printer.print_str(" ");
        try_print_trivia_single_line_spaced(printer, open_trailing, false, true)?;
        for (i, (field, comma)) in self.fields.iter().enumerate() {
            let (field_leading, field_trailing) = printer.trivia.get_for_element(field);
            try_print_trivia_single_line_spaced(printer, field_leading, false, true)?;
            if printer
                .print(field, Shape::unlimited_single_line())
                .multi_lined
            {
                return None;
            }
            try_print_trivia_single_line_spaced(printer, field_trailing, true, false)?;
            if i + 1 < self.fields.len() {
                if let Some(comma) = comma {
                    let (comma_leading, comma_trailing) =
                        printer.trivia.get_for_range_split(comma.span());
                    try_print_trivia_single_line_spaced(printer, comma_leading, true, false)?;
                    printer.print_raw_token(comma);
                    try_print_trivia_single_line_spaced(printer, comma_trailing, true, false)?;
                } else {
                    printer.print_str(",");
                }
                printer.print_str(" ");
            } else if let Some(comma) = comma {
                let (comma_leading, comma_trailing) =
                    printer.trivia.get_for_range_split(comma.span());
                try_print_trivia_single_line_spaced(printer, comma_leading, true, false)?;
                try_print_trivia_single_line_spaced(printer, comma_trailing, true, false)?;
            }
        }
        let (close_leading, _) = printer.trivia.get_for_range_split(self.close_brace.span());
        try_print_trivia_single_line_spaced(printer, close_leading, true, false)?;
        printer.print_str(" ");
        printer.print_raw_token(&self.close_brace);

        (printer.output.len() <= shape.width).then(PrintInfo::default_single_line)
    }
}

impl PrintMultiLine for DestructurePattern {
    fn print_multi_line(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        self.print_path(printer);
        printer.print_str(" ");
        printer.print_raw_token(&self.open_brace);
        printer.print_trivia_all_trailing_for(self.open_brace.span());
        printer.print_newline();

        let inner_shape = Shape {
            width: shape.width.saturating_sub(printer.config.indent_width),
            indent: shape.indent + printer.config.indent_width,
            first_line_offset: 0,
        };
        for (field, comma) in &self.fields {
            printer.print_trivia_all_leading_with_newline_for(
                field.leftmost_token(),
                inner_shape.indent,
            );
            printer.print_spaces(inner_shape.indent);
            printer.print(field, inner_shape.clone());
            let field_trailing = printer.trivia.get_trailing_for_element(field);
            print_trivia_squished_spaced(printer, field_trailing, true, false);
            if let Some(comma) = comma {
                let (comma_leading, comma_trailing) =
                    printer.trivia.get_for_range_split(comma.span());
                print_trivia_squished_spaced(printer, comma_leading, true, false);
                printer.print_raw_token(comma);
                printer.print_trivia_trailing(comma_trailing);
            } else {
                printer.print_str(",");
            }
            printer.print_newline();
        }

        printer.print_trivia_all_leading_with_newline_for(self.close_brace.span(), shape.indent);
        printer.print_spaces(shape.indent);
        printer.print_raw_token(&self.close_brace);
        PrintInfo::default_multi_lined()
    }
}

impl Printable for DestructurePattern {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|p| self.try_print_single_line(&shape, p))
            .unwrap_or_else(|| self.print_multi_line(shape, printer))
    }

    fn leftmost_token(&self) -> TextRange {
        self.let_keyword
            .as_ref()
            .map(super::tokens::Token::span)
            .unwrap_or_else(|| self.first.span())
    }

    fn rightmost_token(&self) -> TextRange {
        self.close_brace.span()
    }
}

/// A single field inside a destructure pattern.
#[derive(Debug)]
pub struct FieldPattern {
    pub name: t::Word,
    pub pattern: Option<(t::Colon, MatchPattern)>,
}

impl FromCST for FieldPattern {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::FIELD_PATTERN)?;
        let mut it = SyntaxNodeIter::new(&node);
        let name = it.expect_parse()?;
        let pattern = if let Some(colon_elem) = it.next_if_kind(SyntaxKind::COLON) {
            let colon = t::Colon::from_cst(colon_elem)?;
            let pattern = it.expect_parse()?;
            Some((colon, pattern))
        } else {
            None
        };
        it.expect_end()?;
        Ok(Self { name, pattern })
    }
}

impl Printable for FieldPattern {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.name);
        if let Some((colon, pattern)) = &self.pattern {
            let (_, name_trailing) = printer.trivia.get_for_range_split(self.name.span());
            print_trivia_squished_spaced(printer, name_trailing, true, false);
            let (colon_leading, colon_trailing) = printer.trivia.get_for_range_split(colon.span());
            print_trivia_squished_spaced(printer, colon_leading, true, false);
            printer.print_raw_token(colon);
            printer.print_str(" ");
            print_trivia_squished_spaced(printer, colon_trailing, false, true);
            let pattern_leading = printer.trivia.get_leading_for_element(pattern);
            print_trivia_squished_spaced(printer, pattern_leading, false, true);
            printer.print(pattern, shape)
        } else {
            PrintInfo::default_single_line()
        }
    }

    fn leftmost_token(&self) -> TextRange {
        self.name.span()
    }

    fn rightmost_token(&self) -> TextRange {
        self.pattern
            .as_ref()
            .map(|(_, p)| p.rightmost_token())
            .unwrap_or_else(|| self.name.span())
    }
}

#[derive(Debug)]
pub struct ArrayPattern {
    pub open_bracket: t::LBracket,
    pub elements: Vec<(ArrayPatternElement, Option<t::Comma>)>,
    pub close_bracket: t::RBracket,
    /// `[…]: T` — optional type ascription folded into the
    /// [`SyntaxKind::ARRAY_PATTERN`] node by the parser.
    pub ascription: Option<(t::Colon, Type)>,
}

impl ArrayPattern {
    fn from_node(node: &baml_db::baml_compiler_syntax::SyntaxNode) -> Result<Self, StrongAstError> {
        let mut it = SyntaxNodeIter::new(node);
        let open_bracket = it.expect_parse()?;
        let mut elements = Vec::new();
        let close_bracket = loop {
            let Some(elem) = it.next() else {
                return Err(StrongAstError::missing(SyntaxKind::R_BRACKET, it.parent));
            };
            if elem.kind() == SyntaxKind::R_BRACKET {
                break t::RBracket::from_cst(elem)?;
            }
            let element = ArrayPatternElement::from_cst(elem)?;
            let comma = it
                .next_if_kind(SyntaxKind::COMMA)
                .map(t::Comma::from_cst)
                .transpose()?;
            elements.push((element, comma));
        };
        let ascription = if let Some(colon_elem) = it.next_if_kind(SyntaxKind::COLON) {
            let colon = t::Colon::from_cst(colon_elem)?;
            let ty: Type = it.expect_parse()?;
            Some((colon, ty))
        } else {
            None
        };
        it.expect_end()?;
        Ok(Self {
            open_bracket,
            elements,
            close_bracket,
            ascription,
        })
    }

    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        printer.print_raw_token(&self.open_bracket);
        let (_, open_trailing) = printer.trivia.get_for_range_split(self.open_bracket.span());
        try_print_trivia_single_line_spaced(printer, open_trailing, false, true)?;

        for (idx, (element, comma)) in self.elements.iter().enumerate() {
            let (element_leading, element_trailing) = printer.trivia.get_for_element(element);
            try_print_trivia_single_line_spaced(printer, element_leading, false, true)?;
            if printer
                .print(element, Shape::unlimited_single_line())
                .multi_lined
            {
                return None;
            }
            try_print_trivia_single_line_spaced(printer, element_trailing, true, false)?;

            if idx + 1 < self.elements.len() {
                if let Some(comma) = comma {
                    let (comma_leading, comma_trailing) =
                        printer.trivia.get_for_range_split(comma.span());
                    try_print_trivia_single_line_spaced(printer, comma_leading, true, false)?;
                    printer.print_raw_token(comma);
                    try_print_trivia_single_line_spaced(printer, comma_trailing, true, false)?;
                } else {
                    printer.print_str(",");
                }
                printer.print_str(" ");
            } else if let Some(comma) = comma {
                let (comma_leading, comma_trailing) =
                    printer.trivia.get_for_range_split(comma.span());
                try_print_trivia_single_line_spaced(printer, comma_leading, true, false)?;
                try_print_trivia_single_line_spaced(printer, comma_trailing, true, false)?;
            }
        }

        let (close_leading, _) = printer
            .trivia
            .get_for_range_split(self.close_bracket.span());
        try_print_trivia_single_line_spaced(printer, close_leading, true, false)?;
        printer.print_raw_token(&self.close_bracket);

        if let Some((colon, ty)) = &self.ascription {
            let (_, close_trailing) = printer
                .trivia
                .get_for_range_split(self.close_bracket.span());
            try_print_trivia_single_line_spaced(printer, close_trailing, true, false)?;
            let (colon_leading, colon_trailing) = printer.trivia.get_for_range_split(colon.span());
            try_print_trivia_single_line_spaced(printer, colon_leading, true, false)?;
            printer.print_raw_token(colon);
            printer.print_str(" ");
            try_print_trivia_single_line_spaced(printer, colon_trailing, false, true)?;
            if printer
                .print(ty, Shape::unlimited_single_line())
                .multi_lined
            {
                return None;
            }
        }

        (printer.output.len() <= shape.width).then(PrintInfo::default_single_line)
    }
}

impl PrintMultiLine for ArrayPattern {
    fn print_multi_line(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.open_bracket);
        printer.print_trivia_all_trailing_for(self.open_bracket.span());
        printer.print_newline();

        let inner_shape = Shape {
            width: shape.width.saturating_sub(printer.config.indent_width),
            indent: shape.indent + printer.config.indent_width,
            first_line_offset: 0,
        };

        for (element, comma) in &self.elements {
            printer.print_trivia_all_leading_with_newline_for(
                element.leftmost_token(),
                inner_shape.indent,
            );
            printer.print_spaces(inner_shape.indent);
            printer.print(element, inner_shape.clone());
            let element_trailing = printer.trivia.get_trailing_for_element(element);
            print_trivia_squished_spaced(printer, element_trailing, true, false);
            if let Some(comma) = comma {
                let (comma_leading, comma_trailing) =
                    printer.trivia.get_for_range_split(comma.span());
                print_trivia_squished_spaced(printer, comma_leading, true, false);
                printer.print_raw_token(comma);
                printer.print_trivia_trailing(comma_trailing);
            } else {
                printer.print_str(",");
            }
            printer.print_newline();
        }

        printer.print_trivia_all_leading_with_newline_for(self.close_bracket.span(), shape.indent);
        printer.print_spaces(shape.indent);
        printer.print_raw_token(&self.close_bracket);
        if let Some((colon, ty)) = &self.ascription {
            let (_, close_trailing) = printer
                .trivia
                .get_for_range_split(self.close_bracket.span());
            print_trivia_squished_spaced(printer, close_trailing, true, false);
            let (colon_leading, colon_trailing) = printer.trivia.get_for_range_split(colon.span());
            print_trivia_squished_spaced(printer, colon_leading, true, false);
            printer.print_raw_token(colon);
            printer.print_str(" ");
            print_trivia_squished_spaced(printer, colon_trailing, false, true);
            printer.print(ty, shape);
        }
        PrintInfo::default_multi_lined()
    }
}

impl Printable for ArrayPattern {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|p| self.try_print_single_line(&shape, p))
            .unwrap_or_else(|| self.print_multi_line(shape, printer))
    }

    fn leftmost_token(&self) -> TextRange {
        self.open_bracket.span()
    }

    fn rightmost_token(&self) -> TextRange {
        self.ascription
            .as_ref()
            .map(|(_, ty)| ty.rightmost_token())
            .unwrap_or_else(|| self.close_bracket.span())
    }
}

#[derive(Debug)]
pub struct ArrayPatternElement {
    pub rest: Option<t::DotDot>,
    pub pattern: Option<MatchPattern>,
}

impl FromCST for ArrayPatternElement {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::ARRAY_PATTERN_ELEMENT)?;
        let mut it = SyntaxNodeIter::new(&node);
        let rest = it
            .next_if_kind(SyntaxKind::DOT_DOT)
            .map(t::DotDot::from_cst)
            .transpose()?;
        let pattern = it.next().map(MatchPattern::from_cst).transpose()?;
        it.expect_end()?;
        Ok(Self { rest, pattern })
    }
}

impl Printable for ArrayPatternElement {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        if let Some(rest) = &self.rest {
            printer.print_raw_token(rest);
        }
        if let Some(pattern) = &self.pattern {
            if let Some(rest) = &self.rest {
                let (_, rest_trailing) = printer.trivia.get_for_range_split(rest.span());
                print_trivia_squished_spaced(printer, rest_trailing, true, true);
                let pattern_leading = printer.trivia.get_leading_for_element(pattern);
                print_trivia_squished_spaced(printer, pattern_leading, false, true);
            }
            printer.print(pattern, shape)
        } else {
            PrintInfo::default_single_line()
        }
    }

    fn leftmost_token(&self) -> TextRange {
        self.rest
            .as_ref()
            .map(super::tokens::Token::span)
            .or_else(|| self.pattern.as_ref().map(Printable::leftmost_token))
            .unwrap_or(TextRange::empty(0.into()))
    }

    fn rightmost_token(&self) -> TextRange {
        self.pattern
            .as_ref()
            .map(Printable::rightmost_token)
            .or_else(|| self.rest.as_ref().map(super::tokens::Token::span))
            .unwrap_or(TextRange::empty(0.into()))
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
            let left_trailing = if i == 0 {
                printer.trivia.get_trailing_for_element(&*self.first)
            } else {
                printer.trivia.get_trailing_for_element(&self.rest[i - 1].1)
            };
            print_trivia_squished_spaced(printer, left_trailing, true, false);
            let (colon_leading, colon_trailing) = printer.trivia.get_for_range_split(colon.span());
            print_trivia_squished_spaced(printer, colon_leading, true, false);
            printer.print_raw_token(colon);
            printer.print_str(" ");
            // Preserve trivia between `:` and the next pattern (block
            // comments, etc.) — mirrors the trivia handling on let-stmt
            // type annotations.
            print_trivia_squished_spaced(printer, colon_trailing, false, true);
            let pat_leading = printer.trivia.get_leading_for_element(pat);
            print_trivia_squished_spaced(printer, pat_leading, false, true);
            info.multi_lined |= printer.print(pat, shape.clone()).multi_lined;
            // Trailing trivia before the next `:` is emitted at the start of
            // the next iteration. The last link's trailing trivia belongs to
            // the surrounding context.
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
