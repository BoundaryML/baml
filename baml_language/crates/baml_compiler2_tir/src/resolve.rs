//! On-demand name resolution.
//!
//! `resolve_name_at` walks the scope chain upward from a given offset,
//! checking `ScopeBindings` at each level, then falls through to
//! `package_items` for top-level names.
//!
//! This is the Ty-style approach: no pre-built resolution map. Each call
//! re-derives the answer from the scope tree (Salsa-cached via
//! `file_semantic_index`).

use baml_base::{Name, SourceFile};
use baml_compiler2_hir::{
    contributions::Definition, package::PackageId, semantic_index::DefinitionSite,
};
use text_size::TextSize;

/// What a name resolves to — produced on demand, NOT stored in a map.
///
/// Resolution order (innermost scope first):
/// 1. Let-bindings in the current scope (`ScopeBindings::bindings`)
/// 2. Parameters of the enclosing Function/Lambda scope (`ScopeBindings::params`)
/// 3. Walk ancestor scopes repeating 1-2
/// 4. Package-level names in the file's own namespace via `package_items`
/// 5. Builtin package names (`baml`)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedName<'db> {
    /// Local variable (let binding or parameter).
    Local {
        name: Name,
        definition_site: Option<DefinitionSite>,
    },
    /// A top-level item from `package_items`.
    Item(Definition<'db>),
    /// A builtin function/type from the `baml` package.
    Builtin(Definition<'db>),
    /// Could not resolve.
    Unknown,
}

/// Resolve a name at a given position within a file.
///
/// Walks the scope chain upward from the innermost scope containing
/// `at_offset`, checking `ScopeBindings` at each level, then falls
/// through to `package_items` for top-level names.
pub fn resolve_name_at<'db>(
    db: &'db dyn crate::Db,
    file: SourceFile,
    at_offset: TextSize,
    name: &Name,
) -> ResolvedName<'db> {
    resolve_name_at_in_scope(db, file, at_offset, name, None)
}

/// Like `resolve_name_at`, but disambiguates companion functions that share
/// the same span by requiring the scope name to match `scope_name`.
pub fn resolve_name_at_in_scope<'db>(
    db: &'db dyn crate::Db,
    file: SourceFile,
    at_offset: TextSize,
    name: &Name,
    scope_name: Option<&Name>,
) -> ResolvedName<'db> {
    let index = baml_compiler2_ppir::file_semantic_index(db, file);
    let scope_id = index.scope_at_offset(at_offset, scope_name);

    // Resolve locals (let-bindings + parameters) via the shared visibility walk.
    // This scans all ancestor scopes (innermost first, skipping nested class
    // scopes, honoring `visible_from`) before falling through to PRC.
    if let Some(bid) = index.visible_binding_at(scope_id, at_offset, name) {
        return ResolvedName::Local {
            name: name.clone(),
            definition_site: index.binding_site(bid),
        };
    }

    // No local match — resolve File/Package-level names through PRC. This is
    // independent of `scope_id` (it only depends on the file's namespace path).
    let pkg_info = baml_compiler2_hir::file_package::file_package(db, file);
    let pkg_id = PackageId::new(db, pkg_info.package.clone());
    let res_ctx = crate::package_interface::package_resolution_context(db, pkg_id);

    let name_path = std::slice::from_ref(name);
    if let Some((source, def)) = res_ctx.resolve_value(db, name_path, &pkg_info.namespace_path) {
        return match source {
            crate::package_interface::ResolvedSource::Item => ResolvedName::Item(def),
            crate::package_interface::ResolvedSource::Builtin => ResolvedName::Builtin(def),
        };
    }
    // For types, resolve_type returns Ty — we need Definition, so fall back to direct
    // lookup. resolve_type's success is only a guard; re-derive the Definition by
    // walking own items (Item) then deps (Builtin, also probing the root namespace).
    if res_ctx
        .resolve_type(db, name_path, &pkg_info.namespace_path)
        .is_some()
    {
        if let Some(def) = res_ctx
            .own_items
            .lookup_type(&pkg_info.namespace_path, name)
        {
            return ResolvedName::Item(def);
        }
        for (dep_name, _) in &res_ctx.dep_interfaces {
            if let Some(def) = res_ctx.items_for_package(db, dep_name).and_then(|dep| {
                dep.lookup_type(&pkg_info.namespace_path, name)
                    .or_else(|| dep.lookup_type(&[], name))
            }) {
                return ResolvedName::Builtin(def);
            }
        }
    }

    ResolvedName::Unknown
}
