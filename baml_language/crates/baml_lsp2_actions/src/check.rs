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

use std::collections::HashSet;

use baml_base::{FileId, Name, SourceFile, Span};
use baml_compiler_diagnostics::{Diagnostic, DiagnosticId, DiagnosticPhase, ToDiagnostic};
use baml_compiler2_hir::{body::FunctionBody, file_semantic_index, scope::ScopeKind};
use baml_compiler2_tir::{
    infer_context::{DiagnosticLocation, TirTypeError},
    inference::render_scope_diagnostics,
    ty::{Ty, TyAttr},
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

    // Deduplicate: multiple steps can produce the same diagnostic (e.g. scope
    // inference + signature validation for the same unresolved return type).
    diagnostics.dedup_by(|a, b| {
        a.code() == b.code() && a.message == b.message && a.primary_span() == b.primary_span()
    });

    diagnostics
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
