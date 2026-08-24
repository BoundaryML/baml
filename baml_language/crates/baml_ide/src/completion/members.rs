//! What follows a dot, per [`DotTarget`] arm.
//!
//! A VALUE offers its INSTANCE members — the ones with a `self` receiver,
//! since that receiver is the value already written. A TYPE offers
//! everything it declares: statics, and instance methods in their UFCS form
//! (`int.min(a, b)` is the call `a.min(b)`). A NAMESPACE offers its items
//! and child namespaces. All three are compiler enumerations the context
//! already resolved; what remains here is each arm's own filter — which
//! names a reader can actually write at this position.

use baml_base::SourceFile;
use baml_compiler2_hir::contributions::Definition;
use baml_compiler2_ppir::resolve::NamespaceMemberKind;

use super::{completions::Completions, context::DotTarget, render::MemberForm};
use crate::symbols;

pub(crate) fn complete(
    db: &dyn baml_compiler2_ppir::Db,
    file: SourceFile,
    target: &DotTarget<'_>,
    out: &mut Completions,
) {
    match target {
        DotTarget::Value { owner, receiver } => {
            for candidate in baml_compiler2_hir_ty::ide::members_for_receiver(db, *owner, receiver)
            {
                // A static is reached through the TYPE. Offering it here
                // would suggest a call the checker rejects.
                if candidate.is_static {
                    continue;
                }
                out.add_member(db, file, &candidate, MemberForm::Instance);
            }
        }
        DotTarget::Type(definition) => {
            for candidate in baml_compiler2_hir_ty::ide::members_for_type(db, *definition) {
                out.add_member(db, file, &candidate, MemberForm::Qualified);
            }
        }
        DotTarget::Namespace(members) => {
            for member in members {
                if let NamespaceMemberKind::Item(def) = &member.kind
                    && (symbols::is_synthesized(db, &member.name, *def)
                        || is_builtin_companion(db, *def))
                {
                    continue;
                }
                out.add_namespace_member(db, file, member);
            }
        }
    }
}

/// Whether a definition is a COMPANION CARRIER — the class a builtin's
/// methods are declared on, whose written spelling is the builtin itself.
///
/// `baml.Int` is where `int`'s methods live and `int` is how it is written;
/// likewise `baml.Array<T>.item` reads `T[].item` and `baml.Map<K, V>.item`
/// reads `map<K, V>.item`. Offering the carrier under its package path would
/// teach a spelling nobody uses, so `baml.` lists neither it nor its
/// siblings. The set is the language's own
/// ([`builtin_companion_of`](baml_type::type_kind::builtin_companion_of)),
/// not a list kept here.
fn is_builtin_companion(db: &dyn baml_compiler2_ppir::Db, def: Definition<'_>) -> bool {
    let Definition::Class(class) = def else {
        return false;
    };
    let data = baml_compiler2_ppir::item_data::class_data(db, class);
    let pkg = baml_compiler2_hir::file_package::file_package(db, class.file(db));
    let qtn = baml_type::QualifiedTypeName::new(pkg.package, pkg.namespace_path, data.name.clone());
    baml_type::type_kind::builtin_companion_of(&qtn).is_some()
}
