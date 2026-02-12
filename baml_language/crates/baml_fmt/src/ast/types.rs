//! Reference: [baml_compiler_syntax::type_ref], though many of the types are grouped into [`Type::Path`] for us,
//! since we shouldn't need special treatment for things like `string` and `int` during formatting.
//! If this ever gets used for something else, we can split it up into multiple types.

use baml_compiler_syntax::{SyntaxElement, SyntaxKind};

use super::{FromCST, KnownKind, StrongAstError, tokens as t};
use crate::{
    ast::{Literal, SyntaxNodeIter},
    printer::*,
};
use rowan::TextRange;

/// Corresponds to a [`SyntaxKind::TYPE_EXPR`] node.
#[derive(Debug)]
pub enum Type {
    Paren(ParenType),
    Path(PathType),
    /// Generally only string literals are used in normal types,
    /// but other literals are valid in some contexts like match bindings.
    Literal(Literal),
    Union(UnionType),
    Optional(OptionalType),
    Array(ArrayType),
    Generic(GenericType),
    Function(FunctionType),
    /// Types constrained by attributes.
    Constrained(TextRange), // TODO
    Unknown(TextRange),
}

impl Type {
    /// Check if, when multi-line printed the last line is indented.
    ///
    /// For example, multi-lined paths and unions are indented,
    /// while generics and parenthesized types are not.
    /// Optional types and array types follow their inner type.
    pub const fn multi_line_is_indented(&self) -> bool {
        match self {
            Type::Paren(_) => false,
            Type::Path(_) => true,
            Type::Literal(_) => false,
            Type::Union(_) => true,
            Type::Optional(inner) => inner.ty.multi_line_is_indented(),
            Type::Array(inner) => inner.ty.multi_line_is_indented(),
            Type::Generic(_) => false,
            Type::Function(_) => true,
            Type::Constrained(_) => true,
            Type::Unknown(_) => true, // to be safe
        }
    }
}

impl FromCST for Type {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::TYPE_EXPR)?;

        // TYPE_EXPR contains tokens and nodes directly in a flat structure
        // We need to parse them into the appropriate Type variant

        let mut it = SyntaxNodeIter::new(node);

        let first = UnionTypeMember::take(&mut it)?;

        let mut rest = Vec::new();
        while let Some(pipe) = it.next_if_kind(SyntaxKind::PIPE) {
            let pipe = StrongAstError::assert_is_token(pipe)?;
            let next = UnionTypeMember::take(&mut it)?;
            rest.push((t::Pipe::new_from_span(pipe.text_range()), next));
        }

        if rest.is_empty() {
            Ok(first.into())
        } else {
            Ok(Type::Union(UnionType {
                first: Box::new(first),
                rest,
            }))
        }
    }
}

impl KnownKind for Type {
    fn kind() -> SyntaxKind {
        SyntaxKind::TYPE_EXPR
    }
}

impl Printable for Type {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            Type::Paren(paren) => paren.print(shape, printer),
            Type::Path(path) => path.print(shape, printer),
            Type::Literal(literal) => literal.print(shape, printer),
            Type::Union(union) => union.print(shape, printer),
            Type::Optional(optional) => optional.print(shape, printer),
            Type::Array(array) => array.print(shape, printer),
            Type::Generic(generic) => generic.print(shape, printer),
            Type::Function(function) => function.print(shape, printer),
            Type::Constrained(range) | Type::Unknown(range) => {
                printer.print_input_range(*range);
                PrintInfo::default_single_line()
            }
        }
    }
}

#[derive(Debug)]
pub struct ParenType {
    pub open_paren: t::LParen,
    /// Will have a [`SyntaxKind::FUNCTION_TYPE_PARAM`] with a [`SyntaxKind::TYPE_EXPR`] inside for some reason
    pub ty: Box<Type>,
    pub close_paren: t::RParen,
}

impl PrintMultiLine for ParenType {
    /// Multi-line layout: inner type wraps to an indented new line,
    /// closing paren aligns with the opening context.
    ///
    /// ```baml
    /// (
    ///     SomeLongInnerType
    /// )
    /// ```
    fn print_multi_line(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let inner_shape = Shape {
            width: shape.width.saturating_sub(printer.config.indent_width),
            indent: shape.indent + printer.config.indent_width,
            first_line_offset: 0,
        };
        printer.print_raw_token(&self.open_paren);
        printer.print_newline();
        printer.print_spaces(inner_shape.indent);
        printer.print(&*self.ty, inner_shape);
        printer.print_newline();
        printer.print_spaces(shape.indent);
        printer.print_raw_token(&self.close_paren);
        PrintInfo::default_multi_lined()
    }
}

impl Printable for ParenType {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        // Calculate max width for inner type
        let single_lined_max_width = shape.width.saturating_sub(2);
        let multi_lined_max_width = printer
            .config
            .line_width
            .saturating_sub(shape.indent + printer.config.indent_width);

        let mut inner_printer = Printer::new_empty(printer.input, printer.config, printer.trivia);
        let inner_shape = Shape {
            width: single_lined_max_width,
            indent: shape.indent + printer.config.indent_width,
            first_line_offset: 0,
        };
        let info = inner_printer.print(&*self.ty, inner_shape);

        if info.multi_lined || inner_printer.output.len() > single_lined_max_width {
            // We do not fit, switch to multi-line
            let inner_shape = Shape {
                width: multi_lined_max_width,
                indent: shape.indent + printer.config.indent_width,
                first_line_offset: 0,
            };
            printer.print_raw_token(&self.open_paren);
            printer.print_newline();
            printer.print_spaces(inner_shape.indent);
            printer.print(&*self.ty, inner_shape);
            printer.print_newline();
            printer.print_spaces(shape.indent);
            printer.print_raw_token(&self.close_paren);
            PrintInfo::default_multi_lined()
        } else {
            // We fit, print single-line
            printer.print_raw_token(&self.open_paren);
            printer.append_from_printer(inner_printer);
            printer.print_raw_token(&self.close_paren);
            PrintInfo::default_single_line()
        }
    }
}

#[derive(Debug)]
pub struct PathType {
    pub first: t::Word,
    pub rest: Vec<(t::DoubleColon, t::Word)>,
}

impl Printable for PathType {
    /// Always prints as a single line.
    fn print(&self, _shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.first);
        for (double_colon, word) in &self.rest {
            printer.print_raw_token(double_colon);
            printer.print_raw_token(word);
        }
        PrintInfo::default_single_line()
    }
}

#[derive(Debug)]
pub struct StringType(pub t::QuotedString);

impl Printable for StringType {
    fn print(&self, _shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer.print_raw_token(&self.0);
        PrintInfo::default_single_line()
    }
}

#[derive(Debug)]
pub struct UnionType {
    pub first: Box<UnionTypeMember>,
    pub rest: Vec<(t::Pipe, UnionTypeMember)>,
}

impl PrintMultiLine for UnionType {
    fn print_multi_line(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let mut info = printer.print(&*self.first, shape.clone());
        for (pipe, ty) in &self.rest {
            info.multi_lined = true;
            printer.print_newline();
            printer.print_spaces(shape.indent + printer.config.indent_width);
            printer.print_raw_token(pipe);
            printer.print_str(" ");
            printer.print(ty, shape.clone());
        }
        info
    }
}

impl Printable for UnionType {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        // try printing single-line first, then multi-line if it doesn't fit

        let mut single_line_printer =
            Printer::new_empty(printer.input, printer.config, printer.trivia);
        let mut multi_line = false;
        multi_line |= single_line_printer
            .print(&*self.first, shape.clone())
            .multi_lined;
        for (pipe, ty) in &self.rest {
            if multi_line || single_line_printer.output.len() > shape.width {
                return Self::print_multi_line(&self, shape, printer);
            }
            single_line_printer.print_str(" ");
            single_line_printer.print_raw_token(pipe);
            single_line_printer.print_str(" ");
            multi_line |= single_line_printer.print(ty, shape.clone()).multi_lined;
        }
        if multi_line || single_line_printer.output.len() > shape.width {
            return Self::print_multi_line(&self, shape, printer);
        }

        printer.append_from_printer(single_line_printer);
        PrintInfo::default_single_line()
    }
}

#[derive(Debug)]
pub enum UnionTypeMember {
    Paren(ParenType),
    Path(PathType),
    Literal(Literal),
    Optional(OptionalType),
    Array(ArrayType),
    Generic(GenericType),
    Function(FunctionType),
    /// Types constrained by attributes.
    Constrained(TextRange), // TODO
    Unknown(TextRange),
}

impl UnionTypeMember {
    /// Take a base type (no postfix operators).
    /// If there are postix operators, they will remain in the iterator.
    ///
    /// So Paren, Path, String, or Function.
    fn take_base_type(it: &mut SyntaxNodeIter) -> Result<Self, StrongAstError> {
        let first = it.expect_next("a type")?;
        match first.kind() {
            SyntaxKind::L_PAREN => {
                // Either a parenthesized type or a function type
                let open_paren = t::LParen::from_cst(first)?;
                let mut params = Vec::new();
                let close_paren = loop {
                    let Some(elem) = it.next() else {
                        return Err(StrongAstError::missing(SyntaxKind::R_PAREN, it.parent));
                    };
                    match elem.kind() {
                        SyntaxKind::R_PAREN => {
                            break t::RParen::from_cst(elem)?;
                        }
                        SyntaxKind::FUNCTION_TYPE_PARAM => {
                            let param = FunctionTypeParam::from_cst(elem)?;
                            let comma = it
                                .next_if_kind(SyntaxKind::COMMA)
                                .map(t::Comma::from_cst)
                                .transpose()?;
                            params.push((param, comma));
                        }
                        _ => {
                            return Err(StrongAstError::UnexpectedKindDesc {
                                expected_desc: "FUNCTION_TYPE_PARAM or R_PAREN".into(),
                                found: elem.kind(),
                                at: elem.text_range(),
                            });
                        }
                    }
                };
                let must_be_func_type = params.len() != 1
                    || params
                        .iter()
                        .any(|item| item.0.name.is_some() || item.1.is_some());
                if must_be_func_type {
                    let arrow = it.expect_parse()?;
                    let return_ty: Type = it.expect_parse()?;

                    Ok(UnionTypeMember::Function(FunctionType {
                        open_paren,
                        params,
                        close_paren,
                        arrow,
                        return_type: Box::new(return_ty),
                    }))
                } else if let Some(arrow) = it.next_if_kind(SyntaxKind::ARROW) {
                    let arrow = t::Arrow::from_cst(arrow)?;
                    let return_ty: Type = it.expect_parse()?;

                    Ok(UnionTypeMember::Function(FunctionType {
                        open_paren,
                        params,
                        close_paren,
                        arrow,
                        return_type: Box::new(return_ty),
                    }))
                } else {
                    // Really a paren type
                    let (inner, _) = params
                        .pop()
                        .unwrap_or_else(|| unreachable!("we checked it has length 1"));
                    Ok(UnionTypeMember::Paren(ParenType {
                        open_paren,
                        ty: inner.ty,
                        close_paren,
                    }))
                }
            }
            SyntaxKind::WORD => {
                let first = t::Word::from_cst(first)?;
                let mut rest = Vec::new();
                while let Some(double_colon) = it.next_if_kind(SyntaxKind::DOUBLE_COLON) {
                    let double_colon = t::DoubleColon::from_cst(double_colon)?;
                    let word = it.expect_parse()?;
                    rest.push((double_colon, word));
                }
                Ok(UnionTypeMember::Path(PathType { first, rest }))
            }
            SyntaxKind::STRING_LITERAL
            | SyntaxKind::INTEGER_LITERAL
            | SyntaxKind::FLOAT_LITERAL => {
                let string = Literal::from_cst(first)?;
                Ok(UnionTypeMember::Literal(string))
            }
            found => Err(StrongAstError::UnexpectedKindDesc {
                expected_desc: "L_PAREN, WORD, STRING_LITERAL, INTEGER_LITERAL, or FLOAT_LITERAL"
                    .into(),
                found,
                at: first.text_range(),
            }),
        }
    }
    pub fn take(it: &mut SyntaxNodeIter) -> Result<Self, StrongAstError> {
        let mut ty = Self::take_base_type(it)?;

        // Handle non-union postfix operators: `[][][][]...`, `?`, `<...>`
        loop {
            if it
                .peek()
                .is_some_and(|elem| elem.kind() == SyntaxKind::L_BRACKET)
            {
                // Array type
                let mut brackets = Vec::new();
                while let Some(open_bracket) = it.next_if_kind(SyntaxKind::L_BRACKET) {
                    let open_bracket = t::LBracket::from_cst(open_bracket)?;
                    let close_bracket: t::RBracket = it.expect_parse()?;
                    brackets.push((open_bracket, close_bracket));
                }
                ty = UnionTypeMember::Array(ArrayType {
                    ty: Box::new(ty.into()),
                    brackets,
                });
                continue;
            } else if let Some(question) = it.next_if_kind(SyntaxKind::QUESTION) {
                // Optional type
                let question = t::Question::from_cst(question)?;
                ty = UnionTypeMember::Optional(OptionalType {
                    ty: Box::new(ty.into()),
                    question,
                });
                continue;
            } else if let Some(type_args) = it.next_if_kind(SyntaxKind::TYPE_ARGS) {
                // Generic type
                let type_args = TypeArgs::from_cst(type_args)?;
                ty = UnionTypeMember::Generic(GenericType {
                    base: Box::new(ty.into()),
                    args: type_args,
                });
                continue;
            }
            // Done with postfix operators
            break;
        }

        Ok(ty)
    }
}

impl From<UnionTypeMember> for Type {
    fn from(member: UnionTypeMember) -> Self {
        match member {
            UnionTypeMember::Paren(paren) => Type::Paren(paren),
            UnionTypeMember::Path(path) => Type::Path(path),
            UnionTypeMember::Literal(literal) => Type::Literal(literal),
            UnionTypeMember::Optional(optional) => Type::Optional(optional),
            UnionTypeMember::Array(array) => Type::Array(array),
            UnionTypeMember::Generic(generic) => Type::Generic(generic),
            UnionTypeMember::Function(function) => Type::Function(function),
            UnionTypeMember::Constrained(range) | UnionTypeMember::Unknown(range) => {
                Type::Unknown(range)
            }
        }
    }
}

impl Printable for UnionTypeMember {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            UnionTypeMember::Paren(paren) => paren.print(shape, printer),
            UnionTypeMember::Path(path) => path.print(shape, printer),
            UnionTypeMember::Literal(literal) => literal.print(shape, printer),
            UnionTypeMember::Optional(optional) => optional.print(shape, printer),
            UnionTypeMember::Array(array) => array.print(shape, printer),
            UnionTypeMember::Generic(generic) => generic.print(shape, printer),
            UnionTypeMember::Function(function) => function.print(shape, printer),
            UnionTypeMember::Constrained(range) | UnionTypeMember::Unknown(range) => {
                printer.print_input_range(*range);
                PrintInfo::default_single_line()
            }
        }
    }
}

#[derive(Debug)]
pub struct OptionalType {
    pub ty: Box<Type>,
    pub question: t::Question,
}

impl Printable for OptionalType {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let info = printer.print(&*self.ty, shape);
        printer.print_raw_token(&self.question);
        info
    }
}

#[derive(Debug)]
pub struct ArrayType {
    pub ty: Box<Type>,
    pub brackets: Vec<(t::LBracket, t::RBracket)>,
}

impl Printable for ArrayType {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let info = printer.print(&*self.ty, shape);
        for (open, close) in &self.brackets {
            printer.print_raw_token(open);
            printer.print_raw_token(close);
        }
        info
    }
}

#[derive(Debug)]
pub struct GenericType {
    pub base: Box<Type>,
    pub args: TypeArgs,
}

impl Printable for GenericType {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let mut multi_lined = false;
        multi_lined |= printer.print(&*self.base, shape.clone()).multi_lined;
        multi_lined |= printer.print(&self.args, shape).multi_lined;
        PrintInfo { multi_lined }
    }
}

/// Corresponds to a [`SyntaxKind::TYPE_ARGS`] node.
#[derive(Debug)]
pub struct TypeArgs {
    pub open_angle: t::Less,
    pub first: Box<Type>,
    pub rest: Vec<(t::Comma, Type)>,
    pub close_angle: t::Greater,
}

impl FromCST for TypeArgs {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;
        StrongAstError::assert_kind_node(&node, SyntaxKind::TYPE_ARGS)?;

        let mut it = SyntaxNodeIter::new(node);

        let open_angle: t::Less = it.expect_parse()?;

        let first: Type = it.expect_parse()?;

        let mut rest = Vec::new();
        let close_angle = loop {
            let Some(elem) = it.next() else {
                return Err(StrongAstError::missing(SyntaxKind::GREATER, it.parent));
            };
            match elem.kind() {
                SyntaxKind::COMMA => {
                    let comma = StrongAstError::assert_is_token(elem)?;
                    let comma = t::Comma::new_from_span(comma.text_range());
                    let next: Type = it.expect_parse()?;
                    rest.push((comma, next));
                }
                SyntaxKind::GREATER => {
                    let token = StrongAstError::assert_is_token(elem)?;
                    let close_angle = t::Greater::new_from_span(token.text_range());
                    break close_angle;
                }
                _ => {
                    return Err(StrongAstError::UnexpectedKindDesc {
                        expected_desc: "COMMA or GREATER".into(),
                        found: elem.kind(),
                        at: elem.text_range(),
                    });
                }
            }
        };

        it.expect_end()?;

        Ok(TypeArgs {
            open_angle,
            first: Box::new(first),
            rest,
            close_angle,
        })
    }
}

impl KnownKind for TypeArgs {
    fn kind() -> SyntaxKind {
        SyntaxKind::TYPE_ARGS
    }
}

impl PrintMultiLine for TypeArgs {
    /// Multi-line layout: each type argument on its own indented line
    /// with trailing comma. Closing `>` on its own line.
    ///
    /// ```baml
    /// <
    ///     SomeLongType,
    ///     AnotherType,
    /// >
    /// ```
    fn print_multi_line(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let inner_shape = Shape {
            width: shape.width.saturating_sub(printer.config.indent_width),
            indent: shape.indent + printer.config.indent_width,
            first_line_offset: 0,
        };

        printer.print_raw_token(&self.open_angle);
        printer.print_newline();

        printer.print_spaces(inner_shape.indent);
        printer.print(&*self.first, inner_shape.clone());
        printer.print_str(",");
        printer.print_newline();

        for (_comma, ty) in &self.rest {
            printer.print_spaces(inner_shape.indent);
            printer.print(ty, inner_shape.clone());
            printer.print_str(",");
            printer.print_newline();
        }

        printer.print_spaces(shape.indent);
        printer.print_raw_token(&self.close_angle);
        PrintInfo::default_multi_lined()
    }
}

impl Printable for TypeArgs {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let mut multi_lined = false;
        let mut single_line_printer =
            Printer::new_empty(printer.input, printer.config, printer.trivia);
        single_line_printer.print_raw_token(&self.open_angle);
        multi_lined |= single_line_printer
            .print(&*self.first, Shape::unlimited_single_line())
            .multi_lined;
        for (comma, ty) in &self.rest {
            single_line_printer.print_raw_token(comma);
            single_line_printer.print_str(" ");
            multi_lined |= single_line_printer
                .print(ty, Shape::unlimited_single_line())
                .multi_lined;
        }
        single_line_printer.print_raw_token(&self.close_angle);

        if multi_lined || single_line_printer.output.len() > shape.width {
            Self::print_multi_line(self, shape, printer)
        } else {
            printer.append_from_printer(single_line_printer);
            PrintInfo::default_single_line()
        }
    }
}

#[derive(Debug)]
pub struct FunctionType {
    pub open_paren: t::LParen,
    pub params: Vec<(FunctionTypeParam, Option<t::Comma>)>,
    pub close_paren: t::RParen,
    pub arrow: t::Arrow,
    pub return_type: Box<Type>,
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
        printer.print_newline();

        for (param, comma) in &self.params {
            printer.print_spaces(inner_shape.indent);
            printer.print(param, inner_shape.clone());
            if let Some(comma) = comma {
                printer.print_raw_token(comma);
            } else {
                printer.print_str(",");
            }
            printer.print_newline();
        }

        printer.print_spaces(shape.indent);
        printer.print_raw_token(&self.close_paren);
        printer.print_str(" ");
        printer.print_raw_token(&self.arrow);
        printer.print_str(" ");
        printer.print(&*self.return_type, shape);
        PrintInfo::default_multi_lined()
    }
}

impl Printable for FunctionType {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let mut multi_lined = false;
        let mut single_line_printer =
            Printer::new_empty(printer.input, printer.config, printer.trivia);
        single_line_printer.print_raw_token(&self.open_paren);
        for (i, (param, comma)) in self.params.iter().enumerate() {
            multi_lined |= single_line_printer
                .print(param, Shape::unlimited_single_line())
                .multi_lined;
            if i + 1 < self.params.len() {
                if let Some(comma) = comma {
                    single_line_printer.print_raw_token(comma);
                } else {
                    single_line_printer.print_str(",");
                }
                single_line_printer.print_str(" ");
            }
        }
        single_line_printer.print_raw_token(&self.close_paren);
        single_line_printer.print_str(" ");
        single_line_printer.print_raw_token(&self.arrow);
        single_line_printer.print_str(" ");
        multi_lined |= single_line_printer
            .print(&*self.return_type, Shape::unlimited_single_line())
            .multi_lined;

        if multi_lined || single_line_printer.output.len() > shape.width {
            Self::print_multi_line(self, shape, printer)
        } else {
            printer.append_from_printer(single_line_printer);
            PrintInfo::default_single_line()
        }
    }
}

/// Corresponds to a [`SyntaxKind::FUNCTION_TYPE_PARAM`] node.
///
/// Exists in [`FunctionType`] but also in [`ParenType`] for some reason.
#[derive(Debug)]
pub struct FunctionTypeParam {
    pub name: Option<(t::Word, Option<t::Colon>)>,
    pub ty: Box<Type>,
}

impl FromCST for FunctionTypeParam {
    fn from_cst(elem: SyntaxElement) -> Result<Self, StrongAstError> {
        let node = StrongAstError::assert_is_node(elem)?;

        let mut it = SyntaxNodeIter::new(node);

        let name = if let Some(name) = it.next_if_kind(SyntaxKind::WORD) {
            let name = t::Word::new_from_span(name.text_range());
            let colon = it
                .next_if_kind(SyntaxKind::COLON)
                .map(|elem| {
                    let colon = StrongAstError::assert_is_token(elem)?;
                    Ok(t::Colon::new_from_span(colon.text_range()))
                })
                .transpose()?;
            Some((name, colon))
        } else {
            None
        };

        let ty: Type = it.expect_parse()?;

        it.expect_end()?;

        Ok(FunctionTypeParam {
            name,
            ty: Box::new(ty),
        })
    }
}

impl Printable for FunctionTypeParam {
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        if let Some((name, colon)) = &self.name {
            printer.print_raw_token(name);
            if let Some(colon) = colon {
                printer.print_raw_token(colon);
            } else {
                printer.print_str(":");
            }
            printer.print_str(" ");
        }
        printer.print(&*self.ty, shape);
        PrintInfo::default_single_line()
    }
}
