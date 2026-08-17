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
pub fn resolve_name_at<'db>(
    db: &'db dyn crate::Db,
    file: SourceFile,
    at_offset: TextSize,
    name: &Name,
) -> ResolvedName<'db> {
    resolve_name_at_in_scope(db, file, at_offset, name, None)
}

/// Like [`resolve_name_at`], but disambiguates companion functions that
/// share a span by requiring the scope name to match `scope_name`.
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

/// `PackageItems` for a package accessible from `file`'s own package: the
/// own package itself, or a declared dependency. Undeclared packages are
/// invisible (`None`) - the same access rule the type resolver applies.
fn accessible_package_items<'db>(
    db: &'db dyn crate::Db,
    file: SourceFile,
    pkg_name: &Name,
) -> Option<&'db PackageItems<'db>> {
    let own = baml_compiler2_hir::file_package::file_package(db, file).package;
    let own_id = PackageId::new(db, own.clone());
    if pkg_name.as_str() == own.as_str() {
        return Some(crate::package_items(db, own_id));
    }
    if package_dependencies(db, own_id)
        .iter()
        .any(|dep| dep.name(db).as_str() == pkg_name.as_str())
    {
        let dep_id = PackageId::new(db, pkg_name.clone());
        return Some(crate::package_items(db, dep_id));
    }
    None
}

/// Resolve the package and namespace split for a qualified path.
///
/// A real accessible package wins. If there is none, BEP-066's `reflect`,
/// `type`, and `json` roots are interpreted as namespaces of the accessible
/// builtin `baml` package, matching compiler name resolution and completions.
fn accessible_path_package<'db>(
    db: &'db dyn crate::Db,
    file: SourceFile,
    segments: &[Name],
) -> Option<(Name, &'db PackageItems<'db>, usize)> {
    let first = segments.first()?;
    let own = baml_compiler2_hir::file_package::file_package(db, file).package;
    if first.as_str() == "root" {
        let items = accessible_package_items(db, file, &own)?;
        return Some((own, items, 1));
    }
    if let Some(items) = accessible_package_items(db, file, first) {
        return Some((first.clone(), items, 1));
    }
    if matches!(first.as_str(), "reflect" | "type" | "json") {
        let baml = Name::new("baml");
        let items = accessible_package_items(db, file, &baml)?;
        return Some((baml, items, 0));
    }
    None
}

/// Resolve a path expression at a given position.
///
/// Single-segment paths are resolved via `resolve_name_at`. Multi-segment
/// paths treat the first segment as either `root` (the current file's
/// package), a literal package name, or a BEP-066 builtin namespace shorthand;
/// the remaining segments look up inside that package.
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
    let Some((_pkg_name, pkg_items, namespace_start)) = accessible_path_package(db, file, segments)
    else {
        return ResolvedName::Unknown;
    };
    let after_pkg = &segments[namespace_start..];
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

/// Resolve a path *prefix* that names a namespace - a package (`baml`,
/// `root`) and/or a namespace within it. `Some(is_builtin)` reports whether
/// the namespace lives outside the file's own package; `None` means the
/// prefix names nothing (stays neutral rather than painting a bogus
/// namespace).
pub fn resolve_namespace_prefix(
    db: &dyn crate::Db,
    file: SourceFile,
    segments: &[Name],
) -> Option<bool> {
    let own = baml_compiler2_hir::file_package::file_package(db, file).package;
    let (pkg_name, pkg_items, namespace_start) = accessible_path_package(db, file, segments)?;
    let ns_prefix = &segments[namespace_start..];
    let is_namespace = ns_prefix.is_empty()
        || pkg_items
            .namespaces
            .keys()
            .any(|k| k.starts_with(ns_prefix));
    if !is_namespace {
        return None;
    }
    Some(pkg_name.as_str() != own.as_str())
}

/// Resolve `Enum.Variant` - the leaf of an enum-rooted type path. `true`
/// only for a real variant, so a typo'd `Enum.Nope` stays a plain type.
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
    crate::item_data::enum_data(db, enum_loc)
        .variants
        .iter()
        .any(|v| v.name == *variant)
}

/// Resolve a field of the type named by `type_segments` (class or
/// interface). `true` only when the field exists on that type.
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
        Definition::Class(loc) => crate::item_data::class_data(db, loc)
            .fields
            .iter()
            .any(|f| f.name == *field),
        Definition::Interface(loc) => crate::item_data::interface_data(db, loc)
            .fields
            .iter()
            .any(|f| f.name == *field),
        _ => false,
    }
}
