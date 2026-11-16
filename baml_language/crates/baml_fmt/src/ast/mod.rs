//! Printing the typed tree: every node kind prints itself through the
//! printer, with its trivia emitted by the parent. Spans come from the tree
//! the printer holds. The formatter prints only error-free trees, so a
//! `Missing` or `Bogus` is never met.

mod attributes;
mod declarations;
mod expressions;
mod interfaces;
mod pattern;
mod statements;
mod tokens;
mod types;

use baml_syntax::{
    nodes::{
        AnyArrayPatternElement, AnyAttrArg, AnyClassMember, AnyDoc, AnyElse, AnyEnumMember,
        AnyExpr, AnyForHeader, AnyForInit, AnyForStep, AnyFunctionBody, AnyImplMember,
        AnyInterfaceMember, AnyItem, AnyLlmField, AnyMatchScrutinee, AnyObjectCtor, AnyObjectKey,
        AnyObjectMember, AnyPattern, AnyStmt, AnyStringLike, AnyType, AnyTypeArg, FileTree,
        SourceFile,
    },
    tree::{AnyNode as _, Id, Missing, TokenRange},
};
use text_size::TextRange;

use crate::{
    printer::{PrintInfo, Printable, Printer, Shape, Spanned},
    trivia_classifier::TriviaSliceExt as _,
};

/// A required part of an error-free tree.
pub(crate) fn present<T>(part: Result<T, Missing>) -> T {
    part.unwrap_or_else(|_| unreachable!("the formatter prints only error-free trees"))
}

/// `Spanned` for a union whose alternatives are all nodes.
macro_rules! union_spanned {
    ($($Union:ident),+ $(,)?) => {
        $(impl Spanned for $Union {
            fn tokens(self, tree: &FileTree) -> TokenRange {
                let id = self
                    .any_node_id()
                    .unwrap_or_else(|| unreachable!("every alternative of the union is a node"));
                tree.arenas().span(id)
            }
        })+
    };
}

/// `Printable` for a union: each alternative prints as itself; `Bogus` is
/// never met in an error-free tree.
macro_rules! union_printable {
    ($Union:ident { $($Variant:ident),+ $(,)? }) => {
        impl $crate::printer::Printable for $Union {
            fn print(
                self,
                shape: $crate::printer::Shape,
                printer: &mut $crate::printer::Printer,
            ) -> $crate::printer::PrintInfo {
                match self {
                    $(Self::$Variant(value) => printer.print(value, shape),)+
                    Self::Bogus(_) => unreachable!("the formatter prints only error-free trees"),
                }
            }
        }
    };
}
pub(crate) use union_printable;

union_spanned!(
    AnyItem,
    AnyFunctionBody,
    AnyLlmField,
    AnyClassMember,
    AnyEnumMember,
    AnyInterfaceMember,
    AnyImplMember,
    AnyDoc,
    AnyAttrArg,
    AnyType,
    AnyTypeArg,
    AnyPattern,
    AnyArrayPatternElement,
    AnyExpr,
    AnyElse,
    AnyMatchScrutinee,
    AnyObjectCtor,
    AnyObjectMember,
    AnyObjectKey,
    AnyStringLike,
    AnyStmt,
    AnyForHeader,
    AnyForInit,
    AnyForStep,
);

impl Printable for Id<SourceFile> {
    fn print(self, shape: Shape, printer: &mut Printer) -> PrintInfo {
        let node = printer.tree.get(self);
        assert_eq!(shape.indent, 0);
        assert_eq!(shape.first_line_offset, 0);
        assert_eq!(shape.width, printer.config.line_width);

        let items = printer.tree.list(node.items);
        for (idx, &decl) in items.iter().enumerate() {
            // Items are separated by a blank line, except that a run of
            // global variables keeps the grouping it was written with:
            // adjacent globals stay adjacent, and a blank line the author put
            // between two of them stays (collapsed to one).
            let grouped_with_previous = idx > 0
                && matches!(decl, AnyItem::Let(_))
                && matches!(items[idx - 1], AnyItem::Let(_))
                && !has_blank_line(
                    &printer.input[TextRange::new(
                        items[idx - 1].rightmost_token(printer.tree).end(),
                        decl.leftmost_token(printer.tree).start(),
                    )],
                );
            if idx > 0 && !grouped_with_previous {
                printer.print_newline();
            }

            let (leading_trivia, trailing_trivia) =
                printer.trivia.get_for_element(printer.tree, decl);
            printer.print_trivia_with_newline(leading_trivia.trim_leading_blanks(), 0);
            printer.print(decl, shape);
            printer.print_trivia_trailing(trailing_trivia);
            printer.print_newline();
        }
        for trivia in printer.trivia.get_for_eof() {
            printer.print_trivia(trivia);
            printer.print_newline();
        }

        PrintInfo::default_multi_lined()
    }
}

/// Whether `gap` — the source between two items, comments included —
/// contains an empty line.
pub(super) fn has_blank_line(gap: &str) -> bool {
    let lines: Vec<&str> = gap.split('\n').collect();
    lines.len() > 2
        && lines[1..lines.len() - 1]
            .iter()
            .any(|line| line.trim().is_empty())
}
