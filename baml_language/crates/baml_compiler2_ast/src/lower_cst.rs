//! Pure structural CST → AST lowering.
//!
//! One function per item kind. Type expressions are fully lowered to recursive
//! `TypeExpr`. Expression bodies are fully lowered to `ExprBody` arenas with a
//! parallel `AstSourceMap`. Missing names skip the item (`return None`), missing
//! types produce `TypeExpr::Unknown`.
//!
//! No LLM function expansion, no attribute validation, no duplicate detection —
//! all of that moves downstream.

use baml_base::Name;
use baml_compiler_syntax::{SyntaxKind, SyntaxNode, ast};
use rowan::ast::AstNode;

use crate::{
    DeclarativeMeta, LoweringDiagnostic,
    ast::{
        AstSourceMap, BuiltinKind, ConfigItemDef, EnumDef, Expr, ExprBody, ExprId, FieldDef,
        FunctionBodyDef, FunctionDef, GeneratorDef, Interpolation, Item, LetDef, LetOrigin,
        LlmBodyDef, Param, RawAttribute, RawAttributeArg, RawPrompt, SpannedTypeExpr,
        TemplateStringDef, TestDef, TypeAliasDef, VariantDef,
    },
    companions::expand_companions,
    lower_expr_body, lower_type_expr,
};

// ── Test/Testset desugaring intermediate types ───────────────────

/// Intermediate representation of a test/testset block before synthesis.
///
/// Collected during file lowering, then passed to `synthesize_init_test_function`
/// which emits a per-file `$init_test_<file_id>` function containing
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
pub fn lower_file(root: &SyntaxNode) -> (Vec<Item>, Vec<LoweringDiagnostic>) {
    lower_file_with_file_id(root, baml_base::FileId::sentinel())
}

pub fn lower_file_with_file_id(
    root: &SyntaxNode,
    file_id: baml_base::FileId,
) -> (Vec<Item>, Vec<LoweringDiagnostic>) {
    let mut diags = Vec::new();
    let mut items = Vec::new();
    let mut test_registrations: Vec<TestRegistrationItem> = Vec::new();

    for child in root.children() {
        match child.kind() {
            baml_compiler_syntax::SyntaxKind::FUNCTION_DEF => {
                if let Some(func) = lower_function(&child, &mut diags) {
                    let companions = expand_companions(&func);
                    items.push(Item::Function(func));
                    items.extend(companions.into_iter().map(Item::Function));
                }
            }
            baml_compiler_syntax::SyntaxKind::CLASS_DEF => {
                if let Some(class) = lower_class(&child, &mut diags) {
                    items.push(Item::Class(class));
                }
            }
            baml_compiler_syntax::SyntaxKind::ENUM_DEF => {
                if let Some(e) = lower_enum(&child, &mut diags) {
                    items.push(Item::Enum(e));
                }
            }
            baml_compiler_syntax::SyntaxKind::TYPE_ALIAS_DEF => {
                if let Some(ta) = lower_type_alias(&child, &mut diags) {
                    items.push(Item::TypeAlias(ta));
                }
            }
            baml_compiler_syntax::SyntaxKind::CLIENT_DEF => {
                if let Some((let_item, companion)) = synthesize_client_items(&child, &mut diags) {
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
                if let Some(g) = lower_generator(&child, &mut diags) {
                    items.push(Item::Generator(g));
                }
            }
            baml_compiler_syntax::SyntaxKind::TEMPLATE_STRING_DEF => {
                if let Some(ts) = lower_template_string(&child, &mut diags) {
                    items.push(Item::TemplateString(ts));
                }
            }
            baml_compiler_syntax::SyntaxKind::RETRY_POLICY_DEF => {
                if let Some(let_item) = synthesize_retry_policy_let(&child, &mut diags) {
                    items.push(let_item);
                }
            }
            _ => {} // skip comments, whitespace, errors
        }
    }

    // Synthesize a per-file $init_test function for all collected test/testset registrations.
    // The file_id suffix ensures uniqueness when multiple files contain tests.
    if !test_registrations.is_empty() {
        let init_fn = synthesize_init_test_function(&test_registrations, file_id, &mut diags);
        items.push(Item::Function(init_fn));
    }

    // Post-lowering validation: reject field attrs in invalid type positions.
    let field_attr_errors = crate::disambiguate::validate_field_attrs(&items);
    for (attr_name, span) in field_attr_errors {
        diags.push(LoweringDiagnostic::FieldAttributeInTypePosition { attr_name, span });
    }

    (items, diags)
}

/// Check if a just-lowered type expression contains `TypeExpr::Unknown` at the root.
/// If so, emit an `UnparseableType` diagnostic.
fn check_unknown_type(
    type_expr: &crate::ast::TypeExpr,
    context: String,
    span: text_size::TextRange,
    diags: &mut Vec<LoweringDiagnostic>,
) {
    if matches!(type_expr, crate::ast::TypeExpr::Unknown { .. }) {
        diags.push(LoweringDiagnostic::UnparseableType { context, span });
    }
}

// ── Per-item lowering ───────────────────────────────────────────

fn lower_function(node: &SyntaxNode, diags: &mut Vec<LoweringDiagnostic>) -> Option<FunctionDef> {
    let func = ast::FunctionDef::cast(node.clone())?;
    let Some(name_token) = func.name() else {
        diags.push(LoweringDiagnostic::MissingItemName {
            item_kind: "function",
            span: node.text_range(),
        });
        return None;
    };
    let name = Name::new(name_token.text());
    let name_span = name_token.text_range();

    let generic_params = extract_generic_params(node);

    let params = func
        .param_list()
        .map(|pl| lower_params(&pl, name.as_str(), diags))
        .unwrap_or_default();

    let return_type = func.return_type().map(|te| {
        let expr = lower_type_expr::lower_type_expr_node(&te);
        let te_span = te.syntax().text_range();
        check_unknown_type(&expr, format!("return type of `{name}`"), te_span, diags);
        SpannedTypeExpr {
            expr,
            span: te_span,
        }
    });

    let throws = func
        .throws_clause()
        .and_then(|tc| tc.type_expr())
        .map(|te| SpannedTypeExpr {
            expr: lower_type_expr::lower_type_expr_node(&te),
            span: te.syntax().text_range(),
        });

    let (body, declarative_meta) = if let Some(llm) = func.llm_body() {
        let llm_body_def = lower_llm_body(&llm);
        let param_names: Vec<Name> = params.iter().map(|p| p.name.clone()).collect();
        let client_name = llm_body_def.client.as_ref().map(|n| n.as_str().to_string());
        let (expr_body, source_map) = synthesize_llm_builtin_call(
            "call_llm_function",
            name.as_str(),
            &param_names,
            client_name.as_deref(),
            llm_body_def.span,
        );
        (
            Some(FunctionBodyDef::Expr(expr_body, source_map)),
            Some(DeclarativeMeta::Llm(llm_body_def)),
        )
    } else if let Some(expr) = func.expr_body() {
        // Check if the body is `$rust_function` or `$rust_io_function` before lowering
        if let Some(builtin_kind) = check_builtin_body(expr.syntax()) {
            (Some(FunctionBodyDef::Builtin(builtin_kind)), None)
        } else {
            let param_names: Vec<Name> = params.iter().map(|p| p.name.clone()).collect();
            let (expr_body, source_map) = lower_expr_body::lower(&expr, &param_names, diags);
            (Some(FunctionBodyDef::Expr(expr_body, source_map)), None)
        }
    } else {
        (None, None)
    };

    let attributes = lower_attributes_from_node(node);

    Some(FunctionDef {
        name,
        generic_params,
        params,
        return_type,
        throws,
        body,
        declarative_meta,
        attributes,
        span: node.text_range(),
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
            _ => {}
        }
    }
    None
}

pub(crate) fn lower_params(
    pl: &ast::ParameterList,
    function_name: &str,
    diags: &mut Vec<LoweringDiagnostic>,
) -> Vec<Param> {
    pl.params()
        .filter_map(|p| lower_param(&p, function_name, diags))
        .collect()
}

pub(crate) fn lower_param(
    param: &ast::Parameter,
    function_name: &str,
    diags: &mut Vec<LoweringDiagnostic>,
) -> Option<Param> {
    let Some(name_token) = param.name() else {
        diags.push(LoweringDiagnostic::MissingParamName {
            function_name: function_name.to_string(),
            span: param.syntax().text_range(),
        });
        return None;
    };
    let param_name_str = name_token.text().to_string();
    Some(Param {
        name: Name::new(&param_name_str),
        type_expr: param.ty().map(|te| {
            let expr = lower_type_expr::lower_type_expr_node(&te);
            let te_span = te.syntax().text_range();
            check_unknown_type(
                &expr,
                format!("parameter `{param_name_str}` in `{function_name}`"),
                te_span,
                diags,
            );
            SpannedTypeExpr {
                expr,
                span: te_span,
            }
        }),
        span: param.syntax().text_range(),
        name_span: name_token.text_range(),
    })
}

fn lower_llm_body(llm_body: &ast::LlmFunctionBody) -> LlmBodyDef {
    let span = llm_body.syntax().text_range();

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
/// Only `call_llm_function` passes a client; companion builtins (`render_prompt`, `build_request`)
/// use `client_name = None` and keep their existing 2-argument signature.
///
/// All synthetic spans point to `span`.
pub(crate) fn synthesize_llm_builtin_call(
    builtin_name: &str,
    function_name: &str,
    param_names: &[Name],
    client_name: Option<&str>,
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
            let ct_path = alloc(Expr::Path(vec![
                Name::new("baml"),
                Name::new("llm"),
                Name::new("ClientType"),
            ]));
            let ct_variant = alloc(Expr::FieldAccess {
                base: ct_path,
                field: Name::new("Primitive"),
            });
            let sub = alloc(Expr::Array { elements: vec![] });
            let retry = alloc(Expr::Null);
            let counter = alloc(Expr::Literal(Literal::Int(0)));
            alloc(Expr::Object {
                type_name: Some(Name::new("baml.llm.Client")),
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
        args: vec![client_arg, fn_name_expr, args_map],
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
        stmt_spans: Arena::new(),
        pattern_spans: Arena::new(),
        match_arm_spans: Arena::new(),
        type_annotation_spans: Arena::new(),
        catch_arm_spans: Arena::new(),
        field_access_member_spans: std::collections::HashMap::new(),
    };

    (body, source_map)
}

/// Synthesize a `baml.llm.parse("FunctionName", json)` call.
///
/// Unlike `synthesize_llm_builtin_call`, there is no client argument and
/// the second argument is a single `json` identifier (a path expression)
/// rather than a map of parent params.
pub(crate) fn synthesize_llm_parse_call(
    function_name: &str,
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

    // 1. Function name literal: "FunctionName"
    let fn_name_expr = alloc(Expr::Literal(Literal::String(function_name.to_string())));

    // 2. `json` parameter reference
    let json_expr = alloc(Expr::Path(vec![Name::new("json")]));

    // 3. Callee: baml.llm.parse
    let callee = alloc(Expr::Path(vec![
        Name::new("baml"),
        Name::new("llm"),
        Name::new("parse"),
    ]));

    let call = alloc(Expr::Call {
        callee,
        args: vec![fn_name_expr, json_expr],
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
        stmt_spans: Arena::new(),
        pattern_spans: Arena::new(),
        match_arm_spans: Arena::new(),
        type_annotation_spans: Arena::new(),
        catch_arm_spans: Arena::new(),
        field_access_member_spans: std::collections::HashMap::new(),
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
    let prompt_span = raw_string.syntax().text_range();

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
                    let span = child.text_range();
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
) -> Option<crate::ast::ClassDef> {
    let class = ast::ClassDef::cast(node.clone())?;
    let Some(name_token) = class.name() else {
        diags.push(LoweringDiagnostic::MissingItemName {
            item_kind: "class",
            span: node.text_range(),
        });
        return None;
    };

    let generic_params = extract_generic_params(node);
    let class_name = name_token.text().to_string();

    let fields = class
        .fields()
        .filter_map(|f| {
            let Some(fname) = f.name() else {
                diags.push(LoweringDiagnostic::MissingFieldName {
                    class_name: class_name.clone(),
                    span: f.syntax().text_range(),
                });
                return None;
            };
            let field_name_str = fname.text().to_string();
            let mut hoisted_field_attrs = Vec::new();
            let type_expr = f.ty().map(|te| {
                let mut expr = lower_type_expr::lower_type_expr_node(&te);
                let te_span = te.syntax().text_range();
                check_unknown_type(
                    &expr,
                    format!("field `{class_name}.{field_name_str}`"),
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
                    .map(|a| a.syntax().text_range())
                    .collect();

                let all_outer_attrs = std::mem::take(expr.attrs_mut());
                let (hoist, keep): (Vec<_>, Vec<_>) = all_outer_attrs.into_iter().partition(|a| {
                    crate::disambiguate::is_field_attr(a.name.as_str())
                        && direct_attr_spans.contains(&a.span)
                });
                *expr.attrs_mut() = keep;
                hoisted_field_attrs = hoist;

                SpannedTypeExpr {
                    expr,
                    span: te_span,
                }
            });
            Some(FieldDef {
                name: Name::new(&field_name_str),
                type_expr,
                attributes: hoisted_field_attrs,
                span: f.syntax().text_range(),
                name_span: fname.text_range(),
            })
        })
        .collect();

    let methods = class
        .methods()
        .filter_map(|f| lower_function(f.syntax(), diags))
        .flat_map(|func| {
            let companions = expand_companions(&func);
            std::iter::once(func).chain(companions)
        })
        .collect();

    Some(crate::ast::ClassDef {
        name: Name::new(name_token.text()),
        generic_params,
        fields,
        methods,
        attributes: lower_attributes_from_node(node),
        span: node.text_range(),
        name_span: name_token.text_range(),
    })
}

/// Extract generic type parameter names from a `GENERIC_PARAM_LIST` CST child.
///
/// Walks the direct children of `node` to find a `GENERIC_PARAM_LIST`, then
/// extracts each `GENERIC_PARAM` child's `WORD` token as a `Name`.
pub(crate) fn extract_generic_params(node: &SyntaxNode) -> Vec<Name> {
    use baml_compiler_syntax::SyntaxKind;

    let mut params = Vec::new();
    for child in node.children() {
        let child_kind: SyntaxKind = child.kind();
        if child_kind == SyntaxKind::GENERIC_PARAM_LIST {
            for param_node in child.children() {
                let param_kind: SyntaxKind = param_node.kind();
                if param_kind == SyntaxKind::GENERIC_PARAM {
                    for elem in param_node.children_with_tokens() {
                        if let Some(token) = elem.as_token() {
                            let token_kind: SyntaxKind = token.kind();
                            if token_kind == SyntaxKind::WORD {
                                params.push(Name::new(token.text()));
                            }
                        }
                    }
                }
            }
        }
    }
    params
}

fn lower_enum(node: &SyntaxNode, diags: &mut Vec<LoweringDiagnostic>) -> Option<EnumDef> {
    let enum_def = ast::EnumDef::cast(node.clone())?;
    let Some(name_token) = enum_def.name() else {
        diags.push(LoweringDiagnostic::MissingItemName {
            item_kind: "enum",
            span: node.text_range(),
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
                    span: v.syntax().text_range(),
                });
                return None;
            };
            Some(VariantDef {
                name: Name::new(vname.text()),
                attributes: lower_variant_attributes(&v),
                span: v.syntax().text_range(),
                name_span: vname.text_range(),
            })
        })
        .collect();

    Some(EnumDef {
        name: Name::new(name_token.text()),
        variants,
        attributes: lower_attributes_from_node(node),
        span: node.text_range(),
        name_span: name_token.text_range(),
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
            span: node.text_range(),
        });
        return None;
    };

    let alias_name = name_token.text().to_string();
    Some(TypeAliasDef {
        name: Name::new(&alias_name),
        type_expr: alias.ty().map(|te| {
            let expr = lower_type_expr::lower_type_expr_node(&te);
            let te_span = te.syntax().text_range();
            check_unknown_type(&expr, format!("type alias `{alias_name}`"), te_span, diags);
            SpannedTypeExpr {
                expr,
                span: te_span,
            }
        }),
        span: node.text_range(),
        name_span: name_token.text_range(),
    })
}

fn lower_test(node: &SyntaxNode, diags: &mut Vec<LoweringDiagnostic>) -> Option<TestDef> {
    let test = ast::TestDef::cast(node.clone())?;
    let Some(name_token) = test.name() else {
        diags.push(LoweringDiagnostic::MissingItemName {
            item_kind: "test",
            span: node.text_range(),
        });
        return None;
    };

    let test_name = name_token.text().to_string();
    let config_items = test
        .config_block()
        .map(|cb| lower_config_block(&cb, "test", &test_name, diags))
        .unwrap_or_default();

    Some(TestDef {
        name: Name::new(&test_name),
        config_items,
        span: node.text_range(),
        name_span: name_token.text_range(),
    })
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
/// The function is named `"$init_test_<file_id>"` to avoid collisions when
/// multiple files contain tests. For the sentinel `FileId` (PPIR), uses plain
/// `"$init_test"` since PPIR processes files individually.
fn synthesize_init_test_function(
    registrations: &[TestRegistrationItem],
    file_id: baml_base::FileId,
    diags: &mut Vec<LoweringDiagnostic>,
) -> FunctionDef {
    let fn_name = if file_id == baml_base::FileId::sentinel() {
        "$init_test".to_string()
    } else {
        format!("$init_test_{}", file_id.as_u32())
    };
    let span = text_size::TextRange::default();

    let mut ctx = lower_expr_body::InitTestContext::new();

    // Build statements: one per registration
    let mut stmt_ids: Vec<crate::ast::StmtId> = Vec::with_capacity(registrations.len());
    for reg in registrations {
        let stmt_expr = synthesize_register_call(reg, &mut ctx, diags);
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

    let (body, source_map, finish_diags) = ctx.finish(Some(block_expr));
    diags.extend(finish_diags);

    // The single parameter: `registry: testing.TestCollector`
    let registry_param = Param {
        name: Name::new("registry"),
        type_expr: Some(SpannedTypeExpr {
            expr: crate::ast::TypeExpr::Path {
                segments: vec![Name::new("testing"), Name::new("TestCollector")],
                attrs: vec![],
            },
            span,
        }),
        span,
        name_span: span,
    };

    FunctionDef {
        name: Name::new(&fn_name),
        generic_params: vec![],
        params: vec![registry_param],
        return_type: None,
        throws: None,
        body: Some(FunctionBodyDef::Expr(body, source_map)),
        declarative_meta: None,
        attributes: vec![],
        span,
        name_span: span,
    }
}

/// Synthesize a single `registry.register_test(name, lambda, runner)` or
/// `registry.register_test_set(name, collector_lambda, runner)` call expression.
fn synthesize_register_call(
    reg: &TestRegistrationItem,
    ctx: &mut lower_expr_body::InitTestContext,
    diags: &mut Vec<LoweringDiagnostic>,
) -> ExprId {
    let span = text_size::TextRange::default();
    match reg {
        TestRegistrationItem::Test {
            name_element,
            body_node,
            runner_element,
        } => {
            // Lower the test block body into a fresh ExprBody (lambda body)
            let (lambda_body, lambda_source_map, lambda_diags) =
                lower_expr_body::lower_block_node(body_node, &[Name::new("registry")]);
            diags.extend(lambda_diags);

            let lambda_def = FunctionDef {
                name: Name::new("<test body>"),
                generic_params: vec![],
                params: vec![],
                return_type: None,
                throws: None,
                body: Some(FunctionBodyDef::Expr(lambda_body, lambda_source_map)),
                declarative_meta: None,
                attributes: vec![],
                span,
                name_span: span,
            };

            // registry.register_test
            let registry_ref = ctx.alloc_expr(Expr::Path(vec![Name::new("registry")]), span);
            let method_call_target = ctx.alloc_expr(
                Expr::FieldAccess {
                    base: registry_ref,
                    field: Name::new("register_test"),
                },
                span,
            );

            // Args: (name_expr, lambda, runner_or_null)
            let name_arg = lower_expr_body::lower_runner_element(ctx, name_element);
            // Use a unique synthetic span for the lambda so the HIR scope builder
            // can distinguish this lambda's scope from other synthesized lambdas.
            let lambda_span = ctx.next_lambda_span();
            let lambda_arg = ctx.alloc_expr(Expr::Lambda(Box::new(lambda_def)), lambda_span);
            let runner_arg = lower_runner_element(runner_element.as_ref(), ctx, span);

            ctx.alloc_expr(
                Expr::Call {
                    callee: method_call_target,
                    args: vec![name_arg, lambda_arg, runner_arg],
                },
                span,
            )
        }
        TestRegistrationItem::TestSet {
            name_element,
            body_node,
            runner_element,
        } => {
            // Lower the testset body into a collector lambda using the full testset lowering.
            let (collector_exprs, collector_source_map, collector_diags) =
                lower_expr_body::lower_testset_block_node(
                    body_node,
                    &Name::new("testset"),
                    &[Name::new("registry")],
                );
            diags.extend(collector_diags);

            // Collector lambda parameter: `testset`
            let testset_param = Param {
                name: Name::new("testset"),
                type_expr: Some(SpannedTypeExpr {
                    expr: crate::ast::TypeExpr::Path {
                        segments: vec![Name::new("testing"), Name::new("TestCollector")],
                        attrs: vec![],
                    },
                    span,
                }),
                span,
                name_span: span,
            };

            let collector_def = FunctionDef {
                name: Name::new("<testset collector>"),
                generic_params: vec![],
                params: vec![testset_param],
                return_type: None,
                throws: None,
                body: Some(FunctionBodyDef::Expr(collector_exprs, collector_source_map)),
                declarative_meta: None,
                attributes: vec![],
                span,
                name_span: span,
            };

            // registry.register_test_set
            let registry_ref = ctx.alloc_expr(Expr::Path(vec![Name::new("registry")]), span);
            let method_call_target = ctx.alloc_expr(
                Expr::FieldAccess {
                    base: registry_ref,
                    field: Name::new("register_test_set"),
                },
                span,
            );

            // Args: (name_expr, collector_lambda, runner_or_null)
            let name_arg = lower_expr_body::lower_runner_element(ctx, name_element);
            // Use the testset body's real CST range so HIR scope lookup works
            // correctly for name resolution inside the collector lambda body.
            let collector_lambda_span = body_node.text_range();
            let collector_arg =
                ctx.alloc_expr(Expr::Lambda(Box::new(collector_def)), collector_lambda_span);
            let runner_arg = lower_runner_element(runner_element.as_ref(), ctx, span);

            ctx.alloc_expr(
                Expr::Call {
                    callee: method_call_target,
                    args: vec![name_arg, collector_arg, runner_arg],
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

fn lower_generator(node: &SyntaxNode, diags: &mut Vec<LoweringDiagnostic>) -> Option<GeneratorDef> {
    let generator = ast::GeneratorDef::cast(node.clone())?;
    let Some(name_token) = generator.name() else {
        diags.push(LoweringDiagnostic::MissingItemName {
            item_kind: "generator",
            span: node.text_range(),
        });
        return None;
    };

    let gen_name = name_token.text().to_string();
    let config_items = generator
        .config_block()
        .map(|cb| lower_config_block(&cb, "generator", &gen_name, diags))
        .unwrap_or_default();

    Some(GeneratorDef {
        name: Name::new(&gen_name),
        config_items,
        span: node.text_range(),
        name_span: name_token.text_range(),
    })
}

fn lower_template_string(
    node: &SyntaxNode,
    diags: &mut Vec<LoweringDiagnostic>,
) -> Option<TemplateStringDef> {
    let ts = ast::TemplateStringDef::cast(node.clone())?;
    let Some(name_token) = ts.name() else {
        diags.push(LoweringDiagnostic::MissingItemName {
            item_kind: "template_string",
            span: node.text_range(),
        });
        return None;
    };

    let ts_name = name_token.text().to_string();
    let params = ts
        .param_list()
        .map(|pl| lower_params(&pl, &ts_name, diags))
        .unwrap_or_default();

    let body = ts.raw_string().map(|rs| lower_raw_prompt(&rs));

    Some(TemplateStringDef {
        name: Name::new(name_token.text()),
        params,
        body,
        span: node.text_range(),
        name_span: name_token.text_range(),
    })
}

/// Synthesize an `Item::Let` for a `retry_policy` declaration.
///
/// Produces: `RetryPolicy { max_retries: N, initial_delay_ms: N, multiplier: F, max_delay_ms: N }`
///
/// Each config field is lowered generically via `lower_config_item::lower_config_value`,
/// then wrapped in a typed `Expr::Object`.
fn synthesize_retry_policy_let(
    node: &SyntaxNode,
    diags: &mut Vec<LoweringDiagnostic>,
) -> Option<Item> {
    let rp = ast::RetryPolicyDef::cast(node.clone())?;
    let Some(name_token) = rp.name() else {
        diags.push(LoweringDiagnostic::MissingItemName {
            item_kind: "retry_policy",
            span: node.text_range(),
        });
        return None;
    };
    let span = node.text_range();
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
                    span: item.syntax().text_range(),
                });
                return None;
            };
            let value = crate::lower_config_item::lower_config_value(&item, &mut alloc);
            Some((Name::new(key.text()), value))
        })
        .collect();

    let root = alloc(Expr::Object {
        type_name: Some(Name::new("RetryPolicy")),
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
    let source_map = AstSourceMap {
        expr_spans,
        stmt_spans: la_arena::Arena::new(),
        pattern_spans: la_arena::Arena::new(),
        match_arm_spans: la_arena::Arena::new(),
        type_annotation_spans: la_arena::Arena::new(),
        catch_arm_spans: la_arena::Arena::new(),
        field_access_member_spans: std::collections::HashMap::new(),
    };

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
) -> Option<(Item, Option<FunctionDef>)> {
    let client = ast::ClientDef::cast(node.clone())?;
    let Some(name_token) = client.name() else {
        diags.push(LoweringDiagnostic::MissingItemName {
            item_kind: "client",
            span: node.text_range(),
        });
        return None;
    };
    let client_name = name_token.text().to_string();
    let span = node.text_range();
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
        if !VALID_PROVIDERS.contains(&p.as_str()) {
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
        ))
    } else {
        None
    };

    Some((let_item, companion))
}

/// Build the `Client` identity let binding.
///
/// Produces: `Client { name, client_type, sub_clients, retry, counter }`
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
    let client_type_path = alloc(Expr::Path(vec![
        Name::new("baml"),
        Name::new("llm"),
        Name::new("ClientType"),
    ]));
    let variant_name = if is_fallback {
        "Fallback"
    } else if is_round_robin {
        "RoundRobin"
    } else {
        "Primitive"
    };
    let client_type_expr = alloc(Expr::FieldAccess {
        base: client_type_path,
        field: Name::new(variant_name),
    });

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

    // Client { name, client_type, sub_clients, retry, counter }
    let root = alloc(Expr::Object {
        type_name: Some(Name::new("Client")),
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
        stmt_spans: la_arena::Arena::new(),
        pattern_spans: la_arena::Arena::new(),
        match_arm_spans: la_arena::Arena::new(),
        type_annotation_spans: la_arena::Arena::new(),
        catch_arm_spans: la_arena::Arena::new(),
        field_access_member_spans: std::collections::HashMap::new(),
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
) -> FunctionDef {
    use baml_base::Literal;

    let mut exprs: la_arena::Arena<Expr> = la_arena::Arena::new();
    let mut expr_spans: la_arena::Arena<text_size::TextRange> = la_arena::Arena::new();
    let mut alloc = |expr: Expr| -> ExprId {
        let id = exprs.alloc(expr);
        expr_spans.alloc(span);
        id
    };

    let config = provider
        .map(String::as_str)
        .filter(|p| VALID_PROVIDERS.contains(p))
        .map(provider_config_for);

    // ── 1. Seed the unified value map from provider defaults ────
    //
    // All known fields (top-level AND provider-specific) go into one map.
    // The split into PrimitiveClientOptions vs provider sub-object only
    // happens at assembly time.

    let mut values: std::collections::HashMap<String, ExprId> = std::collections::HashMap::new();
    let top_level_names: std::collections::HashSet<&str> =
        TOP_LEVEL_FIELDS.iter().map(|(n, _)| *n).collect();

    // Provider-specific field names: anything in config.fields not in TOP_LEVEL_FIELDS.
    let provider_field_set: std::collections::HashSet<&str> = config
        .as_ref()
        .map(|c| {
            c.fields
                .iter()
                .map(|(n, _)| *n)
                .filter(|n| !top_level_names.contains(n))
                .collect()
        })
        .unwrap_or_default();

    // Seed top-level fields with their base defaults.
    for &(name, ref default) in TOP_LEVEL_FIELDS {
        values.insert(name.to_string(), alloc_field_default(default, &mut alloc));
    }

    // Apply provider field defaults on top (both top-level overrides and provider-specific).
    if let Some(cfg) = &config {
        for &(name, ref default) in cfg.fields {
            values.insert(name.to_string(), alloc_field_default(default, &mut alloc));
        }
    }

    // ── 2. Override from user config ────────────────────────────
    //
    // Every option the user writes goes into the same map. Provider fields
    // skip null (so writing `field null` preserves the default rather than
    // overwriting it). Unknown fields go to request_body.

    let options_span = config_block
        .items()
        .find(|item| item.matches_key("options"))
        .map(|item| item.syntax().text_range())
        .unwrap_or(span);

    let mut has_base_url = false;
    let mut request_body_entries: Vec<(ExprId, ExprId)> = vec![];
    // Track which provider fields have been set to non-null values (by defaults
    // or by user). Used by validate_client_options and assembly.
    let mut provider_fields_set: std::collections::HashSet<String> = config
        .as_ref()
        .map(|c| {
            c.fields
                .iter()
                .filter(|(name, default)| provider_field_set.contains(*name) && !default.is_null())
                .map(|(name, _)| name.to_string())
                .collect()
        })
        .unwrap_or_default();

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
                let val = crate::lower_config_item::lower_config_value(&opt_item, &mut alloc);
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
        validate_client_options(
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
    let provider_options =
        if let Some(type_name) = config.as_ref().and_then(|c| c.provider_options_type) {
            let prov_fields: Vec<(Name, ExprId)> = config
                .as_ref()
                .unwrap()
                .fields
                .iter()
                .filter(|(name, _)| !top_level_names.contains(name))
                .map(|&(name, _)| {
                    let val = values.remove(name).unwrap_or_else(|| alloc(Expr::Null));
                    (Name::new(name), val)
                })
                .collect();
            if !provider_fields_set.is_empty() {
                alloc(Expr::Object {
                    type_name: Some(Name::new(type_name)),
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
    let mut options_fields: Vec<(Name, ExprId)> = TOP_LEVEL_FIELDS
        .iter()
        .map(|&(name, ref default)| {
            let val = values
                .remove(name)
                .unwrap_or_else(|| alloc_field_default(default, &mut alloc));
            (Name::new(name), val)
        })
        .collect();

    let request_body = alloc(Expr::Map {
        entries: request_body_entries,
    });

    // Insert provider_options before "headers" to match PrimitiveClientOptions
    // class-definition order, then append request_body at the end.
    let headers_pos = options_fields
        .iter()
        .position(|(n, _)| n.as_str() == "headers")
        .unwrap_or(options_fields.len());
    options_fields.insert(
        headers_pos,
        (Name::new("provider_options"), provider_options),
    );
    options_fields.push((Name::new("request_body"), request_body));

    let options_expr = alloc(Expr::Object {
        type_name: Some(Name::new("baml.llm.PrimitiveClientOptions")),
        fields: options_fields,
        spreads: vec![],
    });

    // PrimitiveClient { name, provider, options }
    let name_lit = alloc(Expr::Literal(Literal::String(client_name.to_string())));
    let provider_lit = alloc(Expr::Literal(Literal::String(
        provider.map_or("unknown", |s| s.as_str()).to_string(),
    )));
    let root = alloc(Expr::Object {
        type_name: Some(Name::new("baml.llm.PrimitiveClient")),
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
        stmt_spans: la_arena::Arena::new(),
        pattern_spans: la_arena::Arena::new(),
        match_arm_spans: la_arena::Arena::new(),
        type_annotation_spans: la_arena::Arena::new(),
        catch_arm_spans: la_arena::Arena::new(),
        field_access_member_spans: std::collections::HashMap::new(),
    };

    let func_name = format!("{client_name}$new");
    FunctionDef {
        name: Name::new(&func_name),
        generic_params: vec![],
        params: vec![],
        return_type: None,
        throws: None,
        body: Some(FunctionBodyDef::Expr(body, source_map)),
        declarative_meta: None,
        attributes: vec![],
        span,
        name_span: name_token.text_range(),
    }
}

// ── Helpers ─────────────────────────────────────────────────────

/// Default value for a field -- covers both "unset" sentinels (`Null`, `EmptyMap`)
/// used in [`TOP_LEVEL_FIELDS`] and concrete provider defaults used in per-provider
/// field consts like [`ANTHROPIC_FIELDS`].
enum FieldDefault {
    /// Null / unset.
    Null,
    /// Empty map literal (used as the unset default for `headers`, `query_params`).
    EmptyMap,
    /// Literal string.
    Str(&'static str),
    /// Literal integer.
    Int(i64),
    /// Array of strings.
    StrArray(&'static [&'static str]),
    /// Map of string pairs.
    StrPairMap(&'static [(&'static str, &'static str)]),
}

impl FieldDefault {
    fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }
}

/// Allocate an `Expr` tree for a [`FieldDefault`] value.
fn alloc_field_default(default: &FieldDefault, alloc: &mut impl FnMut(Expr) -> ExprId) -> ExprId {
    use baml_base::Literal;
    match default {
        FieldDefault::Null => alloc(Expr::Null),
        FieldDefault::EmptyMap => alloc(Expr::Map { entries: vec![] }),
        FieldDefault::Str(s) => alloc(Expr::Literal(Literal::String(s.to_string()))),
        FieldDefault::Int(v) => alloc(Expr::Literal(Literal::Int(*v))),
        FieldDefault::StrArray(items) => {
            let elements: Vec<ExprId> = items
                .iter()
                .map(|s| alloc(Expr::Literal(Literal::String(s.to_string()))))
                .collect();
            alloc(Expr::Array { elements })
        }
        FieldDefault::StrPairMap(pairs) => {
            let entries: Vec<(ExprId, ExprId)> = pairs
                .iter()
                .map(|(k, v)| {
                    let ke = alloc(Expr::Literal(Literal::String(k.to_string())));
                    let ve = alloc(Expr::Literal(Literal::String(v.to_string())));
                    (ke, ve)
                })
                .collect();
            alloc(Expr::Map { entries })
        }
    }
}

/// Top-level fields on `PrimitiveClientOptions`, in class-definition order.
///
/// To add a new top-level option, add it here and in the `PrimitiveClientOptions`
/// class in `llm_types.baml`. The seed, take, and assembly stages all derive
/// from this list automatically.
const TOP_LEVEL_FIELDS: &[(&str, FieldDefault)] = &[
    ("model", FieldDefault::Null),
    ("base_url", FieldDefault::Null),
    ("allowed_role_metadata", FieldDefault::Null),
    ("finish_reason_allow_list", FieldDefault::Null),
    ("finish_reason_deny_list", FieldDefault::Null),
    ("supports_streaming", FieldDefault::Null),
    ("default_role", FieldDefault::Null),
    ("allowed_roles", FieldDefault::Null),
    ("remap_roles", FieldDefault::Null),
    ("api_key", FieldDefault::Null),
    ("headers", FieldDefault::EmptyMap),
    ("query_params", FieldDefault::EmptyMap),
];

// ── Per-provider field definitions ─────────────────────────────
//
// Each const lists ALL fields the provider cares about -- both top-level
// overrides (e.g. base_url, default_role) and provider-specific fields
// (e.g. max_tokens, region). Fields not in TOP_LEVEL_FIELDS are
// automatically treated as provider-specific and grouped into the typed
// sub-object at assembly time.
//
// To add a new provider:
//   1. Add its name to VALID_PROVIDERS.
//   2. Add a PROVIDER_FIELDS const here.
//   3. Add an arm to provider_config_for().
//   4. If it needs compile-time validation, add checks to validate_client_options().

const SAU: &[&str] = &["system", "user", "assistant"];
const UA: &[&str] = &["user", "assistant"];

const ANTHROPIC_FIELDS: &[(&str, FieldDefault)] = &[
    ("base_url", FieldDefault::Str("https://api.anthropic.com")),
    ("default_role", FieldDefault::Str("user")),
    ("allowed_roles", FieldDefault::StrArray(SAU)),
    ("max_tokens", FieldDefault::Int(4096)),
];

const OPENAI_FIELDS: &[(&str, FieldDefault)] = &[
    ("base_url", FieldDefault::Str("https://api.openai.com/v1")),
    ("default_role", FieldDefault::Str("system")),
    ("allowed_roles", FieldDefault::StrArray(SAU)),
];

const OLLAMA_FIELDS: &[(&str, FieldDefault)] = &[
    ("base_url", FieldDefault::Str("http://localhost:11434")),
    ("default_role", FieldDefault::Str("user")),
    ("allowed_roles", FieldDefault::StrArray(UA)),
];

const OPENROUTER_FIELDS: &[(&str, FieldDefault)] = &[
    (
        "base_url",
        FieldDefault::Str("https://openrouter.ai/api/v1"),
    ),
    ("default_role", FieldDefault::Str("system")),
    ("allowed_roles", FieldDefault::StrArray(SAU)),
];

const AZURE_OPENAI_FIELDS: &[(&str, FieldDefault)] = &[
    ("default_role", FieldDefault::Str("system")),
    ("allowed_roles", FieldDefault::StrArray(SAU)),
    ("resource_name", FieldDefault::Null),
    ("deployment_id", FieldDefault::Null),
    ("api_version", FieldDefault::Null),
    ("max_tokens", FieldDefault::Int(4096)),
];

const BEDROCK_FIELDS: &[(&str, FieldDefault)] = &[
    ("default_role", FieldDefault::Str("user")),
    ("allowed_roles", FieldDefault::StrArray(SAU)),
    ("region", FieldDefault::Null),
    ("endpoint_url", FieldDefault::Null),
    ("access_key_id", FieldDefault::Null),
    ("secret_access_key", FieldDefault::Null),
    ("session_token", FieldDefault::Null),
    ("profile", FieldDefault::Null),
    ("stop_sequences", FieldDefault::Null),
    ("max_tokens", FieldDefault::Null),
    ("temperature", FieldDefault::Null),
    ("top_p", FieldDefault::Null),
];

const GOOGLE_AI_FIELDS: &[(&str, FieldDefault)] = &[
    (
        "base_url",
        FieldDefault::Str("https://generativelanguage.googleapis.com/v1beta"),
    ),
    ("default_role", FieldDefault::Str("user")),
    ("allowed_roles", FieldDefault::StrArray(SAU)),
    (
        "remap_roles",
        FieldDefault::StrPairMap(&[("assistant", "model")]),
    ),
];

const VERTEX_AI_FIELDS: &[(&str, FieldDefault)] = &[
    ("default_role", FieldDefault::Str("user")),
    ("allowed_roles", FieldDefault::StrArray(SAU)),
    (
        "remap_roles",
        FieldDefault::StrPairMap(&[("assistant", "model")]),
    ),
    ("credentials", FieldDefault::Null),
    ("credentials_content", FieldDefault::Null),
    ("location", FieldDefault::Null),
    ("project_id", FieldDefault::Null),
];

/// Provider configuration: field definitions and optional typed sub-object.
///
/// Fields not present in [`TOP_LEVEL_FIELDS`] are automatically treated as
/// provider-specific and assembled into a typed `Expr::Object`.
struct ProviderConfig {
    /// All fields this provider defines (top-level overrides + provider-specific).
    fields: &'static [(&'static str, FieldDefault)],
    /// Type name for the provider-specific options sub-object, if any.
    provider_options_type: Option<&'static str>,
}

fn provider_config_for(provider: &str) -> ProviderConfig {
    match provider {
        "anthropic" => ProviderConfig {
            fields: ANTHROPIC_FIELDS,
            provider_options_type: Some("baml.llm.AnthropicOptions"),
        },
        "openai" | "openai-generic" | "openai-responses" => ProviderConfig {
            fields: OPENAI_FIELDS,
            provider_options_type: None,
        },
        "ollama" => ProviderConfig {
            fields: OLLAMA_FIELDS,
            provider_options_type: None,
        },
        "openrouter" => ProviderConfig {
            fields: OPENROUTER_FIELDS,
            provider_options_type: None,
        },
        "azure-openai" => ProviderConfig {
            fields: AZURE_OPENAI_FIELDS,
            provider_options_type: Some("baml.llm.AzureOpenAiOptions"),
        },
        "aws-bedrock" => ProviderConfig {
            fields: BEDROCK_FIELDS,
            provider_options_type: Some("baml.llm.BedrockOptions"),
        },
        "google-ai" => ProviderConfig {
            fields: GOOGLE_AI_FIELDS,
            provider_options_type: None,
        },
        "vertex-ai" => ProviderConfig {
            fields: VERTEX_AI_FIELDS,
            provider_options_type: Some("baml.llm.VertexAiOptions"),
        },
        _ => unreachable!("unknown provider {provider:?}: add it to provider_config_for"),
    }
}

const VALID_PROVIDERS: &[&str] = &[
    "anthropic",
    "azure-openai",
    "aws-bedrock",
    "openai",
    "openai-generic",
    "openai-responses",
    "ollama",
    "openrouter",
    "google-ai",
    "vertex-ai",
    "fallback",
    "round-robin",
];

/// Validate provider-specific option constraints at compile time.
fn validate_client_options(
    provider: &str,
    client_name: &str,
    has_base_url: bool,
    provider_fields_set: &std::collections::HashSet<String>,
    span: text_size::TextRange,
    diags: &mut Vec<LoweringDiagnostic>,
) {
    let has_prov = |name: &str| -> bool { provider_fields_set.contains(name) };

    if provider == "azure-openai"
        && !has_base_url
        && !(has_prov("resource_name") && has_prov("deployment_id"))
    {
        let missing = match (has_prov("resource_name"), has_prov("deployment_id")) {
            (false, false) => "resource_name and deployment_id",
            (false, true) => "resource_name",
            (true, false) => "deployment_id",
            (true, true) => unreachable!(),
        };
        diags.push(LoweringDiagnostic::MissingClientOptions {
            client_name: client_name.to_string(),
            message: format!(
                "azure-openai requires either base_url or both resource_name and deployment_id (missing: {missing})"
            ),
            span,
        });
    }

    if provider == "vertex-ai" && !has_base_url && !has_prov("location") {
        diags.push(LoweringDiagnostic::MissingClientOptions {
            client_name: client_name.to_string(),
            message: "vertex-ai requires either base_url or location (e.g. us-central1) in options"
                .to_string(),
            span,
        });
    }
}

fn lower_config_block(
    cb: &ast::ConfigBlock,
    block_kind: &'static str,
    block_name: &str,
    diags: &mut Vec<LoweringDiagnostic>,
) -> Vec<ConfigItemDef> {
    cb.items()
        .filter_map(|item| {
            let Some(key_token) = item.key() else {
                diags.push(LoweringDiagnostic::MissingConfigKey {
                    block_kind,
                    block_name: block_name.to_string(),
                    span: item.syntax().text_range(),
                });
                return None;
            };
            let value = item.value_str().unwrap_or_default();
            Some(ConfigItemDef {
                key: Name::new(key_token.text()),
                value,
                span: item.syntax().text_range(),
            })
        })
        .collect()
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
    let span = attr.syntax().text_range();

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
    let span = attr.syntax().text_range();

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
                let span = arg_node.text_range();
                RawAttributeArg {
                    key: None,
                    value: text.trim().to_string(),
                    span,
                }
            })
        })
        .collect()
}
