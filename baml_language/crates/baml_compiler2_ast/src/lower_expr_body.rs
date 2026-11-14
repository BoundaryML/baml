//! The body-lowering context and the desugarings that build arena nodes
//! programmatically: `LoweringContext` (arenas plus the parallel
//! `AstSourceMap`), the LLM `@spec` companion bodies, the test registrar
//! context, and the tagged-template elaboration. The tree readers live in
//! `lower_tree`.

use baml_base::Name;
use la_arena::Arena;
use text_size::TextRange;

use crate::{
    LoweringDiagnostic,
    ast::{
        AstSourceMap, BinaryOp, CallArg, CatchArm, CatchArmId, CatchClause, CatchClauseKind, Expr,
        ExprBody, ExprId, FunctionDefaults, LambdaDef, LambdaKind, LetOrigin, Literal, LoopOrigin,
        MapExprEntry, MatchArm, MatchArmId, ObjectExprField, Param, PatId, Pattern, Stmt, StmtId,
        TemplateIfBranch, TemplateSegment, TypeAnnotId, TypeExpr, TypeExprKind,
    },
};

/// A reference to an environment variable found in source code (`env.VAR_NAME`).
// `PartialEq`/`Eq` let this participate in `FileAst`'s value equality, which
// gives the `file_ast` Salsa query early-cutoff (see `baml_compiler2_hir`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvVarRef {
    /// The variable name (e.g., `"OPENAI_API_KEY"`).
    pub name: String,
    /// The text range of the entire `env.VAR_NAME` expression in the source.
    pub range: TextRange,
}

/// Which surface a file-scope binding belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TopLevelLetSurface {
    /// A global variable written in an ordinary source file.
    Source,
    /// One compiler-generated persistent binding of a `Session.eval`
    /// submission. Sessions use the global representation internally so each
    /// committed binding can be re-imported by later submissions; their
    /// generated source never annotates a binding.
    Session,
}

/// The type a `const NAME: …` sub-pattern denotes when it is a pure type
/// ascription: a type pattern, or an alternation of them (`const V: int |
/// string` parses its annotation as an or-pattern of types, which is the union
/// type).
/// Any structural sub-pattern is not an annotation.
pub(crate) fn ascription_type_expr(
    patterns: &la_arena::Arena<Pattern>,
    sub: PatId,
) -> Option<TypeExpr> {
    match &patterns[sub] {
        Pattern::Type(type_expr) => Some(type_expr.clone()),
        Pattern::Or(alternatives) => {
            let variants = alternatives
                .iter()
                .map(|&alternative| match &patterns[alternative] {
                    Pattern::Type(type_expr) => Some(type_expr.clone()),
                    _ => None,
                })
                .collect::<Option<Vec<_>>>()?;
            let span = variants
                .iter()
                .map(|variant| variant.span)
                .reduce(TextRange::cover)?;
            Some(TypeExpr {
                kind: crate::ast::TypeExprKind::Union { variants },
                span,
            })
        }
        _ => None,
    }
}

/// Context for building the `$init_test` function body.
///
/// Wraps a `LoweringContext` so that runner expressions can be lowered
/// directly into the same arena (no IIFE indirection needed).
pub(crate) struct InitTestContext {
    pub(crate) inner: LoweringContext,
}

impl InitTestContext {
    pub(crate) fn new() -> Self {
        Self {
            inner: LoweringContext::new(),
        }
    }

    pub(crate) fn alloc_expr(&mut self, expr: Expr, span: text_size::TextRange) -> ExprId {
        self.inner.alloc_expr(expr, span)
    }

    pub(crate) fn alloc_stmt(
        &mut self,
        stmt: Stmt,
        span: text_size::TextRange,
    ) -> crate::ast::StmtId {
        self.inner.alloc_stmt(stmt, span)
    }

    pub(crate) fn finish(
        self,
        root_expr: Option<ExprId>,
    ) -> (
        ExprBody,
        AstSourceMap,
        Vec<LoweringDiagnostic>,
        Vec<EnvVarRef>,
    ) {
        self.inner.finish(root_expr)
    }
}

/// BEP `@spec`: synthesize the body of the `<Fn>@spec` companion — an
/// `ai.FunctionSpec<Out>` literal binding the function's arguments:
///
/// ```baml
/// ai.FunctionSpec<Out> {
///     spec_name: "Fn",
///     args: { "p": p, ... },
///     prompt_template: (output_format: ai.OutputFormat) -> {
///         // the parameter's real name is ` __spec_output_format` (leading
///         // space) so it can never shadow a user identifier
///         let ctx = ai.internal.SpecCtx { _output_format: output_format };
///         let tagged = baml.TaggedString { ...the function's prompt... };
///         ai.internal.assemble_llm_prompt(tagged.parts, tagged.values)
///     },
///     toolbox: ai.Toolbox.new([ai.tool(a), ...]),
///     default_client: openai.ResponsesClient.new(model = "gpt-4o-mini"),
/// }
/// ```
///
/// The prompt closure uses the same structural parts/values representation as
/// the built-in `prompt` tag. `${role(...)}` values become prompt messages
/// and media remains structural. `ctx` is bound to an `ai.internal.SpecCtx`,
/// so `${ctx.output_format()}` renders the closure's schema handle and every
/// other interpolation captures the enclosing function's parameters.
///
/// The `default_client` expression is evaluated when this `@spec` body runs —
/// that is, on every call of the LLM function — never during `$init`. Provider
/// construction itself is pure, so building the spec still never touches
/// credentials; only a request reads them.
///
/// In the `tools` list, a bare function reference is wrapped in `ai.tool(...)`;
/// any other element expression must already produce an `ai.Tool`.
/// The parts of an LLM function's spec body that are read off a syntax tree:
/// the prompt, the `tools` value and a dynamic `client` expression. The body
/// itself is built the same way from either tree
/// ([`synthesize_llm_spec_body`]); each source lowers its pieces into the
/// context at the point the body needs them, so the arenas fill in one order.
pub(crate) trait LlmSpecSource {
    /// The prompt literal's range: the prompt lambda's span.
    fn prompt_span(&self) -> TextRange;
    /// The prompt's segments, lowered into `ctx`.
    fn prompt_segments(&self, ctx: &mut LoweringContext) -> Vec<TemplateSegment>;
    /// The `tools` value lowered into `ctx`, or `None` when the field is
    /// absent. A list lowers element by element, each bare function reference
    /// passed through `wrap_bare_ref` (which builds `ai.tool(<ref>)`) right
    /// after it is lowered.
    fn tools(
        &self,
        ctx: &mut LoweringContext,
        span: TextRange,
        wrap_bare_ref: &mut dyn FnMut(&mut LoweringContext, ExprId) -> ExprId,
    ) -> Option<ExprId>;
    /// The `client` expression, for an [`LlmClient::Expr`] client.
    fn client_expr(&self, ctx: &mut LoweringContext, span: TextRange) -> ExprId;
}

/// How an LLM function's `client:` value lowers into the spec's
/// `default_client`.
pub(crate) enum LlmClient {
    /// A `"provider/model"` string, mapped at compile time to a builtin
    /// provider constructor.
    Provider {
        /// The client constructor, `<package>.<Client>.new`, from
        /// [`SHORTHAND_PROVIDERS`](crate::lower_cst::SHORTHAND_PROVIDERS).
        constructor: &'static baml_base::Path,
        model: String,
    },
    /// An arbitrary expression evaluating to `ai.Selector` (a declared
    /// client name, a constructor call, a wrapper, a runtime
    /// `"provider/model"` string, an `env.X` reference, ...), read from the
    /// source. Wrapped in `ai.clients.resolve(...)` and resolved when the
    /// function is called.
    Expr,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn synthesize_llm_spec_body(
    function_name: &str,
    param_names: &[Name],
    generic_param_names: &[Name],
    owner_class_name: Option<&Name>,
    owner_generic_param_names: &[Name],
    client: &LlmClient,
    out_type: Option<crate::ast::TypeExpr>,
    source: &dyn LlmSpecSource,
    span: TextRange,
) -> (
    ExprBody,
    AstSourceMap,
    Vec<TemplateSegment>,
    Vec<LoweringDiagnostic>,
    Vec<EnvVarRef>,
) {
    use crate::ast::{CallArg, Literal};

    let mut ctx = LoweringContext::new();

    // spec_name: "Fn"
    let name_lit = ctx.alloc_expr(
        Expr::Literal(Literal::String(function_name.to_string())),
        span,
    );

    let binds_receiver = owner_class_name.is_some()
        && param_names
            .first()
            .is_some_and(|name| name.as_str() == "self");
    let target_name = Name::new(function_name);
    let target = if binds_receiver {
        let receiver = ctx.alloc_expr(Expr::Path(baml_base::Path::bare(Name::new("self"))), span);
        ctx.alloc_expr(
            Expr::MemberAccess {
                base: receiver,
                member: target_name,
            },
            span,
        )
    } else if let Some(owner) = owner_class_name {
        ctx.alloc_expr(
            Expr::Path(baml_base::Path::compiler_item(vec![
                owner.clone(),
                target_name,
            ])),
            span,
        )
    } else {
        // Compiler-written, so a parameter named like the function cannot
        // shadow it.
        ctx.alloc_expr(
            Expr::Path(baml_base::Path::compiler_item(vec![target_name])),
            span,
        )
    };
    let target_args = companion_type_args(
        generic_param_names,
        owner_generic_param_names,
        owner_class_name.is_some() && !binds_receiver,
        span,
    );
    let target = if target_args.is_empty() {
        target
    } else {
        ctx.alloc_expr(
            Expr::GenericApply {
                base: target,
                type_args: target_args,
            },
            span,
        )
    };

    // args: { "p": p, ... }
    let entries: Vec<MapExprEntry> = param_names
        .iter()
        .map(|name| {
            let key = ctx.alloc_expr(
                Expr::Literal(Literal::String(name.as_str().to_string())),
                span,
            );
            let value = ctx.alloc_expr(Expr::Path(baml_base::Path::bare(name.clone())), span);
            MapExprEntry::explicit(key, value)
        })
        .collect();
    let args_map = ctx.alloc_expr(Expr::Map { entries }, span);

    // prompt_template: ( __spec_output_format: ai.OutputFormat) -> ai.Prompt { ... }
    //
    // The lambda parameter carries a leading-space name so it can never
    // shadow a user identifier: a function parameter named `output_format`
    // must stay visible to `${output_format}` in the template (the parameter
    // is only the render calling convention; `ctx.output_format()` is the
    // documented way to reach the rendered schema). Mirrors the `__tt_*`
    // accumulator naming in `elaborate_tagged_body`.
    //
    // Binding references are span-position-checked against their `let`'s
    // visibility window, and the template's `${ctx.…}` interps keep their real
    // source ranges inside the literal — so the synthesized `let ctx` must sit
    // at an empty range at the literal's *start*, before every reference
    // (mirrors the accumulator lets in `elaborate_tagged_body`).
    let of_param_name = Name::new(" __spec_output_format");
    let prompt_lambda_span = source.prompt_span();
    let prompt_start = TextRange::empty(prompt_lambda_span.start());
    let of_ref = ctx.alloc_expr(
        Expr::Path(baml_base::Path::bare(of_param_name.clone())),
        prompt_start,
    );
    let spec_ctx_obj = ctx.alloc_expr(
        Expr::Object {
            type_name: baml_base::compiler_path!(ai.internal.SpecCtx),
            type_args: vec![],
            fields: vec![ObjectExprField::explicit(
                Name::new("_output_format"),
                of_ref,
            )],
            spreads: vec![],
        },
        prompt_start,
    );
    let ctx_pat = ctx.alloc_pattern(
        Pattern::Bind {
            name: Name::new("ctx"),
            subpat: None,
        },
        prompt_start,
    );
    let let_ctx = ctx.alloc_stmt(
        Stmt::Let {
            pattern: ctx_pat,
            initializer: Some(spec_ctx_obj),
            origin: LetOrigin::Source,
            is_const: false,
            else_branch: None,
        },
        prompt_start,
    );
    // Flatten the template exactly like the public `prompt` tag: values stay
    // structural until the Rust prompt assembler sees them. Rewrite the
    // prompt-local constructors before name resolution; `role` is the same
    // binding that the public tag supplies to its body lambda.
    let segments = source.prompt_segments(&mut ctx);
    // `cache(...)` is prompt-local in the same way: a bare call lowers to
    // `ai.internal.make_cache`. Its one parameter is defaulted (`cache()` asks
    // the rendering client for its default), and a defaulted parameter must be
    // passed by name, so the positional `cache(x)` is labelled `args = x`.
    // BUG: both are recognized by SPELLING before resolution, so a parameter
    // or item of the writer's named `role` or `cache` is never called from a
    // prompt and nothing says so; they should be reserved like `ctx` and
    // `client` (E0010), or rewritten only when no binding claims the name.
    let prompt_local_callees: Vec<(ExprId, ExprId, baml_base::Path)> = ctx
        .exprs
        .iter()
        .filter_map(|(call, expr)| {
            let Expr::Call { callee, .. } = expr else {
                return None;
            };
            let Expr::Path(path) = &ctx.exprs[*callee] else {
                return None;
            };
            let [name] = path.segments() else {
                return None;
            };
            let target = match name.as_str() {
                "role" => baml_base::compiler_path!(ai.internal.make_role),
                "cache" => baml_base::compiler_path!(ai.internal.make_cache),
                _ => return None,
            };
            Some((call, *callee, target))
        })
        .collect();
    for (call, callee, target) in prompt_local_callees {
        let is_cache = target.leaf().as_str() == "make_cache";
        ctx.exprs[callee] = Expr::Path(target);
        if is_cache
            && let Expr::Call { args, .. } = &mut ctx.exprs[call]
            && let [only] = args.as_mut_slice()
            && only.label.is_none()
        {
            only.label = Some(Name::new("args"));
        }
    }

    let prev_synth = std::mem::replace(&mut ctx.synthesizing, true);
    let tagged_expr = ctx.elaborate_tagged_body(&segments, prompt_lambda_span);
    ctx.synthesizing = prev_synth;

    // Evaluate the flattened template once before reading its two arrays.
    let tagged_name = Name::new(" __spec_tagged_prompt");
    let tagged_pat = ctx.alloc_pattern(
        Pattern::Bind {
            name: tagged_name.clone(),
            subpat: None,
        },
        prompt_start,
    );
    let let_tagged = ctx.alloc_stmt(
        Stmt::Let {
            pattern: tagged_pat,
            initializer: Some(tagged_expr),
            origin: LetOrigin::Source,
            is_const: false,
            else_branch: None,
        },
        prompt_start,
    );
    let parts_base = ctx.alloc_expr(
        Expr::Path(baml_base::Path::bare(tagged_name.clone())),
        prompt_start,
    );
    let parts = ctx.alloc_expr(
        Expr::MemberAccess {
            base: parts_base,
            member: Name::new("parts"),
        },
        prompt_start,
    );
    let values_base = ctx.alloc_expr(Expr::Path(baml_base::Path::bare(tagged_name)), prompt_start);
    let values = ctx.alloc_expr(
        Expr::MemberAccess {
            base: values_base,
            member: Name::new("values"),
        },
        prompt_start,
    );
    let assemble_callee = ctx.alloc_expr(
        Expr::Path(baml_base::compiler_path!(ai.internal.assemble_llm_prompt)),
        prompt_start,
    );
    let prompt_ast = ctx.alloc_expr(
        Expr::Call {
            callee: assemble_callee,
            type_args: vec![],
            args: vec![CallArg::positional(parts), CallArg::positional(values)],
        },
        prompt_start,
    );
    let block_body = ctx.alloc_expr(
        Expr::Block {
            stmts: vec![let_ctx, let_tagged],
            tail_expr: Some(prompt_ast),
        },
        prompt_lambda_span,
    );
    // Wrap the whole template body: a user expression inside the prompt may
    // throw (e.g. `${value.to_json()}`); the stored `prompt_template`
    // boundary is typed `throws ai.errors.PromptRenderError`, so anything
    // raised while rendering is wrapped into that one typed failure here.
    let err_name = Name::new(" __prompt_render_err");
    let err_binding = ctx.alloc_pattern(
        Pattern::Bind {
            name: err_name.clone(),
            subpat: None,
        },
        prompt_start,
    );
    let wildcard = ctx.alloc_pattern(Pattern::Wildcard, prompt_start);
    let message = ctx.alloc_expr(
        Expr::Literal(Literal::String(
            "prompt template rendering threw".to_string(),
        )),
        prompt_start,
    );
    let cause = ctx.alloc_expr(Expr::Path(baml_base::Path::bare(err_name)), prompt_start);
    let render_error = ctx.alloc_expr(
        Expr::Object {
            type_name: baml_base::compiler_path!(ai.errors.PromptRenderError),
            type_args: vec![],
            fields: vec![
                ObjectExprField::explicit(Name::new("message"), message),
                ObjectExprField::explicit(Name::new("cause"), cause),
            ],
            spreads: vec![],
        },
        prompt_start,
    );
    let throw_wrapped = ctx.alloc_expr(
        Expr::Throw {
            value: render_error,
        },
        prompt_start,
    );
    let wrap_arm = ctx.alloc_catch_arm(
        CatchArm {
            pattern: wildcard,
            body: throw_wrapped,
        },
        prompt_start,
    );
    let lambda_body = ctx.alloc_expr(
        Expr::Catch {
            base: block_body,
            clause: CatchClause {
                kind: CatchClauseKind::CatchAll,
                binding: err_binding,
                stack_trace_binding: None,
                arms: vec![wrap_arm],
            },
        },
        prompt_lambda_span,
    );
    let of_param = Param {
        name: of_param_name,
        type_expr: Some(
            (TypeExprKind::Path {
                path: baml_base::compiler_path!(ai.OutputFormat),
                generic_args: vec![],
                associated_type_bindings: vec![],
            })
            .at(span),
        ),
        default: None,
        span,
        name_span: span,
    };
    // The prompt lambda carries the backtick's own range: lambda scopes are
    // located by exact span within their owner (semantic index
    // `lambda_scope_for_within`), so the two lambdas synthesized into this
    // body must not share a range — the default-client thunk keeps the block
    // span.
    let prompt_lambda = ctx.alloc_expr(
        Expr::Lambda(Box::new(LambdaDef {
            kind: LambdaKind::Anonymous,
            params: vec![of_param],
            defaults: FunctionDefaults::empty(),
            return_type: None,
            throws: None,
            body: Some(lambda_body),
            span: prompt_lambda_span,
        })),
        prompt_lambda_span,
    );
    // toolbox: ai.Toolbox.new([...])
    let mut wrap_bare_ref = |ctx: &mut LoweringContext, id: ExprId| {
        // A bare function reference: normalize through ai.tool().
        let tool_callee =
            ctx.alloc_expr(Expr::Path(baml_base::compiler_path!(ai.tools.tool)), span);
        ctx.alloc_expr(
            Expr::Call {
                callee: tool_callee,
                type_args: vec![],
                args: vec![CallArg::positional(id)],
            },
            span,
        )
    };
    let tool_list = source
        .tools(&mut ctx, span, &mut wrap_bare_ref)
        .unwrap_or_else(|| ctx.alloc_expr(Expr::Array { elements: vec![] }, span));
    let toolbox_callee = ctx.alloc_expr(
        Expr::Path(baml_base::compiler_path!(ai.tools.Toolbox.new)),
        span,
    );
    let toolbox = ctx.alloc_expr(
        Expr::Call {
            callee: toolbox_callee,
            type_args: vec![],
            args: vec![CallArg::positional(tool_list)],
        },
        span,
    );

    // default_client — evaluated when the `@spec` function RUNS, i.e. when the
    // LLM function is called, *not* during `$init`. (This whole body is the
    // `<Fn>@spec` companion function; the caller is `<Fn>`'s own body,
    // `ai.Agent<Out>.new(client = client).run(Fn@spec(p...))`.) So a dynamic
    // selector is re-read on every call and a host may load secrets after the
    // runtime initializes.
    //
    // Two shapes:
    //
    //   * `client "provider/model"` — a STRING LITERAL, mapped at compile time
    //     to a builtin provider constructor (`spec_client_provider` in
    //     lower_cst.rs). Static, so an unknown prefix is a compile error.
    //     Provider construction is pure — it never touches env.
    //   * anything else — an arbitrary expression, wrapped in
    //     `ai.clients.resolve(selector, providers)` so every dynamic selector
    //     shape works: an `ai.Client` value (identity), a runtime
    //     `"provider/model"` string, or a `baml.env.Ref` (read at call time,
    //     then the string path). Unknown prefixes / unset vars become typed
    //     runtime `ai` errors rather than an opaque `InitFailed`. The call is
    //     compiler-assisted like every `ai.clients.resolve` call
    //     (`complete_client_resolve_call`): the provider table rides along
    //     as a lambda synthesized from `SHORTHAND_PROVIDERS`.
    let default_client = match client {
        LlmClient::Provider { constructor, model } => {
            let model_lit = ctx.alloc_expr(Expr::Literal(Literal::String(model.clone())), span);
            let ctor_callee = ctx.alloc_expr(Expr::Path((*constructor).clone()), span);
            ctx.alloc_expr(
                Expr::Call {
                    callee: ctor_callee,
                    type_args: vec![],
                    args: vec![CallArg::named("model", model_lit)],
                },
                span,
            )
        }
        LlmClient::Expr => {
            let inner = source.client_expr(&mut ctx, span);
            let resolve_callee = ctx.alloc_expr(
                Expr::Path(baml_base::compiler_path!(ai.clients.resolve)),
                span,
            );
            let args = ctx.complete_client_resolve_call(
                resolve_callee,
                vec![CallArg::positional(inner)],
                span,
            );
            ctx.alloc_expr(
                Expr::Call {
                    callee: resolve_callee,
                    type_args: vec![],
                    args,
                },
                span,
            )
        }
    };

    let type_args = out_type.map(|t| vec![t]).unwrap_or_default();
    let spec_obj = ctx.alloc_expr(
        Expr::Object {
            type_name: baml_base::compiler_path!(ai.FunctionSpec),
            type_args,
            fields: vec![
                ObjectExprField::explicit(Name::new("spec_name"), name_lit),
                ObjectExprField::explicit(Name::new("target_function"), target),
                ObjectExprField::explicit(Name::new("args"), args_map),
                ObjectExprField::explicit(Name::new("prompt_template"), prompt_lambda),
                ObjectExprField::explicit(Name::new("toolbox"), toolbox),
                ObjectExprField::explicit(Name::new("default_client"), default_client),
            ],
            spreads: vec![],
        },
        span,
    );

    let (body, source_map, diagnostics, env_var_refs) = ctx.finish(Some(spec_obj));
    (body, source_map, segments, diagnostics, env_var_refs)
}

/// Every prefix the `"provider/model"` shorthand knows, as a string array
/// literal, for `ai.clients.resolve`'s error messages.
fn synthesize_shorthand_prefixes(ctx: &mut LoweringContext, span: TextRange) -> ExprId {
    let elements = crate::lower_cst::SHORTHAND_PROVIDERS
        .iter()
        .map(|(prefix, _)| {
            ctx.alloc_expr(Expr::Literal(Literal::String((*prefix).to_string())), span)
        })
        .collect();
    ctx.alloc_expr(Expr::Array { elements }, span)
}

/// The runtime half of the `"provider/model"` shorthand, as a lambda:
///
/// ```text
/// (prefix: string, model: string) -> ai.Client? => match (prefix) {
///     "openai" => openai.ResponsesClient.new(model = model),
///     ...
///     _ => null,
/// }
/// ```
///
/// Generated from `SHORTHAND_PROVIDERS`, the same table the literal-client
/// lowering reads, and handed to `ai.clients.resolve` at each call site
/// (`complete_client_resolve_call`). The lambda's range is an empty one at
/// the call's end: lambda scopes are located by exact span within their
/// owner, so it must not share a range with any lambda written at the call,
/// nor with the prompt lambda synthesized into a spec body.
fn synthesize_shorthand_providers(ctx: &mut LoweringContext, span: TextRange) -> ExprId {
    let prefix_name = Name::new("prefix");
    let model_name = Name::new("model");
    let string_ty = || (TypeExprKind::String).at(span);
    let param = |name: &Name| Param {
        name: name.clone(),
        type_expr: Some(string_ty()),
        default: None,
        span,
        name_span: span,
    };

    let scrutinee = ctx.alloc_expr(Expr::Path(baml_base::Path::bare(prefix_name.clone())), span);
    let mut arms = Vec::with_capacity(crate::lower_cst::SHORTHAND_PROVIDERS.len() + 1);
    for (prefix, constructor) in crate::lower_cst::SHORTHAND_PROVIDERS {
        let pattern = ctx.alloc_pattern(
            Pattern::Type(
                (TypeExprKind::Literal {
                    value: baml_base::Literal::String((*prefix).to_string()),
                })
                .at(span),
            ),
            span,
        );
        let ctor_callee = ctx.alloc_expr(Expr::Path(constructor.clone()), span);
        let model = ctx.alloc_expr(Expr::Path(baml_base::Path::bare(model_name.clone())), span);
        let body = ctx.alloc_expr(
            Expr::Call {
                callee: ctor_callee,
                type_args: vec![],
                args: vec![CallArg::named("model", model)],
            },
            span,
        );
        arms.push(ctx.alloc_match_arm(
            MatchArm {
                pattern,
                guard: None,
                body,
            },
            span,
        ));
    }
    let wildcard = ctx.alloc_pattern(Pattern::Wildcard, span);
    let null = ctx.alloc_expr(Expr::Null, span);
    arms.push(ctx.alloc_match_arm(
        MatchArm {
            pattern: wildcard,
            guard: None,
            body: null,
        },
        span,
    ));
    let body = ctx.alloc_expr(
        Expr::Match {
            scrutinee,
            scrutinee_type: None,
            arms,
        },
        span,
    );

    let client_ty = (TypeExprKind::Path {
        path: baml_base::compiler_path!(ai.Client),
        generic_args: vec![],
        associated_type_bindings: vec![],
    })
    .at(span);
    let return_type = (TypeExprKind::Optional {
        inner: Box::new(client_ty),
    })
    .at(span);
    let lambda_span = TextRange::empty(span.end());
    ctx.alloc_expr(
        Expr::Lambda(Box::new(LambdaDef {
            kind: LambdaKind::Anonymous,
            params: vec![param(&prefix_name), param(&model_name)],
            defaults: FunctionDefaults::empty(),
            return_type: Some(return_type),
            throws: None,
            body: Some(body),
            span: lambda_span,
        })),
        lambda_span,
    )
}

/// Synthesize the `@render_prompt` companion body: render the spec's prompt
/// with the return type's output-format handle: `Fn@spec(p...).prompt()`.
pub(crate) fn synthesize_spec_render_prompt_body(
    function_name: &str,
    params: &[Param],
    generic_param_names: &[Name],
    owner_class_name: Option<&Name>,
    owner_generic_param_names: &[Name],
    span: TextRange,
) -> (ExprBody, AstSourceMap) {
    let mut ctx = LoweringContext::new();
    let (spec_callee, binds_receiver) =
        companion_callee(&mut ctx, function_name, params, owner_class_name, span);
    let spec_args = companion_param_call_args(
        &mut ctx,
        if binds_receiver { &params[1..] } else { params },
        span,
    );
    let spec_call = ctx.alloc_expr(
        Expr::Call {
            callee: spec_callee,
            type_args: companion_type_args(
                generic_param_names,
                owner_generic_param_names,
                owner_class_name.is_some() && !binds_receiver,
                span,
            ),
            args: spec_args,
        },
        span,
    );
    let prompt_callee = ctx.alloc_expr(
        Expr::MemberAccess {
            base: spec_call,
            member: Name::new("prompt"),
        },
        span,
    );
    let render_call = ctx.alloc_expr(
        Expr::Call {
            callee: prompt_callee,
            type_args: vec![],
            args: vec![],
        },
        span,
    );

    let (body, source_map, _diags, _env_refs) = ctx.finish(Some(render_call));
    (body, source_map)
}

/// Synthesize the `@build_request` companion body:
/// `Fn@spec(p...).build_request(client = client)`.
pub(crate) fn synthesize_spec_build_request_body(
    function_name: &str,
    params: &[Param],
    generic_param_names: &[Name],
    owner_class_name: Option<&Name>,
    owner_generic_param_names: &[Name],
    span: TextRange,
) -> (ExprBody, AstSourceMap) {
    use crate::ast::CallArg;

    let mut ctx = LoweringContext::new();
    let (spec_callee, binds_receiver) =
        companion_callee(&mut ctx, function_name, params, owner_class_name, span);
    let spec_args = companion_param_call_args(
        &mut ctx,
        if binds_receiver { &params[1..] } else { params },
        span,
    );
    let spec_call = ctx.alloc_expr(
        Expr::Call {
            callee: spec_callee,
            type_args: companion_type_args(
                generic_param_names,
                owner_generic_param_names,
                owner_class_name.is_some() && !binds_receiver,
                span,
            ),
            args: spec_args,
        },
        span,
    );
    let build_callee = ctx.alloc_expr(
        Expr::MemberAccess {
            base: spec_call,
            member: Name::new("build_request"),
        },
        span,
    );
    let client_ref = ctx.alloc_expr(Expr::Path(baml_base::Path::bare(Name::new("client"))), span);
    let call = ctx.alloc_expr(
        Expr::Call {
            callee: build_callee,
            type_args: vec![],
            args: vec![CallArg::named("client", client_ref)],
        },
        span,
    );
    let (body, source_map, _diags, _env_refs) = ctx.finish(Some(call));
    (body, source_map)
}

/// Synthesize the `@parse` companion body: a network-free parse of an
/// existing JSON/SAP string into the function's return type,
/// `baml.sap.parse<Out>(json)`.
pub(crate) fn synthesize_spec_parse_body(
    out_type: Option<crate::ast::TypeExpr>,
    span: TextRange,
) -> (ExprBody, AstSourceMap) {
    use crate::ast::CallArg;

    let mut ctx = LoweringContext::new();
    let json_ref = ctx.alloc_expr(Expr::Path(baml_base::Path::bare(Name::new("json"))), span);
    let callee = ctx.alloc_expr(Expr::Path(baml_base::compiler_path!(baml.sap.parse)), span);
    let call = ctx.alloc_expr(
        Expr::Call {
            callee,
            type_args: out_type.map(|t| vec![t]).unwrap_or_default(),
            args: vec![CallArg::positional(json_ref)],
        },
        span,
    );
    let (body, source_map, _diags, _env_refs) = ctx.finish(Some(call));
    (body, source_map)
}

/// BEP `@spec` spec mode: synthesize the direct-call body of a `tools`-bearing
/// LLM function — run the default ai runner over the function's own spec and
/// unwrap the value:
///
/// ```baml
/// ai.Agent.new(client = client).run(Fn@spec(p1, p2)).value
/// ```
///
/// `client` is the compiler-injected `ai.Client? = null` override parameter;
/// `Agent.run` falls back to the spec's default client when it is null. The
/// Agent class is not generic; `run` infers its `Out` from the spec argument,
/// so the synthesized body names no output type.
pub(crate) fn synthesize_spec_agent_run_body(
    function_name: &str,
    params: &[Param],
    generic_param_names: &[Name],
    owner_class_name: Option<&Name>,
    owner_generic_param_names: &[Name],
    span: TextRange,
) -> (ExprBody, AstSourceMap) {
    let mut ctx = LoweringContext::new();
    let (spec_callee, binds_receiver) =
        companion_callee(&mut ctx, function_name, params, owner_class_name, span);
    let spec_args = companion_param_call_args(
        &mut ctx,
        if binds_receiver { &params[1..] } else { params },
        span,
    );
    let spec_call = ctx.alloc_expr(
        Expr::Call {
            callee: spec_callee,
            type_args: companion_type_args(
                generic_param_names,
                owner_generic_param_names,
                owner_class_name.is_some() && !binds_receiver,
                span,
            ),
            args: spec_args,
        },
        span,
    );

    // ai.Agent.new(client = client, on_event = on_event)
    let agent_path = ctx.alloc_expr(Expr::Path(baml_base::compiler_path!(ai.Agent)), span);
    let new_callee = ctx.alloc_expr(
        Expr::MemberAccess {
            base: agent_path,
            member: Name::new("new"),
        },
        span,
    );
    let client_ref = ctx.alloc_expr(Expr::Path(baml_base::Path::bare(Name::new("client"))), span);
    let on_event_ref = ctx.alloc_expr(
        Expr::Path(baml_base::Path::bare(Name::new("on_event"))),
        span,
    );
    let new_call = ctx.alloc_expr(
        Expr::Call {
            callee: new_callee,
            type_args: vec![],
            args: vec![
                CallArg::named("client", client_ref),
                CallArg::named("on_event", on_event_ref),
            ],
        },
        span,
    );

    // .run(spec).value
    let run_callee = ctx.alloc_expr(
        Expr::MemberAccess {
            base: new_call,
            member: Name::new("run"),
        },
        span,
    );
    let trace_callee = ctx.alloc_expr(
        Expr::Path(baml_base::compiler_path!(trace._passthrough)),
        span,
    );
    let trace = ctx.alloc_expr(
        Expr::Call {
            callee: trace_callee,
            type_args: vec![],
            args: vec![],
        },
        span,
    );
    let run_call = ctx.alloc_expr(
        Expr::Call {
            callee: run_callee,
            type_args: vec![],
            args: vec![
                CallArg::positional(spec_call),
                CallArg::named("$trace", trace),
            ],
        },
        span,
    );
    let value = ctx.alloc_expr(
        Expr::MemberAccess {
            base: run_call,
            member: Name::new("value"),
        },
        span,
    );

    let (body, source_map, _diags, _env_refs) = ctx.finish(Some(value));
    (body, source_map)
}

/// Synthesize the `@stream` companion body — one-turn streaming over the
/// function's own spec:
///
/// ```baml
/// ai.stream.from_spec<Out>(Fn@spec(p1, p2), client = client)
/// ```
///
/// `type_args` is the explicit `<Out>`, so the stdlib reifies the type from
/// its own frame via `reflect.Type.of`. `client` is
/// the companion's injected `ai.stream.StreamingClient? = null` override;
/// `from_spec` falls back to the spec's default client when it is null.
pub(crate) fn synthesize_spec_stream_body(
    function_name: &str,
    params: &[Param],
    generic_param_names: &[Name],
    owner_class_name: Option<&Name>,
    owner_generic_param_names: &[Name],
    type_args: Vec<crate::ast::TypeExpr>,
    span: TextRange,
) -> (ExprBody, AstSourceMap) {
    let mut ctx = LoweringContext::new();
    let (spec_callee, binds_receiver) =
        companion_callee(&mut ctx, function_name, params, owner_class_name, span);
    let spec_args = companion_param_call_args(
        &mut ctx,
        if binds_receiver { &params[1..] } else { params },
        span,
    );
    let spec_call = ctx.alloc_expr(
        Expr::Call {
            callee: spec_callee,
            type_args: companion_type_args(
                generic_param_names,
                owner_generic_param_names,
                owner_class_name.is_some() && !binds_receiver,
                span,
            ),
            args: spec_args,
        },
        span,
    );

    // ai.stream.from_spec<Out>(spec, client = client, on_event = on_event)
    let stream_spec_callee = ctx.alloc_expr(
        Expr::Path(baml_base::compiler_path!(ai.stream.from_spec)),
        span,
    );
    let client_ref = ctx.alloc_expr(Expr::Path(baml_base::Path::bare(Name::new("client"))), span);
    let on_event_ref = ctx.alloc_expr(
        Expr::Path(baml_base::Path::bare(Name::new("on_event"))),
        span,
    );
    let trace_callee = ctx.alloc_expr(
        Expr::Path(baml_base::compiler_path!(trace._passthrough)),
        span,
    );
    let trace = ctx.alloc_expr(
        Expr::Call {
            callee: trace_callee,
            type_args: vec![],
            args: vec![],
        },
        span,
    );
    let call = ctx.alloc_expr(
        Expr::Call {
            callee: stream_spec_callee,
            type_args,
            args: vec![
                CallArg::positional(spec_call),
                CallArg::named("client", client_ref),
                CallArg::named("on_event", on_event_ref),
                CallArg::named("$trace", trace),
            ],
        },
        span,
    );

    let (body, source_map, _diags, _env_refs) = ctx.finish(Some(call));
    (body, source_map)
}

/// Re-apply a companion's enclosing generic parameters when it calls another
/// companion. Some LLM type parameters occur only in the return type, so
/// ordinary argument inference has no value-position evidence for them.
fn companion_type_args(
    function_names: &[Name],
    owner_names: &[Name],
    include_owner: bool,
    span: TextRange,
) -> Vec<TypeExpr> {
    owner_names
        .iter()
        .filter(|_| include_owner)
        .chain(function_names)
        .map(|name| {
            TypeExprKind::Path {
                path: baml_base::Path::bare(name.clone()),
                generic_args: vec![],
                associated_type_bindings: vec![],
            }
            .at(span)
        })
        .collect()
}

/// Select the sibling spec companion using the same receiver shape as a user
/// call. Instance methods bind `self`; static methods use their owning class,
/// named as an item of this namespace (`Path::compiler_item`) so a parameter
/// spelled like the class cannot capture it; free functions remain
/// unqualified.
fn companion_callee(
    ctx: &mut LoweringContext,
    function_name: &str,
    params: &[Param],
    owner_class_name: Option<&Name>,
    span: TextRange,
) -> (ExprId, bool) {
    let companion_name = Name::new(format!("{function_name}@spec"));
    let binds_receiver = owner_class_name.is_some()
        && params
            .first()
            .is_some_and(|param| param.name.as_str() == "self");
    let callee = if binds_receiver {
        let receiver = ctx.alloc_expr(Expr::Path(baml_base::Path::bare(Name::new("self"))), span);
        ctx.alloc_expr(
            Expr::MemberAccess {
                base: receiver,
                member: companion_name,
            },
            span,
        )
    } else if let Some(owner) = owner_class_name {
        ctx.alloc_expr(
            Expr::Path(baml_base::Path::compiler_item(vec![
                owner.clone(),
                companion_name,
            ])),
            span,
        )
    } else {
        ctx.alloc_expr(Expr::Path(baml_base::Path::bare(companion_name)), span)
    };
    (callee, binds_receiver)
}

/// Re-apply parameter defaults when a generated companion calls its `@spec`.
/// Required parameters can stay positional; defaulted parameters are named so
/// the callee's default metadata remains visible to argument lowering.
fn companion_param_call_args(
    ctx: &mut LoweringContext,
    params: &[Param],
    span: TextRange,
) -> Vec<CallArg> {
    params
        .iter()
        .map(|param| {
            let value = ctx.alloc_expr(Expr::Path(baml_base::Path::bare(param.name.clone())), span);
            if param.default.is_some() {
                CallArg::named(param.name.clone(), value)
            } else {
                CallArg::positional(value)
            }
        })
        .collect()
}

pub(crate) struct LoweringContext {
    pub(crate) exprs: Arena<Expr>,
    pub(crate) stmts: Arena<Stmt>,
    pub(crate) patterns: Arena<Pattern>,
    pub(crate) match_arms: Arena<MatchArm>,
    pub(crate) catch_arms: Arena<CatchArm>,
    pub(crate) type_annotations: Arena<TypeExpr>,
    /// Parallel span storage
    pub(crate) source_map: AstSourceMap,
    /// When set, `TEST_EXPR_DEF` and `TESTSET_DEF` nodes encountered during block
    /// lowering are converted to `<var>.register_test(...)` / `<var>.register_test_set(...)`
    /// calls using this variable name. This supports dynamic test generation inside
    /// `for`/`if` blocks inside a testset body.
    pub(crate) testset_collector_var: Option<Name>,
    /// Diagnostics accumulated during lowering.
    pub(crate) diags: Vec<LoweringDiagnostic>,
    /// Environment variable references (`env.X`) found during lowering.
    pub(crate) env_var_refs: Vec<EnvVarRef>,
    /// Expressions that contain unwrapped `?.` operators and need an `OptionalChain` wrapper.
    /// Propagated up through chain-continuing nodes (`FieldAccess`, Index, Call, Optional*).
    pub(crate) needs_chain_wrap: std::collections::HashSet<ExprId>,
    /// When set, every node allocated via `alloc_expr`/`alloc_stmt`/
    /// `alloc_pattern` is recorded as compiler-synthesized in the source map.
    /// Scoped on around desugarings (e.g. backtick-template elaboration) so the
    /// generated nodes are distinguishable from the user-written ones they wrap
    /// — see [`AstSourceMap::synthetic_exprs`]. Consumed by inlay hints.
    pub(crate) synthesizing: bool,
    /// A file-scope binding's introducer keyword, which its caller checks
    /// itself: a global requires `const`. The body-level `const` warning
    /// skips this one token; a nested introducer, such as an alias chain's,
    /// still gets it.
    pub(crate) file_scope_introducer: Option<TextRange>,
    /// See [`ExprBody::tag_values`].
    pub(crate) tag_values: Vec<ExprId>,
}

/// The elaborated form of a single `${…}` interpolation in an untagged
/// backtick template (see [`LoweringContext::elaborate_default_interp`]):
/// either a *value* to concatenate, or — for a side-effect-only `${ let … }` —
/// the raw statements it runs (which yield ""). Returning the statements lets
/// the caller splice them into one enclosing concat scope so a `let` in one
/// segment is visible to later segments (BEP-049 §4 cross-site `let`).
enum InterpPart {
    Value(ExprId),
    Stmts(Vec<StmtId>),
}

impl LoweringContext {
    pub(crate) fn new() -> Self {
        Self {
            exprs: Arena::new(),
            stmts: Arena::new(),
            patterns: Arena::new(),
            match_arms: Arena::new(),
            catch_arms: Arena::new(),
            type_annotations: Arena::new(),
            source_map: AstSourceMap::new(),
            testset_collector_var: None,
            diags: Vec::new(),
            env_var_refs: Vec::new(),
            needs_chain_wrap: std::collections::HashSet::new(),
            synthesizing: false,
            file_scope_introducer: None,
            tag_values: Vec::new(),
        }
    }

    pub(crate) fn warn_const_introducer(&mut self, span: TextRange) {
        self.diags
            .push(LoweringDiagnostic::ConstBindingIntroducer { span });
    }

    pub(crate) fn alloc_expr(&mut self, expr: Expr, range: TextRange) -> ExprId {
        let lambda_parameter_spans = match &expr {
            Expr::Lambda(lambda) => Some(
                lambda
                    .params
                    .iter()
                    .map(|param| param.name_span)
                    .collect::<Vec<_>>(),
            ),
            _ => None,
        };
        let id = self.exprs.alloc(expr);
        self.source_map.expr_spans.alloc(range);
        if let Some(spans) = lambda_parameter_spans {
            self.source_map.lambda_parameter_spans.insert(id, spans);
        }
        if self.synthesizing {
            self.source_map.synthetic_exprs.insert(id);
        }
        id
    }

    pub(crate) fn alloc_stmt(&mut self, stmt: Stmt, range: TextRange) -> StmtId {
        let id = self.stmts.alloc(stmt);
        self.source_map.stmt_spans.alloc(range);
        if self.synthesizing {
            self.source_map.synthetic_stmts.insert(id);
        }
        id
    }

    pub(crate) fn alloc_pattern(&mut self, pattern: Pattern, range: TextRange) -> PatId {
        let id = self.patterns.alloc(pattern);
        self.source_map.pattern_spans.alloc(range);
        if self.synthesizing {
            self.source_map.synthetic_patterns.insert(id);
        }
        id
    }

    pub(crate) fn alloc_match_arm(&mut self, arm: MatchArm, range: TextRange) -> MatchArmId {
        let id = self.match_arms.alloc(arm);
        self.source_map.match_arm_spans.alloc(range);
        id
    }

    pub(crate) fn alloc_catch_arm(&mut self, arm: CatchArm, range: TextRange) -> CatchArmId {
        let id = self.catch_arms.alloc(arm);
        self.source_map.catch_arm_spans.alloc(range);
        id
    }

    pub(crate) fn alloc_type_annot(&mut self, ty: TypeExpr, range: TextRange) -> TypeAnnotId {
        let id = self.type_annotations.alloc(ty);
        self.source_map.type_annotation_spans.alloc(range);
        id
    }

    pub(crate) fn finish(
        self,
        root_expr: Option<ExprId>,
    ) -> (
        ExprBody,
        AstSourceMap,
        Vec<LoweringDiagnostic>,
        Vec<EnvVarRef>,
    ) {
        let body = ExprBody {
            exprs: self.exprs,
            stmts: self.stmts,
            patterns: self.patterns,
            match_arms: self.match_arms,
            catch_arms: self.catch_arms,
            type_annotations: self.type_annotations,
            tag_values: self.tag_values,
            root_expr,
        };
        (body, self.source_map, self.diags, self.env_var_refs)
    }

    /// Whether `expr` denotes a storage location an assignment can write
    /// through. `Missing` counts as assignable so a syntax error already
    /// reported for the target does not cascade into a second diagnostic.
    pub(crate) fn is_assignable_place(&self, expr: ExprId) -> bool {
        match &self.exprs[expr] {
            Expr::Path(_)
            | Expr::MemberAccess { .. }
            | Expr::Index { .. }
            | Expr::OptionalMemberAccess { .. }
            | Expr::OptionalIndex { .. }
            | Expr::Missing => true,
            Expr::OptionalChain { expr } => self.is_assignable_place(*expr),
            _ => false,
        }
    }

    // ============ Pattern lowering ============
    //
    // Structural walk of the new PATTERN CST. Lowers PATTERN/CHAIN/UNION/etc
    // nodes into the flat `Pattern` enum.
    //
    // Invariants enforced here:
    //   - 1-element CHAIN_PATTERN / UNION_PATTERN do not allocate a wrapper;
    //     they collapse to the single inner pattern. (The parser already
    //     refuses to produce them, but the lowering is defensive.)
    //   - `_` (in any position) lowers to `Pattern::Wildcard`, never
    //     `Bind { name: "_" }`.
    //   - PAREN_PATTERN evaporates — it was a parser disambiguator only.
    //   - Field shorthand `{ f }` materialises a `Bind { name: f }` pattern
    //     for `FieldPat.pat`, so consumers never see a missing-pattern shape.

    /// `ai.clients.resolve(selector)` is compiler-assisted: only the compiler
    /// knows every provider (every provider package depends on `ai`, so `ai`
    /// cannot name them), and the calling package reaches them all through
    /// the prelude. So at each call site the compiler appends the provider
    /// table — a constructor lambda and the list of prefixes it knows, both
    /// synthesized from `SHORTHAND_PROVIDERS`, the one table the literal
    /// `client "provider/model"` lowering reads too. A call that already
    /// passes them is left alone.
    // BUG: a WRITTEN call is recognized by its spelling before resolution:
    // `let ai = …; ai.clients.resolve(x)` — a local `ai` whose `clients`
    // field has a `resolve` method — is completed with the provider table
    // too, which breaks that call. The recognition belongs after HIR has read
    // the path's head (`PathHead::Package` of the `ai` package).
    pub(crate) fn complete_client_resolve_call(
        &mut self,
        callee: ExprId,
        mut args: Vec<CallArg>,
        span: TextRange,
    ) -> Vec<CallArg> {
        let is_resolve = matches!(
            &self.exprs[callee],
            Expr::Path(segments)
                if segments.len() == 3
                    && segments[0].as_str() == "ai"
                    && segments[1].as_str() == "clients"
                    && segments[2].as_str() == "resolve"
        );
        if is_resolve && args.len() == 1 {
            let providers = synthesize_shorthand_providers(self, span);
            args.push(CallArg::positional(providers));
            let prefixes = synthesize_shorthand_prefixes(self, span);
            args.push(CallArg::positional(prefixes));
        }
        args
    }

    pub(crate) fn finalize_call_args(
        lowered_args: Vec<(CallArg, Option<TextRange>)>,
    ) -> (Vec<CallArg>, Vec<(ExprId, TextRange)>) {
        let mut args = Vec::with_capacity(lowered_args.len());
        let mut label_spans = Vec::with_capacity(lowered_args.len());

        for (arg, label_span) in lowered_args {
            if let Some(label_span) = label_span {
                label_spans.push((arg.expr, label_span));
            }
            args.push(arg);
        }

        (args, label_spans)
    }

    pub(crate) fn record_call_arg_label_spans(
        &mut self,
        call_id: ExprId,
        label_spans: Vec<(ExprId, TextRange)>,
    ) {
        for (arg_expr, label_span) in label_spans {
            self.source_map
                .call_arg_label_spans
                .insert((call_id, arg_expr), label_span);
        }
    }

    // BUG: `env.X` is recognized by the parser, so a binding named `env` is
    // legal but unusable (`env.name` on a parameter `env: Config` reads the
    // environment); the same holds for a binding named `prompt` before a
    // tagged template. Neither is refused, and nothing says so.
    /// Build the desugared realization of an untagged backtick template: a
    /// left-folded `+` concatenation of the segments, with each `${expr}`
    /// wrapped in an implicit `.to_string()` (BEP §11), `${for}` lowered to an
    /// accumulator block, and `${if}` to a host if-chain. Operates on the
    /// already-lowered [`TemplateSegment`]s (reusing their `ExprId`s /
    /// `PatId`s), so the interpolation expressions are shared with the node's
    /// `segments` rather than re-lowered.
    pub(crate) fn elaborate_default_segments(
        &mut self,
        segments: &[TemplateSegment],
        span: TextRange,
    ) -> ExprId {
        if segments.is_empty() {
            return self.alloc_expr(Expr::Literal(Literal::String(String::new())), span);
        }
        // Lower each segment to either a *value* part (concatenated with `+`)
        // or, for a side-effect-only `${ let … }` interpolation, the raw
        // statements it declares. Hoisting those statements into one enclosing
        // concat scope (below) — rather than re-wrapping each in its own block —
        // is what lets a `let` in one segment be seen by a later `${…}` (BEP-049
        // §4 cross-site `let`), mirroring the single-scope discipline the
        // `${for}` accumulator already uses.
        let mut parts: Vec<InterpPart> = Vec::with_capacity(segments.len());
        let mut any_stmts = false;
        for seg in segments {
            let part = match seg {
                TemplateSegment::Text(s) => InterpPart::Value(
                    self.alloc_expr(Expr::Literal(Literal::String(s.clone())), span),
                ),
                TemplateSegment::Interp(e) => {
                    InterpPart::Value(self.elaborate_default_interp(*e, span))
                }
                // Spliced into the one enclosing concat scope rather than
                // wrapped in a block of their own, which is what lets a `let`
                // here be seen by a later `${…}` (BEP-049 §4 cross-site `let`).
                TemplateSegment::Effect { stmts, .. } => InterpPart::Stmts(stmts.clone()),
                TemplateSegment::For {
                    binding,
                    collection,
                    body,
                } => {
                    InterpPart::Value(self.elaborate_default_for(*binding, *collection, body, span))
                }
                TemplateSegment::CStyleFor {
                    init,
                    cond,
                    step,
                    body,
                } => InterpPart::Value(
                    self.elaborate_default_cstyle_for(*init, *cond, *step, body, span),
                ),
                TemplateSegment::If {
                    branches,
                    else_body,
                } => InterpPart::Value(self.elaborate_default_if(
                    branches,
                    else_body.as_deref(),
                    span,
                )),
            };
            if matches!(part, InterpPart::Stmts(_)) {
                any_stmts = true;
            }
            parts.push(part);
        }

        // Fast path: no side-effect-only segment, so there is nothing to hoist.
        // Keep the plain `+` fold — this preserves `Ty::Literal` for a constant
        // template (`` `abc` `` infers `Ty::Literal("abc")` so BEP §9 constant
        // folding still fires).
        if !any_stmts {
            let mut iter = parts.into_iter().map(|p| match p {
                InterpPart::Value(v) => v,
                InterpPart::Stmts(_) => unreachable!("no Stmts parts when !any_stmts"),
            });
            let first = iter.next().expect("non-empty by guard above");
            return iter.fold(first, |acc, next| {
                self.alloc_expr(
                    Expr::Binary {
                        op: BinaryOp::Add,
                        lhs: acc,
                        rhs: next,
                    },
                    span,
                )
            });
        }

        // Hoisting path: build `{ let acc = ""; <…>; acc }` where each value
        // part appends `acc = acc + part` and each side-effect-only segment
        // splices its statements in directly, so a `let` it binds stays visible
        // to every later segment. The accumulator name leads with a space so it
        // is unparseable as a user identifier and can never collide; name
        // resolution is plain string equality, so the synthesized references
        // still resolve to it.
        let after_span = TextRange::empty(span.end());
        let acc_name = Name::new(" __m3_concat");
        let acc_pat = self.alloc_pattern(
            Pattern::Bind {
                name: acc_name.clone(),
                subpat: None,
            },
            span,
        );
        let empty_init = self.alloc_expr(Expr::Literal(Literal::String(String::new())), span);
        let acc_let = self.alloc_stmt(
            Stmt::Let {
                pattern: acc_pat,
                initializer: Some(empty_init),
                origin: LetOrigin::Source,
                is_const: false,
                else_branch: None,
            },
            span,
        );
        let mut block_stmts: Vec<StmtId> = vec![acc_let];
        for part in parts {
            match part {
                InterpPart::Value(value) => {
                    let acc_lhs = self.alloc_expr(
                        Expr::Path(baml_base::Path::bare(acc_name.clone())),
                        after_span,
                    );
                    let acc_rhs = self.alloc_expr(
                        Expr::Path(baml_base::Path::bare(acc_name.clone())),
                        after_span,
                    );
                    let concat = self.alloc_expr(
                        Expr::Binary {
                            op: BinaryOp::Add,
                            lhs: acc_rhs,
                            rhs: value,
                        },
                        after_span,
                    );
                    let assign = self.alloc_stmt(
                        Stmt::Assign {
                            target: acc_lhs,
                            value: concat,
                        },
                        after_span,
                    );
                    block_stmts.push(assign);
                }
                InterpPart::Stmts(stmts) => block_stmts.extend(stmts),
            }
        }
        let acc_tail = self.alloc_expr(Expr::Path(baml_base::Path::bare(acc_name)), after_span);
        self.alloc_expr(
            Expr::Block {
                stmts: block_stmts,
                tail_expr: Some(acc_tail),
            },
            span,
        )
    }

    /// Elaborate a `${expr}` for the untagged path: wrap the inner block with
    /// `.to_string()` (BEP §11).
    fn elaborate_default_interp(&mut self, inner: ExprId, span: TextRange) -> ExprId {
        // Render the value via `baml.String.from(...)` — BAML's universal
        // renderer (BEP-049 §11), the primitive's companion named by identity
        // so a binding called `string` in scope does not capture it. It
        // dispatches `to_string` on the value's runtime class when that class
        // implements `baml.ToString`, otherwise falls back to a structural
        // rendering, so any `${expr}` renders without the type having to opt
        // into the interface.
        let callee = self.alloc_expr(
            Expr::Path(baml_base::compiler_path!(baml.String.from)),
            span,
        );
        self.alloc_expr(
            Expr::Call {
                callee,
                type_args: Vec::new(),
                args: vec![CallArg::positional(inner)],
            },
            span,
        )
    }

    /// The statements and tail of a `${…}` block that produces no value: it
    /// has no tail, or its tail is a syntactically-unit expression. The
    /// syntax decides which way an interpolation is read (BEP-049 §4); the
    /// checker then holds a unit one to the unit type.
    fn unit_interp(&self, inner: ExprId) -> Option<(&[StmtId], Option<ExprId>)> {
        match &self.exprs[inner] {
            Expr::Block { stmts, tail_expr } if tail_expr.is_none_or(|t| self.is_unit_tail(t)) => {
                Some((stmts, *tail_expr))
            }
            _ => None,
        }
    }

    /// The statements of a `${…}` block that produces no value. The block
    /// still runs for its effects — its tail too — so these are everything
    /// it does. Its tail is checked against the unit type rather than
    /// discarded: an `if` with no `else` whose branch produces a value is an
    /// error, not text that silently goes missing. `None` for an
    /// interpolation that has a value.
    pub(crate) fn unit_interp_stmts(
        &mut self,
        inner: ExprId,
        span: TextRange,
    ) -> Option<Vec<StmtId>> {
        let (stmts, tail) = self.unit_interp(inner)?;
        let mut stmts = stmts.to_vec();
        if let Some(t) = tail {
            // The check is the compiler's, not something the user wrote.
            let prev_synth = std::mem::replace(&mut self.synthesizing, true);
            stmts.push(self.unit_let(t, span));
            self.synthesizing = prev_synth;
        }
        Some(stmts)
    }

    /// `let _: null = <value>;`, binding a name no user identifier can
    /// spell: `value` is checked against the unit type and dropped.
    fn unit_let(&mut self, value: ExprId, span: TextRange) -> StmtId {
        let null = self.alloc_pattern(Pattern::Type(TypeExprKind::Null.at(span)), span);
        let pattern = self.alloc_pattern(
            Pattern::Bind {
                name: Name::new(" __unit"),
                subpat: Some(null),
            },
            span,
        );
        self.alloc_stmt(
            Stmt::Let {
                pattern,
                initializer: Some(value),
                origin: LetOrigin::Source,
                is_const: false,
                else_branch: None,
            },
            span,
        )
    }

    /// Is `expr` a syntactically unit-valued expression when it sits in a
    /// block's tail position? Only the implicit-unit forms qualify: an
    /// `if`/`if let` with no `else` branch (which evaluates to `null`), an
    /// `else if` chain that ends in one, or a nested block whose own tail is
    /// unit. `while`/`for`/assignment/`let`/`return`/`throw`/`break`/
    /// `continue` are lowered as `Stmt`s (never a `tail_expr`), so they're
    /// already covered by the no-tail case and need not appear here.
    fn is_unit_tail(&self, expr: ExprId) -> bool {
        match &self.exprs[expr] {
            Expr::If { else_branch, .. } | Expr::IfLet { else_branch, .. } => else_branch
                .is_none_or(|else_branch| {
                    matches!(
                        self.exprs[else_branch],
                        Expr::If { .. } | Expr::IfLet { .. }
                    ) && self.is_unit_tail(else_branch)
                }),
            Expr::Block { tail_expr, .. } => {
                tail_expr.map(|t| self.is_unit_tail(t)).unwrap_or(true)
            }
            _ => false,
        }
    }

    /// Elaborate a `${for (p in c)}…${endfor}` to an accumulator block:
    /// `{ let acc = ""; for (p in c) { acc = acc + <body>; } acc }`.
    fn elaborate_default_for(
        &mut self,
        binding: PatId,
        collection: ExprId,
        body: &[TemplateSegment],
        span: TextRange,
    ) -> ExprId {
        let body_string = self.elaborate_default_segments(body, span);

        // Accumulator binding. The leading space makes the name unparseable as
        // a user identifier, so it can never collide with a user binding; name
        // resolution is plain string equality, so the synthesized references
        // below still resolve to it. References sit at an empty range after the
        // template so they land inside the let binding's visibility window.
        let after_span = TextRange::empty(span.end());
        let acc_name = Name::new(" __m3_for");
        let acc_pat = self.alloc_pattern(
            Pattern::Bind {
                name: acc_name.clone(),
                subpat: None,
            },
            span,
        );
        let empty_init = self.alloc_expr(Expr::Literal(Literal::String(String::new())), span);
        let let_stmt = self.alloc_stmt(
            Stmt::Let {
                pattern: acc_pat,
                initializer: Some(empty_init),
                origin: LetOrigin::Source,
                is_const: false,
                else_branch: None,
            },
            span,
        );

        let acc_path_lhs = self.alloc_expr(
            Expr::Path(baml_base::Path::bare(acc_name.clone())),
            after_span,
        );
        let acc_path_rhs = self.alloc_expr(
            Expr::Path(baml_base::Path::bare(acc_name.clone())),
            after_span,
        );
        let concat = self.alloc_expr(
            Expr::Binary {
                op: BinaryOp::Add,
                lhs: acc_path_rhs,
                rhs: body_string,
            },
            after_span,
        );
        let assign_stmt = self.alloc_stmt(
            Stmt::Assign {
                target: acc_path_lhs,
                value: concat,
            },
            after_span,
        );
        let loop_body = self.alloc_expr(
            Expr::Block {
                stmts: vec![assign_stmt],
                tail_expr: None,
            },
            after_span,
        );
        let for_stmt = self.alloc_stmt(
            Stmt::For {
                binding,
                collection,
                body: loop_body,
            },
            after_span,
        );
        let acc_tail = self.alloc_expr(Expr::Path(baml_base::Path::bare(acc_name)), after_span);
        self.alloc_expr(
            Expr::Block {
                stmts: vec![let_stmt, for_stmt],
                tail_expr: Some(acc_tail),
            },
            span,
        )
    }

    /// Elaborate a C-style `${for (let i = 0; cond; step)}…${endfor}` to an
    /// accumulator block, mirroring [`Self::elaborate_default_for`] but with the
    /// host C-style loop shape: `{ let acc = ""; let i = 0; while cond { acc =
    /// acc + <body>; } after { step }; acc }`. The `init` `let` declares the
    /// loop variable in scope for `cond`/body/`step` (same as `lower_c_style_for`).
    fn elaborate_default_cstyle_for(
        &mut self,
        init: StmtId,
        cond: ExprId,
        step: Option<StmtId>,
        body: &[TemplateSegment],
        span: TextRange,
    ) -> ExprId {
        let body_string = self.elaborate_default_segments(body, span);

        let after_span = TextRange::empty(span.end());
        let acc_name = Name::new(" __m3_for");
        let acc_pat = self.alloc_pattern(
            Pattern::Bind {
                name: acc_name.clone(),
                subpat: None,
            },
            span,
        );
        let empty_init = self.alloc_expr(Expr::Literal(Literal::String(String::new())), span);
        let acc_let = self.alloc_stmt(
            Stmt::Let {
                pattern: acc_pat,
                initializer: Some(empty_init),
                origin: LetOrigin::Source,
                is_const: false,
                else_branch: None,
            },
            span,
        );

        let acc_path_lhs = self.alloc_expr(
            Expr::Path(baml_base::Path::bare(acc_name.clone())),
            after_span,
        );
        let acc_path_rhs = self.alloc_expr(
            Expr::Path(baml_base::Path::bare(acc_name.clone())),
            after_span,
        );
        let concat = self.alloc_expr(
            Expr::Binary {
                op: BinaryOp::Add,
                lhs: acc_path_rhs,
                rhs: body_string,
            },
            after_span,
        );
        let assign_stmt = self.alloc_stmt(
            Stmt::Assign {
                target: acc_path_lhs,
                value: concat,
            },
            after_span,
        );
        let loop_body = self.alloc_expr(
            Expr::Block {
                stmts: vec![assign_stmt],
                tail_expr: None,
            },
            after_span,
        );
        let while_stmt = self.alloc_stmt(
            Stmt::While {
                condition: cond,
                body: loop_body,
                after: step,
                origin: LoopOrigin::For { init },
            },
            after_span,
        );
        let acc_tail = self.alloc_expr(Expr::Path(baml_base::Path::bare(acc_name)), after_span);
        self.alloc_expr(
            Expr::Block {
                stmts: vec![acc_let, init, while_stmt],
                tail_expr: Some(acc_tail),
            },
            span,
        )
    }

    /// Elaborate a `${if (c)}…${else if}…${else}…${endif}` chain to a host
    /// if-expression whose branch bodies are the concat of their segments.
    fn elaborate_default_if(
        &mut self,
        branches: &[TemplateIfBranch],
        else_body: Option<&[TemplateSegment]>,
        span: TextRange,
    ) -> ExprId {
        let mut current_else = match else_body {
            Some(b) => self.elaborate_default_segments(b, span),
            None => self.alloc_expr(Expr::Literal(Literal::String(String::new())), span),
        };
        for branch in branches.iter().rev() {
            let then_branch = self.elaborate_default_segments(&branch.body, span);
            current_else = self.alloc_expr(
                Expr::If {
                    condition: branch.condition,
                    then_branch,
                    else_branch: Some(current_else),
                },
                span,
            );
        }
        current_else
    }

    /// Build the desugared body of a tagged template — the closure the tag is
    /// invoked with. Produces a block that flattens the segments into
    /// `baml.TaggedString { parts, values }` (BEP §10): text runs accumulate
    /// into a `cur` string flushed into `parts` at each interpolation, each
    /// `${expr}` is pushed *raw* (uncoerced — §11) into `values`, and
    /// `${for}`/`${if}` drive runtime growth via real loops/branches.
    ///
    /// Built from the already-lowered [`TemplateSegment`]s (reusing their
    /// `ExprId`s/`PatId`s for interps, for-bindings, collections, conditions).
    /// The `parts`/`values`/`cur` synthetic locals use leading-space names so
    /// they can never collide with user identifiers; their references sit at an
    /// empty range after the template so they land inside each `let`'s
    /// visibility window (mirrors the untagged accumulator in
    /// `elaborate_default_for`).
    pub(crate) fn elaborate_tagged_body(
        &mut self,
        segments: &[TemplateSegment],
        span: TextRange,
    ) -> ExprId {
        let parts = Name::new(" __tt_parts");
        let values = Name::new(" __tt_values");
        let cur = Name::new(" __tt_cur");

        let mut stmts: Vec<StmtId> = Vec::new();
        // let __tt_parts: string[] = [];
        stmts.push(self.tt_let_typed_empty_list(&parts, TypeExprKind::String.at(span), span));
        // let __tt_values: unknown[] = [];
        stmts.push(self.tt_let_typed_empty_list(&values, TypeExprKind::Unknown.at(span), span));
        // let __tt_cur = "";
        let at = TextRange::empty(span.start());
        let cur_init = self.alloc_expr(Expr::Literal(Literal::String(String::new())), at);
        let cur_pat = self.alloc_pattern(
            Pattern::Bind {
                name: cur.clone(),
                subpat: None,
            },
            at,
        );
        stmts.push(self.alloc_stmt(
            Stmt::Let {
                pattern: cur_pat,
                initializer: Some(cur_init),
                origin: LetOrigin::Source,
                is_const: false,
                else_branch: None,
            },
            at,
        ));

        self.elaborate_tagged_walk(segments, &parts, &values, &cur, &mut stmts, span);

        // __tt_parts.push(__tt_cur);  — flush the trailing text run.
        let cur_ref = self.tt_path(&cur, span);
        stmts.push(self.tt_push_stmt(&parts, cur_ref, span));

        // baml.TaggedString { parts: __tt_parts, values: __tt_values }
        let parts_ref = self.tt_path(&parts, span);
        let values_ref = self.tt_path(&values, span);
        let tail = self.alloc_expr(
            Expr::Object {
                type_name: baml_base::compiler_path!(baml.TaggedString),
                type_args: Vec::new(),
                fields: vec![
                    ObjectExprField::explicit(Name::new("parts"), parts_ref),
                    ObjectExprField::explicit(Name::new("values"), values_ref),
                ],
                spreads: Vec::new(),
            },
            span,
        );

        self.alloc_expr(
            Expr::Block {
                stmts,
                tail_expr: Some(tail),
            },
            span,
        )
    }

    /// Emit the per-segment flatten statements into `stmts`. Recurses through
    /// `${for}` bodies and `${if}` branches, threading the same accumulator
    /// locals so the resulting `(parts, values)` honour the alternating
    /// `parts.len() == values.len() + 1` invariant across data-dependent
    /// lengths.
    fn elaborate_tagged_walk(
        &mut self,
        segments: &[TemplateSegment],
        parts: &Name,
        values: &Name,
        cur: &Name,
        stmts: &mut Vec<StmtId>,
        span: TextRange,
    ) {
        for seg in segments {
            match seg {
                TemplateSegment::Text(s) => {
                    // __tt_cur = __tt_cur + "<text>";
                    let lhs = self.tt_path(cur, span);
                    let rhs = self.alloc_expr(Expr::Literal(Literal::String(s.clone())), span);
                    let concat = self.alloc_expr(
                        Expr::Binary {
                            op: BinaryOp::Add,
                            lhs,
                            rhs,
                        },
                        span,
                    );
                    stmts.push(self.tt_assign(cur, concat, span));
                }
                TemplateSegment::Interp(e) => {
                    // __tt_parts.push(__tt_cur); __tt_cur = ""; __tt_values.push(<e>);
                    let cur_ref = self.tt_path(cur, span);
                    stmts.push(self.tt_push_stmt(parts, cur_ref, span));
                    let empty =
                        self.alloc_expr(Expr::Literal(Literal::String(String::new())), span);
                    stmts.push(self.tt_assign(cur, empty, span));
                    stmts.push(self.tt_push_stmt(values, *e, span));
                    self.tag_values.push(*e);
                }
                // No value to hand the tag: the text on either side joins.
                TemplateSegment::Effect { stmts: effect, .. } => {
                    stmts.extend(effect.iter().copied());
                }
                TemplateSegment::For {
                    binding,
                    collection,
                    body,
                } => {
                    // for (let p in c) { <walk body> }
                    let mut inner: Vec<StmtId> = Vec::new();
                    self.elaborate_tagged_walk(body, parts, values, cur, &mut inner, span);
                    let loop_body = self.alloc_expr(
                        Expr::Block {
                            stmts: inner,
                            tail_expr: None,
                        },
                        span,
                    );
                    stmts.push(self.alloc_stmt(
                        Stmt::For {
                            binding: *binding,
                            collection: *collection,
                            body: loop_body,
                        },
                        span,
                    ));
                }
                TemplateSegment::CStyleFor {
                    init,
                    cond,
                    step,
                    body,
                } => {
                    // { let i = 0; while cond { <walk body> } after { step } }
                    let mut inner: Vec<StmtId> = Vec::new();
                    self.elaborate_tagged_walk(body, parts, values, cur, &mut inner, span);
                    let loop_body = self.alloc_expr(
                        Expr::Block {
                            stmts: inner,
                            tail_expr: None,
                        },
                        span,
                    );
                    let while_stmt = self.alloc_stmt(
                        Stmt::While {
                            condition: *cond,
                            body: loop_body,
                            after: *step,
                            origin: LoopOrigin::For { init: *init },
                        },
                        span,
                    );
                    // Wrap `init` + `while` in a block so the loop variable is
                    // scoped to the loop, not the surrounding flatten body.
                    let block = self.alloc_expr(
                        Expr::Block {
                            stmts: vec![*init, while_stmt],
                            tail_expr: None,
                        },
                        span,
                    );
                    stmts.push(self.alloc_stmt(Stmt::Expr(block), span));
                }
                TemplateSegment::If {
                    branches,
                    else_body,
                } => {
                    // Build the if/else-if chain inside-out, each branch body a
                    // block of the walked statements (unit-valued).
                    let mut current_else: Option<ExprId> = else_body.as_deref().map(|eb| {
                        let mut s: Vec<StmtId> = Vec::new();
                        self.elaborate_tagged_walk(eb, parts, values, cur, &mut s, span);
                        self.alloc_expr(
                            Expr::Block {
                                stmts: s,
                                tail_expr: None,
                            },
                            span,
                        )
                    });
                    for branch in branches.iter().rev() {
                        let mut s: Vec<StmtId> = Vec::new();
                        self.elaborate_tagged_walk(&branch.body, parts, values, cur, &mut s, span);
                        let then_branch = self.alloc_expr(
                            Expr::Block {
                                stmts: s,
                                tail_expr: None,
                            },
                            span,
                        );
                        let if_expr = self.alloc_expr(
                            Expr::If {
                                condition: branch.condition,
                                then_branch,
                                else_branch: current_else,
                            },
                            span,
                        );
                        current_else = Some(if_expr);
                    }
                    if let Some(if_expr) = current_else {
                        stmts.push(self.alloc_stmt(Stmt::Expr(if_expr), span));
                    }
                }
            }
        }
    }

    /// `let <name>: <elem>[] = []` with a leading-space (non-collidable) name.
    fn tt_let_typed_empty_list(&mut self, name: &Name, elem: TypeExpr, span: TextRange) -> StmtId {
        // Anchor the binding at the template start so its visibility window
        // (`visible_from == let.span.end`) begins at `span.start`, letting the
        // start-anchored accumulator references (see `tt_path`) resolve to it.
        let at = TextRange::empty(span.start());
        let list_ty = TypeExprKind::List {
            inner: Box::new(elem),
        }
        .at(span);
        let type_pat = self.alloc_pattern(Pattern::Type(list_ty), at);
        let pat = self.alloc_pattern(
            Pattern::Bind {
                name: name.clone(),
                subpat: Some(type_pat),
            },
            at,
        );
        let empty = self.alloc_expr(
            Expr::Array {
                elements: Vec::new(),
            },
            at,
        );
        self.alloc_stmt(
            Stmt::Let {
                pattern: pat,
                initializer: Some(empty),
                origin: LetOrigin::Source,
                is_const: false,
                else_branch: None,
            },
            at,
        )
    }

    /// A `Path` reference to a synthetic accumulator local. Anchored at an
    /// empty range at the *start* of the template — inside the closure's
    /// `ScopeKind::Lambda` range `[span.start, span.end)` (so it resolves) and
    /// `>=` each accumulator `let`'s visibility (also anchored at the start).
    /// `span.end` would be the exclusive scope boundary → out of scope.
    fn tt_path(&mut self, name: &Name, span: TextRange) -> ExprId {
        let at = TextRange::empty(span.start());
        self.alloc_expr(Expr::Path(baml_base::Path::bare(name.clone())), at)
    }

    /// `<name> = <value>;`
    fn tt_assign(&mut self, name: &Name, value: ExprId, span: TextRange) -> StmtId {
        let at = TextRange::empty(span.start());
        let target = self.alloc_expr(Expr::Path(baml_base::Path::bare(name.clone())), at);
        self.alloc_stmt(Stmt::Assign { target, value }, at)
    }

    /// `<name>.push(<arg>);` as a statement.
    fn tt_push_stmt(&mut self, name: &Name, arg: ExprId, span: TextRange) -> StmtId {
        let at = TextRange::empty(span.start());
        let recv = self.alloc_expr(Expr::Path(baml_base::Path::bare(name.clone())), at);
        let callee = self.alloc_expr(
            Expr::MemberAccess {
                base: recv,
                member: Name::new("push"),
            },
            at,
        );
        let call = self.alloc_expr(
            Expr::Call {
                callee,
                type_args: Vec::new(),
                args: vec![CallArg::positional(arg)],
            },
            at,
        );
        self.alloc_stmt(Stmt::Expr(call), at)
    }
}

/// Strip string delimiters from raw token text, decoding escape sequences.
pub(crate) fn strip_string_delimiters(text: &str) -> String {
    let text = text.trim();

    if text.starts_with('"') && text.ends_with('"') && text.len() >= 2 {
        crate::unescape_string_literal(&text[1..text.len() - 1])
    } else {
        text.to_string()
    }
}

/// The verbatim body of a raw `r"..."` string literal's text, or `None` if
/// the literal is unclosed (which is already a parse error).
pub(crate) fn strip_raw_string_delimiters(text: &str) -> Option<&str> {
    text.trim().strip_prefix("r\"")?.strip_suffix('"')
}
