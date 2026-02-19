//! HIR-level path resolution via scope tree.
//!
//! Resolves multi-segment paths by walking through scopes one segment at a
//! time, similar to rust-analyzer's `DefMap` resolution.
//!
//! # Why a tree instead of a flat `HashMap`?
//!
//! A flat map (`"baml.llm.render_prompt" → Function`) can resolve a known
//! path in O(1), but it **cannot enumerate a scope's contents** without
//! scanning every entry. The scope tree can: walk to the `baml.llm` node
//! and its `children` `HashMap` is right there.
//!
//! This matters for **LSP autocomplete**: when a user types `baml.llm.`
//! and triggers completion, we need to list all items in that namespace.
//! The tree gives us that for free via [`scope_children`]. Without it,
//! we'd need O(n) prefix-scanning across all builtins + the symbol table.
//!
//! # Example scope tree
//!
//! ```text
//! Root
//! ├── "baml" ──► Namespace
//! │   ├── "llm" ──► Namespace
//! │   │   ├── "render_prompt" ──► Function (BAML-defined)
//! │   │   ├── "build_plan" ──► Function (BAML-defined)
//! │   │   └── "ClientType" ──► Enum (Rust-defined)
//! │   │       ├── "Primitive" ──► Variant
//! │   │       └── "Fallback" ──► Variant
//! │   ├── "Array" ──► Namespace
//! │   │   └── "length" ──► BuiltinFunction
//! │   └── "sys" ──► Namespace
//! │       └── "panic" ──► BuiltinFunction
//! ├── "env" ──► Namespace
//! │   └── "get" ──► BuiltinFunction
//! ├── "Status" ──► Enum (user-defined)
//! │   ├── "Active" ──► Variant
//! │   └── "Inactive" ──► Variant
//! └── "MyFunc" ──► Function (user-defined)
//! ```
//!
//! The upper portion (under `baml`, `env`) is a **static builtin tree**
//! built once with `OnceLock`.  The lower portion (user-defined items)
//! comes from the **symbol table** (Salsa-cached per project).  BAML-
//! defined items inside builtin namespaces (e.g., `baml.llm.render_prompt`)
//! are looked up via the symbol table when the static tree doesn't contain
//! the name.
//!
//! # Architecture
//!
//! The scope tree has two layers:
//!
//! - **Static builtin tree** (`OnceLock`, computed once per process):
//!   Rust-defined functions and enums under `baml.*`, `env.*`, etc.
//!   These never change, so no Salsa tracking is needed.
//!
//! - **Dynamic symbol table** (`symbol_table`, Salsa-cached per project):
//!   BAML-defined functions and enums, looked up via O(1) hash lookups
//!   with properly constructed `QualifiedName`s.
//!
//! Enum variant lookups use a per-enum Salsa query (`enum_variant_names`),
//! so modifying one enum doesn't invalidate resolution of unrelated paths.
//!
//! # Incrementality
//!
//! `resolve_path` is a plain function, not a Salsa query. Incrementality
//! comes from Salsa's transitive dependency tracking: when a Salsa-tracked
//! caller (e.g., TIR's `function_body`) invokes `resolve_path`, Salsa
//! records dependencies on whichever queries `resolve_path` touched
//! (e.g., `enum_variant_names(db, status_loc)`). Only callers that
//! depend on changed queries are re-executed.
//!
//! Current granularity:
//!
//! - **Enum variants**: per-enum (`enum_variant_names`). Changing one
//!   enum only re-infers functions that reference that enum's variants.
//!
//! - **Everything else**: per-project (`symbol_table`). Adding/removing
//!   a function or enum changes the symbol table, which invalidates all
//!   function bodies that called `resolve_path`. This is the same
//!   granularity as rust-analyzer's per-crate `DefMap` — with a flat
//!   global namespace (no modules), every name lives in one scope, so
//!   any name change can affect any resolution.
//!
//! # Future: user-defined modules
//!
//! When BAML gains modules, the scope-tree walk naturally extends: each
//! module becomes a scope node that the walk descends into, just like it
//! descends into `baml.llm` today.
//!
//! The initial approach should follow rust-analyzer: store per-module
//! scopes as data inside one project-level query (like their per-crate
//! `DefMap` contains per-module `ItemScope`s). Only split into separate
//! per-module Salsa queries if profiling shows the project-level rebuild
//! is too slow — rust-analyzer proves per-crate granularity is sufficient
//! for large codebases.

use std::sync::OnceLock;

use baml_base::{Name, Namespace, QualifiedName};
use rustc_hash::FxHashMap;

use crate::{Db, Definition, enum_variant_names, symbol_table::symbol_table};

/// Result of resolving a multi-segment path expression at HIR level.
///
/// This captures name resolution results that don't require type information:
/// - Builtin function paths (e.g., `baml.Array.length`)
/// - BAML-defined function paths (e.g., `baml.llm.render_prompt`)
/// - Enum variant paths (e.g., `Status.Active`, `baml.llm.ClientType.Primitive`)
///
/// Paths that require type information (variable + field chains like `obj.field`)
/// are left unresolved and handled by TIR during type inference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathResolution {
    /// Rust-implemented builtin function (e.g., `baml.Array.length`, `baml.sys.panic`).
    BuiltinFunction(QualifiedName),
    /// BAML-defined function in a namespace (e.g., `baml.llm.render_prompt`).
    Function(QualifiedName),
    /// Enum variant (e.g., `Status.Active`, `baml.llm.ClientType.Primitive`).
    EnumVariant {
        enum_fqn: QualifiedName,
        variant: Name,
    },
}

// ---------------------------------------------------------------------------
// Static builtin scope tree
// ---------------------------------------------------------------------------

/// A node in the static builtin scope tree.
enum BuiltinNode {
    /// A namespace containing child nodes (e.g., `baml`, `baml.llm`).
    Namespace(FxHashMap<String, BuiltinNode>),
    /// A terminal builtin function (e.g., `baml.Array.length`).
    Function(QualifiedName),
    /// A terminal builtin enum with its variant names.
    Enum {
        fqn: QualifiedName,
        variants: Vec<&'static str>,
    },
}

/// Build the static scope tree from all Rust-defined builtins.
///
/// Computed once and cached for the lifetime of the process.
fn builtin_tree() -> &'static FxHashMap<String, BuiltinNode> {
    static TREE: OnceLock<FxHashMap<String, BuiltinNode>> = OnceLock::new();
    TREE.get_or_init(|| {
        let mut root = FxHashMap::default();

        for builtin in baml_builtins::builtins() {
            let parts: Vec<&str> = builtin.path.split('.').collect();
            let leaf = BuiltinNode::Function(QualifiedName::from_builtin_path(builtin.path));
            insert_leaf(&mut root, &parts, leaf);
        }

        for be in baml_builtins::builtin_enums() {
            let parts: Vec<&str> = be.path.split('.').collect();
            let leaf = BuiltinNode::Enum {
                fqn: QualifiedName::from_builtin_path(be.path),
                variants: be.variants.clone(),
            };
            insert_leaf(&mut root, &parts, leaf);
        }

        root
    })
}

/// Insert a leaf at the given path, creating intermediate `Namespace` nodes.
fn insert_leaf(node: &mut FxHashMap<String, BuiltinNode>, parts: &[&str], leaf: BuiltinNode) {
    if parts.len() == 1 {
        node.insert(parts[0].to_string(), leaf);
    } else {
        let child = node
            .entry(parts[0].to_string())
            .or_insert_with(|| BuiltinNode::Namespace(FxHashMap::default()));
        match child {
            BuiltinNode::Namespace(children) => insert_leaf(children, &parts[1..], leaf),
            _ => debug_assert!(
                false,
                "Path conflict: {} is not a namespace but has child path",
                parts[0]
            ),
        }
    }
}

// ---------------------------------------------------------------------------
// Public API: path resolution
// ---------------------------------------------------------------------------

/// Resolve a multi-segment path by walking the scope tree.
///
/// Returns `None` for paths not in any scope (variable + field chains
/// that require type-directed resolution in TIR).
pub fn resolve_path(
    db: &dyn Db,
    project: baml_workspace::Project,
    segments: &[Name],
) -> Option<PathResolution> {
    let first = segments.first()?;

    // If the first segment enters a builtin namespace, walk the static tree.
    if let Some(node) = builtin_tree().get(first.as_str()) {
        let ns_path: Vec<Name> = Vec::new();
        return walk_builtin(db, project, node, &segments[1..], ns_path);
    }

    // Otherwise: user-defined items in the global scope.
    let sym = symbol_table(db, project);
    let fqn = QualifiedName::local(first.clone());

    // Function (value namespace)?
    if sym.lookup_value(db, &fqn).is_some() {
        return (segments.len() == 1).then_some(PathResolution::Function(fqn));
    }

    // Enum (type namespace) + variant?
    if let Some(Definition::Enum(loc)) = sym.lookup_type(db, &fqn) {
        return resolve_enum_variant(db, loc, &fqn, &segments[1..]);
    }

    None
}

// ---------------------------------------------------------------------------
// Public API: scope enumeration (for LSP autocomplete)
// ---------------------------------------------------------------------------

/// An item visible in a scope, returned by [`scope_children`].
#[derive(Debug, Clone)]
pub enum ScopeChild {
    /// A namespace that can be descended into.
    Namespace(Name),
    /// A builtin function (Rust-defined).
    BuiltinFunction { name: Name, fqn: QualifiedName },
    /// A BAML-defined function.
    Function { name: Name, fqn: QualifiedName },
    /// An enum (builtin or BAML-defined) — its variants are one level deeper.
    Enum { name: Name, fqn: QualifiedName },
    /// An enum variant.
    Variant(Name),
}

/// List all items visible at a given scope path.
///
/// Used by LSP autocomplete to enumerate a namespace's contents.
///
/// # Examples
///
/// - `scope_children(db, project, &[])` — root scope: `baml`, `env`,
///   plus all user-defined functions, enums, classes.
/// - `scope_children(db, project, &["baml", "llm"])` — everything in
///   the `baml.llm` namespace.
/// - `scope_children(db, project, &["Status"])` — variants of `Status`.
pub fn scope_children(
    db: &dyn Db,
    project: baml_workspace::Project,
    scope_path: &[Name],
) -> Vec<ScopeChild> {
    if scope_path.is_empty() {
        return root_scope_children(db, project);
    }

    let first = &scope_path[0];

    // Walk into builtin tree?
    if let Some(node) = builtin_tree().get(first.as_str()) {
        let ns_path: Vec<Name> = Vec::new();
        return builtin_scope_children(db, project, node, &scope_path[1..], ns_path);
    }

    // User-defined enum → list variants.
    let sym = symbol_table(db, project);
    let fqn = QualifiedName::local(first.clone());
    if let Some(Definition::Enum(loc)) = sym.lookup_type(db, &fqn) {
        if scope_path.len() == 1 {
            return enum_variant_names(db, loc)
                .iter()
                .map(|v| ScopeChild::Variant(v.clone()))
                .collect();
        }
    }

    vec![]
}

/// Children of the root scope.
fn root_scope_children(db: &dyn Db, project: baml_workspace::Project) -> Vec<ScopeChild> {
    let mut children = Vec::new();

    // Builtin namespace roots (baml, env, etc.)
    for key in builtin_tree().keys() {
        children.push(ScopeChild::Namespace(Name::new(key)));
    }

    // User-defined items from symbol table
    let sym = symbol_table(db, project);
    for fqn in sym.values(db).keys() {
        if matches!(fqn.namespace, Namespace::Local) {
            children.push(ScopeChild::Function {
                name: fqn.name.clone(),
                fqn: fqn.clone(),
            });
        }
    }
    for (fqn, def) in sym.types(db) {
        if matches!(fqn.namespace, Namespace::Local) {
            if let Definition::Enum(_) = def {
                children.push(ScopeChild::Enum {
                    name: fqn.name.clone(),
                    fqn: fqn.clone(),
                });
            }
        }
    }

    children
}

/// Children of a node in the builtin tree.
fn builtin_scope_children(
    db: &dyn Db,
    project: baml_workspace::Project,
    node: &BuiltinNode,
    remaining: &[Name],
    ns_path: Vec<Name>,
) -> Vec<ScopeChild> {
    match node {
        // Leaf nodes have no children to enumerate.
        BuiltinNode::Function(_) => vec![],

        // Enum node → list its variants.
        BuiltinNode::Enum { variants, .. } => {
            if remaining.is_empty() {
                variants
                    .iter()
                    .map(|v| ScopeChild::Variant(Name::new(v)))
                    .collect()
            } else {
                vec![]
            }
        }

        // Namespace → either descend further or enumerate this level.
        BuiltinNode::Namespace(children) => {
            if let Some(next) = remaining.first() {
                // Descend deeper.
                if let Some(child) = children.get(next.as_str()) {
                    let mut deeper = ns_path;
                    deeper.push(next.clone());
                    return builtin_scope_children(db, project, child, &remaining[1..], deeper);
                }
                // Might be a BAML-defined enum in this namespace.
                let fqn = QualifiedName {
                    namespace: Namespace::BamlStd { path: ns_path },
                    name: next.clone(),
                };
                let sym = symbol_table(db, project);
                if let Some(Definition::Enum(loc)) = sym.lookup_type(db, &fqn) {
                    if remaining.len() == 1 {
                        return enum_variant_names(db, loc)
                            .iter()
                            .map(|v| ScopeChild::Variant(v.clone()))
                            .collect();
                    }
                }
                return vec![];
            }

            // Enumerate this namespace level.
            let mut result = Vec::new();

            // Static builtin children.
            for (name, child) in children {
                match child {
                    BuiltinNode::Namespace(_) => {
                        result.push(ScopeChild::Namespace(Name::new(name)));
                    }
                    BuiltinNode::Function(fqn) => {
                        result.push(ScopeChild::BuiltinFunction {
                            name: Name::new(name),
                            fqn: fqn.clone(),
                        });
                    }
                    BuiltinNode::Enum { fqn, .. } => {
                        result.push(ScopeChild::Enum {
                            name: Name::new(name),
                            fqn: fqn.clone(),
                        });
                    }
                }
            }

            // BAML-defined items in this namespace (via symbol table).
            let sym = symbol_table(db, project);
            for fqn in sym.values(db).keys() {
                if let Namespace::BamlStd { path } = &fqn.namespace {
                    if *path == ns_path {
                        result.push(ScopeChild::Function {
                            name: fqn.name.clone(),
                            fqn: fqn.clone(),
                        });
                    }
                }
            }
            for (fqn, def) in sym.types(db) {
                if let Namespace::BamlStd { path } = &fqn.namespace {
                    if *path == ns_path {
                        if matches!(def, Definition::Enum(_)) {
                            result.push(ScopeChild::Enum {
                                name: fqn.name.clone(),
                                fqn: fqn.clone(),
                            });
                        }
                    }
                }
            }

            result
        }
    }
}

// ---------------------------------------------------------------------------
// Builtin tree walk (for path resolution)
// ---------------------------------------------------------------------------

/// Walk one level of the builtin scope tree.
///
/// `ns_path` is the accumulated namespace segments after `baml` (e.g.,
/// `["llm"]` when inside `baml.llm`), used to construct `BamlStd`
/// `QualifiedName`s for symbol table lookups.
fn walk_builtin(
    db: &dyn Db,
    project: baml_workspace::Project,
    node: &BuiltinNode,
    remaining: &[Name],
    ns_path: Vec<Name>,
) -> Option<PathResolution> {
    match node {
        BuiltinNode::Function(fqn) => remaining
            .is_empty()
            .then_some(PathResolution::BuiltinFunction(fqn.clone())),

        BuiltinNode::Enum { fqn, variants } => {
            if remaining.len() == 1 && variants.contains(&remaining[0].as_str()) {
                Some(PathResolution::EnumVariant {
                    enum_fqn: fqn.clone(),
                    variant: remaining[0].clone(),
                })
            } else {
                None
            }
        }

        BuiltinNode::Namespace(children) => {
            let next = remaining.first()?;

            // Static builtin child?
            if let Some(child) = children.get(next.as_str()) {
                let mut deeper = ns_path;
                deeper.push(next.clone());
                return walk_builtin(db, project, child, &remaining[1..], deeper);
            }

            // BAML-defined item in this namespace (O(1) symbol table lookup).
            let fqn = QualifiedName {
                namespace: Namespace::BamlStd { path: ns_path },
                name: next.clone(),
            };
            let sym = symbol_table(db, project);

            if sym.lookup_value(db, &fqn).is_some() {
                return remaining[1..]
                    .is_empty()
                    .then_some(PathResolution::Function(fqn));
            }

            if let Some(Definition::Enum(loc)) = sym.lookup_type(db, &fqn) {
                return resolve_enum_variant(db, loc, &fqn, &remaining[1..]);
            }

            None
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Resolve enum variant from remaining segments (expects exactly 1).
fn resolve_enum_variant(
    db: &dyn Db,
    loc: crate::EnumLoc<'_>,
    enum_fqn: &QualifiedName,
    remaining: &[Name],
) -> Option<PathResolution> {
    if remaining.len() != 1 {
        return None;
    }
    let variant = &remaining[0];
    enum_variant_names(db, loc)
        .contains(variant)
        .then_some(PathResolution::EnumVariant {
            enum_fqn: enum_fqn.clone(),
            variant: variant.clone(),
        })
}
