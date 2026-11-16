use baml_syntax::{
    nodes::{
        AnyClassMember, AnyDoc, AnyEnumMember, AnyFunctionBody, AnyImplMember, AnyInterfaceMember,
        AnyItem, AnyLlmField, AnyStringLike, AssocTypeDecl, AssocTypeDef, ClassDef, ClientField,
        EnumDef, EnumVariant, Field, FunctionDef, GenericParamList, ImplementsBlock,
        InterfaceFieldLink, LetItem, LlmBody, MethodSig, Name, Param, ParamList, PromptField,
        ReturnType, TestDef, ThrowsClause, ToolsField, TypeAliasDef,
    },
    token::{Token, TokenIdx, TokenKind, TokenSet, kw, punct, sets::MemberSep},
    tree::{Id, List, SepItem},
};
use text_size::TextRange;

use super::{
    attributes::AttributeExt as _, pattern::print_trivia_squished_spaced, present,
    types::multi_line_is_indented, union_printable,
};
use crate::{
    printer::{PrintInfo, PrintMultiLine, Printable, Printer, Shape, Spanned},
    trivia_classifier::{EmittableTrivia, TriviaSliceExt as _},
};

/// The `///` lines, `//baml:` directives and `//#` title a declaration
/// owns, each on its own line before it, with the plain comments written
/// between them (and before the declaration's keyword) kept on their own
/// lines. The parent has printed the indentation for the first line; this
/// prints it for the declaration's own line.
pub(super) fn print_docs(docs: List<AnyDoc>, indent: usize, printer: &mut Printer) {
    let tree = printer.tree;
    for doc in tree.list(docs) {
        printer.print(*doc, Shape::unlimited_single_line());
        printer.print_newline();
        let last = TokenIdx(doc.tokens(tree).end().0 - 1);
        if let Some(next) = tree.tokens().next_significant(last) {
            // The comments before the next line (a later doc, or the
            // keyword) are attached to it, and nobody else prints them: the
            // declaration's own leading trivia is the first doc's.
            let (leading, _) = printer
                .trivia
                .get_for_range_split(tree.tokens().range(next));
            if leading.iter().any(EmittableTrivia::is_comment) {
                printer.print_trivia_with_newline(leading.trim_blanks(), indent);
            } else {
                // A blank line written after a doc line stays, one at most.
                let gap_start = tree.tokens().range(last).end();
                let gap_end = tree.tokens().range(next).start();
                if super::has_blank_line(&printer.input[TextRange::new(gap_start, gap_end)]) {
                    printer.print_newline();
                }
            }
        }
        printer.print_spaces(indent);
    }
}

/// Whether a member separator is a comma.
fn is_comma(sep: Token<MemberSep>, printer: &Printer) -> bool {
    printer.tree.tokens().kind(sep.idx) == TokenKind::Comma
}

/// The comma after a class or interface field: the one written, or an added
/// one (a `;` is replaced).
fn print_member_comma(sep: Option<Token<MemberSep>>, printer: &mut Printer) {
    match sep {
        Some(sep) if is_comma(sep, printer) => printer.print_token(sep),
        Some(_) | None => printer.print_str(","),
    }
}

impl Printable for AnyItem {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            AnyItem::Function(function) => printer.print(function, shape),
            AnyItem::Class(class) => printer.print(class, shape),
            AnyItem::Interface(interface) => printer.print(interface, shape),
            AnyItem::Enum(enum_def) => printer.print(enum_def, shape),
            AnyItem::Test(test) => printer.print(test, shape),
            AnyItem::TypeAlias(alias) => printer.print(alias, shape),
            AnyItem::Let(item) => printer.print(item, shape),
            AnyItem::TypeBinding(stmt) => printer.print(stmt, shape),
            // Not modelled by the printer yet: printed as written.
            AnyItem::ImplementsFor(item) => {
                printer.print_verbatim(item);
                PrintInfo::default_multi_lined()
            }
            AnyItem::ClientValue(item) => {
                printer.print_verbatim(item);
                PrintInfo::default_multi_lined()
            }
            // A removed form parses and is refused by lowering; the
            // formatter leaves it as written.
            AnyItem::Removed(item) => {
                printer.print_verbatim(item);
                PrintInfo::default_multi_lined()
            }
            AnyItem::Bogus(_) => unreachable!("the formatter prints only error-free trees"),
        }
    }
}

impl Printable for Id<LetItem> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let node = printer.tree.get(self);
        print_docs(node.docs, shape.indent, printer);
        printer.print(node.stmt, shape)
    }
}

/// The signature a function definition and an interface method share.
#[derive(Clone, Copy)]
struct Signature {
    docs: List<AnyDoc>,
    kw: Token<kw::Function>,
    name: Id<Name>,
    generics: Option<Id<GenericParamList>>,
    params: Id<ParamList>,
    ret: Id<ReturnType>,
    throws: Option<Id<ThrowsClause>>,
}

impl Signature {
    fn of_function(id: Id<FunctionDef>, printer: &Printer) -> Self {
        let node = printer.tree.get(id);
        Self {
            docs: node.docs,
            kw: node.kw,
            name: present(node.name),
            generics: node.generics,
            params: present(node.params),
            ret: present(node.ret),
            throws: node.throws,
        }
    }

    fn of_method_sig(id: Id<MethodSig>, printer: &Printer) -> Self {
        let node = printer.tree.get(id);
        Self {
            docs: node.docs,
            kw: node.kw,
            name: present(node.name),
            generics: node.generics,
            params: present(node.params),
            ret: present(node.ret),
            throws: node.throws,
        }
    }

    fn print_with_body(
        self,
        body: Option<AnyFunctionBody>,
        shape: Shape,
        printer: &mut Printer,
    ) -> PrintInfo {
        let tree = printer.tree;
        print_docs(self.docs, shape.indent, printer);
        let start = printer.output.len();
        let continuation = shape.indent + printer.config.indent_width;
        printer.print_token(self.kw);
        printer.print_separator(
            printer.range(self.kw),
            Some(self.name.leftmost_token(tree)),
            continuation,
            " ",
        );
        printer.print(self.name, shape);
        let mut previous = self.name.rightmost_token(tree);
        if let Some(params) = self.generics {
            printer.print_separator(
                previous,
                Some(params.leftmost_token(tree)),
                continuation,
                "",
            );
            printer.print(params, shape);
            previous = params.rightmost_token(tree);
        }
        printer.print_separator(
            previous,
            Some(self.params.leftmost_token(tree)),
            continuation,
            "",
        );
        let mut candidate = printer.sub_printer();
        self.params
            .print(Shape::unlimited_single_line(), &mut candidate);
        self.print_return_and_throws(body, &Shape::unlimited_single_line(), &mut candidate);
        let terminator_width = 1;
        if !candidate.output.contains('\n')
            && printer.current_line_len() + candidate.output.len() + terminator_width
                <= printer.config.line_width
        {
            printer.append_from_printer(candidate);
        } else {
            self.params.print_multi_line(
                Shape {
                    width: 0,
                    indent: shape.indent,
                    first_line_offset: 0,
                },
                printer,
            );
            self.print_return_and_throws(body, &shape, printer);
        }
        if let Some(body) = body {
            printer.print(body, shape);
        }
        PrintInfo {
            multi_lined: printer.output[start..].contains('\n'),
        }
    }

    fn print_return_and_throws(
        self,
        body: Option<AnyFunctionBody>,
        shape: &Shape,
        printer: &mut Printer,
    ) {
        let tree = printer.tree;
        let ret = tree.get(self.ret);
        let return_type = present(ret.ty);
        let continuation = shape.indent + printer.config.indent_width;
        printer.print_separator(
            self.params.rightmost_token(tree),
            Some(printer.range(ret.arrow)),
            continuation,
            " ",
        );
        // `=>` is accepted in place of `->` and printed as the latter.
        printer.print_str("->");
        printer.print_separator(
            printer.range(ret.arrow),
            Some(return_type.leftmost_token(tree)),
            continuation,
            " ",
        );
        let remaining = if shape.width == usize::MAX {
            Shape::unlimited_single_line()
        } else {
            Shape {
                width: printer
                    .current_line_remaining_width()
                    .saturating_sub(if body.is_some() { 2 } else { 1 }),
                indent: shape.indent,
                first_line_offset: printer.current_line_len().saturating_sub(shape.indent),
            }
        };
        let return_info = printer.print(return_type, remaining);
        let mut previous = return_type.rightmost_token(tree);
        if let Some(throws) = self.throws {
            printer.print_separator(
                previous,
                Some(throws.leftmost_token(tree)),
                continuation,
                " ",
            );
            printer.print(throws, *shape);
            previous = throws.rightmost_token(tree);
        }
        if let Some(body) = body {
            printer.print_separator(previous, Some(body.leftmost_token(tree)), shape.indent, " ");
            if return_info.multi_lined
                && multi_line_is_indented(tree, return_type)
                && printer.current_line_len() > shape.indent
            {
                printer
                    .output
                    .truncate(printer.output.trim_end_matches(' ').len());
                printer.print_newline();
                printer.print_spaces(shape.indent);
            }
        }
    }
}

impl Printable for Id<FunctionDef> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let body = present(printer.tree.get(self).body);
        Signature::of_function(self, printer).print_with_body(Some(body), shape, printer)
    }
}

impl Printable for Id<MethodSig> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        Signature::of_method_sig(self, printer).print_with_body(None, shape, printer)
    }
}

union_printable!(AnyFunctionBody { Expr, Llm });

impl PrintMultiLine for Id<ParamList> {
    fn print_multi_line(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let tree = printer.tree;
        let node = tree.get(self);
        let close = present(node.r);
        let inner_indent = shape.indent + printer.config.indent_width;
        let inner_shape = Shape {
            width: printer.config.line_width.saturating_sub(inner_indent),
            indent: inner_indent,
            first_line_offset: 0,
        };
        printer.print_token(node.l);
        printer.print_trivia_all_trailing_for(printer.range(node.l));
        printer.print_newline();
        for SepItem { item: param, sep } in tree.list(node.params) {
            let (param_leading, param_trailing) = printer.trivia.get_for_element(tree, *param);
            printer.print_trivia_with_newline(param_leading.trim_blanks(), inner_shape.indent);
            printer.print_spaces(inner_shape.indent);
            printer.print(*param, inner_shape);
            if let Some(comma) = *sep {
                printer.print_trivia_squished(param_trailing);
                let (comma_leading, comma_trailing) =
                    printer.trivia.get_for_range_split(printer.range(comma));
                printer.print_trivia_squished(comma_leading);
                printer.print_token(comma);
                printer.print_trivia_trailing(comma_trailing);
            } else {
                printer.print_str(",");
                printer.print_trivia_trailing(param_trailing);
            }
            printer.print_newline();
        }
        let (close_paren_leading, _) = printer.trivia.get_for_range_split(printer.range(close));
        printer.print_trivia_with_newline(close_paren_leading.trim_blanks(), inner_shape.indent);
        printer.print_spaces(shape.indent);
        printer.print_token(close);
        PrintInfo::default_multi_lined()
    }
}

fn param_list_single_line(
    id: Id<ParamList>,
    shape: &Shape,
    printer: &mut Printer,
) -> Option<PrintInfo> {
    let tree = printer.tree;
    let node = tree.get(id);
    let close = present(node.r);
    let params = tree.list(node.params);
    printer.print_token(node.l);
    let (_, open_trailing) = printer.trivia.get_for_range_split(printer.range(node.l));
    printer.try_print_trivia_single_line_squished(open_trailing)?;
    for (i, SepItem { item: param, sep }) in params.iter().enumerate() {
        if printer.output.len() > shape.width {
            return None;
        }
        let (p_leading, p_trailing) = printer.trivia.get_for_element(tree, *param);
        printer.try_print_trivia_single_line_squished(p_leading)?;
        if printer
            .print(*param, Shape::unlimited_single_line())
            .multi_lined
        {
            return None;
        }
        let (comma_leading, comma_trailing) = if let Some(comma) = *sep {
            printer.trivia.get_for_range_split(printer.range(comma))
        } else {
            (&[][..], &[][..])
        };
        if i + 1 < params.len() {
            printer.print_trivia_squished(p_trailing);
            printer.print_trivia_squished(comma_leading);
            printer.print_str(", ");
            printer.try_print_trivia_single_line_squished(comma_trailing)?;
        } else {
            printer.try_print_trivia_single_line_squished(p_trailing)?;
            printer.try_print_trivia_single_line_squished(comma_leading)?;
            printer.try_print_trivia_single_line_squished(comma_trailing)?;
        }
    }
    let (close_leading, _) = printer.trivia.get_for_range_split(printer.range(close));
    printer.try_print_trivia_single_line_squished(close_leading)?;
    printer.print_token(close);
    if printer.output.len() > shape.width {
        None
    } else {
        Some(PrintInfo::default_single_line())
    }
}

impl Printable for Id<ParamList> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|p| param_list_single_line(self, &shape, p))
            .unwrap_or_else(|| self.print_multi_line(shape, printer))
    }
}

impl Printable for Id<Param> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let tree = printer.tree;
        let node = tree.get(self);
        let name = present(node.name);
        printer.print(name, shape);
        let mut info = if let Some(ascription) = node.ascription {
            let ascription = tree.get(ascription);
            let ty = present(ascription.ty);
            let mut trivia_len = 0;
            if let Some(colon) = ascription.colon {
                let (_, colon_trailing) = printer.trivia.get_for_range_split(printer.range(colon));
                printer.print_str(": ");
                trivia_len += printer.print_trivia_squished(colon_trailing);
            } else {
                printer.print_str(": ");
            }
            let ty_leading = printer.trivia.get_leading_for_element(tree, ty);
            trivia_len += printer.print_trivia_squished(ty_leading);
            let new_offset = usize::from(name.rightmost_token(tree).len()) + 2 + trivia_len;
            let ty_shape = Shape {
                width: shape.width.saturating_sub(new_offset),
                indent: shape.indent,
                first_line_offset: shape.first_line_offset + new_offset,
            };
            printer.print(ty, ty_shape)
        } else {
            PrintInfo::default_single_line()
        };
        if let Some(default) = node.default {
            let default = tree.get(default);
            let value = present(default.value);
            let prev_token = node.ascription.map_or_else(
                || name.rightmost_token(tree),
                |ascription| ascription.rightmost_token(tree),
            );
            let (_, prev_trailing) = printer.trivia.get_for_range_split(prev_token);
            let (equals_leading, equals_trailing) = printer
                .trivia
                .get_for_range_split(printer.range(default.eq));
            printer.print_trivia_squished(prev_trailing);
            printer.print_trivia_squished(equals_leading);
            printer.print_str(" = ");
            printer.print_trivia_squished(equals_trailing);
            let leading = printer.trivia.get_leading_for_element(tree, value);
            printer.print_trivia_squished(leading);
            info = printer.print(value, shape);
        }
        info
    }
}

impl Printable for Id<LlmBody> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let tree = printer.tree;
        let node = tree.get(self);
        let close = present(node.r);
        let inner_indent = shape.indent + printer.config.indent_width;
        printer.print_token(node.l);
        printer.print_trivia_all_trailing_for(printer.range(node.l));
        printer.print_newline();
        // `client`, `tools`, `prompt`: the fields print in that order, as written
        // or not.
        let mut fields = tree.list(node.fields).to_vec();
        fields.sort_by_key(|field| match field {
            AnyLlmField::Client(_) => 0,
            AnyLlmField::Tools(_) => 1,
            AnyLlmField::Prompt(_) => 2,
            AnyLlmField::Bogus(_) => unreachable!("the formatter prints only error-free trees"),
        });
        if let Some((first, rest)) = fields.split_first() {
            let (first_leading, first_trailing) = printer.trivia.get_for_element(tree, *first);
            printer.print_trivia_with_newline(first_leading.trim_leading_blanks(), inner_indent);
            printer.print_spaces(inner_indent);
            let inner_shape = Shape::standalone(printer.config.line_width, inner_indent);
            printer.print(*first, inner_shape);
            printer.print_trivia_trailing(first_trailing);
            printer.print_newline();
            for field in rest {
                printer.print_standalone_with_trivia(*field, inner_indent);
                printer.print_newline();
            }
        }
        let (close_brace_leading, _) = printer.trivia.get_for_range_split(printer.range(close));
        printer.print_trivia_with_newline(close_brace_leading.trim_trailing_blanks(), inner_indent);
        printer.print_spaces(shape.indent);
        printer.print_token(close);
        PrintInfo::default_multi_lined()
    }
}

union_printable!(AnyLlmField {
    Client,
    Prompt,
    Tools
});
union_printable!(AnyStringLike { String, Backtick });

/// `key: value` of an LLM body, comments around the colon kept on the line.
fn print_llm_field<K: TokenSet>(
    kw: Token<K>,
    colon: Token<punct::Colon>,
    value: impl Printable,
    shape: Shape,
    printer: &mut Printer,
) -> PrintInfo {
    printer.print_token(kw);
    let (_, keyword_trailing) = printer.trivia.get_for_range_split(printer.range(kw));
    printer.print_trivia_squished(keyword_trailing);
    let (colon_leading, colon_trailing) = printer.trivia.get_for_range_split(printer.range(colon));
    printer.print_trivia_squished(colon_leading);
    printer.print_str(": ");
    printer.print_trivia_squished(colon_trailing);
    let value_leading = printer.trivia.get_leading_for_element(printer.tree, value);
    printer.print_trivia_squished(value_leading);
    printer.print(value, shape)
}

impl Printable for Id<ClientField> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let node = printer.tree.get(self);
        print_llm_field(
            node.kw,
            present(node.colon),
            present(node.value),
            shape,
            printer,
        )
    }
}

impl Printable for Id<PromptField> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let node = printer.tree.get(self);
        print_llm_field(
            node.kw,
            present(node.colon),
            present(node.value),
            shape,
            printer,
        )
    }
}

impl Printable for Id<ToolsField> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let node = printer.tree.get(self);
        print_llm_field(
            node.kw,
            present(node.colon),
            present(node.value),
            shape,
            printer,
        )
    }
}

impl Printable for Id<ClassDef> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let tree = printer.tree;
        let node = tree.get(self);
        print_docs(node.docs, shape.indent, printer);
        printer.print_token(node.kw);
        printer.print_str(" ");
        printer.print(present(node.name), shape);
        if let Some(gp) = node.generics {
            printer.print(gp, shape);
        }
        printer.print_str(" ");
        print_declaration_body(
            present(node.l),
            tree.list(node.members),
            present(node.r),
            &shape,
            printer,
        )
    }
}

pub(super) fn print_declaration_body<T: Printable>(
    open_brace: Token<punct::LBrace>,
    items: &[T],
    close_brace: Token<punct::RBrace>,
    shape: &Shape,
    printer: &mut Printer,
) -> PrintInfo {
    let tree = printer.tree;
    let inner_indent = shape.indent + printer.config.indent_width;
    printer.print_token(open_brace);
    printer.print_trivia_all_trailing_for(printer.range(open_brace));
    printer.print_newline();
    if let Some((first, rest)) = items.split_first() {
        let (first_leading, first_trailing) = printer.trivia.get_for_element(tree, *first);
        printer.print_trivia_with_newline(first_leading.trim_leading_blanks(), inner_indent);
        printer.print_spaces(inner_indent);
        let inner_shape = Shape::standalone(printer.config.line_width, inner_indent);
        printer.print(*first, inner_shape);
        printer.print_trivia_trailing(first_trailing);
        printer.print_newline();
        for item in rest {
            printer.print_standalone_with_trivia(*item, inner_indent);
            printer.print_newline();
        }
    }
    let (close_brace_leading, _) = printer
        .trivia
        .get_for_range_split(printer.range(close_brace));
    printer.print_trivia_with_newline(close_brace_leading.trim_trailing_blanks(), inner_indent);
    printer.print_spaces(shape.indent);
    printer.print_token(close_brace);
    PrintInfo::default_multi_lined()
}

impl PrintMultiLine for Id<Field> {
    fn print_multi_line(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let tree = printer.tree;
        let node = tree.get(self);
        let ty = present(node.ty);
        let attrs = tree.list(node.attrs);
        let attr_shape = Shape::standalone(
            printer.config.line_width,
            shape.indent + printer.config.indent_width,
        );
        print_docs(node.docs, shape.indent, printer);
        printer.print(node.name, shape);
        let colon_trailing = if let Some(colon) = node.colon {
            let (_, colon_trailing) = printer.trivia.get_for_range_split(printer.range(colon));
            colon_trailing
        } else {
            &[][..]
        };
        printer.print_str(": ");
        printer.print_trivia_squished(colon_trailing);
        let (type_leading, type_trailing) = printer.trivia.get_for_element(tree, ty);
        printer.print_trivia_squished(type_leading);
        let ty_info = printer.print(ty, shape);
        if let [attr] = attrs
            && !ty_info.multi_lined
            && type_trailing.is_empty()
            && printer.trivia.get_for_element(tree, *attr).0.is_empty()
            && let remaining_width = printer.current_line_remaining_width().saturating_sub(1)
            && attr.non_wrappable_len(printer) <= remaining_width
        {
            printer.print_spaces(1);
            let attr_shape = Shape {
                width: remaining_width,
                indent: shape.indent,
                first_line_offset: printer
                    .config
                    .line_width
                    .saturating_sub(shape.indent + remaining_width),
            };
            printer.print(*attr, attr_shape);
            return PrintInfo::default_multi_lined();
        }
        if !attrs.is_empty() {
            printer.print_trivia_trailing(type_trailing);
        }
        for (i, attr) in attrs.iter().enumerate() {
            let (attr_leading, attr_trailing) = printer.trivia.get_for_element(tree, *attr);
            printer.print_newline();
            printer.print_trivia_with_newline(attr_leading.trim_blanks(), attr_shape.indent);
            printer.print_spaces(attr_shape.indent);
            printer.print(*attr, attr_shape);
            let is_last = i + 1 >= attrs.len();
            if !is_last {
                printer.print_trivia_trailing(attr_trailing);
            }
        }
        PrintInfo::default_multi_lined()
    }
}

fn field_single_line(id: Id<Field>, shape: &Shape, printer: &mut Printer) -> Option<PrintInfo> {
    let tree = printer.tree;
    let node = tree.get(id);
    let ty = present(node.ty);
    let attrs = tree.list(node.attrs);
    print_docs(node.docs, shape.indent, printer);
    // The docs are lines of their own: the width check is the field's line.
    let line_start = printer.len();
    printer.print(node.name, *shape);
    let colon_trailing = if let Some(colon) = node.colon {
        let (_, colon_trailing) = printer.trivia.get_for_range_split(printer.range(colon));
        colon_trailing
    } else {
        &[][..]
    };
    printer.print_str(": ");
    printer.try_print_trivia_single_line_squished(colon_trailing)?;
    let (type_leading, type_trailing) = printer.trivia.get_for_element(tree, ty);
    printer.print_trivia_squished(type_leading);
    if printer
        .print(ty, Shape::unlimited_single_line())
        .multi_lined
        || printer.len() - line_start > shape.width
    {
        return None;
    }
    if !attrs.is_empty() {
        printer.try_print_trivia_single_line_squished(type_trailing)?;
    }
    for (i, attr) in attrs.iter().enumerate() {
        printer.print_str(" ");
        let (attr_leading, attr_trailing) = printer.trivia.get_for_element(tree, *attr);
        printer.try_print_trivia_single_line_squished(attr_leading)?;
        if printer
            .print(*attr, Shape::unlimited_single_line())
            .multi_lined
        {
            return None;
        }
        let is_last = i + 1 >= attrs.len();
        if !is_last {
            printer.try_print_trivia_single_line_squished(attr_trailing)?;
        }
    }
    if printer.len() - line_start > shape.width {
        None
    } else {
        Some(PrintInfo::default_single_line())
    }
}

impl Printable for Id<Field> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|p| field_single_line(self, &shape, p))
            .unwrap_or_else(|| self.print_multi_line(shape, printer))
    }
}

/// `type Name [extends B] [= T]` as declared in an interface, with its
/// separator printed as `;`.
impl Printable for Id<AssocTypeDecl> {
    fn print(self, mut shape: Shape, printer: &mut Printer) -> PrintInfo {
        let tree = printer.tree;
        let node = tree.get(self);
        let name = present(node.name);
        print_docs(node.docs, shape.indent, printer);
        let continuation = shape.indent + printer.config.indent_width;
        shape.width = shape.width.saturating_sub(1);
        let start = printer.output.len();
        printer.print_token(node.kw);
        printer.print_separator(
            printer.range(node.kw),
            Some(name.leftmost_token(tree)),
            continuation,
            " ",
        );
        printer.print(name, shape);
        let mut previous = name.rightmost_token(tree);
        if let Some(bound) = node.bound {
            let ty = present(bound.ty);
            printer.print_separator(previous, Some(printer.range(bound.kw)), continuation, " ");
            printer.print_token(bound.kw);
            printer.print_separator(
                printer.range(bound.kw),
                Some(ty.leftmost_token(tree)),
                continuation,
                " ",
            );
            printer.print(ty, shape);
            previous = ty.rightmost_token(tree);
        }
        if let Some(default) = node.default {
            let ty = present(default.ty);
            printer.print_separator(previous, Some(printer.range(default.eq)), continuation, " ");
            printer.print_token(default.eq);
            printer.print_separator(
                printer.range(default.eq),
                Some(ty.leftmost_token(tree)),
                continuation,
                " ",
            );
            printer.print(ty, shape);
            previous = ty.rightmost_token(tree);
        }
        let info = PrintInfo {
            multi_lined: printer.output[start..].contains('\n'),
        };
        printer.print_semicolon(
            previous,
            node.sep.map(|sep| printer.range(sep)),
            continuation,
        );
        info
    }
}

/// `type Name = T` as bound in an `implements` block.
impl Printable for Id<AssocTypeDef> {
    fn print(self, mut shape: Shape, printer: &mut Printer) -> PrintInfo {
        let tree = printer.tree;
        let node = tree.get(self);
        let name = present(node.name);
        let eq = present(node.eq);
        let ty = present(node.ty);
        print_docs(node.docs, shape.indent, printer);
        let continuation = shape.indent + printer.config.indent_width;
        shape.width = shape.width.saturating_sub(1);
        let start = printer.output.len();
        printer.print_token(node.kw);
        printer.print_separator(
            printer.range(node.kw),
            Some(name.leftmost_token(tree)),
            continuation,
            " ",
        );
        printer.print(name, shape);
        printer.print_separator(
            name.rightmost_token(tree),
            Some(printer.range(eq)),
            continuation,
            " ",
        );
        printer.print_token(eq);
        printer.print_separator(
            printer.range(eq),
            Some(ty.leftmost_token(tree)),
            continuation,
            " ",
        );
        printer.print(ty, shape);
        let info = PrintInfo {
            multi_lined: printer.output[start..].contains('\n'),
        };
        printer.print_semicolon(
            ty.rightmost_token(tree),
            node.sep.map(|sep| printer.range(sep)),
            continuation,
        );
        info
    }
}

impl Printable for Id<InterfaceFieldLink> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let node = printer.tree.get(self);
        print_docs(node.docs, shape.indent, printer);
        printer.print(node.iface_field, shape);
        printer.print_str(" ");
        printer.print_token(present(node.as_kw));
        printer.print_str(" ");
        printer.print(present(node.class_field), shape);
        PrintInfo::default_single_line()
    }
}

impl Printable for AnyImplMember {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            AnyImplMember::AssocType(decl) => printer.print(decl, shape),
            AnyImplMember::FieldLink(link) => printer.print(link, shape),
            AnyImplMember::Field(field) => {
                let info = printer.print(field, shape);
                if let Some(sep) = printer.tree.get(field).sep
                    && is_comma(sep, printer)
                {
                    printer.print_token(sep);
                }
                info
            }
            AnyImplMember::Method(function) => printer.print(function, shape),
            AnyImplMember::Bogus(_) => unreachable!("the formatter prints only error-free trees"),
        }
    }
}

impl Printable for Id<ImplementsBlock> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let tree = printer.tree;
        let node = tree.get(self);
        let target = present(node.target);
        let open = present(node.l);
        let close = present(node.r);
        let members = tree.list(node.members);
        print_docs(node.docs, shape.indent, printer);
        // `implement` is accepted and printed as `implements`.
        printer.print_str("implements");
        let (_, keyword_trailing) = printer.trivia.get_for_range_split(printer.range(node.kw));
        let trivia_len = printer.print_trivia_squished(keyword_trailing);
        if trivia_len == 0 {
            printer.print_str(" ");
        }
        let target_leading = printer.trivia.get_leading_for_element(tree, target);
        printer.print_trivia_squished(target_leading);
        printer.print(target, shape);
        if members.is_empty() {
            printer.print_str(" ");
            printer.print_token(open);
            printer.print_token(close);
            return PrintInfo::default_single_line();
        }
        let inner_indent = shape.indent + printer.config.indent_width;
        printer.print_str(" ");
        printer.print_token(open);
        printer.print_trivia_all_trailing_for(printer.range(open));
        printer.print_newline();
        if let Some((first, rest)) = members.split_first() {
            let (first_leading, first_trailing) = printer.trivia.get_for_element(tree, *first);
            printer.print_trivia_with_newline(first_leading.trim_leading_blanks(), inner_indent);
            printer.print_spaces(inner_indent);
            let inner_shape = Shape::standalone(printer.config.line_width, inner_indent);
            printer.print(*first, inner_shape);
            printer.print_trivia_trailing(first_trailing);
            printer.print_newline();
            for item in rest {
                printer.print_standalone_with_trivia(*item, inner_indent);
                printer.print_newline();
            }
        }
        let (close_brace_leading, _) = printer.trivia.get_for_range_split(printer.range(close));
        printer.print_trivia_with_newline(close_brace_leading.trim_trailing_blanks(), inner_indent);
        printer.print_spaces(shape.indent);
        printer.print_token(close);
        PrintInfo::default_multi_lined()
    }
}

impl Printable for AnyClassMember {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            AnyClassMember::Field(field) => {
                let info = printer.print(field, shape);
                print_member_comma(printer.tree.get(field).sep, printer);
                info
            }
            AnyClassMember::Method(function) => printer.print(function, shape),
            AnyClassMember::Implements(block) => printer.print(block, shape),
            AnyClassMember::BlockAttr(attr) => printer.print(attr, shape),
            AnyClassMember::Bogus(_) => unreachable!("the formatter prints only error-free trees"),
        }
    }
}

impl Printable for AnyInterfaceMember {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            AnyInterfaceMember::Field(field) => {
                let info = printer.print(field, shape);
                print_member_comma(printer.tree.get(field).sep, printer);
                info
            }
            AnyInterfaceMember::AssocType(decl) => printer.print(decl, shape),
            AnyInterfaceMember::MethodSig(method) => {
                let tree = printer.tree;
                let continuation = shape.indent + printer.config.indent_width;
                let info = printer.print(method, shape);
                let node = tree.get(method);
                let last = node.throws.map_or_else(
                    || present(node.ret).rightmost_token(tree),
                    |throws| throws.rightmost_token(tree),
                );
                printer.print_semicolon(
                    last,
                    node.semi.map(|semi| printer.range(semi)),
                    continuation,
                );
                info
            }
            AnyInterfaceMember::Method(method) => printer.print(method, shape),
            AnyInterfaceMember::BlockAttr(attr) => printer.print(attr, shape),
            AnyInterfaceMember::Bogus(_) => {
                unreachable!("the formatter prints only error-free trees")
            }
        }
    }
}

impl Printable for Id<EnumDef> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let tree = printer.tree;
        let node = tree.get(self);
        let open = present(node.l);
        let close = present(node.r);
        print_docs(node.docs, shape.indent, printer);
        let inner_indent = shape.indent + printer.config.indent_width;
        printer.print_token(node.kw);
        printer.print_str(" ");
        printer.print(present(node.name), shape);
        printer.print_str(" ");
        printer.print_token(open);
        printer.print_trivia_all_trailing_for(printer.range(open));
        printer.print_newline();
        if let Some((first, rest)) = tree.list(node.members).split_first() {
            let (first_leading, first_trailing) = printer.trivia.get_for_element(tree, *first);
            printer.print_trivia_with_newline(first_leading.trim_leading_blanks(), inner_indent);
            printer.print_spaces(inner_indent);
            let inner_shape = Shape::standalone(printer.config.line_width, inner_indent);
            printer.print(*first, inner_shape);
            printer.print_trivia_trailing(first_trailing);
            printer.print_newline();
            for item in rest {
                printer.print_standalone_with_trivia(*item, inner_indent);
                printer.print_newline();
            }
        }
        let (close_brace_leading, _) = printer.trivia.get_for_range_split(printer.range(close));
        printer.print_trivia_with_newline(close_brace_leading.trim_trailing_blanks(), inner_indent);
        printer.print_spaces(shape.indent);
        printer.print_token(close);
        PrintInfo::default_multi_lined()
    }
}

impl Printable for AnyEnumMember {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        match self {
            AnyEnumMember::Variant(variant) => {
                let info = printer.print(variant, shape);
                if let Some(sep) = printer.tree.get(variant).sep {
                    let (leading, _) = printer.trivia.get_for_range_split(printer.range(sep));
                    printer.print_trivia_squished(leading);
                }
                printer.print_str(",");
                info
            }
            AnyEnumMember::BlockAttr(attr) => printer.print(attr, shape),
            AnyEnumMember::Bogus(_) => unreachable!("the formatter prints only error-free trees"),
        }
    }
}

impl PrintMultiLine for Id<EnumVariant> {
    fn print_multi_line(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let tree = printer.tree;
        let node = tree.get(self);
        let attrs = tree.list(node.attrs);
        print_docs(node.docs, shape.indent, printer);
        printer.print(node.name, shape);
        if attrs.is_empty() {
            return PrintInfo::default_single_line();
        }
        printer.print_trivia_all_trailing_for(node.name.rightmost_token(tree));
        let attr_shape = Shape::standalone(
            printer.config.line_width,
            shape.indent + printer.config.indent_width,
        );
        for (i, attr) in attrs.iter().enumerate() {
            let (attr_leading, attr_trailing) = printer.trivia.get_for_element(tree, *attr);
            printer.print_newline();
            printer.print_trivia_with_newline(attr_leading.trim_blanks(), attr_shape.indent);
            printer.print_spaces(attr_shape.indent);
            printer.print(*attr, attr_shape);
            if i + 1 < attrs.len() {
                printer.print_trivia_trailing(attr_trailing);
            }
        }
        PrintInfo::default_multi_lined()
    }
}

fn enum_variant_single_line(
    id: Id<EnumVariant>,
    shape: &Shape,
    printer: &mut Printer,
) -> Option<PrintInfo> {
    let tree = printer.tree;
    let node = tree.get(id);
    let attrs = tree.list(node.attrs);
    print_docs(node.docs, shape.indent, printer);
    // The docs are lines of their own: the width check is the variant's line.
    let line_start = printer.len();
    printer.print(node.name, *shape);
    let (_, name_trailing) = printer
        .trivia
        .get_for_range_split(node.name.rightmost_token(tree));
    printer.try_print_trivia_single_line_squished(name_trailing)?;
    for (i, attr) in attrs.iter().enumerate() {
        printer.print_spaces(1);
        let (attr_leading, attr_trailing) = printer.trivia.get_for_element(tree, *attr);
        printer.try_print_trivia_single_line_squished(attr_leading)?;
        if printer
            .print(*attr, Shape::unlimited_single_line())
            .multi_lined
        {
            return None;
        }
        if i + 1 < attrs.len() {
            printer.try_print_trivia_single_line_squished(attr_trailing)?;
        }
    }
    if printer.len() - line_start > shape.width {
        None
    } else {
        Some(PrintInfo::default_single_line())
    }
}

impl Printable for Id<EnumVariant> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        printer
            .try_sub_printer(|p| enum_variant_single_line(self, &shape, p))
            .unwrap_or_else(|| self.print_multi_line(shape, printer))
    }
}

impl Printable for Id<TestDef> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let tree = printer.tree;
        let node = tree.get(self);
        print_docs(node.docs, shape.indent, printer);
        printer.print_token(node.kw);
        printer.print_str(" ");
        printer.print(present(node.name), shape);
        if let Some(with) = node.with {
            let with = tree.get(with);
            printer.print_str(" ");
            printer.print_token(with.kw);
            printer.print_str(" ");
            let (_, kw_trailing) = printer.trivia.get_for_range_split(printer.range(with.kw));
            print_trivia_squished_spaced(printer, kw_trailing, false, true);
            // A comment on either side of an item or after a comma stays
            // where it was written, spaced; the comma itself is canonical.
            let mut after_comma = None;
            for (i, SepItem { item, sep }) in tree.list(with.items).iter().enumerate() {
                if i > 0 {
                    printer.print_str(", ");
                    if let Some(comma_trailing) = after_comma.take() {
                        print_trivia_squished_spaced(printer, comma_trailing, false, true);
                    }
                }
                let (leading, trailing) = printer.trivia.get_for_element(tree, *item);
                print_trivia_squished_spaced(printer, leading, false, true);
                printer.print(*item, shape);
                print_trivia_squished_spaced(printer, trailing, true, false);
                after_comma =
                    sep.map(|comma| printer.trivia.get_for_range_split(printer.range(comma)).1);
            }
            if let Some(comma_trailing) = after_comma {
                // A trailing comma's comment, before the body.
                print_trivia_squished_spaced(printer, comma_trailing, true, false);
            }
        }
        printer.print_str(" ");
        printer.print(present(node.body), shape)
    }
}

impl Printable for Id<TypeAliasDef> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let tree = printer.tree;
        let node = tree.get(self);
        let name = present(node.name);
        let eq = present(node.eq);
        let type_expr = present(node.ty);
        print_docs(node.docs, shape.indent, printer);
        printer.print_token(node.kw);
        printer.print_str(" ");
        printer.print(name, shape);
        printer.print_str(" ");
        printer.print_token(eq);
        printer.print_str(" ");
        let (_, eq_trailing) = printer.trivia.get_for_range_split(printer.range(eq));
        let (ty_leading, ty_trailing) = printer.trivia.get_for_element(tree, type_expr);
        let mut ty_leading_len = printer.print_trivia_squished(eq_trailing);
        ty_leading_len += printer.print_trivia_squished(ty_leading);
        let new_offset =
            usize::from(printer.range(node.kw).len() + name.rightmost_token(tree).len())
                + const { "  = ".len() }
                + ty_leading_len;
        let info;
        if let Some(semicolon) = node.semi {
            let (semicolon_leading, _) =
                printer.trivia.get_for_range_split(printer.range(semicolon));
            let mut ty_trailing_len = ty_trailing.squished_len(printer.input);
            ty_trailing_len += semicolon_leading.squished_len(printer.input);
            let ty_shape = Shape {
                width: shape
                    .width
                    .saturating_sub(new_offset + ty_trailing_len + const { ";".len() }),
                indent: shape.indent,
                first_line_offset: shape.first_line_offset + new_offset,
            };
            info = printer.print(type_expr, ty_shape);
            printer.print_trivia_squished(ty_trailing);
            printer.print_trivia_squished(semicolon_leading);
            printer.print_token(semicolon);
        } else {
            let ty_shape = Shape {
                width: shape.width.saturating_sub(new_offset + const { ";".len() }),
                indent: shape.indent,
                first_line_offset: shape.first_line_offset + new_offset,
            };
            info = printer.print(type_expr, ty_shape);
            printer.print_str(";");
        }
        info
    }
}
