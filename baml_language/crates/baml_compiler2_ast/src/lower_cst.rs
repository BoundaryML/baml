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
use baml_compiler_syntax::{SyntaxNode, ast};
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

// ── File-level lowering ─────────────────────────────────────────

/// Lower a CST root node to a list of `Item`s.
///
/// After this returns, the CST is no longer needed — all structural content
/// is owned by the returned `Item`s.
///
/// All diagnostics (structural lowering issues, client validation,
/// field-attr-in-wrong-position) are returned as `LoweringDiagnostic` variants.
pub fn lower_file(root: &SyntaxNode) -> (Vec<Item>, Vec<LoweringDiagnostic>) {
    let mut diags = Vec::new();
    let mut items = Vec::new();

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
    let provider_field_set: std::collections::HashSet<&str> = config
        .as_ref()
        .and_then(|c| c.provider_options.as_ref())
        .map(|(_, fields)| fields.iter().copied().collect())
        .unwrap_or_default();

    // Seed all known fields with null/empty-map defaults. This ensures that
    // user-set values for these fields go into the right place rather than
    // falling through to request_body.
    for name in [
        "model",
        "base_url",
        "allowed_role_metadata",
        "finish_reason_allow_list",
        "finish_reason_deny_list",
        "supports_streaming",
        "default_role",
        "allowed_roles",
        "remap_roles",
        "api_key",
        "headers",
        "query_params",
    ] {
        let default = match name {
            "headers" | "query_params" => alloc(Expr::Map { entries: vec![] }),
            _ => alloc(Expr::Null),
        };
        values.insert(name.to_string(), default);
    }

    // Initialize provider-specific fields to the null sentinel. Fields that
    // remain at this value after defaults + user overrides are considered
    // "unset" when deciding whether to create the provider options sub-object.
    let null_sentinel = alloc(Expr::Null);
    for &name in &provider_field_set {
        values.insert(name.to_string(), null_sentinel);
    }

    // Apply provider defaults on top (both top-level and provider-specific).
    if let Some(cfg) = &config {
        for &(name, ref default) in cfg.defaults {
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
    // or by user). Used by validate_client_options.
    let mut provider_fields_set: std::collections::HashSet<String> = config
        .as_ref()
        .iter()
        .flat_map(|c| c.defaults.iter())
        .filter(|(name, _)| provider_field_set.contains(*name))
        .map(|(name, _)| name.to_string())
        .collect();

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

    // Build provider options sub-object.
    let provider_options = if let Some((type_name, fields)) =
        config.as_ref().and_then(|c| c.provider_options.as_ref())
    {
        let mut any_set = false;
        let prov_fields: Vec<(Name, ExprId)> = fields
            .iter()
            .map(|&f| {
                let val = values.remove(f).unwrap_or(null_sentinel);
                if val != null_sentinel {
                    any_set = true;
                }
                (Name::new(f), val)
            })
            .collect();
        if any_set {
            alloc(Expr::Object {
                type_name: Some(Name::new(type_name)),
                fields: prov_fields,
                spreads: vec![],
            })
        } else {
            null_sentinel
        }
    } else {
        null_sentinel
    };

    // take() removes a field from the map, falling back to null or empty map.
    // Anything left in the map after all take() calls is an unknown field that
    // gets forwarded to request_body.
    let mut take = |name: &str| -> (Name, ExprId) {
        let val = values.remove(name).unwrap_or_else(|| match name {
            "headers" | "query_params" => alloc(Expr::Map { entries: vec![] }),
            _ => alloc(Expr::Null),
        });
        (Name::new(name), val)
    };

    // Extract top-level fields in PrimitiveClientOptions class-definition order.
    let f_model = take("model");
    let f_base_url = take("base_url");
    let f_allowed_role_metadata = take("allowed_role_metadata");
    let f_finish_reason_allow_list = take("finish_reason_allow_list");
    let f_finish_reason_deny_list = take("finish_reason_deny_list");
    let f_supports_streaming = take("supports_streaming");
    let f_default_role = take("default_role");
    let f_allowed_roles = take("allowed_roles");
    let f_remap_roles = take("remap_roles");
    let f_api_key = take("api_key");
    let f_headers = take("headers");
    let f_query_params = take("query_params");

    let request_body = alloc(Expr::Map {
        entries: request_body_entries,
    });

    let options_expr = alloc(Expr::Object {
        type_name: Some(Name::new("baml.llm.PrimitiveClientOptions")),
        fields: vec![
            f_model,
            f_base_url,
            f_allowed_role_metadata,
            f_finish_reason_allow_list,
            f_finish_reason_deny_list,
            f_supports_streaming,
            f_default_role,
            f_allowed_roles,
            f_remap_roles,
            f_api_key,
            (Name::new("provider_options"), provider_options),
            f_headers,
            f_query_params,
            (Name::new("request_body"), request_body),
        ],
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

/// A default value for a field (top-level or provider-specific).
///
/// Used in [`ProviderConfig::defaults`] to declare non-null defaults.
enum FieldDefault {
    /// Literal string (e.g. `base_url = "https://api.openai.com/v1"`).
    Str(&'static str),
    /// Literal integer (e.g. `max_tokens = 4096`).
    Int(i64),
    /// Array of strings (e.g. `allowed_roles = ["system", "user", "assistant"]`).
    StrArray(&'static [&'static str]),
    /// Map of string pairs (e.g. `remap_roles = {"assistant": "model"}`).
    StrPairMap(&'static [(&'static str, &'static str)]),
    /// Empty map literal.
    #[allow(dead_code)]
    EmptyMap,
    /// Nullable env var lookup: compiles to `baml.env.get("VAR_NAME")`.
    #[allow(dead_code)]
    Env(&'static str),
}

/// Allocate an `Expr` tree for a [`FieldDefault`] value.
fn alloc_field_default(default: &FieldDefault, alloc: &mut impl FnMut(Expr) -> ExprId) -> ExprId {
    use baml_base::Literal;
    match default {
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
        FieldDefault::EmptyMap => alloc(Expr::Map { entries: vec![] }),
        FieldDefault::Env(var) => {
            let callee = alloc(Expr::Path(vec![
                Name::new("baml"),
                Name::new("env"),
                Name::new("get"),
            ]));
            let arg = alloc(Expr::Literal(Literal::String(var.to_string())));
            alloc(Expr::Call {
                callee,
                args: vec![arg],
            })
        }
    }
}

/// Provider configuration: defaults and provider-specific options.
///
/// # How to add a new provider
///
/// 1. Add the provider name to [`VALID_PROVIDERS`].
/// 2. Add an arm to [`provider_config_for`] returning a `ProviderConfig`.
///    - `defaults`: a flat `&[(&str, FieldDefault)]` listing ALL non-null
///      defaults for this provider, both top-level (`base_url`, `default_role`,
///      etc.) and provider-specific (`max_tokens`, `region`, etc.) in one list.
///      Fields not listed default to null (or empty map for `headers`/`query_params`).
///    - `provider_options`: if the provider has a typed options class
///      (e.g. `AnthropicOptions`), set `Some(("baml.llm.TypeName", &["field1", "field2"]))`.
///      Fields listed here are extracted into the typed sub-object at assembly
///      time; everything else stays on `PrimitiveClientOptions`.
/// 3. If the provider needs compile-time validation, add checks to
///    [`validate_client_options`].
///
/// # How to add a new top-level option field
///
/// 1. Add the field to the `PrimitiveClientOptions` class in `llm_types.baml`.
/// 2. Add a `take("field_name")` call in `synthesize_client_new_companion`
///    at the assembly site, in class-definition order.
/// 3. If providers need non-null defaults, add entries to their `defaults` slice.
struct ProviderConfig {
    /// Non-null defaults for both top-level and provider-specific fields.
    ///
    /// Top-level fields like `base_url` and provider-specific fields like
    /// `max_tokens` are defined here in a single flat list. The system
    /// determines which object they belong to based on `provider_options`.
    defaults: &'static [(&'static str, FieldDefault)],

    /// Provider-specific option type and field names.
    ///
    /// If set, fields listed here are grouped into a typed `Expr::Object`
    /// (e.g. `AnthropicOptions { max_tokens: 4096 }`) and stored in the
    /// `provider_options` field of `PrimitiveClientOptions`.
    ///
    /// Format: `("baml.llm.TypeName", &["field1", "field2", ...])`.
    provider_options: Option<(&'static str, &'static [&'static str])>,
}

impl ProviderConfig {
    const EMPTY: Self = Self {
        defaults: &[],
        provider_options: None,
    };
}

const SAU: &[&str] = &["system", "user", "assistant"];
const UA: &[&str] = &["user", "assistant"];

fn provider_config_for(provider: &str) -> ProviderConfig {
    match provider {
        "anthropic" => ProviderConfig {
            defaults: &[
                ("base_url", FieldDefault::Str("https://api.anthropic.com")),
                ("default_role", FieldDefault::Str("user")),
                ("allowed_roles", FieldDefault::StrArray(SAU)),
                ("max_tokens", FieldDefault::Int(4096)),
            ],
            provider_options: Some(("baml.llm.AnthropicOptions", &["max_tokens"])),
        },
        "openai" | "openai-generic" | "openai-responses" => ProviderConfig {
            defaults: &[
                ("base_url", FieldDefault::Str("https://api.openai.com/v1")),
                ("default_role", FieldDefault::Str("system")),
                ("allowed_roles", FieldDefault::StrArray(SAU)),
            ],
            ..ProviderConfig::EMPTY
        },
        "ollama" => ProviderConfig {
            defaults: &[
                ("base_url", FieldDefault::Str("http://localhost:11434")),
                ("default_role", FieldDefault::Str("user")),
                ("allowed_roles", FieldDefault::StrArray(UA)),
            ],
            ..ProviderConfig::EMPTY
        },
        "openrouter" => ProviderConfig {
            defaults: &[
                (
                    "base_url",
                    FieldDefault::Str("https://openrouter.ai/api/v1"),
                ),
                ("default_role", FieldDefault::Str("system")),
                ("allowed_roles", FieldDefault::StrArray(SAU)),
            ],
            ..ProviderConfig::EMPTY
        },
        "azure-openai" => ProviderConfig {
            defaults: &[
                ("default_role", FieldDefault::Str("system")),
                ("allowed_roles", FieldDefault::StrArray(SAU)),
                ("max_tokens", FieldDefault::Int(4096)),
            ],
            provider_options: Some((
                "baml.llm.AzureOpenAiOptions",
                &[
                    "resource_name",
                    "deployment_id",
                    "api_version",
                    "max_tokens",
                ],
            )),
        },
        "aws-bedrock" => ProviderConfig {
            defaults: &[
                ("default_role", FieldDefault::Str("user")),
                ("allowed_roles", FieldDefault::StrArray(SAU)),
            ],
            provider_options: Some((
                "baml.llm.BedrockOptions",
                &[
                    "region",
                    "endpoint_url",
                    "access_key_id",
                    "secret_access_key",
                    "session_token",
                    "profile",
                    "stop_sequences",
                    "max_tokens",
                    "temperature",
                    "top_p",
                ],
            )),
        },
        "google-ai" => ProviderConfig {
            defaults: &[
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
            ],
            ..ProviderConfig::EMPTY
        },
        "vertex-ai" => ProviderConfig {
            defaults: &[
                ("default_role", FieldDefault::Str("user")),
                ("allowed_roles", FieldDefault::StrArray(SAU)),
                (
                    "remap_roles",
                    FieldDefault::StrPairMap(&[("assistant", "model")]),
                ),
            ],
            provider_options: Some((
                "baml.llm.VertexAiOptions",
                &[
                    "credentials",
                    "credentials_content",
                    "location",
                    "project_id",
                ],
            )),
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
