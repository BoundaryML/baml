//! Type position: the names that can be written where a type is expected.
//!
//! The names come from [`type_names_in_scope_at`], the enumeration
//! counterpart of the type resolver — generic parameters of the enclosing
//! items, the file's own namespace's types, dependency package names — plus
//! the builtin aliases (`int`, `string`, `json`), which are the language's
//! own table ([`baml_type`]'s), not database state.

use baml_base::SourceFile;
use baml_compiler2_ppir::resolve::type_names_in_scope_at;
use text_size::TextSize;

use super::completions::Completions;
use crate::symbols;

pub(crate) fn complete(
    db: &dyn baml_compiler2_ppir::Db,
    file: SourceFile,
    offset: TextSize,
    out: &mut Completions,
) {
    for entry in type_names_in_scope_at(db, file, offset) {
        if let baml_compiler2_ppir::resolve::TypeScopeNameKind::Item(def) = &entry.kind
            && symbols::is_synthesized(db, &entry.name, *def)
        {
            continue;
        }
        out.add_type_scope_name(db, file, &entry);
    }

    for builtin in baml_type::BuiltinTypeName::all() {
        out.add_builtin_type(builtin.alias());
    }
}
