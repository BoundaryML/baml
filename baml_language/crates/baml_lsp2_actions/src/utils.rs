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
use baml_compiler2_tir::{
    ty::{FunctionParamTy, Ty},
    user_facing::humanize_type_string,
};
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
    let contributions = baml_compiler2_hir::file_symbol_contributions(db, def_file);

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

fn ty_needs_postfix_parens(ty: &Ty) -> bool {
    matches!(ty, Ty::Union(..) | Ty::Function { .. })
}

fn display_ty_as_postfix_base(ty: &Ty) -> String {
    let rendered = display_ty(ty);
    if ty_needs_postfix_parens(ty) {
        format!("({rendered})")
    } else {
        rendered
    }
}

fn display_ty_as_function_result(ty: &Ty) -> String {
    let rendered = display_ty(ty);
    if matches!(ty, Ty::Function { .. }) {
        format!("({rendered})")
    } else {
        rendered
    }
}

pub fn display_function_param_ty(param: &FunctionParamTy) -> String {
    param
        .name
        .as_ref()
        .map(|name| {
            let optional = if param.is_optional() { "?" } else { "" };
            format!("{}{}: {}", name, optional, display_ty(&param.ty))
        })
        .unwrap_or_else(|| display_ty(&param.ty))
}

/// Format a resolved `Ty` as a user-friendly string.
///
/// Delegates to the `Display` impl on `Ty`. For user-visible output (hover,
/// inlay hints) we strip the package qualifier so `user.Foo` shows as `Foo`
/// and `baml.PrimitiveClient` shows as `PrimitiveClient`.
pub fn display_ty(ty: &Ty) -> String {
    use baml_compiler2_tir::ty::PrimitiveType;
    let rendered = match ty {
        Ty::Class(qn, type_args, _) => {
            if type_args.is_empty() {
                qn.to_string()
            } else {
                let args: Vec<String> = type_args.iter().map(display_ty).collect();
                format!("{}<{}>", qn, args.join(", "))
            }
        }
        Ty::Enum(qn, _) | Ty::TypeAlias(qn, _) => qn.to_string(),
        Ty::EnumVariant(qn, v, _) => format!("{qn}.{v}"),
        Ty::Primitive(p, _) => match p {
            PrimitiveType::Int => "int".to_string(),
            PrimitiveType::Float => "float".to_string(),
            PrimitiveType::String => "string".to_string(),
            PrimitiveType::Bool => "bool".to_string(),
            PrimitiveType::Null => "null".to_string(),
            PrimitiveType::Image => "image".to_string(),
            PrimitiveType::Audio => "audio".to_string(),
            PrimitiveType::Video => "video".to_string(),
            PrimitiveType::Pdf => "pdf".to_string(),
            PrimitiveType::Uint8Array => "uint8array".to_string(),
        },
        Ty::List(inner, _) => format!("{}[]", display_ty_as_postfix_base(inner)),
        Ty::Map(k, v, _) => format!("map<{}, {}>", display_ty(k), display_ty(v)),
        Ty::EvolvingList(inner, _) => {
            if matches!(**inner, Ty::Never { .. }) {
                "_[]".to_string()
            } else {
                format!("{}[]", display_ty_as_postfix_base(inner))
            }
        }
        Ty::EvolvingMap(k, v, _) => {
            if matches!(**k, Ty::Never { .. }) && matches!(**v, Ty::Never { .. }) {
                "map<_, _>".to_string()
            } else {
                format!("map<{}, {}>", display_ty(k), display_ty(v))
            }
        }
        Ty::Union(members, _) => {
            let parts: Vec<_> = members.iter().map(display_ty).collect();
            parts.join(" | ")
        }
        Ty::Optional(inner, _) => format!("{}?", display_ty_as_postfix_base(inner)),
        Ty::Literal(lit, _freshness, _) => lit.to_string(),
        Ty::Function {
            params,
            ret,
            throws,
            ..
        } => {
            let ps: Vec<String> = params.iter().map(display_function_param_ty).collect();
            format!(
                "({}) -> {} throws {}",
                ps.join(", "),
                display_ty_as_function_result(ret),
                display_ty(throws)
            )
        }
        Ty::TypeVar(name, _) => name.to_string(),
        Ty::Never { .. } => "never".to_string(),
        Ty::Void { .. } => "void".to_string(),
        Ty::BuiltinUnknown { .. } | Ty::Unknown { .. } => "unknown".to_string(),
        Ty::RustType { .. } => "$rust_type".to_string(),
        Ty::Type { .. } => "type".to_string(),
        Ty::Error { .. } => "!error".to_string(),
        Ty::Future(value, error, _) => {
            format!("Future<{}, {}>", display_ty(value), display_ty(error))
        }
    };
    humanize_type_string(&rendered)
}

// ── display_type_expr ─────────────────────────────────────────────────────────

fn type_expr_needs_postfix_parens(te: &TypeExpr) -> bool {
    matches!(te, TypeExpr::Union { .. } | TypeExpr::Function { .. })
}

fn display_type_expr_as_postfix_base(te: &TypeExpr) -> String {
    let rendered = display_type_expr(te);
    if type_expr_needs_postfix_parens(te) {
        format!("({rendered})")
    } else {
        rendered
    }
}

fn display_type_expr_as_function_result(te: &TypeExpr) -> String {
    let rendered = display_type_expr(te);
    if matches!(te, TypeExpr::Function { .. }) {
        format!("({rendered})")
    } else {
        rendered
    }
}

/// Format a raw (unresolved) `TypeExpr` as a source-level type string.
///
/// Used for displaying function parameter types and return types in hover
/// output, where we have the AST type expression before resolution. This
/// produces output that matches the user's source syntax.
pub fn display_type_expr(te: &TypeExpr) -> String {
    let rendered = match te {
        TypeExpr::Path { segments, .. } => {
            // Use only the last segment for brevity (e.g. `baml.Foo` → `Foo`).
            segments
                .last()
                .map(|n| n.as_str().to_string())
                .unwrap_or_else(|| "unknown".to_string())
        }
        TypeExpr::Int { .. } => "int".to_string(),
        TypeExpr::Float { .. } => "float".to_string(),
        TypeExpr::String { .. } => "string".to_string(),
        TypeExpr::Bool { .. } => "bool".to_string(),
        TypeExpr::Null { .. } => "null".to_string(),
        TypeExpr::Uint8Array { .. } => "uint8array".to_string(),
        TypeExpr::Media { kind, .. } => format!("{kind:?}").to_lowercase(),
        TypeExpr::Optional { inner, .. } => {
            format!("{}?", display_type_expr_as_postfix_base(inner))
        }
        TypeExpr::List { inner, .. } => format!("{}[]", display_type_expr_as_postfix_base(inner)),
        TypeExpr::Map { key, value, .. } => {
            format!(
                "map<{}, {}>",
                display_type_expr(key),
                display_type_expr(value)
            )
        }
        TypeExpr::Union { variants, .. } => {
            let parts: Vec<_> = variants.iter().map(display_type_expr).collect();
            parts.join(" | ")
        }
        TypeExpr::Literal { value, .. } => value.to_string(),
        TypeExpr::Function {
            params,
            ret,
            throws,
            ..
        } => {
            let ps: Vec<String> = params
                .iter()
                .map(|p| {
                    p.name
                        .as_ref()
                        .map(|n| {
                            let optional = if p.optional { "?" } else { "" };
                            format!("{}{}: {}", n, optional, display_type_expr(&p.ty))
                        })
                        .unwrap_or_else(|| display_type_expr(&p.ty))
                })
                .collect();
            let throws = throws
                .as_deref()
                .map(display_type_expr)
                .map(|throws| format!(" throws {throws}"))
                .unwrap_or_default();
            format!(
                "({}) -> {}{}",
                ps.join(", "),
                display_type_expr_as_function_result(ret),
                throws
            )
        }
        TypeExpr::BuiltinUnknown { .. } => "unknown".to_string(),
        TypeExpr::Never { .. } => "never".to_string(),
        TypeExpr::Void { .. } => "void".to_string(),
        TypeExpr::Type { .. } => "type".to_string(),
        TypeExpr::Rust { .. } => "$rust_type".to_string(),
        TypeExpr::Error { .. } | TypeExpr::Unknown { .. } => "unknown".to_string(),
    };
    humanize_type_string(&rendered)
}
