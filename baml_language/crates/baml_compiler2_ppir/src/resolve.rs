//! On-demand name resolution.
//!
//! `resolve_name_at_in_scope` walks the scope chain upward from a given
//! offset, checking `ScopeBindings` at each level, then falls through to
//! `package_items` for top-level names (own package, `root.*` absolute,
//! then dependency packages).
//!
//! No pre-built resolution map: each call re-derives the answer from the
//! scope tree (Salsa-cached via `file_semantic_index`) - the resolver
//! lives with the scopes it walks, rust-analyzer's `Resolver` discipline.

use baml_base::{Name, SourceFile};
use baml_compiler2_hir::{
    contributions::Definition,
    package::{PackageId, PackageItems, package_dependencies},
    scope::ScopeKind,
    semantic_index::DefinitionSite,
};
use text_size::TextSize;

/// What a name resolves to - produced on demand, NOT stored in a map.
///
/// Resolution order (innermost scope first):
/// 1. Let-bindings in the current scope (`ScopeBindings::bindings`)
/// 2. Parameters of the enclosing Function/Lambda scope (`ScopeBindings::params`)
/// 3. Walk ancestor scopes repeating 1-2
/// 4. Package-level names in the file's own namespace via `package_items`
/// 5. Dependency package names (`baml`)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedName<'db> {
    /// Local variable (let binding or parameter).
    Local {
        name: Name,
        definition_site: Option<DefinitionSite>,
    },
    /// A top-level item from the file's own `package_items`.
    Item(Definition<'db>),
    /// An item from a dependency package (e.g. `baml`).
    Builtin(Definition<'db>),
    /// Could not resolve.
    Unknown,
}

/// Resolve a bare `name` at `at_offset` in `file`, honoring local
/// shadowing (a local wins over any item of the same name).
pub fn resolve_name_at_in_scope<'db>(
    db: &'db dyn crate::Db,
    file: SourceFile,
    at_offset: TextSize,
    name: &Name,
    scope_name: Option<&Name>,
) -> ResolvedName<'db> {
    let index = crate::file_semantic_index(db, file);
    let scope_id = index.scope_at_offset(at_offset, scope_name);

    // Walk ancestor scopes from innermost to outermost.
    for ancestor_id in index.ancestor_scopes(scope_id) {
        let scope = &index.scopes[ancestor_id.index() as usize];

        // Skip class scopes when we're in a nested scope - class body names
        // are not visible to methods/lambdas via bare name lookup.
        // (Field access goes through member resolution, not name resolution.)
        if matches!(scope.kind, ScopeKind::Class) && ancestor_id != scope_id {
            continue;
        }

        let bindings = &index.scope_bindings[ancestor_id.index() as usize];

        // Let-bindings in this scope (reverse order for shadowing).
        for binding in bindings.bindings.iter().rev() {
            if &binding.name == name && index.binding_visible_at(binding, at_offset) {
                return ResolvedName::Local {
                    name: name.clone(),
                    definition_site: Some(binding.site),
                };
            }
        }

        // Parameters (for Function/Lambda scopes).
        for (param_name, param_idx) in &bindings.params {
            if param_name == name {
                return ResolvedName::Local {
                    name: name.clone(),
                    definition_site: Some(DefinitionSite::Parameter(*param_idx)),
                };
            }
        }

        // At File/Package scope, resolve through the package items:
        // own package (Item), then dependency packages (Builtin) - the
        // same own-then-deps order the package resolution context walked.
        if matches!(scope.kind, ScopeKind::File | ScopeKind::Package) {
            let pkg_info = baml_compiler2_hir::file_package::file_package(db, file);
            let pkg_id = PackageId::new(db, pkg_info.package.clone());
            let own_items: &PackageItems<'db> = crate::package_items(db, pkg_id);

            // Values resolve in the OWN package only for bare names
            // (cross-package values require explicit qualification);
            // types fall back through dependencies, dep builtins at the
            // root namespace.
            if let Some(def) = own_items.lookup_value(&pkg_info.namespace_path, name) {
                return ResolvedName::Item(def);
            }
            if let Some(def) = own_items.lookup_type(&pkg_info.namespace_path, name) {
                return ResolvedName::Item(def);
            }
            for &dep_id in package_dependencies(db, pkg_id) {
                let dep_items = crate::package_items(db, dep_id);
                if let Some(def) = dep_items
                    .lookup_type(&pkg_info.namespace_path, name)
                    .or_else(|| dep_items.lookup_type(&[], name))
                {
                    return ResolvedName::Builtin(def);
                }
            }
        }
    }

    ResolvedName::Unknown
}
