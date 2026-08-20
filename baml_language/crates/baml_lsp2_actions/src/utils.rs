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

use baml_base::{Name, SourceFile};
use baml_compiler_syntax::{SyntaxToken, TokenAtOffset};
use baml_compiler2_ast::{TypeExpr, TypeExprKind};
use baml_compiler2_hir::{contributions::Definition, package::PackageItems};
use baml_type::{QualifiedTypeName, Ty, TyRenderStrategy, user_facing::humanize_type_string};
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

/// Context for the LSP's hover/completion type rendering: knows the file's
/// current package + namespace so qualified names collapse to the shortest
/// unambiguous form (bare when in scope, `root.path` when not, the dependency
/// package prefix for cross-package types). Implements [`TyRenderStrategy`] so
/// the structural walk lives once in `baml_type`.
struct TyDisplayContext<'db> {
    current_package: Name,
    current_namespace: Vec<Name>,
    package_items: &'db PackageItems<'db>,
    /// When set, collapse builtin companion classes to their lowercase
    /// primitive/keyword alias (`baml.String` → `string`, `baml.json.json` →
    /// `json`). Only the describe + hover + signature paths opt in (via
    /// [`display_ty_canonical_for_file`]); diagnostics/completions/inlay hints
    /// keep the un-collapsed spelling.
    collapse_aliases: bool,
}

impl TyDisplayContext<'_> {
    /// Every namespace decision below reads
    /// [`QualifiedTypeName::source_namespace`], not `namespace()`: a
    /// runtime-minted declaration carries a hidden `$dyn.<mint>` discriminator
    /// that keys its identity in the VM, and rendering it would show a `root.`
    /// path nobody can write. Below the discriminator the name is the one the
    /// source wrote, and that is what this path — hover, completions, describe,
    /// and the diagnostics a runtime compile hands back — must show.
    fn display_qtn(&self, qtn: &QualifiedTypeName) -> String {
        if self.collapse_aliases {
            if let Some(alias) = qtn.builtin_alias() {
                return alias.to_string();
            }
        }

        if qtn.package() != &self.current_package {
            // Cross-package: keep the dependency package prefix to disambiguate,
            // but never the implicit `user` package.
            return qtn.render_user_facing();
        }

        if self.can_use_bare_name(qtn) {
            return qtn.name().to_string();
        }

        let path = qtn
            .source_namespace()
            .iter()
            .chain(std::iter::once(qtn.name()))
            .map(Name::as_str)
            .collect::<Vec<_>>()
            .join(".");
        format!("root.{path}")
    }

    fn can_use_bare_name(&self, qtn: &QualifiedTypeName) -> bool {
        if qtn.source_namespace() == self.current_namespace {
            return true;
        }

        if qtn.source_namespace().is_empty() {
            return self
                .package_items
                .lookup_type(&self.current_namespace, qtn.name())
                .is_none();
        }

        false
    }

    /// Render `ty` in this file's context — the LSP hover/completion form.
    /// The structural walk lives once in `baml_type`; this context
    /// only supplies the name/policy decisions via [`TyRenderStrategy`].
    fn display_ty(&self, ty: &Ty) -> String {
        ty.render_with(self)
    }
}

impl TyRenderStrategy for TyDisplayContext<'_> {
    fn qtn(&self, qtn: &QualifiedTypeName) -> String {
        self.display_qtn(qtn)
    }

    fn type_var(&self, name: &Name) -> String {
        if baml_type::is_synthetic_effect_param(name) {
            "callback".to_string()
        } else {
            name.to_string()
        }
    }

    // Hover/completion hide the streaming-only `(evolving)` annotation.
    fn show_evolving(&self) -> bool {
        false
    }
}

/// Context-free strategy: like the canonical form but elides the implicit
/// `user` package, hides `(evolving)`, and shows synthetic effect params as
/// `callback`. Used by [`display_ty`] where no current-package context is
/// available.
struct PlainTyRender;

impl TyRenderStrategy for PlainTyRender {
    fn qtn(&self, qtn: &QualifiedTypeName) -> String {
        qtn.render_user_facing()
    }

    fn type_var(&self, name: &Name) -> String {
        if baml_type::is_synthetic_effect_param(name) {
            "callback".to_string()
        } else {
            name.to_string()
        }
    }

    fn show_evolving(&self) -> bool {
        false
    }
}

pub fn display_ty_for_file(db: &dyn Db, file: SourceFile, ty: &Ty) -> String {
    display_ty_for_file_impl(db, file, ty, false)
}

/// Like [`display_ty_for_file`], but collapses builtin companion classes to
/// their lowercase primitive/keyword alias (`baml.String` → `string`,
/// `baml.media.Image` → `image`, `baml.json.json` → `json`). This is the
/// canonical type printer used by the describe + hover + signature paths;
/// other call sites (diagnostics, completions, inlay hints) keep the
/// un-collapsed [`display_ty_for_file`].
pub fn display_ty_canonical_for_file(db: &dyn Db, file: SourceFile, ty: &Ty) -> String {
    display_ty_for_file_impl(db, file, ty, true)
}

fn display_ty_for_file_impl(
    db: &dyn Db,
    file: SourceFile,
    ty: &Ty,
    collapse_aliases: bool,
) -> String {
    let pkg_info = baml_compiler2_hir::file_package::file_package(db, file);
    let pkg_id = baml_compiler2_hir::package::PackageId::new(db, pkg_info.package.clone());
    let package_items = baml_compiler2_ppir::package_items(db, pkg_id);
    let ctx = TyDisplayContext {
        current_package: pkg_info.package,
        current_namespace: pkg_info.namespace_path,
        package_items,
        collapse_aliases,
    };
    ctx.display_ty(ty)
}

/// Canonical fully-qualified name string for a resolved type, used by the
/// describe header and the LSP hover "Run `baml describe …`" hint.
///
/// Rules (single source of truth, matching the canonical printer):
/// - builtin companion class with a lowercase alias → the alias (`string`);
/// - user type at package root → its bare name (`Foo`);
/// - user type in a namespace → `root.<ns>.<Name>`;
/// - other dependency type → `<pkg>.<path>` (`baml.json.JsonObject`).
///
/// A runtime-minted declaration reads through its
/// [`QualifiedTypeName::source_namespace`], so it renders as the name its
/// source wrote rather than the hidden discriminator that keys its identity.
pub fn canonical_fqn_string(qtn: &QualifiedTypeName) -> String {
    if let Some(alias) = qtn.builtin_alias() {
        return alias.to_string();
    }
    if qtn.is_local() {
        if qtn.source_namespace().is_empty() {
            qtn.name().to_string()
        } else {
            let path = qtn
                .source_namespace()
                .iter()
                .chain(std::iter::once(qtn.name()))
                .map(Name::as_str)
                .collect::<Vec<_>>()
                .join(".");
            format!("root.{path}")
        }
    } else {
        qtn.render_user_facing()
    }
}

/// Format a resolved `Ty` as a user-friendly string without file context.
///
/// Keeps the dependency package prefix so same-short-name types stay
/// distinguishable in hover output, but elides the implicit `user` package and
/// shows synthetic effect params as `callback`. Used where no current-package
/// context is available; with context, prefer [`display_ty_for_file`].
pub fn display_ty(ty: &Ty) -> String {
    ty.render_with(&PlainTyRender)
}

// ── display_type_expr ─────────────────────────────────────────────────────────

fn type_expr_needs_postfix_parens(te: &TypeExpr) -> bool {
    matches!(
        &te.kind,
        TypeExprKind::Union { .. } | TypeExprKind::Function { .. }
    )
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
    if matches!(&te.kind, TypeExprKind::Function { .. }) {
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
    let rendered = match &te.kind {
        TypeExprKind::Path { segments, .. } => {
            // Use only the last segment for brevity (e.g. `baml.Foo` → `Foo`).
            segments
                .last()
                .map(|n| n.as_str().to_string())
                .unwrap_or_else(|| "unknown".to_string())
        }
        TypeExprKind::AssociatedTypeProjection { .. } => te.to_string(),
        TypeExprKind::Int { .. } => "int".to_string(),
        TypeExprKind::Bigint { .. } => "bigint".to_string(),
        TypeExprKind::Float { .. } => "float".to_string(),
        TypeExprKind::String { .. } => "string".to_string(),
        TypeExprKind::Bool { .. } => "bool".to_string(),
        TypeExprKind::Null { .. } => "null".to_string(),
        TypeExprKind::Uint8Array { .. } => "uint8array".to_string(),
        TypeExprKind::Media { kind, .. } => format!("{kind:?}").to_lowercase(),
        TypeExprKind::Optional { inner, .. } => {
            format!("{}?", display_type_expr_as_postfix_base(inner))
        }
        TypeExprKind::List { inner, .. } => {
            format!("{}[]", display_type_expr_as_postfix_base(inner))
        }
        TypeExprKind::Map { key, value, .. } => {
            format!(
                "map<{}, {}>",
                display_type_expr(key),
                display_type_expr(value)
            )
        }
        TypeExprKind::Union { variants, .. } => {
            let parts: Vec<_> = variants.iter().map(display_type_expr).collect();
            parts.join(" | ")
        }
        TypeExprKind::Literal { value, .. } => value.to_string(),
        TypeExprKind::Function {
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
        TypeExprKind::BuiltinUnknown { .. } => "unknown".to_string(),
        TypeExprKind::Never { .. } => "never".to_string(),
        TypeExprKind::Void { .. } => "void".to_string(),
        TypeExprKind::Type { .. } => "reflect.Type".to_string(),
        TypeExprKind::Rust { .. } => "$rust_type".to_string(),
        TypeExprKind::Infer { .. } => "_".to_string(),
        TypeExprKind::Error { .. } | TypeExprKind::Unknown { .. } => "unknown".to_string(),
    };
    humanize_type_string(&rendered)
}

fn type_ref_needs_postfix_parens(
    store: &baml_compiler2_hir::type_ref::TypeRefStore,
    id: baml_compiler2_hir::type_ref::TypeRefId,
) -> bool {
    use baml_compiler2_hir::type_ref::TypeRefKind;
    matches!(
        store[id].kind,
        TypeRefKind::Union { .. } | TypeRefKind::Function { .. }
    )
}

fn display_type_ref_as_postfix_base(
    store: &baml_compiler2_hir::type_ref::TypeRefStore,
    id: baml_compiler2_hir::type_ref::TypeRefId,
) -> String {
    let rendered = display_type_ref(store, id);
    if type_ref_needs_postfix_parens(store, id) {
        format!("({rendered})")
    } else {
        rendered
    }
}

fn display_type_ref_as_function_result(
    store: &baml_compiler2_hir::type_ref::TypeRefStore,
    id: baml_compiler2_hir::type_ref::TypeRefId,
) -> String {
    use baml_compiler2_hir::type_ref::TypeRefKind;
    let rendered = display_type_ref(store, id);
    if matches!(store[id].kind, TypeRefKind::Function { .. }) {
        format!("({rendered})")
    } else {
        rendered
    }
}

/// The firewall-arena twin of [`display_type_expr`]: format a `TypeRef` as a
/// brief source-level type string (last path segment only, generics dropped),
/// arm-for-arm identical to [`display_type_expr`]. For callers holding firewall
/// data (`FunctionData::type_refs`, `InterfaceData::type_refs`, …) rather than
/// AST nodes. NOTE: this is deliberately NOT
/// [`TypeRefStore::display`](baml_compiler2_hir::type_ref::TypeRefStore::display),
/// which is the *full* `Display` form (whole path + generic args).
pub fn display_type_ref(
    store: &baml_compiler2_hir::type_ref::TypeRefStore,
    id: baml_compiler2_hir::type_ref::TypeRefId,
) -> String {
    use baml_compiler2_hir::type_ref::TypeRefKind as K;
    let rendered = match &store[id].kind {
        K::Path { segments, .. } => segments
            .last()
            .map(|n| n.as_str().to_string())
            .unwrap_or_else(|| "unknown".to_string()),
        // A projection has no brief form — render it fully, matching the AST path's
        // `te.to_string()`.
        K::AssociatedTypeProjection { .. } => store.display(id).to_string(),
        K::Int => "int".to_string(),
        K::Bigint => "bigint".to_string(),
        K::Float => "float".to_string(),
        K::String => "string".to_string(),
        K::Bool => "bool".to_string(),
        K::Null => "null".to_string(),
        K::Uint8Array => "uint8array".to_string(),
        K::Media { kind } => format!("{kind:?}").to_lowercase(),
        K::Optional { inner } => format!("{}?", display_type_ref_as_postfix_base(store, *inner)),
        K::List { inner } => format!("{}[]", display_type_ref_as_postfix_base(store, *inner)),
        K::Map { key, value } => format!(
            "map<{}, {}>",
            display_type_ref(store, *key),
            display_type_ref(store, *value)
        ),
        K::Union { variants } => variants
            .iter()
            .map(|&v| display_type_ref(store, v))
            .collect::<Vec<_>>()
            .join(" | "),
        K::Literal { value } => value.to_string(),
        K::Function {
            params,
            ret,
            throws,
        } => {
            let ps: Vec<String> = params
                .iter()
                .map(|p| {
                    p.name
                        .as_ref()
                        .map(|n| {
                            let optional = if p.optional { "?" } else { "" };
                            format!("{}{}: {}", n, optional, display_type_ref(store, p.ty))
                        })
                        .unwrap_or_else(|| display_type_ref(store, p.ty))
                })
                .collect();
            let throws = throws
                .map(|t| display_type_ref(store, t))
                .map(|throws| format!(" throws {throws}"))
                .unwrap_or_default();
            format!(
                "({}) -> {}{}",
                ps.join(", "),
                display_type_ref_as_function_result(store, *ret),
                throws
            )
        }
        K::BuiltinUnknown => "unknown".to_string(),
        K::Never => "never".to_string(),
        K::Void => "void".to_string(),
        K::Type => "reflect.Type".to_string(),
        K::Rust => "$rust_type".to_string(),
        K::Infer => "_".to_string(),
        K::Error | K::Unknown => "unknown".to_string(),
    };
    humanize_type_string(&rendered)
}

// ── function_at_scope_range ───────────────────────────────────────────────────

/// Resolve the function whose declaration span equals `range`, preferring the
/// user-authored declaration.
///
/// Span equality alone is ambiguous: an LLM function's synthesized companions
/// (`{parent}$parse`, `{parent}$stream`, …) share their parent's declaration
/// span, so every companion matches too. Picking by origin makes the
/// user-authored preference explicit instead of leaning on the enumeration's
/// name tiebreak (a companion's `{parent}$…` name happens to sort after its
/// parent's, but that is a naming convention, not a contract).
pub(crate) fn function_at_scope_range(
    db: &dyn crate::Db,
    file: SourceFile,
    range: TextRange,
) -> Option<baml_compiler2_hir::loc::FunctionLoc<'_>> {
    use baml_compiler2_ast::ast::FunctionOrigin;
    baml_compiler2_ppir::item_data::file_functions(db, file)
        .iter()
        .copied()
        .filter(|&loc| baml_compiler2_ppir::item_data::function_source_map(db, loc).span == range)
        .min_by_key(|&loc| {
            match baml_compiler2_ppir::item_data::function_data(db, loc)
                .metadata
                .origin
            {
                FunctionOrigin::UserDefined => 0u8,
                FunctionOrigin::Companion => 1,
                FunctionOrigin::Internal => 2,
                FunctionOrigin::AutoDerive => 3,
            }
        })
}
