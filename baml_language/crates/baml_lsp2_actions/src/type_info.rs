//! `type_at` — structured type/signature info at a cursor position.
//!
//! This is a regular function (not a Salsa query). It uses the Rowan CST to
//! find the token under the cursor, resolves the name via `resolve_name_at`,
//! and builds a `TypeInfo` value describing what the name refers to.
//!
//! ## Resolution cases
//!
//! - `ResolvedName::Item(Definition::Function(_))` — builds `TypeInfo::Function`
//!   with params and return type from `function_signature`.
//!
//! - `ResolvedName::Item(Definition::Class(_))` — builds `TypeInfo::Class`
//!   with field names and types from `resolve_class_fields`.
//!
//! - `ResolvedName::Item(Definition::Enum(_))` — builds `TypeInfo::Enum`
//!   with variant names from the item tree.
//!
//! - `ResolvedName::Item(Definition::TypeAlias(_))` — builds `TypeInfo::TypeAlias`
//!   with the expansion type from `resolve_type_alias`.
//!
//! - `ResolvedName::Item(Definition::TemplateString(_))` — builds
//!   `TypeInfo::TemplateString` (no further info available).
//!
//! - `ResolvedName::Item(Definition::Client(_) | Generator(_) | ...)` — builds
//!   `TypeInfo::OtherItem` with the kind label.
//!
//! - `ResolvedName::Local { definition_site: Some(Parameter(idx)) }` — builds
//!   `TypeInfo::LocalVar` with the parameter type from `function_signature`.
//!
//! - `ResolvedName::Local { definition_site: Some(Statement(stmt_id)) }` — builds
//!   `TypeInfo::LocalVar` with the binding type from `infer_scope_types`.
//!
//! - `ResolvedName::Builtin(def)` — same as the matching `Item` case above.
//!
//! - `ResolvedName::Unknown` or cursor not on a WORD token — returns `None`.

use baml_base::{Name, SourceFile};
use baml_compiler_syntax::SyntaxKind;
use baml_compiler2_hir::{
    contributions::Definition, scope::ScopeKind, semantic_index::DefinitionSite,
};
use text_size::TextSize;

use crate::{Db, utils};

// ── TypeInfo ──────────────────────────────────────────────────────────────────

/// Structured type/signature info at a cursor position.
///
/// Returned by `type_at`. The LSP layer (`request.rs`) formats this into hover
/// markdown. Keeping it as a structured type makes it easy to format for
/// different output contexts (markdown, plain text, etc.).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeInfo {
    /// A function definition: name, parameters (name + type string), return type.
    Function {
        name: String,
        params: Vec<(String, String)>,
        return_type: Option<String>,
    },
    /// A class definition: name, fields (name + type string).
    Class {
        name: String,
        fields: Vec<(String, String)>,
    },
    /// An enum definition: name, variants.
    Enum { name: String, variants: Vec<String> },
    /// A type alias: name + the expansion type string.
    TypeAlias { name: String, expansion: String },
    /// A template string: name only (no further type info).
    TemplateString { name: String },
    /// A local variable (let binding or parameter): name + inferred/declared type.
    LocalVar { name: String, ty: String },
    /// A non-structural top-level item (client, generator, test, retry_policy).
    OtherItem { name: String, kind: &'static str },
}

impl TypeInfo {
    /// Format this `TypeInfo` as hover markdown.
    ///
    /// The caller (request.rs) wraps the result in an LSP `MarkupContent`.
    pub fn to_hover_markdown(&self) -> String {
        match self {
            TypeInfo::Function {
                name,
                params,
                return_type,
            } => {
                let param_strs: Vec<String> =
                    params.iter().map(|(n, t)| format!("{n}: {t}")).collect();
                let ret = return_type
                    .as_deref()
                    .map(|r| format!(" -> {r}"))
                    .unwrap_or_default();
                format!(
                    "```baml\nfunction {}({}){}\n```",
                    name,
                    param_strs.join(", "),
                    ret
                )
            }
            TypeInfo::Class { name, fields } => {
                let field_strs: Vec<String> = fields
                    .iter()
                    .map(|(n, t)| format!("    {n}: {t}"))
                    .collect();
                if field_strs.is_empty() {
                    format!("```baml\nclass {name} {{}}\n```")
                } else {
                    format!(
                        "```baml\nclass {name} {{\n{}\n}}\n```",
                        field_strs.join("\n")
                    )
                }
            }
            TypeInfo::Enum { name, variants } => {
                let variant_strs: Vec<String> =
                    variants.iter().map(|v| format!("    {v}")).collect();
                if variant_strs.is_empty() {
                    format!("```baml\nenum {name} {{}}\n```")
                } else {
                    format!(
                        "```baml\nenum {name} {{\n{}\n}}\n```",
                        variant_strs.join("\n")
                    )
                }
            }
            TypeInfo::TypeAlias { name, expansion } => {
                format!("```baml\ntype {name} = {expansion}\n```")
            }
            TypeInfo::TemplateString { name } => {
                format!("```baml\ntemplate_string {name}\n```")
            }
            TypeInfo::LocalVar { name, ty } => {
                format!("```baml\n{name}: {ty}\n```")
            }
            TypeInfo::OtherItem { name, kind } => {
                format!("```baml\n{kind} {name}\n```")
            }
        }
    }
}

// ── type_at ───────────────────────────────────────────────────────────────────

/// Find structured type/signature info for the symbol at `offset` in `file`.
///
/// Regular function (not cached). The expensive work (`file_semantic_index`,
/// `function_signature`, `resolve_class_fields`, `infer_scope_types`) is
/// internally Salsa-cached.
///
/// Returns `None` if the cursor is not on an identifier, or if the name
/// cannot be resolved.
pub fn type_at(db: &dyn Db, file: SourceFile, offset: TextSize) -> Option<TypeInfo> {
    // ── Step 1: find the token at the cursor ─────────────────────────────────
    let token = utils::find_token_at_offset(db, file, offset)?;

    // Only WORD tokens can be names.
    if token.kind() != SyntaxKind::WORD {
        return None;
    }

    let name_text = token.text();
    let name = Name::new(name_text);

    // ── Step 2: resolve the name in scope ─────────────────────────────────────
    let resolved = baml_compiler2_tir::resolve::resolve_name_at(db, file, offset, &name);

    // ── Step 3: build TypeInfo based on the resolution ────────────────────────
    match resolved {
        baml_compiler2_tir::resolve::ResolvedName::Item(def)
        | baml_compiler2_tir::resolve::ResolvedName::Builtin(def) => {
            type_info_for_definition(db, def)
        }

        baml_compiler2_tir::resolve::ResolvedName::Local {
            name: local_name,
            definition_site: Some(site),
        } => local_type_info(db, file, offset, &local_name, site),

        baml_compiler2_tir::resolve::ResolvedName::Local {
            definition_site: None,
            ..
        }
        | baml_compiler2_tir::resolve::ResolvedName::Unknown => None,
    }
}

// ── type_info_for_definition ──────────────────────────────────────────────────

/// Build `TypeInfo` for a top-level item definition.
fn type_info_for_definition(db: &dyn Db, def: Definition<'_>) -> Option<TypeInfo> {
    match def {
        Definition::Function(func_loc) => {
            let sig = baml_compiler2_hir::signature::function_signature(db, func_loc);
            let params = sig
                .params
                .iter()
                .map(|(param_name, type_expr)| {
                    (
                        param_name.as_str().to_string(),
                        utils::display_type_expr(type_expr),
                    )
                })
                .collect();
            let return_type = sig
                .return_type
                .as_ref()
                .map(|te| utils::display_type_expr(te));
            Some(TypeInfo::Function {
                name: sig.name.as_str().to_string(),
                params,
                return_type,
            })
        }

        Definition::Class(class_loc) => {
            let item_tree = baml_compiler2_ppir::file_item_tree(db, class_loc.file(db));
            let class_data = &item_tree[class_loc.id(db)];
            let class_name = class_data.name.as_str().to_string();

            // Use resolved field types (Salsa-cached).
            let resolved = baml_compiler2_tir::inference::resolve_class_fields(db, class_loc);
            let fields = resolved
                .fields
                .iter()
                .map(|(field_name, ty)| (field_name.as_str().to_string(), utils::display_ty(ty)))
                .collect();

            Some(TypeInfo::Class {
                name: class_name,
                fields,
            })
        }

        Definition::Enum(enum_loc) => {
            let item_tree = baml_compiler2_ppir::file_item_tree(db, enum_loc.file(db));
            let enum_data = &item_tree[enum_loc.id(db)];
            let variants = enum_data
                .variants
                .iter()
                .map(|v| v.name.as_str().to_string())
                .collect();
            Some(TypeInfo::Enum {
                name: enum_data.name.as_str().to_string(),
                variants,
            })
        }

        Definition::TypeAlias(alias_loc) => {
            let item_tree = baml_compiler2_ppir::file_item_tree(db, alias_loc.file(db));
            let alias_data = &item_tree[alias_loc.id(db)];
            let alias_name = alias_data.name.as_str().to_string();

            // Use the resolved (lowered) type for display.
            let resolved = baml_compiler2_tir::inference::resolve_type_alias(db, alias_loc);
            let expansion = utils::display_ty(&resolved.ty);

            Some(TypeInfo::TypeAlias {
                name: alias_name,
                expansion,
            })
        }

        Definition::TemplateString(ts_loc) => {
            let item_tree = baml_compiler2_ppir::file_item_tree(db, ts_loc.file(db));
            let ts_data = &item_tree[ts_loc.id(db)];
            Some(TypeInfo::TemplateString {
                name: ts_data.name.as_str().to_string(),
            })
        }

        Definition::Client(loc) => {
            let item_tree = baml_compiler2_ppir::file_item_tree(db, loc.file(db));
            let data = &item_tree[loc.id(db)];
            Some(TypeInfo::OtherItem {
                name: data.name.as_str().to_string(),
                kind: "client",
            })
        }

        Definition::Generator(loc) => {
            let item_tree = baml_compiler2_ppir::file_item_tree(db, loc.file(db));
            let data = &item_tree[loc.id(db)];
            Some(TypeInfo::OtherItem {
                name: data.name.as_str().to_string(),
                kind: "generator",
            })
        }

        Definition::Test(loc) => {
            let item_tree = baml_compiler2_ppir::file_item_tree(db, loc.file(db));
            let data = &item_tree[loc.id(db)];
            Some(TypeInfo::OtherItem {
                name: data.name.as_str().to_string(),
                kind: "test",
            })
        }

        Definition::RetryPolicy(loc) => {
            let item_tree = baml_compiler2_ppir::file_item_tree(db, loc.file(db));
            let data = &item_tree[loc.id(db)];
            Some(TypeInfo::OtherItem {
                name: data.name.as_str().to_string(),
                kind: "retry_policy",
            })
        }
    }
}

// ── local_type_info ───────────────────────────────────────────────────────────

/// Build `TypeInfo::LocalVar` for a local variable (let binding or parameter).
fn local_type_info(
    db: &dyn Db,
    file: SourceFile,
    at_offset: TextSize,
    name: &Name,
    site: DefinitionSite,
) -> Option<TypeInfo> {
    let index = baml_compiler2_ppir::file_semantic_index(db, file);
    let item_tree = baml_compiler2_ppir::file_item_tree(db, file);

    // Find the enclosing Function scope.
    let scope_id = index.scope_at_offset(at_offset);
    let enclosing_func_scope = index
        .ancestor_scopes(scope_id)
        .into_iter()
        .find(|ancestor_id| {
            matches!(
                index.scopes[ancestor_id.index() as usize].kind,
                ScopeKind::Function
            )
        })?;

    let func_scope_range = index.scopes[enclosing_func_scope.index() as usize].range;

    // Match scope range to a function in the item tree.
    let (func_local_id, _) = item_tree
        .functions
        .iter()
        .find(|(_, f)| f.span == func_scope_range)?;

    let func_loc = baml_compiler2_hir::loc::FunctionLoc::new(db, file, *func_local_id);

    match site {
        DefinitionSite::Parameter(param_idx) => {
            // Get the declared parameter type from the function signature.
            let sig = baml_compiler2_hir::signature::function_signature(db, func_loc);
            let ty_str = sig
                .params
                .get(param_idx)
                .map(|(_, te)| utils::display_type_expr(te))
                .unwrap_or_else(|| "unknown".to_string());
            Some(TypeInfo::LocalVar {
                name: name.as_str().to_string(),
                ty: ty_str,
            })
        }

        DefinitionSite::Statement(stmt_id) => {
            // Look up the binding type from inferred scope types.
            // We need to go from `StmtId` → `PatId` → binding type.
            //
            // Use the function body to find the statement and extract the pat id,
            // then look up the type from infer_scope_types for the enclosing scope.
            let body = baml_compiler2_hir::body::function_body(db, func_loc);
            let pat_id = body_stmt_to_pat_id(&body, stmt_id)?;

            // Get the scope containing at_offset (may be a nested block scope).
            // infer_scope_types is keyed by ScopeId. We need the function scope's
            // ScopeId to get the binding type, since bindings are stored per scope.
            let func_scope_id = index.scope_ids[enclosing_func_scope.index() as usize];
            let inference = baml_compiler2_tir::inference::infer_scope_types(db, func_scope_id);
            let ty_str = inference
                .binding_type(pat_id)
                .map(|ty| utils::display_ty(ty))
                .unwrap_or_else(|| {
                    // Try child scopes if the binding is in a nested block.
                    find_binding_ty_in_scopes(db, &index, pat_id)
                        .unwrap_or_else(|| "unknown".to_string())
                });

            Some(TypeInfo::LocalVar {
                name: name.as_str().to_string(),
                ty: ty_str,
            })
        }
    }
}

/// Extract the `PatId` for the binding introduced by `stmt_id`.
///
/// For `Stmt::Let { pattern, .. }` statements, returns the pattern ID.
/// Returns `None` for other statement kinds.
fn body_stmt_to_pat_id(
    body: &baml_compiler2_hir::body::FunctionBody,
    stmt_id: baml_compiler2_ast::StmtId,
) -> Option<baml_compiler2_ast::PatId> {
    use baml_compiler2_hir::body::FunctionBody;
    let FunctionBody::Expr(expr_body) = body else {
        return None;
    };
    let stmt = &expr_body.stmts[stmt_id];
    match stmt {
        baml_compiler2_ast::Stmt::Let { pattern, .. } => Some(*pattern),
        _ => None,
    }
}

/// Search all scopes in the file for the binding type of `pat_id`.
///
/// Used as a fallback when the let binding is in a nested block scope (not
/// directly in the enclosing function scope). Iterates all scope IDs in the
/// file index.
fn find_binding_ty_in_scopes(
    db: &dyn Db,
    index: &baml_compiler2_hir::semantic_index::FileSemanticIndex<'_>,
    pat_id: baml_compiler2_ast::PatId,
) -> Option<String> {
    for scope_id in &index.scope_ids {
        let inference = baml_compiler2_tir::inference::infer_scope_types(db, *scope_id);
        if let Some(ty) = inference.binding_type(pat_id) {
            return Some(utils::display_ty(ty));
        }
    }
    None
}
