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
    ast::{
        BuiltinKind, ClientDef, ConfigItemDef, EnumDef, FieldDef, FunctionBodyDef, FunctionDef,
        GeneratorDef, Interpolation, Item, LlmBodyDef, Param, RawAttribute, RawAttributeArg,
        RawPrompt, RetryPolicyDef, SpannedTypeExpr, TemplateStringDef, TestDef, TypeAliasDef,
        VariantDef,
    },
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
                    items.push(Item::Function(func));
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
                if let Some(c) = lower_client(&child) {
                    items.push(Item::Client(c));
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
                if let Some(rp) = lower_retry_policy(&child) {
                    items.push(Item::RetryPolicy(rp));
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

    let body = if let Some(llm) = func.llm_body() {
        Some(FunctionBodyDef::Llm(lower_llm_body(&llm)))
    } else if let Some(expr) = func.expr_body() {
        // Check if the body is `$rust_function` or `$rust_io_function` before lowering
        if let Some(builtin_kind) = check_builtin_body(expr.syntax()) {
            Some(FunctionBodyDef::Builtin(builtin_kind))
        } else {
            let param_names: Vec<Name> = params.iter().map(|p| p.name.clone()).collect();
            let (expr_body, source_map) = lower_expr_body::lower(&expr, &param_names);
            Some(FunctionBodyDef::Expr(expr_body, source_map))
        }
    } else {
        None
    };

    let attributes = lower_attributes_from_node(node);

    Some(FunctionDef {
        name,
        generic_params,
        params,
        return_type,
        throws,
        body,
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

fn lower_params(pl: &ast::ParameterList) -> Vec<Param> {
    pl.params().filter_map(|p| lower_param(&p)).collect()
}

fn lower_param(param: &ast::Parameter) -> Option<Param> {
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

    // Class methods (functions defined inside the class body)
    let methods = class
        .methods()
        .filter_map(|f| lower_function(f.syntax()))
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
fn extract_generic_params(node: &SyntaxNode) -> Vec<Name> {
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

fn lower_client(node: &SyntaxNode) -> Option<ClientDef> {
    let client = ast::ClientDef::cast(node.clone())?;
    let name_token = client.name()?;

    let config_items = client
        .config_block()
        .map(|cb| lower_config_block(&cb))
        .unwrap_or_default();

    Some(ClientDef {
        name: Name::new(name_token.text()),
        config_items,
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

fn lower_retry_policy(node: &SyntaxNode) -> Option<RetryPolicyDef> {
    let rp = ast::RetryPolicyDef::cast(node.clone())?;
    let name_token = rp.name()?;

    let config_items = rp
        .config_block()
        .map(|cb| lower_config_block(&cb))
        .unwrap_or_default();

    Some(RetryPolicyDef {
        name: Name::new(name_token.text()),
        config_items,
        span: node.text_range(),
        name_span: name_token.text_range(),
    })
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
fn lower_attribute(attr: &ast::Attribute) -> Option<RawAttribute> {
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
