//! Item-level desugarings shared by every reader: the `$init_test` registrar
//! synthesis, the LLM client and prompt rules, the injected `@spec` parameters.
//! The tree readers live in `lower_tree`.

use baml_base::{Name, Path, compiler_path};
use text_size::TextRange;

use crate::{
    LoweringDiagnostic,
    ast::{
        CallArg, Expr, ExprId, FunctionBodyDef, FunctionDef, FunctionDefaults, LambdaDef,
        LambdaKind, Param, TypeExprKind,
    },
    lower_expr_body,
};

/// Check if a just-lowered type expression contains `TypeExprKind::Missing` at the root.
/// If so, emit an `UnparseableType` diagnostic.
pub(crate) fn check_missing_type(
    type_expr: &crate::ast::TypeExpr,
    context: String,
    span: text_size::TextRange,
    diags: &mut Vec<LoweringDiagnostic>,
) {
    if matches!(type_expr.kind, crate::ast::TypeExprKind::Missing) {
        diags.push(LoweringDiagnostic::UnparseableType { context, span });
    }
}

/// The class a method is declared in, for the companion bodies it needs.
#[derive(Clone, Copy)]
pub(crate) struct ClassMethodContext<'a> {
    pub(crate) name: &'a Name,
    pub(crate) generic_params: &'a [crate::ast::GenericParam],
}

/// Append the spec-mode client override parameter: `client: ai.Client? = null`.
///
/// The spec-mode direct-call body passes it to `ai.Agent.new(client = client)`;
/// a null override makes the runner fall back to the spec's default client.
pub(crate) fn append_spec_client_param(
    params: &mut Vec<Param>,
    defaults: &mut FunctionDefaults,
    span: text_size::TextRange,
) {
    let null_default = {
        let id = defaults.exprs.exprs.alloc(Expr::Null);
        defaults.source_map.expr_spans.alloc(span);
        id
    };
    let client_ty = TypeExprKind::Path {
        path: baml_base::compiler_path!(ai.Client),
        generic_args: vec![],
        associated_type_bindings: vec![],
    }
    .at(span);
    params.push(Param {
        name: Name::new("client"),
        type_expr: Some(
            TypeExprKind::Optional {
                inner: Box::new(client_ty),
            }
            .at(span),
        ),
        default: Some(crate::ast::DefaultExprId::new(null_default)),
        span,
        name_span: span,
    });
}

/// Append the spec-mode listener parameter:
/// `on_event: ((ai.events.Event) -> void)? = null`.
///
/// Both synthesized companion bodies thread it through unchanged —
/// `Agent.new(on_event = on_event)` for the direct call and
/// `ai.stream.from_spec(..., on_event = on_event)` for the stream; a null
/// listener means no events are delivered.
pub(crate) fn append_spec_on_event_param(
    params: &mut Vec<Param>,
    defaults: &mut FunctionDefaults,
    span: text_size::TextRange,
) {
    let null_default = {
        let id = defaults.exprs.exprs.alloc(Expr::Null);
        defaults.source_map.expr_spans.alloc(span);
        id
    };
    let event_ty = TypeExprKind::Path {
        path: baml_base::compiler_path!(ai.events.Event),
        generic_args: vec![],
        associated_type_bindings: vec![],
    }
    .at(span);
    let listener_ty = TypeExprKind::Function {
        params: vec![crate::ast::FunctionTypeParam {
            name: None,
            optional: false,
            ty: event_ty,
        }],
        ret: Box::new(TypeExprKind::Null.at(span)),
        throws: None,
    }
    .at(span);
    params.push(Param {
        name: Name::new("on_event"),
        type_expr: Some(
            TypeExprKind::Optional {
                inner: Box::new(listener_ty),
            }
            .at(span),
        ),
        default: Some(crate::ast::DefaultExprId::new(null_default)),
        span,
        name_span: span,
    });
}

pub(crate) fn reject_reserved_llm_params(
    params: &mut Vec<Param>,
    function_name: &str,
    diags: &mut Vec<LoweringDiagnostic>,
) {
    const RESERVED: &[(&str, &str)] = &[
        ("client", "the compiler-injected LLM client override"),
        (
            "on_event",
            "the compiler-injected LLM event listener override",
        ),
        ("ctx", "the compiler-provided prompt context"),
    ];

    params.retain(|param| {
        let Some((param_name, reserved_for)) = RESERVED
            .iter()
            .find(|(name, _)| *name == param.name.as_str())
        else {
            return true;
        };
        diags.push(LoweringDiagnostic::ReservedLlmParam {
            function_name: function_name.to_string(),
            param_name: (*param_name).to_string(),
            reserved_for,
            span: param.name_span,
        });
        false
    });
}

/// The provider package + client class the `@spec` desugar constructs for a
/// `"prefix/model"` client string, or `None` for an unknown prefix.
///
/// Kept in sync with the builtin provider packages (`baml_std/openai` etc.)
/// and the user-land `resolve()` convention.
///
/// DRIFT HAZARD — this compile-time `"provider/model"` prefix map is mirrored
/// at runtime by `ai.clients.resolve` (`baml_std/ai/ns_clients/clients.baml`),
/// which handles the same shorthand when the `client:` value is a dynamic
/// string / `baml.env.Ref` instead of a literal. Add a prefix in one place and
/// you must add it in the other.
/// The builtin `"provider/model"` shorthand: each prefix and the provider
/// class it constructs (`<pkg>.<class>.new(model = ...)`).
///
/// Azure is deliberately absent: an Azure request goes to a deployment, which
/// a model name does not identify, so `openai.AzureClient` must be built with
/// its `deployment_id` or `base_url` (see `AZURE_SHORTHAND_REMOVED`).
///
/// The ONE provider table. A literal `client "openai/gpt-4o-mini"` lowers
/// straight to the constructor (`spec_client_provider`); a dynamic
/// `client:` expression lowers to `ai.clients.resolve(selector, providers)`
/// where `providers` is a lambda synthesized from this same table
/// (`synthesize_llm_spec_body`). The stdlib never names a provider package —
/// every provider implements `ai.Client`, so `ai` sits below them in the
/// package graph — and the two lowerings cannot drift because they read one
/// list.
pub const SHORTHAND_PROVIDERS: &[(&str, Path)] = &[
    ("openai", compiler_path!(openai.ResponsesClient.new)),
    ("openai-chat", compiler_path!(openai.ChatClient.new)),
    (
        "openai-decisions",
        compiler_path!(openai.DecisionsClient.new),
    ),
    ("openai-images", compiler_path!(openai.ImageClient.new)),
    ("ollama", compiler_path!(openai.OllamaClient.new)),
    ("openrouter", compiler_path!(openai.OpenRouterClient.new)),
    ("anthropic", compiler_path!(anthropic.Client.new)),
    ("typesafeai", compiler_path!(typesafeai.Client.new)),
    ("google", compiler_path!(google.GeminiClient.new)),
    ("vertex", compiler_path!(google.VertexClient.new)),
    ("bedrock", compiler_path!(aws.BedrockClient.new)),
    (
        "ai-gateway-images",
        compiler_path!(vercel.AiGatewayImageClient.new),
    ),
    (
        "claude-code",
        compiler_path!(claude_code.ClaudeCodeClient.new),
    ),
];

/// Why a literal `client: "azure/<model>"` is rejected. `ai.clients.resolve`
/// gives the same reason at runtime (`_from_shorthand` in `ai/ns_internal`).
const AZURE_SHORTHAND_REMOVED: &str = "there is no \"azure/<model>\" shorthand: an Azure \
     request goes to a deployment, which a model name does not identify; build \
     openai.AzureClient.new(model = ..., deployment_id = ..., resource_name = ...)";

/// The constructor a `"provider/model"` literal's provider names
/// (`<package>.<Client>.new`).
pub(crate) fn spec_client_provider(client: &str) -> Option<&'static Path> {
    let (prefix, _model) = client.split_once('/')?;
    SHORTHAND_PROVIDERS
        .iter()
        .find(|(known, _)| *known == prefix)
        .map(|(_, constructor)| constructor)
}

/// Warn on each `${` in a quoted prompt.
///
/// A quoted prompt is inert, so a `${...}` the author meant as an
/// interpolation reaches the model verbatim with nothing else going wrong —
/// no parse error, no unresolved name, just a wrong prompt. The marker is
/// searched in the *raw* token text so the span lands on the source
/// characters; a quoted string's escapes never introduce or remove a `${`
/// (`\$` is not an escape in this flavor, so it decodes to `\$`).
pub(crate) fn warn_quoted_prompt_interpolation(
    text: &str,
    range: text_size::TextRange,
    diags: &mut Vec<LoweringDiagnostic>,
) {
    for (offset, _) in text.match_indices("${") {
        let offset = u32::try_from(offset).expect("prompt literal exceeds u32 length");
        let start = range.start() + text_size::TextSize::from(offset);
        diags.push(LoweringDiagnostic::QuotedPromptInterpolation {
            span: text_size::TextRange::new(start, start + text_size::TextSize::of("${")),
        });
    }
}

/// Classify a quoted `client "provider/model"` string: the builtin provider
/// constructor it names, or `None` after reporting why it names none.
pub(crate) fn classify_client_string(
    function_name: &str,
    text: &str,
    span: text_size::TextRange,
    diags: &mut Vec<LoweringDiagnostic>,
) -> Option<lower_expr_body::LlmClient> {
    let Some((prefix, model)) = text.split_once('/') else {
        diags.push(LoweringDiagnostic::InvalidLlmClient {
            function_name: function_name.to_string(),
            reason: format!("\"{text}\" is not a \"provider/model\" string (no `/`)"),
            span,
        });
        return None;
    };
    let Some(constructor) = spec_client_provider(text) else {
        if prefix == "azure" {
            diags.push(LoweringDiagnostic::InvalidLlmClient {
                function_name: function_name.to_string(),
                reason: AZURE_SHORTHAND_REMOVED.to_string(),
                span,
            });
            return None;
        }
        diags.push(LoweringDiagnostic::InvalidLlmClient {
            function_name: function_name.to_string(),
            reason: format!(
                "no builtin provider for prefix \"{prefix}\"; construct a client \
                 value instead (OpenAI-compatible endpoints: \
                 openai.GenericClient.new(base_url = ..., model = \"{model}\"))"
            ),
            span,
        });
        return None;
    };
    Some(lower_expr_body::LlmClient::Provider {
        constructor,
        model: model.to_string(),
    })
}

/// The removed unquoted shorthand (`client: openai/gpt-4o-mini`) parses as a
/// whitespace-free division chain; report it before it cascades into
/// unresolved-name errors. `text` is the binary expression's source.
pub(crate) fn reject_unquoted_client_shorthand(
    function_name: &str,
    text: &str,
    span: text_size::TextRange,
    diags: &mut Vec<LoweringDiagnostic>,
) -> bool {
    if text.contains('/') && !text.contains(char::is_whitespace) {
        diags.push(LoweringDiagnostic::InvalidLlmClient {
            function_name: function_name.to_string(),
            reason: format!("quote the model string: client: \"{text}\""),
            span,
        });
        return true;
    }
    false
}

/// Synthesize a `$init_test` function that registers tests and testsets
/// into a `testing.Registry` parameter.
///
/// The function body calls the collector's `register_test(name, lambda, null)`
/// for each test and `register_test_set(name, collector_lambda, null)` for
/// each testset.
///
/// Lambda bodies are lowered from the original CST `BLOCK_EXPR` nodes.
///
/// Derive the `$init_test_<key>` suffix from a file's root-relative path.
///
/// Strips the extension and replaces every character that isn't a letter,
/// digit, or `_` (notably the path separators and `.`, which would otherwise
/// break the `.`-delimited namespace routing of function names). The result is
/// stable across compilations, unique per file within its package, and free of
/// the checkout's location, e.g. `ns_arrays/arrays.baml` -> `ns_arrays_arrays`.
pub(crate) fn init_test_key_from_path(path: &std::path::Path) -> String {
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
pub(crate) fn test_owner_from_path(path: Option<&std::path::Path>) -> String {
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

/// Whether `name` is a compiler-synthesized `$init_test` registration
/// function (see `synthesize_init_test_function`).
pub fn is_init_test_function(name: &str) -> bool {
    name.starts_with("$init_test")
}

/// The function is named `"$init_test_<sanitized_path>"` to avoid collisions
/// when multiple files contain tests. The suffix is derived from the file path
/// rather than its `FileId`: a `FileId` is a load-order index that shifts
/// whenever an earlier file is added (e.g. a new stdlib file), which would
/// churn every snapshot referencing the name. The path is stable across
/// compilations. When no path is available, uses plain `"$init_test"`.
pub(crate) fn synthesize_init_test_function<R: TestRegistration>(
    registrations: &[R],
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

    // The single parameter: `__registry: testing.TestCollector`, spelled
    // with a leading space so no test body can name it.
    let registry_param = Param {
        name: Name::new(" __registry"),
        type_expr: Some(
            crate::ast::TypeExprKind::Path {
                path: baml_base::compiler_path!(testing.TestCollector),
                generic_args: vec![],
                associated_type_bindings: vec![],
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
        // Unit, like the `null` tail above: a synthesized signature is a
        // complete one (a missing return type is a parse error's recovery).
        return_type: Some(crate::ast::TypeExprKind::Null.at(span)),
        throws: None,
        body: Some(FunctionBodyDef::Expr(body, source_map)),
        declarative_meta: None,
        metadata: crate::ast::FunctionMetadata::language_internal(
            crate::ast::FunctionOrigin::TestInitializer,
        ),
        attributes: vec![],
        docstring: None,
        header_title: None,
        is_tagged_template_tag: false,
        directives: crate::ast::NativeDirectives::default(),
        trace_hook: None,
        span,
        name_span: span,
    }
}

/// A top-level `test` or `testset` as the registrar synthesis reads it,
/// whichever tree it came from: the pieces lower into the registrar's arena
/// at the point the call needs them, so the arenas fill in one order.
pub(crate) trait TestRegistration {
    /// A `testset` (registers a collector lambda) rather than a `test`.
    fn is_set(&self) -> bool;
    /// The body's source range: the registered lambda's span, so HIR scope
    /// lookup by offset finds the lambda the body's statements belong to.
    fn body_span(&self) -> TextRange;
    fn lower_name(&self, ctx: &mut lower_expr_body::InitTestContext) -> ExprId;
    /// The body: a test's as a plain lambda body (no collector in scope), a
    /// testset's as the collector lambda's body (`testset` bound).
    fn lower_body(&self, ctx: &mut lower_expr_body::InitTestContext, span: TextRange) -> ExprId;
    /// The `with` runner, or `null` when there is none.
    fn lower_runner(&self, ctx: &mut lower_expr_body::InitTestContext, span: TextRange) -> ExprId;
}

/// Synthesize a single `register_test(name, lambda, runner)` or
/// `register_test_set(name, collector_lambda, runner)` call on the collector.
fn synthesize_register_call<R: TestRegistration>(
    reg: &R,
    test_owner: &str,
    ctx: &mut lower_expr_body::InitTestContext,
) -> ExprId {
    let span = text_size::TextRange::default();
    let collector_param = || Param {
        name: Name::new("testset"),
        type_expr: Some(
            crate::ast::TypeExprKind::Path {
                path: baml_base::compiler_path!(testing.TestCollector),
                generic_args: vec![],
                associated_type_bindings: vec![],
            }
            .at(span),
        ),
        default: None,
        span,
        name_span: span,
    };
    // The body lowers into `$init_test`'s own arena.
    let lambda_body = reg.lower_body(ctx, span);
    let (params, register) = if reg.is_set() {
        (vec![collector_param()], "register_test_set_at")
    } else {
        (vec![], "register_test_at")
    };
    let lambda_def = LambdaDef {
        kind: LambdaKind::Anonymous,
        params,
        defaults: FunctionDefaults::empty(),
        return_type: Some(crate::ast::TypeExprKind::Null.at(span)),
        throws: None,
        body: Some(lambda_body),
        span,
    };

    // registry.register_test_at(owner, ...) / register_test_set_at(owner, ...)
    let method_call_target = ctx.alloc_expr(
        Expr::Path(baml_base::Path::new(vec![
            Name::new(" __registry"),
            Name::new(register),
        ])),
        span,
    );

    // Args: (owner, name_expr, lambda, runner_or_null)
    let name_arg = reg.lower_name(ctx);
    let owner_arg = ctx.alloc_expr(
        Expr::Literal(crate::ast::Literal::String(test_owner.to_string())),
        span,
    );
    // The lambda carries the body's real source range so HIR scope lookup
    // resolves names inside the body correctly. The body's statements carry
    // real source offsets, so a synthetic span (disjoint from those offsets)
    // would make `scope_at_offset` miss the lambda scope, and every
    // `let`-bound local would fail to resolve (reading the local would fall
    // back to a null placeholder). The range is unique per test block, so
    // distinct lambda scopes stay distinguishable.
    let lambda_arg = ctx.alloc_expr(Expr::Lambda(Box::new(lambda_def)), reg.body_span());
    let runner_arg = reg.lower_runner(ctx, span);

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

// NOTE: the legacy `client<llm>` / `retry_policy` config-block synthesis
// (client identity lets, `<Client>$new` companions, provider option tables,
// retry-policy lets) lived here. Both blocks are removed language surface:
// clients are plain values (`client Name = <expr>;`) and retry composes at
// the client boundary via `ai.Retry`. Their CST nodes now lower to a single
// migration diagnostic each, as does `template_string`.

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
