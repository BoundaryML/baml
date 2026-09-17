//! Rendering a completion's columns (rust-analyzer's `render/`): the detail
//! string and the documentation for each shape of thing offered.
//!
//! This is the completion-side face of the crate-level type renderer
//! ([`crate::render`]) and the signature engine ([`crate::info`]): a member
//! here reads identically to the same member on hover, because both go
//! through `resolved_function_sig_parts`.

use baml_base::SourceFile;
use baml_compiler2_hir::contributions::Definition;
use baml_compiler2_hir_ty::method_resolution::MemberDecl;

use crate::info;

/// How a member is being reached, which decides how its signature reads.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum MemberForm {
    /// Through a value: the `self` receiver is the value already written,
    /// so the signature hides it.
    Instance,
    /// Through the type (UFCS): every parameter is passed, `self` included,
    /// so the signature shows it.
    Qualified,
}

/// The right-hand column and the tooltip for a member, rendered from the
/// DECLARATION the enumeration handed back.
pub(super) fn member(
    db: &dyn baml_compiler2_ppir::Db,
    file: SourceFile,
    decl: &MemberDecl<'_>,
    form: MemberForm,
) -> (Option<String>, Option<String>) {
    use baml_compiler2_hir::loc::DeclRef;
    use baml_compiler2_hir_ty::extern_loc::{extern_class_row, extern_function_row};

    let style = match form {
        MemberForm::Instance => info::instance_completion_sig_style(),
        MemberForm::Qualified => info::method_sig_style(),
    };
    match decl {
        MemberDecl::Method(DeclRef::Source(function)) => {
            let data = baml_compiler2_ppir::item_data::function_data(db, *function);
            let signature =
                info::resolved_function_sig_parts(db, *function, None).render(db, file, style);
            (Some(signature), data.docstring.clone())
        }
        // A served package's row: its resolved signature, exactly as hover
        // renders it. A callable row carries no docstring.
        MemberDecl::Method(DeclRef::External(function)) => {
            let signature =
                crate::render::FnSigParts::of_exported(extern_function_row(db, *function))
                    .render(db, file, style);
            (Some(signature), None)
        }
        MemberDecl::EnumVariant {
            enum_loc: DeclRef::Source(enum_loc),
            index,
        } => {
            let data = baml_compiler2_ppir::item_data::enum_data(db, *enum_loc);
            (
                Some(data.name.as_str().to_string()),
                data.variants
                    .get(*index)
                    .and_then(|variant| variant.docstring.clone()),
            )
        }
        MemberDecl::EnumVariant {
            enum_loc: DeclRef::External(enum_loc),
            ..
        } => (Some(enum_loc.head(db).name().as_str().to_string()), None),
        MemberDecl::ClassField {
            class: DeclRef::Source(class),
            index,
        } => {
            let data = baml_compiler2_ppir::item_data::class_data(db, *class);
            let ty = baml_compiler2_hir_ty::lower::resolve_class_fields(db, *class)
                .get(*index)
                .map(|(_, ty, _)| crate::render::display_ty_canonical_for_file(db, file, ty));
            (
                ty,
                data.fields
                    .get(*index)
                    .and_then(|field| field.docstring.clone()),
            )
        }
        // A field row carries its declaration's docstring (`ExportedFieldAttrs`).
        MemberDecl::ClassField {
            class: DeclRef::External(class),
            index,
        } => {
            let field = extern_class_row(db, *class).fields.get(*index);
            (
                field.map(|(_, ty, _)| crate::render::display_ty_canonical_for_file(db, file, ty)),
                field.and_then(|(_, _, attrs)| attrs.docstring.clone()),
            )
        }
        MemberDecl::InterfaceField {
            interface: DeclRef::Source(interface),
            index,
        } => {
            let data = baml_compiler2_ppir::item_data::interface_data(db, *interface);
            (
                None,
                data.fields
                    .get(*index)
                    .and_then(|field| field.docstring.clone()),
            )
        }
        MemberDecl::InterfaceField {
            interface: DeclRef::External(interface),
            index,
        } => (
            None,
            baml_compiler2_hir_ty::extern_loc::extern_interface_row(db, *interface)
                .fields
                .get(*index)
                .and_then(|(_, _, attrs)| attrs.docstring.clone()),
        ),
    }
}

/// The right-hand column and tooltip for a top-level item — a function's
/// signature comes from the same engine hover uses.
pub(super) fn definition(
    db: &dyn baml_compiler2_ppir::Db,
    file: SourceFile,
    def: &Definition<'_>,
) -> (Option<String>, Option<String>) {
    match def {
        Definition::Function(function) => {
            let data = baml_compiler2_ppir::item_data::function_data(db, *function);
            let signature = info::resolved_function_sig_parts(db, *function, None).render(
                db,
                file,
                info::method_sig_style(),
            );
            (Some(signature), data.docstring.clone())
        }
        other => (Some(other.kind().as_str().to_string()), None),
    }
}
