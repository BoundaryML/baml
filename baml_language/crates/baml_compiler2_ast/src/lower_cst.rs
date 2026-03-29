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
use baml_compiler_diagnostics::HirDiagnostic;
use baml_compiler_syntax::{SyntaxNode, ast};
use rowan::ast::AstNode;

use crate::{
    DeclarativeMeta,
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
pub fn lower_file(root: &SyntaxNode) -> (Vec<Item>, Vec<HirDiagnostic>) {
    let mut items = Vec::new();
    let diagnostics = Vec::new();

    for child in root.children() {
        match child.kind() {
            baml_compiler_syntax::SyntaxKind::FUNCTION_DEF => {
                if let Some(func) = lower_function(&child) {
                    let companions = expand_companions(&func);
                    items.push(Item::Function(func));
                    items.extend(companions.into_iter().map(Item::Function));
                }
            }
            baml_compiler_syntax::SyntaxKind::CLASS_DEF => {
                if let Some(class) = lower_class(&child) {
                    items.push(Item::Class(class));
                }
            }
            baml_compiler_syntax::SyntaxKind::ENUM_DEF => {
                if let Some(e) = lower_enum(&child) {
                    items.push(Item::Enum(e));
                }
            }
            baml_compiler_syntax::SyntaxKind::TYPE_ALIAS_DEF => {
                if let Some(ta) = lower_type_alias(&child) {
                    items.push(Item::TypeAlias(ta));
                }
            }
            baml_compiler_syntax::SyntaxKind::CLIENT_DEF => {
                if let Some((let_item, companion)) = synthesize_client_items(&child) {
                    items.push(let_item);
                    if let Some(func) = companion {
                        items.push(Item::Function(func));
                    }
                }
            }
            baml_compiler_syntax::SyntaxKind::TEST_DEF => {
                if let Some(t) = lower_test(&child) {
                    items.push(Item::Test(t));
                }
            }
            baml_compiler_syntax::SyntaxKind::GENERATOR_DEF => {
                if let Some(g) = lower_generator(&child) {
                    items.push(Item::Generator(g));
                }
            }
            baml_compiler_syntax::SyntaxKind::TEMPLATE_STRING_DEF => {
                if let Some(ts) = lower_template_string(&child) {
                    items.push(Item::TemplateString(ts));
                }
            }
            baml_compiler_syntax::SyntaxKind::RETRY_POLICY_DEF => {
                if let Some(let_item) = synthesize_retry_policy_let(&child) {
                    items.push(let_item);
                }
            }
            _ => {} // skip comments, whitespace, errors
        }
    }

    (items, diagnostics)
}

// ── Per-item lowering ───────────────────────────────────────────

fn lower_function(node: &SyntaxNode) -> Option<FunctionDef> {
    let func = ast::FunctionDef::cast(node.clone())?;
    let name_token = func.name()?;
    let name = Name::new(name_token.text());
    let name_span = name_token.text_range();

    let generic_params = extract_generic_params(node);

    let params = func
        .param_list()
        .map(|pl| lower_params(&pl))
        .unwrap_or_default();

    let return_type = func.return_type().map(|te| SpannedTypeExpr {
        expr: lower_type_expr::lower_type_expr_node(&te),
        span: te.syntax().text_range(),
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
            let (expr_body, source_map) = lower_expr_body::lower(&expr, &param_names);
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

pub(crate) fn lower_params(pl: &ast::ParameterList) -> Vec<Param> {
    pl.params().filter_map(|p| lower_param(&p)).collect()
}

pub(crate) fn lower_param(param: &ast::Parameter) -> Option<Param> {
    let name_token = param.name()?;
    Some(Param {
        name: Name::new(name_token.text()),
        type_expr: param.ty().map(|te| SpannedTypeExpr {
            expr: lower_type_expr::lower_type_expr_node(&te),
            span: te.syntax().text_range(),
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

fn lower_class(node: &SyntaxNode) -> Option<crate::ast::ClassDef> {
    let class = ast::ClassDef::cast(node.clone())?;
    let name_token = class.name()?;

    let generic_params = extract_generic_params(node);

    let fields = class
        .fields()
        .filter_map(|f| {
            let fname = f.name()?;
            Some(FieldDef {
                name: Name::new(fname.text()),
                type_expr: f.ty().map(|te| SpannedTypeExpr {
                    expr: lower_type_expr::lower_type_expr_node(&te),
                    span: te.syntax().text_range(),
                }),
                attributes: lower_field_attributes(&f),
                span: f.syntax().text_range(),
                name_span: fname.text_range(),
            })
        })
        .collect();

    let methods = class
        .methods()
        .filter_map(|f| lower_function(f.syntax()))
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

fn lower_enum(node: &SyntaxNode) -> Option<EnumDef> {
    let enum_def = ast::EnumDef::cast(node.clone())?;
    let name_token = enum_def.name()?;

    let variants = enum_def
        .variants()
        .filter_map(|v| {
            let vname = v.name()?;
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

fn lower_type_alias(node: &SyntaxNode) -> Option<TypeAliasDef> {
    let alias = ast::TypeAliasDef::cast(node.clone())?;
    let name_token = alias.name()?;

    Some(TypeAliasDef {
        name: Name::new(name_token.text()),
        type_expr: alias.ty().map(|te| SpannedTypeExpr {
            expr: lower_type_expr::lower_type_expr_node(&te),
            span: te.syntax().text_range(),
        }),
        span: node.text_range(),
        name_span: name_token.text_range(),
    })
}

fn lower_test(node: &SyntaxNode) -> Option<TestDef> {
    let test = ast::TestDef::cast(node.clone())?;
    let name_token = test.name()?;

    let config_items = test
        .config_block()
        .map(|cb| lower_config_block(&cb))
        .unwrap_or_default();

    Some(TestDef {
        name: Name::new(name_token.text()),
        config_items,
        span: node.text_range(),
        name_span: name_token.text_range(),
    })
}

fn lower_generator(node: &SyntaxNode) -> Option<GeneratorDef> {
    let generator = ast::GeneratorDef::cast(node.clone())?;
    let name_token = generator.name()?;

    let config_items = generator
        .config_block()
        .map(|cb| lower_config_block(&cb))
        .unwrap_or_default();

    Some(GeneratorDef {
        name: Name::new(name_token.text()),
        config_items,
        span: node.text_range(),
        name_span: name_token.text_range(),
    })
}

fn lower_template_string(node: &SyntaxNode) -> Option<TemplateStringDef> {
    let ts = ast::TemplateStringDef::cast(node.clone())?;
    let name_token = ts.name()?;

    let params = ts
        .param_list()
        .map(|pl| lower_params(&pl))
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
fn synthesize_retry_policy_let(node: &SyntaxNode) -> Option<Item> {
    let rp = ast::RetryPolicyDef::cast(node.clone())?;
    let name_token = rp.name()?;
    let span = node.text_range();
    let config_block = rp.config_block()?;

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
            let key = item.key()?;
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
fn synthesize_client_items(node: &SyntaxNode) -> Option<(Item, Option<FunctionDef>)> {
    let client = ast::ClientDef::cast(node.clone())?;
    let name_token = client.name()?;
    let client_name = name_token.text().to_string();
    let span = node.text_range();
    let config_block = client.config_block()?;

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
) -> FunctionDef {
    use baml_base::Literal;

    let mut exprs: la_arena::Arena<Expr> = la_arena::Arena::new();
    let mut expr_spans: la_arena::Arena<text_size::TextRange> = la_arena::Arena::new();
    let mut alloc = |expr: Expr| -> ExprId {
        let id = exprs.alloc(expr);
        expr_spans.alloc(span);
        id
    };

    // Named PrimitiveClientOptions fields — default null
    let mut model = alloc(Expr::Null);
    let mut base_url = alloc(Expr::Null);
    let mut default_role = alloc(Expr::Null);
    let mut api_key = alloc(Expr::Null);
    let mut allowed_roles = alloc(Expr::Null);
    let mut remap_roles = alloc(Expr::Null);

    // Map fields — default empty
    let mut headers_expr = alloc(Expr::Map { entries: vec![] });
    let mut query_params_expr = alloc(Expr::Map { entries: vec![] });

    // Provider-specific accumulators
    let mut anthropic_version: Option<ExprId> = None;
    let mut resource_name: Option<ExprId> = None;
    let mut api_version: Option<ExprId> = None;

    // Unknown keys → request_body
    let mut request_body_entries: Vec<(ExprId, ExprId)> = vec![];

    // Walk the options nested block
    if let Some(options_item) = config_block
        .items()
        .find(|item| item.matches_key("options"))
    {
        if let Some(nested) = options_item.nested_block() {
            for opt_item in nested.items() {
                let Some(opt_key) = opt_item.key() else {
                    continue;
                };
                match opt_key.text() {
                    // Named scalar fields
                    "model" => {
                        model = crate::lower_config_item::lower_config_value(&opt_item, &mut alloc);
                    }
                    "base_url" => {
                        base_url =
                            crate::lower_config_item::lower_config_value(&opt_item, &mut alloc);
                    }
                    "default_role" => {
                        default_role =
                            crate::lower_config_item::lower_config_value(&opt_item, &mut alloc);
                    }
                    "api_key" => {
                        api_key =
                            crate::lower_config_item::lower_config_value(&opt_item, &mut alloc);
                    }
                    "allowed_roles" => {
                        allowed_roles =
                            crate::lower_config_item::lower_config_value(&opt_item, &mut alloc);
                    }
                    "remap_roles" => {
                        remap_roles =
                            crate::lower_config_item::lower_config_value(&opt_item, &mut alloc);
                    }
                    // Map fields (nested blocks)
                    "headers" => {
                        headers_expr =
                            crate::lower_config_item::lower_config_value(&opt_item, &mut alloc);
                    }
                    "query_params" => {
                        query_params_expr =
                            crate::lower_config_item::lower_config_value(&opt_item, &mut alloc);
                    }
                    // Provider-specific keys
                    "anthropic_version" => {
                        anthropic_version = Some(crate::lower_config_item::lower_config_value(
                            &opt_item, &mut alloc,
                        ));
                    }
                    "resource_name" => {
                        resource_name = Some(crate::lower_config_item::lower_config_value(
                            &opt_item, &mut alloc,
                        ));
                    }
                    "api_version" => {
                        api_version = Some(crate::lower_config_item::lower_config_value(
                            &opt_item, &mut alloc,
                        ));
                    }
                    // Unknown → request_body
                    other => {
                        let key_expr = alloc(Expr::Literal(Literal::String(other.to_string())));
                        let val_expr =
                            crate::lower_config_item::lower_config_value(&opt_item, &mut alloc);
                        request_body_entries.push((key_expr, val_expr));
                    }
                }
            }
        }
    }

    // Build provider_options from accumulated provider-specific keys
    let provider_options = if let Some(av) = anthropic_version {
        alloc(Expr::Object {
            type_name: Some(Name::new("baml.llm.AnthropicOptions")),
            fields: vec![(Name::new("anthropic_version"), av)],
            spreads: vec![],
        })
    } else if resource_name.is_some() || api_version.is_some() {
        let rn = resource_name.unwrap_or_else(|| alloc(Expr::Null));
        let av = api_version.unwrap_or_else(|| alloc(Expr::Null));
        alloc(Expr::Object {
            type_name: Some(Name::new("baml.llm.AzureOpenAiOptions")),
            fields: vec![
                (Name::new("resource_name"), rn),
                (Name::new("api_version"), av),
            ],
            spreads: vec![],
        })
    } else {
        alloc(Expr::Null)
    };

    let request_body_expr = alloc(Expr::Map {
        entries: request_body_entries,
    });

    // PrimitiveClientOptions { ... }
    let options_expr = alloc(Expr::Object {
        type_name: Some(Name::new("baml.llm.PrimitiveClientOptions")),
        fields: vec![
            (Name::new("model"), model),
            (Name::new("base_url"), base_url),
            (Name::new("default_role"), default_role),
            (Name::new("allowed_roles"), allowed_roles),
            (Name::new("remap_roles"), remap_roles),
            (Name::new("api_key"), api_key),
            (Name::new("provider_options"), provider_options),
            (Name::new("headers"), headers_expr),
            (Name::new("query_params"), query_params_expr),
            (Name::new("request_body"), request_body_expr),
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

fn lower_config_block(cb: &ast::ConfigBlock) -> Vec<ConfigItemDef> {
    cb.items()
        .filter_map(|item| {
            let key_token = item.key()?;
            let value = item.value_str().unwrap_or_default();
            Some(ConfigItemDef {
                key: Name::new(key_token.text()),
                value,
                span: item.syntax().text_range(),
            })
        })
        .collect()
}

/// Lower field-level attributes (single @) from a `Field` node.
fn lower_field_attributes(field: &ast::Field) -> Vec<RawAttribute> {
    field
        .attributes()
        .filter_map(|attr| lower_attribute(&attr))
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
