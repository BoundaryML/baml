//! Per-file semantic index — the central data structure of `compiler2_hir`.
//!
//! Built by `SemanticIndexBuilder`, stored as a Salsa tracked query result
//! with `no_eq` (always re-runs on file change). Projection queries extract
//! individual fields with `Arc` equality for Salsa early-cutoff.

use std::sync::Arc;

use baml_base::Name;
use baml_compiler2_ast::{ExprId, StmtId};
use text_size::{TextRange, TextSize};

use crate::{
    contributions::FileSymbolContributions,
    diagnostic::Hir2Diagnostic,
    item_tree::ItemTree,
    scope::{FileScopeId, Scope, ScopeId},
};

// ── DefinitionSite ───────────────────────────────────────────────────────────

/// Where a local variable was defined (for go-to-definition).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DefinitionSite {
    /// Defined in a let statement.
    Statement(StmtId),
    /// Defined as a function parameter (with its index).
    Parameter(usize),
}

// ── ScopeBindings ────────────────────────────────────────────────────────────

/// Per-scope local bindings — what names are introduced in this scope.
///
/// Lightweight version of Ty's `PlaceTable` + `UseDefMap`. BAML's simpler
/// scoping (no reassignment, no conditional definitions) means a flat list
/// suffices — no flow-sensitive bitsets needed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeBindings {
    /// Let-bindings in this scope, in source order.
    pub bindings: Vec<(Name, DefinitionSite, TextRange)>,
    /// Parameters (for Function/Lambda scopes).
    pub params: Vec<(Name, usize)>, // (name, param_index)
}

impl ScopeBindings {
    pub fn new() -> Self {
        Self {
            bindings: Vec::new(),
            params: Vec::new(),
        }
    }
}

impl Default for ScopeBindings {
    fn default() -> Self {
        Self::new()
    }
}

// ── SemanticIndexExtra ───────────────────────────────────────────────────────

/// Rare/optional data for `FileSemanticIndex`. Heap-allocated only when
/// at least one field is non-empty. Avoids bloating the common case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticIndexExtra {
    pub diagnostics: Vec<Hir2Diagnostic>,
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

    /// Names this file contributes to its package namespace.
    pub symbol_contributions: Arc<FileSymbolContributions<'db>>,

    /// Diagnostics and other rare data. Heap-allocated only when non-empty,
    /// following Ty's `Option<Box<Extra>>` pattern.
    pub extra: Option<Box<SemanticIndexExtra>>,
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
    /// Find the innermost scope containing `offset`.
    ///
    /// Scopes are in DFS pre-order. We walk in reverse (deepest first)
    /// to find the innermost match, preferring deeper (later) scopes.
    pub fn scope_at_offset(&self, offset: TextSize) -> FileScopeId {
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

    pub fn diagnostics(&self) -> &[Hir2Diagnostic] {
        self.extra
            .as_ref()
            .map(|e| e.diagnostics.as_slice())
            .unwrap_or(&[])
    }
}
