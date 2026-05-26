//! `check_file` — aggregate parse + HIR + TIR diagnostics for a single file.
//!
//! This is NOT a Salsa query — it is a regular function that calls cached
//! Salsa queries beneath it and aggregates their results into a
//! `Vec<Diagnostic>` ready for the LSP layer to convert into LSP types.
//!
//! ## Pipeline
//!
//! 1. **Parse errors** — via `baml_compiler_parser::parse_errors`. Always fast
//!    because parsing is Salsa-cached per file.
//! 2. **HIR2 diagnostics** — stored in `file_semantic_index(...).extra`. These
//!    cover duplicate field/variant/binding names found during scope tree
//!    construction.
//! 3. **TIR2 scope diagnostics** — via `render_scope_diagnostics(db, scope_id)`
//!    for each scope. These cover type mismatches, unresolved names, etc. in
//!    expression-body functions. Calls `infer_scope_types` (Salsa-cached per
//!    scope) internally.
//! 4. **TIR2 structural diagnostics** — type errors in class field annotations
//!    and type alias bodies, via `resolve_class_fields` and `resolve_type_alias`
//!    (both Salsa-cached per item).

use std::{collections::HashSet, fmt::Write as _};

use baml_base::{FileId, Name, SourceFile, Span};
use baml_compiler_diagnostics::{Diagnostic, DiagnosticId, DiagnosticPhase, ToDiagnostic};
use baml_compiler2_hir::{body::FunctionBody, file_semantic_index, scope::ScopeKind};
use baml_compiler2_tir::{
    infer_context::{DiagnosticLocation, TirTypeError},
    inference::render_scope_diagnostics,
    ty::{QualifiedTypeName, Ty, TyAttr},
};
use indexmap::IndexMap;
use text_size::{TextRange, TextSize};

use crate::Db;

/// Collect all compiler2 diagnostics for a file (parse + HIR2 + TIR2).
///
/// Returns a flat `Vec<Diagnostic>` in source order (parse first, then HIR,
/// then TIR). The LSP layer converts these to `lsp_types::Diagnostic` values.
///
/// This is a regular function, not a Salsa query. Caching happens at the
/// underlying query layers (parsing, HIR indexing, type inference).
pub fn check_file(db: &dyn Db, file: SourceFile) -> Vec<Diagnostic> {
    let file_id = file.file_id(db);
    let mut diagnostics: Vec<Diagnostic> = Vec::new();

    // ── 1. Parse errors ───────────────────────────────────────────────────────
    //
    // `parse_errors` is Salsa-cached per file. Calling it here is cheap after
    // the first call for a given file revision.
    let parse_errors = baml_compiler_parser::parse_errors(db, file);
    for err in &parse_errors {
        diagnostics.push(err.to_diagnostic());
    }

    // ── 2. HIR2 diagnostics ───────────────────────────────────────────────────
    //
    // `file_semantic_index` is Salsa-tracked with `no_eq` (re-runs on every
    // file change). HIR2 diagnostics live in the optional `extra` box — we only
    // pay for iteration when there are diagnostics.
    let index = file_semantic_index(db, file);
    if let Some(extra) = &index.extra {
        // 2a. Lowering diagnostics (CST → AST structural errors)
        for ld in &extra.lowering_diagnostics {
            diagnostics.push(ld.to_diagnostic(file_id));
        }
        // 2b. HIR2 semantic diagnostics (duplicate definitions, etc.)
        for hir_diag in &extra.diagnostics {
            diagnostics.push(hir_diag.to_diagnostic(file_id));
        }
    }

    // ── 3. TIR2 scope diagnostics ─────────────────────────────────────────────
    //
    // `render_scope_diagnostics` calls `infer_scope_types(db, scope_id)` (Salsa-
    // cached per scope) and resolves the arena IDs in each diagnostic to source
    // `TextRange` values via the function body's `AstSourceMap`.
    for scope_id in &index.scope_ids {
        let rendered = render_scope_diagnostics(db, *scope_id);
        for r in rendered {
            diagnostics.push(tir_rendered_to_diagnostic(r, file_id));
        }
    }

    // ── 4. TIR2 structural diagnostics ───────────────────────────────────────
    //
    // Type errors in class field annotations and type alias bodies. These are
    // produced by `resolve_class_fields` and `resolve_type_alias` (both Salsa-
    // cached per item), which already store `TextRange` in their diagnostics —
    // no source map lookup needed here.
    for (_name, contrib) in &index.symbol_contributions.types {
        use baml_compiler2_hir::contributions::Definition;
        match contrib.definition {
            Definition::Class(class_loc) => {
                let resolved = baml_compiler2_tir::inference::resolve_class_fields(db, class_loc);
                for (error, span) in &resolved.diagnostics {
                    diagnostics.push(
                        Diagnostic::error(
                            tir_type_error_to_diagnostic_id(error),
                            error.to_string(),
                        )
                        .with_primary_span(Span {
                            file_id,
                            range: *span,
                        })
                        .with_phase(DiagnosticPhase::Type),
                    );
                }
            }
            Definition::TypeAlias(alias_loc) => {
                let resolved = baml_compiler2_tir::inference::resolve_type_alias(db, alias_loc);
                for (error, span) in &resolved.diagnostics {
                    diagnostics.push(
                        Diagnostic::error(
                            tir_type_error_to_diagnostic_id(error),
                            error.to_string(),
                        )
                        .with_primary_span(Span {
                            file_id,
                            range: *span,
                        })
                        .with_phase(DiagnosticPhase::Type),
                    );
                }
            }
            _ => {}
        }
    }

    let item_tree = baml_compiler2_hir::file_item_tree(db, file);
    let pkg_info = baml_compiler2_hir::file_package::file_package(db, file);
    let pkg_id = baml_compiler2_hir::package::PackageId::new(db, pkg_info.package.clone());
    let res_ctx = baml_compiler2_tir::package_interface::package_resolution_context(db, pkg_id);
    let pkg_items = &res_ctx.own_items;
    let aliases = collect_type_aliases_for_resolution_context(db, res_ctx);
    let ast_items = {
        let tree = baml_compiler_parser::syntax_tree(db, file);
        let (items, _, _) = baml_compiler2_ast::lower_file_with_file_id(&tree, file_id);
        items
    };

    // ── 5. Jinja prompt/template diagnostics ────────────────────────────────
    //
    // Declarative LLM prompts and template_strings are MiniJinja templates, not
    // regular expression bodies. Validate them with the shared MiniJinja AST
    // type checker so prompt diagnostics match runtime template semantics.
    let source_text = file.text(db);
    diagnostics.extend(check_jinja_templates(
        db,
        file_id,
        &item_tree,
        pkg_items,
        &pkg_info.namespace_path,
        source_text,
    ));

    // ── 6. Function signature diagnostics ────────────────────────────────────
    //
    // Build a method → enclosing class list so we can merge class generic params.
    let mut method_to_class = Vec::new();
    for (class_id, class_data) in &item_tree.classes {
        for &method_id in &class_data.methods {
            method_to_class.push((method_id, *class_id));
        }
    }

    for (local_id, func_data) in &item_tree.functions {
        let func_loc = baml_compiler2_hir::loc::FunctionLoc::new(db, file, *local_id);
        let body = baml_compiler2_hir::body::function_body(db, func_loc);

        // Expression-body functions already have their signatures checked
        // during scope inference (step 3). Only check non-expr bodies here.
        if matches!(body.as_ref(), FunctionBody::Expr(_)) {
            continue;
        }

        let sig = baml_compiler2_hir::signature::function_signature(db, func_loc);
        let mut type_errors = Vec::new();
        let mut param_types = Vec::new();

        // Compute the effective generic params: method params + enclosing class params.
        let mut generic_params = func_data.generic_params.clone();
        let enclosing_class_id = method_to_class
            .iter()
            .find(|(mid, _)| mid == local_id)
            .map(|(_, class_id)| *class_id);
        if let Some(class_id) = enclosing_class_id {
            let class_data = &item_tree[class_id];
            // Prepend class generic params (class params come first, method params after)
            let mut merged = class_data.generic_params.clone();
            merged.extend(generic_params);
            generic_params = merged;
        }

        // Check return type — use the span from the item tree's SpannedTypeExpr.
        if let Some(ret_te) = &sig.return_type {
            baml_compiler2_tir::lower_type_expr::lower_type_expr_in_ns(
                db,
                ret_te,
                pkg_items,
                &pkg_info.namespace_path,
                &generic_params,
                &mut type_errors,
            );
            if !type_errors.is_empty() {
                if let Some(ret_spanned) = &func_data.return_type {
                    for error in type_errors.drain(..) {
                        diagnostics.push(
                            Diagnostic::error(
                                tir_type_error_to_diagnostic_id(&error),
                                error.to_string(),
                            )
                            .with_primary_span(Span {
                                file_id,
                                range: ret_spanned.span,
                            })
                            .with_phase(DiagnosticPhase::Type),
                        );
                    }
                }
            }
        }

        // Check parameter types — use the type_expr span, not the whole param span.
        for (i, param) in sig.params.iter().enumerate() {
            type_errors.clear();
            let param_ty = if param.name.as_str() == "self"
                && matches!(param.ty, baml_compiler2_ast::TypeExpr::Unknown { .. })
            {
                enclosing_class_id
                    .as_ref()
                    .and_then(|class_id| {
                        let class_data = &item_tree[*class_id];
                        pkg_items
                            .lookup_type(&pkg_info.namespace_path, &class_data.name)
                            .map(|def| {
                                Ty::Class(
                                    baml_compiler2_tir::lower_type_expr::qualify_def(
                                        db,
                                        def,
                                        &class_data.name,
                                    ),
                                    vec![],
                                    TyAttr::default(),
                                )
                            })
                    })
                    .unwrap_or(Ty::Unknown {
                        attr: TyAttr::default(),
                    })
            } else {
                baml_compiler2_tir::lower_type_expr::lower_type_expr_in_ns(
                    db,
                    &param.ty,
                    pkg_items,
                    &pkg_info.namespace_path,
                    &generic_params,
                    &mut type_errors,
                )
            };
            if !type_errors.is_empty() {
                if let Some(param) = func_data.params.get(i) {
                    if let Some(type_spanned) = &param.type_expr {
                        for error in type_errors.drain(..) {
                            diagnostics.push(
                                Diagnostic::error(
                                    tir_type_error_to_diagnostic_id(&error),
                                    error.to_string(),
                                )
                                .with_primary_span(Span {
                                    file_id,
                                    range: type_spanned.span,
                                })
                                .with_phase(DiagnosticPhase::Type),
                            );
                        }
                    }
                }
            }
            param_types.push((param.name.clone(), param_ty));
        }

        if let Some(scope_id) = function_scope_id(index, func_data) {
            let context = baml_compiler2_tir::infer_context::InferContext::new(db, scope_id);
            let mut builder = baml_compiler2_tir::builder::TypeInferenceBuilder::new(
                context,
                res_ctx,
                pkg_id,
                scope_id,
                aliases.clone(),
            );
            builder.set_generic_params(generic_params);
            for (name, ty) in &param_types {
                builder.add_local(name.clone(), ty.clone());
                builder.param_types.push((name.clone(), ty.clone()));
            }
            let parameter_defaults =
                baml_compiler2_hir::signature::function_parameter_defaults(db, func_loc);
            builder.check_function_parameter_defaults(
                &func_data.params,
                &parameter_defaults,
                &param_types,
            );

            let (
                _expressions,
                _pattern_types,
                _resolutions,
                _catch_residual_throws,
                _exhaustive_matches,
                type_check_diagnostics,
                _path_root_types,
                _path_segment_types,
                _path_member_resolutions,
                _param_types,
                _call_plans,
                _function_coercions,
                _call_throws,
                _default_parameter_inference,
            ) = builder.finish();
            for tir_diag in type_check_diagnostics.diagnostics {
                if !is_function_default_signature_diagnostic(&tir_diag) {
                    continue;
                }
                diagnostics.push(tir_rendered_to_diagnostic(
                    tir_diag.render(db, file, None),
                    file_id,
                ));
            }
        }
    }

    // ── 7. Interface validation (BEP-044) ────────────────────────────────────
    //
    // Structural / semantic checks for `interface I { ... }` declarations and
    // `implements I { ... }` blocks. Runs over the AST and the package items
    // so cross-file interface references work.
    diagnostics.extend(check_interfaces(
        db,
        file_id,
        &ast_items,
        pkg_items,
        &pkg_info.namespace_path,
        &aliases,
    ));

    // Deduplicate: multiple steps can produce the same diagnostic (e.g. scope
    // inference + signature validation for the same unresolved return type).
    diagnostics.dedup_by(|a, b| {
        a.code() == b.code() && a.message == b.message && a.primary_span() == b.primary_span()
    });

    diagnostics
}

/// Validate `interface` and `implements` blocks for a single file.
///
/// Diagnostics emitted here cover:
/// - Cycle in interface `extends`.
/// - `implements I {}` references an unknown interface.
/// - Duplicate `implements I` blocks on the same class.
/// - Missing implementations of required interface methods.
/// - Method bodies in `implements I {}` that name something I doesn't declare.
/// - Field type mismatches between class and interface.
/// - Two interfaces requiring the same field with conflicting types.
fn check_interfaces<'db>(
    db: &'db dyn Db,
    file_id: FileId,
    items: &[baml_compiler2_ast::Item],
    pkg_items: &baml_compiler2_hir::package::PackageItems<'db>,
    namespace_path: &[Name],
    aliases: &std::collections::HashMap<QualifiedTypeName, Ty>,
) -> Vec<Diagnostic> {
    use baml_compiler2_hir::diagnostic::Hir2Diagnostic;

    let mut diagnostics = Vec::new();

    // Detect direct + transitive cycles in interface `extends`.
    for item in items {
        if let baml_compiler2_ast::Item::Interface(iface) = item
            && interface_has_cycle(db, iface, pkg_items, namespace_path)
        {
            diagnostics.push(
                Hir2Diagnostic::InterfaceExtendsCycle {
                    chain: vec![iface.name.clone()],
                    span: iface.name_span,
                }
                .to_diagnostic(file_id),
            );
        }
    }

    // Detect field conflicts in interface `extends` chains.
    for item in items {
        if let baml_compiler2_ast::Item::Interface(iface) = item
            && !iface.extends.is_empty()
            && !interface_has_cycle(db, iface, pkg_items, namespace_path)
        {
            validate_interface_extends_fields(
                db,
                file_id,
                iface,
                pkg_items,
                namespace_path,
                aliases,
                &mut diagnostics,
            );
        }
    }

    for item in items {
        if let baml_compiler2_ast::Item::Class(class) = item {
            validate_class_implements(
                db,
                file_id,
                class,
                pkg_items,
                namespace_path,
                aliases,
                &mut diagnostics,
            );
        }
    }

    for item in items {
        if let baml_compiler2_ast::Item::ImplementsFor(imp) = item {
            validate_implements_for(
                db,
                file_id,
                imp,
                items,
                pkg_items,
                namespace_path,
                aliases,
                &mut diagnostics,
            );
        }
    }

    for item in items {
        if let baml_compiler2_ast::Item::Interface(iface) = item {
            for method in &iface.default_methods {
                if let Some(ret) = &method.return_type
                    && type_expr_contains_self(&ret.expr)
                {
                    diagnostics.push(
                        Diagnostic::error(
                            DiagnosticId::TypeMismatch,
                            format!(
                                "default method `{}` on interface `{}` cannot return `Self`",
                                method.name, iface.name
                            ),
                        )
                        .with_primary(
                            Span {
                                file_id,
                                range: ret.span,
                            },
                            "`Self` return type is not allowed on interface default methods",
                        )
                        .with_phase(DiagnosticPhase::Hir),
                    );
                }
            }
        }
    }

    diagnostics
}

/// Resolve a `TypeExpr::Path` to an interface, by name, walking the package.
///
/// Returns `None` if the path doesn't name an interface (including: name
/// doesn't exist, or resolves to a class/enum/etc.).
fn resolve_interface_path<'db>(
    db: &'db dyn Db,
    target: &baml_compiler2_ast::TypeExpr,
    pkg_items: &baml_compiler2_hir::package::PackageItems<'db>,
    namespace_path: &[Name],
) -> Option<(
    baml_compiler2_hir::loc::InterfaceLoc<'db>,
    baml_compiler2_hir::item_tree::Interface,
)> {
    use baml_compiler2_ast::TypeExpr;
    use baml_compiler2_hir::contributions::Definition;

    let TypeExpr::Path { segments, .. } = target else {
        return None;
    };
    let (head, name) = match segments.split_last() {
        Some((last, head)) => (head, last.clone()),
        None => return None,
    };
    let lookup_ns: &[Name] = if head.is_empty() {
        namespace_path
    } else {
        head
    };
    let def = pkg_items.lookup_type(lookup_ns, &name)?;
    let Definition::Interface(loc) = def else {
        return None;
    };
    let item_tree = baml_compiler2_hir::file_item_tree(db, loc.file(db));
    let iface = item_tree.interfaces.get(&loc.id(db))?.clone();
    Some((loc, iface))
}

fn interface_has_cycle<'db>(
    db: &'db dyn Db,
    iface: &baml_compiler2_ast::InterfaceDef,
    pkg_items: &baml_compiler2_hir::package::PackageItems<'db>,
    namespace_path: &[Name],
) -> bool {
    use baml_compiler2_ast::TypeExpr;

    let self_probe = TypeExpr::Path {
        segments: vec![iface.name.clone()],
        generic_args: Vec::new(),
        attrs: Vec::new(),
    };
    let self_loc =
        resolve_interface_path(db, &self_probe, pkg_items, namespace_path).map(|(loc, _)| loc);
    let mut frontier: Vec<Vec<Name>> = iface
        .extends
        .iter()
        .filter_map(|p| match &p.expr {
            TypeExpr::Path { segments, .. } if !segments.is_empty() => Some(segments.clone()),
            _ => None,
        })
        .collect();
    let mut visited = HashSet::new();
    while let Some(path) = frontier.pop() {
        if !visited.insert(path.clone()) {
            continue;
        }
        let probe = TypeExpr::Path {
            segments: path,
            generic_args: Vec::new(),
            attrs: Vec::new(),
        };
        if let Some((parent_loc, parent_iface)) =
            resolve_interface_path(db, &probe, pkg_items, namespace_path)
        {
            if self_loc.as_ref().is_some_and(|loc| *loc == parent_loc) {
                return true;
            }
            for parent in &parent_iface.extends {
                if let TypeExpr::Path { segments, .. } = &parent.expr
                    && !segments.is_empty()
                {
                    frontier.push(segments.clone());
                }
            }
        }
    }
    false
}

/// A method signature in canonical string form, used by the interface
/// validator to compare class method overrides against the interface's
/// declared signature.
#[derive(Debug, Clone, PartialEq, Eq)]
struct MethodSignature {
    /// `(name, type)` pairs in declaration order. `self` is excluded — its
    /// type is the implementing class and so trivially matches.
    params: Vec<(Name, String)>,
    /// Rendered return type, or `"<unspecified>"` when missing.
    return_type: String,
    /// Rendered declared throws type. `None` means the signature did not
    /// declare `throws`; interface implementations must preserve that spelling.
    throws: Option<String>,
}

impl MethodSignature {
    fn from_params_and_return(
        params: &[baml_compiler2_ast::Param],
        return_type: Option<&baml_compiler2_ast::SpannedTypeExpr>,
        throws: Option<&baml_compiler2_ast::SpannedTypeExpr>,
    ) -> Self {
        Self::from_params_and_return_with_subst(
            params,
            return_type,
            throws,
            &std::collections::HashMap::new(),
        )
    }

    /// Like [`from_params_and_return`] but substitutes generic parameter
    /// references in the param/return/throws types using `subst` before
    /// stringifying. Used when comparing an `implements Container<int>`
    /// block against `interface Container<T>`: the interface signature is
    /// rebuilt with `T → int` so a concrete-typed override matches.
    fn from_params_and_return_with_subst(
        params: &[baml_compiler2_ast::Param],
        return_type: Option<&baml_compiler2_ast::SpannedTypeExpr>,
        throws: Option<&baml_compiler2_ast::SpannedTypeExpr>,
        subst: &std::collections::HashMap<Name, baml_compiler2_ast::TypeExpr>,
    ) -> Self {
        let params = params
            .iter()
            .filter(|p| p.name.as_str() != "self")
            .map(|p| {
                let ty_str = p
                    .type_expr
                    .as_ref()
                    .map(|te| substitute_type_vars(&te.expr, subst).to_string())
                    .unwrap_or_else(|| "<unknown>".to_string());
                (p.name.clone(), ty_str)
            })
            .collect();
        let return_type = return_type
            .map(|te| substitute_type_vars(&te.expr, subst).to_string())
            .unwrap_or_else(|| "<unspecified>".to_string());
        let throws = throws.map(|te| substitute_type_vars(&te.expr, subst).to_string());
        Self {
            params,
            return_type,
            throws,
        }
    }

    fn render(&self) -> String {
        let ps: Vec<String> = self
            .params
            .iter()
            .map(|(n, t)| format!("{n}: {t}"))
            .collect();
        let mut rendered = format!("({}) -> {}", ps.join(", "), self.return_type);
        if let Some(throws) = &self.throws {
            write!(rendered, " throws {throws}").expect("writing to String cannot fail");
        }
        rendered
    }
}

/// Substitute generic parameter references in a `TypeExpr`. A single-segment
/// `Path` whose segment matches a key in `subst` is replaced with the
/// corresponding `TypeExpr`. Containers (`List`, `Optional`, `Union`, etc.)
/// recurse so nested usages like `T[]` and `T?` substitute too.
fn substitute_type_vars(
    ty: &baml_compiler2_ast::TypeExpr,
    subst: &std::collections::HashMap<Name, baml_compiler2_ast::TypeExpr>,
) -> baml_compiler2_ast::TypeExpr {
    use baml_compiler2_ast::TypeExpr;
    if subst.is_empty() {
        return ty.clone();
    }
    match ty {
        TypeExpr::Path {
            segments,
            generic_args,
            attrs,
        } => {
            if segments.len() == 1
                && generic_args.is_empty()
                && let Some(replacement) = subst.get(&segments[0])
            {
                return replacement.clone();
            }
            TypeExpr::Path {
                segments: segments.clone(),
                generic_args: generic_args
                    .iter()
                    .map(|a| substitute_type_vars(a, subst))
                    .collect(),
                attrs: attrs.clone(),
            }
        }
        TypeExpr::List { inner, attrs } => TypeExpr::List {
            inner: Box::new(substitute_type_vars(inner, subst)),
            attrs: attrs.clone(),
        },
        TypeExpr::Optional { inner, attrs } => TypeExpr::Optional {
            inner: Box::new(substitute_type_vars(inner, subst)),
            attrs: attrs.clone(),
        },
        TypeExpr::Union { variants, attrs } => TypeExpr::Union {
            variants: variants
                .iter()
                .map(|m| substitute_type_vars(m, subst))
                .collect(),
            attrs: attrs.clone(),
        },
        TypeExpr::Map { key, value, attrs } => TypeExpr::Map {
            key: Box::new(substitute_type_vars(key, subst)),
            value: Box::new(substitute_type_vars(value, subst)),
            attrs: attrs.clone(),
        },
        TypeExpr::Function {
            params,
            ret,
            throws,
            attrs,
        } => TypeExpr::Function {
            params: params
                .iter()
                .map(|param| baml_compiler2_ast::FunctionTypeParam {
                    name: param.name.clone(),
                    optional: param.optional,
                    ty: substitute_type_vars(&param.ty, subst),
                })
                .collect(),
            ret: Box::new(substitute_type_vars(ret, subst)),
            throws: throws
                .as_ref()
                .map(|throws| Box::new(substitute_type_vars(throws, subst))),
            attrs: attrs.clone(),
        },
        _ => ty.clone(),
    }
}

#[derive(Debug, Default)]
struct InterfaceMembers {
    /// (origin interface name, field name, field type)
    fields: Vec<(Name, Name, Option<baml_compiler2_ast::SpannedTypeExpr>)>,
    /// (origin interface name, required method name, signature)
    required_methods: Vec<(InterfaceMemberOrigin, Name, MethodSignature)>,
    /// (origin interface name, default method name, signature)
    default_methods: Vec<(InterfaceMemberOrigin, Name, MethodSignature)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct InterfaceMemberOrigin {
    name: Name,
    qualified_name: QualifiedTypeName,
    type_args: Vec<baml_compiler2_ast::TypeExpr>,
    lowered_type_args: Vec<Ty>,
}

type InterfaceMemberStackEntry = (
    baml_compiler2_hir::item_tree::Interface,
    baml_base::SourceFile,
    std::collections::HashMap<Name, baml_compiler2_ast::TypeExpr>,
    Vec<baml_compiler2_ast::TypeExpr>,
    Vec<Name>,
);

fn interface_qtn_for_file(db: &dyn Db, file: SourceFile, name: &Name) -> QualifiedTypeName {
    let pkg_info = baml_compiler2_hir::file_package::file_package(db, file);
    QualifiedTypeName::new(pkg_info.package.clone(), pkg_info.namespace_path, name.clone())
}

fn lower_interface_origin_type_args(
    db: &dyn Db,
    file: SourceFile,
    type_args: &[baml_compiler2_ast::TypeExpr],
    generic_params: &[Name],
) -> Vec<Ty> {
    let pkg_info = baml_compiler2_hir::file_package::file_package(db, file);
    let pkg_id = baml_compiler2_hir::package::PackageId::new(db, pkg_info.package.clone());
    let pkg_items = baml_compiler2_hir::package::package_items(db, pkg_id);
    lower_interface_type_args_in_context(
        db,
        pkg_items,
        &pkg_info.namespace_path,
        type_args,
        generic_params,
    )
}

fn lower_interface_type_args_in_context(
    db: &dyn Db,
    pkg_items: &baml_compiler2_hir::package::PackageItems<'_>,
    namespace_path: &[Name],
    type_args: &[baml_compiler2_ast::TypeExpr],
    generic_params: &[Name],
) -> Vec<Ty> {
    let mut diags = Vec::new();
    type_args
        .iter()
        .map(|arg| {
            baml_compiler2_tir::lower_type_expr::lower_type_expr_in_ns(
                db,
                arg,
                pkg_items,
                namespace_path,
                generic_params,
                &mut diags,
            )
        })
        .collect()
}

/// Walk `extends` of `iface` (including `iface` itself) and gather all members
/// contributed up the chain. Methods are tagged with the interface they came
/// from so diagnostics can point at the right contract.
#[allow(dead_code)]
fn collect_interface_members<'db>(
    db: &'db dyn Db,
    iface: &baml_compiler2_hir::item_tree::Interface,
    iface_file: baml_base::SourceFile,
    pkg_items: &baml_compiler2_hir::package::PackageItems<'db>,
    namespace_path: &[Name],
) -> InterfaceMembers {
    collect_interface_members_with_subst(
        db,
        iface,
        iface_file,
        pkg_items,
        namespace_path,
        &std::collections::HashMap::new(),
        &[],
    )
}

/// Like [`collect_interface_members`] but applies a type-variable
/// substitution to every field, parameter, and return type. Used when an
/// `implements Container<int>` block needs the interface's `T`-typed
/// signatures rewritten to `int` before comparison.
fn collect_interface_members_with_subst<'db>(
    db: &'db dyn Db,
    iface: &baml_compiler2_hir::item_tree::Interface,
    iface_file: baml_base::SourceFile,
    pkg_items: &baml_compiler2_hir::package::PackageItems<'db>,
    namespace_path: &[Name],
    subst: &std::collections::HashMap<Name, baml_compiler2_ast::TypeExpr>,
    generic_params_in_scope: &[Name],
) -> InterfaceMembers {
    use baml_compiler2_ast::TypeExpr;

    let mut out = InterfaceMembers::default();
    let mut visited: HashSet<Vec<Name>> = HashSet::new();
    let root_type_args: Vec<baml_compiler2_ast::TypeExpr> = iface
        .generic_params
        .iter()
        .map(|param| {
            subst.get(param).cloned().unwrap_or_else(|| TypeExpr::Path {
                segments: vec![param.clone()],
                generic_args: Vec::new(),
                attrs: Vec::new(),
            })
        })
        .collect();
    let mut stack: Vec<InterfaceMemberStackEntry> = vec![(
        iface.clone(),
        iface_file,
        subst.clone(),
        root_type_args,
        {
            let mut params = iface.generic_params.clone();
            params.extend(generic_params_in_scope.iter().cloned());
            params
        },
    )];
    visited.insert(vec![iface.name.clone()]);

    while let Some((
        current,
        current_file,
        current_subst,
        current_type_args,
        generic_params_in_scope,
    )) = stack.pop()
    {
        let origin = InterfaceMemberOrigin {
            name: current.name.clone(),
            qualified_name: interface_qtn_for_file(db, current_file, &current.name),
            lowered_type_args: lower_interface_origin_type_args(
                db,
                current_file,
                &current_type_args,
                &generic_params_in_scope,
            ),
            type_args: current_type_args.clone(),
        };
        for field in &current.fields {
            let substituted =
                field
                    .type_expr
                    .as_ref()
                    .map(|te| baml_compiler2_ast::SpannedTypeExpr {
                        expr: substitute_type_vars(&te.expr, &current_subst),
                        span: te.span,
                    });
            out.fields
                .push((current.name.clone(), field.name.clone(), substituted));
        }
        for sig in &current.required_methods {
            // Convert the HIR `FunctionParam` list (no AST defaults) into a
            // canonical signature for comparison.
            let ast_params: Vec<baml_compiler2_ast::Param> = sig
                .params
                .iter()
                .map(|p| baml_compiler2_ast::Param {
                    name: p.name.clone(),
                    type_expr: p.type_expr.clone(),
                    default: None,
                    span: p.span,
                    name_span: p.span,
                })
                .collect();
            let signature = MethodSignature::from_params_and_return_with_subst(
                &ast_params,
                sig.return_type.as_ref(),
                sig.throws.as_ref(),
                &current_subst,
            );
            out.required_methods
                .push((origin.clone(), sig.name.clone(), signature));
        }
        // Default-method ids point into the same file's item tree as the
        // interface itself — fetch each function's name + signature from
        // there.
        let cur_tree = baml_compiler2_hir::file_item_tree(db, current_file);
        for fid in &current.default_methods {
            if let Some(f) = cur_tree.functions.get(fid) {
                let ast_params: Vec<baml_compiler2_ast::Param> = f
                    .params
                    .iter()
                    .map(|p| baml_compiler2_ast::Param {
                        name: p.name.clone(),
                        type_expr: p.type_expr.clone(),
                        default: None,
                        span: p.span,
                        name_span: p.span,
                    })
                    .collect();
                let signature = MethodSignature::from_params_and_return_with_subst(
                    &ast_params,
                    f.return_type.as_ref(),
                    f.throws.as_ref(),
                    &current_subst,
                );
                out.default_methods
                    .push((origin.clone(), f.name.clone(), signature));
            }
        }

        for parent_te in &current.extends {
            let TypeExpr::Path { segments, .. } = &parent_te.expr else {
                continue;
            };
            if segments.is_empty() {
                continue;
            }
            if !visited.insert(segments.clone()) {
                continue;
            }
            let probe = TypeExpr::Path {
                segments: segments.clone(),
                generic_args: Vec::new(),
                attrs: Vec::new(),
            };
            if let Some((loc, parent)) =
                resolve_interface_path(db, &probe, pkg_items, namespace_path)
            {
                let parent_args = match &parent_te.expr {
                    TypeExpr::Path { generic_args, .. } => generic_args
                        .iter()
                        .map(|arg| substitute_type_vars(arg, &current_subst))
                        .collect::<Vec<_>>(),
                    _ => Vec::new(),
                };
                let parent_subst = parent
                    .generic_params
                    .iter()
                    .zip(parent_args.iter())
                    .map(|(param, arg)| (param.clone(), arg.clone()))
                    .collect();
                let mut parent_generic_params = generic_params_in_scope.clone();
                parent_generic_params.extend(parent.generic_params.iter().cloned());
                stack.push((
                    parent,
                    loc.file(db),
                    parent_subst,
                    parent_args,
                    parent_generic_params,
                ));
            }
        }
    }

    out
}

/// Returns `true` if the type expression resolves to a type that exists
/// but is NOT an interface (e.g. a class or enum).
fn is_non_interface_type(
    target: &baml_compiler2_ast::TypeExpr,
    pkg_items: &baml_compiler2_hir::package::PackageItems<'_>,
    namespace_path: &[Name],
) -> bool {
    use baml_compiler2_ast::TypeExpr;
    use baml_compiler2_hir::contributions::Definition;

    let TypeExpr::Path { segments, .. } = target else {
        return false;
    };
    let Some((name, head)) = segments.split_last() else {
        return false;
    };
    let lookup_ns: &[Name] = if head.is_empty() {
        namespace_path
    } else {
        head
    };
    matches!(
        pkg_items.lookup_type(lookup_ns, name),
        Some(Definition::Class(_) | Definition::Enum(_))
    )
}

fn rendered_type_args(args: &[baml_compiler2_ast::TypeExpr]) -> Vec<String> {
    args.iter().map(ToString::to_string).collect()
}

fn type_args_from_target_expr(
    target: &baml_compiler2_ast::TypeExpr,
) -> Vec<baml_compiler2_ast::TypeExpr> {
    match target {
        baml_compiler2_ast::TypeExpr::Path { generic_args, .. } => generic_args.clone(),
        _ => Vec::new(),
    }
}

fn interface_origin_matches_target_expr<'db>(
    db: &'db dyn Db,
    target: &baml_compiler2_ast::TypeExpr,
    pkg_items: &baml_compiler2_hir::package::PackageItems<'db>,
    namespace_path: &[Name],
    generic_params: &[Name],
    aliases: &std::collections::HashMap<QualifiedTypeName, Ty>,
    origin: &InterfaceMemberOrigin,
) -> bool {
    let Some((loc, iface)) = resolve_interface_path(db, target, pkg_items, namespace_path) else {
        return false;
    };
    if interface_qtn_for_file(db, loc.file(db), &iface.name) != origin.qualified_name {
        return false;
    }
    let target_type_args = type_args_from_target_expr(target);
    if target_type_args.len() != origin.type_args.len() {
        return false;
    }
    let target_lowered = lower_interface_type_args_in_context(
        db,
        pkg_items,
        namespace_path,
        &target_type_args,
        generic_params,
    );
    if target_lowered.len() == origin.lowered_type_args.len()
        && target_lowered
            .iter()
            .zip(origin.lowered_type_args.iter())
            .all(|(target_arg, origin_arg)| {
                baml_compiler2_tir::normalize::is_same_normalized_type(
                    target_arg,
                    origin_arg,
                    aliases,
                )
            })
    {
        return true;
    }

    rendered_type_args(&target_type_args) == rendered_type_args(&origin.type_args)
}

fn implements_for_targets_match<'db>(
    db: &'db dyn Db,
    lhs: &baml_compiler2_ast::TypeExpr,
    rhs: &baml_compiler2_ast::TypeExpr,
    pkg_items: &baml_compiler2_hir::package::PackageItems<'db>,
    namespace_path: &[Name],
) -> bool {
    let mut lhs_diags = Vec::new();
    let mut rhs_diags = Vec::new();
    let lhs_ty = baml_compiler2_tir::lower_type_expr::lower_type_expr_in_ns(
        db,
        lhs,
        pkg_items,
        namespace_path,
        &[],
        &mut lhs_diags,
    );
    let rhs_ty = baml_compiler2_tir::lower_type_expr::lower_type_expr_in_ns(
        db,
        rhs,
        pkg_items,
        namespace_path,
        &[],
        &mut rhs_diags,
    );
    lhs_diags.is_empty() && rhs_diags.is_empty() && lhs_ty == rhs_ty
}

fn has_sibling_implements_for_origin<'db>(
    db: &'db dyn Db,
    current: &baml_compiler2_ast::ImplementsForDef,
    all_items: &[baml_compiler2_ast::Item],
    pkg_items: &baml_compiler2_hir::package::PackageItems<'db>,
    namespace_path: &[Name],
    aliases: &std::collections::HashMap<QualifiedTypeName, Ty>,
    origin: &InterfaceMemberOrigin,
) -> bool {
    all_items.iter().any(|item| {
        let baml_compiler2_ast::Item::ImplementsFor(candidate) = item else {
            return false;
        };
        if candidate.span == current.span {
            return false;
        }
        implements_for_targets_match(
            db,
            &candidate.for_target.expr,
            &current.for_target.expr,
            pkg_items,
            namespace_path,
        ) && interface_origin_matches_target_expr(
            db,
            &candidate.interface_target.expr,
            pkg_items,
            namespace_path,
            &[],
            aliases,
            origin,
        )
    })
}

fn validate_interface_extends_fields<'db>(
    db: &'db dyn Db,
    file_id: FileId,
    iface: &baml_compiler2_ast::InterfaceDef,
    pkg_items: &baml_compiler2_hir::package::PackageItems<'db>,
    namespace_path: &[Name],
    aliases: &std::collections::HashMap<QualifiedTypeName, Ty>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    use baml_compiler2_hir::diagnostic::Hir2Diagnostic;

    let mut seen: IndexMap<Name, (Name, baml_compiler2_ast::TypeExpr, String)> = IndexMap::new();

    // Seed with the interface's own fields.
    for field in &iface.fields {
        if let Some(te) = &field.type_expr {
            seen.insert(
                field.name.clone(),
                (iface.name.clone(), te.expr.clone(), format!("{}", te.expr)),
            );
        }
    }

    // Walk each parent via resolve and collect its members.
    for parent_te in &iface.extends {
        let Some((parent_loc, parent)) =
            resolve_interface_path(db, &parent_te.expr, pkg_items, namespace_path)
        else {
            continue;
        };
        let members = collect_interface_members_with_subst(
            db,
            &parent,
            parent_loc.file(db),
            pkg_items,
            namespace_path,
            &std::collections::HashMap::new(),
            &iface.generic_params,
        );
        for (origin, field_name, field_te) in &members.fields {
            let Some(field_te) = field_te else { continue };
            let ty_str = format!("{}", field_te.expr);
            if let Some((existing_origin, existing_ty, existing_rendered)) = seen.get(field_name) {
                if !type_exprs_compatible(
                    db,
                    pkg_items,
                    namespace_path,
                    &iface.generic_params,
                    existing_ty,
                    namespace_path,
                    &iface.generic_params,
                    &field_te.expr,
                    aliases,
                ) {
                    diagnostics.push(
                        Hir2Diagnostic::InterfaceExtendsFieldConflict {
                            interface_name: iface.name.clone(),
                            field_name: field_name.clone(),
                            first_interface: existing_origin.clone(),
                            first_type: existing_rendered.clone(),
                            second_interface: origin.clone(),
                            second_type: ty_str,
                            span: iface.name_span,
                        }
                        .to_diagnostic(file_id),
                    );
                }
            } else {
                seen.insert(
                    field_name.clone(),
                    (origin.clone(), field_te.expr.clone(), ty_str),
                );
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn validate_implements_for<'db>(
    db: &'db dyn Db,
    file_id: FileId,
    imp: &baml_compiler2_ast::ImplementsForDef,
    all_items: &[baml_compiler2_ast::Item],
    pkg_items: &baml_compiler2_hir::package::PackageItems<'db>,
    namespace_path: &[Name],
    aliases: &std::collections::HashMap<QualifiedTypeName, Ty>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    use baml_compiler2_hir::diagnostic::Hir2Diagnostic;

    let target_name = Name::new(format!("{}", imp.for_target.expr));
    let mut target_type_errors = Vec::new();
    baml_compiler2_tir::lower_type_expr::lower_type_expr_in_ns(
        db,
        &imp.for_target.expr,
        pkg_items,
        namespace_path,
        &[],
        &mut target_type_errors,
    );
    if !target_type_errors.is_empty() {
        for error in target_type_errors {
            diagnostics.push(
                Diagnostic::error(tir_type_error_to_diagnostic_id(&error), error.to_string())
                    .with_primary_span(Span {
                        file_id,
                        range: imp.for_target.span,
                    })
                    .with_phase(DiagnosticPhase::Type),
            );
        }
        return;
    }

    let Some((iface_loc, iface)) =
        resolve_interface_path(db, &imp.interface_target.expr, pkg_items, namespace_path)
    else {
        let is_non_interface =
            is_non_interface_type(&imp.interface_target.expr, pkg_items, namespace_path);
        if is_non_interface {
            diagnostics.push(
                Hir2Diagnostic::NotAnInterface {
                    class_name: target_name,
                    target_name: format!("{}", imp.interface_target.expr),
                    span: imp.interface_target.span,
                }
                .to_diagnostic(file_id),
            );
        } else {
            diagnostics.push(
                Hir2Diagnostic::UnknownInterface {
                    class_name: target_name,
                    target_name: format!("{}", imp.interface_target.expr),
                    span: imp.interface_target.span,
                }
                .to_diagnostic(file_id),
            );
        }
        return;
    };

    if !iface.fields.is_empty() {
        diagnostics.push(
            Hir2Diagnostic::OutOfBodyImplementsFieldInterface {
                target_name: target_name.to_string(),
                interface_name: iface.name,
                span: imp.interface_target.span,
            }
            .to_diagnostic(file_id),
        );
        return;
    }

    let iface_file = iface_loc.file(db);
    let subst: std::collections::HashMap<Name, baml_compiler2_ast::TypeExpr> =
        match &imp.interface_target.expr {
            baml_compiler2_ast::TypeExpr::Path { generic_args, .. } => iface
                .generic_params
                .iter()
                .zip(generic_args.iter())
                .map(|(p, a)| (p.clone(), a.clone()))
                .collect(),
            _ => std::collections::HashMap::new(),
        };
    let members = collect_interface_members_with_subst(
        db,
        &iface,
        iface_file,
        pkg_items,
        namespace_path,
        &subst,
        &[],
    );

    let mut provided_method_names: HashSet<Name> = HashSet::new();
    for method in &imp.methods {
        let expected_sig = members
            .required_methods
            .iter()
            .find_map(|(_, name, sig)| {
                if *name == method.name {
                    Some(sig.clone())
                } else {
                    None
                }
            })
            .or_else(|| {
                members.default_methods.iter().find_map(|(_, name, sig)| {
                    if *name == method.name {
                        Some(sig.clone())
                    } else {
                        None
                    }
                })
            });
        match expected_sig {
            None => diagnostics.push(
                Hir2Diagnostic::UnknownInterfaceMember {
                    interface_name: iface.name.clone(),
                    method_name: method.name.clone(),
                    span: method.name_span,
                }
                .to_diagnostic(file_id),
            ),
            Some(expected) => {
                let actual = MethodSignature::from_params_and_return(
                    &method.params,
                    method.return_type.as_ref(),
                    method.throws.as_ref(),
                );
                if actual != expected {
                    diagnostics.push(
                        Hir2Diagnostic::InterfaceMethodSignatureMismatch {
                            class_name: target_name.clone(),
                            interface_name: iface.name.clone(),
                            method_name: method.name.clone(),
                            actual: actual.render(),
                            expected: expected.render(),
                            span: method.name_span,
                        }
                        .to_diagnostic(file_id),
                    );
                }
            }
        }
        provided_method_names.insert(method.name.clone());
    }

    let iface_qtn = interface_qtn_for_file(db, iface_file, &iface.name);
    for (origin, req_name, _sig) in &members.required_methods {
        if provided_method_names.contains(req_name) {
            continue;
        }
        if origin.qualified_name != iface_qtn
            && has_sibling_implements_for_origin(
                db,
                imp,
                all_items,
                pkg_items,
                namespace_path,
                aliases,
                origin,
            )
        {
            continue;
        }
        diagnostics.push(
            Hir2Diagnostic::MissingInterfaceMethod {
                class_name: target_name.clone(),
                interface_name: origin.name.clone(),
                method_name: req_name.clone(),
                span: imp.span,
            }
            .to_diagnostic(file_id),
        );
    }
}

fn validate_class_implements<'db>(
    db: &'db dyn Db,
    file_id: FileId,
    class: &baml_compiler2_ast::ClassDef,
    pkg_items: &baml_compiler2_hir::package::PackageItems<'db>,
    namespace_path: &[Name],
    aliases: &std::collections::HashMap<QualifiedTypeName, Ty>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    use baml_compiler2_hir::diagnostic::Hir2Diagnostic;

    let mut seen_targets: IndexMap<String, (Name, Vec<TextRange>)> = IndexMap::new();
    let mut seen_target_names: HashSet<Name> = HashSet::new();
    for block in &class.implements {
        let Some((iface_loc, iface)) =
            resolve_interface_path(db, &block.target.expr, pkg_items, namespace_path)
        else {
            continue;
        };
        seen_target_names.insert(iface.name.clone());
        // Use the resolved interface identity plus its concrete type
        // arguments for duplicate detection. `Foo` and `ns.Foo` should
        // collide when they resolve to the same interface, but
        // `Converter<int>` and `Converter<float>` are distinct views.
        let type_arg_key = match &block.target.expr {
            baml_compiler2_ast::TypeExpr::Path { generic_args, .. } => generic_args
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(","),
            _ => String::new(),
        };
        let key = format!(
            "{}:{}<{}>",
            iface_loc.file(db).file_id(db).as_u32(),
            iface_loc.id(db).as_u32(),
            type_arg_key
        );
        seen_targets
            .entry(key)
            .or_insert_with(|| (iface.name.clone(), Vec::new()))
            .1
            .push(block.target.span);
    }
    for (_target, (name, sites)) in &seen_targets {
        if sites.len() > 1 {
            diagnostics.push(
                Hir2Diagnostic::DuplicateImplementsBlock {
                    class_name: class.name.clone(),
                    interface_name: name.clone(),
                    sites: sites.clone(),
                }
                .to_diagnostic(file_id),
            );
        }
    }

    // Hoisted: the class's own method set doesn't change across blocks.
    let class_method_names: HashSet<Name> = class.methods.iter().map(|m| m.name.clone()).collect();

    for block in &class.implements {
        let Some((iface_loc, iface)) =
            resolve_interface_path(db, &block.target.expr, pkg_items, namespace_path)
        else {
            // Distinguish "name doesn't exist" (E0112) from "name exists
            // but isn't an interface" (E0119).
            let is_non_interface =
                is_non_interface_type(&block.target.expr, pkg_items, namespace_path);
            if is_non_interface {
                diagnostics.push(
                    Hir2Diagnostic::NotAnInterface {
                        class_name: class.name.clone(),
                        target_name: format!("{}", block.target.expr),
                        span: block.target.span,
                    }
                    .to_diagnostic(file_id),
                );
            } else {
                diagnostics.push(
                    Hir2Diagnostic::UnknownInterface {
                        class_name: class.name.clone(),
                        target_name: format!("{}", block.target.expr),
                        span: block.target.span,
                    }
                    .to_diagnostic(file_id),
                );
            }
            continue;
        };
        let iface_file = iface_loc.file(db);
        let iface_namespace_path =
            baml_compiler2_hir::file_package::file_package(db, iface_file).namespace_path;
        let mut interface_generic_params = class.generic_params.clone();
        interface_generic_params.extend(iface.generic_params.clone());
        if block.is_out_of_body && !iface.fields.is_empty() {
            diagnostics.push(
                Hir2Diagnostic::OutOfBodyImplementsFieldInterface {
                    target_name: class.name.to_string(),
                    interface_name: iface.name.clone(),
                    span: block.target.span,
                }
                .to_diagnostic(file_id),
            );
            continue;
        }
        // Build a T → concrete-type substitution from the implements target's
        // generic args. `implements Container<int>` gives `{T → int}` so
        // signature comparisons see the concrete shape.
        let subst: std::collections::HashMap<Name, baml_compiler2_ast::TypeExpr> =
            match &block.target.expr {
                baml_compiler2_ast::TypeExpr::Path { generic_args, .. } => iface
                    .generic_params
                    .iter()
                    .zip(generic_args.iter())
                    .map(|(p, a)| (p.clone(), a.clone()))
                    .collect(),
                _ => std::collections::HashMap::new(),
            };
        let members = collect_interface_members_with_subst(
            db,
            &iface,
            iface_file,
            pkg_items,
            namespace_path,
            &subst,
            &class.generic_params,
        );

        // Check that every method declared in `implements I {}` actually
        // exists on `I` (required or default), and matches the interface's
        // declared signature.
        let mut provided_method_names: HashSet<Name> = HashSet::new();
        for m in &block.methods {
            let expected_sig = members
                .required_methods
                .iter()
                .find_map(|(_, n, s)| if *n == m.name { Some(s.clone()) } else { None })
                .or_else(|| {
                    members.default_methods.iter().find_map(|(_, n, s)| {
                        if *n == m.name { Some(s.clone()) } else { None }
                    })
                });
            match expected_sig {
                None => diagnostics.push(
                    Hir2Diagnostic::UnknownInterfaceMember {
                        interface_name: iface.name.clone(),
                        method_name: m.name.clone(),
                        span: m.name_span,
                    }
                    .to_diagnostic(file_id),
                ),
                Some(expected) => {
                    let actual = MethodSignature::from_params_and_return(
                        &m.params,
                        m.return_type.as_ref(),
                        m.throws.as_ref(),
                    );
                    if actual != expected {
                        diagnostics.push(
                            Hir2Diagnostic::InterfaceMethodSignatureMismatch {
                                class_name: class.name.clone(),
                                interface_name: iface.name.clone(),
                                method_name: m.name.clone(),
                                actual: actual.render(),
                                expected: expected.render(),
                                span: m.name_span,
                            }
                            .to_diagnostic(file_id),
                        );
                    }
                }
            }
            provided_method_names.insert(m.name.clone());
        }

        // Check that every required method has a body — either provided here,
        // by a same-named class method, or by a separate `implements` block
        // that targets the originating interface.
        let iface_qtn = interface_qtn_for_file(db, iface_file, &iface.name);
        for (origin, req_name, _sig) in &members.required_methods {
            if provided_method_names.contains(req_name) || class_method_names.contains(req_name) {
                continue;
            }
            // If the method originates from a parent interface that this
            // class explicitly implements in a separate block, skip the
            // check — that block is responsible for providing the method.
            if origin.qualified_name != iface_qtn
                && class.implements.iter().any(|candidate| {
                    interface_origin_matches_target_expr(
                        db,
                        &candidate.target.expr,
                        pkg_items,
                        namespace_path,
                        &class.generic_params,
                        aliases,
                        origin,
                    )
                })
            {
                continue;
            }
            diagnostics.push(
                Hir2Diagnostic::MissingInterfaceMethod {
                    class_name: class.name.clone(),
                    interface_name: origin.name.clone(),
                    method_name: req_name.clone(),
                    span: block.span,
                }
                .to_diagnostic(file_id),
            );
        }

        // BEP-044 v2: interface fields are satisfied by class fields. The
        // implements block may contain only explicit `field as class_field`
        // links; an absent link auto-links by exact field name.
        let class_fields: IndexMap<Name, &baml_compiler2_ast::FieldDef> =
            class.fields.iter().map(|f| (f.name.clone(), f)).collect();

        let own_fields: IndexMap<Name, Option<baml_compiler2_ast::SpannedTypeExpr>> = iface
            .fields
            .iter()
            .map(|f| {
                let substituted =
                    f.type_expr
                        .as_ref()
                        .map(|te| baml_compiler2_ast::SpannedTypeExpr {
                            expr: substitute_type_vars(&te.expr, &subst),
                            span: te.span,
                        });
                (f.name.clone(), substituted)
            })
            .collect();

        let mut link_sites: IndexMap<Name, Vec<TextRange>> = IndexMap::new();
        for link in &block.field_links {
            link_sites
                .entry(link.interface_field.clone())
                .or_default()
                .push(link.interface_field_span);
        }
        for (field_name, sites) in &link_sites {
            if sites.len() > 1 {
                diagnostics.push(
                    Hir2Diagnostic::DuplicateInterfaceFieldLink {
                        interface_name: iface.name.clone(),
                        field_name: field_name.clone(),
                        sites: sites.clone(),
                    }
                    .to_diagnostic(file_id),
                );
            }
        }

        let mut explicit_links: IndexMap<Name, &baml_compiler2_ast::InterfaceFieldLinkDef> =
            IndexMap::new();
        for link in &block.field_links {
            if !own_fields.contains_key(&link.interface_field) {
                diagnostics.push(
                    Hir2Diagnostic::UnknownInterfaceFieldLink {
                        interface_name: iface.name.clone(),
                        field_name: link.interface_field.clone(),
                        span: link.interface_field_span,
                    }
                    .to_diagnostic(file_id),
                );
                continue;
            }
            let Some(class_field) = class_fields.get(&link.class_field) else {
                diagnostics.push(
                    Hir2Diagnostic::UnknownClassFieldInInterfaceLink {
                        class_name: class.name.clone(),
                        interface_name: iface.name.clone(),
                        field_name: link.class_field.clone(),
                        span: link.class_field_span,
                    }
                    .to_diagnostic(file_id),
                );
                explicit_links.insert(link.interface_field.clone(), link);
                continue;
            };
            if let (Some(iface_te), Some(class_te)) = (
                own_fields
                    .get(&link.interface_field)
                    .and_then(std::option::Option::as_ref),
                class_field.type_expr.as_ref(),
            ) {
                if !type_exprs_compatible(
                    db,
                    pkg_items,
                    &iface_namespace_path,
                    &interface_generic_params,
                    &iface_te.expr,
                    namespace_path,
                    &class.generic_params,
                    &class_te.expr,
                    aliases,
                ) {
                    diagnostics.push(
                        Hir2Diagnostic::InterfaceFieldTypeMismatch {
                            class_name: class.name.clone(),
                            field_name: link.class_field.clone(),
                            interface_name: iface.name.clone(),
                            class_type: format!("{}", class_te.expr),
                            interface_type: format!("{}", iface_te.expr),
                            span: class_te.span,
                        }
                        .to_diagnostic(file_id),
                    );
                }
            }
            explicit_links
                .entry(link.interface_field.clone())
                .or_insert(link);
        }

        for (field_name, iface_te) in &own_fields {
            if explicit_links.contains_key(field_name) {
                continue;
            }
            let Some(class_field) = class_fields.get(field_name) else {
                diagnostics.push(
                    Hir2Diagnostic::MissingInterfaceField {
                        class_name: class.name.clone(),
                        interface_name: iface.name.clone(),
                        field_name: field_name.clone(),
                        span: block.span,
                    }
                    .to_diagnostic(file_id),
                );
                continue;
            };
            if let (Some(iface_te), Some(class_te)) =
                (iface_te.as_ref(), class_field.type_expr.as_ref())
            {
                if !type_exprs_compatible(
                    db,
                    pkg_items,
                    &iface_namespace_path,
                    &interface_generic_params,
                    &iface_te.expr,
                    namespace_path,
                    &class.generic_params,
                    &class_te.expr,
                    aliases,
                ) {
                    diagnostics.push(
                        Hir2Diagnostic::InterfaceFieldTypeMismatch {
                            class_name: class.name.clone(),
                            field_name: class_field.name.clone(),
                            interface_name: iface.name.clone(),
                            class_type: format!("{}", class_te.expr),
                            interface_type: format!("{}", iface_te.expr),
                            span: class_te.span,
                        }
                        .to_diagnostic(file_id),
                    );
                }
            }
        }

        // E0125: check that all `requires` parents are explicitly
        // implemented by this class.
        if !iface.extends.is_empty() {
            let missing: Vec<Name> = iface
                .extends
                .iter()
                .filter_map(|parent_te| {
                    let baml_compiler2_ast::TypeExpr::Path { segments, .. } = &parent_te.expr
                    else {
                        return None;
                    };
                    let parent_name = segments.last()?;
                    let class_implements_it = seen_target_names.contains(parent_name);
                    if class_implements_it {
                        None
                    } else {
                        Some(parent_name.clone())
                    }
                })
                .collect();
            if !missing.is_empty() {
                diagnostics.push(
                    Hir2Diagnostic::MissingRequiredInterface {
                        class_name: class.name.clone(),
                        interface_name: iface.name.clone(),
                        missing_parents: missing,
                        span: block.target.span,
                    }
                    .to_diagnostic(file_id),
                );
            }
        }
    }

    // BEP-044 §"Method Disambiguation": same-named methods declared in
    // two `implements` blocks are NOT a class-level error. The class
    // compiles; the ambiguity surfaces at the call site instead — see
    // `resolve_member` in TIR for the unqualified-call diagnostic.
}

/// Invariant compatibility check for interface fields.
///
/// Interface fields are writeable through the interface view, so the class
/// storage type must match the interface field type exactly. The exactness
/// rule lives in TIR normalization so LSP and compiler diagnostics agree on
/// semantic equality without permitting assignment subtyping.
#[allow(clippy::too_many_arguments)]
fn type_exprs_compatible(
    db: &dyn Db,
    pkg_items: &baml_compiler2_hir::package::PackageItems<'_>,
    lhs_namespace_path: &[Name],
    lhs_generic_params: &[Name],
    lhs: &baml_compiler2_ast::TypeExpr,
    rhs_namespace_path: &[Name],
    rhs_generic_params: &[Name],
    rhs: &baml_compiler2_ast::TypeExpr,
    aliases: &std::collections::HashMap<QualifiedTypeName, Ty>,
) -> bool {
    let mut diagnostics = Vec::new();
    let lhs_ty = baml_compiler2_tir::lower_type_expr::lower_type_expr_in_ns(
        db,
        lhs,
        pkg_items,
        lhs_namespace_path,
        lhs_generic_params,
        &mut diagnostics,
    );
    let lhs_lowered = diagnostics.is_empty();
    diagnostics.clear();
    let rhs_ty = baml_compiler2_tir::lower_type_expr::lower_type_expr_in_ns(
        db,
        rhs,
        pkg_items,
        rhs_namespace_path,
        rhs_generic_params,
        &mut diagnostics,
    );

    if !lhs_lowered || !diagnostics.is_empty() {
        // Resolution diagnostics are emitted elsewhere. Avoid inventing a
        // misleading interface-field mismatch on partially unresolved types.
        return lhs.to_string() == rhs.to_string();
    }

    baml_compiler2_tir::normalize::is_same_normalized_type(&lhs_ty, &rhs_ty, aliases)
}

fn type_expr_contains_self(expr: &baml_compiler2_ast::TypeExpr) -> bool {
    use baml_compiler2_ast::TypeExpr;
    match expr {
        TypeExpr::Path {
            segments,
            generic_args,
            ..
        } => {
            segments.iter().any(|s| s.as_str() == "Self")
                || generic_args.iter().any(type_expr_contains_self)
        }
        TypeExpr::Optional { inner, .. } | TypeExpr::List { inner, .. } => {
            type_expr_contains_self(inner)
        }
        TypeExpr::Map { key, value, .. } => {
            type_expr_contains_self(key) || type_expr_contains_self(value)
        }
        TypeExpr::Union { variants, .. } => variants.iter().any(type_expr_contains_self),
        TypeExpr::Function {
            params,
            ret,
            throws,
            ..
        } => {
            params.iter().any(|p| type_expr_contains_self(&p.ty))
                || type_expr_contains_self(ret)
                || throws
                    .as_ref()
                    .is_some_and(|throws| type_expr_contains_self(throws))
        }
        _ => false,
    }
}

fn check_jinja_templates(
    db: &dyn Db,
    file_id: FileId,
    item_tree: &baml_compiler2_hir::item_tree::ItemTree,
    pkg_items: &baml_compiler2_hir::package::PackageItems<'_>,
    namespace_path: &[Name],
    source_text: &str,
) -> Vec<Diagnostic> {
    let base_types = build_jinja_types(db, pkg_items, namespace_path);
    let mut diagnostics = Vec::new();

    for func_data in item_tree.functions.values() {
        diagnostics.extend(check_llm_prompt_template(
            db,
            file_id,
            func_data,
            pkg_items,
            namespace_path,
            &base_types,
            source_text,
        ));
    }

    for template in item_tree.template_strings.values() {
        let Some(body) = &template.body else {
            continue;
        };

        let mut types = base_types.clone();
        types.start_scope();
        for param in &template.params {
            let ty = param
                .type_expr
                .as_ref()
                .map(|type_expr| {
                    jinja_type_from_type_expr(db, &type_expr.expr, pkg_items, namespace_path)
                })
                .unwrap_or(sys_jinja_types::Type::Unknown);
            types.add_variable(param.name.as_str(), ty);
        }

        let range_hint = template.span;
        diagnostics.extend(render_jinja_validation_result(
            file_id,
            source_text,
            range_hint,
            body,
            sys_jinja_types::validate_template(template.name.as_str(), body, &mut types),
        ));
    }

    diagnostics
}

fn check_llm_prompt_template(
    db: &dyn Db,
    file_id: FileId,
    func_data: &baml_compiler2_hir::item_tree::Function,
    pkg_items: &baml_compiler2_hir::package::PackageItems<'_>,
    namespace_path: &[Name],
    base_types: &sys_jinja_types::PredefinedTypes,
    source_text: &str,
) -> Vec<Diagnostic> {
    let Some(baml_compiler2_ast::DeclarativeMeta::Llm(llm)) = &func_data.declarative_meta else {
        return Vec::new();
    };
    let Some(prompt) = &llm.prompt else {
        return Vec::new();
    };

    let mut types = base_types.clone();
    types.start_scope();
    for param in &func_data.params {
        let ty = param
            .type_expr
            .as_ref()
            .map(|type_expr| {
                jinja_type_from_type_expr(db, &type_expr.expr, pkg_items, namespace_path)
            })
            .unwrap_or(sys_jinja_types::Type::Unknown);
        types.add_variable(param.name.as_str(), ty);
    }

    render_jinja_validation_result(
        file_id,
        source_text,
        prompt.span,
        &prompt.text,
        sys_jinja_types::validate_template(func_data.name.as_str(), &prompt.text, &mut types),
    )
}

fn build_jinja_types(
    db: &dyn Db,
    pkg_items: &baml_compiler2_hir::package::PackageItems<'_>,
    namespace_path: &[Name],
) -> sys_jinja_types::PredefinedTypes {
    use baml_compiler2_hir::contributions::Definition;

    let mut types =
        sys_jinja_types::PredefinedTypes::default(sys_jinja_types::JinjaContext::Prompt);
    types.add_variable("baml", sys_jinja_types::Type::Unknown);

    let Some(ns_items) = pkg_items.namespaces.get(namespace_path) else {
        return types;
    };

    for def in ns_items.types.values() {
        let Definition::Class(class_loc) = *def else {
            continue;
        };
        let file = class_loc.file(db);
        let item_tree = baml_compiler2_ppir::file_item_tree(db, file);
        let class_data = &item_tree[class_loc.id(db)];
        let class_namespace =
            baml_compiler2_hir::file_package::file_package(db, file).namespace_path;
        let fields = class_data
            .fields
            .iter()
            .map(|field| {
                let ty = field
                    .type_expr
                    .as_ref()
                    .map(|type_expr| {
                        jinja_type_from_type_expr(db, &type_expr.expr, pkg_items, &class_namespace)
                    })
                    .unwrap_or(sys_jinja_types::Type::Unknown);
                (field.name.to_string(), ty)
            })
            .collect::<IndexMap<_, _>>();
        types.add_class(class_data.name.as_str(), fields);
    }

    for def in ns_items.types.values() {
        let Definition::Enum(enum_loc) = *def else {
            continue;
        };
        let item_tree = baml_compiler2_ppir::file_item_tree(db, enum_loc.file(db));
        let enum_data = &item_tree[enum_loc.id(db)];
        types.add_enum(
            enum_data.name.as_str(),
            enum_data
                .variants
                .iter()
                .map(|variant| variant.name.to_string())
                .collect(),
        );
    }

    for def in ns_items.types.values() {
        let Definition::TypeAlias(alias_loc) = *def else {
            continue;
        };
        let file = alias_loc.file(db);
        let item_tree = baml_compiler2_ppir::file_item_tree(db, file);
        let alias_data = &item_tree[alias_loc.id(db)];
        if let Some(type_expr) = &alias_data.type_expr {
            let alias_namespace =
                baml_compiler2_hir::file_package::file_package(db, file).namespace_path;
            types.add_alias(
                alias_data.name.as_str(),
                jinja_type_from_type_expr(db, &type_expr.expr, pkg_items, &alias_namespace),
            );
        }
    }

    for def in ns_items.values.values() {
        let Definition::TemplateString(template_loc) = *def else {
            continue;
        };
        let file = template_loc.file(db);
        let item_tree = baml_compiler2_ppir::file_item_tree(db, file);
        let template = &item_tree[template_loc.id(db)];
        let template_namespace =
            baml_compiler2_hir::file_package::file_package(db, file).namespace_path;
        let args = template
            .params
            .iter()
            .map(|param| {
                let ty = param
                    .type_expr
                    .as_ref()
                    .map(|type_expr| {
                        jinja_type_from_type_expr(
                            db,
                            &type_expr.expr,
                            pkg_items,
                            &template_namespace,
                        )
                    })
                    .unwrap_or(sys_jinja_types::Type::Unknown);
                (param.name.to_string(), ty)
            })
            .collect();
        types.add_function(template.name.as_str(), sys_jinja_types::Type::String, args);
    }

    types
}

fn jinja_type_from_type_expr(
    db: &dyn Db,
    type_expr: &baml_compiler2_ast::TypeExpr,
    pkg_items: &baml_compiler2_hir::package::PackageItems<'_>,
    namespace_path: &[Name],
) -> sys_jinja_types::Type {
    jinja_type_from_type_expr_inner(
        db,
        type_expr,
        pkg_items,
        namespace_path,
        &mut HashSet::new(),
    )
}

fn jinja_type_from_type_expr_inner(
    db: &dyn Db,
    type_expr: &baml_compiler2_ast::TypeExpr,
    pkg_items: &baml_compiler2_hir::package::PackageItems<'_>,
    namespace_path: &[Name],
    resolving_aliases: &mut HashSet<String>,
) -> sys_jinja_types::Type {
    use baml_compiler2_ast::TypeExpr;
    use baml_compiler2_hir::contributions::Definition;
    use sys_jinja_types::Type;

    match type_expr {
        TypeExpr::Int { .. } => Type::Int,
        TypeExpr::Float { .. } => Type::Float,
        TypeExpr::String { .. } => Type::String,
        TypeExpr::Bool { .. } => Type::Bool,
        TypeExpr::Null { .. } => Type::None,
        TypeExpr::Media { kind, .. } => match kind {
            baml_base::MediaKind::Image => Type::Image,
            baml_base::MediaKind::Audio => Type::Audio,
            _ => Type::Unknown,
        },
        TypeExpr::Literal { value, .. } => Type::Literal(value.clone()),
        TypeExpr::Optional { inner, .. } => Type::merge([
            Type::None,
            jinja_type_from_type_expr_inner(
                db,
                inner,
                pkg_items,
                namespace_path,
                resolving_aliases,
            ),
        ]),
        TypeExpr::List { inner, .. } => Type::List(Box::new(jinja_type_from_type_expr_inner(
            db,
            inner,
            pkg_items,
            namespace_path,
            resolving_aliases,
        ))),
        TypeExpr::Map { value, .. } => Type::Map(
            Box::new(Type::String),
            Box::new(jinja_type_from_type_expr_inner(
                db,
                value,
                pkg_items,
                namespace_path,
                resolving_aliases,
            )),
        ),
        TypeExpr::Union { variants, .. } => Type::merge(variants.iter().map(|variant| {
            jinja_type_from_type_expr_inner(
                db,
                variant,
                pkg_items,
                namespace_path,
                resolving_aliases,
            )
        })),
        TypeExpr::Path { segments, .. } if !segments.is_empty() => {
            let (lookup_namespace, name) = jinja_lookup_path(namespace_path, segments);
            let key = format!(
                "{}::{}",
                lookup_namespace
                    .iter()
                    .map(Name::as_str)
                    .collect::<Vec<_>>()
                    .join("::"),
                name
            );
            let name_obj = Name::new(name.as_str());
            match pkg_items.lookup_type(&lookup_namespace, &name_obj) {
                Some(Definition::Class(_)) => Type::ClassRef(name),
                Some(Definition::Enum(_)) => Type::EnumTypeRef(name),
                Some(Definition::TypeAlias(alias_loc)) => {
                    if !resolving_aliases.insert(key.clone()) {
                        return Type::RecursiveTypeAlias(name);
                    }
                    let file = alias_loc.file(db);
                    let item_tree = baml_compiler2_ppir::file_item_tree(db, file);
                    let alias = &item_tree[alias_loc.id(db)];
                    let alias_namespace =
                        baml_compiler2_hir::file_package::file_package(db, file).namespace_path;
                    let resolved = alias
                        .type_expr
                        .as_ref()
                        .map(|spanned| {
                            jinja_type_from_type_expr_inner(
                                db,
                                &spanned.expr,
                                pkg_items,
                                &alias_namespace,
                                resolving_aliases,
                            )
                        })
                        .unwrap_or(Type::Unknown);
                    resolving_aliases.remove(&key);
                    Type::Alias {
                        name,
                        target: Box::new(resolved.clone()),
                        resolved: Box::new(resolved),
                    }
                }
                _ => Type::Unknown,
            }
        }
        TypeExpr::Function { .. } => Type::Unknown,
        TypeExpr::Uint8Array { .. }
        | TypeExpr::Never { .. }
        | TypeExpr::Void { .. }
        | TypeExpr::BuiltinUnknown { .. }
        | TypeExpr::Type { .. }
        | TypeExpr::Rust { .. }
        | TypeExpr::Error { .. }
        | TypeExpr::Unknown { .. }
        | TypeExpr::Path { .. } => Type::Unknown,
    }
}

fn jinja_lookup_path(current_namespace: &[Name], segments: &[Name]) -> (Vec<Name>, String) {
    let (name, namespace) = segments
        .split_last()
        .expect("caller guarantees at least one segment");
    if namespace.is_empty() {
        (current_namespace.to_vec(), name.to_string())
    } else {
        (namespace.to_vec(), name.to_string())
    }
}

fn render_jinja_validation_result(
    file_id: FileId,
    source_text: &str,
    raw_string_range: TextRange,
    _template: &str,
    result: Result<(), sys_jinja_types::ValidationError>,
) -> Vec<Diagnostic> {
    let Err(error) = result else {
        return Vec::new();
    };

    if let Some(parse_error) = error.parsing_errors {
        let range = parse_error
            .range()
            .map(|range| jinja_offset_range(source_text, raw_string_range, range.start, range.end))
            .unwrap_or(raw_string_range);
        return vec![
            Diagnostic::error(
                DiagnosticId::JinjaParseError,
                format!("Error parsing jinja template: {parse_error}"),
            )
            .with_primary_span(Span { file_id, range })
            .with_phase(DiagnosticPhase::Type),
        ];
    }

    error
        .errors
        .into_iter()
        .map(|error| {
            let span = error.span();
            let range = jinja_offset_range(
                source_text,
                raw_string_range,
                span.start_offset as usize,
                span.end_offset as usize,
            );
            Diagnostic::warning(jinja_diagnostic_id(error.message()), error.message())
                .with_primary_span(Span { file_id, range })
                .with_phase(DiagnosticPhase::Type)
        })
        .collect()
}

fn raw_string_content_start(source_text: &str, raw_string_range: TextRange) -> TextSize {
    let start: usize = raw_string_range.start().into();
    let end: usize = raw_string_range.end().into();
    let Some(raw_text) = source_text.get(start..end) else {
        return raw_string_range.start();
    };
    let quote_offset = raw_text.find('"').unwrap_or(0);
    raw_string_range.start() + TextSize::from(u32::try_from(quote_offset + 1).unwrap_or(u32::MAX))
}

fn jinja_offset_range(
    source_text: &str,
    raw_string_range: TextRange,
    start_offset: usize,
    end_offset: usize,
) -> TextRange {
    let content_start = raw_string_content_start(source_text, raw_string_range);
    TextRange::new(
        content_start + TextSize::from(u32::try_from(start_offset).unwrap_or(u32::MAX)),
        content_start
            + TextSize::from(u32::try_from(end_offset.max(start_offset + 1)).unwrap_or(u32::MAX)),
    )
}

fn jinja_diagnostic_id(message: &str) -> DiagnosticId {
    if message.starts_with("Variable `") {
        DiagnosticId::JinjaUnresolvedVariable
    } else if message.contains("referenced without parentheses") {
        DiagnosticId::JinjaFunctionReferenceWithoutCall
    } else if message.starts_with("Filter '") {
        DiagnosticId::JinjaInvalidFilter
    } else if message.contains("expects argument") {
        DiagnosticId::JinjaWrongArgType
    } else if message.contains("expects ") && message.contains(" arguments") {
        DiagnosticId::JinjaWrongArgCount
    } else if message.contains("property ") {
        DiagnosticId::JinjaPropertyNotDefined
    } else if message.contains("enum") && message.contains("string") {
        DiagnosticId::JinjaEnumStringComparison
    } else {
        DiagnosticId::JinjaInvalidType
    }
}

fn collect_type_aliases_for_resolution_context<'db>(
    db: &'db dyn Db,
    res_ctx: &'db baml_compiler2_tir::package_interface::PackageResolutionContext<'db>,
) -> std::collections::HashMap<baml_compiler2_tir::ty::QualifiedTypeName, baml_compiler2_tir::ty::Ty>
{
    let mut aliases = baml_compiler2_tir::inference::collect_type_aliases(db, &res_ctx.own_items);
    for (_dep_name, dep_iface) in &res_ctx.dep_interfaces {
        for types_in_ns in dep_iface.types.values() {
            for exported in types_in_ns.values() {
                if let baml_compiler2_tir::package_interface::ExportedType::TypeAlias {
                    qtn,
                    resolved,
                } = exported
                {
                    aliases.insert(qtn.clone(), resolved.clone());
                }
            }
        }
    }
    aliases
}

fn function_scope_id<'db>(
    index: &baml_compiler2_hir::semantic_index::FileSemanticIndex<'db>,
    func_data: &baml_compiler2_hir::item_tree::Function,
) -> Option<baml_compiler2_hir::scope::ScopeId<'db>> {
    index
        .scopes
        .iter()
        .zip(index.scope_ids.iter())
        .find_map(|(scope, scope_id)| {
            (matches!(scope.kind, ScopeKind::Function)
                && scope.range == func_data.span
                && scope.name.as_ref() == Some(&func_data.name))
            .then_some(*scope_id)
        })
}

fn is_function_default_signature_diagnostic(
    diag: &baml_compiler2_tir::infer_context::TirDiagnostic<'_>,
) -> bool {
    matches!(&diag.primary, DiagnosticLocation::Span(_))
        && matches!(
            &diag.error,
            TirTypeError::RequiredParamAfterDefault { .. }
                | TirTypeError::SelfParamDefault
                | TirTypeError::DefaultParamForwardReference { .. }
                | TirTypeError::TypeMismatch { .. }
        )
}

/// Convert a `RenderedTirDiagnostic` to the shared `Diagnostic` type.
///
/// `RenderedTirDiagnostic` has already resolved arena IDs to `TextRange`.
/// We add the `file_id` to form a full `Span` for the primary annotation.
///
fn tir_rendered_to_diagnostic(
    rendered: baml_compiler2_tir::infer_context::RenderedTirDiagnostic,
    file_id: FileId,
) -> Diagnostic {
    let span = Span {
        file_id,
        range: rendered.range,
    };
    let diag = match rendered.severity {
        baml_compiler2_tir::infer_context::DiagnosticSeverity::Warning => Diagnostic::warning(
            tir_type_error_to_diagnostic_id(&rendered.error),
            rendered.message,
        ),
        baml_compiler2_tir::infer_context::DiagnosticSeverity::Error => Diagnostic::error(
            tir_type_error_to_diagnostic_id(&rendered.error),
            rendered.message,
        ),
    };
    rendered
        .related
        .into_iter()
        .fold(diag.with_primary_span(span), |diag, related| {
            diag.with_related(
                Span {
                    file_id: related.file_id,
                    range: related.range,
                },
                related.message,
            )
        })
        .with_phase(DiagnosticPhase::Type)
}

/// Map a `TirTypeError` to an approximate `DiagnosticId` for structural items.
///
/// This is used when we have access to the typed `TirTypeError` (for class field
/// and type alias diagnostics) rather than just the rendered string.
fn tir_type_error_to_diagnostic_id(
    error: &baml_compiler2_tir::infer_context::TirTypeError,
) -> DiagnosticId {
    use baml_compiler2_tir::infer_context::TirTypeError;
    match error {
        TirTypeError::TypeMismatch { .. } => DiagnosticId::TypeMismatch,
        TirTypeError::UnresolvedMember { .. } => DiagnosticId::NoSuchField,
        TirTypeError::UnresolvedName { .. } => DiagnosticId::UnknownVariable,
        TirTypeError::DeadCode { .. } => DiagnosticId::TypeMismatch,
        TirTypeError::VoidUsedAsValue => DiagnosticId::TypeMismatch,
        TirTypeError::VoidFunctionResultUsed => DiagnosticId::TypeMismatch,
        TirTypeError::NotCallable { .. } => DiagnosticId::NotCallable,
        TirTypeError::NotIterable { .. } => DiagnosticId::NotCallable,
        TirTypeError::NotIndexable { .. } => DiagnosticId::NotIndexable,
        TirTypeError::InvalidBinaryOp { .. } => DiagnosticId::InvalidOperator,
        TirTypeError::InvalidUnaryOp { .. } => DiagnosticId::InvalidOperator,
        TirTypeError::UnresolvedType { .. } => DiagnosticId::UnknownType,
        TirTypeError::ArgumentCountMismatch { .. }
        | TirTypeError::PositionalArgumentAfterNamed
        | TirTypeError::DuplicateNamedArgument { .. }
        | TirTypeError::UnknownNamedArgument { .. }
        | TirTypeError::DefaultedParamPassedPositionally { .. }
        | TirTypeError::MissingRequiredArgument { .. } => DiagnosticId::ArgumentCountMismatch,
        TirTypeError::RequiredParamAfterDefault { .. }
        | TirTypeError::SelfParamDefault
        | TirTypeError::DefaultParamForwardReference { .. } => DiagnosticId::TypeMismatch,
        TirTypeError::MissingReturn { .. } => DiagnosticId::MissingReturnExpression,
        TirTypeError::AliasCycle { .. } => DiagnosticId::AliasCycle,
        TirTypeError::ClassCycle { .. } => DiagnosticId::ClassCycle,
        TirTypeError::NonExhaustiveMatch { .. } => DiagnosticId::NonExhaustiveMatch,
        TirTypeError::UnreachableArm => DiagnosticId::UnreachableArm,
        TirTypeError::OrPatternBindingTypeMismatch { .. } => DiagnosticId::TypeMismatch,
        TirTypeError::GenericClassDestructureRequiresTypeArgs { .. } => DiagnosticId::TypeMismatch,
        TirTypeError::RestSubPatternNotSupported => DiagnosticId::TypeMismatch,
        TirTypeError::RefutablePatternInLet { .. } => DiagnosticId::RefutablePatternInLet,
        TirTypeError::InvalidCatchBindingType { .. } => DiagnosticId::InvalidCatchBindingType,
        TirTypeError::ThrowsContractViolation { .. }
        | TirTypeError::CallbackThrowsContractViolation { .. } => {
            DiagnosticId::ThrowsContractViolation
        }
        TirTypeError::ExtraneousThrowsDeclaration { .. } => DiagnosticId::ThrowsContractExtraneous,
        TirTypeError::CannotInferTypeParameter { .. } => DiagnosticId::UnknownType,
        TirTypeError::TypeParamShadowed { .. } => DiagnosticId::TypeMismatch,
        TirTypeError::CannotInferLambdaParamType { .. } => DiagnosticId::UnknownType,
        TirTypeError::WrongNumberOfTypeArgs { .. } => DiagnosticId::TypeMismatch,
        TirTypeError::TypeIsNotGeneric { .. } => DiagnosticId::TypeMismatch,
        TirTypeError::WrongTypeArgArity { .. } => DiagnosticId::ArgumentCountMismatch,
        // Optional chaining diagnostics
        TirTypeError::UnnecessaryOptionalChaining { .. } => DiagnosticId::InvalidOperator,
        TirTypeError::UnnecessaryNullCoalesce { .. } => DiagnosticId::InvalidOperator,
        TirTypeError::SuggestNullCoalesce { .. } => DiagnosticId::InvalidOperator,
        TirTypeError::NullCoalesceWithNull { .. } => DiagnosticId::InvalidOperator,
        TirTypeError::NullableMemberAccess { .. } => DiagnosticId::TypeMismatch,
        TirTypeError::AmbiguousInterfaceMethod { .. } => DiagnosticId::AmbiguousInterfaceMethod,
        TirTypeError::AmbiguousInterfaceField { .. } => DiagnosticId::AmbiguousInterfaceField,
        TirTypeError::InterfaceFieldRequiresProjection { .. } => DiagnosticId::NoSuchField,
        TirTypeError::InterfaceFieldRequiresQualifiedConstruction { .. } => {
            DiagnosticId::NoSuchField
        }
        TirTypeError::DeprecatedInterfaceProjection { .. } => DiagnosticId::NoSuchField,
        TirTypeError::InvalidInterfaceUpcastTarget { .. } => DiagnosticId::TypeMismatch,
        TirTypeError::InterfaceMemberRequiresReceiver { .. } => DiagnosticId::NoSuchField,
        TirTypeError::InvalidSelfCallThroughInterface { .. } => DiagnosticId::TypeMismatch,
        TirTypeError::DefaultOnRequiredMethod { .. } => DiagnosticId::DefaultOnRequiredMethod,
    }
}

#[cfg(test)]
mod tests {
    use baml_compiler_diagnostics::Severity;
    use baml_compiler2_tir::infer_context::{DiagnosticSeverity, RenderedTirDiagnostic};
    use text_size::{TextRange, TextSize};

    use super::*;
    use crate::testing::CursorTest;

    fn dummy_file_id() -> FileId {
        // Use index 0 — sufficient for span construction in unit tests.
        FileId::new(0)
    }

    fn dummy_rendered(severity: DiagnosticSeverity) -> RenderedTirDiagnostic {
        RenderedTirDiagnostic {
            error: baml_compiler2_tir::infer_context::TirTypeError::TypeMismatch {
                expected: baml_compiler2_tir::ty::Ty::Never {
                    attr: baml_compiler2_tir::ty::TyAttr::default(),
                },
                got: baml_compiler2_tir::ty::Ty::Never {
                    attr: baml_compiler2_tir::ty::TyAttr::default(),
                },
            },
            message: "test message".to_string(),
            range: TextRange::new(TextSize::from(0u32), TextSize::from(5u32)),
            severity,
            related: Vec::new(),
        }
    }

    #[test]
    fn tir_warning_severity_maps_to_warning_diagnostic() {
        let rendered = dummy_rendered(DiagnosticSeverity::Warning);
        let diag = tir_rendered_to_diagnostic(rendered, dummy_file_id());
        assert_eq!(
            diag.severity,
            Severity::Warning,
            "DiagnosticSeverity::Warning must produce a warning-level Diagnostic"
        );
    }

    #[test]
    fn tir_error_severity_maps_to_error_diagnostic() {
        let rendered = dummy_rendered(DiagnosticSeverity::Error);
        let diag = tir_rendered_to_diagnostic(rendered, dummy_file_id());
        assert_eq!(
            diag.severity,
            Severity::Error,
            "DiagnosticSeverity::Error must produce an error-level Diagnostic"
        );
    }

    #[test]
    fn check_file_preserves_callback_related_info() {
        let test = CursorTest::new(
            r#"function forward(cb: (x: int) -> int) -> int {
  return cb(1)
}

function demo() -> int throws string {
  return forward((x: int) -> int {
    throw "boom"
  })
}
<[CURSOR]"#,
        );

        let diagnostics = check_file(&test.db, test.cursor.file);
        let diag = diagnostics
            .iter()
            .find(|diag| {
                diag.id == DiagnosticId::ThrowsContractViolation
                    && diag
                        .message
                        .contains("this body may throw through callback `cb`")
                    && diag.message.contains("declared throws is `string`")
            })
            .expect("callback-aware throws diagnostic");

        assert_eq!(diag.related_info.len(), 2);
        assert_eq!(
            diag.related_info
                .iter()
                .map(|related| related.message.as_str())
                .collect::<Vec<_>>(),
            vec![
                "this call forwards whatever callback `cb` throws",
                "this callback throws `string`",
            ]
        );
    }

    #[test]
    fn check_file_reports_unknown_prompt_variable() {
        let test = CursorTest::new(
            r##"client GPT4o {
  provider openai
  options {
    model "gpt-5"
    api_key env.OPENAI_API_KEY
  }
}

class GuessResponse {
  game_won bool
  text string
}

function TakeGuess(user_guess: string, famous_person_name: string) -> GuessResponse {
  client GPT4o
  prompt #"
    {{ famouse_person_name | lower }}

    {{ ctx.output_format }}
  "#
}
<[CURSOR]"##,
        );

        let diagnostics = check_file(&test.db, test.cursor.file);
        let diag = diagnostics
            .iter()
            .find(|diag| diag.id == DiagnosticId::JinjaUnresolvedVariable)
            .expect("unknown prompt variable diagnostic");

        assert!(diag.message.contains("`famouse_person_name`"));
        assert!(diag.message.contains("does not exist"));
        let span = diag.primary_span().expect("primary span");
        let text = test.cursor.file.text(&test.db);
        let start: usize = span.range.start().into();
        let end: usize = span.range.end().into();
        assert_eq!(&text[start..end], "famouse_person_name");
    }

    #[test]
    fn check_file_allows_template_string_call_in_prompt() {
        let test = CursorTest::new(
            r##"client GPT4o {
  provider openai
  options {
    model "gpt-5"
    api_key env.OPENAI_API_KEY
  }
}

template_string GuessHeader(name: string) #"
  Guess the person: {{ name }}
"#

function TakeGuess(famous_person_name: string) -> string {
  client GPT4o
  prompt #"
    {{ GuessHeader(famous_person_name) }}
  "#
}
<[CURSOR]"##,
        );

        let diagnostics = check_file(&test.db, test.cursor.file);
        assert!(
            diagnostics
                .iter()
                .all(|diag| diag.id != DiagnosticId::JinjaUnresolvedVariable),
            "template string call should not be reported as an unknown prompt variable: {diagnostics:#?}"
        );
    }

    #[test]
    fn check_file_reports_unknown_template_string_argument() {
        let test = CursorTest::new(
            r##"client GPT4o {
  provider openai
  options {
    model "gpt-5"
    api_key env.OPENAI_API_KEY
  }
}

template_string GuessHeader(name: string) #"
  Guess the person: {{ name }}
"#

function TakeGuess(famous_person_name: string) -> string {
  client GPT4o
  prompt #"
    {{ GuessHeader(famouse_person_name) }}
  "#
}
<[CURSOR]"##,
        );

        let diagnostics = check_file(&test.db, test.cursor.file);
        let diag = diagnostics
            .iter()
            .find(|diag| diag.id == DiagnosticId::JinjaUnresolvedVariable)
            .expect("unknown template string argument diagnostic");

        assert!(diag.message.contains("`famouse_person_name`"));
        assert!(diag.message.contains("does not exist"));
        let span = diag.primary_span().expect("primary span");
        let text = test.cursor.file.text(&test.db);
        let start: usize = span.range.start().into();
        let end: usize = span.range.end().into();
        assert_eq!(&text[start..end], "famouse_person_name");
    }

    #[test]
    fn check_file_reports_template_string_call_errors() {
        let test = CursorTest::new(
            r##"template_string WithParams(a: int) #"
  ...
"#

template_string BadCall1() #"
  {{ WithParams(a=2, b=2) }}
"#

template_string BadCall2() #"
  {{ WithParams("a") }}
"#
<[CURSOR]"##,
        );

        let diagnostics = check_file(&test.db, test.cursor.file);
        assert!(
            diagnostics
                .iter()
                .any(|diag| diag.id == DiagnosticId::JinjaWrongArgCount
                    && diag.message.contains("expects 1 arguments")),
            "{diagnostics:#?}"
        );
        assert!(
            diagnostics
                .iter()
                .any(|diag| diag.id == DiagnosticId::JinjaWrongArgType
                    && diag.message.contains("expects argument 'a'")),
            "{diagnostics:#?}"
        );
    }

    #[test]
    fn check_file_resolves_cross_file_jinja_template_strings() {
        let mut builder = CursorTest::builder();
        builder.source(
            "shared.baml",
            r##"class Person {
  name string
}

template_string PersonHeader(person: Person) #"
  {{ person.name }}
"#
"##,
        );
        builder.source(
            "main.baml",
            r##"client GPT4o {
  provider openai
  options {
    model "gpt-5"
    api_key env.OPENAI_API_KEY
  }
}

function TakeGuess(person: Person) -> string {
  client GPT4o
  prompt #"
    {{ PersonHeader(person) }}
  "#
}
<[CURSOR]"##,
        );
        let test = builder.build();

        let diagnostics = check_file(&test.db, test.cursor.file);
        assert!(
            diagnostics
                .iter()
                .all(|diag| diag.id != DiagnosticId::JinjaUnresolvedVariable),
            "cross-file template string calls should resolve in prompts: {diagnostics:#?}"
        );
    }

    #[test]
    fn check_file_reports_builtin_function_default_constraints() {
        let test = CursorTest::new(
            r#"function BadBuiltin(
  a: int = b,
  b: int = 1,
  label: string = 2,
  required: int
) -> int {
  $rust_function
}
<[CURSOR]"#,
        );

        let messages = check_file(&test.db, test.cursor.file)
            .into_iter()
            .map(|diag| diag.message)
            .collect::<Vec<_>>();

        assert!(
            messages.iter().any(|message| message
                == "default for parameter `a` cannot reference later parameter `b`"),
            "missing forward-reference diagnostic; got {messages:#?}"
        );
        assert!(
            messages
                .iter()
                .any(|message| message == "type mismatch: expected string, got 2"),
            "missing default type-mismatch diagnostic; got {messages:#?}"
        );
        assert!(
            messages.iter().any(|message| message
                == "required parameter `required` cannot appear after a defaulted parameter"),
            "missing required-after-default diagnostic; got {messages:#?}"
        );
    }

    #[test]
    fn check_file_reports_builtin_self_default_constraint() {
        let test = CursorTest::new(
            r#"class Counter {
  value int

  function Current(self = null) -> int {
    $rust_function
  }
}
<[CURSOR]"#,
        );

        let messages = check_file(&test.db, test.cursor.file)
            .into_iter()
            .map(|diag| diag.message)
            .collect::<Vec<_>>();

        assert!(
            messages
                .iter()
                .any(|message| message == "`self` cannot have a default value"),
            "missing self-default diagnostic; got {messages:#?}"
        );
    }
}
