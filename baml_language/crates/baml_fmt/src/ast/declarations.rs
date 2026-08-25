use baml_db::baml_compiler_syntax::validated::nodes::{
    AssociatedTypeDecl, ClassDecl, ClassField, ClassFieldDelimiter, ClassItem, ClientDecl,
    ClientField, ClientName, ClientType, ConfigArray, ConfigBlock, ConfigBlockMember, ConfigItem,
    ConfigItemKey, ConfigItemValue, EnumDecl, EnumItem, EnumVariant, FunctionDecl,
    FunctionDeclBody, FunctionParam, FunctionParamList, GeneratorDecl, ImplementsBlock,
    ImplementsItem, ImplementsTarget, InterfaceFieldLink, LlmFunctionBody, PromptField,
    RetryPolicyDecl, StringLiteralValue, TemplateStringDecl, TestDecl, TestExprDecl, TestSetDecl,
    ToolsField, TopLevelDeclaration, TypeAliasDecl,
};
use rowan::TextRange;

use super::expressions::FunctionArrowLayout as _;
use crate::{
    EmittableTrivia,
    ast::{BlockAttribute, Token, tokens as t},
    printer::{PrintInfo, PrintMultiLine, Printable, Printer, Shape},
    trivia_classifier::TriviaSliceExt as _,
};

trait FunctionParamListLayout {
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo>;
}

trait ClassFieldLayout {
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo>;
}

trait EnumVariantLayout {
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo>;
}

trait ConfigArrayLayout {
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo>;
}

impl Printable for TopLevelDeclaration {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            TopLevelDeclaration::Function(function_decl) => function_decl.print(shape, printer),
            TopLevelDeclaration::Class(class_decl) => class_decl.print(shape, printer),
            TopLevelDeclaration::Enum(enum_decl) => enum_decl.print(shape, printer),
            TopLevelDeclaration::Client(client_decl) => client_decl.print(shape, printer),
            TopLevelDeclaration::Test(test_decl) => test_decl.print(shape, printer),
            TopLevelDeclaration::TestExpr(test_expr_decl) => test_expr_decl.print(shape, printer),
            TopLevelDeclaration::TestSet(test_set_decl) => test_set_decl.print(shape, printer),
            TopLevelDeclaration::RetryPolicy(retry_policy_decl) => {
                retry_policy_decl.print(shape, printer)
            }
            TopLevelDeclaration::TemplateString(template_string) => {
                template_string.print(shape, printer)
            }
            TopLevelDeclaration::TypeAlias(type_alias_decl) => {
                type_alias_decl.print(shape, printer)
            }
            TopLevelDeclaration::Generator(generator_decl) => generator_decl.print(shape, printer),
            TopLevelDeclaration::Unknown(range) => {
                let text = &printer.input[*range];
                printer.print_str(text.trim());
                PrintInfo::default_multi_lined()
            }
        }
    }
    fn leftmost_token(&self) -> TextRange {
        match self {
            TopLevelDeclaration::Function(f) => f.leftmost_token(),
            TopLevelDeclaration::Class(c) => c.leftmost_token(),
            TopLevelDeclaration::Enum(e) => e.leftmost_token(),
            TopLevelDeclaration::Client(c) => c.leftmost_token(),
            TopLevelDeclaration::Test(t) => t.leftmost_token(),
            TopLevelDeclaration::TestExpr(t) => t.leftmost_token(),
            TopLevelDeclaration::TestSet(t) => t.leftmost_token(),
            TopLevelDeclaration::RetryPolicy(r) => r.leftmost_token(),
            TopLevelDeclaration::TemplateString(t) => t.leftmost_token(),
            TopLevelDeclaration::TypeAlias(t) => t.leftmost_token(),
            TopLevelDeclaration::Generator(g) => g.leftmost_token(),
            TopLevelDeclaration::Unknown(range) => *range,
        }
    }
    fn rightmost_token(&self) -> TextRange {
        match self {
            TopLevelDeclaration::Function(f) => f.rightmost_token(),
            TopLevelDeclaration::Class(c) => c.rightmost_token(),
            TopLevelDeclaration::Enum(e) => e.rightmost_token(),
            TopLevelDeclaration::Client(c) => c.rightmost_token(),
            TopLevelDeclaration::Test(t) => t.rightmost_token(),
            TopLevelDeclaration::TestExpr(t) => t.rightmost_token(),
            TopLevelDeclaration::TestSet(t) => t.rightmost_token(),
            TopLevelDeclaration::RetryPolicy(r) => r.rightmost_token(),
            TopLevelDeclaration::TemplateString(t) => t.rightmost_token(),
            TopLevelDeclaration::TypeAlias(t) => t.rightmost_token(),
            TopLevelDeclaration::Generator(g) => g.rightmost_token(),
            TopLevelDeclaration::Unknown(range) => *range,
        }
    }
}

impl Printable for FunctionDecl {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.keyword);
        printer.print_str(" ");
        printer.print_raw_token(&self.name);
        if let Some(ref gp) = self.generic_params {
            printer.print(gp, shape.clone());
        }

        let mut param_printer = Printer::new_empty(printer.input, printer.config, printer.trivia);
        let param_info = param_printer.print(&self.params, Shape::unlimited_single_line());

        let mut return_type_printer =
            Printer::new_empty(printer.input, printer.config, printer.trivia);
        let return_type_info =
            return_type_printer.print(&self.return_type, Shape::unlimited_single_line());
        let (_, return_type_line_comment) =
            return_type_printer.print_trivia_all_trailing_for(self.return_type.rightmost_token());
        let mut throws_printer = Printer::new_empty(printer.input, printer.config, printer.trivia);
        let throws_info = self
            .throws
            .as_ref()
            .map(|throws| throws_printer.print(throws, Shape::unlimited_single_line()))
            .unwrap_or_else(PrintInfo::default_single_line);

        let single_line_size = printer.current_line_len()
            + param_printer.output.len()
            + const { " -> ".len() + " {".len() }
            + return_type_printer.output.len()
            + if self.throws.is_some() {
                (const { " ".len() }) + throws_printer.output.len()
            } else {
                0
            };
        if single_line_size <= printer.config.line_width
            && !param_info.multi_lined
            && !return_type_info.multi_lined
            && !throws_info.multi_lined
            && !return_type_line_comment
        {
            // It fits in single line!
            printer.append_from_printer(param_printer);
            printer.print_spaces(1);
            // Normalize the permissively accepted `=>` spelling to `->`.
            printer.print_str("->");
            self.arrow.print_separator_before(
                Some(self.return_type.leftmost_token()),
                shape.indent + printer.config.indent_width,
                printer,
            );
            printer.append_from_printer(return_type_printer);
            if self.throws.is_some() {
                printer.print_spaces(1);
                printer.append_from_printer(throws_printer);
            }
            printer.print_spaces(1);
            printer.print(&self.body, shape)
        } else {
            let params_shape = Shape {
                width: 0, // never single-line
                indent: shape.indent,
                first_line_offset: 0, // not important in function args
            };
            let _ = self.params.print_multi_line(params_shape, printer);

            printer.print_spaces(1);
            // Normalize the permissively accepted `=>` spelling to `->`.
            printer.print_str("->");
            self.arrow.print_separator_before(
                Some(self.return_type.leftmost_token()),
                shape.indent + printer.config.indent_width,
                printer,
            );

            let curr_line_len = printer.current_line_len();
            let return_type_shape = Shape {
                width: printer
                    .config
                    .line_width
                    .saturating_sub(curr_line_len + const { " {".len() }),
                indent: shape.indent,
                first_line_offset: curr_line_len.saturating_sub(shape.indent),
            };

            let return_info = self.return_type.print(return_type_shape, printer);
            let (_, return_type_line_comment) =
                printer.print_trivia_all_trailing_for(self.return_type.rightmost_token());
            let throws_info = if let Some(ref throws) = self.throws {
                printer.print_str(" ");
                printer.print(throws, shape.clone())
            } else {
                PrintInfo::default_single_line()
            };

            if (return_info.multi_lined && self.return_type.multi_line_is_indented())
                || throws_info.multi_lined
                || return_type_line_comment
            {
                // `{` goes on its own line after the type ends
                printer.print_newline();
            } else {
                printer.print_str(" ");
            }

            printer.print(&self.body, shape);

            PrintInfo::default_multi_lined()
        }
    }
    fn leftmost_token(&self) -> TextRange {
        self.keyword.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.body.rightmost_token()
    }
}

impl PrintMultiLine for FunctionParamList {
    /// Multi-line layout: each parameter on its own indented line with trailing comma.
    /// Closing paren on its own line.
    ///
    /// ```baml
    /// (
    ///     first: string,
    ///     second: int,
    ///     third: bool,
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

        for (param, comma) in &self.params {
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

        let (close_paren_leading, _) = printer.trivia.get_for_range_split(self.close_paren.span());
        printer.print_trivia_with_newline(close_paren_leading.trim_blanks(), inner_shape.indent);
        printer.print_spaces(shape.indent);
        printer.print_raw_token(&self.close_paren);
        PrintInfo::default_multi_lined()
    }
}

impl FunctionParamListLayout for FunctionParamList {
    /// Should be passed a sub-printer to avoid printing trivia in the outer printer
    /// in the event that the printer is unable to fit the function param list on a single line.
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        printer.print_raw_token(&self.open_paren);
        let (_, open_trailing) = printer.trivia.get_for_range_split(self.open_paren.span());
        printer.try_print_trivia_single_line_squished(open_trailing)?;

        for (i, (param, comma)) in self.params.iter().enumerate() {
            if printer.output.len() > shape.width {
                return None;
            }
            let (p_leading, p_trailing) = printer.trivia.get_for_element(param);
            printer.try_print_trivia_single_line_squished(p_leading)?;
            if printer
                .print(param, Shape::unlimited_single_line())
                .multi_lined
            {
                return None;
            }

            let (comma_leading, comma_trailing) = if let Some(comma) = comma {
                printer.trivia.get_for_range_split(comma.span())
            } else {
                (&[][..], &[][..])
            };
            if i + 1 < self.params.len() {
                printer.print_trivia_squished(p_trailing);
                printer.print_trivia_squished(comma_leading);
                printer.print_str(", ");
                printer.try_print_trivia_single_line_squished(comma_trailing)?;
            } else {
                // Trailing comma is removed in single-line mode, but we still try the comments.
                printer.try_print_trivia_single_line_squished(p_trailing)?;
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
}

impl Printable for FunctionParamList {
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

impl Printable for FunctionParam {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.name);
        let mut info = if let Some((colon, ty)) = &self.ty {
            let mut trivia_len = 0;
            // Colon is optional per BEP-019; synthesize if absent
            if let Some(colon) = colon {
                let (_, colon_trailing) = printer.trivia.get_for_range_split(colon.span());
                printer.print_str(": ");
                trivia_len += printer.print_trivia_squished(colon_trailing);
            } else {
                printer.print_str(": ");
            }
            let ty_leading = printer.trivia.get_leading_for_element(ty);
            trivia_len += printer.print_trivia_squished(ty_leading);

            let new_offset = usize::from(self.name.span().len()) + 2 + trivia_len;
            let ty_shape = Shape {
                width: shape.width.saturating_sub(new_offset),
                indent: shape.indent,
                first_line_offset: shape.first_line_offset + new_offset,
            };
            ty.print(ty_shape, printer)
        } else {
            PrintInfo::default_single_line()
        };

        if let Some((equals, default)) = &self.default {
            let prev_token = self
                .ty
                .as_ref()
                .map_or_else(|| self.name.span(), |(_, ty)| ty.rightmost_token());
            let (_, prev_trailing) = printer.trivia.get_for_range_split(prev_token);
            let (equals_leading, equals_trailing) =
                printer.trivia.get_for_range_split(equals.span());
            printer.print_trivia_squished(prev_trailing);
            printer.print_trivia_squished(equals_leading);
            printer.print_str(" = ");
            printer.print_trivia_squished(equals_trailing);
            let leading = printer.trivia.get_leading_for_element(default);
            printer.print_trivia_squished(leading);
            info = printer.print(default, shape);
        }

        info
    }
    fn leftmost_token(&self) -> TextRange {
        self.name.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.default.as_ref().map_or_else(
            || {
                self.ty
                    .as_ref()
                    .map_or(self.name.span(), |(_, ty)| ty.rightmost_token())
            },
            |(_, default)| default.rightmost_token(),
        )
    }
}

impl Printable for FunctionDeclBody {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            FunctionDeclBody::Llm(llm) => llm.print(shape, printer),
            FunctionDeclBody::Block(block) => block.print(shape, printer),
        }
    }
    fn leftmost_token(&self) -> TextRange {
        match self {
            FunctionDeclBody::Llm(llm) => llm.leftmost_token(),
            FunctionDeclBody::Block(block) => block.leftmost_token(),
        }
    }
    fn rightmost_token(&self) -> TextRange {
        match self {
            FunctionDeclBody::Llm(llm) => llm.rightmost_token(),
            FunctionDeclBody::Block(block) => block.rightmost_token(),
        }
    }
}

impl Printable for LlmFunctionBody {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let inner_indent = shape.indent + printer.config.indent_width;

        printer.print_raw_token(&self.open_brace);
        printer.print_trivia_all_trailing_for(self.open_brace.span());
        printer.print_newline();

        let (client_leading, client_trailing) = printer.trivia.get_for_element(&self.client);
        printer.print_trivia_with_newline(client_leading.trim_leading_blanks(), inner_indent);
        printer.print_spaces(inner_indent);
        let inner_shape = Shape::standalone(printer.config.line_width, inner_indent);
        self.client.print(inner_shape, printer);
        printer.print_trivia_trailing(client_trailing);
        printer.print_newline();

        if let Some(tools) = &self.tools {
            printer.print_standalone_with_trivia(tools, inner_indent);
            printer.print_newline();
        }

        printer.print_standalone_with_trivia(&self.prompt, inner_indent);
        printer.print_newline();

        let (close_brace_leading, _) = printer.trivia.get_for_range_split(self.close_brace.span());
        printer.print_trivia_with_newline(close_brace_leading.trim_trailing_blanks(), inner_indent);
        printer.print_spaces(shape.indent);
        printer.print_raw_token(&self.close_brace);

        PrintInfo::default_multi_lined()
    }
    fn leftmost_token(&self) -> TextRange {
        self.open_brace.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.close_brace.span()
    }
}

impl Printable for ClientField {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.keyword);
        let (_, keyword_trailing) = printer.trivia.get_for_range_split(self.keyword.span());
        printer.print_trivia_squished(keyword_trailing);
        let (colon_leading, colon_trailing) = printer.trivia.get_for_range_split(self.colon.span());
        printer.print_trivia_squished(colon_leading);
        printer.print_str(": ");
        printer.print_trivia_squished(colon_trailing);
        let name_leading = printer.trivia.get_leading_for_element(&self.name);
        printer.print_trivia_squished(name_leading);
        printer.print(&self.name, shape)
    }
    fn leftmost_token(&self) -> TextRange {
        self.keyword.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.name.rightmost_token()
    }
}

impl Printable for ClientName {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            ClientName::Path(path) => printer.print(path, shape),
            ClientName::String(string) => printer.print(string, shape),
            ClientName::Expr(expr) => printer.print(expr.as_ref(), shape),
        }
    }
    fn leftmost_token(&self) -> TextRange {
        match self {
            ClientName::Path(path) => path.leftmost_token(),
            ClientName::String(string) => string.leftmost_token(),
            ClientName::Expr(expr) => expr.leftmost_token(),
        }
    }
    fn rightmost_token(&self) -> TextRange {
        match self {
            ClientName::Path(path) => path.rightmost_token(),
            ClientName::String(string) => string.rightmost_token(),
            ClientName::Expr(expr) => expr.rightmost_token(),
        }
    }
}

impl Printable for PromptField {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.prompt);
        let (_, prompt_trailing) = printer.trivia.get_for_range_split(self.prompt.span());
        printer.print_trivia_squished(prompt_trailing);
        let (colon_leading, colon_trailing) = printer.trivia.get_for_range_split(self.colon.span());
        printer.print_trivia_squished(colon_leading);
        printer.print_str(": ");
        printer.print_trivia_squished(colon_trailing);
        let string_leading = printer.trivia.get_leading_for_element(&self.string);
        printer.print_trivia_squished(string_leading);
        printer.print(&self.string, shape)
    }
    fn leftmost_token(&self) -> TextRange {
        self.prompt.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.string.rightmost_token()
    }
}

impl Printable for ToolsField {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.keyword);
        let (_, keyword_trailing) = printer.trivia.get_for_range_split(self.keyword.span());
        printer.print_trivia_squished(keyword_trailing);
        let (colon_leading, colon_trailing) = printer.trivia.get_for_range_split(self.colon.span());
        printer.print_trivia_squished(colon_leading);
        printer.print_str(": ");
        printer.print_trivia_squished(colon_trailing);
        let value_leading = printer.trivia.get_leading_for_element(&self.value);
        printer.print_trivia_squished(value_leading);
        printer.print(&self.value, shape)
    }
    fn leftmost_token(&self) -> TextRange {
        self.keyword.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.value.rightmost_token()
    }
}

impl Printable for StringLiteralValue {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            StringLiteralValue::RawString(raw_string) => printer.print(raw_string, shape),
            StringLiteralValue::String(string) => printer.print(string, shape),
            StringLiteralValue::Backtick(backtick) => printer.print(backtick, shape),
        }
    }
    fn leftmost_token(&self) -> TextRange {
        match self {
            StringLiteralValue::RawString(raw_string) => raw_string.leftmost_token(),
            StringLiteralValue::String(string) => string.leftmost_token(),
            StringLiteralValue::Backtick(backtick) => backtick.leftmost_token(),
        }
    }
    fn rightmost_token(&self) -> TextRange {
        match self {
            StringLiteralValue::RawString(raw_string) => raw_string.rightmost_token(),
            StringLiteralValue::String(string) => string.rightmost_token(),
            StringLiteralValue::Backtick(backtick) => backtick.rightmost_token(),
        }
    }
}

impl Printable for ClassDecl {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let inner_indent = shape.indent + printer.config.indent_width;

        printer.print_raw_token(&self.keyword);
        printer.print_str(" ");
        printer.print_raw_token(&self.name);
        if let Some(ref gp) = self.generic_params {
            printer.print(gp, shape.clone());
        }
        printer.print_str(" ");
        printer.print_raw_token(&self.open_brace);
        printer.print_trivia_all_trailing_for(self.open_brace.span());
        printer.print_newline();

        if let Some((first, rest)) = self.items.split_first() {
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

        let (close_brace_leading, _) = printer.trivia.get_for_range_split(self.close_brace.span());
        printer.print_trivia_with_newline(close_brace_leading.trim_trailing_blanks(), inner_indent);
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

impl PrintMultiLine for ClassField {
    /// Multi-line layout: attributes wrap to their own indented lines
    /// below the field name and type. Trailing comments on the type are preserved.
    ///
    /// ```baml
    /// myField ReallyLongTypeName // trailing comment
    ///     @alias("theLongField")
    ///     @description("some desc")
    /// ```
    fn print_multi_line(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let attr_shape = Shape::standalone(
            printer.config.line_width,
            shape.indent + printer.config.indent_width,
        );

        printer.print_raw_token(&self.name);
        let colon_trailing = if let Some(colon) = &self.colon {
            let (_, colon_trailing) = printer.trivia.get_for_range_split(colon.span());
            colon_trailing
        } else {
            &[][..]
        };
        printer.print_str(": ");
        printer.print_trivia_squished(colon_trailing);

        let (type_leading, type_trailing) = printer.trivia.get_for_element(&self.ty);
        printer.print_trivia_squished(type_leading);
        printer.print(&self.ty, shape);
        if !self.attributes.is_empty() {
            // we have attributes, they will be on their own lines so we can print the trailing trivia
            printer.print_trivia_trailing(type_trailing);
        }

        for (i, attr) in self.attributes.iter().enumerate() {
            let (attr_leading, attr_trailing) = printer.trivia.get_for_element(attr);
            printer.print_newline();
            printer.print_trivia_with_newline(attr_leading.trim_blanks(), attr_shape.indent);
            printer.print_spaces(attr_shape.indent);
            printer.print(attr, attr_shape.clone());
            let is_last = i + 1 >= self.attributes.len();
            if !is_last {
                // we have more attributes, so we can print the trailing trivia
                printer.print_trivia_trailing(attr_trailing);
            }
        }

        PrintInfo::default_multi_lined()
    }
}

impl ClassFieldLayout for ClassField {
    /// Should be passed a sub-printer to avoid printing trivia in the outer printer
    /// in the event that the printer is unable to fit the class field on a single line.
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        printer.print_raw_token(&self.name);
        let colon_trailing = if let Some(colon) = &self.colon {
            let (_, colon_trailing) = printer.trivia.get_for_range_split(colon.span());
            colon_trailing
        } else {
            &[][..]
        };
        printer.print_str(": ");
        printer.try_print_trivia_single_line_squished(colon_trailing)?;

        let (type_leading, type_trailing) = printer.trivia.get_for_element(&self.ty);
        printer.print_trivia_squished(type_leading);
        if self
            .ty
            .print(Shape::unlimited_single_line(), printer)
            .multi_lined
            || printer.len() > shape.width
        {
            return None;
        }
        if !self.attributes.is_empty() {
            // type is not the last element
            printer.try_print_trivia_single_line_squished(type_trailing)?;
        }

        for (i, attr) in self.attributes.iter().enumerate() {
            printer.print_str(" ");
            let (attr_leading, attr_trailing) = printer.trivia.get_for_element(attr);
            printer.try_print_trivia_single_line_squished(attr_leading)?;
            if printer
                .print(attr, Shape::unlimited_single_line())
                .multi_lined
            {
                return None;
            }
            let is_last = i + 1 >= self.attributes.len();
            if !is_last {
                // not last, we could take up the rest of the line if multilined
                printer.try_print_trivia_single_line_squished(attr_trailing)?;
            }
        }

        if printer.len() > shape.width {
            None
        } else {
            Some(PrintInfo::default_single_line())
        }
    }
}

impl Printable for ClassField {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|p| self.try_print_single_line(&shape, p))
            .unwrap_or_else(|| self.print_multi_line(shape, printer))
    }
    fn leftmost_token(&self) -> TextRange {
        self.name.span()
    }
    fn rightmost_token(&self) -> TextRange {
        if let Some(attr) = self.attributes.last() {
            return attr.rightmost_token();
        }
        self.ty.rightmost_token()
    }
}

impl Printable for ImplementsTarget {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        self.ty.print(shape, printer)
    }

    fn leftmost_token(&self) -> TextRange {
        self.ty.leftmost_token()
    }

    fn rightmost_token(&self) -> TextRange {
        self.ty.rightmost_token()
    }
}

impl Printable for AssociatedTypeDecl {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let mut multi_lined = false;
        printer.print_raw_token(&self.keyword);
        printer.print_str(" ");
        printer.print_raw_token(&self.name);
        if let Some((extends, ty)) = &self.bound {
            let (_, extends_trailing) = printer.trivia.get_for_range_split(extends.span());
            printer.print_str(" extends ");
            printer.print_trivia_squished(extends_trailing);
            let leading = printer.trivia.get_leading_for_element(ty);
            printer.print_trivia_squished(leading);
            multi_lined |= ty.print(shape.clone(), printer).multi_lined;
        }
        if let Some((equals, ty)) = &self.default {
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
        self.keyword.span()
    }

    fn rightmost_token(&self) -> TextRange {
        self.default
            .as_ref()
            .map(|(_, ty)| ty.rightmost_token())
            .or_else(|| self.bound.as_ref().map(|(_, ty)| ty.rightmost_token()))
            .unwrap_or_else(|| self.name.span())
    }
}

impl Printable for InterfaceFieldLink {
    fn print(&self, _shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.interface_field);
        printer.print_str(" ");
        printer.print_raw_token(&self.as_token);
        printer.print_str(" ");
        printer.print_raw_token(&self.class_field);
        PrintInfo::default_single_line()
    }

    fn leftmost_token(&self) -> TextRange {
        self.interface_field.span()
    }

    fn rightmost_token(&self) -> TextRange {
        self.class_field.span()
    }
}

impl Printable for ImplementsItem {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            ImplementsItem::AssociatedType(decl, _) => decl.print(shape, printer),
            ImplementsItem::FieldLink(link, _) => link.print(shape, printer),
            ImplementsItem::Field(field, delimiter) => {
                let info = field.print(shape, printer);
                match delimiter {
                    Some(ClassFieldDelimiter::Comma(comma)) => printer.print_raw_token(comma),
                    Some(ClassFieldDelimiter::Semicolon(_)) | None => {}
                }
                info
            }
            ImplementsItem::Function(function) => function.print(shape, printer),
        }
    }

    fn leftmost_token(&self) -> TextRange {
        match self {
            ImplementsItem::AssociatedType(decl, _) => decl.leftmost_token(),
            ImplementsItem::FieldLink(link, _) => link.leftmost_token(),
            ImplementsItem::Field(field, _) => field.leftmost_token(),
            ImplementsItem::Function(function) => function.leftmost_token(),
        }
    }

    fn rightmost_token(&self) -> TextRange {
        match self {
            ImplementsItem::AssociatedType(decl, delimiter) => {
                Self::delimiter_rightmost(delimiter.as_ref(), || decl.rightmost_token())
            }
            ImplementsItem::FieldLink(link, delimiter) => {
                Self::delimiter_rightmost(delimiter.as_ref(), || link.rightmost_token())
            }
            ImplementsItem::Field(field, delimiter) => {
                Self::delimiter_rightmost(delimiter.as_ref(), || field.rightmost_token())
            }
            ImplementsItem::Function(function) => function.rightmost_token(),
        }
    }
}

impl Printable for ImplementsBlock {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_str("implements");
        let (_, keyword_trailing) = printer.trivia.get_for_range_split(self.keyword_span);
        let trivia_len = printer.print_trivia_squished(keyword_trailing);
        if trivia_len == 0 {
            printer.print_str(" ");
        }
        let target_leading = printer.trivia.get_leading_for_element(&self.target);
        printer.print_trivia_squished(target_leading);
        printer.print(&self.target, shape.clone());

        if self.items.is_empty() {
            printer.print_str(" ");
            printer.print_raw_token(&self.open_brace);
            printer.print_raw_token(&self.close_brace);
            return PrintInfo::default_single_line();
        }

        let inner_indent = shape.indent + printer.config.indent_width;
        printer.print_str(" ");
        printer.print_raw_token(&self.open_brace);
        printer.print_trivia_all_trailing_for(self.open_brace.span());
        printer.print_newline();

        if let Some((first, rest)) = self.items.split_first() {
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

        let (close_brace_leading, _) = printer.trivia.get_for_range_split(self.close_brace.span());
        printer.print_trivia_with_newline(close_brace_leading.trim_trailing_blanks(), inner_indent);
        printer.print_spaces(shape.indent);
        printer.print_raw_token(&self.close_brace);
        PrintInfo::default_multi_lined()
    }

    fn leftmost_token(&self) -> TextRange {
        self.keyword_span
    }

    fn rightmost_token(&self) -> TextRange {
        self.close_brace.span()
    }
}

impl Printable for ClassItem {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            ClassItem::Field(field, delimiter) => {
                let info = field.print(shape, printer);
                // Always print comma, but preserve trivia from original delimiter
                match delimiter {
                    Some(ClassFieldDelimiter::Comma(comma)) => {
                        printer.print_raw_token(comma);
                    }
                    Some(ClassFieldDelimiter::Semicolon(_)) => {
                        // Normalize to comma; parent handles trailing trivia via rightmost_token()
                        printer.print_str(",");
                    }
                    None => {
                        printer.print_str(",");
                    }
                }
                info
            }
            ClassItem::Function(function) => function.print(shape, printer),
            ClassItem::Implements(block) => block.print(shape, printer),
            ClassItem::BlockAttribute(attr) => attr.print(shape, printer),
            ClassItem::Unknown(range) => {
                printer.print_input_range(*range);
                PrintInfo::default_multi_lined()
            }
        }
    }
    fn leftmost_token(&self) -> TextRange {
        match self {
            ClassItem::Field(field, _) => field.leftmost_token(),
            ClassItem::Function(function) => function.leftmost_token(),
            ClassItem::Implements(block) => block.leftmost_token(),
            ClassItem::BlockAttribute(attr) => attr.leftmost_token(),
            ClassItem::Unknown(range) => *range,
        }
    }
    fn rightmost_token(&self) -> TextRange {
        match self {
            ClassItem::Field(field, delimiter) => match delimiter {
                Some(ClassFieldDelimiter::Comma(comma)) => comma.span(),
                Some(ClassFieldDelimiter::Semicolon(semi)) => semi.span(),
                None => field.rightmost_token(),
            },
            ClassItem::Function(function) => function.rightmost_token(),
            ClassItem::Implements(block) => block.rightmost_token(),
            ClassItem::BlockAttribute(attr) => attr.rightmost_token(),
            ClassItem::Unknown(range) => *range,
        }
    }
}

impl Printable for EnumDecl {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let inner_indent = shape.indent + printer.config.indent_width;

        printer.print_raw_token(&self.keyword);
        printer.print_str(" ");
        printer.print_raw_token(&self.name);
        printer.print_str(" ");
        printer.print_raw_token(&self.open_brace);
        printer.print_trivia_all_trailing_for(self.open_brace.span());
        printer.print_newline();

        if let Some((first, rest)) = self.items.split_first() {
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

        let (close_brace_leading, _) = printer.trivia.get_for_range_split(self.close_brace.span());
        printer.print_trivia_with_newline(close_brace_leading.trim_trailing_blanks(), inner_indent);
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

impl Printable for EnumItem {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            EnumItem::Variant(variant, delimiter) => {
                let info = variant.print(shape, printer);
                if let Some(delimiter) = delimiter {
                    let (leading, _) = printer.trivia.get_for_range_split(delimiter.span());
                    printer.print_trivia_squished(leading);
                }
                printer.print_str(",");
                info
            }
            EnumItem::BlockAttribute(attr) => attr.print(shape, printer),
        }
    }
    fn leftmost_token(&self) -> TextRange {
        match self {
            EnumItem::Variant(variant, _) => variant.leftmost_token(),
            EnumItem::BlockAttribute(attr) => attr.leftmost_token(),
        }
    }
    fn rightmost_token(&self) -> TextRange {
        match self {
            EnumItem::Variant(variant, delimiter) => {
                if let Some(delimiter) = delimiter {
                    delimiter.span()
                } else {
                    variant.rightmost_token()
                }
            }
            EnumItem::BlockAttribute(attr) => attr.rightmost_token(),
        }
    }
}

impl PrintMultiLine for EnumVariant {
    /// Multi-line layout: attributes wrap to their own indented lines
    /// below the variant name. Trailing comments on the name are preserved.
    ///
    /// ```baml
    /// VariantName // description
    ///     @alias("something_long")
    ///     @description("a long description")
    /// ```
    fn print_multi_line(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.name);

        if self.attributes.is_empty() {
            // you shouldn't call print_multi_line if this is the case.
            return PrintInfo::default_single_line();
        }
        printer.print_trivia_all_trailing_for(self.name.span());

        let attr_shape = Shape::standalone(
            printer.config.line_width,
            shape.indent + printer.config.indent_width,
        );
        for (i, attr) in self.attributes.iter().enumerate() {
            let (attr_leading, attr_trailing) = printer.trivia.get_for_element(attr);
            printer.print_newline();
            printer.print_trivia_with_newline(attr_leading.trim_blanks(), attr_shape.indent);
            printer.print_spaces(attr_shape.indent);
            printer.print(attr, attr_shape.clone());
            if i + 1 < self.attributes.len() {
                printer.print_trivia_trailing(attr_trailing);
            }
        }

        PrintInfo::default_multi_lined()
    }
}

impl EnumVariantLayout for EnumVariant {
    /// Should be passed a sub-printer to avoid printing trivia in the outer printer
    /// in the event that the printer is unable to fit the enum variant on a single line.
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        printer.print_raw_token(&self.name);
        let (_, name_trailing) = printer.trivia.get_for_range_split(self.name.span());
        printer.try_print_trivia_single_line_squished(name_trailing)?;

        for (i, attr) in self.attributes.iter().enumerate() {
            printer.print_spaces(1);
            let (attr_leading, attr_trailing) = printer.trivia.get_for_element(attr);
            printer.try_print_trivia_single_line_squished(attr_leading)?;
            if attr
                .print(Shape::unlimited_single_line(), printer)
                .multi_lined
            {
                return None;
            }
            if i + 1 < self.attributes.len() {
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

impl Printable for EnumVariant {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|p| self.try_print_single_line(&shape, p))
            .unwrap_or_else(|| self.print_multi_line(shape, printer))
    }
    fn leftmost_token(&self) -> TextRange {
        self.name.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.attributes
            .last()
            .map_or(self.name.span(), Printable::rightmost_token)
    }
}

impl Printable for ClientDecl {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.keyword);
        if let Some(client_type) = &self.client_type {
            printer.print(client_type, Shape::unlimited_single_line());
        }
        printer.print_str(" ");
        printer.print_raw_token(&self.name);
        printer.print_str(" ");
        printer.print(&self.config_block, shape)
    }
    fn leftmost_token(&self) -> TextRange {
        self.keyword.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.config_block.rightmost_token()
    }
}

impl Printable for ClientType {
    fn print(&self, _shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.langle);
        printer.print_raw_token(&self.generic);
        printer.print_raw_token(&self.rangle);
        PrintInfo::default_single_line()
    }
    fn leftmost_token(&self) -> TextRange {
        self.langle.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.rangle.span()
    }
}

impl Printable for ConfigBlock {
    /// [`ConfigBlock`] prints multi-line unless empty.
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let inner_indent = shape.indent + printer.config.indent_width;

        if self.items.is_empty() {
            // Check if there's trivia inside the empty block (e.g. comments between { and })
            let (_, open_trailing) = printer.trivia.get_for_range_split(self.open_brace.span());
            let (close_leading, _) = printer.trivia.get_for_range_split(self.close_brace.span());
            let has_comments = open_trailing
                .iter()
                .chain(close_leading.iter())
                .any(EmittableTrivia::is_comment);

            if has_comments {
                printer.print_raw_token(&self.open_brace);
                printer.print_trivia_trailing(open_trailing);
                printer.print_newline();
                printer.print_trivia_with_newline(close_leading.trim_blanks(), inner_indent);
                printer.print_spaces(shape.indent);
                printer.print_raw_token(&self.close_brace);
                return PrintInfo::default_multi_lined();
            }
            printer.print_raw_token(&self.open_brace);
            printer.print_raw_token(&self.close_brace);
            return PrintInfo::default_single_line();
        }

        let inner_shape = Shape::standalone(printer.config.line_width, inner_indent);

        printer.print_raw_token(&self.open_brace);
        printer.print_trivia_all_trailing_for(self.open_brace.span());
        printer.print_newline();

        let mut block_attrs: Vec<(&BlockAttribute, &ConfigBlockMember, Option<&t::Comma>)> = self
            .items
            .iter()
            .filter_map(|(item, comma)| match item {
                ConfigBlockMember::BlockAttribute(attr) => Some((attr, item, comma.as_ref())),
                ConfigBlockMember::Item(_) => None,
            })
            .collect();
        block_attrs.sort_by_cached_key(|(attr, _, _)| {
            attr.name_parts_str(printer.input).collect::<Vec<&str>>()
        });
        let other_items = self
            .items
            .iter()
            .filter(|(item, _)| !matches!(item, ConfigBlockMember::BlockAttribute(_)))
            .map(|(item, comma)| (item, comma.as_ref()));

        let ordered_items = block_attrs
            .into_iter()
            .map(|(_, member, comma)| (member, comma))
            .chain(other_items);

        for (i, (item, comma)) in ordered_items.enumerate() {
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
                (ConfigBlockMember::BlockAttribute(_), Some(comma)) => {
                    // remove the trailing comma, keep the comments
                    let (comma_leading, comma_trailing) =
                        printer.trivia.get_for_range_split(comma.span());
                    printer.print_trivia_trailing(item_trailing);
                    printer.print_trivia_trailing(comma_leading);
                    printer.print_trivia_trailing(comma_trailing);
                }
                (ConfigBlockMember::BlockAttribute(_), None) => {
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

        let (close_brace_leading, _) = printer.trivia.get_for_range_split(self.close_brace.span());
        printer.print_trivia_with_newline(close_brace_leading.trim_trailing_blanks(), inner_indent);
        printer.print_spaces(shape.indent);
        printer.print_raw_token(&self.close_brace);

        PrintInfo::default_multi_lined()
    }
    fn leftmost_token(&self) -> TextRange {
        self.open_brace.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.close_brace.span()
    }
}

impl Printable for ConfigBlockMember {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            ConfigBlockMember::Item(item) => item.print(shape, printer),
            ConfigBlockMember::BlockAttribute(attr) => attr.print(shape, printer),
        }
    }
    fn leftmost_token(&self) -> TextRange {
        match self {
            ConfigBlockMember::Item(item) => item.leftmost_token(),
            ConfigBlockMember::BlockAttribute(attr) => attr.leftmost_token(),
        }
    }
    fn rightmost_token(&self) -> TextRange {
        match self {
            ConfigBlockMember::Item(item) => item.rightmost_token(),
            ConfigBlockMember::BlockAttribute(attr) => attr.rightmost_token(),
        }
    }
}

impl Printable for ConfigItem {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let mut multi_lined = false;
        multi_lined |= printer.print(&self.key, shape.clone()).multi_lined;
        let colon_trailing = if let Some(colon) = &self.colon {
            let (_, colon_trailing) = printer.trivia.get_for_range_split(colon.span());
            colon_trailing
        } else {
            &[][..]
        };
        printer.print_str(": ");
        printer.print_trivia_squished(colon_trailing);
        let value_leading = printer.trivia.get_leading_for_element(&self.value);
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
        multi_lined |= printer.print(&self.value, value_shape).multi_lined;
        PrintInfo { multi_lined }
    }
    fn leftmost_token(&self) -> TextRange {
        self.key.leftmost_token()
    }
    fn rightmost_token(&self) -> TextRange {
        self.value.rightmost_token()
    }
}

impl Printable for ConfigItemKey {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            ConfigItemKey::Word(word) => {
                printer.print_raw_token(word);
                PrintInfo::default_single_line()
            }
            ConfigItemKey::String(string) => printer.print(string, shape),
            ConfigItemKey::RetryPolicy(retry_policy) => {
                printer.print_raw_token(retry_policy);
                PrintInfo::default_single_line()
            }
            ConfigItemKey::Enum(enum_) => {
                printer.print_raw_token(enum_);
                PrintInfo::default_single_line()
            }
            ConfigItemKey::Class(class) => {
                printer.print_raw_token(class);
                PrintInfo::default_single_line()
            }
        }
    }
    fn leftmost_token(&self) -> TextRange {
        match self {
            ConfigItemKey::Word(word) => word.span(),
            ConfigItemKey::String(string) => string.leftmost_token(),
            ConfigItemKey::RetryPolicy(retry_policy) => retry_policy.span(),
            ConfigItemKey::Enum(enum_) => enum_.span(),
            ConfigItemKey::Class(class) => class.span(),
        }
    }
    fn rightmost_token(&self) -> TextRange {
        match self {
            ConfigItemKey::Word(word) => word.span(),
            ConfigItemKey::String(string) => string.rightmost_token(),
            ConfigItemKey::RetryPolicy(retry_policy) => retry_policy.span(),
            ConfigItemKey::Enum(enum_) => enum_.span(),
            ConfigItemKey::Class(class) => class.span(),
        }
    }
}

impl Printable for ConfigItemValue {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            ConfigItemValue::Value(expr) => expr.print(shape, printer),
            ConfigItemValue::ConfigBlock(block) => block.print(shape, printer),
            ConfigItemValue::ConfigArray(array) => array.print(shape, printer),
        }
    }
    fn leftmost_token(&self) -> TextRange {
        match self {
            ConfigItemValue::Value(expr) => expr.leftmost_token(),
            ConfigItemValue::ConfigBlock(block) => block.leftmost_token(),
            ConfigItemValue::ConfigArray(array) => array.leftmost_token(),
        }
    }
    fn rightmost_token(&self) -> TextRange {
        match self {
            ConfigItemValue::Value(expr) => expr.rightmost_token(),
            ConfigItemValue::ConfigBlock(block) => block.rightmost_token(),
            ConfigItemValue::ConfigArray(array) => array.rightmost_token(),
        }
    }
}

impl PrintMultiLine for ConfigArray {
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

        printer.print_raw_token(&self.open_bracket);
        printer.print_trivia_all_trailing_for(self.open_bracket.span());
        printer.print_newline();

        for (elem, comma) in &self.elements {
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

        printer.print_trivia_all_leading_with_newline_for(
            self.close_bracket.span(),
            inner_shape.indent,
        );
        printer.print_spaces(shape.indent);
        printer.print_raw_token(&self.close_bracket);
        PrintInfo::default_multi_lined()
    }
}

impl ConfigArrayLayout for ConfigArray {
    /// Should be passed a sub-printer to avoid printing trivia in the outer printer
    /// in the event that the printer is unable to fit the config array on a single line.
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
            printer.try_print_trivia_single_line_squished(el_trailing)?;
            if i + 1 < self.elements.len() {
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

impl Printable for ConfigArray {
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

impl Printable for TestDecl {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.keyword);
        printer.print_str(" ");
        printer.print_raw_token(&self.name);
        printer.print_str(" ");
        printer.print(&self.config_block, shape)
    }
    fn leftmost_token(&self) -> TextRange {
        self.keyword.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.config_block.rightmost_token()
    }
}

impl Printable for TestExprDecl {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.keyword);
        printer.print_str(" ");
        printer.print(&self.name, shape.clone());
        if let Some(wc) = &self.with_clause {
            printer.print_str(" ");
            printer.print_raw_token(&wc.keyword);
            printer.print_str(" ");
            printer.print(&wc.expr, shape.clone());
        }
        printer.print_str(" ");
        printer.print(&self.body, shape)
    }
    fn leftmost_token(&self) -> TextRange {
        self.keyword.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.body.rightmost_token()
    }
}

impl Printable for TestSetDecl {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.keyword);
        printer.print_str(" ");
        printer.print(&self.name, shape.clone());
        if let Some(wc) = &self.with_clause {
            printer.print_str(" ");
            printer.print_raw_token(&wc.keyword);
            printer.print_str(" ");
            printer.print(&wc.expr, shape.clone());
        }
        printer.print_str(" ");
        printer.print(&self.body, shape)
    }
    fn leftmost_token(&self) -> TextRange {
        self.keyword.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.body.rightmost_token()
    }
}

impl Printable for RetryPolicyDecl {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.keyword);
        printer.print_str(" ");
        printer.print_raw_token(&self.name);
        printer.print_str(" ");
        printer.print(&self.config_block, shape)
    }
    fn leftmost_token(&self) -> TextRange {
        self.keyword.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.config_block.rightmost_token()
    }
}

impl Printable for TemplateStringDecl {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let mut multi_lined = false;

        printer.print_raw_token(&self.keyword);
        printer.print_str(" ");
        printer.print_raw_token(&self.name);
        multi_lined |= printer.print(&self.args, shape).multi_lined;
        printer.print_str(" ");
        multi_lined |= printer
            .print(&self.body, Shape::unlimited_single_line())
            .multi_lined;
        PrintInfo { multi_lined }
    }
    fn leftmost_token(&self) -> TextRange {
        self.keyword.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.body.rightmost_token()
    }
}

impl Printable for TypeAliasDecl {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.keyword);
        printer.print_str(" ");
        printer.print_raw_token(&self.name);
        printer.print_str(" ");
        printer.print_raw_token(&self.equals);
        printer.print_str(" ");
        let (_, eq_trailing) = printer.trivia.get_for_range_split(self.equals.span());
        let (ty_leading, ty_trailing) = printer.trivia.get_for_element(&self.type_expr);
        let mut ty_leading_len = printer.print_trivia_squished(eq_trailing);
        ty_leading_len += printer.print_trivia_squished(ty_leading);
        let new_offset = usize::from(self.keyword.span().len() + self.name.span().len())
            + const { "  = ".len() }
            + ty_leading_len;

        let info;
        if let Some(semicolon) = &self.semicolon {
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
            info = printer.print(&self.type_expr, ty_shape);
            printer.print_trivia_squished(ty_trailing);
            printer.print_trivia_squished(semicolon_leading);
            printer.print_raw_token(semicolon);
        } else {
            let ty_shape = Shape {
                width: shape.width.saturating_sub(new_offset + const { ";".len() }),
                indent: shape.indent,
                first_line_offset: shape.first_line_offset + new_offset,
            };
            info = printer.print(&self.type_expr, ty_shape);
            // this is the last child so trivia is handled by parent
            printer.print_str(";");
        }

        info
    }
    fn leftmost_token(&self) -> TextRange {
        self.keyword.span()
    }
    fn rightmost_token(&self) -> TextRange {
        if let Some(semicolon) = &self.semicolon {
            semicolon.span()
        } else {
            self.type_expr.rightmost_token()
        }
    }
}

impl Printable for GeneratorDecl {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.keyword);
        printer.print_str(" ");
        printer.print_raw_token(&self.name);
        printer.print_str(" ");
        printer.print(&self.config, shape)
    }
    fn leftmost_token(&self) -> TextRange {
        self.keyword.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.config.rightmost_token()
    }
}
