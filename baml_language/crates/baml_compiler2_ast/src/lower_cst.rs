//! Pure structural CST → AST lowering.
//!
//! One function per item kind. Type expressions are fully lowered to recursive
//! `TypeExpr`. Expression bodies are fully lowered to `ExprBody` arenas with a
//! parallel `AstSourceMap`. Missing names skip the item (`return None`), missing
//! types produce `TypeExprKind::Unknown`.
//!
//! No LLM function expansion, no attribute validation, no duplicate detection —
//! all of that moves downstream.

use baml_base::{ClientOptionsPresence, Name, TypePath};
use baml_compiler_syntax::{SyntaxKind, SyntaxNode, SyntaxNodeExt, ast};
use rowan::ast::AstNode;

use crate::{
    DeclarativeMeta, LoweringDiagnostic,
    ast::{
        AssociatedTypeBindingDef, AssociatedTypeDef, AstSourceMap, BuiltinKind, CallArg, EnumDef,
        Expr, ExprBody, ExprId, FieldDef, FunctionBodyDef, FunctionDef, FunctionDefaults,
        ImplementsBlockDef, ImplementsForDef, InterfaceDef, InterfaceFieldLinkDef, Interpolation,
        Item, LambdaDef, LambdaKind, LetDef, LetOrigin, LlmBodyDef, MethodSigDef, Param,
        RawAttribute, RawAttributeArg, RawPrompt, TemplateStringDef, TestArgValue, TestDef,
        TypeAliasDef, TypeExpr, TypeExprKind, VariantDef,
    },
    companions::expand_companions,
    lower_expr_body, lower_type_expr,
};

// ── Test/Testset desugaring intermediate types ───────────────────

/// Intermediate representation of a test/testset block before synthesis.
///
/// Collected during file lowering, then passed to `synthesize_init_test_function`
/// which emits a per-file `$init_test_<path>` function containing
/// `registry.register_test(...)` / `registry.register_test_set(...)` calls.
enum TestRegistrationItem {
    Test {
        /// The name expression CST element (`STRING_LITERAL`, `BINARY_EXPR`, etc.).
        name_element: baml_compiler_syntax::SyntaxElement,
        /// The `BLOCK_EXPR` CST node for the test body — lowered lazily into a lambda.
        body_node: SyntaxNode,
        /// Optional runner expression element from `with <expr>`.
        /// May be a node (e.g. `CALL_EXPR`) or a token (e.g. `INTEGER_LITERAL`).
        runner_element: Option<baml_compiler_syntax::SyntaxElement>,
    },
    TestSet {
        /// The name expression CST element (`STRING_LITERAL`, `BINARY_EXPR`, etc.).
        name_element: baml_compiler_syntax::SyntaxElement,
        /// The `BLOCK_EXPR` body node of the testset — lowered as a collector lambda body.
        /// May contain setup statements (let bindings), for/if blocks, and nested test/testset.
        body_node: SyntaxNode,
        /// Optional runner expression element from `with <expr>`.
        runner_element: Option<baml_compiler_syntax::SyntaxElement>,
    },
}

// ── File-level lowering ─────────────────────────────────────────

/// Lower a CST root node to a list of `Item`s.
///
/// After this returns, the CST is no longer needed — all structural content
/// is owned by the returned `Item`s.
///
/// All diagnostics (structural lowering issues, client validation,
/// field-attr-in-wrong-position) are returned as `LoweringDiagnostic` variants.
pub fn lower_file(
    root: &SyntaxNode,
) -> (Vec<Item>, Vec<LoweringDiagnostic>, Vec<crate::EnvVarRef>) {
    lower_file_with_path(root, None)
}

pub fn lower_file_with_path(
    root: &SyntaxNode,
    file_path: Option<&std::path::Path>,
) -> (Vec<Item>, Vec<LoweringDiagnostic>, Vec<crate::EnvVarRef>) {
    lower_file_with_path_and_test_owner(root, file_path, None)
}

/// Variant used by project-aware lowering, where the HIR layer already knows
/// the exact BAML namespace derived from the project root. Keeping the owner an
/// input avoids accidentally treating an absolute ancestor named `ns_*` as a
/// user namespace.
pub fn lower_file_with_path_and_test_owner(
    root: &SyntaxNode,
    file_path: Option<&std::path::Path>,
    test_owner: Option<&str>,
) -> (Vec<Item>, Vec<LoweringDiagnostic>, Vec<crate::EnvVarRef>) {
    lower_file_with_path_and_test_owner_impl(root, file_path, test_owner, false)
}

/// Lower compiler-generated source for a `Session.eval` submission.
///
/// Session lowering rewrites persistent bindings into root lets before the
/// transient source reaches HIR. This entry point is intentionally separate
/// from ordinary file lowering so user-authored file-scope lets stay rejected.
pub fn lower_session_file_with_path_and_test_owner(
    root: &SyntaxNode,
    file_path: Option<&std::path::Path>,
    test_owner: Option<&str>,
) -> (Vec<Item>, Vec<LoweringDiagnostic>, Vec<crate::EnvVarRef>) {
    lower_file_with_path_and_test_owner_impl(root, file_path, test_owner, true)
}

fn lower_file_with_path_and_test_owner_impl(
    root: &SyntaxNode,
    file_path: Option<&std::path::Path>,
    test_owner: Option<&str>,
    is_session_submission: bool,
) -> (Vec<Item>, Vec<LoweringDiagnostic>, Vec<crate::EnvVarRef>) {
    let mut diags = Vec::new();
    let mut env_var_refs = Vec::new();
    let mut items = Vec::new();
    let mut test_registrations: Vec<TestRegistrationItem> = Vec::new();

    for child in root.children() {
        match child.kind() {
            baml_compiler_syntax::SyntaxKind::FUNCTION_DEF => {
                if let Some(func) = lower_function(&child, &mut diags, &mut env_var_refs) {
                    let companions = expand_companions(&func);
                    items.push(Item::Function(func));
                    items.extend(companions.into_iter().map(Item::Function));
                }
            }
            baml_compiler_syntax::SyntaxKind::CLASS_DEF => {
                if let Some(class) = lower_class(&child, &mut diags, &mut env_var_refs) {
                    items.push(Item::Class(class));
                }
            }
            baml_compiler_syntax::SyntaxKind::ENUM_DEF => {
                if let Some(e) = lower_enum(&child, &mut diags) {
                    items.push(Item::Enum(e));
                }
            }
            baml_compiler_syntax::SyntaxKind::INTERFACE_DEF => {
                if let Some(i) = lower_interface(&child, &mut diags, &mut env_var_refs) {
                    items.push(Item::Interface(i));
                }
            }
            baml_compiler_syntax::SyntaxKind::TYPE_ALIAS_DEF => {
                if let Some(ta) = lower_type_alias(&child, &mut diags) {
                    items.push(Item::TypeAlias(ta));
                }
            }
            baml_compiler_syntax::SyntaxKind::CLIENT_DEF => {
                if let Some((let_item, companion)) =
                    synthesize_client_items(&child, &mut diags, &mut env_var_refs)
                {
                    items.push(let_item);
                    if let Some(func) = companion {
                        items.push(Item::Function(func));
                    }
                }
            }
            baml_compiler_syntax::SyntaxKind::TEST_DEF => {
                if let Some(t) = lower_test(&child, &mut diags) {
                    items.push(Item::Test(t));
                }
            }
            baml_compiler_syntax::SyntaxKind::TEST_EXPR_DEF => {
                if let Some(reg) = lower_test_expr(&child) {
                    test_registrations.push(reg);
                }
            }
            baml_compiler_syntax::SyntaxKind::TESTSET_DEF => {
                if let Some(reg) = lower_testset(&child) {
                    test_registrations.push(reg);
                }
            }
            baml_compiler_syntax::SyntaxKind::GENERATOR_DEF => {
                diags.push(lower_generator_deprecation(&child));
            }
            baml_compiler_syntax::SyntaxKind::TEMPLATE_STRING_DEF => {
                if let Some(ts) = lower_template_string(&child, &mut diags) {
                    items.push(Item::TemplateString(ts));
                }
            }
            baml_compiler_syntax::SyntaxKind::RETRY_POLICY_DEF => {
                if let Some(let_item) =
                    synthesize_retry_policy_let(&child, &mut diags, &mut env_var_refs)
                {
                    items.push(let_item);
                }
            }
            baml_compiler_syntax::SyntaxKind::LET_STMT => {
                if is_session_submission {
                    if let Some(let_item) =
                        lower_expr_body::lower_session_let(&child, &mut diags, &mut env_var_refs)
                    {
                        items.push(Item::Let(let_item));
                    }
                } else {
                    diags.push(LoweringDiagnostic::TopLevelLetNotSupported {
                        span: child.span_range(),
                    });
                }
            }
            baml_compiler_syntax::SyntaxKind::IMPLEMENTS_FOR => {
                if let Some(imp) = lower_implements_for(&child, &mut diags, &mut env_var_refs) {
                    items.push(Item::ImplementsFor(imp));
                }
            }
            _ => {} // skip comments, whitespace, errors
        }
    }

    // BEP-044: merge top-level `implements I for T` items into the target
    // class when `T` names a local class. Targets declared in another file,
    // primitives, and fixed-shape types must remain as first-class
    // implementation records for later package-level resolution.
    let impl_fors: Vec<ImplementsForDef> = items
        .iter()
        .filter_map(|item| match item {
            Item::ImplementsFor(imp) => Some(imp.clone()),
            _ => None,
        })
        .collect();
    items.retain(|item| !matches!(item, Item::ImplementsFor(_)));
    for imp in impl_fors {
        if !imp.generic_params.is_empty() {
            items.push(Item::ImplementsFor(imp));
            continue;
        }
        let target_name = match &imp.for_target.kind {
            crate::ast::TypeExprKind::Path {
                segments,
                generic_args,
                ..
            } if segments.len() == 1 && generic_args.is_empty() => segments.first().cloned(),
            _ => None,
        };
        if let Some(target_name) = target_name {
            let target_class = items.iter_mut().find_map(|item| {
                if let Item::Class(class) = item
                    && class.name == target_name
                {
                    Some(class)
                } else {
                    None
                }
            });
            if let Some(class) = target_class {
                class.implements.push(ImplementsBlockDef {
                    target: imp.interface_target,
                    field_links: imp.field_links,
                    associated_type_bindings: imp.associated_type_bindings,
                    methods: imp.methods,
                    is_out_of_body: true,
                    span: imp.span,
                });
            } else {
                items.push(Item::ImplementsFor(imp));
            }
        } else {
            items.push(Item::ImplementsFor(imp));
        }
    }

    // Synthesize a per-file $init_test function for all collected test/testset registrations.
    // The path-derived suffix keeps the name unique across files while staying
    // stable across compilations (unlike a load-order file id).
    if !test_registrations.is_empty() {
        let init_fn = synthesize_init_test_function(
            &test_registrations,
            file_path,
            test_owner,
            &mut diags,
            &mut env_var_refs,
        );
        items.push(Item::Function(init_fn));
    }

    // Post-lowering validation: reject field attrs in invalid type positions.
    let field_attr_errors = crate::disambiguate::validate_field_attrs(&items);
    for (attr_name, span) in field_attr_errors {
        diags.push(LoweringDiagnostic::FieldAttributeInTypePosition { attr_name, span });
    }

    (items, diags, env_var_refs)
}

/// Check if a just-lowered type expression contains `TypeExprKind::Unknown` at the root.
/// If so, emit an `UnparseableType` diagnostic.
fn check_unknown_type(
    type_expr: &crate::ast::TypeExpr,
    context: String,
    span: text_size::TextRange,
    diags: &mut Vec<LoweringDiagnostic>,
) {
    if matches!(type_expr.kind, crate::ast::TypeExprKind::Unknown { .. }) {
        diags.push(LoweringDiagnostic::UnparseableType { context, span });
    }
}

// ── Per-item lowering ───────────────────────────────────────────

fn lower_function(
    node: &SyntaxNode,
    diags: &mut Vec<LoweringDiagnostic>,
    env_var_refs: &mut Vec<crate::EnvVarRef>,
) -> Option<FunctionDef> {
    let func = ast::FunctionDef::cast(node.clone())?;
    let Some(name_token) = func.name() else {
        diags.push(LoweringDiagnostic::MissingItemName {
            item_kind: "function",
            span: node.span_range(),
        });
        return None;
    };
    let name = Name::new(name_token.text());
    let name_span = name_token.text_range();
    // A function named `$id` is unreachable through a bare call (`$id()`
    // resolves to the runtime-identity special form first) — reject the
    // declaration with the reserved-name diagnostic instead of letting use
    // sites fail with a misleading "`string` is not a function".
    if name.as_str() == "$id" {
        diags.push(LoweringDiagnostic::ReservedRuntimeIdBindingName { span: name_span });
    }

    let generic_params = extract_generic_params_with_bounds(node, diags);
    let parameter_context = format!("function `{}`", name.as_str());

    let (mut params, mut defaults) = func
        .param_list()
        .map(|pl| {
            lower_params_with_defaults(
                &pl,
                name.as_str(),
                &parameter_context,
                diags,
                true,
                env_var_refs,
            )
        })
        .unwrap_or_else(|| (Vec::new(), FunctionDefaults::empty()));

    let return_type = func.return_type().map(|te| {
        let mut expr = lower_type_expr::lower_type_expr_node(&te, diags);
        let te_span = te.syntax().span_range();
        check_unknown_type(&expr, format!("return type of `{name}`"), te_span, diags);
        // void is allowed as a bare return type, but not wrapped (void?, void[], etc.).
        lower_type_expr::check_void_type(
            &expr,
            format!("return type of `{name}`"),
            te_span,
            true,
            diags,
        );
        lower_type_expr::check_wildcard_type(&mut expr, "a return type", te_span, diags);
        expr.with_span(te_span)
    });

    let throws = func
        .throws_clause()
        .and_then(|tc| tc.type_expr())
        .map(|te| {
            let mut expr = lower_type_expr::lower_type_expr_node(&te, diags);
            let te_span = te.syntax().span_range();
            lower_type_expr::check_throws_wildcard(&mut expr, te_span, diags);
            expr.with_span(te_span)
        });

    let (body, declarative_meta) = if let Some(llm) = func.llm_body() {
        let mut llm_body_def = lower_llm_body(&llm);
        reject_reserved_llm_client_params(&mut params, name.as_str(), diags);
        let client_name = llm_body_def.client.as_ref().map(|n| n.as_str().to_string());
        if let Some(client_name) = client_name.as_deref() {
            append_default_client_param(&mut params, &mut defaults, client_name, llm_body_def.span);
        }
        let param_names: Vec<Name> = params
            .iter()
            .filter(|p| p.name.as_str() != "client")
            .map(|p| p.name.clone())
            .collect();
        let client_arg_name = client_name.as_ref().map(|_| "client");
        // Pass the LLM function's declared return type as the explicit `<T>`
        // type argument to `baml.llm.call_llm_function<T>`. This is required
        // for the runtime type-arg threading: without it, `T` falls back to
        // inferred-only and resolves to BuiltinUnknown inside the stdlib's
        // `primitive.parse<T>(body)` call, surfacing as a "Non-parsable type:
        // BuiltinUnknown" error from the LLM client.
        let call_type_args: Vec<crate::ast::TypeExpr> = return_type
            .as_ref()
            .map(|rt| vec![rt.clone()])
            .unwrap_or_default();
        // New-mode (BEP-049 M5f): a backtick prompt compiles to a `prompt`…``
        // closure passed as the 4th arg to `call_llm_function`; the orchestrator
        // invokes it per attempt. Legacy `#"..."#` Jinja prompts keep the 3-arg
        // path (the closure defaults to `null`, so the Jinja render runs).
        let prompt_backtick = llm.prompt_field().and_then(|pf| pf.backtick_string());
        let (expr_body, source_map) = if let Some(backtick) = &prompt_backtick {
            let (body, sm, mut closure_diags, mut closure_env_refs) =
                lower_expr_body::synthesize_llm_call_with_prompt(
                    "call_llm_function",
                    name.as_str(),
                    &param_names,
                    client_arg_name,
                    call_type_args,
                    backtick,
                    llm_body_def.span,
                );
            diags.append(&mut closure_diags);
            env_var_refs.append(&mut closure_env_refs);
            // BEP-049 M5e: pre-build the streaming companion's body from the
            // same backtick now, while the CST is in hand, and stash it for
            // PPIR (which materializes the `$stream` companion but no longer has
            // the CST). The closure captures this function's params, so it's a
            // separate arena from the oneshot body above. Its prompt diagnostics
            // / `env.X` refs duplicate the oneshot body's — drop them.
            let (stream_body, stream_sm, _diags, _env_refs) =
                lower_expr_body::synthesize_llm_call_with_prompt(
                    "stream_llm_function",
                    name.as_str(),
                    &param_names,
                    client_arg_name,
                    Vec::new(),
                    backtick,
                    llm_body_def.span,
                );
            llm_body_def.stream_body = Some((stream_body, stream_sm));
            // BEP-049 M5: pre-build the render_prompt / build_request /
            // build_request_stream companion bodies from the same backtick, each
            // carrying the prompt closure, so the playground preview/cURL render
            // through the closure exactly like execution. Built here while the CST
            // is in hand; read back by `make_llm_companion`. Their prompt diags /
            // `env.X` refs duplicate the oneshot body's — drop them.
            for target in ["render_prompt", "build_request", "build_request_stream"] {
                let (c_body, c_sm, _diags, _env_refs) =
                    lower_expr_body::synthesize_llm_call_with_prompt(
                        target,
                        name.as_str(),
                        &param_names,
                        client_arg_name,
                        return_type
                            .as_ref()
                            .map(|return_type| vec![return_type.clone()])
                            .unwrap_or_default(),
                        backtick,
                        llm_body_def.span,
                    );
                llm_body_def
                    .companion_bodies
                    .push((target.to_string(), (c_body, c_sm)));
            }
            (body, sm)
        } else {
            synthesize_llm_builtin_call(
                "call_llm_function",
                name.as_str(),
                &param_names,
                client_arg_name,
                call_type_args,
                llm_body_def.span,
            )
        };
        (
            Some(FunctionBodyDef::Expr(expr_body, source_map)),
            Some(DeclarativeMeta::Llm(llm_body_def)),
        )
    } else if let Some(expr) = func.expr_body() {
        // Check if the body is `$rust_function` or `$rust_io_function` before lowering
        if let Some(builtin_kind) = check_builtin_body(expr.syntax()) {
            (Some(FunctionBodyDef::Builtin(builtin_kind)), None)
        } else {
            let (expr_body, source_map) = lower_expr_body::lower(&expr, diags, env_var_refs);
            (Some(FunctionBodyDef::Expr(expr_body, source_map)), None)
        }
    } else {
        (None, None)
    };

    let attributes = lower_attributes_from_node(node);
    let docstring = crate::docstring::extract_docstring(node);
    let is_tagged_template_tag = crate::docstring::has_baml_marker(node, "tagged_string");

    Some(FunctionDef {
        name,
        generic_params,
        params,
        defaults,
        return_type,
        throws,
        body,
        declarative_meta,
        metadata: crate::ast::FunctionMetadata::user_facing(
            crate::ast::FunctionOrigin::UserDefined,
        ),
        attributes,
        docstring,
        is_tagged_template_tag,
        span: node.span_range(),
        name_span,
    })
}

/// Check if an `EXPR_FUNCTION_BODY` node's content is a single `$rust_function`
/// or `$rust_io_function` word. Returns the `BuiltinKind` if so.
///
/// The expected CST structure is:
/// `EXPR_FUNCTION_BODY { BLOCK_EXPR { L_BRACE PATH_EXPR { WORD("$rust_function") } R_BRACE } }`
fn check_builtin_body(expr_body_node: &SyntaxNode) -> Option<BuiltinKind> {
    use baml_compiler_syntax::SyntaxKind;

    // Collect all non-trivia tokens from the body
    let meaningful_tokens: Vec<_> = expr_body_node
        .descendants_with_tokens()
        .filter_map(baml_compiler_syntax::NodeOrToken::into_token)
        .filter(|t| {
            let kind: SyntaxKind = t.kind();
            !kind.is_trivia() && kind != SyntaxKind::L_BRACE && kind != SyntaxKind::R_BRACE
        })
        .collect();

    if meaningful_tokens.len() == 1 {
        let text = meaningful_tokens[0].text();
        match text {
            "$rust_function" => return Some(BuiltinKind::Vm),
            "$rust_io_function" => return Some(BuiltinKind::Io),
            "$compiler_intrinsic" => return Some(BuiltinKind::Intrinsic),
            "$await_any" => return Some(BuiltinKind::AwaitAny),
            _ => {}
        }
    }
    None
}

pub(crate) fn lower_params(
    pl: &ast::ParameterList,
    owner_name: &str,
    unsupported_default_context: &str,
    diags: &mut Vec<LoweringDiagnostic>,
) -> Vec<Param> {
    let mut env_var_refs = Vec::new();
    lower_params_with_defaults(
        pl,
        owner_name,
        unsupported_default_context,
        diags,
        false,
        &mut env_var_refs,
    )
    .0
}

pub(crate) fn lower_params_with_defaults(
    pl: &ast::ParameterList,
    owner_name: &str,
    unsupported_default_context: &str,
    diags: &mut Vec<LoweringDiagnostic>,
    defaults_allowed: bool,
    env_var_refs: &mut Vec<crate::EnvVarRef>,
) -> (Vec<Param>, FunctionDefaults) {
    let mut default_nodes = Vec::new();
    let mut params = Vec::new();
    for p in pl.params() {
        let default_expr = p.default_expr_syntax();
        let lowered_param_idx = params.len();

        match lower_param(&p, owner_name, diags) {
            Some(param) => {
                if let Some(default_expr) = default_expr.clone() {
                    if !defaults_allowed {
                        diags.push(LoweringDiagnostic::UnsupportedParameterDefault {
                            context: unsupported_default_context.to_string(),
                            span: default_expr.text_range(),
                        });
                    }
                    default_nodes.push((lowered_param_idx, default_expr));
                }
                params.push(param);
            }
            None => {
                let Some(default_expr) = default_expr else {
                    continue;
                };
                if !defaults_allowed {
                    diags.push(LoweringDiagnostic::UnsupportedParameterDefault {
                        context: unsupported_default_context.to_string(),
                        span: default_expr.text_range(),
                    });
                }
                default_nodes.push((usize::MAX, default_expr));
            }
        }
    }

    let (defaults, default_ids) =
        lower_expr_body::lower_default_expr_nodes(&default_nodes, diags, env_var_refs);
    for (idx, default_id) in default_ids {
        if let Some(param) = params.get_mut(idx) {
            param.default = Some(default_id);
        }
    }

    (params, defaults)
}

pub(crate) fn lower_param(
    param: &ast::Parameter,
    function_name: &str,
    diags: &mut Vec<LoweringDiagnostic>,
) -> Option<Param> {
    let Some(name_token) = param.name() else {
        diags.push(LoweringDiagnostic::MissingParamName {
            function_name: function_name.to_string(),
            span: param.syntax().span_range(),
        });
        return None;
    };
    let param_name_str = name_token.text().to_string();
    // `$id` is the runtime-identity special form; a parameter named `$id`
    // would be a silently-dead binding (reads hit the special cases first).
    if param_name_str == "$id" {
        diags.push(LoweringDiagnostic::ReservedRuntimeIdBindingName {
            span: name_token.text_range(),
        });
    }
    Some(Param {
        name: Name::new(&param_name_str),
        type_expr: param.ty().map(|te| {
            let mut expr = lower_type_expr::lower_type_expr_node(&te, diags);
            let te_span = te.syntax().span_range();
            check_unknown_type(
                &expr,
                format!("parameter `{param_name_str}` in `{function_name}`"),
                te_span,
                diags,
            );
            lower_type_expr::check_void_type(
                &expr,
                "a parameter type".to_string(),
                te_span,
                false,
                diags,
            );
            lower_type_expr::check_wildcard_type(&mut expr, "a parameter type", te_span, diags);
            expr.with_span(te_span)
        }),
        default: None,
        span: param.syntax().span_range(),
        name_span: name_token.text_range(),
    })
}

pub(crate) fn append_default_client_param(
    params: &mut Vec<Param>,
    defaults: &mut FunctionDefaults,
    client_name: &str,
    span: text_size::TextRange,
) {
    let default_expr = alloc_client_override_default_expr(defaults, client_name, span);
    params.push(Param {
        name: Name::new("client"),
        type_expr: Some(
            TypeExprKind::Path {
                segments: vec![Name::new("baml"), Name::new("llm"), Name::new("Client")],
                generic_args: vec![],
                associated_type_bindings: vec![],
                attrs: vec![],
            }
            .at(span),
        ),
        default: Some(crate::ast::DefaultExprId::new(default_expr)),
        span,
        name_span: span,
    });
}

fn reject_reserved_llm_client_params(
    params: &mut Vec<Param>,
    function_name: &str,
    diags: &mut Vec<LoweringDiagnostic>,
) {
    let mut reserved_spans = Vec::new();
    params.retain(|param| {
        if param.name.as_str() == "client" {
            reserved_spans.push(param.name_span);
            false
        } else {
            true
        }
    });

    for span in reserved_spans {
        diags.push(LoweringDiagnostic::ReservedLlmClientParam {
            function_name: function_name.to_string(),
            param_name: "client".to_string(),
            span,
        });
    }
}

fn alloc_client_override_default_expr(
    defaults: &mut FunctionDefaults,
    client_name: &str,
    span: text_size::TextRange,
) -> ExprId {
    fn alloc(defaults: &mut FunctionDefaults, expr: Expr, span: text_size::TextRange) -> ExprId {
        let id = defaults.exprs.exprs.alloc(expr);
        defaults.source_map.expr_spans.alloc(span);
        id
    }

    if client_name.contains('/') {
        // Shorthand clients are not backed by a synthesized top-level let binding,
        // so defaults have to carry the inline Client value.
        let name_lit = alloc(
            defaults,
            Expr::Literal(crate::ast::Literal::String(client_name.to_string())),
            span,
        );
        let client_type = alloc(
            defaults,
            Expr::Path(vec![
                Name::new("baml"),
                Name::new("llm"),
                Name::new("ClientType"),
                Name::new("Primitive"),
            ]),
            span,
        );
        let sub_clients = alloc(defaults, Expr::Array { elements: vec![] }, span);
        let retry = alloc(defaults, Expr::Null, span);
        let counter = alloc(defaults, Expr::Literal(crate::ast::Literal::Int(0)), span);
        alloc(
            defaults,
            Expr::Object {
                type_name: TypePath::from_dotted("baml.llm.Client"),
                type_args: vec![],
                fields: vec![
                    (Name::new("name"), name_lit),
                    (Name::new("client_type"), client_type),
                    (Name::new("sub_clients"), sub_clients),
                    (Name::new("retry"), retry),
                    (Name::new("counter"), counter),
                ],
                spreads: vec![],
            },
            span,
        )
    } else {
        // Named client declarations already lower to a top-level client let binding.
        alloc(defaults, Expr::Path(vec![Name::new(client_name)]), span)
    }
}

fn lower_llm_body(llm_body: &ast::LlmFunctionBody) -> LlmBodyDef {
    let span = llm_body.syntax().span_range();

    let client = llm_body
        .client_field()
        .and_then(|cf| cf.value())
        .map(|name| Name::new(&name));

    let prompt = llm_body
        .prompt_field()
        .and_then(|pf| pf.raw_string())
        .map(|raw_str| lower_raw_prompt(&raw_str));

    LlmBodyDef {
        client,
        prompt,
        // Filled in by the LLM-function branch once param names are known.
        stream_body: None,
        companion_bodies: Vec::new(),
        span,
    }
}

/// Build a synthetic expression body equivalent to:
/// `baml.llm.<builtin_name>(client, "FunctionName", { "param1": param1, "param2": param2 })`
///
/// When `client_name` is `Some("MyClient")`, the first argument is `Expr::Path(["MyClient"])`.
/// When `client_name` is `Some("openai/gpt-4o")` (a shorthand with `/`), the first argument is
/// an inline `Client { name, client_type: ClientType.Primitive, sub_clients: [], retry: null, counter: 0 }`.
/// When `client_name` is `None`, `Expr::Null` is used as a fallback.
///
/// LLM functions and request/render/stream companions pass a client as the first argument.
/// After default-client parameter synthesis, call sites pass `client_name = Some("client")`
/// so explicit `client=` overrides flow through the generated body.
///
/// All synthetic spans point to `span`.
pub fn synthesize_llm_builtin_call(
    builtin_name: &str,
    function_name: &str,
    param_names: &[Name],
    client_name: Option<&str>,
    type_args: Vec<crate::ast::TypeExpr>,
    span: text_size::TextRange,
) -> (crate::ast::ExprBody, crate::ast::AstSourceMap) {
    use la_arena::Arena;

    use crate::ast::{AstSourceMap, Expr, ExprBody, Literal};

    let mut exprs = Arena::new();
    let mut expr_spans = Arena::new();

    // Helper: allocate an expr + its span
    let mut alloc = |expr: Expr| -> crate::ast::ExprId {
        let id = exprs.alloc(expr);
        expr_spans.alloc(span);
        id
    };

    // 1. Function name literal: "FunctionName"
    let fn_name_expr = alloc(Expr::Literal(Literal::String(function_name.to_string())));

    // 2. Map entries: { "param1": param1, "param2": param2 }
    let entries: Vec<(crate::ast::ExprId, crate::ast::ExprId)> = param_names
        .iter()
        .map(|name| {
            let key = alloc(Expr::Literal(Literal::String(name.as_str().to_string())));
            let value = alloc(Expr::Path(vec![name.clone()]));
            (key, value)
        })
        .collect();
    let args_map = alloc(Expr::Map { entries });

    // Callee: baml.llm.<builtin_name> as a multi-segment Path
    let callee = alloc(Expr::Path(vec![
        Name::new("baml"),
        Name::new("llm"),
        Name::new(builtin_name),
    ]));

    // Build the client expression from the client name.
    // All LLM builtins (call_llm_function, render_prompt, build_request) take
    // a Client as the first argument.
    let client_arg = match client_name {
        Some(name) if name.contains('/') => {
            // Shorthand client (e.g. "openai/gpt-4o"): build an inline Client object.
            let name_lit = alloc(Expr::Literal(Literal::String(name.to_string())));
            let ct_variant = alloc(Expr::Path(vec![
                Name::new("baml"),
                Name::new("llm"),
                Name::new("ClientType"),
                Name::new("Primitive"),
            ]));
            let sub = alloc(Expr::Array { elements: vec![] });
            let retry = alloc(Expr::Null);
            let counter = alloc(Expr::Literal(Literal::Int(0)));
            alloc(Expr::Object {
                type_name: TypePath::from_dotted("baml.llm.Client"),
                type_args: vec![],
                fields: vec![
                    (Name::new("name"), name_lit),
                    (Name::new("client_type"), ct_variant),
                    (Name::new("sub_clients"), sub),
                    (Name::new("retry"), retry),
                    (Name::new("counter"), counter),
                ],
                spreads: vec![],
            })
        }
        Some(name) => {
            // Named client: Expr::Path(["MyClient"]) — TIR resolves to the let binding.
            alloc(Expr::Path(vec![Name::new(name)]))
        }
        None => {
            // No client specified (e.g. missing `client` field) — use null as fallback.
            alloc(Expr::Null)
        }
    };
    let call = alloc(Expr::Call {
        callee,
        type_args: type_args.into_iter().map(Into::into).collect(),
        args: vec![
            CallArg::positional(client_arg),
            CallArg::positional(fn_name_expr),
            CallArg::positional(args_map),
        ],
    });

    let body = ExprBody {
        exprs,
        stmts: Arena::new(),
        patterns: Arena::new(),
        match_arms: Arena::new(),
        catch_arms: Arena::new(),
        type_annotations: Arena::new(),
        root_expr: Some(call),
    };

    let source_map = AstSourceMap {
        expr_spans,
        ..Default::default()
    };

    (body, source_map)
}

/// Synthesize a `baml.llm.parse<STREAM_EXPANDED, ORIGINAL>(json)` call.
///
/// Unlike `synthesize_llm_builtin_call`, there is no client argument and
/// the only argument is a single `json` identifier (a path expression)
/// rather than a map of parent params. The explicit type args carry the
/// stream-expanded and original return types so the stdlib `parse` can
/// reify them via `type.of` instead of a name-keyed registry
/// lookup (same threading as `call_llm_function<T>`).
pub(crate) fn synthesize_llm_parse_call(
    type_args: Vec<crate::ast::TypeExpr>,
    span: text_size::TextRange,
) -> (crate::ast::ExprBody, crate::ast::AstSourceMap) {
    use la_arena::Arena;

    use crate::ast::{AstSourceMap, Expr, ExprBody};

    let mut exprs = Arena::new();
    let mut expr_spans = Arena::new();

    let mut alloc = |expr: Expr| -> crate::ast::ExprId {
        let id = exprs.alloc(expr);
        expr_spans.alloc(span);
        id
    };

    // 1. `json` parameter reference
    let json_expr = alloc(Expr::Path(vec![Name::new("json")]));

    // 2. Callee: baml.llm.parse
    let callee = alloc(Expr::Path(vec![
        Name::new("baml"),
        Name::new("llm"),
        Name::new("parse"),
    ]));

    let call = alloc(Expr::Call {
        callee,
        type_args: type_args.into_iter().map(Into::into).collect(),
        args: vec![CallArg::positional(json_expr)],
    });

    let body = ExprBody {
        exprs,
        stmts: Arena::new(),
        patterns: Arena::new(),
        match_arms: Arena::new(),
        catch_arms: Arena::new(),
        type_annotations: Arena::new(),
        root_expr: Some(call),
    };

    let source_map = AstSourceMap {
        expr_spans,
        ..Default::default()
    };

    (body, source_map)
}

/// Synthesize a `CLIENT.__make_stream<STREAM_EXPANDED, ORIGINAL>(sse)` method call.
///
/// Used by the PPIR to generate `$parse_stream` companion function bodies.
/// The explicit type args carry the stream-expanded and original return
/// types into `__make_stream`'s frame, where `type.of` reifies them
/// for `StreamCache.new`.
pub fn synthesize_llm_make_stream_call(
    type_args: Vec<crate::ast::TypeExpr>,
    client_name: &str,
    span: text_size::TextRange,
) -> (crate::ast::ExprBody, crate::ast::AstSourceMap) {
    use la_arena::Arena;

    use crate::ast::{AstSourceMap, Expr, ExprBody, Literal};

    let mut exprs = Arena::new();
    let mut expr_spans = Arena::new();

    let mut alloc = |expr: Expr| -> crate::ast::ExprId {
        let id = exprs.alloc(expr);
        expr_spans.alloc(span);
        id
    };

    // 1. `sse` parameter reference
    let sse_expr = alloc(Expr::Path(vec![Name::new("sse")]));

    // 2. Client argument (same logic as synthesize_llm_builtin_call)
    let client_arg = if client_name.contains('/') {
        let name_lit = alloc(Expr::Literal(Literal::String(client_name.to_string())));
        let ct_path = alloc(Expr::Path(vec![
            Name::new("baml"),
            Name::new("llm"),
            Name::new("ClientType"),
        ]));
        let ct_variant = alloc(Expr::MemberAccess {
            base: ct_path,
            member: Name::new("Primitive"),
        });
        let sub = alloc(Expr::Array { elements: vec![] });
        let retry = alloc(Expr::Null);
        let counter = alloc(Expr::Literal(Literal::Int(0)));
        alloc(Expr::Object {
            type_name: TypePath::from_dotted("baml.llm.Client"),
            type_args: vec![],
            fields: vec![
                (Name::new("name"), name_lit),
                (Name::new("client_type"), ct_variant),
                (Name::new("sub_clients"), sub),
                (Name::new("retry"), retry),
                (Name::new("counter"), counter),
            ],
            spreads: vec![],
        })
    } else {
        alloc(Expr::Path(vec![Name::new(client_name)]))
    };

    // 3. Callee: CLIENT.__make_stream (method call on the client)
    let callee = alloc(Expr::MemberAccess {
        base: client_arg,
        member: Name::new("__make_stream"),
    });

    let call = alloc(Expr::Call {
        callee,
        type_args: type_args.into_iter().map(Into::into).collect(),
        args: vec![CallArg::positional(sse_expr)],
    });

    let body = ExprBody {
        exprs,
        stmts: Arena::new(),
        patterns: Arena::new(),
        match_arms: Arena::new(),
        catch_arms: Arena::new(),
        type_annotations: Arena::new(),
        root_expr: Some(call),
    };

    let source_map = AstSourceMap {
        expr_spans,
        ..Default::default()
    };

    (body, source_map)
}

fn lower_raw_prompt(raw_string: &ast::RawStringLiteral) -> RawPrompt {
    use baml_compiler_syntax::{
        SyntaxKind,
        ast::{JinjaExpression, JinjaStatement, PromptText},
    };

    let mut text = String::new();
    let mut interpolations = Vec::new();
    let prompt_span = raw_string.syntax().span_range();

    for child in raw_string.syntax().children() {
        match child.kind() {
            SyntaxKind::PROMPT_TEXT => {
                if let Some(prompt_text) = PromptText::cast(child.clone()) {
                    text.push_str(&prompt_text.text());
                }
            }
            SyntaxKind::TEMPLATE_INTERPOLATION => {
                if let Some(jinja_expr) = JinjaExpression::cast(child.clone()) {
                    let inner = jinja_expr.inner_text();
                    let full = jinja_expr.full_text();
                    let span = child.span_range();
                    interpolations.push(Interpolation {
                        content: inner,
                        span,
                    });
                    text.push_str(&full);
                }
            }
            SyntaxKind::TEMPLATE_CONTROL => {
                if let Some(jinja_stmt) = JinjaStatement::cast(child.clone()) {
                    text.push_str(&jinja_stmt.full_text());
                }
            }
            _ => {}
        }
    }

    RawPrompt {
        text,
        interpolations,
        span: prompt_span,
    }
}

fn lower_class(
    node: &SyntaxNode,
    diags: &mut Vec<LoweringDiagnostic>,
    env_var_refs: &mut Vec<crate::EnvVarRef>,
) -> Option<crate::ast::ClassDef> {
    let class = ast::ClassDef::cast(node.clone())?;
    let Some(name_token) = class.name() else {
        diags.push(LoweringDiagnostic::MissingItemName {
            item_kind: "class",
            span: node.span_range(),
        });
        return None;
    };

    let generic_params = extract_generic_params_with_bounds(node, diags);
    let class_name = name_token.text().to_string();

    let fields = class
        .fields()
        .filter_map(|f| {
            let Some(fname) = f.name() else {
                diags.push(LoweringDiagnostic::MissingFieldName {
                    class_name: class_name.clone(),
                    span: f.syntax().span_range(),
                });
                return None;
            };
            let field_name_str = fname.text().to_string();
            let mut hoisted_field_attrs = Vec::new();
            // A field with no type is already reported by the parser ("field '<name>'
            // is missing a type annotation"), so recover with the error sentinel rather
            // than making the type optional: an absent type is not a kind of type, and
            // leaving it representable downstream forces every consumer to invent its
            // own stand-in. `Error` suppresses follow-on diagnostics while the rest of
            // the declaration still type-checks.
            let type_expr = f.ty().map_or_else(
                || TypeExprKind::Error { attrs: Vec::new() }.at(f.syntax().span_range()),
                |te| {
                    let mut expr = lower_type_expr::lower_type_expr_node(&te, diags);
                    let te_span = te.syntax().span_range();
                    check_unknown_type(
                        &expr,
                        format!("field `{class_name}.{field_name_str}`"),
                        te_span,
                        diags,
                    );
                    lower_type_expr::check_void_type(
                        &expr,
                        "a class field type".to_string(),
                        te_span,
                        false,
                        diags,
                    );
                    lower_type_expr::check_wildcard_type(
                        &mut expr,
                        "a class field type",
                        te_span,
                        diags,
                    );

                    // Hoist field attrs from the outermost TypeExpr to FieldDef.
                    // Only attrs that are direct ATTRIBUTE children of the outermost
                    // CST TYPE_EXPR are hoistable — attrs nested inside parens or
                    // generics are not (and will be flagged by validate_field_attrs).
                    let direct_attr_spans: std::collections::HashSet<text_size::TextRange> = te
                        .syntax()
                        .children()
                        .filter_map(ast::Attribute::cast)
                        .map(|a| a.syntax().span_range())
                        .collect();

                    let all_outer_attrs = std::mem::take(expr.attrs_mut());
                    let (hoist, keep): (Vec<_>, Vec<_>) =
                        all_outer_attrs.into_iter().partition(|a| {
                            crate::disambiguate::should_hoist_field_attr(a.name.as_str())
                                && direct_attr_spans.contains(&a.span)
                        });
                    *expr.attrs_mut() = keep;
                    hoisted_field_attrs = hoist;

                    expr.with_span(te_span)
                },
            );
            let field_docstring = crate::docstring::extract_docstring(f.syntax());
            Some(FieldDef {
                name: Name::new(&field_name_str),
                type_expr,
                attributes: hoisted_field_attrs,
                docstring: field_docstring,
                span: f.syntax().span_range(),
                name_span: fname.text_range(),
            })
        })
        .collect();

    let methods = class
        .methods()
        .filter_map(|f| lower_function(f.syntax(), diags, env_var_refs))
        .flat_map(|func| {
            let companions = expand_companions(&func);
            std::iter::once(func).chain(companions)
        })
        .collect();

    let implements = class
        .implements_blocks()
        .filter_map(|block| lower_implements_block(&block, diags, env_var_refs))
        .collect();

    let mut class_def = crate::ast::ClassDef {
        name: Name::new(name_token.text()),
        generic_params,
        fields,
        methods,
        implements,
        attributes: lower_attributes_from_node(node),
        docstring: crate::docstring::extract_docstring(node),
        span: node.span_range(),
        name_span: name_token.text_range(),
    };

    // No per-class JSON method synthesis: `to_json` / `from_json` are not real
    // methods. `obj.to_json()` desugars to `baml.json.from(obj)` and
    // `Type.from_json(j)` desugars to `baml.json.to<Type>(j)` (TIR + MIR);
    // customization is via `implements baml.ToJson` / `baml.FromJson`.

    // BEP-042: wrap a magic `cleanup(self) -> void` method in its run-once guard.
    crate::cleanup_guard::maybe_inject_cleanup_guard(&mut class_def);

    Some(class_def)
}

/// BEP-044 generic bounds: walk `GENERIC_PARAM_LIST` and return each
/// parameter's `Name` paired with **every** `&`-separated bound it declares
/// (`<T>` → `(T, [])`; `<T extends A & B>` → `(T, [A, B])`). The bound list is
/// a conjunction: an argument for `T` must satisfy all of them.
///
/// Bounds are captured as `TypeExpr`s so generic parents like `Container<int>`
/// round-trip; that they must denote *interfaces* (never interface-existential
/// types) is enforced when they are lowered to constraints in TIR.
pub(crate) fn extract_generic_params_with_bounds(
    node: &SyntaxNode,
    diags: &mut Vec<LoweringDiagnostic>,
) -> Vec<crate::ast::GenericParam> {
    use baml_compiler_syntax::SyntaxKind;

    let mut out: Vec<crate::ast::GenericParam> = Vec::new();
    for child in node.children() {
        if child.kind() != SyntaxKind::GENERIC_PARAM_LIST {
            continue;
        }
        for param_node in child.children() {
            if param_node.kind() != SyntaxKind::GENERIC_PARAM {
                continue;
            }
            let mut name: Option<Name> = None;
            for elem in param_node.children_with_tokens() {
                if let Some(token) = elem.as_token()
                    && token.kind() == SyntaxKind::WORD
                    && name.is_none()
                {
                    name = Some(Name::new(token.text()));
                }
            }
            let bounds: Vec<crate::ast::TypeExpr> = param_node
                .children()
                .find(|n| n.kind() == SyntaxKind::GENERIC_PARAM_BOUNDS)
                .map(|bounds_node| {
                    bounds_node
                        .children()
                        .filter_map(|n| {
                            let te = baml_compiler_syntax::ast::TypeExpr::cast(n)?;
                            let mut bound = lower_type_expr::lower_type_expr_node(&te, diags);
                            let span = bound.span;
                            lower_type_expr::check_wildcard_type(
                                &mut bound,
                                "a generic type bound",
                                span,
                                diags,
                            );
                            Some(bound)
                        })
                        .collect()
                })
                .unwrap_or_default();
            if let Some(name) = name {
                out.push(crate::ast::GenericParam { name, bounds });
            }
        }
    }
    out
}

fn lower_enum(node: &SyntaxNode, diags: &mut Vec<LoweringDiagnostic>) -> Option<EnumDef> {
    let enum_def = ast::EnumDef::cast(node.clone())?;
    let Some(name_token) = enum_def.name() else {
        diags.push(LoweringDiagnostic::MissingItemName {
            item_kind: "enum",
            span: node.span_range(),
        });
        return None;
    };
    let enum_name = name_token.text().to_string();

    let variants = enum_def
        .variants()
        .filter_map(|v| {
            let Some(vname) = v.name() else {
                diags.push(LoweringDiagnostic::MissingVariantName {
                    enum_name: enum_name.clone(),
                    span: v.syntax().span_range(),
                });
                return None;
            };
            let variant_docstring = crate::docstring::extract_docstring(v.syntax());
            Some(VariantDef {
                name: Name::new(vname.text()),
                attributes: lower_variant_attributes(&v),
                docstring: variant_docstring,
                span: v.syntax().span_range(),
                name_span: vname.text_range(),
            })
        })
        .collect();

    Some(EnumDef {
        name: Name::new(name_token.text()),
        variants,
        attributes: lower_attributes_from_node(node),
        docstring: crate::docstring::extract_docstring(node),
        span: node.span_range(),
        name_span: name_token.text_range(),
    })
}

fn lower_interface(
    node: &SyntaxNode,
    diags: &mut Vec<LoweringDiagnostic>,
    env_var_refs: &mut Vec<crate::EnvVarRef>,
) -> Option<InterfaceDef> {
    let iface = ast::InterfaceDef::cast(node.clone())?;
    let Some(name_token) = iface.name() else {
        diags.push(LoweringDiagnostic::MissingItemName {
            item_kind: "interface",
            span: node.span_range(),
        });
        return None;
    };
    let iface_name = name_token.text().to_string();
    let generic_params = extract_generic_params_with_bounds(node, diags);

    let parent_type_nodes: Vec<baml_compiler_syntax::ast::TypeExpr> =
        if let Some(c) = iface.requires_clause() {
            c.parents().collect()
        } else {
            Vec::new()
        };
    let requires: Vec<TypeExpr> = parent_type_nodes
        .into_iter()
        .map(|te| {
            let mut expr = lower_type_expr::lower_type_expr_node(&te, diags);
            let te_span = te.syntax().span_range();
            check_unknown_type(
                &expr,
                format!("requires clause of interface `{iface_name}`"),
                te_span,
                diags,
            );
            lower_type_expr::check_wildcard_type(
                &mut expr,
                "an interface `requires` clause",
                te_span,
                diags,
            );
            expr.with_span(te_span)
        })
        .collect();

    let fields = iface
        .fields()
        .filter_map(|f| {
            let Some(fname) = f.name() else {
                diags.push(LoweringDiagnostic::MissingFieldName {
                    class_name: iface_name.clone(),
                    span: f.syntax().span_range(),
                });
                return None;
            };
            let field_name_str = fname.text().to_string();
            // See the class-field site: the parser already reports a missing type, so
            // recover with the error sentinel instead of an optional type.
            let type_expr = f.ty().map_or_else(
                || TypeExprKind::Error { attrs: Vec::new() }.at(f.syntax().span_range()),
                |te| {
                    let mut expr = lower_type_expr::lower_type_expr_node(&te, diags);
                    let te_span = te.syntax().span_range();
                    check_unknown_type(
                        &expr,
                        format!("interface field `{iface_name}.{field_name_str}`"),
                        te_span,
                        diags,
                    );
                    lower_type_expr::check_void_type(
                        &expr,
                        "an interface field type".to_string(),
                        te_span,
                        false,
                        diags,
                    );
                    lower_type_expr::check_wildcard_type(
                        &mut expr,
                        "an interface field type",
                        te_span,
                        diags,
                    );
                    expr.with_span(te_span)
                },
            );
            Some(FieldDef {
                name: Name::new(&field_name_str),
                type_expr,
                attributes: lower_attributes_from_node(f.syntax()),
                docstring: crate::docstring::extract_docstring(f.syntax()),
                span: f.syntax().span_range(),
                name_span: fname.text_range(),
            })
        })
        .collect();

    let required_methods = iface
        .required_methods()
        .filter_map(|sig| lower_method_sig(&sig, diags))
        .collect();

    let associated_types = iface
        .associated_types()
        .filter_map(|decl| lower_associated_type_def(&decl, diags))
        .collect();

    let default_methods = iface
        .default_methods()
        .filter_map(|f| lower_function(f.syntax(), diags, env_var_refs))
        .collect();

    Some(InterfaceDef {
        name: Name::new(&iface_name),
        generic_params,
        requires,
        fields,
        associated_types,
        required_methods,
        default_methods,
        attributes: lower_attributes_from_node(node),
        docstring: crate::docstring::extract_docstring(node),
        span: node.span_range(),
        name_span: name_token.text_range(),
    })
}

fn lower_associated_type_def(
    decl: &ast::AssociatedTypeDecl,
    diags: &mut Vec<LoweringDiagnostic>,
) -> Option<AssociatedTypeDef> {
    let Some(name_token) = decl.name() else {
        diags.push(LoweringDiagnostic::MissingItemName {
            item_kind: "associated type",
            span: decl.syntax().span_range(),
        });
        return None;
    };
    let name = Name::new(name_token.text());
    let bound = decl.bound().map(|te| {
        let expr = lower_type_expr::lower_type_expr_node(&te, diags);
        let span = te.syntax().span_range();
        check_unknown_type(
            &expr,
            format!("bound of associated type `{name}`"),
            span,
            diags,
        );
        expr.with_span(span)
    });
    let default = decl.default_or_binding().map(|te| {
        let expr = lower_type_expr::lower_type_expr_node(&te, diags);
        let span = te.syntax().span_range();
        check_unknown_type(
            &expr,
            format!("default of associated type `{name}`"),
            span,
            diags,
        );
        expr.with_span(span)
    });
    Some(AssociatedTypeDef {
        name,
        bound,
        default,
        span: decl.syntax().span_range(),
        name_span: name_token.text_range(),
    })
}

fn lower_associated_type_binding_def(
    decl: &ast::AssociatedTypeDecl,
    diags: &mut Vec<LoweringDiagnostic>,
) -> Option<AssociatedTypeBindingDef> {
    let Some(name_token) = decl.name() else {
        diags.push(LoweringDiagnostic::MissingItemName {
            item_kind: "associated type binding",
            span: decl.syntax().span_range(),
        });
        return None;
    };
    let name = Name::new(name_token.text());
    let type_expr = decl.default_or_binding().map(|te| {
        let expr = lower_type_expr::lower_type_expr_node(&te, diags);
        let span = te.syntax().span_range();
        check_unknown_type(
            &expr,
            format!("binding of associated type `{name}`"),
            span,
            diags,
        );
        expr.with_span(span)
    });
    Some(AssociatedTypeBindingDef {
        name,
        type_expr,
        span: decl.syntax().span_range(),
        name_span: name_token.text_range(),
    })
}

fn lower_method_sig(
    sig: &ast::MethodSig,
    diags: &mut Vec<LoweringDiagnostic>,
) -> Option<MethodSigDef> {
    let Some(name_token) = sig.name() else {
        diags.push(LoweringDiagnostic::MissingItemName {
            item_kind: "method signature",
            span: sig.syntax().span_range(),
        });
        return None;
    };
    let name = Name::new(name_token.text());
    let name_span = name_token.text_range();
    let generic_params = extract_generic_params_with_bounds(sig.syntax(), diags);
    let parameter_context = format!("method signature `{}`", name.as_str());

    let (params, defaults) = sig
        .param_list()
        .map(|pl| lower_params(&pl, name.as_str(), &parameter_context, diags))
        .map(|params| (params, FunctionDefaults::empty()))
        .unwrap_or_else(|| (Vec::new(), FunctionDefaults::empty()));

    let return_type = sig.return_type().map(|te| {
        let mut expr = lower_type_expr::lower_type_expr_node(&te, diags);
        let te_span = te.syntax().span_range();
        check_unknown_type(&expr, format!("return type of `{name}`"), te_span, diags);
        lower_type_expr::check_void_type(
            &expr,
            format!("return type of `{name}`"),
            te_span,
            true,
            diags,
        );
        lower_type_expr::check_wildcard_type(&mut expr, "a return type", te_span, diags);
        expr.with_span(te_span)
    });

    let throws = sig.throws_clause().and_then(|tc| tc.type_expr()).map(|te| {
        let mut expr = lower_type_expr::lower_type_expr_node(&te, diags);
        let te_span = te.syntax().span_range();
        // A bodyless method signature (interface required method) has nothing to
        // infer an open `throws … | _` from, and its declared throws is compared
        // structurally during conformance checking — so reject ANY `_` here
        // (unlike a function with a body, where a top-level `_` is the open slot).
        lower_type_expr::check_wildcard_type(
            &mut expr,
            "a method signature `throws` clause",
            te_span,
            diags,
        );
        expr.with_span(te_span)
    });

    Some(MethodSigDef {
        name,
        generic_params,
        params,
        defaults,
        return_type,
        throws,
        attributes: lower_attributes_from_node(sig.syntax()),
        docstring: crate::docstring::extract_docstring(sig.syntax()),
        span: sig.syntax().span_range(),
        name_span,
    })
}

fn lower_implements_block(
    block: &ast::ImplementsBlock,
    diags: &mut Vec<LoweringDiagnostic>,
    env_var_refs: &mut Vec<crate::EnvVarRef>,
) -> Option<ImplementsBlockDef> {
    let target_node = block.target()?;
    let target_te = target_node.type_expr()?;
    let target_span = target_te.syntax().span_range();
    let target = lower_type_expr::lower_type_expr_node(&target_te, diags).with_span(target_span);
    check_unknown_type(
        &target,
        "interface name in `implements`".to_string(),
        target_span,
        diags,
    );

    let target_label = match &target.kind {
        crate::ast::TypeExprKind::Path { segments, .. } => segments
            .last()
            .map(|n: &Name| n.to_string())
            .unwrap_or_else(|| "?".to_string()),
        _ => "?".to_string(),
    };

    for f in block.fields() {
        let field_name = f
            .name()
            .map(|name| name.text().to_string())
            .unwrap_or_else(|| "?".to_string());
        diags.push(
            LoweringDiagnostic::InterfaceFieldDeclaredInImplementsBlock {
                interface_name: target_label.clone(),
                field_name,
                span: f.syntax().span_range(),
            },
        );
    }

    let field_links = block
        .field_links()
        .filter_map(|link| lower_interface_field_link(&link, diags))
        .collect();

    let associated_type_bindings = block
        .associated_type_bindings()
        .filter_map(|decl| lower_associated_type_binding_def(&decl, diags))
        .collect();

    let methods = block
        .methods()
        .filter_map(|f| lower_function(f.syntax(), diags, env_var_refs))
        .collect();

    Some(ImplementsBlockDef {
        target,
        field_links,
        associated_type_bindings,
        methods,
        is_out_of_body: false,
        span: block.syntax().span_range(),
    })
}

fn lower_implements_for(
    node: &SyntaxNode,
    diags: &mut Vec<LoweringDiagnostic>,
    env_var_refs: &mut Vec<crate::EnvVarRef>,
) -> Option<ImplementsForDef> {
    let imp = ast::ImplementsFor::cast(node.clone())?;

    let generic_params = extract_generic_params_with_bounds(node, diags);

    // Interface target (the `I` in `implements I for T`)
    let target_node = imp.target()?;
    let target_te = target_node.type_expr()?;
    let target_span = target_te.syntax().span_range();
    let interface_target =
        lower_type_expr::lower_type_expr_node(&target_te, diags).with_span(target_span);
    check_unknown_type(
        &interface_target,
        "interface name in `implements ... for`".to_string(),
        target_span,
        diags,
    );

    // For target (the `T` in `implements I for T`)
    let for_node = imp.for_target()?;
    let for_te = for_node.type_expr()?;
    let for_span = for_te.syntax().span_range();
    let for_target = lower_type_expr::lower_type_expr_node(&for_te, diags).with_span(for_span);
    check_unknown_type(
        &for_target,
        "target type in `implements ... for`".to_string(),
        for_span,
        diags,
    );

    let iface_label = match &interface_target.kind {
        crate::ast::TypeExprKind::Path { segments, .. } => segments
            .last()
            .map(|n: &Name| n.to_string())
            .unwrap_or_else(|| "?".to_string()),
        _ => "?".to_string(),
    };

    for f in imp.fields() {
        let field_name = f
            .name()
            .map(|name| name.text().to_string())
            .unwrap_or_else(|| "?".to_string());
        diags.push(
            LoweringDiagnostic::InterfaceFieldDeclaredInImplementsBlock {
                interface_name: iface_label.clone(),
                field_name,
                span: f.syntax().span_range(),
            },
        );
    }

    let field_links = imp
        .field_links()
        .filter_map(|link| lower_interface_field_link(&link, diags))
        .collect();

    let associated_type_bindings = imp
        .associated_type_bindings()
        .filter_map(|decl| lower_associated_type_binding_def(&decl, diags))
        .collect();

    let methods = imp
        .methods()
        .filter_map(|f| lower_function(f.syntax(), diags, env_var_refs))
        .collect();

    Some(ImplementsForDef {
        generic_params,
        interface_target,
        for_target,
        field_links,
        associated_type_bindings,
        methods,
        span: node.span_range(),
        docstring: crate::docstring::extract_docstring(node),
    })
}

fn lower_interface_field_link(
    link: &ast::InterfaceFieldLink,
    diags: &mut Vec<LoweringDiagnostic>,
) -> Option<InterfaceFieldLinkDef> {
    let Some(interface_field) = link.interface_field() else {
        diags.push(LoweringDiagnostic::MissingFieldName {
            class_name: "interface field link".to_string(),
            span: link.syntax().span_range(),
        });
        return None;
    };
    let Some(class_field) = link.class_field() else {
        diags.push(LoweringDiagnostic::MissingFieldName {
            class_name: "interface field link".to_string(),
            span: link.syntax().span_range(),
        });
        return None;
    };
    Some(InterfaceFieldLinkDef {
        interface_field: Name::new(interface_field.text()),
        class_field: Name::new(class_field.text()),
        span: link.syntax().span_range(),
        interface_field_span: interface_field.text_range(),
        class_field_span: class_field.text_range(),
    })
}

fn lower_type_alias(
    node: &SyntaxNode,
    diags: &mut Vec<LoweringDiagnostic>,
) -> Option<TypeAliasDef> {
    let alias = ast::TypeAliasDef::cast(node.clone())?;
    let Some(name_token) = alias.name() else {
        diags.push(LoweringDiagnostic::MissingItemName {
            item_kind: "type alias",
            span: node.span_range(),
        });
        return None;
    };

    let alias_name = name_token.text().to_string();
    Some(TypeAliasDef {
        name: Name::new(&alias_name),
        type_expr: alias.ty().map(|te| {
            let mut expr = lower_type_expr::lower_type_expr_node(&te, diags);
            let te_span = te.syntax().span_range();
            check_unknown_type(&expr, format!("type alias `{alias_name}`"), te_span, diags);
            lower_type_expr::check_void_type(
                &expr,
                "a type alias".to_string(),
                te_span,
                false,
                diags,
            );
            lower_type_expr::check_wildcard_type(&mut expr, "a type alias", te_span, diags);
            expr.with_span(te_span)
        }),
        span: node.span_range(),
        name_span: name_token.text_range(),
        docstring: crate::docstring::extract_docstring(node),
    })
}

fn lower_test(node: &SyntaxNode, diags: &mut Vec<LoweringDiagnostic>) -> Option<TestDef> {
    let test = ast::TestDef::cast(node.clone())?;
    let Some(name_token) = test.name() else {
        diags.push(LoweringDiagnostic::MissingItemName {
            item_kind: "test",
            span: node.span_range(),
        });
        return None;
    };

    let test_name = name_token.text().to_string();
    let config_block = test.config_block();
    if let Some(block) = &config_block {
        for item in block.items() {
            if item.key().is_none() {
                diags.push(LoweringDiagnostic::MissingConfigKey {
                    block_kind: "test",
                    block_name: test_name.clone(),
                    span: item.syntax().span_range(),
                });
            }
        }
    }
    let function_refs = test
        .function_reference_names()
        .into_iter()
        .map(Name::new)
        .collect();
    let args = config_block
        .as_ref()
        .and_then(|block| block.items().find(|item| item.matches_key("args")))
        .and_then(|item| item.nested_block())
        .map(|block| lower_test_arg_map(&block))
        .unwrap_or_default();

    Some(TestDef {
        name: Name::new(&test_name),
        function_refs,
        args,
        span: node.span_range(),
        name_span: name_token.text_range(),
    })
}

fn lower_test_arg_map(block: &ast::ConfigBlock) -> Vec<(Name, TestArgValue)> {
    block
        .items()
        .filter_map(|item| {
            let key = item.key()?;
            Some((Name::new(key.text()), lower_test_arg_item(&item)))
        })
        .collect()
}

fn lower_test_arg_map_as_value(block: &ast::ConfigBlock) -> TestArgValue {
    TestArgValue::Map(
        lower_test_arg_map(block)
            .into_iter()
            .map(|(key, value)| (key.to_string(), value))
            .collect(),
    )
}

fn lower_test_arg_item(item: &ast::ConfigItem) -> TestArgValue {
    if let Some(block) = item.nested_block() {
        return lower_test_arg_map_as_value(&block);
    }

    item.config_value_node()
        .map(|value| lower_test_arg_config_value(&value))
        .unwrap_or(TestArgValue::Null)
}

fn lower_test_arg_config_value(value: &SyntaxNode) -> TestArgValue {
    if let Some(array) = value
        .children()
        .find(|child| child.kind() == SyntaxKind::ARRAY_LITERAL)
    {
        return TestArgValue::Array(
            array
                .children()
                .filter_map(|element| match element.kind() {
                    SyntaxKind::CONFIG_VALUE => Some(lower_test_arg_config_value(&element)),
                    SyntaxKind::CONFIG_BLOCK => ast::ConfigBlock::cast(element)
                        .map(|block| lower_test_arg_map_as_value(&block)),
                    _ => None,
                })
                .collect(),
        );
    }

    let raw = value.text().to_string();
    if let Some(string) = crate::parse_string_attr_value(raw.trim()) {
        return TestArgValue::String(string);
    }

    let text = ast::ConfigValue::cast(value.clone())
        .and_then(|config_value| config_value.scalar_text())
        .unwrap_or_default();

    match text.as_str() {
        "null" => return TestArgValue::Null,
        "true" => return TestArgValue::Bool(true),
        "false" => return TestArgValue::Bool(false),
        _ => {}
    }

    // Duck-typed scalar: number-shaped text becomes a number, everything
    // else stays a string, so no diagnostics here. `num_lit` handles base
    // prefixes and underscores; a leading `-` is handled by hand since the
    // helper only accepts unsigned magnitudes.
    let (negated, magnitude) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.as_str()),
    };
    if let Ok(value) = baml_base::num_lit::parse_int_literal(magnitude) {
        return TestArgValue::Int(if negated { -value } else { value });
    }
    if let Ok(value) = text.parse::<f64>() {
        return TestArgValue::float(value);
    }
    // Underscored floats (`1_000.5`) fail the plain parse; retry with
    // separators stripped, but only for digit-led text so words containing
    // underscores (`in_f`) can't be misread as `inf`.
    if magnitude.starts_with(|c: char| c.is_ascii_digit())
        && text.contains('_')
        && let Ok(value) = baml_base::num_lit::normalize_float_literal(&text).parse::<f64>()
    {
        return TestArgValue::float(value);
    }

    TestArgValue::String(text)
}

/// Extract the name expression element from a `TEST_EXPR_DEF` or `TESTSET_DEF` node.
///
/// Returns the first non-trivial child (node or token) after the keyword
/// (`KW_TEST` or `KW_TESTSET`) and before `KW_WITH` or `BLOCK_EXPR`.
fn extract_name_element(
    node: &SyntaxNode,
    keyword_kind: SyntaxKind,
) -> Option<baml_compiler_syntax::SyntaxElement> {
    let mut past_keyword = false;
    for child in node.children_with_tokens() {
        let kind = child.kind();
        // Skip whitespace/newline trivia
        if matches!(
            kind,
            SyntaxKind::WHITESPACE | SyntaxKind::NEWLINE | SyntaxKind::LINE_COMMENT
        ) {
            continue;
        }
        if kind == keyword_kind {
            past_keyword = true;
            continue;
        }
        if past_keyword && kind != SyntaxKind::KW_WITH && kind != SyntaxKind::BLOCK_EXPR {
            return Some(child);
        }
        if kind == SyntaxKind::KW_WITH || kind == SyntaxKind::BLOCK_EXPR {
            break;
        }
    }
    None
}

/// Extract the runner expression element from a `TEST_EXPR_DEF` or `TESTSET_DEF` node.
///
/// Returns the first non-trivial child (node or token) that appears after `KW_WITH` and
/// before `BLOCK_EXPR`. This uses `children_with_tokens()` because the runner expression
/// may be a bare token (e.g. `INTEGER_LITERAL "42"`) rather than a wrapped node.
pub(crate) fn extract_runner_element(
    node: &SyntaxNode,
) -> Option<baml_compiler_syntax::SyntaxElement> {
    let mut found_with = false;
    for child in node.children_with_tokens() {
        let kind = child.kind();
        // Skip whitespace/newline trivia
        if matches!(
            kind,
            SyntaxKind::WHITESPACE | SyntaxKind::NEWLINE | SyntaxKind::LINE_COMMENT
        ) {
            continue;
        }
        if kind == SyntaxKind::KW_WITH {
            found_with = true;
            continue;
        }
        if found_with && kind != SyntaxKind::BLOCK_EXPR {
            return Some(child);
        }
        if kind == SyntaxKind::BLOCK_EXPR {
            break;
        }
    }
    None
}

/// Lower a `TEST_EXPR_DEF` CST node into a `TestRegistrationItem::Test`.
///
/// The CST structure is:
/// `TEST_EXPR_DEF [ KW_TEST STRING_LITERAL [KW_WITH expr] BLOCK_EXPR ]`
fn lower_test_expr(node: &SyntaxNode) -> Option<TestRegistrationItem> {
    // Find the name expression — first child element after KW_TEST, before KW_WITH/BLOCK_EXPR.
    let name_element = extract_name_element(node, SyntaxKind::KW_TEST)?;

    // Find the optional runner expression (first child after KW_WITH, before BLOCK_EXPR)
    let runner_element = extract_runner_element(node);

    // Find the BLOCK_EXPR child (the test body)
    let body_node = node
        .children()
        .find(|c| c.kind() == SyntaxKind::BLOCK_EXPR)?;

    Some(TestRegistrationItem::Test {
        name_element,
        body_node,
        runner_element,
    })
}

/// Lower a `TESTSET_DEF` CST node into a `TestRegistrationItem::TestSet`.
///
/// The CST structure is:
/// `TESTSET_DEF [ KW_TESTSET STRING_LITERAL [KW_WITH expr] BLOCK_EXPR ]`
///
/// The `BLOCK_EXPR` body may contain setup statements (let bindings), for/if control
/// flow, and nested `TEST_EXPR_DEF` / `TESTSET_DEF` nodes. The entire body is stored
/// and lowered lazily into a collector lambda body.
fn lower_testset(node: &SyntaxNode) -> Option<TestRegistrationItem> {
    // Find the name expression — first child element after KW_TESTSET, before KW_WITH/BLOCK_EXPR.
    let name_element = extract_name_element(node, SyntaxKind::KW_TESTSET)?;

    // Find the optional runner expression (first child after KW_WITH, before BLOCK_EXPR)
    let runner_element = extract_runner_element(node);

    // Find the BLOCK_EXPR child (the full testset body)
    let body_node = node
        .children()
        .find(|c| c.kind() == SyntaxKind::BLOCK_EXPR)?;

    Some(TestRegistrationItem::TestSet {
        name_element,
        body_node,
        runner_element,
    })
}

/// Synthesize a `$init_test` function that registers tests and testsets
/// into a `testing.Registry` parameter.
///
/// The function body calls `registry.register_test(name, lambda, null)` for each test
/// and `registry.register_test_set(name, collector_lambda, null)` for each testset.
///
/// Lambda bodies are lowered from the original CST `BLOCK_EXPR` nodes.
///
/// Derive the `$init_test_<key>` suffix from a file path.
///
/// Strips the extension and replaces every character that isn't a letter,
/// digit, or `_` (notably the path separators and `.`, which would otherwise
/// break the `.`-delimited namespace routing of function names). The result is
/// stable across compilations and unique per file, e.g.
/// `ns_arrays/arrays.baml` -> `ns_arrays_arrays`.
fn init_test_key_from_path(path: &std::path::Path) -> String {
    path.with_extension("")
        .to_string_lossy()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Public owner of tests declared in `path`. User package names are deliberately
/// not embedded in test ids: `root` is stable across package renames, while
/// `ns_<name>` directories retain BAML's dotted namespace qualification.
fn test_owner_from_path(path: Option<&std::path::Path>) -> String {
    let mut namespaces = Vec::new();
    if let Some(path) = path {
        for component in path
            .parent()
            .into_iter()
            .flat_map(std::path::Path::components)
        {
            let std::path::Component::Normal(component) = component else {
                continue;
            };
            let Some(component) = component.to_str() else {
                continue;
            };
            let Some(name) = component.strip_prefix("ns_") else {
                continue;
            };
            let mut chars = name.chars();
            let valid = chars
                .next()
                .map(|c| c.is_ascii_alphabetic() || c == '_')
                .unwrap_or(false)
                && chars.all(|c| c.is_ascii_alphanumeric() || c == '_');
            if valid {
                namespaces.push(name.to_string());
            }
        }
    }
    if namespaces.is_empty() {
        "root".to_string()
    } else {
        format!("root.{}", namespaces.join("."))
    }
}

/// The function is named `"$init_test_<sanitized_path>"` to avoid collisions
/// when multiple files contain tests. The suffix is derived from the file path
/// rather than its `FileId`: a `FileId` is a load-order index that shifts
/// whenever an earlier file is added (e.g. a new stdlib file), which would
/// churn every snapshot referencing the name. The path is stable across
/// compilations. When no path is available (e.g. PPIR, which processes files
/// individually), uses plain `"$init_test"`.
fn synthesize_init_test_function(
    registrations: &[TestRegistrationItem],
    file_path: Option<&std::path::Path>,
    explicit_test_owner: Option<&str>,
    diags: &mut Vec<LoweringDiagnostic>,
    env_var_refs: &mut Vec<crate::EnvVarRef>,
) -> FunctionDef {
    let fn_name = match file_path {
        Some(path) => format!("$init_test_{}", init_test_key_from_path(path)),
        None => "$init_test".to_string(),
    };
    let span = text_size::TextRange::default();

    let mut ctx = lower_expr_body::InitTestContext::new();
    let test_owner = explicit_test_owner
        .map(str::to_owned)
        .unwrap_or_else(|| test_owner_from_path(file_path));

    // Build statements: one per registration
    let mut stmt_ids: Vec<crate::ast::StmtId> = Vec::with_capacity(registrations.len());
    for reg in registrations {
        let stmt_expr = synthesize_register_call(reg, &test_owner, &mut ctx);
        stmt_ids.push(ctx.alloc_stmt(crate::ast::Stmt::Expr(stmt_expr), span));
    }

    // Block expression containing all registration calls, with a null tail
    let null_expr = ctx.alloc_expr(Expr::Null, span);
    let block_expr = ctx.alloc_expr(
        Expr::Block {
            stmts: stmt_ids,
            tail_expr: Some(null_expr),
        },
        span,
    );

    let (body, source_map, finish_diags, finish_env_refs) = ctx.finish(Some(block_expr));
    diags.extend(finish_diags);
    env_var_refs.extend(finish_env_refs);

    // The single parameter: `registry: testing.TestCollector`
    let registry_param = Param {
        name: Name::new("registry"),
        type_expr: Some(
            crate::ast::TypeExprKind::Path {
                segments: vec![Name::new("testing"), Name::new("TestCollector")],
                generic_args: vec![],
                associated_type_bindings: vec![],
                attrs: vec![],
            }
            .at(span),
        ),
        default: None,
        span,
        name_span: span,
    };

    FunctionDef {
        name: Name::new(&fn_name),
        generic_params: vec![],
        params: vec![registry_param],
        defaults: FunctionDefaults::empty(),
        return_type: None,
        throws: None,
        body: Some(FunctionBodyDef::Expr(body, source_map)),
        declarative_meta: None,
        metadata: crate::ast::FunctionMetadata::language_internal(
            crate::ast::FunctionOrigin::Internal,
        ),
        attributes: vec![],
        docstring: None,
        is_tagged_template_tag: false,
        span,
        name_span: span,
    }
}

/// Synthesize a single `registry.register_test(name, lambda, runner)` or
/// `registry.register_test_set(name, collector_lambda, runner)` call expression.
fn synthesize_register_call(
    reg: &TestRegistrationItem,
    test_owner: &str,
    ctx: &mut lower_expr_body::InitTestContext,
) -> ExprId {
    let span = text_size::TextRange::default();
    match reg {
        TestRegistrationItem::Test {
            name_element,
            body_node,
            runner_element,
        } => {
            // The body lowers into `$init_test`'s own arena.
            let lambda_body = ctx.lower_test_body(body_node, span);

            let lambda_def = LambdaDef {
                kind: LambdaKind::Anonymous,
                params: vec![],
                defaults: FunctionDefaults::empty(),
                return_type: Some(crate::ast::TypeExprKind::Void { attrs: vec![] }.at(span)),
                throws: None,
                body: Some(lambda_body),
                span,
            };

            // registry.register_test_at(owner, ...)
            let method_call_target = ctx.alloc_expr(
                Expr::Path(vec![Name::new("registry"), Name::new("register_test_at")]),
                span,
            );

            // Args: (name_expr, lambda, runner_or_null)
            let name_arg = lower_expr_body::lower_runner_element(ctx, name_element);
            let owner_arg = ctx.alloc_expr(
                Expr::Literal(crate::ast::Literal::String(test_owner.to_string())),
                span,
            );
            // Use the test body's real CST range as the lambda's span so HIR
            // scope lookup resolves names inside the body correctly. The body's
            // statements carry real source offsets, so a synthetic span (disjoint
            // from those offsets) would make `scope_at_offset` miss the lambda
            // scope, and every `let`-bound local would fail to resolve (reading
            // the local would fall back to a null placeholder). The range is
            // unique per test block, so distinct lambda scopes stay
            // distinguishable. Mirrors the testset collector lambda below.
            let lambda_span = body_node.span_range();
            let lambda_arg = ctx.alloc_expr(Expr::Lambda(Box::new(lambda_def)), lambda_span);
            let runner_arg = lower_runner_element(runner_element.as_ref(), ctx, span);

            ctx.alloc_expr(
                Expr::Call {
                    callee: method_call_target,
                    type_args: vec![],
                    args: vec![
                        CallArg::positional(owner_arg),
                        CallArg::positional(name_arg),
                        CallArg::positional(lambda_arg),
                        CallArg::positional(runner_arg),
                    ],
                },
                span,
            )
        }
        TestRegistrationItem::TestSet {
            name_element,
            body_node,
            runner_element,
        } => {
            let collector_exprs = ctx.lower_testset_body(body_node, Name::new("testset"), span);

            // Collector lambda parameter: `testset`
            let testset_param = Param {
                name: Name::new("testset"),
                type_expr: Some(
                    crate::ast::TypeExprKind::Path {
                        segments: vec![Name::new("testing"), Name::new("TestCollector")],
                        generic_args: vec![],
                        associated_type_bindings: vec![],
                        attrs: vec![],
                    }
                    .at(span),
                ),
                default: None,
                span,
                name_span: span,
            };

            let collector_def = LambdaDef {
                kind: LambdaKind::Anonymous,
                params: vec![testset_param],
                defaults: FunctionDefaults::empty(),
                return_type: Some(crate::ast::TypeExprKind::Void { attrs: vec![] }.at(span)),
                throws: None,
                body: Some(collector_exprs),
                span,
            };

            // registry.register_test_set_at(owner, ...)
            let method_call_target = ctx.alloc_expr(
                Expr::Path(vec![
                    Name::new("registry"),
                    Name::new("register_test_set_at"),
                ]),
                span,
            );

            // Args: (name_expr, collector_lambda, runner_or_null)
            let name_arg = lower_expr_body::lower_runner_element(ctx, name_element);
            let owner_arg = ctx.alloc_expr(
                Expr::Literal(crate::ast::Literal::String(test_owner.to_string())),
                span,
            );
            // Use the testset body's real CST range so HIR scope lookup works
            // correctly for name resolution inside the collector lambda body.
            let collector_lambda_span = body_node.span_range();
            let collector_arg =
                ctx.alloc_expr(Expr::Lambda(Box::new(collector_def)), collector_lambda_span);
            let runner_arg = lower_runner_element(runner_element.as_ref(), ctx, span);

            ctx.alloc_expr(
                Expr::Call {
                    callee: method_call_target,
                    type_args: vec![],
                    args: vec![
                        CallArg::positional(owner_arg),
                        CallArg::positional(name_arg),
                        CallArg::positional(collector_arg),
                        CallArg::positional(runner_arg),
                    ],
                },
                span,
            )
        }
    }
}

/// Lower an optional runner CST element directly into the parent arena.
///
/// If present, the runner expression is lowered into the same `InitTestContext` arena
/// (no IIFE wrapping needed). If absent, returns `Expr::Null`.
fn lower_runner_element(
    runner_element: Option<&baml_compiler_syntax::SyntaxElement>,
    ctx: &mut lower_expr_body::InitTestContext,
    span: text_size::TextRange,
) -> ExprId {
    match runner_element {
        Some(element) => lower_expr_body::lower_runner_element(ctx, element),
        None => ctx.alloc_expr(Expr::Null, span),
    }
}

/// Build the migration warning for a deprecated top-level `generator { … }`
/// block. The block is otherwise ignored — generators are configured in
/// `baml.toml` now (see `baml_cli`'s `discover_generators`).
fn lower_generator_deprecation(node: &SyntaxNode) -> LoweringDiagnostic {
    // Point the diagnostic at the `generator <name>` header, not the whole
    // node — the node's `text_range()` would swallow leading trivia and the
    // entire opaque body, producing an ugly multi-line caret.
    let mut kw_range = None;
    let mut name = None;
    let mut name_range = None;
    for token in node
        .children_with_tokens()
        .filter_map(baml_compiler_syntax::NodeOrToken::into_token)
    {
        match token.kind() {
            SyntaxKind::KW_GENERATOR => kw_range = Some(token.text_range()),
            SyntaxKind::WORD if name.is_none() => {
                name = Some(token.text().to_string());
                name_range = Some(token.text_range());
            }
            // Stop before the opaque `{ … }` body.
            SyntaxKind::L_BRACE => break,
            _ => {}
        }
    }
    let span = match (kw_range, name_range) {
        (Some(kw), Some(nm)) => text_size::TextRange::new(kw.start(), nm.end()),
        (Some(kw), None) => kw,
        _ => node.span_range(),
    };
    LoweringDiagnostic::GeneratorBlockInBaml { name, span }
}

fn lower_template_string(
    node: &SyntaxNode,
    diags: &mut Vec<LoweringDiagnostic>,
) -> Option<TemplateStringDef> {
    let ts = ast::TemplateStringDef::cast(node.clone())?;
    let Some(name_token) = ts.name() else {
        diags.push(LoweringDiagnostic::MissingItemName {
            item_kind: "template_string",
            span: node.span_range(),
        });
        return None;
    };

    let ts_name = name_token.text().to_string();
    let context = format!("template_string `{ts_name}`");
    let params = ts
        .param_list()
        .map(|pl| lower_params(&pl, &ts_name, &context, diags))
        .unwrap_or_default();

    let body = ts.raw_string().map(|rs| lower_raw_prompt(&rs));

    Some(TemplateStringDef {
        name: Name::new(name_token.text()),
        params,
        body,
        span: node.span_range(),
        name_span: name_token.text_range(),
    })
}

/// Synthesize an `Item::Let` for a `retry_policy` declaration.
///
/// Produces: `baml.llm.RetryPolicy { max_retries: N, initial_delay_ms: N, multiplier: F, max_delay_ms: N }`
///
/// Each config field is lowered generically via `lower_config_item::lower_config_value`,
/// then wrapped in a typed `Expr::Object`.
fn synthesize_retry_policy_let(
    node: &SyntaxNode,
    diags: &mut Vec<LoweringDiagnostic>,
    env_var_refs: &mut Vec<crate::EnvVarRef>,
) -> Option<Item> {
    let rp = ast::RetryPolicyDef::cast(node.clone())?;
    let Some(name_token) = rp.name() else {
        diags.push(LoweringDiagnostic::MissingItemName {
            item_kind: "retry_policy",
            span: node.span_range(),
        });
        return None;
    };
    let span = node.span_range();
    let rp_name = name_token.text().to_string();
    let Some(config_block) = rp.config_block() else {
        diags.push(LoweringDiagnostic::MissingConfigBlock {
            block_kind: "retry_policy",
            block_name: rp_name,
            span,
        });
        return None;
    };

    let mut exprs: la_arena::Arena<Expr> = la_arena::Arena::new();
    let mut expr_spans: la_arena::Arena<text_size::TextRange> = la_arena::Arena::new();
    let mut alloc = |expr: Expr| -> ExprId {
        let id = exprs.alloc(expr);
        expr_spans.alloc(span);
        id
    };

    // Lower each config item generically
    let fields: Vec<(Name, ExprId)> = config_block
        .items()
        .filter_map(|item| {
            let Some(key) = item.key() else {
                diags.push(LoweringDiagnostic::MissingConfigKey {
                    block_kind: "retry_policy",
                    block_name: rp_name.clone(),
                    span: item.syntax().span_range(),
                });
                return None;
            };
            let value = crate::lower_config_item::lower_config_value_with_env_refs(
                &item,
                &mut alloc,
                env_var_refs,
                crate::lower_config_item::EnvReadMode::Strict,
            );
            Some((Name::new(key.text()), value))
        })
        .collect();

    let root = alloc(Expr::Object {
        type_name: TypePath::from_dotted("baml.llm.RetryPolicy"),
        type_args: vec![],
        fields,
        spreads: vec![],
    });

    let body = ExprBody {
        exprs,
        stmts: la_arena::Arena::new(),
        patterns: la_arena::Arena::new(),
        match_arms: la_arena::Arena::new(),
        catch_arms: la_arena::Arena::new(),
        type_annotations: la_arena::Arena::new(),
        root_expr: Some(root),
    };
    let mut source_map = AstSourceMap {
        expr_spans,
        ..Default::default()
    };
    // The object constructor is an implementation detail of `retry_policy`
    // config lowering, not a user-written class literal. TIR uses this marker
    // to avoid applying ordinary object-literal field diagnostics to config
    // keys such as the legacy `strategy` block.
    source_map.synthetic_exprs.insert(root);

    Some(Item::Let(LetDef {
        name: Name::new(name_token.text()),
        initializer: Some((body, source_map)),
        origin: LetOrigin::RetryPolicy,
        span,
        name_span: name_token.text_range(),
    }))
}

/// Synthesize `Item::Let` + optional `Item::Function` from a `CLIENT_DEF` CST node.
///
/// - Every client produces an `Item::Let("ClientName", LetOrigin::Client)` whose initializer
///   constructs `Client { name, client_type, sub_clients: [], retry: null }`.
/// - Primitive clients also produce an `Item::Function("ClientName$new")` whose body
///   constructs `PrimitiveClient { name, provider, options }`.
fn synthesize_client_items(
    node: &SyntaxNode,
    diags: &mut Vec<LoweringDiagnostic>,
    env_var_refs: &mut Vec<crate::EnvVarRef>,
) -> Option<(Item, Option<FunctionDef>)> {
    let client = ast::ClientDef::cast(node.clone())?;
    let Some(name_token) = client.name() else {
        diags.push(LoweringDiagnostic::MissingItemName {
            item_kind: "client",
            span: node.span_range(),
        });
        return None;
    };
    let client_name = name_token.text().to_string();
    let span = node.span_range();
    let Some(config_block) = client.config_block() else {
        diags.push(LoweringDiagnostic::MissingConfigBlock {
            block_kind: "client",
            block_name: client_name,
            span,
        });
        return None;
    };

    // Determine provider
    let provider: Option<String> = config_block.items().find_map(|item| {
        let key = item.key()?;
        if key.text() != "provider" {
            return None;
        }
        item.value_word().map(|w| w.text().to_string()).or_else(|| {
            item.config_value()
                .and_then(|cv| cv.scalar_text())
                .map(|t| t.trim().trim_matches('"').to_string())
        })
    });

    // Validate provider name.
    if let Some(p) = &provider {
        if !is_valid_provider(p.as_str()) {
            diags.push(LoweringDiagnostic::UnknownProvider {
                client_name: client_name.clone(),
                provider: p.clone(),
                span,
            });
        }
    }

    let is_fallback = provider.as_deref() == Some("fallback");
    let is_round_robin = provider.as_deref() == Some("round-robin");
    let is_composite = is_fallback || is_round_robin;

    // Build the Client let binding
    let let_item = synthesize_client_let(
        &client_name,
        span,
        &name_token,
        is_fallback,
        is_round_robin,
        &config_block,
    );

    // Build the $new companion for primitive clients only
    let companion = if !is_composite {
        Some(synthesize_client_new_companion(
            &client_name,
            span,
            &name_token,
            &config_block,
            provider.as_ref(),
            diags,
            env_var_refs,
        ))
    } else {
        None
    };

    Some((let_item, companion))
}

/// Build the `Client` identity let binding.
///
/// Produces: `baml.llm.Client { name, client_type, sub_clients, retry, counter }`
///
/// - Composite clients (fallback/round-robin) get sub-client `Expr::Path` references
///   from `options { strategy [A, B] }`, enabling TIR name validation and
///   `topological_sort_lets` dependency ordering.
/// - All clients get `retry` wired as `Expr::Path("RetryPolicyName")` or `null`.
/// - Round-robin clients get `counter` from `options { start N }`, others get 0.
fn synthesize_client_let(
    client_name: &str,
    span: text_size::TextRange,
    name_token: &rowan::SyntaxToken<baml_compiler_syntax::BamlLanguage>,
    is_fallback: bool,
    is_round_robin: bool,
    config_block: &ast::ConfigBlock,
) -> Item {
    use baml_base::Literal;

    let is_composite = is_fallback || is_round_robin;

    let mut exprs: la_arena::Arena<Expr> = la_arena::Arena::new();
    let mut expr_spans: la_arena::Arena<text_size::TextRange> = la_arena::Arena::new();
    let mut alloc = |expr: Expr| -> ExprId {
        let id = exprs.alloc(expr);
        expr_spans.alloc(span);
        id
    };

    // Extract retry_policy reference and composite sub-client/start from config block
    let mut retry_policy_name: Option<String> = None;
    let mut sub_client_exprs: Vec<ExprId> = vec![];
    let mut round_robin_start: i64 = 0;

    for item in config_block.items() {
        let Some(key) = item.key() else { continue };
        match key.text() {
            "retry_policy" => {
                // retry_policy MyRetry → Expr::Path(["MyRetry"])
                if let Some(word) = item.value_word() {
                    retry_policy_name = Some(word.text().to_string());
                } else if let Some(cv) = item.config_value() {
                    if let Some(text) = cv.scalar_text() {
                        let cleaned = text.trim().trim_matches('"');
                        if !cleaned.is_empty() {
                            retry_policy_name = Some(cleaned.to_string());
                        }
                    }
                }
            }
            "options" if is_composite => {
                if let Some(nested) = item.nested_block() {
                    for opt_item in nested.items() {
                        let Some(opt_key) = opt_item.key() else {
                            continue;
                        };
                        match opt_key.text() {
                            "strategy" => {
                                if let Some(elements) = opt_item.array_string_elements() {
                                    for (maybe_name, _range) in &elements {
                                        if let Some(name) = maybe_name {
                                            let expr = alloc(Expr::Path(vec![Name::new(name)]));
                                            sub_client_exprs.push(expr);
                                        }
                                    }
                                }
                            }
                            "start" => {
                                if let Some(v) = opt_item.value_int() {
                                    round_robin_start = v;
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            _ => {}
        }
    }

    // name: "MyClient"
    let name_expr = alloc(Expr::Literal(Literal::String(client_name.to_string())));

    // client_type: baml.llm.ClientType.Primitive (or Fallback / RoundRobin)
    let variant_name = if is_fallback {
        "Fallback"
    } else if is_round_robin {
        "RoundRobin"
    } else {
        "Primitive"
    };
    let client_type_expr = alloc(Expr::Path(vec![
        Name::new("baml"),
        Name::new("llm"),
        Name::new("ClientType"),
        Name::new(variant_name),
    ]));

    // sub_clients: [A, B, ...] for composites, [] for primitive
    let sub_clients_expr = alloc(Expr::Array {
        elements: sub_client_exprs,
    });

    // retry: MyRetry (path reference) or null
    let retry_expr = if let Some(rp_name) = retry_policy_name {
        alloc(Expr::Path(vec![Name::new(&rp_name)]))
    } else {
        alloc(Expr::Null)
    };

    // counter: round_robin_start for RR clients, 0 otherwise
    let counter_val = if is_round_robin { round_robin_start } else { 0 };
    let counter_expr = alloc(Expr::Literal(Literal::Int(counter_val)));

    // baml.llm.Client { name, client_type, sub_clients, retry, counter }
    let root = alloc(Expr::Object {
        type_name: TypePath::from_dotted("baml.llm.Client"),
        type_args: vec![],
        fields: vec![
            (Name::new("name"), name_expr),
            (Name::new("client_type"), client_type_expr),
            (Name::new("sub_clients"), sub_clients_expr),
            (Name::new("retry"), retry_expr),
            (Name::new("counter"), counter_expr),
        ],
        spreads: vec![],
    });

    let body = ExprBody {
        exprs,
        stmts: la_arena::Arena::new(),
        patterns: la_arena::Arena::new(),
        match_arms: la_arena::Arena::new(),
        catch_arms: la_arena::Arena::new(),
        type_annotations: la_arena::Arena::new(),
        root_expr: Some(root),
    };
    let source_map = AstSourceMap {
        expr_spans,
        ..Default::default()
    };

    Item::Let(LetDef {
        name: Name::new(client_name),
        initializer: Some((body, source_map)),
        origin: LetOrigin::Client,
        span,
        name_span: name_token.text_range(),
    })
}

/// Build the `ClientName$new` companion function for primitive clients.
///
/// Body constructs:
/// ```text
/// PrimitiveClient {
///   name: "X", provider: "openai",
///   options: PrimitiveClientOptions {
///     base_url, default_role, api_key, allowed_roles, remap_roles,
///     provider_options, headers, query_params, request_body
///   }
/// }
/// ```
///
/// Option keys are routed to match `PrimitiveClientOptions`:
/// - Known scalar fields (`base_url`, `default_role`, `api_key`, `allowed_roles`, `remap_roles`)
///   → named fields, default null
/// - `headers`, `query_params` → `Expr::Map`, default empty
/// - Provider-specific (`anthropic_version` → `AnthropicOptions`,
///   `resource_name`+`api_version` → `AzureOpenAiOptions`) → `provider_options`
/// - Unknown keys → `request_body` map entries
fn synthesize_client_new_companion(
    client_name: &str,
    span: text_size::TextRange,
    name_token: &rowan::SyntaxToken<baml_compiler_syntax::BamlLanguage>,
    config_block: &ast::ConfigBlock,
    provider: Option<&String>,
    diags: &mut Vec<LoweringDiagnostic>,
    env_var_refs: &mut Vec<crate::EnvVarRef>,
) -> FunctionDef {
    use baml_base::Literal;

    // The constructor's `lenient: bool` parameter. `env.X` option reads are
    // lowered to `baml.env.get_or_panic_lenient("X", lenient)` so the offline
    // `render_prompt` path (which calls the constructor with `lenient = true`)
    // can build the client for its metadata without a credential env var set,
    // while the network paths keep `lenient = false` and still panic if unset.
    let lenient_param_name = Name::new("lenient");

    let mut exprs: la_arena::Arena<Expr> = la_arena::Arena::new();
    let mut expr_spans: la_arena::Arena<text_size::TextRange> = la_arena::Arena::new();
    let mut alloc = |expr: Expr| -> ExprId {
        let id = exprs.alloc(expr);
        expr_spans.alloc(span);
        id
    };

    let config = provider
        .map(String::as_str)
        .filter(|p| is_valid_provider(p))
        .and_then(provider_config_for);

    // ── 1. Seed known fields as null/empty ─────────────────────
    //
    // All known fields go into one map, seeded as Null (or EmptyMap for
    // map-typed fields). No provider-specific defaults are injected --
    // those are applied at runtime in sys_llm.

    let mut values: std::collections::HashMap<String, ExprId> = std::collections::HashMap::new();
    // Provider-specific field names (from the generated config).
    let provider_field_set: std::collections::HashSet<&str> = config
        .map(|c| c.fields.iter().copied().collect())
        .unwrap_or_default();

    // Seed top-level fields as null / empty map.
    // Skip provider_options and request_body -- they're assembled separately.
    for &name in CLIENT_OPTION_FIELDS {
        if SEPARATELY_ASSEMBLED_FIELDS.contains(&name) {
            continue;
        }
        let expr = if EMPTY_MAP_FIELDS.contains(&name) {
            alloc(Expr::Map { entries: vec![] })
        } else {
            alloc(Expr::Null)
        };
        values.insert(name.to_string(), expr);
    }

    // Seed provider-specific fields as null (so they're recognized as known).
    for &name in &provider_field_set {
        values
            .entry(name.to_string())
            .or_insert_with(|| alloc(Expr::Null));
    }

    // ── 2. Override from user config ────────────────────────────
    //
    // Every option the user writes goes into the same map. Provider fields
    // skip null (so writing `field null` preserves the default rather than
    // overwriting it). Unknown fields go to request_body.

    let options_span = config_block
        .items()
        .find(|item| item.matches_key("options"))
        .map(|item| item.syntax().span_range())
        .unwrap_or(span);

    let mut has_base_url = false;
    let mut request_body_entries: Vec<(ExprId, ExprId)> = vec![];
    // Track which provider fields have been set to non-null values by the user.
    // No compile-time defaults, so this starts empty.
    let mut provider_fields_set: std::collections::HashSet<String> =
        std::collections::HashSet::new();

    if let Some(options_item) = config_block
        .items()
        .find(|item| item.matches_key("options"))
    {
        if let Some(nested) = options_item.nested_block() {
            for opt_item in nested.items() {
                let Some(opt_key) = opt_item.key() else {
                    continue;
                };
                let k = opt_key.text();
                let val = crate::lower_config_item::lower_config_value_with_env_refs(
                    &opt_item,
                    &mut alloc,
                    env_var_refs,
                    crate::lower_config_item::EnvReadMode::Lenient(&lenient_param_name),
                );
                let is_null = opt_item.value_str().as_deref() == Some("null");

                if values.contains_key(k) || provider_field_set.contains(k) {
                    // Known field. Provider fields skip null to preserve defaults.
                    if !provider_field_set.contains(k) || !is_null {
                        values.insert(k.to_string(), val);
                    }
                    if provider_field_set.contains(k) && !is_null {
                        provider_fields_set.insert(k.to_string());
                    }
                } else {
                    // Unknown field -> request_body (preserves source order).
                    let kx = alloc(Expr::Literal(Literal::String(k.to_string())));
                    request_body_entries.push((kx, val));
                }
                if k == "base_url" {
                    has_base_url = !is_null;
                }
            }
        }
    }

    // ── 3. Validate ─────────────────────────────────────────────

    if let Some(provider_str) = provider.map(String::as_str) {
        report_client_options_validation(
            provider_str,
            client_name,
            has_base_url,
            &provider_fields_set,
            options_span,
            diags,
        );
    }

    // ── 4. Assemble ─────────────────────────────────────────────
    //
    // Extract provider fields -> build typed sub-object.
    // Then take() known top-level fields -> leftover = request_body.

    // Build provider options sub-object from provider-specific fields.
    let null_expr = alloc(Expr::Null);
    let provider_options = if let Some(type_name) = config.and_then(|c| c.options_type) {
        let prov_fields: Vec<(Name, ExprId)> = config
            .unwrap()
            .fields
            .iter()
            .map(|&name| {
                let val = values.remove(name).unwrap_or_else(|| alloc(Expr::Null));
                (Name::new(name), val)
            })
            .collect();
        if !provider_fields_set.is_empty() {
            alloc(Expr::Object {
                type_name: TypePath::from_dotted(type_name),
                type_args: vec![],
                fields: prov_fields,
                spreads: vec![],
            })
        } else {
            null_expr
        }
    } else {
        null_expr
    };

    // Extract top-level fields in class-definition order.
    // Skip provider_options and request_body -- they're inserted separately below.
    let mut options_fields: Vec<(Name, ExprId)> = CLIENT_OPTION_FIELDS
        .iter()
        .filter(|name| !SEPARATELY_ASSEMBLED_FIELDS.contains(name))
        .map(|&name| {
            let val = values.remove(name).unwrap_or_else(|| alloc(Expr::Null));
            (Name::new(name), val)
        })
        .collect();

    let request_body = alloc(Expr::Map {
        entries: request_body_entries,
    });

    // Insert provider_options before "media_url_handler" to match PrimitiveClientOptions
    // class-definition order, then append request_body at the end.
    let insert_pos = options_fields
        .iter()
        .position(|(n, _)| n.as_str() == "media_url_handler")
        .unwrap_or_else(|| {
            options_fields
                .iter()
                .position(|(n, _)| n.as_str() == "headers")
                .unwrap_or(options_fields.len())
        });
    options_fields.insert(
        insert_pos,
        (Name::new("provider_options"), provider_options),
    );
    options_fields.push((Name::new("request_body"), request_body));

    let options_expr = alloc(Expr::Object {
        type_name: TypePath::from_dotted("baml.llm.PrimitiveClientOptions"),
        type_args: vec![],
        fields: options_fields,
        spreads: vec![],
    });

    // PrimitiveClient { name, provider, options }
    let name_lit = alloc(Expr::Literal(Literal::String(client_name.to_string())));
    let provider_lit = alloc(Expr::Literal(Literal::String(
        provider.map_or("unknown", |s| s.as_str()).to_string(),
    )));
    let root = alloc(Expr::Object {
        type_name: TypePath::from_dotted("baml.llm.PrimitiveClient"),
        type_args: vec![],
        fields: vec![
            (Name::new("name"), name_lit),
            (Name::new("provider"), provider_lit),
            (Name::new("options"), options_expr),
        ],
        spreads: vec![],
    });

    let body = ExprBody {
        exprs,
        stmts: la_arena::Arena::new(),
        patterns: la_arena::Arena::new(),
        match_arms: la_arena::Arena::new(),
        catch_arms: la_arena::Arena::new(),
        type_annotations: la_arena::Arena::new(),
        root_expr: Some(root),
    };
    let source_map = AstSourceMap {
        expr_spans,
        ..Default::default()
    };

    let func_name = format!("{client_name}$new");
    let lenient_param = Param {
        name: lenient_param_name,
        type_expr: Some((TypeExprKind::Bool { attrs: vec![] }).at(span)),
        default: None,
        span,
        name_span: name_token.text_range(),
    };
    FunctionDef {
        name: Name::new(&func_name),
        generic_params: vec![],
        params: vec![lenient_param],
        defaults: FunctionDefaults::empty(),
        return_type: None,
        throws: None,
        body: Some(FunctionBodyDef::Expr(body, source_map)),
        declarative_meta: None,
        metadata: crate::ast::FunctionMetadata::language_internal(
            crate::ast::FunctionOrigin::Internal,
        ),
        attributes: vec![],
        docstring: None,
        is_tagged_template_tag: false,
        span,
        name_span: name_token.text_range(),
    }
}

// ── Generated field lists & provider config ────────────────────
//
// Extracted from llm_types.baml by build.rs. To add a new field or provider,
// edit the BAML file (and its @providers annotations) -- the compiler picks it
// up automatically.

#[allow(dead_code)]
mod generated_client_fields {
    include!(concat!(env!("OUT_DIR"), "/client_fields_generated.rs"));
}
use generated_client_fields::{CLIENT_OPTION_FIELDS, PROVIDER_CONFIGS, ProviderFieldConfig};

/// Map fields that should be seeded as empty maps rather than null.
const EMPTY_MAP_FIELDS: &[&str] = &["headers", "query_params"];

/// Fields that are assembled separately (not seeded/extracted with normal top-level fields).
const SEPARATELY_ASSEMBLED_FIELDS: &[&str] = &["provider_options", "request_body"];

/// Look up the provider config for a given provider name.
fn provider_config_for(provider: &str) -> Option<&'static ProviderFieldConfig> {
    PROVIDER_CONFIGS
        .iter()
        .find(|c| c.providers.contains(&provider))
}

/// Check whether a provider name is known.
fn is_valid_provider(provider: &str) -> bool {
    PROVIDER_CONFIGS
        .iter()
        .any(|c| c.providers.contains(&provider))
}

/// Attach compile-time source context to shared client option validation errors.
fn report_client_options_validation(
    provider: &str,
    client_name: &str,
    has_base_url: bool,
    provider_fields_set: &std::collections::HashSet<String>,
    span: text_size::TextRange,
    diags: &mut Vec<LoweringDiagnostic>,
) {
    let options = ClientOptionsPresence {
        provider,
        base_url: has_base_url,
        resource_name: provider_fields_set.contains("resource_name"),
        deployment_id: provider_fields_set.contains("deployment_id"),
    };
    if let Err(error) = baml_base::validate_client_options(options) {
        diags.push(LoweringDiagnostic::MissingClientOptions {
            client_name: client_name.to_string(),
            error,
            span,
        });
    }

    // vertex-ai deliberately has no compile-time location requirement:
    // `location` (like `project_id`) can come from the GOOGLE_CLOUD_LOCATION
    // env var at request time, which the compiler cannot see. A client with
    // neither base_url, location, nor the env var fails at $build_request
    // with an actionable error (see sys_llm auth_request/vertex.rs).
}

/// Lower variant-level attributes from an `EnumVariant` node.
fn lower_variant_attributes(variant: &ast::EnumVariant) -> Vec<RawAttribute> {
    variant
        .attributes()
        .filter_map(|attr| lower_attribute(&attr))
        .collect()
}

/// Lower block-level attributes (@@) from any item node.
fn lower_attributes_from_node(node: &SyntaxNode) -> Vec<RawAttribute> {
    node.children()
        .filter_map(ast::BlockAttribute::cast)
        .filter_map(|attr| lower_block_attribute(&attr))
        .collect()
}

/// Lower a single field attribute (single @).
pub(crate) fn lower_attribute(attr: &ast::Attribute) -> Option<RawAttribute> {
    let name_token = attr.name()?;
    let attr_name = attr
        .full_name()
        .unwrap_or_else(|| name_token.text().to_string());
    let span = attr.syntax().span_range();

    let args = lower_attribute_args_from_node(attr.syntax());

    Some(RawAttribute {
        name: Name::new(&attr_name),
        args,
        span,
    })
}

/// Lower a single block attribute (@@).
fn lower_block_attribute(attr: &ast::BlockAttribute) -> Option<RawAttribute> {
    let name_token = attr.name()?;
    let attr_name = attr
        .full_name()
        .unwrap_or_else(|| name_token.text().to_string());
    let span = attr.syntax().span_range();

    let args = lower_attribute_args_from_node(attr.syntax());

    Some(RawAttribute {
        name: Name::new(&attr_name),
        args,
        span,
    })
}

/// Extract raw attribute arguments as strings from an attribute node.
fn lower_attribute_args_from_node(node: &SyntaxNode) -> Vec<RawAttributeArg> {
    // Arguments are inside ATTRIBUTE_ARGS nodes
    node.children()
        .filter(|n| n.kind() == baml_compiler_syntax::SyntaxKind::ATTRIBUTE_ARGS)
        .flat_map(|args_node| {
            args_node.children().map(|arg_node| {
                let text = arg_node.text().to_string();
                let span = arg_node.span_range();
                RawAttributeArg {
                    key: None,
                    value: text.trim().to_string(),
                    span,
                }
            })
        })
        .collect()
}

#[cfg(test)]
mod test_owner_tests {
    use std::path::Path;

    use super::test_owner_from_path;

    #[test]
    fn path_owner_uses_the_same_namespace_identifier_rules_as_hir() {
        assert_eq!(
            test_owner_from_path(Some(Path::new("ns_orders/ns_v2/tests.baml"))),
            "root.orders.v2"
        );
        assert_eq!(
            test_owner_from_path(Some(Path::new("ns_123/tests.baml"))),
            "root"
        );
    }
}
