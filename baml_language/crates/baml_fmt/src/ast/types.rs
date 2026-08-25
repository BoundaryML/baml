//! Reference: [`baml_db::baml_compiler_syntax::type_ref`], though many of the types are grouped into [`Type::Path`] for us,
//! since we shouldn't need special treatment for things like `string` and `int` during formatting.
//! If this ever gets used for something else, we can split it up into multiple types.

use baml_db::baml_compiler_syntax::validated::nodes::{
    ArrayType, AssociatedProjectionType, AssociatedTypeArgBinding, ConstrainedType, FunctionType,
    FunctionTypeParam, GenericType, OptionalType, ParenType, PathType, SignedLiteralType,
    StringType, Type, TypeArg, TypeArgs, UnionType, UnionTypeMember,
};
use rowan::{TextRange, TextSize};

use crate::{
    ast::Token,
    printer::{PrintInfo, PrintMultiLine, Printable, Printer, Shape},
    trivia_classifier::TriviaSliceExt,
};

trait TryPrintSingleLine {
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo>;
}

impl Printable for Type {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            Type::Paren(paren) => paren.print(shape, printer),
            Type::Path(path) => path.print(shape, printer),
            Type::Literal(literal) => literal.print(shape, printer),
            Type::SignedLiteral(literal) => literal.print(shape, printer),
            Type::Union(union) => union.print(shape, printer),
            Type::Optional(optional) => optional.print(shape, printer),
            Type::Array(array) => array.print(shape, printer),
            Type::Generic(generic) => generic.print(shape, printer),
            Type::AssociatedProjection(projection) => projection.print(shape, printer),
            Type::Function(function) => function.print(shape, printer),
            Type::Constrained(constrained) => constrained.print(shape, printer),
            Type::Unknown(range) => {
                printer.print_input_range(*range);
                PrintInfo {
                    multi_lined: printer.input[*range].contains('\n'),
                }
            }
        }
    }
    fn leftmost_token(&self) -> TextRange {
        match self {
            Type::Paren(paren) => paren.leftmost_token(),
            Type::Path(path) => path.leftmost_token(),
            Type::Literal(literal) => literal.leftmost_token(),
            Type::SignedLiteral(literal) => literal.leftmost_token(),
            Type::Union(union) => union.leftmost_token(),
            Type::Optional(optional) => optional.leftmost_token(),
            Type::Array(array) => array.leftmost_token(),
            Type::Generic(generic) => generic.leftmost_token(),
            Type::AssociatedProjection(projection) => projection.leftmost_token(),
            Type::Function(function) => function.leftmost_token(),
            Type::Constrained(constrained) => constrained.leftmost_token(),
            Type::Unknown(range) => *range,
        }
    }
    fn rightmost_token(&self) -> TextRange {
        match self {
            Type::Paren(paren) => paren.rightmost_token(),
            Type::Path(path) => path.rightmost_token(),
            Type::Literal(literal) => literal.rightmost_token(),
            Type::SignedLiteral(literal) => literal.rightmost_token(),
            Type::Union(union) => union.rightmost_token(),
            Type::Optional(optional) => optional.rightmost_token(),
            Type::Array(array) => array.rightmost_token(),
            Type::Generic(generic) => generic.rightmost_token(),
            Type::AssociatedProjection(projection) => projection.rightmost_token(),
            Type::Function(function) => function.rightmost_token(),
            Type::Constrained(constrained) => constrained.rightmost_token(),
            Type::Unknown(range) => *range,
        }
    }
}

impl PrintMultiLine for ParenType {
    /// Multi-line layout: inner type wraps to an indented new line,
    /// closing paren aligns with the opening context. Trivia is preserved.
    ///
    /// ```baml
    /// (
    ///     SomeLongInnerType
    /// )
    /// ```
    fn print_multi_line(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let inner_indent = shape.indent + printer.config.indent_width;
        printer.print_raw_token(&self.open_paren);
        printer.print_trivia_all_trailing_for(self.open_paren.span());
        printer.print_newline();

        printer.print_standalone_with_trivia(&*self.ty, inner_indent);

        printer.print_newline();

        let (close_paren_leading, _) = printer.trivia.get_for_range_split(self.close_paren.span());
        printer.print_trivia_with_newline(close_paren_leading.trim_blanks(), inner_indent);

        printer.print_spaces(shape.indent);
        printer.print_raw_token(&self.close_paren);
        PrintInfo::default_multi_lined()
    }
}

impl TryPrintSingleLine for ParenType {
    /// Should be passed a sub-printer to avoid printing trivia in the outer printer
    /// in the event that the printer is unable to fit the parenthesized type on a single line.
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        printer.print_raw_token(&self.open_paren);
        let (_, open_trailing) = printer.trivia.get_for_range_split(self.open_paren.span());
        printer.try_print_trivia_single_line_squished(open_trailing)?;

        let (ty_leading, ty_trailing) = printer.trivia.get_for_element(&*self.ty);
        printer.try_print_trivia_single_line_squished(ty_leading)?;
        if printer
            .print(&*self.ty, Shape::unlimited_single_line())
            .multi_lined
        {
            return None;
        }
        printer.try_print_trivia_single_line_squished(ty_trailing)?;

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

impl Printable for ParenType {
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

impl Printable for PathType {
    fn print(&self, _shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.first);
        for (dot, word) in &self.rest {
            printer.print_raw_token(dot);
            printer.print_raw_token(word);
        }
        PrintInfo::default_single_line()
    }
    fn leftmost_token(&self) -> TextRange {
        self.first.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.rest
            .last()
            .map_or(&self.first, |(_, word)| word)
            .span()
    }
}

impl Printable for SignedLiteralType {
    fn print(&self, _shape: Shape, printer: &mut Printer) -> PrintInfo {
        let range = TextRange::new(self.minus.span().start(), self.literal.end());
        printer.print_input_range(range);
        PrintInfo {
            multi_lined: printer.input[range].contains('\n'),
        }
    }
    fn leftmost_token(&self) -> TextRange {
        self.minus.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.literal
    }
}

impl Printable for StringType {
    fn print(&self, _shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.0);
        PrintInfo::default_single_line()
    }
    fn leftmost_token(&self) -> TextRange {
        self.0.leftmost_token()
    }
    fn rightmost_token(&self) -> TextRange {
        self.0.rightmost_token()
    }
}

impl PrintMultiLine for UnionType {
    fn print_multi_line(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let mut info = printer.print(&*self.first, shape.clone());
        printer.print_trivia_all_trailing_for(self.first.rightmost_token());
        let inner_indent = shape.indent + printer.config.indent_width;
        let inner_shape = Shape {
            width: printer.config.line_width.saturating_sub(inner_indent),
            indent: inner_indent,
            first_line_offset: 0,
        };
        for (i, (pipe, ty)) in self.rest.iter().enumerate() {
            info.multi_lined = true;
            let (pipe_leading, pipe_trailing) = printer.trivia.get_for_range_split(pipe.span());
            let (ty_leading, ty_trailing) = printer.trivia.get_for_element(ty);

            printer.print_newline();
            printer.print_trivia_with_newline(pipe_leading.trim_blanks(), inner_shape.indent);

            printer.print_spaces(inner_indent);
            printer.print_raw_token(pipe);

            let mut post_pipe_len = printer.print_trivia_squished(pipe_trailing);
            post_pipe_len += printer.print_trivia_squished(ty_leading);
            if post_pipe_len == 0 {
                printer.print_spaces(1); // only add space if there are no block comments between
                post_pipe_len = 1;
            }
            let offset = const { "| ".len() } + post_pipe_len;
            let ty_shape = Shape {
                width: printer
                    .config
                    .line_width
                    .saturating_sub(inner_indent + offset),
                indent: inner_indent,
                first_line_offset: offset,
            };
            printer.print(ty, ty_shape);
            if i + 1 < self.rest.len() {
                printer.print_trivia_trailing(ty_trailing);
            }
        }
        info
    }
}

impl TryPrintSingleLine for UnionType {
    /// Should be passed a sub-printer to avoid printing trivia in the outer printer
    /// in the event that the printer is unable to fit the union type on a single line.
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        if printer
            .print(&*self.first, Shape::unlimited_single_line())
            .multi_lined
        {
            return None;
        }
        let first_trailing = printer.trivia.get_trailing_for_element(&*self.first);
        let mut pre_pipe_len = printer.try_print_trivia_single_line_squished(first_trailing)?;

        for (i, (pipe, ty)) in self.rest.iter().enumerate() {
            if printer.output.len() > shape.width {
                return None;
            }
            let (pipe_leading, pipe_trailing) = printer.trivia.get_for_range_split(pipe.span());
            let (ty_leading, ty_trailing) = printer.trivia.get_for_element(ty);
            pre_pipe_len += printer.print_trivia_squished(pipe_leading);
            if pre_pipe_len == 0 {
                printer.print_spaces(1); // only add space if there are no block comments between
            }

            printer.print_raw_token(pipe);

            let mut post_pipe_len = printer.print_trivia_squished(pipe_trailing);
            post_pipe_len += printer.print_trivia_squished(ty_leading);
            if post_pipe_len == 0 {
                printer.print_spaces(1); // only add space if there are no block comments between
            }

            if printer
                .print(ty, Shape::unlimited_single_line())
                .multi_lined
            {
                return None;
            }
            if i + 1 < self.rest.len() {
                pre_pipe_len = printer.try_print_trivia_single_line_squished(ty_trailing)?;
            }
        }

        if printer.output.len() > shape.width {
            None
        } else {
            Some(PrintInfo::default_single_line())
        }
    }
}

impl Printable for UnionType {
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
            .map_or(&*self.first, |(_, ty)| ty)
            .rightmost_token()
    }
}

impl Printable for UnionTypeMember {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            UnionTypeMember::Paren(paren) => paren.print(shape, printer),
            UnionTypeMember::Path(path) => path.print(shape, printer),
            UnionTypeMember::Literal(literal) => literal.print(shape, printer),
            UnionTypeMember::SignedLiteral(literal) => literal.print(shape, printer),
            UnionTypeMember::Optional(optional) => optional.print(shape, printer),
            UnionTypeMember::Array(array) => array.print(shape, printer),
            UnionTypeMember::Generic(generic) => generic.print(shape, printer),
            UnionTypeMember::AssociatedProjection(projection) => projection.print(shape, printer),
            UnionTypeMember::Function(function) => function.print(shape, printer),
            UnionTypeMember::Constrained(constrained) => constrained.print(shape, printer),
            UnionTypeMember::Unknown(range) => {
                printer.print_input_range(*range);
                PrintInfo { multi_lined: false }
            }
        }
    }
    fn leftmost_token(&self) -> TextRange {
        match self {
            UnionTypeMember::Paren(paren) => paren.leftmost_token(),
            UnionTypeMember::Path(path) => path.leftmost_token(),
            UnionTypeMember::Literal(lit) => lit.leftmost_token(),
            UnionTypeMember::SignedLiteral(lit) => lit.leftmost_token(),
            UnionTypeMember::Optional(optional) => optional.leftmost_token(),
            UnionTypeMember::Array(array) => array.leftmost_token(),
            UnionTypeMember::Generic(generic) => generic.leftmost_token(),
            UnionTypeMember::AssociatedProjection(projection) => projection.leftmost_token(),
            UnionTypeMember::Function(function) => function.leftmost_token(),
            UnionTypeMember::Constrained(constrained) => constrained.leftmost_token(),
            UnionTypeMember::Unknown(range) => *range,
        }
    }
    fn rightmost_token(&self) -> TextRange {
        match self {
            UnionTypeMember::Paren(paren) => paren.rightmost_token(),
            UnionTypeMember::Path(path) => path.rightmost_token(),
            UnionTypeMember::Literal(lit) => lit.rightmost_token(),
            UnionTypeMember::SignedLiteral(lit) => lit.rightmost_token(),
            UnionTypeMember::Optional(optional) => optional.rightmost_token(),
            UnionTypeMember::Array(array) => array.rightmost_token(),
            UnionTypeMember::Generic(generic) => generic.rightmost_token(),
            UnionTypeMember::AssociatedProjection(projection) => projection.rightmost_token(),
            UnionTypeMember::Function(function) => function.rightmost_token(),
            UnionTypeMember::Constrained(constrained) => constrained.rightmost_token(),
            UnionTypeMember::Unknown(range) => *range,
        }
    }
}

impl Printable for OptionalType {
    fn print(&self, mut shape: Shape, printer: &mut Printer) -> PrintInfo {
        shape.width = shape
            .width
            .saturating_sub(usize::from(self.question.span().len()));
        let info = printer.print(&*self.ty, shape);
        printer.print_raw_token(&self.question);
        info
    }
    fn leftmost_token(&self) -> TextRange {
        self.ty.leftmost_token()
    }
    fn rightmost_token(&self) -> TextRange {
        self.question.span()
    }
}

impl Printable for ArrayType {
    fn print(&self, mut shape: Shape, printer: &mut Printer) -> PrintInfo {
        let brackets_width: TextSize = self
            .brackets
            .iter()
            .map(|(l, r)| l.span().len() + r.span().len())
            .sum();
        shape.width = shape.width.saturating_sub(usize::from(brackets_width));
        let info = printer.print(&*self.ty, shape);
        for (open, close) in &self.brackets {
            printer.print_raw_token(open);
            printer.print_raw_token(close);
        }
        info
    }
    fn leftmost_token(&self) -> TextRange {
        self.ty.leftmost_token()
    }
    fn rightmost_token(&self) -> TextRange {
        self.brackets
            .last()
            .map_or(self.ty.rightmost_token(), |(_, close)| close.span())
    }
}

impl Printable for GenericType {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let mut multi_lined = false;
        multi_lined |= printer.print(&*self.base, shape.clone()).multi_lined;
        multi_lined |= printer.print(&self.args, shape).multi_lined;
        PrintInfo { multi_lined }
    }
    fn leftmost_token(&self) -> TextRange {
        self.base.leftmost_token()
    }
    fn rightmost_token(&self) -> TextRange {
        self.args.rightmost_token()
    }
}

impl Printable for AssociatedProjectionType {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.open_paren);
        printer.print(&*self.base, shape.clone());
        printer.print_str(" ");
        printer.print_raw_token(&self.as_token);
        printer.print_str(" ");
        printer.print(&*self.interface, shape);
        printer.print_raw_token(&self.close_paren);
        printer.print_raw_token(&self.dot);
        printer.print_raw_token(&self.member);
        PrintInfo::default_single_line()
    }

    fn leftmost_token(&self) -> TextRange {
        self.open_paren.span()
    }

    fn rightmost_token(&self) -> TextRange {
        self.member.span()
    }
}

impl Printable for TypeArg {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            TypeArg::Type(ty) => ty.print(shape, printer),
            TypeArg::Associated(binding) => binding.print(shape, printer),
        }
    }

    fn leftmost_token(&self) -> TextRange {
        match self {
            TypeArg::Type(ty) => ty.leftmost_token(),
            TypeArg::Associated(binding) => binding.leftmost_token(),
        }
    }

    fn rightmost_token(&self) -> TextRange {
        match self {
            TypeArg::Type(ty) => ty.rightmost_token(),
            TypeArg::Associated(binding) => binding.rightmost_token(),
        }
    }
}

impl Printable for AssociatedTypeArgBinding {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.name);
        let (_, equals_trailing) = printer.trivia.get_for_range_split(self.equals.span());
        printer.print_str(" = ");
        printer.print_trivia_squished(equals_trailing);
        let leading = printer.trivia.get_leading_for_element(&self.ty);
        printer.print_trivia_squished(leading);
        self.ty.print(shape, printer)
    }

    fn leftmost_token(&self) -> TextRange {
        self.name.span()
    }

    fn rightmost_token(&self) -> TextRange {
        self.ty.rightmost_token()
    }
}

impl PrintMultiLine for TypeArgs {
    /// Multi-line layout: each type argument on its own indented line
    /// with trailing comma except for the last one. Closing `>` on its own line.
    ///
    /// ```baml
    /// <
    ///     SomeLongType,
    ///     AnotherType
    /// >
    /// ```
    fn print_multi_line(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let inner_indent = shape.indent + printer.config.indent_width;
        let inner_shape = Shape {
            width: printer.config.line_width.saturating_sub(inner_indent),
            indent: inner_indent,
            first_line_offset: 0,
        };

        printer.print_raw_token(&self.open_angle);
        printer.print_trivia_all_trailing_for(self.open_angle.span());
        printer.print_newline();

        // First element
        let (first_leading, first_trailing) = printer.trivia.get_for_element(&*self.first);
        printer.print_trivia_with_newline(first_leading.trim_blanks(), inner_shape.indent);
        printer.print_spaces(inner_shape.indent);
        printer.print(&*self.first, inner_shape.clone());
        if self.rest.is_empty() {
            // This is the only element, so we can have a line comment directly after the type
            printer.print_trivia_trailing(first_trailing);
            printer.print_newline();

            let (close_angle_leading, _) =
                printer.trivia.get_for_range_split(self.close_angle.span());
            printer
                .print_trivia_with_newline(close_angle_leading.trim_blanks(), inner_shape.indent);
            printer.print_spaces(shape.indent);
            printer.print_raw_token(&self.close_angle);
            return PrintInfo::default_multi_lined();
        }

        let _ = printer.try_print_trivia_single_line_squished(first_trailing); // only keep if single-line block comments
        for (i, (comma, ty)) in self.rest.iter().enumerate() {
            let (comma_leading, comma_trailing) = printer.trivia.get_for_range_split(comma.span());
            let _ = printer.try_print_trivia_single_line_squished(comma_leading); // only keep if single-line block comments
            printer.print_raw_token(comma);
            printer.print_trivia_trailing(comma_trailing);
            printer.print_newline();

            let (ty_leading, ty_trailing) = printer.trivia.get_for_element(ty);
            printer.print_trivia_with_newline(ty_leading.trim_blanks(), inner_shape.indent);
            printer.print_spaces(inner_shape.indent);
            printer.print(ty, inner_shape.clone());
            if i + 1 < self.rest.len() {
                // not the last element, will have a comma after these comments:
                let _ = printer.try_print_trivia_single_line_squished(ty_trailing); // only keep if single-line block comments
            } else {
                // last element, we can have a line comment directly after the type
                printer.print_trivia_trailing(ty_trailing);
            }
        }

        printer.print_newline();
        let (close_angle_leading, _) = printer.trivia.get_for_range_split(self.close_angle.span());
        printer.print_trivia_with_newline(close_angle_leading.trim_blanks(), inner_shape.indent);
        printer.print_spaces(shape.indent);
        printer.print_raw_token(&self.close_angle);
        PrintInfo::default_multi_lined()
    }
}

impl TryPrintSingleLine for TypeArgs {
    /// Should be passed a sub-printer to avoid printing trivia in the outer printer
    /// in the event that the printer is unable to fit the type args on a single line.
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        printer.print_raw_token(&self.open_angle);
        let (_, open_trailing) = printer.trivia.get_for_range_split(self.open_angle.span());
        printer.try_print_trivia_single_line_squished(open_trailing)?;

        // First element
        let (first_leading, first_trailing) = printer.trivia.get_for_element(&*self.first);
        printer.try_print_trivia_single_line_squished(first_leading)?;
        if printer
            .print(&*self.first, Shape::unlimited_single_line())
            .multi_lined
        {
            return None;
        }
        printer.try_print_trivia_single_line_squished(first_trailing)?;

        for (comma, ty) in &self.rest {
            let (comma_leading, comma_trailing) = printer.trivia.get_for_range_split(comma.span());
            printer.try_print_trivia_single_line_squished(comma_leading)?;
            printer.print_raw_token(comma);
            printer.try_print_trivia_single_line_squished(comma_trailing)?;
            printer.print_str(" ");
            let (ty_leading, ty_trailing) = printer.trivia.get_for_element(ty);
            printer.try_print_trivia_single_line_squished(ty_leading)?;
            if printer
                .print(ty, Shape::unlimited_single_line())
                .multi_lined
            {
                return None;
            }
            printer.try_print_trivia_single_line_squished(ty_trailing)?;
        }

        let (close_leading, _) = printer.trivia.get_for_range_split(self.close_angle.span());
        printer.try_print_trivia_single_line_squished(close_leading)?;
        printer.print_raw_token(&self.close_angle);

        if printer.output.len() > shape.width {
            None
        } else {
            Some(PrintInfo::default_single_line())
        }
    }
}

impl Printable for TypeArgs {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|p| self.try_print_single_line(&shape, p))
            .unwrap_or_else(|| self.print_multi_line(shape, printer))
    }
    fn leftmost_token(&self) -> TextRange {
        self.open_angle.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.close_angle.span()
    }
}

impl PrintMultiLine for FunctionType {
    /// Multi-line layout: each parameter on its own indented line
    /// with trailing comma. Arrow and return type follow the closing paren.
    ///
    /// ```baml
    /// (
    ///     SomeLongTypeThatForcesMultilining,
    ///     can_have_names: AnotherLongType,
    /// ) -> ReturnType
    /// ```
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

        let (close_paren_leading, _) = printer.trivia.get_for_range_split(self.close_paren.span());
        printer.print_trivia_with_newline(close_paren_leading.trim_blanks(), inner_shape.indent);
        printer.print_spaces(shape.indent);
        printer.print_raw_token(&self.close_paren);
        printer.print_str(" ");
        printer.print_raw_token(&self.arrow);
        printer.print_str(" ");
        printer.print(&*self.return_type, shape.clone());
        if let Some(throws) = &self.throws {
            printer.print_str(" ");
            printer.print(&**throws, shape);
        }
        PrintInfo::default_multi_lined()
    }
}

impl TryPrintSingleLine for FunctionType {
    /// Should be passed a sub-printer to avoid printing trivia in the outer printer
    /// in the event that the printer is unable to fit the function type on a single line.
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
            printer.try_print_trivia_single_line_squished(p_trailing)?;
            if i + 1 < self.params.len() {
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
        printer.print_str(" ");
        printer.print_raw_token(&self.arrow);
        printer.print_str(" ");
        if printer
            .print(&*self.return_type, Shape::unlimited_single_line())
            .multi_lined
        {
            return None;
        }
        if let Some(throws) = &self.throws {
            printer.print_str(" ");
            if printer
                .print(&**throws, Shape::unlimited_single_line())
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

impl Printable for FunctionType {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|p| self.try_print_single_line(&shape, p))
            .unwrap_or_else(|| self.print_multi_line(shape, printer))
    }
    fn leftmost_token(&self) -> TextRange {
        self.open_paren.span()
    }
    fn rightmost_token(&self) -> TextRange {
        self.throws
            .as_ref()
            .map(|throws| throws.rightmost_token())
            .unwrap_or_else(|| self.return_type.rightmost_token())
    }
}

impl Printable for FunctionTypeParam {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        if let Some((name, question, colon)) = &self.name {
            printer.print_raw_token(name);
            if let Some(question) = question {
                printer.print_raw_token(question);
            }
            if let Some(colon) = colon {
                printer.print_raw_token(colon);
            } else {
                printer.print_str(":");
            }
            printer.print_str(" ");
        }
        printer.print(&self.ty, shape)
    }
    fn leftmost_token(&self) -> TextRange {
        self.name
            .as_ref()
            .map_or(self.ty.leftmost_token(), |(name, _, _)| name.span())
    }
    fn rightmost_token(&self) -> TextRange {
        self.ty.rightmost_token()
    }
}

impl<T: Printable> PrintMultiLine for ConstrainedType<T> {
    /// Multi-line layout: each attribute is indented one layer and is on a new line.
    ///
    /// ```baml
    /// map<string, int>
    ///     @stream.done
    /// ```
    fn print_multi_line(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let ty_info = printer.print(&*self.ty, shape.clone());
        let (ty_trailing, _) = printer.print_trivia_all_trailing_for(self.ty.rightmost_token());
        if !ty_info.multi_lined
            && ty_trailing == 0
            && let [attr] = self.attrs.as_slice()
            && let remaining_width = printer.current_line_remaining_width().saturating_sub(1)
            && attr.non_wrappable_len() <= remaining_width
        {
            // only one attribute and type was single line.
            // we can start the attribute on the same line as the type
            // ```baml
            // MyReallyReallyLongTypeButOnOneLine
            // ```
            printer.print_spaces(1);
            let attr_shape = Shape {
                width: remaining_width,
                indent: shape.indent,
                first_line_offset: printer
                    .config
                    .line_width
                    .saturating_sub(shape.indent + remaining_width),
            };
            return printer.print(attr, attr_shape);
        }

        let attr_indent = shape.indent + printer.config.indent_width;
        let attr_shape = Shape {
            width: printer.config.line_width.saturating_sub(attr_indent),
            indent: attr_indent,
            first_line_offset: 0,
        };
        for attr in &self.attrs {
            printer.print_newline();
            printer.print_spaces(attr_indent);
            printer.print(attr, attr_shape.clone());
        }
        PrintInfo::default_multi_lined()
    }
}

impl<T: Printable> TryPrintSingleLine for ConstrainedType<T> {
    /// Should be passed a sub-printer to avoid printing trivia in the outer printer
    /// in the event that the printer is unable to fit the type alias on a single line.
    fn try_print_single_line(&self, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
        if printer
            .print(&*self.ty, Shape::unlimited_single_line())
            .multi_lined
        {
            return None;
        }

        let (_, ty_trailing) = printer.trivia.get_for_element(&*self.ty);
        let mut trivia_len = printer.try_print_trivia_single_line_squished(ty_trailing)?;

        for (i, attr) in self.attrs.iter().enumerate() {
            let (attr_leading, attr_trailing) = printer.trivia.get_for_element(attr);
            trivia_len += printer.try_print_trivia_single_line_squished(attr_leading)?;
            if trivia_len == 0 {
                printer.print_spaces(1);
            }
            if printer
                .print(attr, Shape::unlimited_single_line())
                .multi_lined
            {
                return None;
            }
            let is_last = i + 1 >= self.attrs.len();
            if !is_last {
                trivia_len = printer.try_print_trivia_single_line_squished(attr_trailing)?;
            }
        }

        if printer.len() > shape.width {
            None
        } else {
            Some(PrintInfo::default_single_line())
        }
    }
}

impl<T: Printable> Printable for ConstrainedType<T> {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        debug_assert!(!self.attrs.is_empty());
        printer
            .try_sub_printer(|p| self.try_print_single_line(&shape, p))
            .unwrap_or_else(|| self.print_multi_line(shape, printer))
    }
    fn leftmost_token(&self) -> TextRange {
        self.ty.leftmost_token()
    }
    fn rightmost_token(&self) -> TextRange {
        if let Some(attr) = self.attrs.last() {
            attr.rightmost_token()
        } else {
            self.ty.rightmost_token()
        }
    }
}

#[cfg(test)]
mod tests {
    use baml_db::{
        baml_compiler_parser::parse_green,
        baml_compiler_syntax::{FromCST as _, SyntaxElement, SyntaxKind, SyntaxNode},
    };
    use baml_project::ProjectDatabase;

    use super::*;

    fn function_type_param(source: &str, index: usize) -> FunctionTypeParam {
        let mut db = ProjectDatabase::new();
        let file = db.add_file("test.baml", source);
        let parsed = parse_green(&db, file);
        let syntax_tree = SyntaxNode::new_root(parsed);
        let node = syntax_tree
            .descendants()
            .filter(|node| node.kind() == SyntaxKind::FUNCTION_TYPE_PARAM)
            .nth(index)
            .expect("expected FUNCTION_TYPE_PARAM");

        FunctionTypeParam::from_cst(SyntaxElement::Node(node))
            .expect("expected FunctionTypeParam to parse")
    }

    #[test]
    fn function_type_param_optional_name_round_trips() {
        let source = "type Searcher = (name?: string) -> int\n";
        let param = function_type_param(source, 0);
        let Some((name, question, colon)) = &param.name else {
            panic!("expected named function type param");
        };

        assert!(question.is_some(), "expected optional marker before colon");
        assert!(colon.is_some(), "expected colon after optional marker");
        assert_eq!(param.leftmost_token(), name.span());

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

        assert!(
            param
                .name
                .as_ref()
                .and_then(|(_, q, _)| q.as_ref())
                .is_some()
        );
        assert!(matches!(param.ty, Type::Optional(_)));

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
