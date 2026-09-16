//! What follows a dot, per [`DotTarget`] arm.
//!
//! A VALUE offers its INSTANCE members — the ones with a `self` receiver,
//! since that receiver is the value already written. A TYPE offers
//! everything it declares: statics, and instance methods in their UFCS form
//! (`int.min(a, b)` is the call `a.min(b)`). A NAMESPACE offers its items
//! and child namespaces. All three are compiler enumerations the context
//! already resolved; what remains here is each arm's own filter — which
//! names a reader can actually write at this position.

use baml_compiler2_hir::contributions::Definition;
use baml_compiler2_hir_ty::method_resolution::MemberDecl;
use baml_compiler2_ppir::resolve::NamespaceMemberKind;

use super::{
    completions::Completions,
    context::{DotTarget, PathKind},
    render::MemberForm,
};
use crate::symbols;

pub(crate) fn complete(
    db: &dyn baml_compiler2_ppir::Db,
    target: &DotTarget<'_>,
    kind: PathKind,
    out: &mut Completions<'_>,
) {
    match target {
        DotTarget::Value { owner, receiver } => {
            // A value's members are not types; a value receiver cannot even
            // occur syntactically to the left of a type-position dot.
            if kind == PathKind::Type {
                return;
            }
            for candidate in baml_compiler2_hir_ty::ide::members_for_receiver(db, *owner, receiver)
            {
                // A static is reached through the TYPE. Offering it here
                // would suggest a call the checker rejects.
                if candidate.is_static {
                    continue;
                }
                out.add_member(&candidate, MemberForm::Instance);
            }
        }
        DotTarget::Type(definition) => {
            for candidate in baml_compiler2_hir_ty::ide::members_for_type(db, *definition) {
                // In type position only a member that IS a type resolves:
                // an enum's variants (`Status.Active` as a type pattern).
                // Methods are call syntax, not types.
                if kind == PathKind::Type
                    && !matches!(candidate.decl, MemberDecl::EnumVariant { .. })
                {
                    continue;
                }
                out.add_member(&candidate, MemberForm::Qualified);
            }
        }
        DotTarget::Namespace(members) => {
            for member in members {
                if let NamespaceMemberKind::Item(def) = &member.kind {
                    if !symbols::offered_in_completion(db, &member.name, *def) {
                        continue;
                    }
                    // A type position reaches the namespace's TYPES; its
                    // values (functions, lets, clients) do not resolve there.
                    if kind == PathKind::Type && !is_type_definition(*def) {
                        continue;
                    }
                }
                out.add_namespace_member(member);
            }
        }
    }
}

/// Whether a definition can be written where a type is expected.
fn is_type_definition(def: Definition<'_>) -> bool {
    use baml_compiler2_hir::contributions::DefinitionKind;
    matches!(
        def.kind(),
        DefinitionKind::Class
            | DefinitionKind::Enum
            | DefinitionKind::Interface
            | DefinitionKind::TypeAlias
    )
}
