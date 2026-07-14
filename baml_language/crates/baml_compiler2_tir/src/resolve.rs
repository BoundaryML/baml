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
        for binding in bindings.bindings.iter().rev() {
            if &binding.name == name && index.binding_visible_at(binding, at_offset) {
                return ResolvedName::Local {
                    name: name.clone(),
                    definition_site: Some(binding.site),
                };
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

        // At File/Package scope, resolve through PRC
        if matches!(scope.kind, ScopeKind::File | ScopeKind::Package) {
            let pkg_info = baml_compiler2_hir::file_package::file_package(db, file);
            let pkg_id = PackageId::new(db, pkg_info.package.clone());
            let res_ctx = crate::package_interface::package_resolution_context(db, pkg_id);

            let name_path = std::slice::from_ref(name);
            if let Some((source, def)) =
                res_ctx.resolve_value(db, name_path, &pkg_info.namespace_path)
            {
                return match source {
                    crate::package_interface::ResolvedSource::Item => ResolvedName::Item(def),
                    crate::package_interface::ResolvedSource::Builtin => ResolvedName::Builtin(def),
                };
            }
            // For types, resolve_type returns Ty — we need Definition, so fall back to direct lookup
            if let Some((_source, _ty)) =
                res_ctx.resolve_type(db, name_path, &pkg_info.namespace_path)
            {
                let pkg_items = &res_ctx.own_items;
                if let Some(def) = pkg_items.lookup_type(&pkg_info.namespace_path, name) {
                    return ResolvedName::Item(def);
                }
                // The type was found in deps — search deps.
                // Dep builtins are in the root namespace (&[]).
                for (dep_name, _) in &res_ctx.dep_interfaces {
                    if let Some(dep_items) = res_ctx.items_for_package(db, dep_name) {
                        if let Some(def) = dep_items
                            .lookup_type(&pkg_info.namespace_path, name)
                            .or_else(|| dep_items.lookup_type(&[], name))
                        {
                            return ResolvedName::Builtin(def);
                        }
                    }
                }
            }
        }
    }

    ResolvedName::Unknown
}

/// Resolve a path expression at a given position.
///
/// Single-segment paths are resolved via `resolve_name_at`.
/// Multi-segment paths are resolved by treating the first segment as either:
/// - `root` — substituted with the current file's package
/// - a literal package name (e.g. `baml`)
///   The remaining segments are looked up inside that package.
pub fn resolve_path_at<'db>(
    db: &'db dyn crate::Db,
    file: SourceFile,
    at_offset: TextSize,
    segments: &[Name],
    scope_name: Option<&Name>,
) -> ResolvedName<'db> {
    if segments.is_empty() {
        return ResolvedName::Unknown;
    }

    if segments.len() == 1 {
        return resolve_name_at_in_scope(db, file, at_offset, &segments[0], scope_name);
    }

    // First segment: `root` maps to the current file's package,
    // anything else is a literal package name.
    let pkg_info = baml_compiler2_hir::file_package::file_package(db, file);
    let pkg_name = if segments[0].as_str() == "root" {
        pkg_info.package.clone()
    } else {
        segments[0].clone()
    };

    // Use PackageResolutionContext to validate access to the target package.
    let own_pkg_id = PackageId::new(db, pkg_info.package);
    let res_ctx = crate::package_interface::package_resolution_context(db, own_pkg_id);

    let Some(pkg_items) = res_ctx.items_for_package(db, &pkg_name) else {
        return ResolvedName::Unknown;
    };

    let after_pkg = &segments[1..];
    // The path already includes namespace segments, so look up directly.
    let item = after_pkg
        .last()
        .expect("multi-segment path has elements after pkg prefix");
    let ns = &after_pkg[..after_pkg.len() - 1];
    if let Some(def) = pkg_items.lookup_value(ns, item) {
        return ResolvedName::Builtin(def);
    }
    if let Some(def) = pkg_items.lookup_type(ns, item) {
        return ResolvedName::Builtin(def);
    }

    ResolvedName::Unknown
}

/// Resolve a path *prefix* that names a namespace — a package (`baml`, `root`)
/// and/or a namespace within it (`baml.json`, `root.llm`).
///
/// Unlike item resolution this answers "is this prefix a real namespace at all",
/// so a prefix that names nothing (a typo) returns `None` and can stay neutral
/// instead of being painted as a bogus namespace. `Some(is_builtin)` reports
/// whether the namespace lives in a builtin / dependency package (`baml`, ...)
/// rather than the file's own package, so the caller can mark it `defaultLibrary`.
pub fn resolve_namespace_prefix(
    db: &dyn crate::Db,
    file: SourceFile,
    segments: &[Name],
) -> Option<bool> {
    let first = segments.first()?;
    let pkg_info = baml_compiler2_hir::file_package::file_package(db, file);
    let own_pkg_id = PackageId::new(db, pkg_info.package);
    let res_ctx = crate::package_interface::package_resolution_context(db, own_pkg_id);

    // Segment 0 is a package: `root` is the file's own package; anything else is
    // a literal package name.
    let pkg_name = if first.as_str() == "root" {
        res_ctx.own_package_name.clone()
    } else {
        first.clone()
    };
    let pkg_items = res_ctx.items_for_package(db, &pkg_name)?;

    // The remaining segments must be a (possibly empty) prefix of a real
    // namespace path in that package. Empty = the package root itself.
    let ns_prefix = &segments[1..];
    let is_namespace = ns_prefix.is_empty()
        || pkg_items
            .namespaces
            .keys()
            .any(|k| k.starts_with(ns_prefix));
    if !is_namespace {
        return None;
    }

    Some(pkg_name.as_str() != res_ctx.own_package_name.as_str())
}

/// Resolve `Enum.Variant` — the leaf of an enum-rooted type path, e.g. the
/// `North` in a match pattern `Direction.North` (a type expression).
///
/// Goes through the same package-resolution system as item/namespace lookup:
/// `resolve_path_at` resolves the prefix (local or `baml`/dep, possibly
/// namespaced) to an enum, then the leaf is verified against that enum's actual
/// variants. Returns `true` only for a real variant, so a typo'd `Enum.Nope`
/// stays a plain type rather than a bogus `enumMember`.
pub fn resolve_enum_variant(
    db: &dyn crate::Db,
    file: SourceFile,
    at_offset: TextSize,
    segments: &[Name],
) -> bool {
    let Some((variant, prefix)) = segments.split_last() else {
        return false;
    };
    if prefix.is_empty() {
        return false;
    }
    let (ResolvedName::Item(def) | ResolvedName::Builtin(def)) =
        resolve_path_at(db, file, at_offset, prefix, None)
    else {
        return false;
    };
    let Definition::Enum(enum_loc) = def else {
        return false;
    };
    let item_tree = baml_compiler2_ppir::file_item_tree(db, enum_loc.file(db));
    item_tree[enum_loc.id(db)]
        .variants
        .iter()
        .any(|v| v.name == *variant)
}

/// Resolve a field `field` of the type named by `type_segments` (a class or
/// interface), through the same package-resolution system as item/namespace/
/// variant lookup. Returns `true` only when the field actually exists on that
/// type — used to classify the two sides of an interface field link
/// (`name as display_name`) as `property` only when they resolve.
pub fn resolve_field(
    db: &dyn crate::Db,
    file: SourceFile,
    at_offset: TextSize,
    type_segments: &[Name],
    field: &Name,
) -> bool {
    if type_segments.is_empty() {
        return false;
    }
    let (ResolvedName::Item(def) | ResolvedName::Builtin(def)) =
        resolve_path_at(db, file, at_offset, type_segments, None)
    else {
        return false;
    };
    match def {
        Definition::Class(loc) => {
            let item_tree = baml_compiler2_ppir::file_item_tree(db, loc.file(db));
            item_tree[loc.id(db)]
                .fields
                .iter()
                .any(|f| f.name == *field)
        }
        Definition::Interface(loc) => {
            let item_tree = baml_compiler2_ppir::file_item_tree(db, loc.file(db));
            item_tree[loc.id(db)]
                .fields
                .iter()
                .any(|f| f.name == *field)
        }
        _ => false,
    }
}
