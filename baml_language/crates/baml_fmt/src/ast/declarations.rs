use baml_db::baml_compiler_syntax::{
    FromCST, SyntaxElement, SyntaxKind, SyntaxToken, ast as syntax_ast,
    validated::{Validated, ValidatedSyntaxToken},
};
use rowan::{TextRange, ast::AstNode as _};

use super::expressions::FunctionArrowLayout;
use crate::{
    EmittableTrivia,
    ast::{BlockAttribute, Token, tokens as t},
    printer::{PrintInfo, PrintMultiLine, Printable, Printer, Shape},
    trivia_classifier::TriviaSliceExt as _,
};

trait ClassFieldLayout {
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo>;
}

trait EnumVariantLayout {
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo>;
}

trait ConfigArrayLayout {
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo>;
}

impl Printable for Validated<'_, syntax_ast::TopLevelDeclaration> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self.syntax().kind() {
            SyntaxKind::FUNCTION_DEF => self
                .cast::<syntax_ast::FunctionDef>()
                .expect("validated function")
                .print(shape, printer),
            SyntaxKind::CLASS_DEF => self
                .cast::<syntax_ast::ClassDef>()
                .expect("validated class")
                .print(shape, printer),
            SyntaxKind::ENUM_DEF => self
                .cast::<syntax_ast::EnumDef>()
                .expect("validated enum")
                .print(shape, printer),
            SyntaxKind::CLIENT_DEF => self
                .cast::<syntax_ast::ClientDef>()
                .expect("validated client")
                .print(shape, printer),
            SyntaxKind::TEST_DEF => self
                .cast::<syntax_ast::TestDef>()
                .expect("validated test")
                .print(shape, printer),
            SyntaxKind::TEST_EXPR_DEF => self
                .cast::<syntax_ast::TestExprDef>()
                .expect("validated expression test")
                .print(shape, printer),
            SyntaxKind::TESTSET_DEF => self
                .cast::<syntax_ast::TestsetDef>()
                .expect("validated test set")
                .print(shape, printer),
            SyntaxKind::RETRY_POLICY_DEF => self
                .cast::<syntax_ast::RetryPolicyDef>()
                .expect("validated retry policy")
                .print(shape, printer),
            SyntaxKind::TEMPLATE_STRING_DEF => self
                .cast::<syntax_ast::TemplateStringDef>()
                .expect("validated template string")
                .print(shape, printer),
            SyntaxKind::TYPE_ALIAS_DEF => self
                .cast::<syntax_ast::TypeAliasDef>()
                .expect("validated type alias")
                .print(shape, printer),
            SyntaxKind::GENERATOR_DEF => self
                .cast::<syntax_ast::GeneratorDef>()
                .expect("validated generator")
                .print(shape, printer),
            _ => {
                let text = &printer.input[self.text_range()];
                printer.print_str(text.trim());
                PrintInfo::default_multi_lined()
            }
        }
    }
    fn leftmost_token(&self) -> TextRange {
        self.first_token_range()
    }
    fn rightmost_token(&self) -> TextRange {
        self.last_token_range()
    }
}

impl Printable for Validated<'_, syntax_ast::FunctionDef> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let keyword = self.function_token();
        let name = self.name_token();
        let generic_params = self.generic_param_list().map(|params| {
            super::GenericParamList::from_cst(SyntaxElement::Node(params.syntax().clone()))
                .expect("validated generic parameters")
        });
        let params = self.parameter_list();
        let arrow = self
            .arrow_token()
            .or_else(|| self.fat_arrow_token())
            .expect("validated function arrow");
        let return_type =
            super::Type::from_cst(SyntaxElement::Node(self.type_expr().syntax().clone()))
                .expect("validated return type");
        let throws = self.throws_clause().map(|throws| {
            super::ThrowsClause::from_cst(SyntaxElement::Node(throws.syntax().clone()))
                .expect("validated throws clause")
        });
        let body = self.function_body_kind();
        print_function_layout(
            &keyword,
            &name,
            generic_params.as_ref(),
            &params,
            &arrow,
            &return_type,
            throws.as_ref(),
            &body,
            shape,
            printer,
        )
    }
    fn leftmost_token(&self) -> TextRange {
        self.first_token_range()
    }
    fn rightmost_token(&self) -> TextRange {
        self.last_token_range()
    }
}

#[allow(clippy::too_many_arguments)]
fn print_function_layout(
    keyword: &impl Token,
    name: &impl Token,
    generic_params: Option<&super::GenericParamList>,
    params: &Validated<'_, syntax_ast::ParameterList>,
    arrow: &impl FunctionArrowLayout,
    return_type: &super::Type,
    throws: Option<&super::ThrowsClause>,
    body: &Validated<'_, syntax_ast::FunctionBodyKind>,
    shape: Shape,
    printer: &mut Printer,
) -> PrintInfo {
    printer.print_raw_token(keyword);
    printer.print_str(" ");
    printer.print_raw_token(name);
    if let Some(generic_params) = generic_params {
        printer.print(generic_params, shape.clone());
    }

    let mut param_printer = Printer::new_empty(printer.input, printer.config, printer.trivia);
    let param_info = param_printer.print(params, Shape::unlimited_single_line());
    let mut return_type_printer = Printer::new_empty(printer.input, printer.config, printer.trivia);
    let return_type_info = return_type_printer.print(return_type, Shape::unlimited_single_line());
    let (_, return_type_line_comment) =
        return_type_printer.print_trivia_all_trailing_for(return_type.rightmost_token());
    let mut throws_printer = Printer::new_empty(printer.input, printer.config, printer.trivia);
    let throws_info = throws
        .map(|throws| throws_printer.print(throws, Shape::unlimited_single_line()))
        .unwrap_or_else(PrintInfo::default_single_line);

    let single_line_size = printer.current_line_len()
        + param_printer.output.len()
        + const { " -> ".len() + " {".len() }
        + return_type_printer.output.len()
        + throws.map_or(0, |_| const { " ".len() } + throws_printer.output.len());
    if single_line_size <= printer.config.line_width
        && !param_info.multi_lined
        && !return_type_info.multi_lined
        && !throws_info.multi_lined
        && !return_type_line_comment
    {
        printer.append_from_printer(param_printer);
        printer.print_spaces(1);
        printer.print_str("->");
        arrow.print_separator_before(
            Some(return_type.leftmost_token()),
            shape.indent + printer.config.indent_width,
            printer,
        );
        printer.append_from_printer(return_type_printer);
        if throws.is_some() {
            printer.print_spaces(1);
            printer.append_from_printer(throws_printer);
        }
        printer.print_spaces(1);
        return printer.print(body, shape);
    }

    params.print_multi_line(
        Shape {
            width: 0,
            indent: shape.indent,
            first_line_offset: 0,
        },
        printer,
    );
    printer.print_spaces(1);
    printer.print_str("->");
    arrow.print_separator_before(
        Some(return_type.leftmost_token()),
        shape.indent + printer.config.indent_width,
        printer,
    );
    let curr_line_len = printer.current_line_len();
    let return_info = return_type.print(
        Shape {
            width: printer
                .config
                .line_width
                .saturating_sub(curr_line_len + const { " {".len() }),
            indent: shape.indent,
            first_line_offset: curr_line_len.saturating_sub(shape.indent),
        },
        printer,
    );
    let (_, return_type_line_comment) =
        printer.print_trivia_all_trailing_for(return_type.rightmost_token());
    let throws_info = throws.map_or_else(PrintInfo::default_single_line, |throws| {
        printer.print_str(" ");
        printer.print(throws, shape.clone())
    });
    if (return_info.multi_lined && return_type.multi_line_is_indented())
        || throws_info.multi_lined
        || return_type_line_comment
    {
        printer.print_newline();
    } else {
        printer.print_str(" ");
    }
    printer.print(body, shape);
    PrintInfo::default_multi_lined()
}

#[derive(Clone)]
struct RawToken(SyntaxToken);

impl Token for RawToken {
    fn span(&self) -> TextRange {
        self.0.text_range()
    }
}

#[derive(Clone)]
enum ParameterToken {
    Cached(ValidatedSyntaxToken),
    Raw(SyntaxToken),
}

impl Token for ParameterToken {
    fn span(&self) -> TextRange {
        match self {
            Self::Cached(token) => token.span(),
            Self::Raw(token) => token.text_range(),
        }
    }
}

#[derive(Clone)]
enum ParameterView<'tree> {
    Cached(Validated<'tree, syntax_ast::Parameter>),
    Raw(syntax_ast::Parameter),
}

impl ParameterView<'_> {
    fn name_token(&self) -> ParameterToken {
        match self {
            Self::Cached(parameter) => ParameterToken::Cached(
                parameter
                    .direct_elements()
                    .next()
                    .and_then(|element| element.token())
                    .expect("validated parameter name"),
            ),
            Self::Raw(parameter) => ParameterToken::Raw(
                parameter
                    .syntax()
                    .children_with_tokens()
                    .filter_map(rowan::NodeOrToken::into_token)
                    .find(|token| matches!(token.kind(), SyntaxKind::WORD | SyntaxKind::KW_CLIENT))
                    .expect("validated parameter name"),
            ),
        }
    }

    fn colon_token(&self) -> Option<ParameterToken> {
        match self {
            Self::Cached(parameter) => parameter.colon_token().map(ParameterToken::Cached),
            Self::Raw(parameter) => parameter.colon_token().map(ParameterToken::Raw),
        }
    }

    fn equals_token(&self) -> Option<ParameterToken> {
        match self {
            Self::Cached(parameter) => parameter.equals_token().map(ParameterToken::Cached),
            Self::Raw(parameter) => parameter.equals_token().map(ParameterToken::Raw),
        }
    }

    fn ty(&self) -> Option<super::Type> {
        match self {
            Self::Cached(parameter) => {
                let ty = parameter.type_expr()?;
                super::Type::from_cst(SyntaxElement::Node(ty.syntax().clone())).ok()
            }
            Self::Raw(parameter) => {
                let ty = parameter.type_expr()?;
                super::Type::from_cst(SyntaxElement::Node(ty.syntax().clone())).ok()
            }
        }
    }

    fn default(&self) -> Option<super::Expression> {
        match self {
            Self::Cached(parameter) => {
                let default = parameter.default_value()?;
                super::Expression::from_cst(SyntaxElement::Node(default.syntax().clone())).ok()
            }
            Self::Raw(parameter) => {
                let default = parameter.default_value()?;
                super::Expression::from_cst(SyntaxElement::Node(default.syntax().clone())).ok()
            }
        }
    }
}

impl Printable for ParameterView<'_> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let name = self.name_token();
        let ty = self.ty();
        let default = self.default();
        printer.print_raw_token(&name);
        let mut info = if let Some(ty) = &ty {
            let mut trivia_len = 0;
            if let Some(colon) = self.colon_token() {
                printer.print_str(": ");
                trivia_len += printer
                    .print_trivia_squished(printer.trivia.get_for_range_split(colon.span()).1);
            } else {
                printer.print_str(": ");
            }
            trivia_len += printer.print_trivia_squished(printer.trivia.get_leading_for_element(ty));
            let offset = usize::from(name.span().len()) + 2 + trivia_len;
            ty.print(
                Shape {
                    width: shape.width.saturating_sub(offset),
                    indent: shape.indent,
                    first_line_offset: shape.first_line_offset + offset,
                },
                printer,
            )
        } else {
            PrintInfo::default_single_line()
        };
        if let Some(default) = &default {
            let equals = self
                .equals_token()
                .expect("validated parameter default equals");
            let previous = ty
                .as_ref()
                .map_or_else(|| name.span(), Printable::rightmost_token);
            printer.print_trivia_squished(printer.trivia.get_for_range_split(previous).1);
            let (leading, trailing) = printer.trivia.get_for_range_split(equals.span());
            printer.print_trivia_squished(leading);
            printer.print_str(" = ");
            printer.print_trivia_squished(trailing);
            printer.print_trivia_squished(printer.trivia.get_leading_for_element(default));
            info = printer.print(default, shape);
        }
        info
    }

    fn leftmost_token(&self) -> TextRange {
        self.name_token().span()
    }

    fn rightmost_token(&self) -> TextRange {
        match self {
            Self::Cached(parameter) => parameter.last_token_range(),
            Self::Raw(parameter) => parameter
                .syntax()
                .last_token()
                .expect("validated parameter")
                .text_range(),
        }
    }
}

#[derive(Clone)]
enum ParameterListView<'tree> {
    Cached(Validated<'tree, syntax_ast::ParameterList>),
    Raw(syntax_ast::ParameterList),
}

impl<'tree> ParameterListView<'tree> {
    fn tokens(&self) -> (ParameterToken, ParameterToken) {
        match self {
            Self::Cached(parameters) => (
                ParameterToken::Cached(parameters.l_paren_token()),
                ParameterToken::Cached(parameters.r_paren_token()),
            ),
            Self::Raw(parameters) => (
                ParameterToken::Raw(parameters.l_paren_token().expect("validated open paren")),
                ParameterToken::Raw(parameters.r_paren_token().expect("validated close paren")),
            ),
        }
    }

    fn parameters(&self) -> Vec<(ParameterView<'tree>, Option<ParameterToken>)> {
        match self {
            Self::Cached(parameters) => {
                let mut elements = parameters.direct_elements().peekable();
                let mut result = Vec::new();
                while let Some(element) = elements.next() {
                    if let Some(parameter) = element.node::<syntax_ast::Parameter>() {
                        result.push((
                            ParameterView::Cached(parameter),
                            take_delimiter(&mut elements).map(ParameterToken::Cached),
                        ));
                    }
                }
                result
            }
            Self::Raw(parameters) => {
                let mut elements = parameters
                    .syntax()
                    .children_with_tokens()
                    .filter(|element| !element.kind().is_trivia())
                    .peekable();
                let mut result = Vec::new();
                while let Some(element) = elements.next() {
                    let Some(node) = element.into_node() else {
                        continue;
                    };
                    let Some(parameter) = syntax_ast::Parameter::cast(node) else {
                        continue;
                    };
                    let delimiter = elements
                        .next_if(|element| {
                            matches!(element.kind(), SyntaxKind::COMMA | SyntaxKind::SEMICOLON)
                        })
                        .and_then(rowan::NodeOrToken::into_token)
                        .map(ParameterToken::Raw);
                    result.push((ParameterView::Raw(parameter), delimiter));
                }
                result
            }
        }
    }

    fn print_multi_line(&self, shape: &Shape, printer: &mut Printer) -> PrintInfo {
        let inner_indent = shape.indent + printer.config.indent_width;
        let inner_shape = Shape {
            width: printer.config.line_width.saturating_sub(inner_indent),
            indent: inner_indent,
            first_line_offset: 0,
        };
        let (open_paren, close_paren) = self.tokens();
        let params = self.parameters();
        printer.print_raw_token(&open_paren);
        printer.print_trivia_all_trailing_for(open_paren.span());
        printer.print_newline();
        for (param, comma) in &params {
            let (param_leading, param_trailing) = printer.trivia.get_for_element(param);
            printer.print_trivia_with_newline(param_leading.trim_blanks(), inner_shape.indent);
            printer.print_spaces(inner_shape.indent);
            printer.print(param, inner_shape.clone());
            if let Some(comma) = comma {
                printer.print_trivia_squished(param_trailing);
                let (comma_leading, comma_trailing) =
                    printer.trivia.get_for_range_split(comma.span());
                printer.print_trivia_squished(comma_leading);
                printer.print_raw_token(comma);
                printer.print_trivia_trailing(comma_trailing);
            } else {
                printer.print_str(",");
                printer.print_trivia_trailing(param_trailing);
            }
            printer.print_newline();
        }
        let (close_leading, _) = printer.trivia.get_for_range_split(close_paren.span());
        printer.print_trivia_with_newline(close_leading.trim_blanks(), inner_shape.indent);
        printer.print_spaces(shape.indent);
        printer.print_raw_token(&close_paren);
        PrintInfo::default_multi_lined()
    }

    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        let (open_paren, close_paren) = self.tokens();
        let params = self.parameters();
        printer.print_raw_token(&open_paren);
        let (_, open_trailing) = printer.trivia.get_for_range_split(open_paren.span());
        printer.try_print_trivia_single_line_squished(open_trailing)?;
        for (i, (param, comma)) in params.iter().enumerate() {
            if printer.output.len() > shape.width {
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
            let (comma_leading, comma_trailing) =
                comma.as_ref().map_or((&[][..], &[][..]), |comma| {
                    printer.trivia.get_for_range_split(comma.span())
                });
            if i + 1 < params.len() {
                printer.print_trivia_squished(trailing);
                printer.print_trivia_squished(comma_leading);
                printer.print_str(", ");
                printer.try_print_trivia_single_line_squished(comma_trailing)?;
            } else {
                printer.try_print_trivia_single_line_squished(trailing)?;
                printer.try_print_trivia_single_line_squished(comma_leading)?;
                printer.try_print_trivia_single_line_squished(comma_trailing)?;
            }
        }
        let (close_leading, _) = printer.trivia.get_for_range_split(close_paren.span());
        printer.try_print_trivia_single_line_squished(close_leading)?;
        printer.print_raw_token(&close_paren);
        (printer.output.len() <= shape.width).then(PrintInfo::default_single_line)
    }
}

impl Printable for ParameterListView<'_> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|p| self.try_print_single_line(&shape, p))
            .unwrap_or_else(|| self.print_multi_line(&shape, printer))
    }

    fn leftmost_token(&self) -> TextRange {
        self.tokens().0.span()
    }

    fn rightmost_token(&self) -> TextRange {
        self.tokens().1.span()
    }
}

impl PrintMultiLine for Validated<'_, syntax_ast::ParameterList> {
    fn print_multi_line(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        ParameterListView::Cached(*self).print_multi_line(&shape, printer)
    }
}

impl Printable for Validated<'_, syntax_ast::ParameterList> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        ParameterListView::Cached(*self).print(shape, printer)
    }

    fn leftmost_token(&self) -> TextRange {
        self.first_token_range()
    }

    fn rightmost_token(&self) -> TextRange {
        self.last_token_range()
    }
}

impl PrintMultiLine for syntax_ast::ParameterList {
    fn print_multi_line(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        ParameterListView::Raw(self.clone()).print_multi_line(&shape, printer)
    }
}

impl Printable for syntax_ast::ParameterList {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        ParameterListView::Raw(self.clone()).print(shape, printer)
    }

    fn leftmost_token(&self) -> TextRange {
        self.l_paren_token()
            .expect("validated open paren")
            .text_range()
    }

    fn rightmost_token(&self) -> TextRange {
        self.r_paren_token()
            .expect("validated close paren")
            .text_range()
    }
}

impl Printable for Validated<'_, syntax_ast::FunctionBodyKind> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self.syntax().kind() {
            SyntaxKind::LLM_FUNCTION_BODY => self
                .cast::<syntax_ast::LlmFunctionBody>()
                .expect("validated LLM function body")
                .print(shape, printer),
            SyntaxKind::EXPR_FUNCTION_BODY => {
                let body = self
                    .cast::<syntax_ast::ExprFunctionBody>()
                    .expect("validated expression function body")
                    .block_expr();
                let body = super::BlockExpr::from_cst(SyntaxElement::Node(body.syntax().clone()))
                    .expect("validated expression function body");
                body.print(shape, printer)
            }
            _ => unreachable!("validated function body kind"),
        }
    }
    fn leftmost_token(&self) -> TextRange {
        self.first_token_range()
    }
    fn rightmost_token(&self) -> TextRange {
        self.last_token_range()
    }
}

impl Printable for Validated<'_, syntax_ast::LlmFunctionBody> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let inner_indent = shape.indent + printer.config.indent_width;
        let open_brace = self.l_brace_token();
        let close_brace = self.r_brace_token();
        let client = self.client_field().expect("validated LLM client field");
        let prompt = self.prompt_field().expect("validated LLM prompt field");

        printer.print_raw_token(&open_brace);
        printer.print_trivia_all_trailing_for(open_brace.span());
        printer.print_newline();

        let (client_leading, client_trailing) = printer.trivia.get_for_element(&client);
        printer.print_trivia_with_newline(client_leading.trim_leading_blanks(), inner_indent);
        printer.print_spaces(inner_indent);
        let inner_shape = Shape::standalone(printer.config.line_width, inner_indent);
        client.print(inner_shape, printer);
        printer.print_trivia_trailing(client_trailing);
        printer.print_newline();

        if let Some(tools) = self.tools_field() {
            printer.print_standalone_with_trivia(&tools, inner_indent);
            printer.print_newline();
        }

        printer.print_standalone_with_trivia(&prompt, inner_indent);
        printer.print_newline();

        let (close_brace_leading, _) = printer.trivia.get_for_range_split(close_brace.span());
        printer.print_trivia_with_newline(close_brace_leading.trim_trailing_blanks(), inner_indent);
        printer.print_spaces(shape.indent);
        printer.print_raw_token(&close_brace);

        PrintInfo::default_multi_lined()
    }
    fn leftmost_token(&self) -> TextRange {
        self.first_token_range()
    }
    fn rightmost_token(&self) -> TextRange {
        self.last_token_range()
    }
}

fn print_llm_field(
    keyword: &impl Token,
    colon: &impl Token,
    value: &Validated<'_, syntax_ast::ExprNode>,
    shape: Shape,
    printer: &mut Printer,
) -> PrintInfo {
    let value = super::Expression::from_cst(SyntaxElement::Node(value.syntax().clone()))
        .expect("validated LLM field value");
    printer.print_raw_token(keyword);
    let (_, keyword_trailing) = printer.trivia.get_for_range_split(keyword.span());
    printer.print_trivia_squished(keyword_trailing);
    let (colon_leading, colon_trailing) = printer.trivia.get_for_range_split(colon.span());
    printer.print_trivia_squished(colon_leading);
    printer.print_str(": ");
    printer.print_trivia_squished(colon_trailing);
    let value_leading = printer.trivia.get_leading_for_element(&value);
    printer.print_trivia_squished(value_leading);
    printer.print(&value, shape)
}

impl Printable for Validated<'_, syntax_ast::ClientField> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        print_llm_field(
            &self.client_token(),
            &self.colon_token(),
            &self.value(),
            shape,
            printer,
        )
    }
    fn leftmost_token(&self) -> TextRange {
        self.first_token_range()
    }
    fn rightmost_token(&self) -> TextRange {
        self.last_token_range()
    }
}

impl Printable for Validated<'_, syntax_ast::PromptField> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        print_llm_field(
            &self.name_token(),
            &self.colon_token(),
            &self.value(),
            shape,
            printer,
        )
    }
    fn leftmost_token(&self) -> TextRange {
        self.first_token_range()
    }
    fn rightmost_token(&self) -> TextRange {
        self.last_token_range()
    }
}

impl Printable for Validated<'_, syntax_ast::ToolsField> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        print_llm_field(
            &self.name_token(),
            &self.colon_token(),
            &self.value(),
            shape,
            printer,
        )
    }
    fn leftmost_token(&self) -> TextRange {
        self.first_token_range()
    }
    fn rightmost_token(&self) -> TextRange {
        self.last_token_range()
    }
}

enum ClassLayoutItem<'tree> {
    Field(
        Validated<'tree, syntax_ast::Field>,
        Option<ValidatedSyntaxToken>,
    ),
    Function(Validated<'tree, syntax_ast::FunctionDef>),
    Implements(Validated<'tree, syntax_ast::ImplementsBlock>),
    BlockAttribute(Validated<'tree, syntax_ast::BlockAttribute>),
}

impl Printable for Validated<'_, syntax_ast::ClassDef> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let inner_indent = shape.indent + printer.config.indent_width;
        let generic_params = self.generic_param_list().map(|params| {
            super::GenericParamList::from_cst(SyntaxElement::Node(params.syntax().clone()))
                .expect("validated generic parameters")
        });
        let mut elements = self.direct_elements().peekable();
        let mut items = Vec::new();
        while let Some(element) = elements.next() {
            let item = match element.kind() {
                SyntaxKind::FIELD => {
                    let field = element
                        .node::<syntax_ast::Field>()
                        .expect("validated class field");
                    let delimiter = elements
                        .next_if(|next| {
                            matches!(next.kind(), SyntaxKind::COMMA | SyntaxKind::SEMICOLON)
                        })
                        .and_then(|element| element.token());
                    ClassLayoutItem::Field(field, delimiter)
                }
                SyntaxKind::FUNCTION_DEF => ClassLayoutItem::Function(
                    element
                        .node::<syntax_ast::FunctionDef>()
                        .expect("validated class function"),
                ),
                SyntaxKind::IMPLEMENTS_BLOCK => ClassLayoutItem::Implements(
                    element
                        .node::<syntax_ast::ImplementsBlock>()
                        .expect("validated implements block"),
                ),
                SyntaxKind::BLOCK_ATTRIBUTE => ClassLayoutItem::BlockAttribute(
                    element
                        .node::<syntax_ast::BlockAttribute>()
                        .expect("validated block attribute"),
                ),
                _ => continue,
            };
            items.push(item);
        }
        let open_brace = self.l_brace_token();
        let close_brace = self.r_brace_token();

        printer.print_raw_token(&self.class_token());
        printer.print_str(" ");
        printer.print_raw_token(&self.name_token());
        if let Some(ref gp) = generic_params {
            printer.print(gp, shape.clone());
        }
        printer.print_str(" ");
        printer.print_raw_token(&open_brace);
        printer.print_trivia_all_trailing_for(open_brace.span());
        printer.print_newline();

        if let Some((first, rest)) = items.split_first() {
            // first has leading empty lines trimmed
            let (first_leading, first_trailing) = printer.trivia.get_for_element(first);
            printer.print_trivia_with_newline(first_leading.trim_leading_blanks(), inner_indent);
            printer.print_spaces(inner_indent);
            let inner_shape = Shape::standalone(printer.config.line_width, inner_indent);
            first.print(inner_shape, printer);
            printer.print_trivia_trailing(first_trailing);
            printer.print_newline();

            // rest can have leading empty lines
            for item in rest {
                printer.print_standalone_with_trivia(item, inner_indent);
                printer.print_newline();
            }
        }

        let (close_brace_leading, _) = printer.trivia.get_for_range_split(close_brace.span());
        printer.print_trivia_with_newline(close_brace_leading.trim_trailing_blanks(), inner_indent);
        printer.print_spaces(shape.indent);
        printer.print_raw_token(&close_brace);

        PrintInfo::default_multi_lined()
    }
    fn leftmost_token(&self) -> TextRange {
        self.first_token_range()
    }
    fn rightmost_token(&self) -> TextRange {
        self.last_token_range()
    }
}

fn class_field_parts(
    field: &Validated<'_, syntax_ast::Field>,
) -> (ValidatedSyntaxToken, super::Type) {
    let name = field
        .direct_elements()
        .filter_map(|element| element.token())
        .find(|token| matches!(token.kind(), SyntaxKind::WORD | SyntaxKind::KW_CLIENT))
        .expect("validated class field name");
    let ty = field.type_expr().expect("validated class field type");
    let ty = super::Type::from_cst(SyntaxElement::Node(ty.syntax().clone()))
        .expect("validated class field type");
    (name, ty)
}

impl PrintMultiLine for Validated<'_, syntax_ast::Field> {
    /// Multi-line layout: attributes wrap to their own indented lines
    /// below the field name and type. Trailing comments on the type are preserved.
    ///
    /// ```baml
    /// myField ReallyLongTypeName // trailing comment
    ///     @alias("theLongField")
    ///     @description("some desc")
    /// ```
    fn print_multi_line(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let (name, ty) = class_field_parts(self);
        printer.print_raw_token(&name);
        let colon_trailing = if let Some(colon) = self.colon_token() {
            let (_, colon_trailing) = printer.trivia.get_for_range_split(colon.span());
            colon_trailing
        } else {
            &[][..]
        };
        printer.print_str(": ");
        printer.print_trivia_squished(colon_trailing);

        let (type_leading, _) = printer.trivia.get_for_element(&ty);
        printer.print_trivia_squished(type_leading);
        printer.print(&ty, shape);
        PrintInfo::default_multi_lined()
    }
}

impl ClassFieldLayout for Validated<'_, syntax_ast::Field> {
    /// Should be passed a sub-printer to avoid printing trivia in the outer printer
    /// in the event that the printer is unable to fit the class field on a single line.
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        let (name, ty) = class_field_parts(self);
        printer.print_raw_token(&name);
        let colon_trailing = if let Some(colon) = self.colon_token() {
            let (_, colon_trailing) = printer.trivia.get_for_range_split(colon.span());
            colon_trailing
        } else {
            &[][..]
        };
        printer.print_str(": ");
        printer.try_print_trivia_single_line_squished(colon_trailing)?;

        let (type_leading, _) = printer.trivia.get_for_element(&ty);
        printer.print_trivia_squished(type_leading);
        if ty
            .print(Shape::unlimited_single_line(), printer)
            .multi_lined
            || printer.len() > shape.width
        {
            return None;
        }

        if printer.len() > shape.width {
            None
        } else {
            Some(PrintInfo::default_single_line())
        }
    }
}

impl Printable for Validated<'_, syntax_ast::Field> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|p| self.try_print_single_line(&shape, p))
            .unwrap_or_else(|| self.print_multi_line(shape, printer))
    }
    fn leftmost_token(&self) -> TextRange {
        self.first_token_range()
    }
    fn rightmost_token(&self) -> TextRange {
        self.last_token_range()
    }
}

impl Printable for Validated<'_, syntax_ast::ImplementsTarget> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let ty = self.type_expr();
        let ty = super::Type::from_cst(SyntaxElement::Node(ty.syntax().clone()))
            .expect("validated implements target");
        ty.print(shape, printer)
    }

    fn leftmost_token(&self) -> TextRange {
        self.first_token_range()
    }

    fn rightmost_token(&self) -> TextRange {
        self.last_token_range()
    }
}

impl Printable for Validated<'_, syntax_ast::AssociatedTypeDecl> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let bound = self.bound().map(|ty| {
            super::Type::from_cst(SyntaxElement::Node(ty.syntax().clone()))
                .expect("validated associated type bound")
        });
        let binding = self.binding().map(|ty| {
            super::Type::from_cst(SyntaxElement::Node(ty.syntax().clone()))
                .expect("validated associated type binding")
        });
        let mut multi_lined = false;
        printer.print_raw_token(&self.type_token().expect("validated type keyword"));
        printer.print_str(" ");
        printer.print_raw_token(&self.name_token());
        if let Some(ty) = &bound {
            let extends = self.extends_token().expect("validated extends token");
            let (_, extends_trailing) = printer.trivia.get_for_range_split(extends.span());
            printer.print_str(" extends ");
            printer.print_trivia_squished(extends_trailing);
            let leading = printer.trivia.get_leading_for_element(ty);
            printer.print_trivia_squished(leading);
            multi_lined |= ty.print(shape.clone(), printer).multi_lined;
        }
        if let Some(ty) = &binding {
            let equals = self.equals_token().expect("validated equals token");
            let (_, equals_trailing) = printer.trivia.get_for_range_split(equals.span());
            printer.print_str(" = ");
            printer.print_trivia_squished(equals_trailing);
            let leading = printer.trivia.get_leading_for_element(ty);
            printer.print_trivia_squished(leading);
            multi_lined |= ty.print(shape, printer).multi_lined;
        }
        PrintInfo { multi_lined }
    }

    fn leftmost_token(&self) -> TextRange {
        self.first_token_range()
    }

    fn rightmost_token(&self) -> TextRange {
        self.last_token_range()
    }
}

impl Printable for Validated<'_, syntax_ast::InterfaceFieldLink> {
    fn print(&self, _shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.interface_field());
        printer.print_str(" ");
        printer.print_raw_token(&self.as_token());
        printer.print_str(" ");
        printer.print_raw_token(&self.class_field());
        PrintInfo::default_single_line()
    }

    fn leftmost_token(&self) -> TextRange {
        self.first_token_range()
    }

    fn rightmost_token(&self) -> TextRange {
        self.last_token_range()
    }
}

enum ImplementsLayoutItem<'tree> {
    AssociatedType(
        Validated<'tree, syntax_ast::AssociatedTypeDecl>,
        Option<ValidatedSyntaxToken>,
    ),
    FieldLink(
        Validated<'tree, syntax_ast::InterfaceFieldLink>,
        Option<ValidatedSyntaxToken>,
    ),
    Field(
        Validated<'tree, syntax_ast::Field>,
        Option<ValidatedSyntaxToken>,
    ),
    Function(Validated<'tree, syntax_ast::FunctionDef>),
    BlockAttribute(Validated<'tree, syntax_ast::BlockAttribute>),
}

fn take_delimiter(
    elements: &mut std::iter::Peekable<
        baml_db::baml_compiler_syntax::validated::ValidatedDirectElements<'_>,
    >,
) -> Option<ValidatedSyntaxToken> {
    elements
        .next_if(|next| matches!(next.kind(), SyntaxKind::COMMA | SyntaxKind::SEMICOLON))
        .and_then(|element| element.token())
}

impl Printable for ImplementsLayoutItem<'_> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            ImplementsLayoutItem::AssociatedType(decl, _) => decl.print(shape, printer),
            ImplementsLayoutItem::FieldLink(link, _) => link.print(shape, printer),
            ImplementsLayoutItem::Field(field, delimiter) => {
                let info = field.print(shape, printer);
                if let Some(delimiter) = delimiter.filter(|token| token.kind() == SyntaxKind::COMMA)
                {
                    printer.print_raw_token(&delimiter);
                }
                info
            }
            ImplementsLayoutItem::Function(function) => function.print(shape, printer),
            ImplementsLayoutItem::BlockAttribute(attribute) => {
                let attribute =
                    BlockAttribute::from_cst(SyntaxElement::Node(attribute.syntax().clone()))
                        .expect("validated block attribute");
                attribute.print(shape, printer)
            }
        }
    }

    fn leftmost_token(&self) -> TextRange {
        match self {
            ImplementsLayoutItem::AssociatedType(decl, _) => decl.leftmost_token(),
            ImplementsLayoutItem::FieldLink(link, _) => link.leftmost_token(),
            ImplementsLayoutItem::Field(field, _) => field.leftmost_token(),
            ImplementsLayoutItem::Function(function) => function.leftmost_token(),
            ImplementsLayoutItem::BlockAttribute(attribute) => attribute.first_token_range(),
        }
    }

    fn rightmost_token(&self) -> TextRange {
        match self {
            ImplementsLayoutItem::AssociatedType(decl, delimiter) => delimiter
                .as_ref()
                .map_or_else(|| decl.rightmost_token(), Token::span),
            ImplementsLayoutItem::FieldLink(link, delimiter) => delimiter
                .as_ref()
                .map_or_else(|| link.rightmost_token(), Token::span),
            ImplementsLayoutItem::Field(field, delimiter) => delimiter
                .as_ref()
                .map_or_else(|| field.rightmost_token(), Token::span),
            ImplementsLayoutItem::Function(function) => function.rightmost_token(),
            ImplementsLayoutItem::BlockAttribute(attribute) => attribute.last_token_range(),
        }
    }
}

impl Printable for Validated<'_, syntax_ast::ImplementsBlock> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let target = self.implements_target();
        let mut elements = self.direct_elements().peekable();
        let mut items = Vec::new();
        while let Some(element) = elements.next() {
            let item = match element.kind() {
                SyntaxKind::ASSOCIATED_TYPE_DECL => ImplementsLayoutItem::AssociatedType(
                    element
                        .node::<syntax_ast::AssociatedTypeDecl>()
                        .expect("validated associated type"),
                    take_delimiter(&mut elements),
                ),
                SyntaxKind::INTERFACE_FIELD_LINK => ImplementsLayoutItem::FieldLink(
                    element
                        .node::<syntax_ast::InterfaceFieldLink>()
                        .expect("validated interface field link"),
                    take_delimiter(&mut elements),
                ),
                SyntaxKind::FIELD => ImplementsLayoutItem::Field(
                    element
                        .node::<syntax_ast::Field>()
                        .expect("validated implements field"),
                    take_delimiter(&mut elements),
                ),
                SyntaxKind::FUNCTION_DEF => ImplementsLayoutItem::Function(
                    element
                        .node::<syntax_ast::FunctionDef>()
                        .expect("validated implements function"),
                ),
                SyntaxKind::BLOCK_ATTRIBUTE => ImplementsLayoutItem::BlockAttribute(
                    element
                        .node::<syntax_ast::BlockAttribute>()
                        .expect("validated block attribute"),
                ),
                _ => continue,
            };
            items.push(item);
        }
        let keyword = self.implements_token();
        let open_brace = self.l_brace_token();
        let close_brace = self.r_brace_token();
        printer.print_str("implements");
        let (_, keyword_trailing) = printer.trivia.get_for_range_split(keyword.span());
        let trivia_len = printer.print_trivia_squished(keyword_trailing);
        if trivia_len == 0 {
            printer.print_str(" ");
        }
        let target_leading = printer.trivia.get_leading_for_element(&target);
        printer.print_trivia_squished(target_leading);
        printer.print(&target, shape.clone());

        if items.is_empty() {
            printer.print_str(" ");
            printer.print_raw_token(&open_brace);
            printer.print_raw_token(&close_brace);
            return PrintInfo::default_single_line();
        }

        let inner_indent = shape.indent + printer.config.indent_width;
        printer.print_str(" ");
        printer.print_raw_token(&open_brace);
        printer.print_trivia_all_trailing_for(open_brace.span());
        printer.print_newline();

        if let Some((first, rest)) = items.split_first() {
            let (first_leading, first_trailing) = printer.trivia.get_for_element(first);
            printer.print_trivia_with_newline(first_leading.trim_leading_blanks(), inner_indent);
            printer.print_spaces(inner_indent);
            let inner_shape = Shape::standalone(printer.config.line_width, inner_indent);
            first.print(inner_shape, printer);
            printer.print_trivia_trailing(first_trailing);
            printer.print_newline();

            for item in rest {
                printer.print_standalone_with_trivia(item, inner_indent);
                printer.print_newline();
            }
        }

        let (close_brace_leading, _) = printer.trivia.get_for_range_split(close_brace.span());
        printer.print_trivia_with_newline(close_brace_leading.trim_trailing_blanks(), inner_indent);
        printer.print_spaces(shape.indent);
        printer.print_raw_token(&close_brace);
        PrintInfo::default_multi_lined()
    }

    fn leftmost_token(&self) -> TextRange {
        self.first_token_range()
    }

    fn rightmost_token(&self) -> TextRange {
        self.last_token_range()
    }
}

impl Printable for ClassLayoutItem<'_> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            ClassLayoutItem::Field(field, delimiter) => {
                let info = field.print(shape, printer);
                match delimiter {
                    Some(delimiter) if delimiter.kind() == SyntaxKind::COMMA => {
                        printer.print_raw_token(delimiter);
                    }
                    _ => printer.print_str(","),
                }
                info
            }
            ClassLayoutItem::Function(function) => function.print(shape, printer),
            ClassLayoutItem::Implements(block) => block.print(shape, printer),
            ClassLayoutItem::BlockAttribute(attr) => {
                let attr = BlockAttribute::from_cst(SyntaxElement::Node(attr.syntax().clone()))
                    .expect("validated block attribute");
                attr.print(shape, printer)
            }
        }
    }
    fn leftmost_token(&self) -> TextRange {
        match self {
            ClassLayoutItem::Field(field, _) => field.leftmost_token(),
            ClassLayoutItem::Function(function) => function.leftmost_token(),
            ClassLayoutItem::Implements(block) => block.leftmost_token(),
            ClassLayoutItem::BlockAttribute(attr) => attr.first_token_range(),
        }
    }
    fn rightmost_token(&self) -> TextRange {
        match self {
            ClassLayoutItem::Field(field, delimiter) => delimiter
                .as_ref()
                .map_or_else(|| field.rightmost_token(), Token::span),
            ClassLayoutItem::Function(function) => function.rightmost_token(),
            ClassLayoutItem::Implements(block) => block.rightmost_token(),
            ClassLayoutItem::BlockAttribute(attr) => attr.last_token_range(),
        }
    }
}

enum EnumLayoutItem<'tree> {
    Variant(
        Validated<'tree, syntax_ast::EnumVariant>,
        Option<ValidatedSyntaxToken>,
    ),
    BlockAttribute(Validated<'tree, syntax_ast::BlockAttribute>),
}

impl Printable for Validated<'_, syntax_ast::EnumDef> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let inner_indent = shape.indent + printer.config.indent_width;
        let mut elements = self.direct_elements().peekable();
        let mut items = Vec::new();
        while let Some(element) = elements.next() {
            match element.kind() {
                SyntaxKind::ENUM_VARIANT => {
                    let variant = element
                        .node::<syntax_ast::EnumVariant>()
                        .expect("validated enum variant");
                    let delimiter = elements
                        .next_if(|next| {
                            matches!(next.kind(), SyntaxKind::COMMA | SyntaxKind::SEMICOLON)
                        })
                        .and_then(|element| element.token());
                    items.push(EnumLayoutItem::Variant(variant, delimiter));
                }
                SyntaxKind::BLOCK_ATTRIBUTE => items.push(EnumLayoutItem::BlockAttribute(
                    element
                        .node::<syntax_ast::BlockAttribute>()
                        .expect("validated enum block attribute"),
                )),
                _ => {}
            }
        }
        let open_brace = self.l_brace_token();
        let close_brace = self.r_brace_token();

        printer.print_raw_token(&self.enum_token());
        printer.print_str(" ");
        printer.print_raw_token(&self.name_token());
        printer.print_str(" ");
        printer.print_raw_token(&open_brace);
        printer.print_trivia_all_trailing_for(open_brace.span());
        printer.print_newline();

        if let Some((first, rest)) = items.split_first() {
            // first has leading empty lines trimmed
            let (first_leading, first_trailing) = printer.trivia.get_for_element(first);
            printer.print_trivia_with_newline(first_leading.trim_leading_blanks(), inner_indent);
            printer.print_spaces(inner_indent);
            let inner_shape = Shape::standalone(printer.config.line_width, inner_indent);
            first.print(inner_shape, printer);
            printer.print_trivia_trailing(first_trailing);
            printer.print_newline();

            // rest can have leading empty lines
            for item in rest {
                printer.print_standalone_with_trivia(item, inner_indent);
                printer.print_newline();
            }
        }

        let (close_brace_leading, _) = printer.trivia.get_for_range_split(close_brace.span());
        printer.print_trivia_with_newline(close_brace_leading.trim_trailing_blanks(), inner_indent);
        printer.print_spaces(shape.indent);
        printer.print_raw_token(&close_brace);

        PrintInfo::default_multi_lined()
    }
    fn leftmost_token(&self) -> TextRange {
        self.first_token_range()
    }
    fn rightmost_token(&self) -> TextRange {
        self.last_token_range()
    }
}

impl Printable for EnumLayoutItem<'_> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            EnumLayoutItem::Variant(variant, delimiter) => {
                let info = variant.print(shape, printer);
                if let Some(delimiter) = delimiter {
                    let (leading, _) = printer.trivia.get_for_range_split(delimiter.span());
                    printer.print_trivia_squished(leading);
                }
                printer.print_str(",");
                info
            }
            EnumLayoutItem::BlockAttribute(attr) => {
                let attr = BlockAttribute::from_cst(SyntaxElement::Node(attr.syntax().clone()))
                    .expect("validated block attribute");
                attr.print(shape, printer)
            }
        }
    }
    fn leftmost_token(&self) -> TextRange {
        match self {
            EnumLayoutItem::Variant(variant, _) => variant.leftmost_token(),
            EnumLayoutItem::BlockAttribute(attr) => attr.first_token_range(),
        }
    }
    fn rightmost_token(&self) -> TextRange {
        match self {
            EnumLayoutItem::Variant(variant, delimiter) => {
                if let Some(delimiter) = delimiter {
                    delimiter.span()
                } else {
                    variant.rightmost_token()
                }
            }
            EnumLayoutItem::BlockAttribute(attr) => attr.last_token_range(),
        }
    }
}

impl PrintMultiLine for Validated<'_, syntax_ast::EnumVariant> {
    /// Multi-line layout: attributes wrap to their own indented lines
    /// below the variant name. Trailing comments on the name are preserved.
    ///
    /// ```baml
    /// VariantName // description
    ///     @alias("something_long")
    ///     @description("a long description")
    /// ```
    fn print_multi_line(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let name = self.name_token();
        let attributes = self
            .attribute()
            .map(|attribute| {
                super::Attribute::from_cst(SyntaxElement::Node(attribute.syntax().clone()))
                    .expect("validated enum attribute")
            })
            .collect::<Vec<_>>();
        printer.print_raw_token(&name);

        if attributes.is_empty() {
            // you shouldn't call print_multi_line if this is the case.
            return PrintInfo::default_single_line();
        }
        printer.print_trivia_all_trailing_for(name.span());

        let attr_shape = Shape::standalone(
            printer.config.line_width,
            shape.indent + printer.config.indent_width,
        );
        for (i, attr) in attributes.iter().enumerate() {
            let (attr_leading, attr_trailing) = printer.trivia.get_for_element(attr);
            printer.print_newline();
            printer.print_trivia_with_newline(attr_leading.trim_blanks(), attr_shape.indent);
            printer.print_spaces(attr_shape.indent);
            printer.print(attr, attr_shape.clone());
            if i + 1 < attributes.len() {
                printer.print_trivia_trailing(attr_trailing);
            }
        }

        PrintInfo::default_multi_lined()
    }
}

impl EnumVariantLayout for Validated<'_, syntax_ast::EnumVariant> {
    /// Should be passed a sub-printer to avoid printing trivia in the outer printer
    /// in the event that the printer is unable to fit the enum variant on a single line.
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        let name = self.name_token();
        let attributes = self
            .attribute()
            .map(|attribute| {
                super::Attribute::from_cst(SyntaxElement::Node(attribute.syntax().clone()))
                    .expect("validated enum attribute")
            })
            .collect::<Vec<_>>();
        printer.print_raw_token(&name);
        let (_, name_trailing) = printer.trivia.get_for_range_split(name.span());
        printer.try_print_trivia_single_line_squished(name_trailing)?;

        for (i, attr) in attributes.iter().enumerate() {
            printer.print_spaces(1);
            let (attr_leading, attr_trailing) = printer.trivia.get_for_element(attr);
            printer.try_print_trivia_single_line_squished(attr_leading)?;
            if attr
                .print(Shape::unlimited_single_line(), printer)
                .multi_lined
            {
                return None;
            }
            if i + 1 < attributes.len() {
                printer.try_print_trivia_single_line_squished(attr_trailing)?;
            }
        }

        if printer.output.len() > shape.width {
            None
        } else {
            Some(PrintInfo::default_single_line())
        }
    }
}

impl Printable for Validated<'_, syntax_ast::EnumVariant> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|p| self.try_print_single_line(&shape, p))
            .unwrap_or_else(|| self.print_multi_line(shape, printer))
    }
    fn leftmost_token(&self) -> TextRange {
        self.first_token_range()
    }
    fn rightmost_token(&self) -> TextRange {
        self.last_token_range()
    }
}

impl Printable for Validated<'_, syntax_ast::ClientDef> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let config = self.config_block();
        printer.print_raw_token(&self.client_token());
        if let Some(client_type) = self.client_type() {
            printer.print(&client_type, Shape::unlimited_single_line());
        }
        printer.print_str(" ");
        printer.print_raw_token(&self.name_token());
        printer.print_str(" ");
        printer.print(&config, shape)
    }
    fn leftmost_token(&self) -> TextRange {
        self.first_token_range()
    }
    fn rightmost_token(&self) -> TextRange {
        self.last_token_range()
    }
}

impl Printable for Validated<'_, syntax_ast::ClientType> {
    fn print(&self, _shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.less_token());
        printer.print_raw_token(&self.name_token());
        printer.print_raw_token(&self.greater_token());
        PrintInfo::default_single_line()
    }
    fn leftmost_token(&self) -> TextRange {
        self.first_token_range()
    }
    fn rightmost_token(&self) -> TextRange {
        self.last_token_range()
    }
}

enum ConfigBlockLayoutMember<'tree> {
    Item(Validated<'tree, syntax_ast::ConfigItem>),
    BlockAttribute(BlockAttribute),
}

fn config_block_items<'tree>(
    block: &Validated<'tree, syntax_ast::ConfigBlock>,
) -> Vec<(ConfigBlockLayoutMember<'tree>, Option<ValidatedSyntaxToken>)> {
    let mut elements = block.direct_elements().peekable();
    let mut items = Vec::new();
    while let Some(element) = elements.next() {
        let member = match element.kind() {
            SyntaxKind::CONFIG_ITEM => ConfigBlockLayoutMember::Item(
                element
                    .node::<syntax_ast::ConfigItem>()
                    .expect("validated config item"),
            ),
            SyntaxKind::BLOCK_ATTRIBUTE => {
                let attribute = element
                    .node::<syntax_ast::BlockAttribute>()
                    .expect("validated block attribute");
                ConfigBlockLayoutMember::BlockAttribute(
                    BlockAttribute::from_cst(SyntaxElement::Node(attribute.syntax().clone()))
                        .expect("validated block attribute"),
                )
            }
            _ => continue,
        };
        items.push((member, take_delimiter(&mut elements)));
    }
    items
}

impl Printable for Validated<'_, syntax_ast::ConfigBlock> {
    /// [`ConfigBlock`] prints multi-line unless empty.
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let inner_indent = shape.indent + printer.config.indent_width;

        let mut items = config_block_items(self);
        if items.is_empty() {
            // Check if there's trivia inside the empty block (e.g. comments between { and })
            let open_brace = self.l_brace_token();
            let close_brace = self.r_brace_token();
            let (_, open_trailing) = printer.trivia.get_for_range_split(open_brace.span());
            let (close_leading, _) = printer.trivia.get_for_range_split(close_brace.span());
            let has_comments = open_trailing
                .iter()
                .chain(close_leading.iter())
                .any(EmittableTrivia::is_comment);

            if has_comments {
                printer.print_raw_token(&open_brace);
                printer.print_trivia_trailing(open_trailing);
                printer.print_newline();
                printer.print_trivia_with_newline(close_leading.trim_blanks(), inner_indent);
                printer.print_spaces(shape.indent);
                printer.print_raw_token(&close_brace);
                return PrintInfo::default_multi_lined();
            }
            printer.print_raw_token(&open_brace);
            printer.print_raw_token(&close_brace);
            return PrintInfo::default_single_line();
        }

        let inner_shape = Shape::standalone(printer.config.line_width, inner_indent);

        let open_brace = self.l_brace_token();
        let close_brace = self.r_brace_token();
        printer.print_raw_token(&open_brace);
        printer.print_trivia_all_trailing_for(open_brace.span());
        printer.print_newline();

        items.sort_by_cached_key(|(member, _)| match member {
            ConfigBlockLayoutMember::BlockAttribute(attribute) => (
                0,
                attribute
                    .name_parts_str(printer.input)
                    .collect::<Vec<_>>()
                    .join("."),
            ),
            ConfigBlockLayoutMember::Item(_) => (1, String::new()),
        });
        for (i, (item, comma)) in items.iter().enumerate() {
            let (item_leading, item_trailing) = printer.trivia.get_for_element(item);
            let item_leading = if i == 0 {
                item_leading.trim_leading_blanks() // this is first item
            } else {
                item_leading
            };

            printer.print_trivia_with_newline(item_leading, inner_indent);
            printer.print_spaces(inner_indent);
            printer.print(item, inner_shape.clone());

            match (item, comma) {
                (ConfigBlockLayoutMember::BlockAttribute(_), Some(comma)) => {
                    // remove the trailing comma, keep the comments
                    let (comma_leading, comma_trailing) =
                        printer.trivia.get_for_range_split(comma.span());
                    printer.print_trivia_trailing(item_trailing);
                    printer.print_trivia_trailing(comma_leading);
                    printer.print_trivia_trailing(comma_trailing);
                }
                (ConfigBlockLayoutMember::BlockAttribute(_), None) => {
                    // keep no comma, print trivia nicely
                    printer.print_trivia_trailing(item_trailing);
                }
                (_, Some(comma)) => {
                    // keep the comma, print trivia nicely
                    let (comma_leading, comma_trailing) =
                        printer.trivia.get_for_range_split(comma.span());
                    printer.print_trivia_squished(item_trailing);
                    printer.print_trivia_squished(comma_leading);
                    printer.print_raw_token(comma);
                    printer.print_trivia_trailing(comma_trailing);
                }
                (_, None) => {
                    // comma is inserted *before* the trailing trivia
                    printer.print_str(",");
                    printer.print_trivia_trailing(item_trailing);
                }
            }
            printer.print_newline();
        }

        let (close_brace_leading, _) = printer.trivia.get_for_range_split(close_brace.span());
        printer.print_trivia_with_newline(close_brace_leading.trim_trailing_blanks(), inner_indent);
        printer.print_spaces(shape.indent);
        printer.print_raw_token(&close_brace);

        PrintInfo::default_multi_lined()
    }
    fn leftmost_token(&self) -> TextRange {
        self.first_token_range()
    }
    fn rightmost_token(&self) -> TextRange {
        self.last_token_range()
    }
}

impl Printable for ConfigBlockLayoutMember<'_> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            ConfigBlockLayoutMember::Item(item) => item.print(shape, printer),
            ConfigBlockLayoutMember::BlockAttribute(attr) => attr.print(shape, printer),
        }
    }
    fn leftmost_token(&self) -> TextRange {
        match self {
            ConfigBlockLayoutMember::Item(item) => item.leftmost_token(),
            ConfigBlockLayoutMember::BlockAttribute(attr) => attr.leftmost_token(),
        }
    }
    fn rightmost_token(&self) -> TextRange {
        match self {
            ConfigBlockLayoutMember::Item(item) => item.rightmost_token(),
            ConfigBlockLayoutMember::BlockAttribute(attr) => attr.rightmost_token(),
        }
    }
}

enum ConfigItemKeyLayout {
    Token(ValidatedSyntaxToken),
    String(t::QuotedString),
}

impl Printable for ConfigItemKeyLayout {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            Self::Token(token) => {
                printer.print_raw_token(token);
                PrintInfo::default_single_line()
            }
            Self::String(string) => printer.print(string, shape),
        }
    }
    fn leftmost_token(&self) -> TextRange {
        match self {
            Self::Token(token) => token.span(),
            Self::String(string) => string.leftmost_token(),
        }
    }
    fn rightmost_token(&self) -> TextRange {
        match self {
            Self::Token(token) => token.span(),
            Self::String(string) => string.rightmost_token(),
        }
    }
}

enum ConfigItemValueLayout<'tree> {
    Value(Validated<'tree, syntax_ast::ConfigValue>),
    Block(Validated<'tree, syntax_ast::ConfigBlock>),
}

impl Printable for ConfigItemValueLayout<'_> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            Self::Value(value) => value.print(shape, printer),
            Self::Block(block) => block.print(shape, printer),
        }
    }
    fn leftmost_token(&self) -> TextRange {
        match self {
            Self::Value(value) => value.leftmost_token(),
            Self::Block(block) => block.leftmost_token(),
        }
    }
    fn rightmost_token(&self) -> TextRange {
        match self {
            Self::Value(value) => value.rightmost_token(),
            Self::Block(block) => block.rightmost_token(),
        }
    }
}

impl Printable for Validated<'_, syntax_ast::ConfigItem> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let first = self
            .direct_elements()
            .next()
            .expect("validated config item key");
        let key = if first.kind() == SyntaxKind::STRING_LITERAL {
            ConfigItemKeyLayout::String(
                t::QuotedString::from_cst(SyntaxElement::Node(
                    first
                        .node::<syntax_ast::StringLiteral>()
                        .expect("validated string config key")
                        .syntax()
                        .clone(),
                ))
                .expect("validated string config key"),
            )
        } else {
            ConfigItemKeyLayout::Token(first.token().expect("validated token config key"))
        };
        let value = self
            .config_value()
            .map(ConfigItemValueLayout::Value)
            .or_else(|| self.config_block().map(ConfigItemValueLayout::Block))
            .expect("validated config item value");
        let mut multi_lined = false;
        multi_lined |= printer.print(&key, shape.clone()).multi_lined;
        let colon_trailing = if let Some(colon) = self.colon_token() {
            let (_, colon_trailing) = printer.trivia.get_for_range_split(colon.span());
            colon_trailing
        } else {
            &[][..]
        };
        printer.print_str(": ");
        printer.print_trivia_squished(colon_trailing);
        let value_leading = printer.trivia.get_leading_for_element(&value);
        printer.print_trivia_squished(value_leading);
        let remaining_width = printer.current_line_remaining_width();
        let value_shape = Shape {
            width: remaining_width.saturating_sub(const { ",".len() }),
            indent: shape.indent,
            first_line_offset: printer
                .config
                .line_width
                .saturating_sub(shape.indent + remaining_width),
        };
        multi_lined |= printer.print(&value, value_shape).multi_lined;
        for attribute in self.attribute() {
            let attribute =
                super::Attribute::from_cst(SyntaxElement::Node(attribute.syntax().clone()))
                    .expect("validated config item attribute");
            let leading = printer.trivia.get_leading_for_element(&attribute);
            printer.print_str(" ");
            printer.print_trivia_squished(leading);
            multi_lined |= printer.print(&attribute, shape.clone()).multi_lined;
        }
        PrintInfo { multi_lined }
    }
    fn leftmost_token(&self) -> TextRange {
        self.first_token_range()
    }
    fn rightmost_token(&self) -> TextRange {
        self.last_token_range()
    }
}

impl Printable for Validated<'_, syntax_ast::ConfigValue> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        if let Some(array) = self.config_array() {
            return array.print(shape, printer);
        }
        if let Some(block) = self.config_block() {
            return block.print(shape, printer);
        }
        let expr = self.expr().expect("validated config expression");
        let expr = super::Expression::from_cst(SyntaxElement::Node(expr.syntax().clone()))
            .expect("validated config expression");
        expr.print(shape, printer)
    }
    fn leftmost_token(&self) -> TextRange {
        self.first_token_range()
    }
    fn rightmost_token(&self) -> TextRange {
        self.last_token_range()
    }
}

enum ConfigArrayElement<'tree> {
    Value(Validated<'tree, syntax_ast::ConfigValue>),
    Block(Validated<'tree, syntax_ast::ConfigBlock>),
}

impl Printable for ConfigArrayElement<'_> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            Self::Value(value) => value.print(shape, printer),
            Self::Block(block) => block.print(shape, printer),
        }
    }
    fn leftmost_token(&self) -> TextRange {
        match self {
            Self::Value(value) => value.leftmost_token(),
            Self::Block(block) => block.leftmost_token(),
        }
    }
    fn rightmost_token(&self) -> TextRange {
        match self {
            Self::Value(value) => value.rightmost_token(),
            Self::Block(block) => block.rightmost_token(),
        }
    }
}

fn config_array_elements<'tree>(
    array: &Validated<'tree, syntax_ast::ConfigArray>,
) -> Vec<(ConfigArrayElement<'tree>, Option<ValidatedSyntaxToken>)> {
    let mut direct = array.direct_elements().peekable();
    let mut elements = Vec::new();
    while let Some(element) = direct.next() {
        let value = match element.kind() {
            SyntaxKind::CONFIG_VALUE => ConfigArrayElement::Value(
                element
                    .node::<syntax_ast::ConfigValue>()
                    .expect("validated config array value"),
            ),
            SyntaxKind::CONFIG_BLOCK => ConfigArrayElement::Block(
                element
                    .node::<syntax_ast::ConfigBlock>()
                    .expect("validated config array block"),
            ),
            _ => continue,
        };
        elements.push((value, take_delimiter(&mut direct)));
    }
    elements
}

impl PrintMultiLine for Validated<'_, syntax_ast::ConfigArray> {
    /// Multi-line layout: each element on its own indented line with trailing comma.
    /// Brackets wrap the entire construct.
    ///
    /// ```baml
    /// [
    ///     some_long_expression,
    ///     another_expression,
    /// ]
    /// ```
    fn print_multi_line(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let inner_shape = Shape {
            width: shape.width.saturating_sub(printer.config.indent_width),
            indent: shape.indent + printer.config.indent_width,
            first_line_offset: 0,
        };

        let open_bracket = self.l_bracket_token();
        let close_bracket = self.r_bracket_token();
        let elements = config_array_elements(self);
        printer.print_raw_token(&open_bracket);
        printer.print_trivia_all_trailing_for(open_bracket.span());
        printer.print_newline();

        for (elem, comma) in &elements {
            let (elem_leading, elem_trailing) = printer.trivia.get_for_element(elem);
            printer
                .print_trivia_with_newline(elem_leading.trim_leading_blanks(), inner_shape.indent);
            printer.print_spaces(inner_shape.indent);
            printer.print(elem, inner_shape.clone());
            if let Some(comma) = comma {
                printer.print_trivia_squished(elem_trailing);
                let (comma_leading, comma_trailing) =
                    printer.trivia.get_for_range_split(comma.span());
                printer.print_trivia_squished(comma_leading);
                printer.print_raw_token(comma);
                printer.print_trivia_trailing(comma_trailing);
            } else {
                printer.print_str(",");
                printer.print_trivia_trailing(elem_trailing);
            }
            printer.print_newline();
        }

        printer.print_trivia_all_leading_with_newline_for(close_bracket.span(), inner_shape.indent);
        printer.print_spaces(shape.indent);
        printer.print_raw_token(&close_bracket);
        PrintInfo::default_multi_lined()
    }
}

impl ConfigArrayLayout for Validated<'_, syntax_ast::ConfigArray> {
    /// Should be passed a sub-printer to avoid printing trivia in the outer printer
    /// in the event that the printer is unable to fit the config array on a single line.
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        let open_bracket = self.l_bracket_token();
        let close_bracket = self.r_bracket_token();
        let elements = config_array_elements(self);
        printer.print_raw_token(&open_bracket);
        let (_, open_trailing) = printer.trivia.get_for_range_split(open_bracket.span());
        printer.try_print_trivia_single_line_squished(open_trailing)?;

        for (i, (elem, comma)) in elements.iter().enumerate() {
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
            printer.try_print_trivia_single_line_squished(el_trailing)?;
            if i + 1 < elements.len() {
                // not the last element: will have comma
                if let Some(comma) = comma {
                    let (comma_leading, comma_trailing) =
                        printer.trivia.get_for_range_split(comma.span());
                    printer.print_trivia_squished(comma_leading);
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

        let (close_leading, _) = printer.trivia.get_for_range_split(close_bracket.span());
        printer.try_print_trivia_single_line_squished(close_leading)?;
        printer.print_raw_token(&close_bracket);

        if printer.output.len() > shape.width {
            None
        } else {
            Some(PrintInfo::default_single_line())
        }
    }
}

impl Printable for Validated<'_, syntax_ast::ConfigArray> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|p| self.try_print_single_line(&shape, p))
            .unwrap_or_else(|| self.print_multi_line(shape, printer))
    }
    fn leftmost_token(&self) -> TextRange {
        self.first_token_range()
    }
    fn rightmost_token(&self) -> TextRange {
        self.last_token_range()
    }
}

impl Printable for Validated<'_, syntax_ast::TestDef> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let config = self.config_block();
        printer.print_raw_token(&self.test_token());
        printer.print_str(" ");
        printer.print_raw_token(&self.name_token());
        printer.print_str(" ");
        printer.print(&config, shape)
    }
    fn leftmost_token(&self) -> TextRange {
        self.first_token_range()
    }
    fn rightmost_token(&self) -> TextRange {
        self.last_token_range()
    }
}

impl Printable for Validated<'_, syntax_ast::TestExprDef> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let name = super::Expression::from_cst(SyntaxElement::Node(self.name().syntax().clone()))
            .expect("validated test name");
        let with_value = self.with_value().map(|value| {
            super::Expression::from_cst(SyntaxElement::Node(value.syntax().clone()))
                .expect("validated test runner")
        });
        let body = super::BlockExpr::from_cst(SyntaxElement::Node(self.body().syntax().clone()))
            .expect("validated test body");
        printer.print_raw_token(&self.test_token());
        printer.print_str(" ");
        printer.print(&name, shape.clone());
        if let Some((keyword, value)) = self.with_token().zip(with_value.as_ref()) {
            printer.print_str(" ");
            printer.print_raw_token(&keyword);
            printer.print_str(" ");
            printer.print(value, shape.clone());
        }
        printer.print_str(" ");
        printer.print(&body, shape)
    }
    fn leftmost_token(&self) -> TextRange {
        self.first_token_range()
    }
    fn rightmost_token(&self) -> TextRange {
        self.last_token_range()
    }
}

impl Printable for Validated<'_, syntax_ast::TestsetDef> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let name = super::Expression::from_cst(SyntaxElement::Node(self.name().syntax().clone()))
            .expect("validated test set name");
        let with_value = self.with_value().map(|value| {
            super::Expression::from_cst(SyntaxElement::Node(value.syntax().clone()))
                .expect("validated test set runner")
        });
        let body = super::BlockExpr::from_cst(SyntaxElement::Node(self.body().syntax().clone()))
            .expect("validated test set body");
        printer.print_raw_token(&self.testset_token());
        printer.print_str(" ");
        printer.print(&name, shape.clone());
        if let Some((keyword, value)) = self.with_token().zip(with_value.as_ref()) {
            printer.print_str(" ");
            printer.print_raw_token(&keyword);
            printer.print_str(" ");
            printer.print(value, shape.clone());
        }
        printer.print_str(" ");
        printer.print(&body, shape)
    }
    fn leftmost_token(&self) -> TextRange {
        self.first_token_range()
    }
    fn rightmost_token(&self) -> TextRange {
        self.last_token_range()
    }
}

impl Printable for syntax_ast::TestExprDef {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let mut expressions = self.expressions();
        let name = super::Expression::from_cst(SyntaxElement::Node(
            expressions
                .next()
                .expect("validated test name")
                .syntax()
                .clone(),
        ))
        .expect("validated test name");
        let with_value = self.with_token().map(|_| {
            super::Expression::from_cst(SyntaxElement::Node(
                expressions
                    .next()
                    .expect("validated test runner")
                    .syntax()
                    .clone(),
            ))
            .expect("validated test runner")
        });
        let body = super::BlockExpr::from_cst(SyntaxElement::Node(
            self.body().expect("validated test body").syntax().clone(),
        ))
        .expect("validated test body");
        printer.print_raw_token(&RawToken(
            self.test_token().expect("validated test keyword"),
        ));
        printer.print_str(" ");
        printer.print(&name, shape.clone());
        if let Some((keyword, value)) = self.with_token().zip(with_value.as_ref()) {
            printer.print_str(" ");
            printer.print_raw_token(&RawToken(keyword));
            printer.print_str(" ");
            printer.print(value, shape.clone());
        }
        printer.print_str(" ");
        printer.print(&body, shape)
    }

    fn leftmost_token(&self) -> TextRange {
        self.syntax()
            .first_token()
            .expect("validated test")
            .text_range()
    }

    fn rightmost_token(&self) -> TextRange {
        self.syntax()
            .last_token()
            .expect("validated test")
            .text_range()
    }
}

impl Printable for syntax_ast::TestsetDef {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let mut expressions = self.expressions();
        let name = super::Expression::from_cst(SyntaxElement::Node(
            expressions
                .next()
                .expect("validated test set name")
                .syntax()
                .clone(),
        ))
        .expect("validated test set name");
        let with_value = self.with_token().map(|_| {
            super::Expression::from_cst(SyntaxElement::Node(
                expressions
                    .next()
                    .expect("validated test set runner")
                    .syntax()
                    .clone(),
            ))
            .expect("validated test set runner")
        });
        let body = super::BlockExpr::from_cst(SyntaxElement::Node(
            self.body()
                .expect("validated test set body")
                .syntax()
                .clone(),
        ))
        .expect("validated test set body");
        printer.print_raw_token(&RawToken(
            self.testset_token().expect("validated test set keyword"),
        ));
        printer.print_str(" ");
        printer.print(&name, shape.clone());
        if let Some((keyword, value)) = self.with_token().zip(with_value.as_ref()) {
            printer.print_str(" ");
            printer.print_raw_token(&RawToken(keyword));
            printer.print_str(" ");
            printer.print(value, shape.clone());
        }
        printer.print_str(" ");
        printer.print(&body, shape)
    }

    fn leftmost_token(&self) -> TextRange {
        self.syntax()
            .first_token()
            .expect("validated test set")
            .text_range()
    }

    fn rightmost_token(&self) -> TextRange {
        self.syntax()
            .last_token()
            .expect("validated test set")
            .text_range()
    }
}

impl Printable for Validated<'_, syntax_ast::RetryPolicyDef> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let config = self.config_block();
        printer.print_raw_token(&self.retry_policy_token());
        printer.print_str(" ");
        printer.print_raw_token(&self.name_token());
        printer.print_str(" ");
        printer.print(&config, shape)
    }
    fn leftmost_token(&self) -> TextRange {
        self.first_token_range()
    }
    fn rightmost_token(&self) -> TextRange {
        self.last_token_range()
    }
}

impl Printable for Validated<'_, syntax_ast::TemplateStringDef> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let args = self.parameter_list();
        let body = t::RawString::from_cst(SyntaxElement::Node(
            self.raw_string_literal().syntax().clone(),
        ))
        .expect("validated template body");
        let mut multi_lined = false;

        printer.print_raw_token(&self.template_string_token());
        printer.print_str(" ");
        printer.print_raw_token(&self.name_token());
        multi_lined |= printer.print(&args, shape).multi_lined;
        printer.print_str(" ");
        multi_lined |= printer
            .print(&body, Shape::unlimited_single_line())
            .multi_lined;
        PrintInfo { multi_lined }
    }
    fn leftmost_token(&self) -> TextRange {
        self.first_token_range()
    }
    fn rightmost_token(&self) -> TextRange {
        self.last_token_range()
    }
}

impl Printable for Validated<'_, syntax_ast::TypeAliasDef> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let keyword = self.type_token();
        let name = self.name_token();
        let equals = self.equals_token();
        let type_expr =
            super::Type::from_cst(SyntaxElement::Node(self.type_expr().syntax().clone()))
                .expect("validated type expression");
        let semicolon = self.semicolon_token();
        printer.print_raw_token(&keyword);
        printer.print_str(" ");
        printer.print_raw_token(&name);
        printer.print_str(" ");
        printer.print_raw_token(&equals);
        printer.print_str(" ");
        let (_, eq_trailing) = printer.trivia.get_for_range_split(equals.span());
        let (ty_leading, ty_trailing) = printer.trivia.get_for_element(&type_expr);
        let mut ty_leading_len = printer.print_trivia_squished(eq_trailing);
        ty_leading_len += printer.print_trivia_squished(ty_leading);
        let new_offset = usize::from(keyword.span().len() + name.span().len())
            + const { "  = ".len() }
            + ty_leading_len;

        let info;
        if let Some(semicolon) = semicolon {
            let (semicolon_leading, _) = printer.trivia.get_for_range_split(semicolon.span());
            let mut ty_trailing_len = ty_trailing.squished_len(printer.input);
            ty_trailing_len += semicolon_leading.squished_len(printer.input);
            let ty_shape = Shape {
                width: shape
                    .width
                    .saturating_sub(new_offset + ty_trailing_len + const { ";".len() }),
                indent: shape.indent,
                first_line_offset: shape.first_line_offset + new_offset,
            };
            info = printer.print(&type_expr, ty_shape);
            printer.print_trivia_squished(ty_trailing);
            printer.print_trivia_squished(semicolon_leading);
            printer.print_raw_token(&semicolon);
        } else {
            let ty_shape = Shape {
                width: shape.width.saturating_sub(new_offset + const { ";".len() }),
                indent: shape.indent,
                first_line_offset: shape.first_line_offset + new_offset,
            };
            info = printer.print(&type_expr, ty_shape);
            // this is the last child so trivia is handled by parent
            printer.print_str(";");
        }

        info
    }
    fn leftmost_token(&self) -> TextRange {
        self.first_token_range()
    }
    fn rightmost_token(&self) -> TextRange {
        self.last_token_range()
    }
}

impl Printable for Validated<'_, syntax_ast::GeneratorDef> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let config = self.config_block();
        printer.print_raw_token(&self.generator_token());
        printer.print_str(" ");
        printer.print_raw_token(&self.name_token());
        printer.print_str(" ");
        printer.print(&config, shape)
    }
    fn leftmost_token(&self) -> TextRange {
        self.first_token_range()
    }
    fn rightmost_token(&self) -> TextRange {
        self.last_token_range()
    }
}
