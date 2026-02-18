use baml_db::baml_compiler_syntax::{SyntaxElement, SyntaxKind, SyntaxNodeExt};
use rowan::{TextRange, TextSize};

use crate::{
    ast::{FromCST, KnownKind, StrongAstError, SyntaxNodeIter, Token, tokens as t},
    printer::{PrintInfo, PrintMultiLine, Printable, Printer, Shape},
};

/// Corresponds to a [`SyntaxKind::BLOCK_ATTRIBUTE`] node.
#[derive(Debug)]
pub struct BlockAttribute {
    pub atat: t::AtAt,
    pub name: AttributeName,
    pub args: Option<AttributeArgs>,
}

impl FromCST for BlockAttribute {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::BLOCK_ATTRIBUTE)?;

        let mut it = SyntaxNodeIter::new(&node);

        // @@
        let atat = it.expect_parse()?;

        // name (can have dots like @stream.done)
        let name = AttributeName::take(&mut it)?;

        let args = it.next().map(AttributeArgs::from_cst).transpose()?;

        it.expect_end()?;

        Ok(BlockAttribute { atat, name, args })
    }
}

impl KnownKind for BlockAttribute {
    fn kind() -> SyntaxKind {
        SyntaxKind::BLOCK_ATTRIBUTE
    }
}

impl Printable for BlockAttribute {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let mut multi_lined = false;
        printer.print_raw_token(&self.atat);
        multi_lined |= printer.print(&self.name, shape.clone()).multi_lined;
        if let Some(args) = &self.args {
            multi_lined |= printer.print(args, shape).multi_lined;
        }
        PrintInfo { multi_lined }
    }
    fn leftmost_token(&self) -> TextRange {
        self.atat.span()
    }
    fn rightmost_token(&self) -> TextRange {
        if let Some(args) = &self.args {
            args.rightmost_token()
        } else {
            self.name.rightmost_token()
        }
    }
}

/// Corresponds to a [`SyntaxKind::ATTRIBUTE`] node.
#[derive(Debug)]
pub struct Attribute {
    pub at: t::At,
    pub name: AttributeName,
    pub args: Option<AttributeArgs>,
}

impl FromCST for Attribute {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::ATTRIBUTE)?;

        let mut it = SyntaxNodeIter::new(&node);

        // @
        let at = it.expect_parse()?;

        // name (can have dots like @stream.done)
        let name_first = it.expect_next("attribute name part")?;
        let name_first = AttributeNamePart::from_cst(name_first)?;
        let mut name_rest = Vec::new();
        let args = loop {
            let Some(elem) = it.next() else {
                break None;
            };
            match elem.kind() {
                SyntaxKind::DOT => {
                    let dot = StrongAstError::assert_is_token(elem)?;
                    let name = it.expect_next("attribute name part")?;
                    let name = AttributeNamePart::from_cst(name)?;
                    name_rest.push((t::Dot::new_from_span(dot.text_range()), name));
                }
                SyntaxKind::ATTRIBUTE_ARGS => {
                    let args = AttributeArgs::from_cst(elem)?;
                    break Some(args);
                }
                _ => {
                    return Err(StrongAstError::UnexpectedKindDesc {
                        expected_desc: "DOT or ATTRIBUTE_ARGS".into(),
                        found: elem.kind(),
                        at: elem.text_range(),
                    });
                }
            }
        };

        it.expect_end()?;

        let name = AttributeName {
            first: name_first,
            rest: name_rest,
        };

        Ok(Attribute { at, name, args })
    }
}

impl KnownKind for Attribute {
    fn kind() -> SyntaxKind {
        SyntaxKind::ATTRIBUTE
    }
}

impl Printable for Attribute {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.at);
        printer.print(&self.name, shape.clone());
        if let Some(args) = &self.args {
            printer.print(args, shape)
        } else {
            PrintInfo::default_single_line()
        }
    }
    fn leftmost_token(&self) -> TextRange {
        self.at.span()
    }
    fn rightmost_token(&self) -> TextRange {
        if let Some(args) = &self.args {
            args.rightmost_token()
        } else {
            self.name.rightmost_token()
        }
    }
}

/// Attribute names are not normal paths: they may contain keywords.
#[derive(Debug)]
pub enum AttributeNamePart {
    Word(t::Word),
    Keyword(TextRange),
}

impl FromCST for AttributeNamePart {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let token = StrongAstError::assert_is_token(elem)?;
        match token.kind() {
            SyntaxKind::WORD => Ok(AttributeNamePart::Word(t::Word::new_from_span(
                token.text_range(),
            ))),
            keyword if keyword.is_keyword() => Ok(AttributeNamePart::Keyword(token.text_range())),
            _ => Err(StrongAstError::UnexpectedKindDesc {
                expected_desc: "KEYWORD or WORD".into(),
                found: token.kind(),
                at: token.text_range(),
            }),
        }
    }
}

impl Printable for AttributeNamePart {
    fn print(&self, _shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            AttributeNamePart::Word(word) => printer.print_raw_token(word),
            AttributeNamePart::Keyword(range) => printer.print_input_range(*range),
        }
        PrintInfo::default_single_line()
    }
    fn leftmost_token(&self) -> TextRange {
        match self {
            AttributeNamePart::Word(word) => word.span(),
            AttributeNamePart::Keyword(range) => *range,
        }
    }
    fn rightmost_token(&self) -> TextRange {
        match self {
            AttributeNamePart::Word(word) => word.span(),
            AttributeNamePart::Keyword(range) => *range,
        }
    }
}

/// Attribute names are not normal paths: they may contain keywords.
#[derive(Debug)]
pub struct AttributeName {
    pub first: AttributeNamePart,
    pub rest: Vec<(t::Dot, AttributeNamePart)>,
}

impl AttributeName {
    pub fn take(it: &mut SyntaxNodeIter) -> Result<Self, StrongAstError> {
        let first = it.expect_token("attribute name part")?;
        let first = AttributeNamePart::from_cst(SyntaxElement::Token(first))?;

        let mut rest = Vec::new();
        while let Some(dot) = it.next_if_kind(SyntaxKind::DOT) {
            let dot_token = StrongAstError::assert_is_token(dot)?;
            let part = it.expect_token("attribute name part")?;
            let part = AttributeNamePart::from_cst(SyntaxElement::Token(part))?;
            rest.push((t::Dot::new_from_span(dot_token.text_range()), part));
        }

        Ok(AttributeName { first, rest })
    }
}

impl Printable for AttributeName {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print(&self.first, shape.clone());
        for (dot, part) in &self.rest {
            printer.print_raw_token(dot);
            printer.print(part, shape.clone());
        }
        PrintInfo::default_single_line()
    }
    fn leftmost_token(&self) -> TextRange {
        self.first.leftmost_token()
    }
    fn rightmost_token(&self) -> TextRange {
        self.rest
            .last()
            .map_or(&self.first, |(_, part)| part)
            .rightmost_token()
    }
}

/// Corresponds to a [`SyntaxKind::ATTRIBUTE_ARGS`] node.
#[derive(Debug)]
pub struct AttributeArgs {
    pub open_paren: t::LParen,
    pub args: Vec<(AttributeArg, Option<t::Comma>)>,
    pub close_paren: t::RParen,
}
impl FromCST for AttributeArgs {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::ATTRIBUTE_ARGS)?;

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

            let next = AttributeArg::from_cst(elem)?;
            let comma = it
                .next_if_kind(SyntaxKind::COMMA)
                .map(t::Comma::from_cst)
                .transpose()?;
            args.push((next, comma));
        };

        it.expect_end()?;

        Ok(AttributeArgs {
            open_paren,
            args,
            close_paren,
        })
    }
}

impl KnownKind for AttributeArgs {
    fn kind() -> SyntaxKind {
        SyntaxKind::ATTRIBUTE_ARGS
    }
}

impl PrintMultiLine for AttributeArgs {
    /// Multi-line layout: each argument on its own indented line with trailing comma.
    /// Closing paren on its own line.
    ///
    /// ```baml
    /// (
    ///     "quoted string",
    ///     {{ this > 0 }},
    ///     #"raw string"#,
    /// )
    /// ```
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

impl AttributeArgs {
    /// Should be passed a sub-printer to avoid printing trivia in the outer printer
    /// in the event that the printer is unable to fit the attribute args on a single line.
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        printer.print_raw_token(&self.open_paren);
        let (_, open_trailing) = printer.trivia.get_for_range_split(self.open_paren.span());
        printer.print_trivia_single_line_squished(open_trailing)?;

        for (i, (arg, comma)) in self.args.iter().enumerate() {
            if printer.output.len() > shape.width {
                return None;
            }
            let (arg_leading, arg_trailing) = printer.trivia.get_for_element(arg);
            printer.print_trivia_single_line_squished(arg_leading)?;
            if printer
                .print(arg, Shape::unlimited_single_line())
                .multi_lined
            {
                return None;
            }
            printer.print_trivia_single_line_squished(arg_trailing)?;
            if i + 1 < self.args.len() {
                if let Some(comma) = comma {
                    let (comma_leading, comma_trailing) =
                        printer.trivia.get_for_range_split(comma.span());
                    printer.print_trivia_single_line_squished(comma_leading)?;
                    printer.print_raw_token(comma);
                    printer.print_trivia_single_line_squished(comma_trailing)?;
                } else {
                    printer.print_str(",");
                }
                printer.print_str(" ");
            } else if let Some(comma) = comma {
                // Trailing comma is removed in single-line mode, but we still try the comments.
                let (comma_leading, comma_trailing) =
                    printer.trivia.get_for_range_split(comma.span());
                printer.print_trivia_single_line_squished(comma_leading)?;
                printer.print_trivia_single_line_squished(comma_trailing)?;
            }
        }

        let (close_leading, _) = printer.trivia.get_for_range_split(self.close_paren.span());
        printer.print_trivia_single_line_squished(close_leading)?;
        printer.print_raw_token(&self.close_paren);

        if printer.output.len() > shape.width {
            None
        } else {
            Some(PrintInfo::default_single_line())
        }
    }
}

impl Printable for AttributeArgs {
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

#[derive(Debug)]
pub enum AttributeArg {
    QuotedString(t::QuotedString),
    RawString(t::RawString),
    /// Something like `{{ this > 0 }}`
    ///
    /// TODO: the [`SyntaxKind::EXPR`] node currently just contains unstructured tokens
    AttrExpr(TextRange),
    /// Unquoted strings are single words.
    ///
    /// Historically, multi-word unquoted strings were allowed. This is now an error.
    UnquotedString(t::Word),
}

impl FromCST for AttributeArg {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        match elem.kind() {
            SyntaxKind::STRING_LITERAL => {
                let string = t::QuotedString::from_cst(elem)?;
                Ok(AttributeArg::QuotedString(string))
            }
            SyntaxKind::RAW_STRING_LITERAL => {
                let string = t::RawString::from_cst(elem)?;
                Ok(AttributeArg::RawString(string))
            }
            SyntaxKind::EXPR => {
                let node = StrongAstError::assert_is_node(elem)?;
                let start = node
                    .first_child_token_of_kind(SyntaxKind::L_BRACE)
                    .ok_or_else(|| {
                        StrongAstError::missing(SyntaxKind::L_BRACE, node.text_range())
                    })?;

                Ok(AttributeArg::AttrExpr(TextRange::new(
                    start.text_range().start(),
                    node.text_range().end(),
                )))
            }
            SyntaxKind::UNQUOTED_STRING => {
                let node = StrongAstError::assert_is_node(elem)?;
                let mut it = SyntaxNodeIter::new(&node);
                let word = it.expect_parse()?;
                it.expect_end()?; // multi-word unquoted strings are not valid in the new engine

                Ok(AttributeArg::UnquotedString(word))
            }
            found => Err(StrongAstError::UnexpectedKindDesc {
                expected_desc: "STRING_LITERAL, RAW_STRING_LITERAL, EXPR, or UNQUOTED_STRING"
                    .into(),
                found,
                at: elem.text_range(),
            }),
        }
    }
}

impl Printable for AttributeArg {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            AttributeArg::QuotedString(s) => printer.print(s, shape),
            AttributeArg::RawString(s) => printer.print(s, shape),
            AttributeArg::AttrExpr(range) => {
                printer.print_input_range(*range);
                PrintInfo {
                    multi_lined: printer.input[*range].contains('\n'),
                }
            }
            AttributeArg::UnquotedString(s) => {
                printer.print_raw_token(s);
                PrintInfo::default_single_line()
            }
        }
    }
    fn leftmost_token(&self) -> TextRange {
        match self {
            AttributeArg::QuotedString(s) => s.leftmost_token(),
            AttributeArg::RawString(s) => s.leftmost_token(),
            AttributeArg::AttrExpr(range) => {
                TextRange::new(range.start(), range.start() + TextSize::from(1))
            }
            AttributeArg::UnquotedString(s) => s.span(),
        }
    }
    fn rightmost_token(&self) -> TextRange {
        match self {
            AttributeArg::QuotedString(s) => s.rightmost_token(),
            AttributeArg::RawString(s) => s.rightmost_token(),
            AttributeArg::AttrExpr(range) => {
                TextRange::new(range.end(), range.end() + TextSize::from(1))
            }
            AttributeArg::UnquotedString(s) => s.span(),
        }
    }
}
