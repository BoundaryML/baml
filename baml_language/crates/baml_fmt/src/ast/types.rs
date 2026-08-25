//! Formatting for generated type syntax nodes.

use baml_db::baml_compiler_syntax::{
    SyntaxElement, SyntaxKind, SyntaxToken, ast as raw_ast, validated::Validated,
};
use rowan::{TextRange, ast::AstNode as _};

use crate::{
    ast::Token,
    printer::{PrintInfo, PrintMultiLine, Printable, Printer, Shape},
    trivia_classifier::TriviaSliceExt,
};

trait TryPrintSingleLine {
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo>;
}

#[derive(Clone)]
struct RawToken(SyntaxToken);

impl Token for RawToken {
    fn span(&self) -> TextRange {
        self.0.text_range()
    }
}

#[derive(Clone)]
enum GeneratedTypeArg {
    Type(raw_ast::TypeExpr),
    Associated(raw_ast::AssociatedTypeDecl),
}

impl Printable for GeneratedTypeArg {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            Self::Type(ty) => ty.print(shape, printer),
            Self::Associated(binding) => {
                let name = binding.name().expect("validated associated type name");
                let equals = binding
                    .equals_token()
                    .expect("validated associated type equals");
                let ty = binding
                    .default_or_binding()
                    .expect("validated associated type binding");
                printer.print_input_range(name.text_range());
                let (_, equals_trailing) = printer.trivia.get_for_range_split(equals.text_range());
                printer.print_str(" = ");
                printer.print_trivia_squished(equals_trailing);
                let leading = printer.trivia.get_leading_for_element(&ty);
                printer.print_trivia_squished(leading);
                ty.print(shape, printer)
            }
        }
    }

    fn leftmost_token(&self) -> TextRange {
        match self {
            Self::Type(ty) => ty.leftmost_token(),
            Self::Associated(binding) => binding
                .name()
                .expect("validated associated type name")
                .text_range(),
        }
    }

    fn rightmost_token(&self) -> TextRange {
        match self {
            Self::Type(ty) => ty.rightmost_token(),
            Self::Associated(binding) => binding
                .default_or_binding()
                .expect("validated associated type binding")
                .rightmost_token(),
        }
    }
}

struct GeneratedTypeArgs {
    open_angle: RawToken,
    args: Vec<GeneratedTypeArg>,
    commas: Vec<RawToken>,
    close_angle: RawToken,
}

struct GeneratedFunctionType {
    open_paren: RawToken,
    params: Vec<(raw_ast::FunctionTypeParam, Option<RawToken>)>,
    close_paren: RawToken,
    arrow: RawToken,
    return_type: raw_ast::TypeExpr,
    throws: Option<raw_ast::ThrowsClause>,
}

struct GeneratedParenType {
    open_paren: RawToken,
    ty: raw_ast::TypeExpr,
    close_paren: RawToken,
    suffix: Vec<SyntaxElement>,
}

impl TryPrintSingleLine for GeneratedTypeArgs {
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        printer.print_raw_token(&self.open_angle);
        let (_, open_trailing) = printer.trivia.get_for_range_split(self.open_angle.span());
        printer.try_print_trivia_single_line_squished(open_trailing)?;

        for (index, arg) in self.args.iter().enumerate() {
            if index > 0 {
                if let Some(comma) = self.commas.get(index - 1) {
                    let (leading, trailing) = printer.trivia.get_for_range_split(comma.span());
                    printer.try_print_trivia_single_line_squished(leading)?;
                    printer.print_raw_token(comma);
                    printer.try_print_trivia_single_line_squished(trailing)?;
                } else {
                    printer.print_str(",");
                }
                printer.print_str(" ");
            }
            let (leading, trailing) = printer.trivia.get_for_element(arg);
            printer.try_print_trivia_single_line_squished(leading)?;
            if printer
                .print(arg, Shape::unlimited_single_line())
                .multi_lined
            {
                return None;
            }
            printer.try_print_trivia_single_line_squished(trailing)?;
        }

        let (close_leading, _) = printer.trivia.get_for_range_split(self.close_angle.span());
        printer.try_print_trivia_single_line_squished(close_leading)?;
        printer.print_raw_token(&self.close_angle);
        (printer.output.len() <= shape.width).then(PrintInfo::default_single_line)
    }
}

impl PrintMultiLine for GeneratedTypeArgs {
    fn print_multi_line(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let inner_indent = shape.indent + printer.config.indent_width;
        let inner_shape = Shape {
            width: printer.config.line_width.saturating_sub(inner_indent),
            indent: inner_indent,
            first_line_offset: 0,
        };
        printer.print_raw_token(&self.open_angle);
        printer.print_trivia_all_trailing_for(self.open_angle.span());
        if self.args.is_empty() {
            printer.print_raw_token(&self.close_angle);
            return PrintInfo::default_single_line();
        }
        printer.print_newline();
        for (index, arg) in self.args.iter().enumerate() {
            let (leading, trailing) = printer.trivia.get_for_element(arg);
            printer.print_trivia_with_newline(leading.trim_blanks(), inner_indent);
            printer.print_spaces(inner_indent);
            printer.print(arg, inner_shape.clone());
            if index + 1 < self.args.len() {
                let _ = printer.try_print_trivia_single_line_squished(trailing);
                if let Some(comma) = self.commas.get(index) {
                    let (comma_leading, comma_trailing) =
                        printer.trivia.get_for_range_split(comma.span());
                    let _ = printer.try_print_trivia_single_line_squished(comma_leading);
                    printer.print_raw_token(comma);
                    printer.print_trivia_trailing(comma_trailing);
                } else {
                    printer.print_str(",");
                }
            } else {
                printer.print_trivia_trailing(trailing);
            }
            printer.print_newline();
        }
        let (close_leading, _) = printer.trivia.get_for_range_split(self.close_angle.span());
        printer.print_trivia_with_newline(close_leading.trim_blanks(), inner_indent);
        printer.print_spaces(shape.indent);
        printer.print_raw_token(&self.close_angle);
        PrintInfo::default_multi_lined()
    }
}

impl Printable for GeneratedTypeArgs {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|sub| self.try_print_single_line(&shape, sub))
            .unwrap_or_else(|| self.print_multi_line(shape, printer))
    }

    fn leftmost_token(&self) -> TextRange {
        self.open_angle.span()
    }

    fn rightmost_token(&self) -> TextRange {
        self.close_angle.span()
    }
}

fn non_trivia_elements(
    node: &rowan::SyntaxNode<baml_db::baml_compiler_syntax::BamlLanguage>,
) -> Vec<SyntaxElement> {
    node.children_with_tokens()
        .filter(|element| !element.kind().is_trivia())
        .collect()
}

fn element_leftmost(element: &SyntaxElement) -> TextRange {
    match element {
        rowan::NodeOrToken::Token(token) => token.text_range(),
        rowan::NodeOrToken::Node(node) => node
            .descendants_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .find(|token| !token.kind().is_trivia())
            .expect("validated syntax node")
            .text_range(),
    }
}

fn element_rightmost(element: &SyntaxElement) -> TextRange {
    match element {
        rowan::NodeOrToken::Token(token) => token.text_range(),
        rowan::NodeOrToken::Node(node) => node
            .descendants_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .filter(|token| !token.kind().is_trivia())
            .last()
            .expect("validated syntax node")
            .text_range(),
    }
}

fn type_gap_has_space(previous: SyntaxKind, next: SyntaxKind) -> bool {
    matches!(
        previous,
        SyntaxKind::PIPE
            | SyntaxKind::COMMA
            | SyntaxKind::ARROW
            | SyntaxKind::KW_AS
            | SyntaxKind::KW_THROWS
    ) || matches!(
        next,
        SyntaxKind::PIPE
            | SyntaxKind::ARROW
            | SyntaxKind::KW_AS
            | SyntaxKind::THROWS_CLAUSE
            | SyntaxKind::ATTRIBUTE
    )
}

fn type_elements_attach(_previous: SyntaxKind, next: SyntaxKind) -> bool {
    matches!(
        next,
        SyntaxKind::TYPE_ARGS
            | SyntaxKind::QUESTION
            | SyntaxKind::L_BRACKET
            | SyntaxKind::R_BRACKET
            | SyntaxKind::DOT
    )
}

fn try_print_type_gap(
    previous: &SyntaxElement,
    next: &SyntaxElement,
    printer: &mut Printer,
) -> Option<()> {
    let (_, trailing) = printer
        .trivia
        .get_for_range_split(element_rightmost(previous));
    let (leading, _) = printer.trivia.get_for_range_split(element_leftmost(next));
    let mut trivia_len = printer.try_print_trivia_single_line_squished(trailing)?;
    trivia_len += printer.try_print_trivia_single_line_squished(leading)?;
    if trivia_len == 0 && type_gap_has_space(previous.kind(), next.kind()) {
        printer.print_str(" ");
    }
    Some(())
}

fn print_type_gap(
    previous: &SyntaxElement,
    next: &SyntaxElement,
    indent: usize,
    printer: &mut Printer,
) {
    let (_, trailing) = printer
        .trivia
        .get_for_range_split(element_rightmost(previous));
    let (leading, _) = printer.trivia.get_for_range_split(element_leftmost(next));
    let single_line = trailing
        .iter()
        .chain(leading)
        .all(|trivia| trivia.single_line_len(printer.input).is_some());

    if single_line {
        let trivia_len =
            printer.print_trivia_squished(trailing) + printer.print_trivia_squished(leading);
        if trivia_len == 0 && type_gap_has_space(previous.kind(), next.kind()) {
            printer.print_str(" ");
        }
        return;
    }

    printer.print_trivia_trailing(trailing);
    printer.print_newline();
    printer.print_trivia_with_newline(leading.trim_blanks(), indent);
    printer.print_spaces(indent);
}

fn print_type_element(element: &SyntaxElement, shape: Shape, printer: &mut Printer) -> PrintInfo {
    match element {
        rowan::NodeOrToken::Token(token) => {
            printer.print_input_range(token.text_range());
            PrintInfo::default_single_line()
        }
        rowan::NodeOrToken::Node(node) => match node.kind() {
            SyntaxKind::TYPE_EXPR => printer.print(
                &raw_ast::TypeExpr::cast(node.clone()).expect("checked type expression"),
                shape,
            ),
            SyntaxKind::TYPE_ARGS => printer.print(
                &raw_ast::TypeArgs::cast(node.clone()).expect("checked type arguments"),
                shape,
            ),
            SyntaxKind::FUNCTION_TYPE_PARAM => printer.print(
                &raw_ast::FunctionTypeParam::cast(node.clone())
                    .expect("checked function type parameter"),
                shape,
            ),
            SyntaxKind::THROWS_CLAUSE => printer.print(
                &raw_ast::ThrowsClause::cast(node.clone()).expect("checked throws clause"),
                shape,
            ),
            SyntaxKind::ATTRIBUTE => printer.print(
                &raw_ast::Attribute::cast(node.clone()).expect("checked attribute"),
                shape,
            ),
            _ => {
                printer.print_input_range(node.text_range());
                PrintInfo {
                    multi_lined: printer.input[node.text_range()].contains('\n'),
                }
            }
        },
    }
}

fn try_print_type_elements(
    elements: &[SyntaxElement],
    shape: &Shape,
    printer: &mut Printer,
) -> Option<PrintInfo> {
    let mut multi_lined = false;
    for (index, element) in elements.iter().enumerate() {
        if let Some(previous) = index.checked_sub(1).and_then(|index| elements.get(index)) {
            if !type_elements_attach(previous.kind(), element.kind()) {
                try_print_type_gap(previous, element, printer)?;
            }
        }
        multi_lined |=
            print_type_element(element, Shape::unlimited_single_line(), printer).multi_lined;
        if multi_lined || printer.len() > shape.width {
            return None;
        }
    }
    Some(PrintInfo::default_single_line())
}

impl GeneratedFunctionType {
    fn new(elements: &[SyntaxElement]) -> Option<Self> {
        let open_paren = elements.first()?.as_token()?;
        if open_paren.kind() != SyntaxKind::L_PAREN {
            return None;
        }
        let arrow_index = elements
            .iter()
            .position(|element| element.kind() == SyntaxKind::ARROW)?;
        let close_paren = elements.get(arrow_index.checked_sub(1)?)?.as_token()?;
        if close_paren.kind() != SyntaxKind::R_PAREN {
            return None;
        }

        let mut params = Vec::new();
        for element in &elements[1..arrow_index - 1] {
            match element {
                rowan::NodeOrToken::Node(node)
                    if node.kind() == SyntaxKind::FUNCTION_TYPE_PARAM =>
                {
                    params.push((raw_ast::FunctionTypeParam::cast(node.clone())?, None));
                }
                rowan::NodeOrToken::Token(token) if token.kind() == SyntaxKind::COMMA => {
                    params.last_mut()?.1 = Some(RawToken(token.clone()));
                }
                _ => return None,
            }
        }

        let arrow = elements.get(arrow_index)?.as_token()?;
        let return_type = elements.get(arrow_index + 1)?.as_node()?;
        let return_type = raw_ast::TypeExpr::cast(return_type.clone())?;
        let throws = elements
            .get(arrow_index + 2)
            .and_then(SyntaxElement::as_node)
            .and_then(|node| raw_ast::ThrowsClause::cast(node.clone()));
        let consumed = arrow_index + 2 + usize::from(throws.is_some());
        if consumed != elements.len() {
            return None;
        }

        Some(Self {
            open_paren: RawToken(open_paren.clone()),
            params,
            close_paren: RawToken(close_paren.clone()),
            arrow: RawToken(arrow.clone()),
            return_type,
            throws,
        })
    }
}

impl TryPrintSingleLine for GeneratedFunctionType {
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        printer.print_raw_token(&self.open_paren);
        let (_, open_trailing) = printer.trivia.get_for_range_split(self.open_paren.span());
        printer.try_print_trivia_single_line_squished(open_trailing)?;

        for (index, (param, comma)) in self.params.iter().enumerate() {
            if printer.len() > shape.width {
                return None;
            }
            let (leading, trailing) = printer.trivia.get_for_element(param);
            printer.try_print_trivia_single_line_squished(leading)?;
            if printer
                .print(param, Shape::unlimited_single_line())
                .multi_lined
            {
                return None;
            }
            printer.try_print_trivia_single_line_squished(trailing)?;
            if index + 1 < self.params.len() {
                if let Some(comma) = comma {
                    let (leading, trailing) = printer.trivia.get_for_range_split(comma.span());
                    printer.try_print_trivia_single_line_squished(leading)?;
                    printer.print_raw_token(comma);
                    printer.try_print_trivia_single_line_squished(trailing)?;
                } else {
                    printer.print_str(",");
                }
                printer.print_str(" ");
            } else if let Some(comma) = comma {
                let (leading, trailing) = printer.trivia.get_for_range_split(comma.span());
                printer.try_print_trivia_single_line_squished(leading)?;
                printer.try_print_trivia_single_line_squished(trailing)?;
            }
        }

        let (close_leading, _) = printer.trivia.get_for_range_split(self.close_paren.span());
        printer.try_print_trivia_single_line_squished(close_leading)?;
        printer.print_raw_token(&self.close_paren);
        printer.print_str(" ");
        printer.print_raw_token(&self.arrow);
        printer.print_str(" ");
        if printer
            .print(&self.return_type, Shape::unlimited_single_line())
            .multi_lined
        {
            return None;
        }
        if let Some(throws) = &self.throws {
            printer.print_str(" ");
            if printer
                .print(throws, Shape::unlimited_single_line())
                .multi_lined
            {
                return None;
            }
        }

        (printer.len() <= shape.width).then(PrintInfo::default_single_line)
    }
}

impl PrintMultiLine for GeneratedFunctionType {
    fn print_multi_line(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let inner_shape = Shape {
            width: shape.width.saturating_sub(printer.config.indent_width),
            indent: shape.indent + printer.config.indent_width,
            first_line_offset: 0,
        };
        printer.print_raw_token(&self.open_paren);
        printer.print_trivia_all_trailing_for(self.open_paren.span());
        printer.print_newline();

        for (param, comma) in &self.params {
            printer.print_trivia_all_leading_with_newline_for(
                param.leftmost_token(),
                inner_shape.indent,
            );
            printer.print_spaces(inner_shape.indent);
            printer.print(param, inner_shape.clone());
            if let Some(comma) = comma {
                printer.print_raw_token(comma);
                printer.print_trivia_all_trailing_for(comma.span());
            } else {
                printer.print_str(",");
                printer.print_trivia_all_trailing_for(param.rightmost_token());
            }
            printer.print_newline();
        }

        let (close_leading, _) = printer.trivia.get_for_range_split(self.close_paren.span());
        printer.print_trivia_with_newline(close_leading.trim_blanks(), inner_shape.indent);
        printer.print_spaces(shape.indent);
        printer.print_raw_token(&self.close_paren);
        printer.print_str(" ");
        printer.print_raw_token(&self.arrow);
        printer.print_str(" ");
        printer.print(&self.return_type, shape.clone());
        if let Some(throws) = &self.throws {
            printer.print_str(" ");
            printer.print(throws, shape);
        }
        PrintInfo::default_multi_lined()
    }
}

impl Printable for GeneratedFunctionType {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|sub| self.try_print_single_line(&shape, sub))
            .unwrap_or_else(|| self.print_multi_line(shape, printer))
    }

    fn leftmost_token(&self) -> TextRange {
        self.open_paren.span()
    }

    fn rightmost_token(&self) -> TextRange {
        self.throws.as_ref().map_or_else(
            || self.return_type.rightmost_token(),
            Printable::rightmost_token,
        )
    }
}

impl GeneratedParenType {
    fn new(elements: &[SyntaxElement]) -> Option<Self> {
        let open_paren = elements.first()?.as_token()?;
        if open_paren.kind() != SyntaxKind::L_PAREN {
            return None;
        }
        let close_index = elements
            .iter()
            .position(|element| element.kind() == SyntaxKind::R_PAREN)?;
        if close_index != 2 {
            return None;
        }
        let param = elements.get(1)?.as_node()?;
        let param = raw_ast::FunctionTypeParam::cast(param.clone())?;
        if param.colon_token().is_some() {
            return None;
        }
        let close_paren = elements.get(close_index)?.as_token()?;
        let suffix = elements[close_index + 1..].to_vec();
        if suffix.iter().any(|element| {
            !matches!(
                element.kind(),
                SyntaxKind::QUESTION | SyntaxKind::L_BRACKET | SyntaxKind::R_BRACKET
            )
        }) {
            return None;
        }
        Some(Self {
            open_paren: RawToken(open_paren.clone()),
            ty: param.ty()?,
            close_paren: RawToken(close_paren.clone()),
            suffix,
        })
    }

    fn print_suffix(&self, printer: &mut Printer) {
        for element in &self.suffix {
            print_type_element(element, Shape::unlimited_single_line(), printer);
        }
    }
}

impl TryPrintSingleLine for GeneratedParenType {
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        printer.print_raw_token(&self.open_paren);
        let (_, open_trailing) = printer.trivia.get_for_range_split(self.open_paren.span());
        printer.try_print_trivia_single_line_squished(open_trailing)?;
        let (leading, trailing) = printer.trivia.get_for_element(&self.ty);
        printer.try_print_trivia_single_line_squished(leading)?;
        if printer
            .print(&self.ty, Shape::unlimited_single_line())
            .multi_lined
        {
            return None;
        }
        printer.try_print_trivia_single_line_squished(trailing)?;
        let (close_leading, _) = printer.trivia.get_for_range_split(self.close_paren.span());
        printer.try_print_trivia_single_line_squished(close_leading)?;
        printer.print_raw_token(&self.close_paren);
        self.print_suffix(printer);
        (printer.len() <= shape.width).then(PrintInfo::default_single_line)
    }
}

impl PrintMultiLine for GeneratedParenType {
    fn print_multi_line(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let inner_indent = shape.indent + printer.config.indent_width;
        printer.print_raw_token(&self.open_paren);
        printer.print_trivia_all_trailing_for(self.open_paren.span());
        printer.print_newline();
        printer.print_standalone_with_trivia(&self.ty, inner_indent);
        printer.print_newline();
        let (close_leading, _) = printer.trivia.get_for_range_split(self.close_paren.span());
        printer.print_trivia_with_newline(close_leading.trim_blanks(), inner_indent);
        printer.print_spaces(shape.indent);
        printer.print_raw_token(&self.close_paren);
        self.print_suffix(printer);
        PrintInfo::default_multi_lined()
    }
}

impl Printable for GeneratedParenType {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|sub| self.try_print_single_line(&shape, sub))
            .unwrap_or_else(|| self.print_multi_line(shape, printer))
    }

    fn leftmost_token(&self) -> TextRange {
        self.open_paren.span()
    }

    fn rightmost_token(&self) -> TextRange {
        self.suffix
            .last()
            .map_or_else(|| self.close_paren.span(), element_rightmost)
    }
}

fn print_union_type(elements: &[SyntaxElement], shape: &Shape, printer: &mut Printer) -> PrintInfo {
    let inner_indent = shape.indent + printer.config.indent_width;
    let mut start = 0;
    let mut first = true;
    while start < elements.len() {
        let pipe = elements[start..]
            .iter()
            .position(|element| element.kind() == SyntaxKind::PIPE)
            .map(|offset| start + offset);
        let end = pipe.unwrap_or(elements.len());
        if !first {
            let pipe = &elements[start - 1];
            printer.print_newline();
            let (leading, trailing) = printer.trivia.get_for_range_split(element_leftmost(pipe));
            printer.print_trivia_with_newline(leading.trim_blanks(), inner_indent);
            printer.print_spaces(inner_indent);
            print_type_element(pipe, shape.clone(), printer);
            let next = &elements[start];
            let (next_leading, _) = printer.trivia.get_for_range_split(element_leftmost(next));
            let mut trivia_len = printer.print_trivia_squished(trailing);
            trivia_len += printer.print_trivia_squished(next_leading);
            if trivia_len == 0 {
                printer.print_str(" ");
            }
        }
        for (index, element) in elements[start..end].iter().enumerate() {
            if index > 0 {
                let previous = &elements[start + index - 1];
                print_type_gap(previous, element, inner_indent, printer);
            }
            print_type_element(element, shape.clone(), printer);
        }
        if pipe.is_some()
            && let Some(last) = elements.get(end.saturating_sub(1))
        {
            let (_, trailing) = printer.trivia.get_for_range_split(element_rightmost(last));
            printer.print_trivia_trailing(trailing);
        }
        first = false;
        let Some(pipe) = pipe else { break };
        start = pipe + 1;
    }
    PrintInfo::default_multi_lined()
}

fn print_type_core(elements: &[SyntaxElement], shape: &Shape, printer: &mut Printer) -> PrintInfo {
    if let Some(function) = GeneratedFunctionType::new(elements) {
        return function.print(shape.clone(), printer);
    }
    if let Some(paren) = GeneratedParenType::new(elements) {
        return paren.print(shape.clone(), printer);
    }
    if let Some(info) = printer.try_sub_printer(|sub| try_print_type_elements(elements, shape, sub))
    {
        return info;
    }
    if elements
        .iter()
        .any(|element| element.kind() == SyntaxKind::PIPE)
    {
        return print_union_type(elements, shape, printer);
    }
    let mut multi_lined = false;
    for (index, element) in elements.iter().enumerate() {
        if index > 0 {
            let previous = &elements[index - 1];
            if !type_elements_attach(previous.kind(), element.kind()) {
                print_type_gap(
                    previous,
                    element,
                    shape.indent + printer.config.indent_width,
                    printer,
                );
            }
        }
        multi_lined |= print_type_element(element, shape.clone(), printer).multi_lined;
    }
    PrintInfo { multi_lined }
}

fn try_print_constrained_type(
    base: &[SyntaxElement],
    attrs: &[raw_ast::Attribute],
    shape: &Shape,
    printer: &mut Printer,
) -> Option<PrintInfo> {
    if print_type_core(base, &Shape::unlimited_single_line(), printer).multi_lined {
        return None;
    }
    let last = base.last()?;
    let (_, trailing) = printer.trivia.get_for_range_split(element_rightmost(last));
    let mut trivia_len = printer.try_print_trivia_single_line_squished(trailing)?;
    for (index, attr) in attrs.iter().enumerate() {
        let (leading, trailing) = printer.trivia.get_for_element(attr);
        trivia_len += printer.try_print_trivia_single_line_squished(leading)?;
        if trivia_len == 0 {
            printer.print_str(" ");
        }
        if printer
            .print(attr, Shape::unlimited_single_line())
            .multi_lined
        {
            return None;
        }
        if index + 1 < attrs.len() {
            trivia_len = printer.try_print_trivia_single_line_squished(trailing)?;
        }
    }
    (printer.len() <= shape.width).then(PrintInfo::default_single_line)
}

fn print_constrained_type(
    base: &[SyntaxElement],
    attrs: &[raw_ast::Attribute],
    shape: &Shape,
    printer: &mut Printer,
) -> PrintInfo {
    if let Some(info) =
        printer.try_sub_printer(|sub| try_print_constrained_type(base, attrs, shape, sub))
    {
        return info;
    }

    let base_info = print_type_core(base, shape, printer);
    let trailing_count = base.last().map_or(0, |last| {
        printer
            .print_trivia_all_trailing_for(element_rightmost(last))
            .0
    });
    if !base_info.multi_lined && trailing_count == 0 && attrs.len() == 1 {
        let remaining = printer.current_line_remaining_width();
        let prefix_len = attrs[0]
            .syntax()
            .children_with_tokens()
            .take_while(|element| element.kind() != SyntaxKind::ATTRIBUTE_ARGS)
            .filter(|element| !element.kind().is_trivia())
            .map(|element| usize::from(element.text_range().len()))
            .sum::<usize>();
        if prefix_len < remaining {
            printer.print_str(" ");
            let attr_shape = Shape {
                width: remaining.saturating_sub(1),
                indent: shape.indent,
                first_line_offset: printer.current_line_len().saturating_sub(shape.indent),
            };
            return printer.print(&attrs[0], attr_shape);
        }
    }

    let attr_indent = shape.indent + printer.config.indent_width;
    let attr_shape = Shape::standalone(printer.config.line_width, attr_indent);
    for attr in attrs {
        printer.print_newline();
        printer.print_spaces(attr_indent);
        printer.print(attr, attr_shape.clone());
    }
    PrintInfo::default_multi_lined()
}

fn print_generated_type(ty: &raw_ast::TypeExpr, shape: &Shape, printer: &mut Printer) -> PrintInfo {
    let elements = non_trivia_elements(ty.syntax());
    if let Some(attr_index) = elements
        .iter()
        .position(|element| element.kind() == SyntaxKind::ATTRIBUTE)
        .filter(|&index| {
            elements[index..]
                .iter()
                .all(|element| element.kind() == SyntaxKind::ATTRIBUTE)
        })
    {
        let attrs = elements[attr_index..]
            .iter()
            .filter_map(SyntaxElement::as_node)
            .filter_map(|node| raw_ast::Attribute::cast(node.clone()))
            .collect::<Vec<_>>();
        return print_constrained_type(&elements[..attr_index], &attrs, shape, printer);
    }
    print_type_core(&elements, shape, printer)
}

impl Printable for raw_ast::FunctionTypeParam {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let ty = self.ty().expect("validated function type parameter");
        let name = self
            .syntax()
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .filter(|token| !token.kind().is_trivia())
            .find(|token| matches!(token.kind(), SyntaxKind::WORD | SyntaxKind::KW_CLIENT));
        if let Some(name) = name.filter(|_| self.colon_token().is_some()) {
            printer.print_input_range(name.text_range());
            if let Some(question) = self.question_token() {
                printer.print_input_range(question.text_range());
            }
            if let Some(colon) = self.colon_token() {
                printer.print_input_range(colon.text_range());
            } else {
                printer.print_str(":");
            }
            printer.print_str(" ");
        }
        printer.print(&ty, shape)
    }

    fn leftmost_token(&self) -> TextRange {
        self.syntax()
            .descendants_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .find(|token| !token.kind().is_trivia())
            .expect("validated function type parameter")
            .text_range()
    }

    fn rightmost_token(&self) -> TextRange {
        self.ty()
            .expect("validated function type parameter")
            .rightmost_token()
    }
}

impl GeneratedTypeArgs {
    fn new(args: &raw_ast::TypeArgs) -> Self {
        let open_angle = RawToken(args.less_token().expect("validated type arguments"));
        let close_angle = RawToken(args.greater_token().expect("validated type arguments"));
        let mut values = Vec::new();
        for element in args
            .syntax()
            .children_with_tokens()
            .filter(|element| !element.kind().is_trivia())
        {
            match element {
                rowan::NodeOrToken::Node(node) => match node.kind() {
                    SyntaxKind::TYPE_EXPR => values.push(GeneratedTypeArg::Type(
                        raw_ast::TypeExpr::cast(node).expect("checked type expression"),
                    )),
                    SyntaxKind::ASSOCIATED_TYPE_DECL => {
                        values.push(GeneratedTypeArg::Associated(
                            raw_ast::AssociatedTypeDecl::cast(node)
                                .expect("checked associated type binding"),
                        ));
                    }
                    _ => unreachable!("validated type argument node"),
                },
                rowan::NodeOrToken::Token(_) => {}
            }
        }
        let commas = args.comma_tokens().map(RawToken).collect();
        Self {
            open_angle,
            args: values,
            commas,
            close_angle,
        }
    }
}

pub(super) trait TypeLayout {
    fn multi_line_is_indented(&self) -> bool;
}

impl TypeLayout for Validated<'_, raw_ast::TypeExpr> {
    fn multi_line_is_indented(&self) -> bool {
        let ty = raw_ast::TypeExpr::cast(self.syntax().clone()).expect("validated type expression");
        type_multi_line_is_indented(&ty)
    }
}

impl Printable for Validated<'_, raw_ast::TypeExpr> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let ty = raw_ast::TypeExpr::cast(self.syntax().clone()).expect("validated type expression");
        print_generated_type(&ty, &shape, printer)
    }

    fn leftmost_token(&self) -> TextRange {
        self.first_token_range()
    }

    fn rightmost_token(&self) -> TextRange {
        self.last_token_range()
    }
}

impl Printable for raw_ast::TypeExpr {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        print_generated_type(self, &shape, printer)
    }

    fn leftmost_token(&self) -> TextRange {
        self.syntax()
            .descendants_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .find(|token| !token.kind().is_trivia())
            .expect("validated type expression")
            .text_range()
    }

    fn rightmost_token(&self) -> TextRange {
        self.syntax()
            .descendants_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .filter(|token| !token.kind().is_trivia())
            .last()
            .expect("validated type expression")
            .text_range()
    }
}

fn type_multi_line_is_indented(ty: &raw_ast::TypeExpr) -> bool {
    let elements = non_trivia_elements(ty.syntax());
    elements.iter().any(|element| {
        matches!(
            element.kind(),
            SyntaxKind::PIPE | SyntaxKind::ATTRIBUTE | SyntaxKind::ARROW
        )
    }) || elements
        .first()
        .is_some_and(|element| element.kind() == SyntaxKind::WORD)
}

impl Printable for raw_ast::TypeArgs {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        GeneratedTypeArgs::new(self).print(shape, printer)
    }

    fn leftmost_token(&self) -> TextRange {
        self.less_token()
            .expect("validated type arguments")
            .text_range()
    }

    fn rightmost_token(&self) -> TextRange {
        self.greater_token()
            .expect("validated type arguments")
            .text_range()
    }
}

#[cfg(test)]
mod tests {
    use baml_db::{
        baml_compiler_parser::parse_green,
        baml_compiler_syntax::{SyntaxKind, SyntaxNode, ast as syntax_ast},
    };

    use super::*;

    fn function_type_param(source: &str, index: usize) -> syntax_ast::FunctionTypeParam {
        let (db, file) = crate::single_file_db("test.baml", source);
        let parsed = parse_green(&db, file);
        let syntax_tree = SyntaxNode::new_root(parsed);
        let node = syntax_tree
            .descendants()
            .filter(|node| node.kind() == SyntaxKind::FUNCTION_TYPE_PARAM)
            .nth(index)
            .expect("expected FUNCTION_TYPE_PARAM");

        syntax_ast::FunctionTypeParam::cast(node).expect("expected FunctionTypeParam to parse")
    }

    #[test]
    fn function_type_param_optional_name_round_trips() {
        let source = "type Searcher = (name?: string) -> int\n";
        let param = function_type_param(source, 0);
        assert!(param.question_token().is_some());
        assert!(param.colon_token().is_some());

        let formatted = crate::format(source, &crate::FormatOptions::default())
            .expect("formatter should print optional function type params");
        assert!(formatted.contains("(name?: string) -> int"));
        assert_eq!(
            crate::format(&formatted, &crate::FormatOptions::default())
                .expect("formatter should be idempotent"),
            formatted
        );
    }

    #[test]
    fn function_type_param_optional_name_with_optional_type_round_trips() {
        let source = "type Searcher = (name?: (string)?) -> int\n";
        let param = function_type_param(source, 0);

        assert!(param.question_token().is_some());
        assert!(
            param
                .ty()
                .expect("validated parameter type")
                .question_tokens()
                .next()
                .is_some()
        );

        let formatted = crate::format(source, &crate::FormatOptions::default())
            .expect("formatter should disambiguate optional parameter and optional type");
        assert!(formatted.contains("name?:"));
        assert!(formatted.contains("string"));
        assert_eq!(
            crate::format(&formatted, &crate::FormatOptions::default())
                .expect("formatter should be idempotent"),
            formatted
        );
    }
}
