//! Per-file semantic index — the central data structure of `compiler2_hir`.
//!
//! Built by `SemanticIndexBuilder`, stored as a Salsa tracked query result
//! with `no_eq` (always re-runs on file change). Projection queries extract
//! individual fields with `Arc` equality for Salsa early-cutoff.

use std::{collections::HashSet, sync::Arc};

use baml_base::Name;
use baml_compiler2_ast::{ExprId, LoweringDiagnostic, PatId, StmtId};
use text_size::{TextRange, TextSize};

// ── PathResolution ────────────────────────────────────────────────────────────

/// Scope-level resolution of a multi-segment `Path` expression's root segment.
///
/// Produced during HIR scope building for `Path` nodes with ≥ 2 segments.
/// Only distinguishes whether the root is a locally-declared variable/param
/// (in which case segments[1..] are field accesses on the local) or not
/// (in which case the path is a package-qualified name, to be resolved by TIR).
///
/// Note: Package-item resolution requires cross-file `package_items` queries
/// that are not available during HIR building (would create a circular
/// dependency: `file_semantic_index` → `package_items` → `file_semantic_index`).
/// Full resolution (namespace vs package-item vs unknown) is done in TIR.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathResolution {
    /// Root segment is a local variable, parameter, or capture in the current scope.
    /// Segments[1..] are field/method accesses on the local, resolved by TIR type
    /// checking.
    Local { name: Name },
    /// Root segment is not a known local. May be a package name, namespace
    /// shorthand, or truly unresolved — TIR determines which.
    Unknown,
}

use crate::{
    contributions::FileSymbolContributions,
    diagnostic::Hir2Diagnostic,
    item_tree::{ItemTree, ItemTreeSourceMap},
    scope::{FileScopeId, Scope, ScopeId, ScopeKind},
};

// ── DefinitionSite ───────────────────────────────────────────────────────────

/// Where a local variable was defined (for go-to-definition).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DefinitionSite {
    /// Defined in a let statement.
    Statement(StmtId),
    /// Defined as a function parameter (with its index).
    Parameter(usize),
    /// Defined by a pattern binding (match arm, catch arm, catch clause, etc.).
    PatternBinding(PatId),
}

// ── BindingId ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BindingKind {
    /// A local binding row in `scope_bindings[scope].bindings`.
    Local(u32),
    /// A parameter index in `scope_bindings[scope].params`.
    Parameter(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BindingId {
    pub scope: FileScopeId,
    pub kind: BindingKind,
}

impl BindingId {
    pub fn local(scope: FileScopeId, binding_index: usize) -> Self {
        Self {
            scope,
            kind: BindingKind::Local(
                u32::try_from(binding_index).expect("local binding index overflow"),
            ),
        }
    }

    pub fn parameter(scope: FileScopeId, param_idx: usize) -> Self {
        Self {
            scope,
            kind: BindingKind::Parameter(param_idx),
        }
    }

    pub fn local_index(self) -> Option<usize> {
        match self.kind {
            BindingKind::Local(idx) => Some(idx as usize),
            BindingKind::Parameter(_) => None,
        }
    }
}

// ── ScopeBindings ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalBinding {
    pub name: Name,
    pub site: DefinitionSite,
    pub pattern: PatId,
    pub name_range: TextRange,
    pub visible_from: TextSize,
}

/// Per-scope local bindings — what names are introduced in this scope.
///
/// Lightweight version of Ty's `PlaceTable` + `UseDefMap`. BAML's simpler
/// scoping (no reassignment, no conditional definitions) means a flat list
/// suffices — no flow-sensitive bitsets needed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeBindings {
    /// Let-bindings in this scope, in source order.
    pub bindings: Vec<LocalBinding>,
    /// Parameters (for Function/Lambda scopes).
    pub params: Vec<(Name, usize)>, // (name, param_index)
    /// Variables captured from ancestor scopes (for Lambda scopes only).
    /// Each entry is `(name, definition_site)` to uniquely identify the
    /// captured declaration, even in the presence of shadowing.
    /// Populated by capture analysis in `SemanticIndexBuilder::walk_expr_body`.
    pub captures: Vec<(Name, BindingId)>,
    /// Bindings in this scope that are captured by a descendant lambda.
    /// Used by MIR lowering to decide which locals need cell wrapping.
    pub captured_bindings: HashSet<BindingId>,
}

impl ScopeBindings {
    pub fn new() -> Self {
        Self {
            bindings: Vec::new(),
            params: Vec::new(),
            captures: Vec::new(),
            captured_bindings: HashSet::new(),
        }
    }
}

impl Default for ScopeBindings {
    fn default() -> Self {
        Self::new()
    }
}

/// Shared local-binding lookup used while building and after indexing.
///
/// Keep this as the single source for parent-scope visibility semantics:
/// skip ancestor class scopes, scan local bindings in reverse source order,
/// check `visible_from`, then fall back to parameters.
pub(crate) fn visible_binding_at_in_scopes(
    scopes: &[Scope],
    scope_bindings: &[ScopeBindings],
    scope_id: FileScopeId,
    at_offset: TextSize,
    name: &Name,
) -> Option<BindingId> {
    let mut current = Some(scope_id);
    while let Some(ancestor_id) = current {
        let scope = &scopes[ancestor_id.index() as usize];
        if matches!(scope.kind, ScopeKind::Class) && ancestor_id != scope_id {
            current = scope.parent;
            continue;
        }

        let bindings = &scope_bindings[ancestor_id.index() as usize];
        for (binding_idx, binding) in bindings.bindings.iter().enumerate().rev() {
            if &binding.name == name && binding.visible_from <= at_offset {
                return Some(BindingId::local(ancestor_id, binding_idx));
            }
        }
        for (param_name, param_idx) in &bindings.params {
            if param_name == name {
                return Some(BindingId::parameter(ancestor_id, *param_idx));
            }
        }
        current = scope.parent;
    }
    None
}

// ── SemanticIndexExtra ───────────────────────────────────────────────────────

/// Rare/optional data for `FileSemanticIndex`. Heap-allocated only when
/// at least one field is non-empty. Avoids bloating the common case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticIndexExtra {
    pub diagnostics: Vec<Hir2Diagnostic>,
    pub lowering_diagnostics: Vec<LoweringDiagnostic>,
}

// ── FileSemanticIndex ────────────────────────────────────────────────────────

/// Per-file semantic index.
///
/// Built by `SemanticIndexBuilder`, stored as a Salsa tracked query result
/// with `no_eq` (always re-runs on file change). Projection queries extract
/// individual fields with `Arc` equality for Salsa early-cutoff.
pub struct FileSemanticIndex<'db> {
    /// Scope tree — arena of `Scope` nodes indexed by `FileScopeId`.
    pub scopes: Vec<Scope>,

    /// Expression → owning scope mapping.
    ///
    /// Every `ExprId` in the file's `ExprBody` arenas is mapped to the
    /// `FileScopeId` of its innermost containing scope. Built during the
    /// `SemanticIndexBuilder` walk.
    ///
    /// Sorted by `ExprId` for binary search (more compact than `HashMap`).
    pub expr_scopes: Vec<(ExprId, FileScopeId)>,

    /// Per-scope local bindings, indexed by `FileScopeId`.
    /// Parallel to `scopes` — `scope_bindings[i]` holds bindings for `scopes[i]`.
    pub scope_bindings: Vec<ScopeBindings>,

    /// Pre-interned `ScopeId<'db>` for each `FileScopeId`.
    /// Avoids repeated Salsa interning at query time.
    pub scope_ids: Vec<ScopeId<'db>>,

    /// Per-file item tree — maps `LocalItemId` to item data.
    pub item_tree: Arc<ItemTree>,

    /// Source map for item tree — field/variant name spans.
    pub item_tree_source_map: Arc<ItemTreeSourceMap>,

    /// Names this file contributes to its package namespace.
    pub symbol_contributions: Arc<FileSymbolContributions<'db>>,

    /// Diagnostics and other rare data. Heap-allocated only when non-empty,
    /// following Ty's `Option<Box<Extra>>` pattern.
    pub extra: Option<Box<SemanticIndexExtra>>,

    /// Root-segment resolution for multi-segment `Path` expressions.
    ///
    /// For each `Path` with ≥ 2 segments, records whether the root is a local
    /// variable/parameter (`PathResolution::Local`) or not
    /// (`PathResolution::Unknown`). Sorted by `ExprId` for binary-search lookup.
    ///
    /// Package-item resolution (namespace, split point) is deferred to TIR
    /// since it requires cross-file `package_items` queries.
    pub path_resolutions: Vec<(ExprId, PathResolution)>,

    /// Environment variable references (`env.X`) found in this file's expression bodies.
    pub env_var_refs: Vec<baml_compiler2_ast::EnvVarRef>,
}

// ── salsa::Update impl ────────────────────────────────────────────────────────

/// # Safety
///
/// This impl is required for `FileSemanticIndex` to be returned from a
/// `#[salsa::tracked(no_eq)]` query. With `no_eq`, Salsa never calls
/// `values_equal` so the actual equality logic doesn't matter — but the
/// `maybe_update` function must correctly transfer ownership of `new_value`
/// into `old_pointer` and return whether a change occurred.
///
/// We always return `true` (always mark as changed), matching the `no_eq`
/// semantics. The `*old_pointer = new_value` write is safe because `old_pointer`
/// points to valid allocated memory that Salsa owns.
#[allow(unsafe_code)]
unsafe impl salsa::Update for FileSemanticIndex<'_> {
    #[allow(unsafe_code)]
    unsafe fn maybe_update(old_pointer: *mut Self, new_value: Self) -> bool {
        // SAFETY: `old_pointer` is valid, aligned, and points to memory Salsa
        // has previously initialized. We drop the old value and write the new one.
        #[allow(unsafe_code)]
        unsafe {
            std::ptr::drop_in_place(old_pointer);
            std::ptr::write(old_pointer, new_value);
        }
        true
    }
}

impl FileSemanticIndex<'_> {
    /// Find the `Lambda` scope whose range exactly matches `span`.
    ///
    /// Linear walk over the scope list (which is small in practice —
    /// bounded by the number of lambda nestings in a file).
    pub fn lambda_scope_for(&self, span: text_size::TextRange) -> Option<FileScopeId> {
        self.scopes
            .iter()
            .enumerate()
            .find(|(_, scope)| matches!(scope.kind, ScopeKind::Lambda) && scope.range == span)
            .map(|(i, _)| {
                #[allow(clippy::cast_possible_truncation)]
                FileScopeId::new(i as u32)
            })
    }

    /// Find the innermost scope containing `offset`.
    ///
    /// Scopes are in DFS pre-order. We walk in reverse (deepest first)
    /// to find the innermost match, preferring deeper (later) scopes.
    ///
    /// When `name` is `Some`, find the named scope first, then return
    /// the deepest **descendant** of that scope at the offset. This
    /// disambiguates companion functions that share the same span as
    /// their parent while still resolving bindings in inner scopes
    /// (match arms, blocks, catch clauses, etc.).
    pub fn scope_at_offset(&self, offset: TextSize, name: Option<&Name>) -> FileScopeId {
        if let Some(n) = name {
            // Find the named scope that contains the offset.
            let named_idx = self.scopes.iter().enumerate().rev().find_map(|(i, scope)| {
                if (scope.range.contains(offset) || scope.range.end() == offset)
                    && scope.name.as_ref() == Some(n)
                {
                    Some(i)
                } else {
                    None
                }
            });

            if let Some(idx) = named_idx {
                let named = &self.scopes[idx];
                let desc_start = named.descendants.start.index() as usize;
                let desc_end = named.descendants.end.index() as usize;

                // Search descendants in reverse (deepest first) for one containing offset.
                for i in (desc_start..desc_end).rev() {
                    let scope = &self.scopes[i];
                    if scope.range.contains(offset) || scope.range.end() == offset {
                        #[allow(clippy::cast_possible_truncation)]
                        return FileScopeId::new(i as u32);
                    }
                }

                // No descendant matched — return the named scope itself.
                #[allow(clippy::cast_possible_truncation)]
                return FileScopeId::new(idx as u32);
            }
        }

        // No name filter — original behavior: find deepest scope at offset.
        for (i, scope) in self.scopes.iter().enumerate().rev() {
            if scope.range.contains(offset) || scope.range.end() == offset {
                #[allow(clippy::cast_possible_truncation)]
                return FileScopeId::new(i as u32);
            }
        }
        FileScopeId::ROOT
    }

    /// Look up which scope owns an expression.
    pub fn expression_scope(&self, expr_id: ExprId) -> Option<FileScopeId> {
        self.expr_scopes
            .binary_search_by_key(&expr_id, |(id, _)| *id)
            .ok()
            .map(|idx| self.expr_scopes[idx].1)
    }

    /// Walk ancestor scopes from `scope_id` upward to the root.
    pub fn ancestor_scopes(&self, scope_id: FileScopeId) -> Vec<FileScopeId> {
        let mut ancestors = vec![scope_id];
        let mut current = scope_id;
        while let Some(parent) = self.scopes[current.index() as usize].parent {
            ancestors.push(parent);
            current = parent;
        }
        ancestors
    }

    pub fn binding_visible_at(&self, binding: &LocalBinding, at_offset: TextSize) -> bool {
        binding.visible_from <= at_offset
    }

    pub fn visible_binding_at(
        &self,
        scope_id: FileScopeId,
        at_offset: TextSize,
        name: &Name,
    ) -> Option<BindingId> {
        visible_binding_at_in_scopes(
            &self.scopes,
            &self.scope_bindings,
            scope_id,
            at_offset,
            name,
        )
    }

    pub fn local_binding(&self, binding_id: BindingId) -> Option<&LocalBinding> {
        let idx = binding_id.local_index()?;
        self.scope_bindings
            .get(binding_id.scope.index() as usize)?
            .bindings
            .get(idx)
    }

    pub fn binding_site(&self, binding_id: BindingId) -> Option<DefinitionSite> {
        match binding_id.kind {
            BindingKind::Local(_) => self.local_binding(binding_id).map(|binding| binding.site),
            BindingKind::Parameter(param_idx) => Some(DefinitionSite::Parameter(param_idx)),
        }
    }

    pub fn diagnostics(&self) -> &[Hir2Diagnostic] {
        self.extra
            .as_ref()
            .map(|e| e.diagnostics.as_slice())
            .unwrap_or(&[])
    }

    /// Look up the path resolution for a multi-segment `Path` expression.
    ///
    /// Returns `None` if `expr_id` was not a multi-segment path (i.e., single-
    /// segment paths and non-path expressions are not recorded here).
    pub fn path_resolution(&self, expr_id: ExprId) -> Option<&PathResolution> {
        self.path_resolutions
            .binary_search_by_key(&expr_id, |(id, _)| *id)
            .ok()
            .map(|idx| &self.path_resolutions[idx].1)
    }
}
