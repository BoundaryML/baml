//! `receiver.<here>` — the members of the value to the left of the dot.
//!
//! Nothing here searches for a member by name or walks an interface graph:
//! [`baml_compiler2_hir_ty::ide::members_for_receiver`] enumerates through
//! the same ladder resolution uses, so this module's whole job is to render
//! what the compiler already decided is reachable.

use baml_base::SourceFile;
use baml_compiler2_hir_ty::method_resolution::{MemberDecl, MemberSource};
use text_size::TextSize;

use super::{
    context::CompletionContext,
    item::{Completion, CompletionInsert, CompletionKind, CompletionRelevance},
};
use crate::{info, render, resolve};

pub(crate) fn complete_members(
    db: &dyn baml_compiler2_ppir::Db,
    file: SourceFile,
    dot: TextSize,
    context: &CompletionContext,
    out: &mut Vec<Completion>,
) {
    let Some((owner, receiver)) = resolve::receiver_at_dot(db, file, dot) else {
        return;
    };
    for candidate in baml_compiler2_hir_ty::ide::members_for_receiver(db, owner, &receiver) {
        let (detail, documentation) = describe_member(db, file, &candidate.decl);
        out.push(Completion {
            label: candidate.name.as_str().to_string(),
            source_range: context.source_range,
            // A method's parentheses are part of writing the call; the tab
            // stop lands inside them so the next keystroke is the argument.
            insert: if candidate.is_method {
                CompletionInsert::Snippet(format!("{}($0)", candidate.name.as_str()))
            } else {
                CompletionInsert::Plain(candidate.name.as_str().to_string())
            },
            kind: if candidate.is_method {
                CompletionKind::Method
            } else {
                CompletionKind::Field
            },
            detail,
            documentation,
            relevance: CompletionRelevance {
                is_inherent: matches!(candidate.source, MemberSource::Inherent),
            },
        });
    }
}

/// The right-hand column and the tooltip: rendered from the DECLARATION the
/// enumeration handed back, through the same signature engine hover uses, so
/// a member reads identically in both surfaces.
fn describe_member(
    db: &dyn baml_compiler2_ppir::Db,
    file: SourceFile,
    decl: &MemberDecl<'_>,
) -> (Option<String>, Option<String>) {
    match decl {
        MemberDecl::Method(function) => {
            let data = baml_compiler2_ppir::item_data::function_data(db, *function);
            let signature = info::resolved_function_sig_parts(db, *function, None).render(
                db,
                file,
                info::method_sig_style(),
            );
            (Some(signature), data.docstring.clone())
        }
        MemberDecl::ClassField { class, index } => {
            let data = baml_compiler2_ppir::item_data::class_data(db, *class);
            let ty = baml_compiler2_hir_ty::lower::resolve_class_fields(db, *class)
                .get(*index)
                .map(|(_, ty, _)| render::display_ty_canonical_for_file(db, file, ty));
            (
                ty,
                data.fields
                    .get(*index)
                    .and_then(|field| field.docstring.clone()),
            )
        }
        MemberDecl::InterfaceField { interface, index } => {
            let data = baml_compiler2_ppir::item_data::interface_data(db, *interface);
            (
                None,
                data.fields
                    .get(*index)
                    .and_then(|field| field.docstring.clone()),
            )
        }
        // A mounted package exports rows, not declarations: the name is all
        // there is to show until the row itself is threaded through.
        MemberDecl::Mounted => (None, None),
    }
}
