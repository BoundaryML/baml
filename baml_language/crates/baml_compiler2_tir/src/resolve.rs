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
    contributions::Definition, package::PackageId, scope::ScopeKind, semantic_index::DefinitionSite,
};
use baml_compiler2_ppir::package_items;
use text_size::TextSize;

/// What a name resolves to — produced on demand, NOT stored in a map.
///
/// Resolution order (innermost scope first):
/// 1. Let-bindings in the current scope (`ScopeBindings::bindings`)
/// 2. Parameters of the enclosing Function/Lambda scope (`ScopeBindings::params`)
/// 3. Walk ancestor scopes repeating 1-2
/// 4. Package-level names via `package_items` (functions, classes, enums, type aliases)
/// 5. Builtin package names (`baml`, `env`)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedName<'db> {
    /// Local variable (let binding or parameter).
    Local {
        name: Name,
        definition_site: Option<DefinitionSite>,
    },
    /// A top-level item from `package_items`.
    Item(Definition<'db>),
    /// A builtin function/type from the `baml` or `env` packages.
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
    let index = baml_compiler2_ppir::file_semantic_index(db, file);
    let scope_id = index.scope_at_offset(at_offset);

    // Walk ancestor scopes from innermost to outermost
    for ancestor_id in index.ancestor_scopes(scope_id) {
        let scope = &index.scopes[ancestor_id.index() as usize];

        // Skip class scopes when we're in a nested scope — class body names
        // are not visible to methods/lambdas via bare name lookup.
        // (Field access goes through `resolve_member`, not name resolution.)
        if matches!(scope.kind, ScopeKind::Class) && ancestor_id != scope_id {
            continue;
        }

        let bindings = &index.scope_bindings[ancestor_id.index() as usize];

        // Check let-bindings in this scope (reverse order for shadowing)
        for (binding_name, def_site, binding_range) in bindings.bindings.iter().rev() {
            if binding_name == name {
                // Only visible if the binding precedes the use site
                if binding_range.start() <= at_offset {
                    return ResolvedName::Local {
                        name: name.clone(),
                        definition_site: Some(*def_site),
                    };
                }
            }
        }

        // Check parameters (for Function/Lambda scopes)
        for (param_name, param_idx) in &bindings.params {
            if param_name == name {
                return ResolvedName::Local {
                    name: name.clone(),
                    definition_site: Some(DefinitionSite::Parameter(*param_idx)),
                };
            }
        }

        // At File/Package scope, check package_items
        if matches!(scope.kind, ScopeKind::File | ScopeKind::Package) {
            let pkg_info = baml_compiler2_hir::file_package::file_package(db, file);
            let pkg_id = PackageId::new(db, pkg_info.package.clone());
            let pkg_items = package_items(db, pkg_id);

            // Check value namespace first (functions, template strings)
            if let Some(def) = pkg_items.lookup_value(&[name.clone()]) {
                return ResolvedName::Item(def);
            }
            // Check type namespace (classes, enums, type aliases)
            if let Some(def) = pkg_items.lookup_type(&[name.clone()]) {
                return ResolvedName::Item(def);
            }

            // Check builtin packages (baml, env)
            for builtin_pkg_name in &["baml", "env"] {
                let builtin_pkg_id = PackageId::new(db, Name::new(*builtin_pkg_name));
                let builtin_items = package_items(db, builtin_pkg_id);
                if let Some(def) = builtin_items.lookup_value(&[name.clone()]) {
                    return ResolvedName::Builtin(def);
                }
                if let Some(def) = builtin_items.lookup_type(&[name.clone()]) {
                    return ResolvedName::Builtin(def);
                }
            }
        }
    }

    ResolvedName::Unknown
}

/// Resolve a path expression at a given position.
///
/// After AST lowering, paths are always single-segment (bare identifiers).
/// Multi-segment paths like `Color.Red` are desugared to FieldAccess chains.
pub fn resolve_path_at<'db>(
    db: &'db dyn crate::Db,
    file: SourceFile,
    at_offset: TextSize,
    segments: &[Name],
) -> ResolvedName<'db> {
    if segments.is_empty() {
        return ResolvedName::Unknown;
    }

    debug_assert!(
        segments.len() == 1,
        "multi-segment Path should have been desugared to FieldAccess: {:?}",
        segments
    );

    resolve_name_at(db, file, at_offset, &segments[0])
}
