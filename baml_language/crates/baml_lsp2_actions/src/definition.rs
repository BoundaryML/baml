//! `definition_at` — go-to-definition at a cursor position.
//!
//! This is a regular function (not a Salsa query). It uses the Rowan CST to
//! find the token under the cursor, extracts its text as a name, calls
//! `resolve_name_at` to resolve the name in scope, and then maps the
//! `ResolvedName` to a `Location` (source file + text range).
//!
//! ## Resolution cases
//!
//! - `ResolvedName::Item(def)` — top-level item (class, function, enum, …).
//!   File and name span come from `definition_span(db, def)` in `utils.rs`.
//!
//! - `ResolvedName::Builtin(def)` — same as `Item`, but in the builtin file.
//!
//! - `ResolvedName::Local { definition_site: Some(Statement(stmt_id)) }` — a
//!   `let` binding. Resolve via `function_body_source_map` to get the
//!   statement's source range.
//!
//! - `ResolvedName::Local { definition_site: Some(Parameter(idx)) }` — a
//!   function parameter. Resolve via `function_signature_source_map` to get
//!   the parameter's source range.
//!
//! - `ResolvedName::Unknown` or missing `definition_site` — return `None`.

use baml_base::{Name, SourceFile};
use baml_compiler_syntax::SyntaxKind;
use baml_compiler2_hir::semantic_index::DefinitionSite;
use text_size::{TextRange, TextSize};

use crate::{Db, utils};

// ── Location ─────────────────────────────────────────────────────────────────

/// A source location returned by `definition_at`.
///
/// Contains the target source file and the byte range of the name token at the
/// definition site. The LSP layer converts `file` to a URI and `range` to an
/// LSP `Range`.
#[derive(Clone, PartialEq, Eq)]
pub struct Location {
    /// The file where the symbol is defined.
    pub file: SourceFile,
    /// Byte range of the name token (not the full item body).
    pub range: TextRange,
}

// ── definition_at ─────────────────────────────────────────────────────────────

/// Find the definition of the symbol at `offset` in `file`.
///
/// Regular function (not cached). The expensive work (`file_semantic_index`,
/// `resolve_name_at`) is internally Salsa-cached.
///
/// Returns `None` if the cursor is not on an identifier, or if the name
/// cannot be resolved.
pub fn definition_at(db: &dyn Db, file: SourceFile, offset: TextSize) -> Option<Location> {
    // ── Step 1: find the token at the cursor ─────────────────────────────────
    let token = utils::find_token_at_offset(db, file, offset)?;

    // Only WORD tokens can be names that resolve to definitions.
    if token.kind() != SyntaxKind::WORD {
        return None;
    }

    let name_text = token.text();
    let name = Name::new(name_text);

    // ── Step 2: resolve the name in scope ─────────────────────────────────────
    let resolved = baml_compiler2_tir::resolve::resolve_name_at(db, file, offset, &name);

    // ── Step 3: map ResolvedName to a Location ────────────────────────────────
    match resolved {
        baml_compiler2_tir::resolve::ResolvedName::Item(def)
        | baml_compiler2_tir::resolve::ResolvedName::Builtin(def) => {
            // Top-level item — look up the contribution's name_span.
            let (def_file, range) = utils::definition_span(db, def)?;
            Some(Location {
                file: def_file,
                range,
            })
        }

        baml_compiler2_tir::resolve::ResolvedName::Local {
            definition_site: Some(site),
            ..
        } => local_definition_location(db, file, offset, site),

        baml_compiler2_tir::resolve::ResolvedName::Local {
            definition_site: None,
            ..
        }
        | baml_compiler2_tir::resolve::ResolvedName::Unknown => None,
    }
}

// ── local_definition_location ─────────────────────────────────────────────────

/// Resolve a local variable's definition site to a `Location`.
///
/// Finds the enclosing function by matching the scope range against item_tree
/// functions, then uses the appropriate source map to get the span.
fn local_definition_location(
    db: &dyn Db,
    file: SourceFile,
    at_offset: TextSize,
    site: DefinitionSite,
) -> Option<Location> {
    let index = baml_compiler2_hir::file_semantic_index(db, file);
    let item_tree = baml_compiler2_hir::file_item_tree(db, file);

    // Find the enclosing Function scope to locate the function in the item tree.
    let scope_id = index.scope_at_offset(at_offset);
    let enclosing_func_scope = index
        .ancestor_scopes(scope_id)
        .into_iter()
        .find(|ancestor_id| {
            matches!(
                index.scopes[ancestor_id.index() as usize].kind,
                baml_compiler2_hir::scope::ScopeKind::Function
            )
        })?;

    let func_scope_range = index.scopes[enclosing_func_scope.index() as usize].range;

    // Find the function in the item tree by matching the scope range.
    let (func_local_id, _) = item_tree
        .functions
        .iter()
        .find(|(_, f)| f.span == func_scope_range)?;

    let func_loc = baml_compiler2_hir::loc::FunctionLoc::new(db, file, *func_local_id);

    match site {
        DefinitionSite::Statement(stmt_id) => {
            // Get the source map to convert StmtId → TextRange.
            let source_map = baml_compiler2_hir::body::function_body_source_map(db, func_loc)?;
            let range = source_map.stmt_span(stmt_id);
            Some(Location { file, range })
        }
        DefinitionSite::Parameter(param_idx) => {
            // Get the signature source map to find parameter spans.
            let sig_map =
                baml_compiler2_hir::signature::function_signature_source_map(db, func_loc);
            let range = sig_map.param_spans.get(param_idx).copied()?;
            Some(Location { file, range })
        }
    }
}
