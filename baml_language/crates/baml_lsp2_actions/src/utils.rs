//! Shared helpers for `baml_lsp2_actions`.
//!
//! ## Phase 3 helpers
//!
//! - `find_token_at_offset(db, file, offset) -> Option<SyntaxToken>` — locates
//!   the leaf token in the CST that contains or abuts `offset`. Used by
//!   `definition_at`, `type_at`, `usages_at`, and `completions_at`.
//!
//! - `definition_span(db, def) -> Option<(SourceFile, TextRange)>` — maps a
//!   top-level `Definition` to the file it lives in and the byte range of its
//!   name token. Used by `definition_at` to produce a `Location` for item-level
//!   resolutions.
//!
//! ## Phase 4 helpers
//!
//! - `display_ty(ty: &Ty) -> String` — user-friendly type string for hover
//!   and inlay hints. Delegates to the `Display` impl on `Ty`.
//!
//! - `display_type_expr(te: &TypeExpr) -> String` — format a raw (unresolved)
//!   `TypeExpr` as a source-level string. Used for function parameter types
//!   in hover output.

use baml_base::SourceFile;
use baml_compiler_syntax::{SyntaxToken, TokenAtOffset};
use baml_compiler2_ast::TypeExpr;
use baml_compiler2_hir::contributions::Definition;
use baml_compiler2_tir::ty::Ty;
use text_size::{TextRange, TextSize};

use crate::Db;

// ── find_token_at_offset ──────────────────────────────────────────────────────

/// Find the leaf token in the file's CST that best covers `offset`.
///
/// Uses `rowan::SyntaxNode::token_at_offset`, which returns `TokenAtOffset`:
/// - `Single(tok)` — cursor sits inside one token.
/// - `Between(left, right)` — cursor is exactly at a boundary; we prefer the
///   right-hand token (the one starting at `offset`), falling back to left.
/// - `None` — file is empty; returns `None`.
///
/// For go-to-definition we want identifiers (`WORD` tokens), so the caller
/// should filter on `token.kind() == SyntaxKind::WORD`.
pub fn find_token_at_offset(
    db: &dyn Db,
    file: SourceFile,
    offset: TextSize,
) -> Option<SyntaxToken> {
    let tree = baml_compiler_parser::syntax_tree(db, file);
    match tree.token_at_offset(offset) {
        TokenAtOffset::Single(tok) => Some(tok),
        TokenAtOffset::Between(left, right) => {
            // Prefer the right token (the one the cursor is entering).
            // Fall back to left if right is trivia or whitespace.
            use baml_compiler_syntax::SyntaxKind;
            if right.kind() != SyntaxKind::WHITESPACE && right.kind() != SyntaxKind::NEWLINE {
                Some(right)
            } else {
                Some(left)
            }
        }
        TokenAtOffset::None => None,
    }
}

// ── definition_span ───────────────────────────────────────────────────────────

/// Map a top-level `Definition` to its source file and name span.
///
/// Looks up the `Contribution` for the definition in the target file's
/// `file_symbol_contributions`. The contribution carries the `name_span`
/// (byte range of the name token) — exactly what we need for go-to-definition.
///
/// Returns `None` if the definition is not found in the target file's
/// contributions (which should not happen in practice for well-formed code).
pub fn definition_span<'db>(
    db: &'db dyn Db,
    def: Definition<'db>,
) -> Option<(SourceFile, TextRange)> {
    let def_file = def.file(db);
    let contributions = baml_compiler2_ppir::file_symbol_contributions(db, def_file);

    // Search both type and value namespaces.
    let name_span = contributions
        .types
        .iter()
        .find_map(|(_, contrib)| {
            if contrib.definition == def {
                Some(contrib.name_span)
            } else {
                None
            }
        })
        .or_else(|| {
            contributions.values.iter().find_map(|(_, contrib)| {
                if contrib.definition == def {
                    Some(contrib.name_span)
                } else {
                    None
                }
            })
        })?;

    Some((def_file, name_span))
}

// ── display_ty ────────────────────────────────────────────────────────────────

/// Format a resolved `Ty` as a user-friendly string.
///
/// Delegates to the `Display` impl on `Ty`. For user-visible output (hover,
/// inlay hints) we strip the package qualifier so `user.Foo` shows as `Foo`
/// and `baml.PrimitiveClient` shows as `PrimitiveClient`.
pub fn display_ty(ty: &Ty) -> String {
    use baml_compiler2_tir::ty::PrimitiveType;
    match ty {
        Ty::Class(qn) | Ty::Enum(qn) | Ty::TypeAlias(qn) => qn.name.as_str().to_string(),
        Ty::EnumVariant(qn, v) => format!("{}.{}", qn.name, v),
        Ty::Primitive(p) => match p {
            PrimitiveType::Int => "int".to_string(),
            PrimitiveType::Float => "float".to_string(),
            PrimitiveType::String => "string".to_string(),
            PrimitiveType::Bool => "bool".to_string(),
            PrimitiveType::Null => "null".to_string(),
            PrimitiveType::Image => "image".to_string(),
            PrimitiveType::Audio => "audio".to_string(),
            PrimitiveType::Video => "video".to_string(),
            PrimitiveType::Pdf => "pdf".to_string(),
        },
        Ty::List(inner) => format!("{}[]", display_ty(inner)),
        Ty::Map(k, v) => format!("map<{}, {}>", display_ty(k), display_ty(v)),
        Ty::EvolvingList(inner) => {
            if matches!(**inner, Ty::Never) {
                "_[]".to_string()
            } else {
                format!("{}[]", display_ty(inner))
            }
        }
        Ty::EvolvingMap(k, v) => {
            if matches!(**k, Ty::Never) && matches!(**v, Ty::Never) {
                "map<_, _>".to_string()
            } else {
                format!("map<{}, {}>", display_ty(k), display_ty(v))
            }
        }
        Ty::Union(members) => {
            let parts: Vec<_> = members.iter().map(display_ty).collect();
            parts.join(" | ")
        }
        Ty::Optional(inner) => format!("{}?", display_ty(inner)),
        Ty::Literal(lit, _freshness) => lit.to_string(),
        Ty::Function { params, ret } => {
            let ps: Vec<String> = params
                .iter()
                .map(|(name, ty)| {
                    name.as_ref()
                        .map(|n| format!("{}: {}", n, display_ty(ty)))
                        .unwrap_or_else(|| display_ty(ty))
                })
                .collect();
            format!("({}) -> {}", ps.join(", "), display_ty(ret))
        }
        Ty::Never => "never".to_string(),
        Ty::Void => "void".to_string(),
        Ty::BuiltinUnknown | Ty::Unknown => "unknown".to_string(),
        Ty::RustType => "$rust_type".to_string(),
        Ty::Error => "!error".to_string(),
    }
}

// ── display_type_expr ─────────────────────────────────────────────────────────

/// Format a raw (unresolved) `TypeExpr` as a source-level type string.
///
/// Used for displaying function parameter types and return types in hover
/// output, where we have the AST type expression before resolution. This
/// produces output that matches the user's source syntax.
pub fn display_type_expr(te: &TypeExpr) -> String {
    match te {
        TypeExpr::Path(segments) => {
            // Use only the last segment for brevity (e.g. `baml.Foo` → `Foo`).
            segments
                .last()
                .map(|n| n.as_str().to_string())
                .unwrap_or_else(|| "unknown".to_string())
        }
        TypeExpr::Int => "int".to_string(),
        TypeExpr::Float => "float".to_string(),
        TypeExpr::String => "string".to_string(),
        TypeExpr::Bool => "bool".to_string(),
        TypeExpr::Null => "null".to_string(),
        TypeExpr::Media(kind) => format!("{kind:?}").to_lowercase(),
        TypeExpr::Optional(inner) => format!("{}?", display_type_expr(inner)),
        TypeExpr::List(inner) => format!("{}[]", display_type_expr(inner)),
        TypeExpr::Map { key, value } => {
            format!(
                "map<{}, {}>",
                display_type_expr(key),
                display_type_expr(value)
            )
        }
        TypeExpr::Union(members) => {
            let parts: Vec<_> = members.iter().map(display_type_expr).collect();
            parts.join(" | ")
        }
        TypeExpr::Literal(lit) => lit.to_string(),
        TypeExpr::Function { params, ret } => {
            let ps: Vec<String> = params
                .iter()
                .map(|p| {
                    p.name
                        .as_ref()
                        .map(|n| format!("{}: {}", n, display_type_expr(&p.ty)))
                        .unwrap_or_else(|| display_type_expr(&p.ty))
                })
                .collect();
            format!("({}) -> {}", ps.join(", "), display_type_expr(ret))
        }
        TypeExpr::BuiltinUnknown => "unknown".to_string(),
        TypeExpr::Never => "never".to_string(),
        TypeExpr::Type => "type".to_string(),
        TypeExpr::Rust => "$rust_type".to_string(),
        TypeExpr::Error | TypeExpr::Unknown => "unknown".to_string(),
    }
}
