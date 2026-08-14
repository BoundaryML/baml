//! CST `ExprFunctionBody` → `(ExprBody, AstSourceMap)`.
//!
//! Adapts the `LoweringContext` from `baml_compiler_hir/src/body.rs` which creates arenas,
//! walks block expressions, etc. Produces `ExprBody` (semantic data) and `AstSourceMap`
//! (parallel span storage) in one pass.

use baml_base::{Name, TypePath, num_lit};
use baml_compiler_syntax::{SyntaxKind, SyntaxNode, SyntaxNodeExt, SyntaxToken};
use la_arena::Arena;
use rowan::ast::AstNode;
use text_size::TextRange;

use crate::{
    LoweringDiagnostic,
    ast::{
        ArrayRestPat, AssignOp, AstSourceMap, BinaryOp, CallArg, CatchArm, CatchArmId, CatchClause,
        CatchClauseKind, DefaultExprId, Expr, ExprBody, ExprId, FieldPat, FunctionDefaults,
        LambdaDef, LambdaKind, LetOrigin, Literal, LoopOrigin, MapExprEntry, MatchArm, MatchArmId,
        ObjectExprField, Param, PatId, Pattern, SpreadField, Stmt, StmtId, TemplateIfBranch,
        TemplateSegment, TemplateTag, TypeAnnotId, TypeExpr, TypeExprKind, UnaryOp,
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

/// Returns true if `kind` can serve as an identifier token in expression position.
///
/// The parser allows `KW_CLIENT` (and `WORD`) inside `PATH_EXPR` / `FIELD_ACCESS_EXPR`
/// nodes when `client` is used as a variable or field name. It likewise allows
/// `KW_SPAWN` / `KW_AWAIT` as path segments (e.g. the `baml.spawn` namespace),
/// since they are unambiguous after a `.`, and the interface-related keywords
/// (`implements`, `interface`, `extends`) as member names — e.g.
/// `dog_t.implements(animal_t)` on the reflection `type` value. This must
/// match exactly what `parse_path_or_ident` / `at_member_name` accept in the
/// parser; adding a new keyword there requires adding it here too.
pub(crate) fn is_ident_token(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::WORD
            | SyntaxKind::KW_CLIENT
            | SyntaxKind::KW_SPAWN
            | SyntaxKind::KW_AWAIT
            | SyntaxKind::KW_IMPLEMENTS
            | SyntaxKind::KW_IMPLEMENT
            | SyntaxKind::KW_INTERFACE
            | SyntaxKind::KW_EXTENDS
            | SyntaxKind::KW_REQUIRES
            // Contextual keywords re-lexed from a `Word`: still lower by text
            // (the literal/identifier arms below switch on the text), so they
            // must read as ident tokens just as they did when they were `Word`.
            | SyntaxKind::KW_AS
            | SyntaxKind::KW_TYPE
            | SyntaxKind::KW_TRUE
            | SyntaxKind::KW_FALSE
            | SyntaxKind::KW_NULL
    )
}

/// Locate the `GENERIC_ARGS` node that should be treated as the *call-site*
/// type-args for a `CALL_EXPR` whose callee is `callee_node`.
///
/// Direct child case (`foo<T>(args)`): the `GENERIC_ARGS` sits inside the
/// callee `PATH_EXPR`. For static-method-on-generic-class calls
/// (`Box<Secret>.from_json(j)`) the parser emits a `FIELD_ACCESS_EXPR` whose
/// base `PATH_EXPR` carries the receiver's `GENERIC_ARGS` — that's also a
/// call-site type-arg from a semantic standpoint, so we walk into the base.
///
/// We don't merge `GENERIC_ARGS` from both positions: if both are present
/// (e.g. `Container<int>.method<U>(args)`) the *method-level* args (direct
/// child) win, since they're attached to the call itself.
fn find_callee_generic_args(callee_node: &SyntaxNode) -> Option<SyntaxNode> {
    if let Some(args) = callee_node
        .children()
        .find(|n| n.kind() == SyntaxKind::GENERIC_ARGS)
    {
        return Some(args);
    }
    match callee_node.kind() {
        SyntaxKind::FIELD_ACCESS_EXPR | SyntaxKind::OPTIONAL_FIELD_ACCESS_EXPR => {
            // Base is the first child node — it carries the receiver type's
            // `GENERIC_ARGS` for `<Type<...>>.method(args)` shape.
            let base = callee_node.children().next()?;
            if base.kind() == SyntaxKind::UPCAST_EXPR {
                return None;
            }
            find_callee_generic_args(&base)
        }
        _ => None,
    }
}

/// Lower a CST `ExprFunctionBody` to an owned `ExprBody` + parallel `AstSourceMap`.
pub(crate) fn lower(
    expr_body: &baml_compiler_syntax::ast::ExprFunctionBody,
    diags: &mut Vec<LoweringDiagnostic>,
    env_var_refs: &mut Vec<EnvVarRef>,
) -> (ExprBody, AstSourceMap) {
    let mut ctx = LoweringContext::new();

    // The EXPR_FUNCTION_BODY contains a BLOCK_EXPR as its child
    let root_expr = expr_body
        .syntax()
        .children()
        .find_map(baml_compiler_syntax::ast::BlockExpr::cast)
        .map(|block| ctx.lower_block_expr(&block));

    let (body, source_map, ctx_diags, ctx_env_refs) = ctx.finish(root_expr);
    diags.extend(ctx_diags);
    env_var_refs.extend(ctx_env_refs);
    (body, source_map)
}

pub(crate) fn lower_default_expr_nodes(
    defaults: &[(usize, baml_compiler_syntax::SyntaxElement)],
    diags: &mut Vec<LoweringDiagnostic>,
    env_var_refs: &mut Vec<EnvVarRef>,
) -> (FunctionDefaults, Vec<(usize, DefaultExprId)>) {
    let mut ctx = LoweringContext::new();

    let mut lowered = Vec::with_capacity(defaults.len());
    for (idx, element) in defaults {
        let expr = match element {
            rowan::NodeOrToken::Node(node) => ctx.lower_expr(node),
            rowan::NodeOrToken::Token(token) => {
                let expr = lower_bare_token_expr(&mut ctx, token);
                ctx.alloc_expr(expr, token.text_range())
            }
        };
        lowered.push((*idx, DefaultExprId::new(expr)));
    }

    let (exprs, source_map, ctx_diags, ctx_env_refs) = ctx.finish(None);
    diags.extend(ctx_diags);
    env_var_refs.extend(ctx_env_refs);
    (FunctionDefaults { exprs, source_map }, lowered)
}

/// Lower a runner `SyntaxElement` (node or token) into an `ExprId` within the given context.
///
/// If the element is a node (e.g. `CALL_EXPR`, `OBJECT_LITERAL`), delegates to `lower_expr`.
/// If the element is a bare token (e.g. `INTEGER_LITERAL`, `WORD`), lowers inline.
/// Lower a bare token (not wrapped in a CST node) into an `Expr`.
/// Used for runner expressions that are simple literals or identifiers.
fn lower_bare_token_expr(ctx: &mut LoweringContext, token: &SyntaxToken) -> Expr {
    match token.kind() {
        SyntaxKind::BIGINT_LITERAL => {
            Expr::Literal(Literal::Bigint(ctx.bigint_literal_value(token)))
        }
        SyntaxKind::INTEGER_LITERAL => Expr::Literal(Literal::Int(ctx.int_literal_value(token))),
        SyntaxKind::FLOAT_LITERAL => Expr::Literal(Literal::Float(
            num_lit::normalize_float_literal(token.text()),
        )),
        k if is_ident_token(k) => match token.text() {
            "null" => Expr::Null,
            "true" => Expr::Literal(Literal::Bool(true)),
            "false" => Expr::Literal(Literal::Bool(false)),
            text => Expr::Path(vec![Name::new(text)]),
        },
        _ => Expr::Missing,
    }
}

pub(crate) fn lower_runner_element(
    ctx: &mut InitTestContext,
    element: &baml_compiler_syntax::SyntaxElement,
) -> ExprId {
    let span = element.text_range();
    match element {
        rowan::NodeOrToken::Node(node) => ctx.inner.lower_expr(node),
        rowan::NodeOrToken::Token(token) => {
            let expr = lower_bare_token_expr(&mut ctx.inner, token);
            ctx.inner.alloc_expr(expr, span)
        }
    }
}

/// Context for building the `$init_test` function body.
///
/// Wraps a `LoweringContext` so that runner expressions can be lowered
/// directly into the same arena (no IIFE indirection needed).
pub(crate) struct InitTestContext {
    inner: LoweringContext,
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

    /// Lower a top-level `test`'s body into the `$init_test` arena, as the body
    /// of the lambda that gets registered.
    ///
    /// The nodes carry their real source offsets even though `$init_test`'s own
    /// synthesized nodes carry empty ranges — both live in one source map, and
    /// HIR resolves names inside the body by offset.
    pub(crate) fn lower_test_body(&mut self, block_node: &SyntaxNode, span: TextRange) -> ExprId {
        match baml_compiler_syntax::ast::BlockExpr::cast(block_node.clone()) {
            Some(block) => self.inner.lower_lambda_body(&block),
            None => self.inner.alloc_expr(Expr::Null, span),
        }
    }

    /// Lower a top-level `testset`'s body into the `$init_test` arena, as the
    /// body of the collector lambda that gets registered.
    pub(crate) fn lower_testset_body(
        &mut self,
        block_node: &SyntaxNode,
        collector: Name,
        span: TextRange,
    ) -> ExprId {
        match baml_compiler_syntax::ast::BlockExpr::cast(block_node.clone()) {
            Some(block) => {
                self.inner
                    .lower_testset_collector_body(&block, collector, block_node.span_range())
            }
            None => self.inner.alloc_expr(Expr::Null, span),
        }
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

/// Lower a `client Name = <expr>;` initializer into its own `ExprBody`.
/// The value may be a node or a bare identifier/literal token.
pub(crate) fn lower_client_initializer(
    value: Option<&rowan::NodeOrToken<SyntaxNode, baml_compiler_syntax::SyntaxToken>>,
    span: TextRange,
    diags: &mut Vec<LoweringDiagnostic>,
    env_var_refs: &mut Vec<EnvVarRef>,
) -> (ExprBody, AstSourceMap) {
    let mut ctx = LoweringContext::new();
    let root = match value {
        Some(rowan::NodeOrToken::Node(node)) => ctx.lower_expr(node),
        Some(rowan::NodeOrToken::Token(token)) => ctx
            .try_lower_bare_token(token)
            .unwrap_or_else(|| ctx.alloc_expr(Expr::Missing, span)),
        None => ctx.alloc_expr(Expr::Missing, span),
    };
    let (body, source_map, ctx_diags, ctx_env_refs) = ctx.finish(Some(root));
    diags.extend(ctx_diags);
    env_var_refs.extend(ctx_env_refs);
    (body, source_map)
}

/// BEP `@spec`: synthesize the body of the `<Fn>$spec` companion — an
/// `ai.FunctionSpec<Out>` literal binding the function's arguments:
///
/// ```baml
/// ai.FunctionSpec<Out> {
///     spec_name: "Fn",
///     args: { "p": p, ... },
///     prompt_template: (output_format: string) -> {
///         // the parameter's real name is ` __spec_output_format` (leading
///         // space) so it can never shadow a user identifier
///         let ctx = ai.internal.SpecCtx { output_format: output_format };
///         let tagged = baml.TaggedString { ...the function's prompt... };
///         ai.internal.assemble_prompt(tagged.parts, tagged.values)
///     },
///     toolbox: ai.Toolbox.new([ai.tool(a), ...]),
///     default_client: openai.OpenAiClient.new(model = "gpt-4o-mini"),
/// }
/// ```
///
/// The prompt closure uses the same structural parts/values representation as
/// the built-in `prompt` tag. `${role(...)}` values become prompt messages
/// and media remains structural. `ctx` is bound to an `ai.internal.SpecCtx`,
/// so `${ctx.output_format}` resolves to the closure's parameter and every
/// other interpolation captures the enclosing function's parameters. Provider
/// construction is pure, so the eager default client never touches credentials.
///
/// In the `tools` list, a bare function reference is wrapped in `ai.tool(...)`;
/// any other element expression must already produce an `ai.Tool`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn synthesize_llm_spec_body(
    function_name: &str,
    param_names: &[Name],
    client_spec: &crate::lower_cst::LlmClientSpec,
    out_type: Option<crate::ast::TypeExpr>,
    tools_value: Option<&rowan::NodeOrToken<SyntaxNode, baml_compiler_syntax::SyntaxToken>>,
    prompt: &crate::lower_cst::LlmPromptLiteral,
    span: TextRange,
) -> (
    ExprBody,
    AstSourceMap,
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

    // args: { "p": p, ... }
    let entries: Vec<MapExprEntry> = param_names
        .iter()
        .map(|name| {
            let key = ctx.alloc_expr(
                Expr::Literal(Literal::String(name.as_str().to_string())),
                span,
            );
            let value = ctx.alloc_expr(Expr::Path(vec![name.clone()]), span);
            MapExprEntry::explicit(key, value)
        })
        .collect();
    let args_map = ctx.alloc_expr(Expr::Map { entries }, span);

    // prompt_template: ( __spec_output_format: string) -> ai.Prompt { ... }
    //
    // The lambda parameter carries a leading-space name so it can never
    // shadow a user identifier: a function parameter named `output_format`
    // must stay visible to `${output_format}` in the template (the parameter
    // is only the render calling convention; `ctx.output_format` is the
    // documented way to reach the rendered schema). Mirrors the `__tt_*`
    // accumulator naming in `elaborate_tagged_body`.
    //
    // Binding references are span-position-checked against their `let`'s
    // visibility window, and the template's `${ctx.…}` interps keep their real
    // source ranges inside the literal — so the synthesized `let ctx` must sit
    // at an empty range at the literal's *start*, before every reference
    // (mirrors the accumulator lets in `elaborate_tagged_body`).
    let of_param_name = Name::new(" __spec_output_format");
    let prompt_lambda_span = prompt.span_range();
    let prompt_start = TextRange::empty(prompt_lambda_span.start());
    let of_ref = ctx.alloc_expr(Expr::Path(vec![of_param_name.clone()]), prompt_start);
    let spec_ctx_obj = ctx.alloc_expr(
        Expr::Object {
            type_name: baml_base::TypePath::from_dotted("ai.internal.SpecCtx"),
            type_args: vec![],
            fields: vec![ObjectExprField::explicit(
                Name::new("output_format"),
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
            else_branch: None,
        },
        prompt_start,
    );
    // Flatten the template exactly like the public `prompt` tag: values stay
    // structural until the Rust prompt assembler sees them. Rewrite the
    // prompt-local role constructor before name resolution; it is the same
    // binding that the public tag supplies to its body lambda.
    //
    // A quoted prompt is the degenerate template: regular string literals do
    // not interpolate, so the decoded value is one text segment and every
    // downstream step (role rewrite, tagged elaboration, assembly) is shared.
    let segments = match prompt {
        crate::lower_cst::LlmPromptLiteral::Backtick(lit) => {
            ctx.lower_template_segments_checked(lit)
        }
        crate::lower_cst::LlmPromptLiteral::Quoted(lit) => {
            vec![TemplateSegment::Text(lit.value())]
        }
    };
    let role_callees: Vec<ExprId> = ctx
        .exprs
        .iter()
        .filter_map(|(_, expr)| match expr {
            Expr::Call { callee, .. }
                if matches!(&ctx.exprs[*callee], Expr::Path(path) if path.len() == 1 && path[0].as_str() == "role") =>
            {
                Some(*callee)
            }
            _ => None,
        })
        .collect();
    for callee in role_callees {
        ctx.exprs[callee] = Expr::Path(vec![
            Name::new("baml"),
            Name::new("prompt"),
            Name::new("make_role"),
        ]);
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
            else_branch: None,
        },
        prompt_start,
    );
    let parts_base = ctx.alloc_expr(Expr::Path(vec![tagged_name.clone()]), prompt_start);
    let parts = ctx.alloc_expr(
        Expr::MemberAccess {
            base: parts_base,
            member: Name::new("parts"),
        },
        prompt_start,
    );
    let values_base = ctx.alloc_expr(Expr::Path(vec![tagged_name]), prompt_start);
    let values = ctx.alloc_expr(
        Expr::MemberAccess {
            base: values_base,
            member: Name::new("values"),
        },
        prompt_start,
    );
    let assemble_callee = ctx.alloc_expr(
        Expr::Path(vec![
            Name::new("ai"),
            Name::new("internal"),
            Name::new("assemble_prompt"),
        ]),
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
    let lambda_body = ctx.alloc_expr(
        Expr::Block {
            stmts: vec![let_ctx, let_tagged],
            tail_expr: Some(prompt_ast),
        },
        prompt_lambda_span,
    );
    let of_param = Param {
        name: of_param_name,
        type_expr: Some((TypeExprKind::String { attrs: vec![] }).at(span)),
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
    let tool_list = match tools_value {
        Some(rowan::NodeOrToken::Node(node)) if node.kind() == SyntaxKind::ARRAY_LITERAL => {
            let mut elements = Vec::new();
            for elem in node.children_with_tokens() {
                let lowered = match &elem {
                    rowan::NodeOrToken::Node(child) => {
                        Some((child.kind() == SyntaxKind::PATH_EXPR, ctx.lower_expr(child)))
                    }
                    rowan::NodeOrToken::Token(token) => {
                        let is_bare_ref = is_ident_token(token.kind());
                        ctx.try_lower_bare_token(token).map(|id| (is_bare_ref, id))
                    }
                };
                if let Some((is_bare_ref, id)) = lowered {
                    let id = if is_bare_ref {
                        // A bare function reference: normalize through ai.tool().
                        let tool_callee = ctx.alloc_expr(
                            Expr::Path(vec![
                                Name::new("ai"),
                                Name::new("tools"),
                                Name::new("tool"),
                            ]),
                            span,
                        );
                        ctx.alloc_expr(
                            Expr::Call {
                                callee: tool_callee,
                                type_args: vec![],
                                args: vec![CallArg::positional(id)],
                            },
                            span,
                        )
                    } else {
                        id
                    };
                    elements.push(id);
                }
            }
            ctx.alloc_expr(Expr::Array { elements }, span)
        }
        // A non-list expression must already produce an `ai.Tool[]`.
        Some(rowan::NodeOrToken::Node(node)) => ctx.lower_expr(node),
        // A bare dot-free identifier is a token, not a node: a variable
        // holding the `ai.Tool[]` value.
        Some(rowan::NodeOrToken::Token(token)) => ctx
            .try_lower_bare_token(token)
            .unwrap_or_else(|| ctx.alloc_expr(Expr::Missing, span)),
        None => ctx.alloc_expr(Expr::Array { elements: vec![] }, span),
    };
    let toolbox_callee = ctx.alloc_expr(
        Expr::Path(vec![
            Name::new("ai"),
            Name::new("tools"),
            Name::new("Toolbox"),
            Name::new("new"),
        ]),
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

    // default_client — an eager value; provider construction is pure
    // (credentials resolve from the environment at request time), so
    // building the spec never touches env. Either the compile-time-mapped
    // provider constructor for a "provider/model" string, or the user's own
    // client expression lowered in place.
    let default_client = match client_spec {
        crate::lower_cst::LlmClientSpec::Provider { pkg, class, model } => {
            let model_lit = ctx.alloc_expr(Expr::Literal(Literal::String(model.clone())), span);
            let ctor_callee = ctx.alloc_expr(
                Expr::Path(vec![Name::new(*pkg), Name::new(*class), Name::new("new")]),
                span,
            );
            ctx.alloc_expr(
                Expr::Call {
                    callee: ctor_callee,
                    type_args: vec![],
                    args: vec![CallArg::named("model", model_lit)],
                },
                span,
            )
        }
        crate::lower_cst::LlmClientSpec::Expr(rowan::NodeOrToken::Node(node)) => {
            ctx.lower_expr(node)
        }
        crate::lower_cst::LlmClientSpec::Expr(rowan::NodeOrToken::Token(token)) => ctx
            .try_lower_bare_token(token)
            .unwrap_or_else(|| ctx.alloc_expr(Expr::Missing, span)),
    };

    let type_args = out_type.map(|t| vec![t]).unwrap_or_default();
    let spec_obj = ctx.alloc_expr(
        Expr::Object {
            type_name: baml_base::TypePath::from_dotted("ai.FunctionSpec"),
            type_args,
            fields: vec![
                ObjectExprField::explicit(Name::new("spec_name"), name_lit),
                ObjectExprField::explicit(Name::new("args"), args_map),
                ObjectExprField::explicit(Name::new("prompt_template"), prompt_lambda),
                ObjectExprField::explicit(Name::new("toolbox"), toolbox),
                ObjectExprField::explicit(Name::new("default_client"), default_client),
            ],
            spreads: vec![],
        },
        span,
    );

    ctx.finish(Some(spec_obj))
}

/// Synthesize the `$render_prompt` companion body: render the spec's prompt
/// with the return type's output-format text —
/// `Fn$spec(p...).prompt(ai.wire.render_output_format(reflect.type_of<Out>()))`.
pub(crate) fn synthesize_spec_render_prompt_body(
    function_name: &str,
    param_names: &[Name],
    spec_type_args: Vec<crate::ast::TypeExpr>,
    out_type: Option<crate::ast::TypeExpr>,
    span: TextRange,
) -> (ExprBody, AstSourceMap) {
    use crate::ast::CallArg;

    let mut ctx = LoweringContext::new();

    let spec_callee = ctx.alloc_expr(
        Expr::Path(vec![Name::new(format!("{function_name}$spec"))]),
        span,
    );
    let spec_args: Vec<CallArg> = param_names
        .iter()
        .map(|n| CallArg::positional(ctx.alloc_expr(Expr::Path(vec![n.clone()]), span)))
        .collect();
    let spec_call = ctx.alloc_expr(
        Expr::Call {
            callee: spec_callee,
            type_args: spec_type_args,
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
    let type_of_callee = ctx.alloc_expr(
        Expr::Path(vec![Name::new("reflect"), Name::new("type_of")]),
        span,
    );
    let type_of_call = ctx.alloc_expr(
        Expr::Call {
            callee: type_of_callee,
            type_args: out_type.map(|t| vec![t]).unwrap_or_default(),
            args: vec![],
        },
        span,
    );
    let rof_callee = ctx.alloc_expr(
        Expr::Path(vec![
            Name::new("ai"),
            Name::new("wire"),
            Name::new("render_output_format"),
        ]),
        span,
    );
    let rof_call = ctx.alloc_expr(
        Expr::Call {
            callee: rof_callee,
            type_args: vec![],
            args: vec![CallArg::positional(type_of_call)],
        },
        span,
    );
    let render_call = ctx.alloc_expr(
        Expr::Call {
            callee: prompt_callee,
            type_args: vec![],
            args: vec![CallArg::named("output_format", rof_call)],
        },
        span,
    );

    let (body, source_map, _diags, _env_refs) = ctx.finish(Some(render_call));
    (body, source_map)
}

/// Synthesize the `$parse` companion body: a network-free parse of an
/// existing JSON/SAP string into the function's return type —
/// `baml.sap.parse<Out>(json)`.
pub(crate) fn synthesize_spec_parse_body(
    out_type: Option<crate::ast::TypeExpr>,
    span: TextRange,
) -> (ExprBody, AstSourceMap) {
    use crate::ast::CallArg;

    let mut ctx = LoweringContext::new();
    let json_ref = ctx.alloc_expr(Expr::Path(vec![Name::new("json")]), span);
    let callee = ctx.alloc_expr(
        Expr::Path(vec![
            Name::new("baml"),
            Name::new("sap"),
            Name::new("parse"),
        ]),
        span,
    );
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
/// ai.Agent<Out>.new(client = client).run(Fn$spec(p1, p2)).value
/// ```
///
/// `client` is the compiler-injected `ai.Client? = null` override parameter;
/// `Agent.run` falls back to the spec's default client when it is null.
pub(crate) fn synthesize_spec_agent_run_body(
    function_name: &str,
    param_names: &[Name],
    spec_type_args: Vec<crate::ast::TypeExpr>,
    out_type: Option<crate::ast::TypeExpr>,
    span: TextRange,
) -> (ExprBody, AstSourceMap) {
    use crate::ast::CallArg;

    let mut ctx = LoweringContext::new();

    // Fn$spec(p1, p2, ...)
    let spec_callee = ctx.alloc_expr(
        Expr::Path(vec![Name::new(format!("{function_name}$spec"))]),
        span,
    );
    let spec_args: Vec<CallArg> = param_names
        .iter()
        .map(|n| {
            let id = ctx.alloc_expr(Expr::Path(vec![n.clone()]), span);
            CallArg::positional(id)
        })
        .collect();
    let spec_call = ctx.alloc_expr(
        Expr::Call {
            callee: spec_callee,
            type_args: spec_type_args,
            args: spec_args,
        },
        span,
    );

    // ai.Agent<Out>.new(client = client)
    let agent_path = ctx.alloc_expr(Expr::Path(vec![Name::new("ai"), Name::new("Agent")]), span);
    let new_callee = ctx.alloc_expr(
        Expr::MemberAccess {
            base: agent_path,
            member: Name::new("new"),
        },
        span,
    );
    let client_ref = ctx.alloc_expr(Expr::Path(vec![Name::new("client")]), span);
    let type_args = out_type.map(|t| vec![t]).unwrap_or_default();
    let new_call = ctx.alloc_expr(
        Expr::Call {
            callee: new_callee,
            type_args,
            args: vec![CallArg::named("client", client_ref)],
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
    let run_call = ctx.alloc_expr(
        Expr::Call {
            callee: run_callee,
            type_args: vec![],
            args: vec![CallArg::positional(spec_call)],
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

/// Synthesize the `$stream` companion body (built at PPIR level, where the
/// stream-expanded return type is known) — one-turn streaming over the
/// function's own spec:
///
/// ```baml
/// ai.stream.from_spec<Out$stream, Out>(Fn$spec(p1, p2), client = client)
/// ```
///
/// `type_args` is the explicit `<STREAM_EXPANDED, ORIGINAL>` pair, so the
/// stdlib reifies both types from its own frame via `reflect.type_of`.
/// `client` is the companion's injected `ai.StreamingClient? = null`
/// override; `from_spec` falls back to the spec's default client when it
/// is null.
pub fn synthesize_spec_stream_body(
    function_name: &str,
    param_names: &[Name],
    spec_type_args: Vec<crate::ast::TypeExpr>,
    type_args: Vec<crate::ast::TypeExpr>,
    span: TextRange,
) -> (ExprBody, AstSourceMap) {
    use crate::ast::CallArg;

    let mut ctx = LoweringContext::new();

    // Fn$spec(p1, p2, ...)
    let spec_callee = ctx.alloc_expr(
        Expr::Path(vec![Name::new(format!("{function_name}$spec"))]),
        span,
    );
    let spec_args: Vec<CallArg> = param_names
        .iter()
        .map(|n| {
            let id = ctx.alloc_expr(Expr::Path(vec![n.clone()]), span);
            CallArg::positional(id)
        })
        .collect();
    let spec_call = ctx.alloc_expr(
        Expr::Call {
            callee: spec_callee,
            type_args: spec_type_args,
            args: spec_args,
        },
        span,
    );

    // ai.stream.from_spec<TS, TF>(spec, client = client)
    let stream_spec_callee = ctx.alloc_expr(
        Expr::Path(vec![
            Name::new("ai"),
            Name::new("stream"),
            Name::new("from_spec"),
        ]),
        span,
    );
    let client_ref = ctx.alloc_expr(Expr::Path(vec![Name::new("client")]), span);
    let call = ctx.alloc_expr(
        Expr::Call {
            callee: stream_spec_callee,
            type_args,
            args: vec![
                CallArg::positional(spec_call),
                CallArg::named("client", client_ref),
            ],
        },
        span,
    );

    let (body, source_map, _diags, _env_refs) = ctx.finish(Some(call));
    (body, source_map)
}

struct LoweringContext {
    exprs: Arena<Expr>,
    stmts: Arena<Stmt>,
    patterns: Arena<Pattern>,
    match_arms: Arena<MatchArm>,
    catch_arms: Arena<CatchArm>,
    type_annotations: Arena<TypeExpr>,
    /// Parallel span storage
    source_map: AstSourceMap,
    /// When set, `TEST_EXPR_DEF` and `TESTSET_DEF` nodes encountered during block
    /// lowering are converted to `<var>.register_test(...)` / `<var>.register_test_set(...)`
    /// calls using this variable name. This supports dynamic test generation inside
    /// `for`/`if` blocks inside a testset body.
    testset_collector_var: Option<Name>,
    /// Diagnostics accumulated during lowering.
    diags: Vec<LoweringDiagnostic>,
    /// Environment variable references (`env.X`) found during lowering.
    env_var_refs: Vec<EnvVarRef>,
    /// Expressions that contain unwrapped `?.` operators and need an `OptionalChain` wrapper.
    /// Propagated up through chain-continuing nodes (`FieldAccess`, Index, Call, Optional*).
    needs_chain_wrap: std::collections::HashSet<ExprId>,
    /// Text ranges of `GENERIC_ARGS` nodes that an enclosing call has already
    /// consumed as call-site `type_args` (e.g. the `<int>` in `foo<int>(x)`).
    /// `lower_path_expr` skips wrapping these into an `Expr::GenericApply` so the
    /// args aren't double-counted. Only standalone, value-position `<...>` (e.g.
    /// `let f = foo<int>`) becomes a `GenericApply`.
    consumed_generic_args: std::collections::HashSet<TextRange>,
    /// When set, every node allocated via `alloc_expr`/`alloc_stmt`/
    /// `alloc_pattern` is recorded as compiler-synthesized in the source map.
    /// Scoped on around desugarings (e.g. backtick-template elaboration) so the
    /// generated nodes are distinguishable from the user-written ones they wrap
    /// — see [`AstSourceMap::synthetic_exprs`]. Consumed by inlay hints.
    synthesizing: bool,
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
    fn new() -> Self {
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
            consumed_generic_args: std::collections::HashSet::new(),
            synthesizing: false,
        }
    }

    fn warn_const_introducer(&mut self, span: TextRange) {
        self.diags
            .push(LoweringDiagnostic::ConstBindingIntroducer { span });
    }

    /// Lower an `INTEGER_LITERAL` token to its value, emitting diagnostics
    /// for invalid literals (bad base prefix, no digits, invalid digit for
    /// the base, too large).
    fn int_literal_value(&mut self, token: &SyntaxToken) -> i64 {
        crate::lower_int_literal(token.text(), token.text_range(), &mut self.diags)
    }

    /// Lower a `BIGINT_LITERAL` token to its value, emitting diagnostics for
    /// invalid literals.
    fn bigint_literal_value(&mut self, token: &SyntaxToken) -> num_bigint::BigInt {
        crate::lower_bigint_literal(token.text(), token.text_range(), &mut self.diags)
    }

    fn warn_direct_const_introducers(&mut self, node: &SyntaxNode) {
        let spans: Vec<TextRange> = node
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .filter(|token| token.kind() == SyntaxKind::KW_CONST)
            .map(|token| token.text_range())
            .collect();
        for span in spans {
            self.warn_const_introducer(span);
        }
    }

    fn alloc_expr(&mut self, expr: Expr, range: TextRange) -> ExprId {
        let id = self.exprs.alloc(expr);
        self.source_map.expr_spans.alloc(range);
        if self.synthesizing {
            self.source_map.synthetic_exprs.insert(id);
        }
        id
    }

    fn alloc_stmt(&mut self, stmt: Stmt, range: TextRange) -> StmtId {
        let id = self.stmts.alloc(stmt);
        self.source_map.stmt_spans.alloc(range);
        if self.synthesizing {
            self.source_map.synthetic_stmts.insert(id);
        }
        id
    }

    fn alloc_pattern(&mut self, pattern: Pattern, range: TextRange) -> PatId {
        let id = self.patterns.alloc(pattern);
        self.source_map.pattern_spans.alloc(range);
        if self.synthesizing {
            self.source_map.synthetic_patterns.insert(id);
        }
        id
    }

    fn alloc_match_arm(&mut self, arm: MatchArm, range: TextRange) -> MatchArmId {
        let id = self.match_arms.alloc(arm);
        self.source_map.match_arm_spans.alloc(range);
        id
    }

    fn alloc_catch_arm(&mut self, arm: CatchArm, range: TextRange) -> CatchArmId {
        let id = self.catch_arms.alloc(arm);
        self.source_map.catch_arm_spans.alloc(range);
        id
    }

    fn alloc_type_annot(&mut self, ty: TypeExpr, range: TextRange) -> TypeAnnotId {
        let id = self.type_annotations.alloc(ty);
        self.source_map.type_annotation_spans.alloc(range);
        id
    }

    /// Try to lower a bare token (not wrapped in a node) into an expression.
    ///
    /// The parser sometimes emits single identifiers and literals as bare tokens
    /// rather than wrapping them in `PATH_EXPR` or other nodes. This helper handles
    /// those cases so lowering functions that iterate `children_with_tokens()` can
    /// process both nodes and tokens uniformly.
    fn try_lower_bare_token(
        &mut self,
        token: &rowan::SyntaxToken<baml_compiler_syntax::BamlLanguage>,
    ) -> Option<ExprId> {
        let span = token.text_range();
        match token.kind() {
            k if is_ident_token(k) => {
                let text = token.text();
                let expr = match text {
                    "true" => Expr::Literal(Literal::Bool(true)),
                    "false" => Expr::Literal(Literal::Bool(false)),
                    "null" => Expr::Null,
                    _ => Expr::Path(vec![Name::new(text)]),
                };
                Some(self.alloc_expr(expr, span))
            }
            SyntaxKind::BIGINT_LITERAL => {
                let value = self.bigint_literal_value(token);
                Some(self.alloc_expr(Expr::Literal(Literal::Bigint(value)), span))
            }
            SyntaxKind::INTEGER_LITERAL => {
                let value = self.int_literal_value(token);
                Some(self.alloc_expr(Expr::Literal(Literal::Int(value)), span))
            }
            SyntaxKind::FLOAT_LITERAL => Some(self.alloc_expr(
                Expr::Literal(Literal::Float(num_lit::normalize_float_literal(
                    token.text(),
                ))),
                span,
            )),
            _ => None,
        }
    }

    /// Lower a lambda's `BLOCK_EXPR` into this arena.
    ///
    /// Clears `testset_collector_var` for the duration: a `test` / `testset`
    /// written inside a lambda body is not in a collector's scope, so it must
    /// lower to `Stmt::Missing` rather than a registration call. Before lambda
    /// bodies shared this arena that fell out of building them in a fresh
    /// `LoweringContext`; now it has to be said.
    fn lower_lambda_body(&mut self, block: &baml_compiler_syntax::ast::BlockExpr) -> ExprId {
        let saved_collector = self.testset_collector_var.take();
        let body = self.lower_block_expr(block);
        self.testset_collector_var = saved_collector;
        body
    }

    /// Lower a testset's `BLOCK_EXPR` into this arena as a collector-lambda body.
    ///
    /// Sets `testset_collector_var` for the duration — the exact inverse of
    /// [`Self::lower_lambda_body`] — so a `test` written inside registers
    /// against `collector`. The body always ends in a `null` tail, which is what
    /// the collector lambda's `-> void` signature expects.
    fn lower_testset_collector_body(
        &mut self,
        block: &baml_compiler_syntax::ast::BlockExpr,
        collector: Name,
        range: TextRange,
    ) -> ExprId {
        let saved_collector = self.testset_collector_var.replace(collector);
        let inner = self.lower_block_expr(block);
        let body = self.ensure_null_tail(inner, range);
        self.testset_collector_var = saved_collector;
        body
    }

    /// Ensure a block expression ends with a `null` tail.
    ///
    /// If `block_id` refers to a `Block` with no tail expression, this adds a `null` tail
    /// by constructing a new block expression that reuses the same statements.
    /// If the block already has a non-null tail, this wraps it in a new block that evaluates
    /// the original block as a statement and then returns null.
    fn ensure_null_tail(&mut self, block_id: ExprId, range: TextRange) -> ExprId {
        match self.exprs[block_id].clone() {
            Expr::Block { stmts, tail_expr } => {
                match tail_expr {
                    None => {
                        // No tail — add explicit null tail by allocating a new block
                        let null_id = self.alloc_expr(Expr::Null, range);
                        self.alloc_expr(
                            Expr::Block {
                                stmts,
                                tail_expr: Some(null_id),
                            },
                            range,
                        )
                    }
                    Some(t) if matches!(self.exprs[t], Expr::Null) => {
                        // Already has null tail — return as-is
                        block_id
                    }
                    Some(_) => {
                        // Has a non-null tail expression — keep it as a statement and add null
                        let inner_as_stmt = self.alloc_stmt(Stmt::Expr(block_id), range);
                        let null_id = self.alloc_expr(Expr::Null, range);
                        self.alloc_expr(
                            Expr::Block {
                                stmts: vec![inner_as_stmt],
                                tail_expr: Some(null_id),
                            },
                            range,
                        )
                    }
                }
            }
            _ => {
                // Not a block — wrap in a block with null tail
                let inner_as_stmt = self.alloc_stmt(Stmt::Expr(block_id), range);
                let null_id = self.alloc_expr(Expr::Null, range);
                self.alloc_expr(
                    Expr::Block {
                        stmts: vec![inner_as_stmt],
                        tail_expr: Some(null_id),
                    },
                    range,
                )
            }
        }
    }

    fn finish(
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
            root_expr,
        };
        (body, self.source_map, self.diags, self.env_var_refs)
    }

    fn lower_block_expr(&mut self, block: &baml_compiler_syntax::ast::BlockExpr) -> ExprId {
        use baml_compiler_syntax::ast::BlockElement;

        let mut stmts = Vec::new();
        let mut tail_expr = None;

        let elements: Vec<_> = block.elements().collect();

        for (idx, element) in elements.iter().enumerate() {
            let is_last = idx == elements.len() - 1;
            match element {
                BlockElement::Stmt(node) => {
                    let stmt_id = match node.kind() {
                        SyntaxKind::LET_STMT => self.lower_let_stmt(node),
                        SyntaxKind::RETURN_STMT => self.lower_return_stmt(node),
                        SyntaxKind::THROW_STMT => self.lower_throw_stmt(node),
                        SyntaxKind::WHILE_STMT => self.lower_while_stmt(node),
                        SyntaxKind::WHILE_LET_STMT => self.lower_while_let_stmt(node),
                        SyntaxKind::FOR_EXPR => self.lower_for_stmt(node),
                        SyntaxKind::BREAK_STMT => self.alloc_stmt(Stmt::Break, node.span_range()),
                        SyntaxKind::CONTINUE_STMT => {
                            self.alloc_stmt(Stmt::Continue, node.span_range())
                        }
                        SyntaxKind::DEFER_STMT => self.lower_defer_stmt(node),
                        // A nested `test` / `testset` registers against the
                        // enclosing `testset`'s collector, so it only means
                        // something where `testset_collector_var` is set — i.e.
                        // directly inside a `testset` body. Everywhere else there
                        // is nothing to register against and the declaration is
                        // dropped: inside a `test` body (which clears the var, so
                        // tests don't nest), inside a lambda, or in an ordinary
                        // function.
                        //
                        // BUG: it is dropped *silently* — neither the parser nor
                        // this lowering reports it. `testset "A" { test "B" { test
                        // "C" {} } }` compiles clean while `test "C"` never runs.
                        SyntaxKind::TEST_EXPR_DEF => {
                            if self.testset_collector_var.is_some() {
                                let expr_id = self.lower_test_expr_as_register_call(node);
                                self.alloc_stmt(Stmt::Expr(expr_id), node.span_range())
                            } else {
                                self.alloc_stmt(Stmt::Missing, node.span_range())
                            }
                        }
                        SyntaxKind::TESTSET_DEF => {
                            if self.testset_collector_var.is_some() {
                                let expr_id = self.lower_testset_as_register_call(node);
                                self.alloc_stmt(Stmt::Expr(expr_id), node.span_range())
                            } else {
                                self.alloc_stmt(Stmt::Missing, node.span_range())
                            }
                        }
                        _ => self.alloc_stmt(Stmt::Missing, node.span_range()),
                    };
                    stmts.push(stmt_id);
                }
                BlockElement::ExprNode(node) => {
                    // First, try to lower as an assignment statement
                    if let Some(stmt_id) = self.try_lower_assignment(node) {
                        stmts.push(stmt_id);
                        continue;
                    }

                    let expr_id = self.lower_expr(node);
                    let has_semicolon = element.has_trailing_semicolon();

                    if is_last && !has_semicolon {
                        tail_expr = Some(expr_id);
                    } else {
                        stmts.push(self.alloc_stmt(Stmt::Expr(expr_id), node.span_range()));
                    }
                }
                BlockElement::ExprToken(token) => {
                    let span = token.text_range();
                    let expr_id = match token.kind() {
                        k if is_ident_token(k) => {
                            let text = token.text();
                            let e = match text {
                                "true" => Expr::Literal(Literal::Bool(true)),
                                "false" => Expr::Literal(Literal::Bool(false)),
                                "null" => Expr::Null,
                                _ => Expr::Path(vec![Name::new(text)]),
                            };
                            self.alloc_expr(e, span)
                        }
                        SyntaxKind::BIGINT_LITERAL => {
                            let value = self.bigint_literal_value(token);
                            self.alloc_expr(Expr::Literal(Literal::Bigint(value)), span)
                        }
                        SyntaxKind::INTEGER_LITERAL => {
                            let value = self.int_literal_value(token);
                            self.alloc_expr(Expr::Literal(Literal::Int(value)), span)
                        }
                        SyntaxKind::FLOAT_LITERAL => {
                            let text = num_lit::normalize_float_literal(token.text());
                            self.alloc_expr(Expr::Literal(Literal::Float(text)), span)
                        }
                        SyntaxKind::STRING_LITERAL | SyntaxKind::RAW_STRING_LITERAL => {
                            let text = token.text().to_string();
                            let content = strip_string_delimiters(&text);
                            self.alloc_expr(Expr::Literal(Literal::String(content)), span)
                        }
                        _ => self.alloc_expr(Expr::Missing, span),
                    };

                    let has_semicolon = element.has_trailing_semicolon();
                    if is_last && !has_semicolon {
                        tail_expr = Some(expr_id);
                    } else {
                        stmts.push(self.alloc_stmt(Stmt::Expr(expr_id), span));
                    }
                }
                BlockElement::HeaderComment(node) => {
                    let stmt_id = self.lower_header_comment(node);
                    stmts.push(stmt_id);
                }
            }
        }

        self.alloc_expr(
            Expr::Block { stmts, tail_expr },
            block.syntax().span_range(),
        )
    }

    /// General entry point — wraps any unwrapped optional chain.
    fn lower_expr(&mut self, node: &SyntaxNode) -> ExprId {
        let id = self.lower_expr_inner(node);
        if self.needs_chain_wrap.remove(&id) {
            self.alloc_expr(Expr::OptionalChain { expr: id }, node.span_range())
        } else {
            id
        }
    }

    /// Chain-internal entry point — does NOT wrap.
    /// Used by chain-continuing handlers (`FieldAccess`, Index, Call, Optional*)
    /// when lowering their base/callee child.
    fn lower_expr_in_chain(&mut self, node: &SyntaxNode) -> ExprId {
        self.lower_expr_inner(node)
    }

    fn lower_expr_inner(&mut self, node: &SyntaxNode) -> ExprId {
        match node.kind() {
            SyntaxKind::BINARY_EXPR => self.lower_binary_expr(node),
            SyntaxKind::IS_EXPR => self.lower_is_expr(node),
            SyntaxKind::UNARY_EXPR => self.lower_unary_expr(node),
            SyntaxKind::CALL_EXPR => self.lower_call_expr(node),
            SyntaxKind::IF_EXPR => self.lower_if_expr(node),
            SyntaxKind::IF_LET_EXPR => self.lower_if_let_expr(node),
            SyntaxKind::MATCH_EXPR => self.lower_match_expr(node),
            SyntaxKind::CATCH_EXPR => self.lower_catch_expr(node),
            SyntaxKind::THROW_EXPR => self.lower_throw_expr(node),
            SyntaxKind::RETURN_EXPR => self.lower_return_expr(node),
            SyntaxKind::BREAK_EXPR => self.lower_jump_expr(node, Stmt::Break),
            SyntaxKind::CONTINUE_EXPR => self.lower_jump_expr(node, Stmt::Continue),
            SyntaxKind::BLOCK_EXPR => {
                if let Some(block) = baml_compiler_syntax::ast::BlockExpr::cast(node.clone()) {
                    self.lower_block_expr(&block)
                } else {
                    self.alloc_expr(Expr::Missing, node.span_range())
                }
            }
            SyntaxKind::PATH_EXPR => self.lower_path_expr(node),
            SyntaxKind::FIELD_ACCESS_EXPR => self.lower_field_access_expr(node),
            SyntaxKind::UPCAST_EXPR => self.lower_upcast_expr(node),
            SyntaxKind::SPEC_EXPR => self.lower_spec_expr(node),
            SyntaxKind::OPTIONAL_FIELD_ACCESS_EXPR => self.lower_optional_field_access_expr(node),
            SyntaxKind::ENV_ACCESS_EXPR => self.lower_env_access_expr(node),
            SyntaxKind::INDEX_EXPR => self.lower_index_expr(node),
            SyntaxKind::OPTIONAL_INDEX_EXPR => self.lower_optional_index_expr(node),
            SyntaxKind::OPTIONAL_CALL_EXPR => self.lower_optional_call_expr(node),
            SyntaxKind::TAGGED_TEMPLATE_EXPR => self.lower_tagged_template_expr(node),
            SyntaxKind::PAREN_EXPR => {
                if let Some(inner) = node.children().next() {
                    self.lower_expr(&inner)
                } else {
                    self.try_lower_paren_token_content(node)
                        .unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.span_range()))
                }
            }
            SyntaxKind::STRING_LITERAL | SyntaxKind::RAW_STRING_LITERAL => {
                self.lower_string_literal(node)
            }
            SyntaxKind::BACKTICK_STRING_LITERAL => self.lower_backtick_string_literal(node),
            SyntaxKind::BYTE_STRING_LITERAL => self.lower_byte_string_literal(node),
            SyntaxKind::ARRAY_LITERAL => self.lower_array_literal(node),
            SyntaxKind::OBJECT_LITERAL => self.lower_object_literal(node),
            SyntaxKind::MAP_LITERAL => self.lower_map_literal(node),
            SyntaxKind::LAMBDA_EXPR => self.lower_lambda_expr(node),
            SyntaxKind::SPAWN_EXPR => self.lower_spawn_expr(node),
            SyntaxKind::AWAIT_EXPR => self.lower_await_expr(node),
            _ => {
                if let Some(literal) = self.try_lower_literal_token(node) {
                    literal
                } else {
                    self.alloc_expr(Expr::Missing, node.span_range())
                }
            }
        }
    }

    /// Lower `spawn name_expr? { body }`. The CST shape is
    /// `SPAWN_EXPR [ KW_SPAWN [expr] BLOCK_EXPR ]`.
    ///
    /// To reuse the existing lambda machinery, the body is lowered as a
    /// fresh 0-arg lambda (its own `ExprBody` + source map). The
    /// `Expr::Spawn { body }` ID points at an `Expr::Lambda(func_def)`,
    /// which the MIR lowering then handles via `lower_lambda` to emit
    /// the proper `MakeClosure`. The name expression is parsed in the
    /// outer context (where it can reference outer bindings).
    ///
    /// That body-is-a-lambda invariant is upheld here rather than assumed
    /// downstream: with no `BLOCK_EXPR` there is no lambda to build, so the
    /// whole `spawn` lowers to [`Expr::Missing`] instead of a `Spawn` wrapping
    /// a non-lambda body. Inference projects the body's return type and reads
    /// its effective throws out of the lambda side table, neither of which
    /// exists for a non-lambda.
    fn lower_spawn_expr(&mut self, node: &SyntaxNode) -> ExprId {
        use baml_compiler_syntax::ast as cst_ast;

        let mut name: Option<ExprId> = None;
        let mut with_exprs: Vec<ExprId> = Vec::new();
        let mut body_lambda: Option<ExprId> = None;
        // The CST is flat: `KW_SPAWN [name expr] [KW_WITH expr (COMMA expr)*]
        // BLOCK_EXPR`. We walk children-with-tokens so the `KW_WITH` token is
        // visible; everything after it (other than the body) is a `with` expr.
        let mut seen_with = false;

        for child in node.children_with_tokens() {
            let kind = child.kind();
            if kind == SyntaxKind::KW_WITH {
                seen_with = true;
                continue;
            }
            // Expression NODES and bare expression TOKENS both matter past
            // here (a literal like `spawn with 42 { .. }` arrives as a plain
            // token); skip `KW_SPAWN`, commas, and trivia. Dropping tokens
            // here would silently swallow the expression — the type checker
            // must see it to reject it with a real diagnostic.
            let child = match child {
                rowan::NodeOrToken::Node(n) => n,
                rowan::NodeOrToken::Token(t) => {
                    let span = t.text_range();
                    let expr = match t.kind() {
                        SyntaxKind::INTEGER_LITERAL => {
                            Some(Expr::Literal(Literal::Int(self.int_literal_value(&t))))
                        }
                        SyntaxKind::FLOAT_LITERAL => Some(Expr::Literal(Literal::Float(
                            num_lit::normalize_float_literal(t.text()),
                        ))),
                        SyntaxKind::STRING_LITERAL | SyntaxKind::RAW_STRING_LITERAL => Some(
                            Expr::Literal(Literal::String(strip_string_delimiters(t.text()))),
                        ),
                        // `spawn`/`await` pass `is_ident_token` (they're
                        // valid path SEGMENTS) but here they're the keywords
                        // themselves — never a name/with expression.
                        SyntaxKind::KW_SPAWN | SyntaxKind::KW_AWAIT => None,
                        k if is_ident_token(k) => Some(match t.text() {
                            "true" => Expr::Literal(Literal::Bool(true)),
                            "false" => Expr::Literal(Literal::Bool(false)),
                            "null" => Expr::Null,
                            other => Expr::Path(vec![Name::new(other)]),
                        }),
                        _ => None,
                    };
                    if let Some(expr) = expr {
                        let id = self.alloc_expr(expr, span);
                        if seen_with {
                            with_exprs.push(id);
                        } else if name.is_none() {
                            name = Some(id);
                        }
                    }
                    continue;
                }
            };
            let child = &child;
            if kind == SyntaxKind::BLOCK_EXPR {
                // Synthesize a 0-arg lambda whose body is this block —
                // mirroring `lower_lambda_expr` so the existing
                // capture / scope / MIR plumbing applies unchanged.
                let block = cst_ast::BlockExpr::cast(child.clone());
                let func_def = block.map(|block| {
                    let body = self.lower_lambda_body(&block);
                    LambdaDef {
                        kind: LambdaKind::Spawn,
                        params: Vec::new(),
                        defaults: crate::ast::FunctionDefaults::empty(),
                        return_type: None,
                        throws: None,
                        body: Some(body),
                        span: child.span_range(),
                    }
                });
                if let Some(fd) = func_def {
                    body_lambda =
                        Some(self.alloc_expr(Expr::Lambda(Box::new(fd)), child.span_range()));
                }
            } else if seen_with {
                with_exprs.push(self.lower_expr(child));
            } else if name.is_none() {
                name = Some(self.lower_expr(child));
            }
        }

        // No block — an incomplete `spawn` (an editor prefix typed before the
        // body exists, or a syntax error the parser has already reported).
        // There is no lambda to hang the spawn off, so the expression itself is
        // what is missing.
        let Some(body) = body_lambda else {
            return self.alloc_expr(Expr::Missing, node.span_range());
        };
        self.alloc_expr(
            Expr::Spawn {
                name,
                with_exprs,
                body,
            },
            node.span_range(),
        )
    }

    /// Lower `await expr`. The CST shape is `AWAIT_EXPR [ KW_AWAIT
    /// (expr_node | ident_token | literal_token) ]`.
    ///
    /// Bare identifiers like `await f` are emitted by the parser as a
    /// raw `WORD` token (not a `PATH_EXPR` node), so we have to scan
    /// both child nodes and child tokens — analogous to how
    /// `BlockElement` handles `ExprToken`.
    fn lower_await_expr(&mut self, node: &SyntaxNode) -> ExprId {
        for elem in node.children_with_tokens() {
            match elem {
                rowan::NodeOrToken::Node(child) if child.kind() != SyntaxKind::KW_AWAIT => {
                    let future = self.lower_expr(&child);
                    return self.alloc_expr(Expr::Await { future }, node.span_range());
                }
                rowan::NodeOrToken::Token(token) if token.kind() != SyntaxKind::KW_AWAIT => {
                    if let Some(future) = self.try_lower_bare_token(&token) {
                        return self.alloc_expr(Expr::Await { future }, node.span_range());
                    }
                }
                _ => {}
            }
        }
        let future = self.alloc_expr(Expr::Missing, node.span_range());
        self.alloc_expr(Expr::Await { future }, node.span_range())
    }

    fn lower_binary_expr(&mut self, node: &SyntaxNode) -> ExprId {
        let mut lhs = None;
        let mut rhs = None;
        let mut op = None;

        for elem in node.children_with_tokens() {
            match elem {
                rowan::NodeOrToken::Node(child_node) => {
                    let expr_id = self.lower_expr(&child_node);
                    if lhs.is_none() {
                        lhs = Some(expr_id);
                    } else {
                        rhs = Some(expr_id);
                    }
                }
                rowan::NodeOrToken::Token(token) => {
                    let span = token.text_range();
                    match token.kind() {
                        SyntaxKind::PLUS => op = Some(BinaryOp::Add),
                        SyntaxKind::MINUS => op = Some(BinaryOp::Sub),
                        SyntaxKind::STAR => op = Some(BinaryOp::Mul),
                        SyntaxKind::SLASH => op = Some(BinaryOp::Div),
                        SyntaxKind::PERCENT => op = Some(BinaryOp::Mod),
                        SyntaxKind::EQUALS_EQUALS => op = Some(BinaryOp::Eq),
                        SyntaxKind::NOT_EQUALS => op = Some(BinaryOp::Ne),
                        SyntaxKind::LESS => op = Some(BinaryOp::Lt),
                        SyntaxKind::LESS_EQUALS => op = Some(BinaryOp::Le),
                        SyntaxKind::GREATER => op = Some(BinaryOp::Gt),
                        SyntaxKind::GREATER_EQUALS => op = Some(BinaryOp::Ge),
                        SyntaxKind::AND_AND => op = Some(BinaryOp::And),
                        SyntaxKind::OR_OR => op = Some(BinaryOp::Or),
                        SyntaxKind::AND => op = Some(BinaryOp::BitAnd),
                        SyntaxKind::PIPE => op = Some(BinaryOp::BitOr),
                        SyntaxKind::CARET => op = Some(BinaryOp::BitXor),
                        SyntaxKind::LESS_LESS => op = Some(BinaryOp::Shl),
                        SyntaxKind::GREATER_GREATER => op = Some(BinaryOp::Shr),
                        SyntaxKind::KW_INSTANCEOF => {
                            self.diags
                                .push(LoweringDiagnostic::InstanceofRemoved { span });
                            return self.alloc_expr(Expr::Missing, node.span_range());
                        }
                        SyntaxKind::QUESTION_QUESTION => op = Some(BinaryOp::NullCoalesce),
                        SyntaxKind::QUESTION if op.is_none() => {
                            // Two consecutive QUESTION tokens = null coalescing (??)
                            // The parser emits them as two separate tokens in BINARY_EXPR.
                            // First QUESTION sets a provisional op; second one confirms.
                            op = Some(BinaryOp::NullCoalesce);
                        }
                        SyntaxKind::BIGINT_LITERAL => {
                            let value = self.bigint_literal_value(&token);
                            let expr_id =
                                self.alloc_expr(Expr::Literal(Literal::Bigint(value)), span);
                            if lhs.is_none() {
                                lhs = Some(expr_id);
                            } else {
                                rhs = Some(expr_id);
                            }
                        }
                        SyntaxKind::INTEGER_LITERAL => {
                            let value = self.int_literal_value(&token);
                            let expr_id = self.alloc_expr(Expr::Literal(Literal::Int(value)), span);
                            if lhs.is_none() {
                                lhs = Some(expr_id);
                            } else {
                                rhs = Some(expr_id);
                            }
                        }
                        SyntaxKind::FLOAT_LITERAL => {
                            let expr_id = self.alloc_expr(
                                Expr::Literal(Literal::Float(num_lit::normalize_float_literal(
                                    token.text(),
                                ))),
                                span,
                            );
                            if lhs.is_none() {
                                lhs = Some(expr_id);
                            } else {
                                rhs = Some(expr_id);
                            }
                        }
                        k if is_ident_token(k) => {
                            let text = token.text();
                            let expr_id = match text {
                                "true" => self.alloc_expr(Expr::Literal(Literal::Bool(true)), span),
                                "false" => {
                                    self.alloc_expr(Expr::Literal(Literal::Bool(false)), span)
                                }
                                "null" => self.alloc_expr(Expr::Null, span),
                                _ => self.alloc_expr(Expr::Path(vec![Name::new(text)]), span),
                            };
                            if lhs.is_none() {
                                lhs = Some(expr_id);
                            } else {
                                rhs = Some(expr_id);
                            }
                        }
                        // Assignment operators are not valid in expression context.
                        // They are handled as statements by try_lower_assignment().
                        // If we see them here, the user wrote something like `(x = 5)`
                        // which is not a valid expression — report it and lower to
                        // Missing instead of silently defaulting to BinaryOp::Add.
                        SyntaxKind::EQUALS
                        | SyntaxKind::PLUS_EQUALS
                        | SyntaxKind::MINUS_EQUALS
                        | SyntaxKind::STAR_EQUALS
                        | SyntaxKind::SLASH_EQUALS
                        | SyntaxKind::PERCENT_EQUALS
                        | SyntaxKind::AND_EQUALS
                        | SyntaxKind::PIPE_EQUALS
                        | SyntaxKind::CARET_EQUALS
                        | SyntaxKind::LESS_LESS_EQUALS
                        | SyntaxKind::GREATER_GREATER_EQUALS => {
                            self.diags
                                .push(LoweringDiagnostic::AssignmentInExpressionPosition {
                                    span: node.span_range(),
                                });
                            return self.alloc_expr(Expr::Missing, node.span_range());
                        }
                        _ => {}
                    }
                }
            }
        }

        let lhs = lhs.unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.span_range()));
        let rhs = rhs.unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.span_range()));
        let op = op.unwrap_or(BinaryOp::Add);

        self.alloc_expr(Expr::Binary { op, lhs, rhs }, node.span_range())
    }

    /// Lower `<expr> is <pattern>` to `Expr::Is`. The pattern-test semantics
    /// (always `bool`, no exhaustiveness, non-matching is fine) are enforced
    /// downstream by TIR/MIR — this layer just preserves the shape.
    fn lower_is_expr(&mut self, node: &SyntaxNode) -> ExprId {
        let mut scrutinee = None;
        let mut pattern = None;

        for elem in node.children_with_tokens() {
            match elem {
                rowan::NodeOrToken::Node(child) => match child.kind() {
                    SyntaxKind::PATTERN => {
                        pattern = Some(self.lower_pattern(&child));
                    }
                    _ => {
                        if scrutinee.is_none() {
                            scrutinee = Some(self.lower_expr(&child));
                        }
                    }
                },
                rowan::NodeOrToken::Token(token) => {
                    // The LHS may be a bare token (WORD, INTEGER_LITERAL, …)
                    // not wrapped in its own node by the Pratt parser.
                    if scrutinee.is_none() {
                        let span = token.text_range();
                        match token.kind() {
                            SyntaxKind::BIGINT_LITERAL => {
                                let value = self.bigint_literal_value(&token);
                                scrutinee = Some(
                                    self.alloc_expr(Expr::Literal(Literal::Bigint(value)), span),
                                );
                            }
                            SyntaxKind::INTEGER_LITERAL => {
                                let value = self.int_literal_value(&token);
                                scrutinee =
                                    Some(self.alloc_expr(Expr::Literal(Literal::Int(value)), span));
                            }
                            SyntaxKind::FLOAT_LITERAL => {
                                scrutinee = Some(self.alloc_expr(
                                    Expr::Literal(Literal::Float(
                                        num_lit::normalize_float_literal(token.text()),
                                    )),
                                    span,
                                ));
                            }
                            k if is_ident_token(k) => {
                                let text = token.text();
                                let e = match text {
                                    "true" => Expr::Literal(Literal::Bool(true)),
                                    "false" => Expr::Literal(Literal::Bool(false)),
                                    "null" => Expr::Null,
                                    _ => Expr::Path(vec![Name::new(text)]),
                                };
                                scrutinee = Some(self.alloc_expr(e, span));
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        let span = node.span_range();
        let scrutinee = scrutinee.unwrap_or_else(|| self.alloc_expr(Expr::Missing, span));
        let pattern = pattern.unwrap_or_else(|| self.alloc_pattern(Pattern::Wildcard, span));

        self.alloc_expr(Expr::Is { scrutinee, pattern }, span)
    }

    fn try_lower_assignment(&mut self, node: &SyntaxNode) -> Option<StmtId> {
        if node.kind() != SyntaxKind::BINARY_EXPR {
            return None;
        }

        // Check for an assignment operator first (avoid allocating expressions early)
        let mut assign_op: Option<Option<AssignOp>> = None;

        for child in node.children_with_tokens() {
            if let rowan::NodeOrToken::Token(token) = child {
                match token.kind() {
                    SyntaxKind::EQUALS => assign_op = Some(None),
                    SyntaxKind::PLUS_EQUALS => assign_op = Some(Some(AssignOp::Add)),
                    SyntaxKind::MINUS_EQUALS => assign_op = Some(Some(AssignOp::Sub)),
                    SyntaxKind::STAR_EQUALS => assign_op = Some(Some(AssignOp::Mul)),
                    SyntaxKind::SLASH_EQUALS => assign_op = Some(Some(AssignOp::Div)),
                    SyntaxKind::PERCENT_EQUALS => assign_op = Some(Some(AssignOp::Mod)),
                    SyntaxKind::AND_EQUALS => assign_op = Some(Some(AssignOp::BitAnd)),
                    SyntaxKind::PIPE_EQUALS => assign_op = Some(Some(AssignOp::BitOr)),
                    SyntaxKind::CARET_EQUALS => assign_op = Some(Some(AssignOp::BitXor)),
                    SyntaxKind::LESS_LESS_EQUALS => assign_op = Some(Some(AssignOp::Shl)),
                    SyntaxKind::GREATER_GREATER_EQUALS => assign_op = Some(Some(AssignOp::Shr)),
                    _ => {}
                }
            }
        }

        let assign_op = assign_op?;

        let mut lhs: Option<ExprId> = None;
        let mut rhs: Option<ExprId> = None;

        for child in node.children_with_tokens() {
            match child {
                rowan::NodeOrToken::Node(n) => {
                    let expr_id = self.lower_expr(&n);
                    if lhs.is_none() {
                        lhs = Some(expr_id);
                    } else {
                        rhs = Some(expr_id);
                    }
                }
                rowan::NodeOrToken::Token(token) => {
                    let span = token.text_range();
                    match token.kind() {
                        SyntaxKind::BIGINT_LITERAL => {
                            let value = self.bigint_literal_value(&token);
                            let expr_id =
                                self.alloc_expr(Expr::Literal(Literal::Bigint(value)), span);
                            if lhs.is_none() {
                                lhs = Some(expr_id);
                            } else {
                                rhs = Some(expr_id);
                            }
                        }
                        SyntaxKind::INTEGER_LITERAL => {
                            let value = self.int_literal_value(&token);
                            let expr_id = self.alloc_expr(Expr::Literal(Literal::Int(value)), span);
                            if lhs.is_none() {
                                lhs = Some(expr_id);
                            } else {
                                rhs = Some(expr_id);
                            }
                        }
                        k if is_ident_token(k) => {
                            let text = token.text();
                            let expr_id = match text {
                                "true" => self.alloc_expr(Expr::Literal(Literal::Bool(true)), span),
                                "false" => {
                                    self.alloc_expr(Expr::Literal(Literal::Bool(false)), span)
                                }
                                "null" => self.alloc_expr(Expr::Null, span),
                                _ => self.alloc_expr(Expr::Path(vec![Name::new(text)]), span),
                            };
                            if lhs.is_none() {
                                lhs = Some(expr_id);
                            } else {
                                rhs = Some(expr_id);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        let target = lhs.unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.span_range()));
        let value = rhs.unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.span_range()));

        let stmt = match assign_op {
            None => Stmt::Assign { target, value },
            Some(op) => Stmt::AssignOp { target, op, value },
        };

        Some(self.alloc_stmt(stmt, node.span_range()))
    }

    fn lower_unary_expr(&mut self, node: &SyntaxNode) -> ExprId {
        let mut op = None;
        let mut operand = None;
        let mut double_op = false;
        // Set when the prefix operator is `~` (bitwise NOT). Desugared below
        // into `-x - 1` (two's-complement complement) rather than a dedicated
        // `UnaryOp` variant, so it reuses the existing, correct `Neg`/`Sub`
        // type rules and VM opcodes without a new operator in the pipeline.
        let mut bit_not = false;
        // Value of an `INTEGER_LITERAL` token seen *directly* in this
        // `UNARY_EXPR` (not via a child node like a parenthesized expr).
        let mut direct_int_lit: Option<i64> = None;

        for elem in node.children_with_tokens() {
            match elem {
                rowan::NodeOrToken::Node(child_node) => {
                    operand = Some(self.lower_expr(&child_node));
                }
                rowan::NodeOrToken::Token(token) => {
                    let span = token.text_range();
                    match token.kind() {
                        SyntaxKind::NOT => op = Some(UnaryOp::Not),
                        SyntaxKind::TILDE => bit_not = true,
                        SyntaxKind::MINUS => op = Some(UnaryOp::Neg),
                        SyntaxKind::MINUS_MINUS => {
                            op = Some(UnaryOp::Neg);
                            double_op = true;
                        }
                        SyntaxKind::BIGINT_LITERAL => {
                            let value = self.bigint_literal_value(&token);
                            operand =
                                Some(self.alloc_expr(Expr::Literal(Literal::Bigint(value)), span));
                        }
                        SyntaxKind::INTEGER_LITERAL => {
                            let value = self.int_literal_value(&token);
                            direct_int_lit = Some(value);
                            operand =
                                Some(self.alloc_expr(Expr::Literal(Literal::Int(value)), span));
                        }
                        SyntaxKind::FLOAT_LITERAL => {
                            operand = Some(self.alloc_expr(
                                Expr::Literal(Literal::Float(num_lit::normalize_float_literal(
                                    token.text(),
                                ))),
                                span,
                            ));
                        }
                        k if is_ident_token(k) => {
                            let text = token.text();
                            let expr_id = match text {
                                "true" => self.alloc_expr(Expr::Literal(Literal::Bool(true)), span),
                                "false" => {
                                    self.alloc_expr(Expr::Literal(Literal::Bool(false)), span)
                                }
                                "null" => self.alloc_expr(Expr::Null, span),
                                _ => self.alloc_expr(Expr::Path(vec![Name::new(text)]), span),
                            };
                            operand = Some(expr_id);
                        }
                        _ => {}
                    }
                }
            }
        }

        let expr = operand.unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.span_range()));

        // Bitwise NOT: desugar `~x` into `-x - 1`. Two's-complement complement
        // is `~x == -x - 1`, correct for every BAML int (63-bit signed) whose
        // negation is representable. Lowering to existing `Neg`/`Sub` keeps `~`
        // out of the type checker and VM as a distinct operator while producing
        // the correct value; the operand is evaluated exactly once. The lone
        // corner is `~INT_MIN`: its result (`INT_MAX`) is representable, but the
        // intermediate `-INT_MIN` overflows the range, so it throws/errors like
        // any other `-INT_MIN` — an inherent limitation of negating INT_MIN
        // here, not a silent wrong answer (the bug this fix removes).
        if bit_not {
            let span = node.span_range();
            // Every node built here is compiler-generated: the user wrote `~`,
            // which desugars away entirely, so — unlike the backtick/tagged
            // desugarings, whose outer `Template` still maps 1:1 to user syntax
            // — no surviving node corresponds to the source. Mark all of them
            // synthetic (so tooling like inlay hints skips them) and restore the
            // flag afterward. Only the operand `x`, lowered above, is user code
            // and keeps its real, non-synthetic id.
            let prev_synth = std::mem::replace(&mut self.synthesizing, true);
            let neg = self.alloc_expr(
                Expr::Unary {
                    op: UnaryOp::Neg,
                    expr,
                },
                span,
            );
            let one = self.alloc_expr(Expr::Literal(Literal::Int(1)), span);
            let result = self.alloc_expr(
                Expr::Binary {
                    op: BinaryOp::Sub,
                    lhs: neg,
                    rhs: one,
                },
                span,
            );
            self.synthesizing = prev_synth;
            return result;
        }

        let Some(op) = op else {
            return expr;
        };

        // `-<int literal>` whose magnitude exceeds INT_MAX is folded into a
        // single negative literal here, so the out-of-range positive
        // intermediate `+v` (which can't be a valid `int` and would panic the
        // VM at load) is never created. The classic case is INT_MIN, written
        // `-4611686018427387904` (= -2^62): `+2^62` is not a valid `int`
        // literal, but the negative literal is — mirroring the i64::MIN literal
        // rule in Rust/Java/C#. In-range negative literals (`-42`) keep the
        // ordinary `Neg(literal)` form and are folded by the MIR optimizer as
        // before, and a bare `+2^62` stays rejected.
        //
        // Gated on the `INTEGER_LITERAL` token seen *directly* in this
        // `UNARY_EXPR`: a parenthesized `-(2^62)` lowers its operand through a
        // child node (not this token), so `direct_int_lit` is `None` and it is
        // correctly NOT treated as a negative literal (the `+2^62` is rejected).
        // `INT_MAX == bex_vm_types::Value::INT_MAX == i64::MAX >> 1`.
        if op == UnaryOp::Neg
            && !double_op
            && let Some(v) = direct_int_lit
            && v > (i64::MAX >> 1)
        {
            return self.alloc_expr(
                Expr::Literal(Literal::Int(v.wrapping_neg())),
                node.span_range(),
            );
        }

        let result = self.alloc_expr(Expr::Unary { op, expr }, node.span_range());

        if double_op {
            self.alloc_expr(Expr::Unary { op, expr: result }, node.span_range())
        } else {
            result
        }
    }

    fn lower_if_expr(&mut self, node: &SyntaxNode) -> ExprId {
        // Collect sub-expressions from both child nodes and bare tokens
        let mut sub_exprs = Vec::new();
        for elem in node.children_with_tokens() {
            match elem {
                rowan::NodeOrToken::Node(child) => {
                    sub_exprs.push(self.lower_expr(&child));
                }
                rowan::NodeOrToken::Token(token) => {
                    if let Some(expr_id) = self.try_lower_bare_token(&token) {
                        sub_exprs.push(expr_id);
                    }
                }
            }
        }

        let condition = sub_exprs
            .first()
            .copied()
            .unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.span_range()));

        let then_branch = sub_exprs
            .get(1)
            .copied()
            .unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.span_range()));

        let else_branch = sub_exprs.get(2).copied();

        self.alloc_expr(
            Expr::If {
                condition,
                then_branch,
                else_branch,
            },
            node.span_range(),
        )
    }

    fn lower_if_let_expr(&mut self, node: &SyntaxNode) -> ExprId {
        // CST shape: `if let PATTERN = SCRUTINEE THEN_BLOCK (else (BLOCK | IF_EXPR | IF_LET_EXPR))?`
        //
        // Children we care about, in source order:
        //   1. PATTERN
        //   2. scrutinee expression (may be a bare WORD/literal token, not a node)
        //   3. then-branch (BLOCK_EXPR)
        //   4. else-branch (BLOCK_EXPR | IF_EXPR | IF_LET_EXPR), optional
        //
        // The scrutinee can appear as either a wrapper node (PATH_EXPR,
        // BINARY_EXPR, …) or as a bare token (single identifier / literal), so
        // we mirror `lower_if_expr` and walk children-with-tokens.
        self.warn_direct_const_introducers(node);

        let mut pattern = None;
        let mut exprs: Vec<ExprId> = Vec::new();
        for elem in node.children_with_tokens() {
            match elem {
                rowan::NodeOrToken::Node(child) => {
                    if child.kind() == SyntaxKind::PATTERN {
                        if pattern.is_none() {
                            pattern = Some(self.lower_pattern(&child));
                        }
                    } else {
                        exprs.push(self.lower_expr(&child));
                    }
                }
                rowan::NodeOrToken::Token(token) => {
                    if let Some(expr_id) = self.try_lower_bare_token(&token) {
                        exprs.push(expr_id);
                    }
                }
            }
        }

        let pattern =
            pattern.unwrap_or_else(|| self.alloc_pattern(Pattern::Wildcard, node.span_range()));
        let scrutinee = exprs
            .first()
            .copied()
            .unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.span_range()));
        let then_branch = exprs
            .get(1)
            .copied()
            .unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.span_range()));
        let else_branch = exprs.get(2).copied();

        self.alloc_expr(
            Expr::IfLet {
                pattern,
                scrutinee,
                then_branch,
                else_branch,
            },
            node.span_range(),
        )
    }

    fn lower_while_let_stmt(&mut self, node: &SyntaxNode) -> StmtId {
        // CST shape: `while let PATTERN = SCRUTINEE BODY_BLOCK`.
        // The first PATTERN node is the pattern; remaining children are
        // [0]=scrutinee, [1]=body (mirrors `lower_if_let_expr` minus else).
        self.warn_direct_const_introducers(node);

        let mut pattern = None;
        let mut exprs: Vec<ExprId> = Vec::new();
        for elem in node.children_with_tokens() {
            match elem {
                rowan::NodeOrToken::Node(child) => {
                    if child.kind() == SyntaxKind::PATTERN {
                        if pattern.is_none() {
                            pattern = Some(self.lower_pattern(&child));
                        }
                    } else {
                        exprs.push(self.lower_expr(&child));
                    }
                }
                rowan::NodeOrToken::Token(token) => {
                    if let Some(expr_id) = self.try_lower_bare_token(&token) {
                        exprs.push(expr_id);
                    }
                }
            }
        }

        let pattern =
            pattern.unwrap_or_else(|| self.alloc_pattern(Pattern::Wildcard, node.span_range()));
        let scrutinee = exprs
            .first()
            .copied()
            .unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.span_range()));
        let body = exprs
            .get(1)
            .copied()
            .unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.span_range()));

        self.alloc_stmt(
            Stmt::WhileLet {
                pattern,
                scrutinee,
                body,
            },
            node.span_range(),
        )
    }

    fn lower_match_expr(&mut self, node: &SyntaxNode) -> ExprId {
        let mut scrutinee = None;
        let mut scrutinee_type = None;
        let mut arm_ids = Vec::new();

        for elem in node.children_with_tokens() {
            match elem {
                rowan::NodeOrToken::Node(child) => match child.kind() {
                    SyntaxKind::MATCH_ARM => {
                        let arm_id = self.lower_match_arm(&child);
                        arm_ids.push(arm_id);
                    }
                    SyntaxKind::TYPE_EXPR => {
                        if let Some(type_expr) =
                            baml_compiler_syntax::ast::TypeExpr::cast(child.clone())
                        {
                            let span = child.span_range();
                            let ty = crate::lower_type_expr::lower_type_expr_node(
                                &type_expr,
                                &mut self.diags,
                            );
                            scrutinee_type = Some(self.alloc_type_annot(ty, span));
                        }
                    }
                    _ => {
                        if scrutinee.is_none() {
                            scrutinee = Some(self.lower_expr(&child));
                        }
                    }
                },
                rowan::NodeOrToken::Token(token) => {
                    if scrutinee.is_none() {
                        let span = token.text_range();
                        match token.kind() {
                            SyntaxKind::BIGINT_LITERAL => {
                                let value = self.bigint_literal_value(&token);
                                scrutinee = Some(
                                    self.alloc_expr(Expr::Literal(Literal::Bigint(value)), span),
                                );
                            }
                            SyntaxKind::INTEGER_LITERAL => {
                                let value = self.int_literal_value(&token);
                                scrutinee =
                                    Some(self.alloc_expr(Expr::Literal(Literal::Int(value)), span));
                            }
                            k if is_ident_token(k) => {
                                let text = token.text();
                                let e = match text {
                                    "true" => Expr::Literal(Literal::Bool(true)),
                                    "false" => Expr::Literal(Literal::Bool(false)),
                                    "null" => Expr::Null,
                                    _ => Expr::Path(vec![Name::new(text)]),
                                };
                                scrutinee = Some(self.alloc_expr(e, span));
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        let scrutinee =
            scrutinee.unwrap_or_else(|| self.alloc_expr(Expr::Missing, TextRange::default()));

        self.alloc_expr(
            Expr::Match {
                scrutinee,
                scrutinee_type,
                arms: arm_ids,
            },
            node.span_range(),
        )
    }

    fn lower_match_arm(&mut self, node: &SyntaxNode) -> MatchArmId {
        let arm_span = node.span_range();
        let mut pattern = None;
        let mut guard = None;
        let mut body = None;
        let mut seen_fat_arrow = false;

        for elem in node.children_with_tokens() {
            match elem {
                rowan::NodeOrToken::Node(child) => match child.kind() {
                    SyntaxKind::PATTERN => {
                        pattern = Some(self.lower_pattern(&child));
                    }
                    SyntaxKind::MATCH_GUARD => {
                        if let Some(expr_node) = child.children().next() {
                            guard = Some(self.lower_expr(&expr_node));
                        } else {
                            for tok in child.children_with_tokens() {
                                if let rowan::NodeOrToken::Token(t) = tok {
                                    match t.kind() {
                                        SyntaxKind::KW_IF => continue,
                                        k if is_ident_token(k) => {
                                            let text = t.text();
                                            let range = t.text_range();
                                            let e = match text {
                                                "true" => Expr::Literal(Literal::Bool(true)),
                                                "false" => Expr::Literal(Literal::Bool(false)),
                                                "null" => Expr::Null,
                                                _ => Expr::Path(vec![Name::new(text)]),
                                            };
                                            guard = Some(self.alloc_expr(e, range));
                                            break;
                                        }
                                        SyntaxKind::BIGINT_LITERAL => {
                                            let value = self.bigint_literal_value(&t);
                                            guard = Some(self.alloc_expr(
                                                Expr::Literal(Literal::Bigint(value)),
                                                t.text_range(),
                                            ));
                                            break;
                                        }
                                        SyntaxKind::INTEGER_LITERAL => {
                                            let value = self.int_literal_value(&t);
                                            guard = Some(self.alloc_expr(
                                                Expr::Literal(Literal::Int(value)),
                                                t.text_range(),
                                            ));
                                            break;
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                    }
                    SyntaxKind::STRING_LITERAL | SyntaxKind::RAW_STRING_LITERAL
                        if seen_fat_arrow && body.is_none() =>
                    {
                        body = Some(self.lower_string_literal(&child));
                    }
                    _ => {
                        if seen_fat_arrow && body.is_none() {
                            body = Some(self.lower_expr(&child));
                        }
                    }
                },
                rowan::NodeOrToken::Token(token) => match token.kind() {
                    SyntaxKind::FAT_ARROW => {
                        seen_fat_arrow = true;
                    }
                    SyntaxKind::BIGINT_LITERAL if seen_fat_arrow && body.is_none() => {
                        let value = self.bigint_literal_value(&token);
                        body =
                            Some(self.alloc_expr(
                                Expr::Literal(Literal::Bigint(value)),
                                token.text_range(),
                            ));
                    }
                    SyntaxKind::INTEGER_LITERAL if seen_fat_arrow && body.is_none() => {
                        let value = self.int_literal_value(&token);
                        body = Some(
                            self.alloc_expr(Expr::Literal(Literal::Int(value)), token.text_range()),
                        );
                    }
                    SyntaxKind::FLOAT_LITERAL if seen_fat_arrow && body.is_none() => {
                        let text = num_lit::normalize_float_literal(token.text());
                        body =
                            Some(self.alloc_expr(
                                Expr::Literal(Literal::Float(text)),
                                token.text_range(),
                            ));
                    }
                    SyntaxKind::STRING_LITERAL | SyntaxKind::RAW_STRING_LITERAL
                        if seen_fat_arrow && body.is_none() =>
                    {
                        let content = strip_string_delimiters(token.text());
                        body = Some(self.alloc_expr(
                            Expr::Literal(Literal::String(content)),
                            token.text_range(),
                        ));
                    }
                    k if is_ident_token(k) && seen_fat_arrow && body.is_none() => {
                        let text = token.text();
                        let range = token.text_range();
                        let e = match text {
                            "true" => Expr::Literal(Literal::Bool(true)),
                            "false" => Expr::Literal(Literal::Bool(false)),
                            "null" => Expr::Null,
                            _ => Expr::Path(vec![Name::new(text)]),
                        };
                        body = Some(self.alloc_expr(e, range));
                    }
                    _ => {}
                },
            }
        }

        let arm = MatchArm {
            pattern: pattern.unwrap_or_else(|| self.alloc_pattern(Pattern::Wildcard, arm_span)),
            guard,
            body: body.unwrap_or_else(|| self.alloc_expr(Expr::Missing, arm_span)),
        };

        self.alloc_match_arm(arm, arm_span)
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

    /// Entry point: lower a `PATTERN` node to a `PatId`.
    fn lower_pattern(&mut self, node: &SyntaxNode) -> PatId {
        debug_assert_eq!(node.kind(), SyntaxKind::PATTERN);
        match node.children().next() {
            Some(inner) => self.lower_pattern_atom_node(&inner),
            None => self.alloc_pattern(Pattern::Wildcard, node.span_range()),
        }
    }

    /// Dispatch on the kind of an atom-shaped pattern node
    /// (`UNION_PATTERN`, `BINDING_PATTERN`, `WILDCARD_PATTERN`,
    /// `DESTRUCTURE_PATTERN`, `ARRAY_PATTERN`, `TYPE_PATTERN`,
    /// `PAREN_PATTERN`). Returns a fresh `PatId`.
    fn lower_pattern_atom_node(&mut self, node: &SyntaxNode) -> PatId {
        self.warn_direct_const_introducers(node);

        match node.kind() {
            SyntaxKind::UNION_PATTERN => self.lower_union_pattern(node),
            SyntaxKind::BINDING_PATTERN => self.lower_binding_pattern(node),
            SyntaxKind::WILDCARD_PATTERN => {
                self.alloc_pattern(Pattern::Wildcard, node.span_range())
            }
            SyntaxKind::DESTRUCTURE_PATTERN => self.lower_destructure_pattern(node),
            SyntaxKind::ARRAY_PATTERN => self.lower_array_pattern(node),
            SyntaxKind::TYPE_PATTERN => self.lower_type_pattern(node),
            SyntaxKind::PAREN_PATTERN => {
                match node.children().find(|n| n.kind() == SyntaxKind::PATTERN) {
                    Some(inner) => self.lower_pattern(&inner),
                    None => self.alloc_pattern(Pattern::Wildcard, node.span_range()),
                }
            }
            // Defensive: an unexpected node where a pattern atom should be.
            // Lower as wildcard so downstream doesn't crash; the parse error
            // (if any) will surface elsewhere.
            _ => self.alloc_pattern(Pattern::Wildcard, node.span_range()),
        }
    }

    /// Lower a `UNION_PATTERN` node. Children are atom-shaped pattern nodes
    /// separated by `|` tokens. Length-1 unions collapse to the inner pattern.
    fn lower_union_pattern(&mut self, node: &SyntaxNode) -> PatId {
        let parts: Vec<PatId> = node
            .children()
            .map(|child| self.lower_pattern_atom_node(&child))
            .collect();
        match parts.len() {
            0 => self.alloc_pattern(Pattern::Wildcard, node.span_range()),
            1 => parts[0],
            _ => self.alloc_pattern(Pattern::Or(parts), node.span_range()),
        }
    }

    /// Lower a `BINDING_PATTERN` (`let WORD` / `const WORD`). The parser routes
    /// `let _` / `const _` to
    /// `WILDCARD_PATTERN` before it ever reaches here, so the WORD's text is
    /// never `_`. The only defensive case is a malformed `let` without a
    /// following WORD (parse error like `let = 1`), which we recover as
    /// wildcard.
    fn lower_binding_pattern(&mut self, node: &SyntaxNode) -> PatId {
        let name_token = node
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .find(|t| t.kind() == SyntaxKind::WORD);

        if let Some(token) = &name_token
            && token.text() == "const"
        {
            self.diags
                .push(LoweringDiagnostic::ReservedConstBindingName {
                    span: token.text_range(),
                });
        }
        if let Some(token) = &name_token
            && token.text() == "$id"
        {
            self.diags
                .push(LoweringDiagnostic::ReservedRuntimeIdBindingName {
                    span: token.text_range(),
                });
        }

        let name = name_token.map(|t| Name::new(t.text()));

        // The parser folds `: <pattern>` into BINDING_PATTERN as a
        // PATTERN child (any pattern).
        let subpat = node
            .children()
            .find(|c| c.kind() == SyntaxKind::PATTERN)
            .map(|pat_node| self.lower_pattern(&pat_node));

        let pat = match name {
            Some(name) => Pattern::Bind { name, subpat },
            None => Pattern::Wildcard,
        };
        self.alloc_pattern(pat, node.span_range())
    }

    /// Walk a pattern and emit `VoidInNonReturnPosition` for any `Pattern::Type`
    /// whose annotation is `void` (or contains `void` in a wrapper position).
    fn check_pattern_void_in_annotation(&mut self, pat_id: PatId, context: &str) {
        match self.patterns[pat_id].clone() {
            Pattern::Type(ty) => {
                let span = self.source_map.pattern_span(pat_id);
                crate::lower_type_expr::check_void_type(
                    &ty,
                    context.to_string(),
                    span,
                    false,
                    &mut self.diags,
                );
            }
            Pattern::Or(pats) => {
                for p in pats {
                    self.check_pattern_void_in_annotation(p, context);
                }
            }
            Pattern::Class { fields, .. } => {
                for f in fields {
                    self.check_pattern_void_in_annotation(f.pat, context);
                }
            }
            Pattern::Array {
                prefix,
                rest,
                suffix,
                ascription,
            } => {
                for p in prefix.into_iter().chain(suffix) {
                    self.check_pattern_void_in_annotation(p, context);
                }
                if let Some(rest) = rest
                    && let Some(p) = rest.pat
                {
                    self.check_pattern_void_in_annotation(p, context);
                }
                if let Some(ty) = ascription {
                    let span = self.source_map.pattern_span(pat_id);
                    crate::lower_type_expr::check_void_type(
                        &ty,
                        context.to_string(),
                        span,
                        false,
                        &mut self.diags,
                    );
                }
            }
            Pattern::Wildcard => {}
            Pattern::Bind { subpat, .. } => {
                if let Some(sp) = subpat {
                    self.check_pattern_void_in_annotation(sp, context);
                }
            }
        }
    }

    /// Lower a `TYPE_PATTERN`. Normally a `TYPE_EXPR` child is present, but
    /// error-recovered CSTs (e.g. `let x: = 1`) may emit a `TYPE_PATTERN` with
    /// no usable type child — fall back to a wildcard so downstream passes
    /// don't crash on malformed input.
    fn lower_type_pattern(&mut self, node: &SyntaxNode) -> PatId {
        let Some(type_expr) = node
            .children()
            .find_map(baml_compiler_syntax::ast::TypeExpr::cast)
        else {
            return self.alloc_pattern(Pattern::Wildcard, node.span_range());
        };
        let ty = crate::lower_type_expr::lower_type_expr_node(&type_expr, &mut self.diags);
        self.alloc_pattern(Pattern::Type(ty), node.span_range())
    }

    /// Lower a `DESTRUCTURE_PATTERN` (`(let|const)? PATH ('<' types '>')? '{' field_list '}'`).
    fn lower_destructure_pattern(&mut self, node: &SyntaxNode) -> PatId {
        // Path tokens live between the optional binding introducer
        // (`KW_LET`/`KW_CONST`) and either
        // `GENERIC_ARGS`, `TYPE_ARGS`, or `L_BRACE`.
        // Collect WORD tokens in that range, ignoring DOTs.
        let mut class: Vec<Name> = Vec::new();
        for elem in node.children_with_tokens() {
            match elem {
                rowan::NodeOrToken::Token(t) => match t.kind() {
                    SyntaxKind::WORD => class.push(Name::new(t.text())),
                    SyntaxKind::LESS => break,
                    SyntaxKind::L_BRACE => break,
                    _ => {}
                },
                rowan::NodeOrToken::Node(n)
                    if n.kind() == SyntaxKind::GENERIC_ARGS
                        || n.kind() == SyntaxKind::TYPE_ARGS =>
                {
                    break;
                }
                rowan::NodeOrToken::Node(_) => {}
            }
        }

        let args_node = node
            .children()
            .find(|n| n.kind() == SyntaxKind::GENERIC_ARGS || n.kind() == SyntaxKind::TYPE_ARGS);

        let generic_args: Vec<TypeExpr> = args_node
            .as_ref()
            .into_iter()
            .flat_map(rowan::SyntaxNode::children)
            .filter(|n| n.kind() == SyntaxKind::TYPE_EXPR)
            .filter_map(baml_compiler_syntax::ast::TypeExpr::cast)
            .map(|te| crate::lower_type_expr::lower_type_expr_node(&te, &mut self.diags))
            .collect();

        let associated_type_bindings = args_node
            .into_iter()
            .filter(|args_node| args_node.kind() == SyntaxKind::TYPE_ARGS)
            .flat_map(|args_node| args_node.children())
            .filter_map(baml_compiler_syntax::ast::AssociatedTypeDecl::cast)
            .filter_map(|binding| {
                crate::lower_type_expr::lower_associated_type_binding(&binding, &mut self.diags)
            })
            .collect();

        let fields: Vec<FieldPat> = node
            .children()
            .filter(|n| n.kind() == SyntaxKind::FIELD_PATTERN)
            .map(|f| self.lower_field_pattern(&f))
            .collect();

        self.alloc_pattern(
            Pattern::Class {
                class,
                generic_args,
                associated_type_bindings,
                fields,
            },
            node.span_range(),
        )
    }

    /// Lower a `FIELD_PATTERN`. Shorthand `{ f }` synthesises a
    /// `Bind { name: f }` so `FieldPat.pat` is always populated.
    fn lower_field_pattern(&mut self, node: &SyntaxNode) -> FieldPat {
        let field_token = node
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .find(|t| t.kind() == SyntaxKind::WORD);
        let field_span = field_token
            .as_ref()
            .map(rowan::SyntaxToken::text_range)
            .unwrap_or_else(|| node.span_range());
        let field_name = field_token
            .map(|t| Name::new(t.text()))
            .unwrap_or_else(|| Name::new("_"));

        let value_pattern = node.children().find(|n| n.kind() == SyntaxKind::PATTERN);

        let pat = match value_pattern {
            Some(child) => self.lower_pattern(&child),
            None => {
                // Shorthand `{ f }` → bind to a local of the same name. `_`
                // canonicalises to `Wildcard`, same rule as elsewhere.
                // A shorthand binding named `$id` would be silently dead
                // (reads hit the runtime-identity special form first) —
                // reject it like every other `$id` binding site.
                if field_name.as_str() == "$id" {
                    self.diags
                        .push(LoweringDiagnostic::ReservedRuntimeIdBindingName {
                            span: field_span,
                        });
                }
                let synth = if field_name.as_str() == "_" {
                    Pattern::Wildcard
                } else {
                    Pattern::Bind {
                        name: field_name.clone(),
                        subpat: None,
                    }
                };
                self.alloc_pattern(synth, node.span_range())
            }
        };

        FieldPat {
            field: field_name,
            field_span,
            pat,
        }
    }

    fn lower_array_pattern(&mut self, node: &SyntaxNode) -> PatId {
        let mut prefix = Vec::new();
        let mut rest = None;
        let mut suffix = Vec::new();
        let mut seen_rest = false;

        for elem in node
            .children()
            .filter(|n| n.kind() == SyntaxKind::ARRAY_PATTERN_ELEMENT)
        {
            let is_rest = elem.children_with_tokens().any(|c| {
                matches!(
                    c,
                    rowan::NodeOrToken::Token(t) if t.kind() == SyntaxKind::DOT_DOT
                )
            });
            let pat = elem
                .children()
                .find(|n| n.kind() == SyntaxKind::PATTERN)
                .map(|p| self.lower_pattern(&p));

            if is_rest {
                seen_rest = true;
                if rest.is_none() {
                    rest = Some(ArrayRestPat { pat });
                }
            } else if let Some(pat) = pat {
                if seen_rest {
                    suffix.push(pat);
                } else {
                    prefix.push(pat);
                }
            }
        }

        // Parser folds optional `: T` ascription into ARRAY_PATTERN as
        // a TYPE_EXPR child.
        let ascription = node
            .children()
            .find_map(baml_compiler_syntax::ast::TypeExpr::cast)
            .map(|type_expr| {
                crate::lower_type_expr::lower_type_expr_node(&type_expr, &mut self.diags)
            });

        self.alloc_pattern(
            Pattern::Array {
                prefix,
                rest,
                suffix,
                ascription,
            },
            node.span_range(),
        )
    }

    fn lower_catch_expr(&mut self, node: &SyntaxNode) -> ExprId {
        use baml_compiler_syntax::SyntaxKind;

        let mut base = None;
        let mut clauses = Vec::new();

        for child in node.children() {
            match child.kind() {
                SyntaxKind::CATCH_CLAUSE => clauses.push(self.lower_catch_clause(&child)),
                _ if base.is_none() => {
                    base = Some(self.lower_expr(&child));
                }
                _ => {}
            }
        }

        let base = base.unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.span_range()));
        self.alloc_expr(Expr::Catch { base, clauses }, node.span_range())
    }

    fn lower_catch_clause(&mut self, node: &SyntaxNode) -> CatchClause {
        use baml_compiler_syntax::SyntaxKind;

        let mut kind = CatchClauseKind::Catch;
        let mut binding = None;
        let mut stack_trace_binding = None;
        let mut arms = Vec::new();

        for elem in node.children_with_tokens() {
            match elem {
                rowan::NodeOrToken::Token(token) => match token.kind() {
                    SyntaxKind::KW_CATCH => kind = CatchClauseKind::Catch,
                    SyntaxKind::KW_CATCH_ALL => kind = CatchClauseKind::CatchAll,
                    SyntaxKind::KW_CATCH_ALL_PANICS => {
                        kind = CatchClauseKind::CatchAllPanics;
                    }
                    _ => {}
                },
                rowan::NodeOrToken::Node(child) => match child.kind() {
                    SyntaxKind::CATCH_BINDING => {
                        let name = child
                            .children_with_tokens()
                            .find_map(|t| match t {
                                rowan::NodeOrToken::Token(tok)
                                    if tok.kind() == SyntaxKind::WORD =>
                                {
                                    Some(Name::new(tok.text()))
                                }
                                _ => None,
                            })
                            .unwrap_or_else(|| Name::new("_"));
                        binding = Some(self.alloc_pattern(
                            Pattern::Bind { name, subpat: None },
                            child.span_range(),
                        ));
                    }
                    SyntaxKind::CATCH_STACK_TRACE_BINDING => {
                        // Extract the identifier name from the node.
                        let name = child
                            .children_with_tokens()
                            .find_map(|t| match t {
                                rowan::NodeOrToken::Token(tok)
                                    if tok.kind() == SyntaxKind::WORD =>
                                {
                                    Some(Name::new(tok.text()))
                                }
                                _ => None,
                            })
                            .unwrap_or_else(|| Name::new("_"));
                        stack_trace_binding = Some(self.alloc_pattern(
                            Pattern::Bind { name, subpat: None },
                            child.span_range(),
                        ));
                    }
                    SyntaxKind::CATCH_ARM => {
                        let arm = self.lower_catch_arm(&child);
                        arms.push(arm);
                    }
                    _ => {}
                },
            }
        }

        CatchClause {
            kind,
            binding: binding
                .unwrap_or_else(|| self.alloc_pattern(Pattern::Wildcard, node.span_range())),
            stack_trace_binding,
            arms,
        }
    }

    fn lower_catch_arm(&mut self, node: &SyntaxNode) -> CatchArmId {
        use baml_compiler_syntax::SyntaxKind;

        let mut pattern = None;
        let mut body = None;
        let mut seen_fat_arrow = false;

        for elem in node.children_with_tokens() {
            match elem {
                rowan::NodeOrToken::Node(child) => match child.kind() {
                    SyntaxKind::PATTERN => {
                        pattern = Some(self.lower_pattern(&child));
                    }
                    SyntaxKind::STRING_LITERAL | SyntaxKind::RAW_STRING_LITERAL
                        if seen_fat_arrow && body.is_none() =>
                    {
                        body = Some(self.lower_string_literal(&child));
                    }
                    _ if seen_fat_arrow && body.is_none() => {
                        body = Some(self.lower_expr(&child));
                    }
                    _ => {}
                },
                rowan::NodeOrToken::Token(token) => match token.kind() {
                    SyntaxKind::FAT_ARROW => seen_fat_arrow = true,
                    SyntaxKind::BIGINT_LITERAL if seen_fat_arrow && body.is_none() => {
                        let value = self.bigint_literal_value(&token);
                        body =
                            Some(self.alloc_expr(
                                Expr::Literal(Literal::Bigint(value)),
                                token.text_range(),
                            ));
                    }
                    SyntaxKind::INTEGER_LITERAL if seen_fat_arrow && body.is_none() => {
                        let value = self.int_literal_value(&token);
                        body = Some(
                            self.alloc_expr(Expr::Literal(Literal::Int(value)), token.text_range()),
                        );
                    }
                    SyntaxKind::FLOAT_LITERAL if seen_fat_arrow && body.is_none() => {
                        body = Some(self.alloc_expr(
                            Expr::Literal(Literal::Float(num_lit::normalize_float_literal(
                                token.text(),
                            ))),
                            token.text_range(),
                        ));
                    }
                    SyntaxKind::STRING_LITERAL | SyntaxKind::RAW_STRING_LITERAL
                        if seen_fat_arrow && body.is_none() =>
                    {
                        body = Some(self.alloc_expr(
                            Expr::Literal(Literal::String(strip_string_delimiters(token.text()))),
                            token.text_range(),
                        ));
                    }
                    k if is_ident_token(k) && seen_fat_arrow && body.is_none() => {
                        let expr = match token.text() {
                            "true" => Expr::Literal(Literal::Bool(true)),
                            "false" => Expr::Literal(Literal::Bool(false)),
                            "null" => Expr::Null,
                            _ => Expr::Path(vec![Name::new(token.text())]),
                        };
                        body = Some(self.alloc_expr(expr, token.text_range()));
                    }
                    _ => {}
                },
            }
        }

        let pattern = match pattern {
            Some(pattern) => pattern,
            None => self.alloc_pattern(Pattern::Wildcard, node.span_range()),
        };
        let body = match body {
            Some(body) => body,
            None => self.alloc_expr(Expr::Missing, node.span_range()),
        };

        self.alloc_catch_arm(CatchArm { pattern, body }, node.span_range())
    }

    fn lower_throw_expr(&mut self, node: &SyntaxNode) -> ExprId {
        let value = if let Some(child) = node.children().next() {
            self.lower_expr(&child)
        } else {
            self.lower_throw_value_token(node)
                .unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.span_range()))
        };
        self.alloc_expr(Expr::Throw { value }, node.span_range())
    }

    fn lower_throw_stmt(&mut self, node: &SyntaxNode) -> StmtId {
        use baml_compiler_syntax::SyntaxKind;

        let expr_child = node.children().find(|c| {
            matches!(
                c.kind(),
                SyntaxKind::THROW_EXPR
                    | SyntaxKind::CATCH_EXPR
                    | SyntaxKind::EXPR
                    | SyntaxKind::BINARY_EXPR
                    | SyntaxKind::UNARY_EXPR
                    | SyntaxKind::CALL_EXPR
                    | SyntaxKind::PATH_EXPR
                    | SyntaxKind::FIELD_ACCESS_EXPR
                    | SyntaxKind::UPCAST_EXPR
                    | SyntaxKind::ENV_ACCESS_EXPR
                    | SyntaxKind::INDEX_EXPR
                    | SyntaxKind::IF_EXPR
                    | SyntaxKind::IF_LET_EXPR
                    | SyntaxKind::MATCH_EXPR
                    | SyntaxKind::BLOCK_EXPR
                    | SyntaxKind::PAREN_EXPR
                    | SyntaxKind::STRING_LITERAL
                    | SyntaxKind::RAW_STRING_LITERAL
                    | SyntaxKind::OBJECT_LITERAL
                    | SyntaxKind::ARRAY_LITERAL
                    | SyntaxKind::MAP_LITERAL
            )
        });

        if let Some(child) = expr_child.clone() {
            if child.kind() != SyntaxKind::THROW_EXPR {
                let expr_id = self.lower_expr(&child);
                return self.alloc_stmt(Stmt::Expr(expr_id), node.span_range());
            }
        }

        let value = expr_child
            .filter(|c| c.kind() == SyntaxKind::THROW_EXPR)
            .map(|throw_expr_node| {
                if let Some(child) = throw_expr_node.children().next() {
                    self.lower_expr(&child)
                } else {
                    self.lower_throw_value_token(&throw_expr_node)
                        .unwrap_or_else(|| {
                            self.alloc_expr(Expr::Missing, throw_expr_node.span_range())
                        })
                }
            })
            .unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.span_range()));

        self.alloc_stmt(Stmt::Throw { value }, node.span_range())
    }

    fn lower_throw_value_token(&mut self, node: &SyntaxNode) -> Option<ExprId> {
        use baml_compiler_syntax::SyntaxKind;

        for token in node
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
        {
            match token.kind() {
                SyntaxKind::KW_THROW => continue,
                SyntaxKind::BIGINT_LITERAL => {
                    let value = self.bigint_literal_value(&token);
                    return Some(
                        self.alloc_expr(Expr::Literal(Literal::Bigint(value)), token.text_range()),
                    );
                }
                SyntaxKind::INTEGER_LITERAL => {
                    let value = self.int_literal_value(&token);
                    return Some(
                        self.alloc_expr(Expr::Literal(Literal::Int(value)), token.text_range()),
                    );
                }
                SyntaxKind::FLOAT_LITERAL => {
                    return Some(self.alloc_expr(
                        Expr::Literal(Literal::Float(num_lit::normalize_float_literal(
                            token.text(),
                        ))),
                        token.text_range(),
                    ));
                }
                SyntaxKind::STRING_LITERAL | SyntaxKind::RAW_STRING_LITERAL => {
                    return Some(self.alloc_expr(
                        Expr::Literal(Literal::String(strip_string_delimiters(token.text()))),
                        token.text_range(),
                    ));
                }
                k if is_ident_token(k) => {
                    let expr = match token.text() {
                        "true" => Expr::Literal(Literal::Bool(true)),
                        "false" => Expr::Literal(Literal::Bool(false)),
                        "null" => Expr::Null,
                        _ => Expr::Path(vec![Name::new(token.text())]),
                    };
                    return Some(self.alloc_expr(expr, token.text_range()));
                }
                _ => {}
            }
        }
        None
    }

    fn lower_call_expr(&mut self, node: &SyntaxNode) -> ExprId {
        // CALL_EXPR structure: callee expr node (or WORD token), then CALL_ARGS node
        let callee_node = node.children().find(|n| n.kind() != SyntaxKind::CALL_ARGS);

        // Extract explicit type arguments from the callee node.
        //
        // Two parser shapes produce GENERIC_ARGS we should treat as call-site
        // type-args:
        //
        //   1. `foo<T, U>(args)` — GENERIC_ARGS is a direct child of the
        //      callee PATH_EXPR.
        //   2. `Box<Secret>.from_json(args)` — GENERIC_ARGS is on the
        //      receiver-type PATH_EXPR nested inside the FIELD_ACCESS_EXPR
        //      callee.  Treat the receiver's type-args as the call's
        //      type-args so the BEP-039 type-arg channel seeds the static
        //      method's frame correctly (e.g. `Box.from_json` sees
        //      `T = Secret`).
        let callee_generic_args = callee_node.as_ref().and_then(find_callee_generic_args);
        let type_args: Vec<TypeExpr> = callee_generic_args
            .as_ref()
            .map(|ga| Self::lower_generic_args_node(ga, &mut self.diags))
            .unwrap_or_default();
        // Mark EVERY `GENERIC_ARGS` node in the callee subtree as consumed, so
        // lowering the callee/receiver below does not wrap any of them into an
        // `Expr::GenericApply`. `foo<int>(x)` keeps its callee a plain path (the
        // `<int>` lives on the `Call`); for `Container<int>.method<U>(x)` the
        // call uses the method-level `<U>` as its type args while the receiver
        // `<int>` fills the class's generic params (BEP-039) — neither is a
        // value-position instantiation, so both must be suppressed here.
        if let Some(n) = &callee_node {
            for ga in n
                .descendants()
                .filter(|d| d.kind() == SyntaxKind::GENERIC_ARGS)
            {
                self.consumed_generic_args.insert(ga.text_range());
            }
        }

        let callee = if let Some(n) = callee_node {
            self.lower_expr_in_chain(&n)
        } else {
            // No callee node - check for an identifier token (simple function name)
            let word_token = node
                .children_with_tokens()
                .filter_map(rowan::NodeOrToken::into_token)
                .find(|t| is_ident_token(t.kind()));

            if let Some(token) = word_token {
                self.alloc_expr(
                    Expr::Path(vec![Name::new(token.text())]),
                    token.text_range(),
                )
            } else {
                self.alloc_expr(Expr::Missing, node.span_range())
            }
        };

        let lowered_args = node
            .children()
            .find(|n| n.kind() == SyntaxKind::CALL_ARGS)
            .map(|args_node| self.lower_call_args_node(&args_node))
            .unwrap_or_default();
        let (args, label_spans) = Self::finalize_call_args(lowered_args);

        let id = self.alloc_expr(
            Expr::Call {
                callee,
                type_args,
                args,
            },
            node.span_range(),
        );
        self.record_call_arg_label_spans(id, label_spans);
        if self.needs_chain_wrap.remove(&callee) {
            self.needs_chain_wrap.insert(id);
        }
        id
    }

    fn finalize_call_args(
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

    fn record_call_arg_label_spans(
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

    fn lower_call_args_node(
        &mut self,
        args_node: &SyntaxNode,
    ) -> Vec<(CallArg, Option<TextRange>)> {
        args_node
            .children()
            .filter(|n| n.kind() == SyntaxKind::CALL_ARG)
            .filter_map(|node| self.lower_call_arg_node(&node))
            .collect()
    }

    fn lower_call_arg_node(&mut self, node: &SyntaxNode) -> Option<(CallArg, Option<TextRange>)> {
        let cst_arg = baml_compiler_syntax::ast::CallArg::cast(node.clone())?;
        let label_token = cst_arg.label();
        let label = label_token.as_ref().map(|token| Name::new(token.text()));
        let label_span = label_token.as_ref().map(rowan::SyntaxToken::text_range);

        let expr = if let Some(expr_node) = cst_arg.expr_syntax() {
            self.lower_expr(&expr_node)
        } else {
            let expr_search_start = label_token.as_ref().map(|label_token| {
                node.children_with_tokens()
                    .filter_map(rowan::NodeOrToken::into_token)
                    .filter(|token| !token.kind().is_trivia())
                    .filter(|token| token.text_range().start() >= label_token.text_range().end())
                    .find(|token| token.kind() == SyntaxKind::EQUALS)
                    .map_or(label_token.text_range().end(), |token| {
                        token.text_range().end()
                    })
            });
            let expr_token = node
                .children_with_tokens()
                .filter_map(rowan::NodeOrToken::into_token)
                .filter(|token| !token.kind().is_trivia())
                .filter(|token| {
                    expr_search_start
                        .map(|start| token.text_range().start() >= start)
                        .unwrap_or(true)
                })
                .find(|token| token.kind() != SyntaxKind::COMMA)?;
            let expr = lower_bare_token_expr(self, &expr_token);
            self.alloc_expr(expr, expr_token.text_range())
        };

        Some((CallArg { label, expr }, label_span))
    }

    /// Lower the `TYPE_EXPR` children of a `GENERIC_ARGS` node to `TypeExpr`s.
    fn lower_generic_args_node(
        ga: &SyntaxNode,
        diags: &mut Vec<LoweringDiagnostic>,
    ) -> Vec<TypeExpr> {
        ga.children()
            .filter(|n| n.kind() == SyntaxKind::TYPE_EXPR)
            .filter_map(baml_compiler_syntax::ast::TypeExpr::cast)
            .map(|te| crate::lower_type_expr::lower_type_expr_node(&te, diags))
            .collect()
    }

    /// If `node` has a direct, unconsumed `GENERIC_ARGS` child, wrap `base` in an
    /// `Expr::GenericApply` carrying its type args; otherwise return `base`.
    /// `range` spans the whole instantiation (`foo<int>`).
    fn wrap_generic_apply(&mut self, node: &SyntaxNode, base: ExprId, range: TextRange) -> ExprId {
        let Some(ga) = node
            .children()
            .find(|n| n.kind() == SyntaxKind::GENERIC_ARGS)
        else {
            return base;
        };
        if self.consumed_generic_args.contains(&ga.text_range()) {
            return base;
        }
        let type_args = Self::lower_generic_args_node(&ga, &mut self.diags);
        if type_args.is_empty() {
            return base;
        }
        // Only a bare path reference to a generic function may be specialized into
        // a value (`foo<int>`, `a.b.foo<int>`). A *parenthesized* base (`(foo)<int>`)
        // is rejected even though its inner expression is a path — the paren lowers
        // transparently, so it must be detected on the CST node — as is any other
        // non-path base (`(a + b)<int>`, `g().foo<int>`).
        let parenthesized_base = node.children().any(|n| n.kind() == SyntaxKind::PAREN_EXPR);
        let path_base = matches!(self.exprs[base], Expr::Path(_));
        if parenthesized_base || !path_base {
            self.diags
                .push(LoweringDiagnostic::TypeArgsOnNonPathBase { span: range });
            // Recover by still specializing the inner reference when it is a path
            // (`(foo)<int>`), so a downstream "must be specialized" error does not
            // pile on; a genuinely non-path base cannot be specialized.
            if !path_base {
                return base;
            }
        }
        self.alloc_expr(Expr::GenericApply { base, type_args }, range)
    }

    fn lower_path_expr(&mut self, node: &SyntaxNode) -> ExprId {
        // PATH_EXPR contains WORD (or keyword-as-ident) tokens joined by DOTs.
        //
        // When a PATH_EXPR is wrapped in another PATH_EXPR for generic-arg
        // annotation (e.g. `reflect.type_of<User>` → outer PATH_EXPR wrapping
        // inner PATH_EXPR + GENERIC_ARGS), the outer node has no direct token
        // children. In that case, delegate to the inner PATH_EXPR node.
        let mut segments: Vec<(Name, TextRange)> = Vec::new();

        for elem in node.children_with_tokens() {
            if let rowan::NodeOrToken::Token(token) = elem {
                if is_ident_token(token.kind()) {
                    segments.push((Name::new(token.text()), token.text_range()));
                }
            }
        }

        if segments.is_empty() {
            // An outer PATH_EXPR with no direct ident tokens wraps an inner
            // expression plus a `GENERIC_ARGS` annotation. The parser produces
            // this for any `<receiver><...>` value whose receiver is itself a
            // compound expression: `foo.bar<int>` (inner PATH_EXPR), but also
            // `(b).foo<int>`, `b?.foo<int>`, `arr[0].foo<int>`, `g().foo<int>`
            // (inner FIELD_ACCESS_EXPR / OPTIONAL_FIELD_ACCESS_EXPR / INDEX_EXPR
            // / CALL_EXPR / PAREN_EXPR). Lower the inner expression through the
            // normal chain dispatch and capture the `GENERIC_ARGS` here, so the
            // receiver and type args are never silently dropped.
            if let Some(inner) = node
                .children()
                .find(|n| n.kind() != SyntaxKind::GENERIC_ARGS)
            {
                let base = self.lower_expr_in_chain(&inner);
                return self.wrap_generic_apply(node, base, node.span_range());
            }
            return self.alloc_expr(Expr::Missing, node.span_range());
        }

        // Check if single segment is a literal keyword.
        //
        // Note `$id` is deliberately NOT desugared here: a lone `$id` reaches
        // the AST as a bare WORD token (the parser only builds PATH_EXPR for
        // dotted paths), so the special form is owned downstream — TIR types
        // the read as `string` (builder.rs `infer_path`) and MIR lowers reads
        // to `baml.id.current()` / writes to `baml.id.set(...)` (lower.rs).
        if segments.len() == 1 {
            match segments[0].0.as_str() {
                "true" => {
                    return self.alloc_expr(Expr::Literal(Literal::Bool(true)), node.span_range());
                }
                "false" => {
                    return self.alloc_expr(Expr::Literal(Literal::Bool(false)), node.span_range());
                }
                "null" => return self.alloc_expr(Expr::Null, node.span_range()),
                _ => {}
            }
        }

        // Multi-segment paths stay as Path(["a", "b", "c"]).
        // Record per-segment spans for diagnostics and LSP.
        let names: Vec<Name> = segments.iter().map(|(n, _)| n.clone()).collect();
        let id = self.alloc_expr(Expr::Path(names), node.span_range());
        if segments.len() > 1 {
            let spans: Vec<TextRange> = segments.iter().map(|(_, r)| *r).collect();
            self.source_map.path_segment_spans.insert(id, spans);
        }
        // `foo<int>` in value position: the GENERIC_ARGS is a direct child of
        // this PATH_EXPR. Wrap unless an enclosing call already consumed it.
        self.wrap_generic_apply(node, id, node.span_range())
    }

    /// Lower `MyFunc@spec` (BEP `@spec` postfix) by renaming the base path's
    /// last segment to the `<name>$spec` companion function — resolution then
    /// proceeds exactly as if the companion had been named directly. The base
    /// must be a plain path (an LLM function reference); anything else lowers
    /// to `Missing` with a diagnostic-friendly span.
    fn lower_spec_expr(&mut self, node: &SyntaxNode) -> ExprId {
        let span = node.span_range();
        // The base is either a PATH_EXPR child or a bare WORD token (single
        // identifiers are tokens, not nodes, in postfix wrappers).
        let mut segments: Vec<Name> = Vec::new();
        for elem in node.children_with_tokens() {
            match elem {
                rowan::NodeOrToken::Node(child) if child.kind() == SyntaxKind::PATH_EXPR => {
                    for t in child
                        .children_with_tokens()
                        .filter_map(rowan::NodeOrToken::into_token)
                    {
                        if is_ident_token(t.kind()) {
                            segments.push(Name::new(t.text()));
                        }
                    }
                }
                // Everything before the `@` is the base; the trailing
                // `spec` word after it is the operator, not a segment.
                rowan::NodeOrToken::Token(t) if t.kind() == SyntaxKind::AT => break,
                rowan::NodeOrToken::Token(t) if is_ident_token(t.kind()) => {
                    segments.push(Name::new(t.text()));
                }
                _ => {}
            }
        }
        let Some(last) = segments.pop() else {
            self.diags.push(LoweringDiagnostic::UnparseableType {
                context: "`@spec` target (expected an LLM function reference)".to_string(),
                span,
            });
            return self.alloc_expr(Expr::Missing, span);
        };
        segments.push(Name::new(format!("{}$spec", last.as_str())));
        self.alloc_expr(Expr::Path(segments), span)
    }

    fn lower_field_access_expr(&mut self, node: &SyntaxNode) -> ExprId {
        let mut base = None;
        let mut field = None;
        let mut field_range = None;
        let mut seen_accessor = false;

        for elem in node.children_with_tokens() {
            match elem {
                rowan::NodeOrToken::Node(child) => {
                    if base.is_none() {
                        base = Some(self.lower_expr_in_chain(&child));
                    }
                }
                rowan::NodeOrToken::Token(token) => {
                    if matches!(token.kind(), SyntaxKind::DOT | SyntaxKind::DOLLAR) {
                        seen_accessor = true;
                    } else if !seen_accessor && base.is_none() {
                        // Base is a bare token that the parser emits without a
                        // wrapper node: a numeric literal (`7.to_string()`), or
                        // an identifier/keyword like `value.implements()` where
                        // `implements` lexes as a keyword and the parser cannot
                        // build a PATH_EXPR. `try_lower_bare_token` handles all
                        // of these (and returns None for anything else, leaving
                        // the Missing recovery below intact).
                        base = self.try_lower_bare_token(&token);
                    } else if seen_accessor && is_ident_token(token.kind()) {
                        field = Some(Name::new(token.text()));
                        field_range = Some(token.text_range());
                    }
                }
            }
        }

        let base = base.unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.span_range()));
        let member = field.unwrap_or_else(|| Name::new("_"));

        let id = self.alloc_expr(Expr::MemberAccess { base, member }, node.span_range());
        if let Some(range) = field_range {
            self.source_map.member_access_member_spans.insert(id, range);
        }
        if self.needs_chain_wrap.remove(&base) {
            self.needs_chain_wrap.insert(id);
        }
        id
    }

    fn lower_upcast_expr(&mut self, node: &SyntaxNode) -> ExprId {
        let mut base = None;
        for elem in node.children_with_tokens() {
            match elem {
                rowan::NodeOrToken::Node(child)
                    if child.kind() != SyntaxKind::GENERIC_ARGS && base.is_none() =>
                {
                    base = Some(self.lower_expr_in_chain(&child));
                }
                rowan::NodeOrToken::Token(token) if base.is_none() => {
                    base = self.try_lower_bare_token(&token);
                }
                _ => {}
            }
        }
        let base = base.unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.span_range()));

        let target = node
            .children()
            .find(|child| child.kind() == SyntaxKind::GENERIC_ARGS)
            .and_then(|args| {
                args.children()
                    .find(|child| child.kind() == SyntaxKind::TYPE_EXPR)
            })
            .and_then(baml_compiler_syntax::ast::TypeExpr::cast)
            .map(|te| crate::lower_type_expr::lower_type_expr_node(&te, &mut self.diags))
            .unwrap_or_else(|| TypeExprKind::Unknown { attrs: Vec::new() }.at(node.span_range()));

        let id = self.alloc_expr(Expr::Upcast { base, target }, node.span_range());
        if self.needs_chain_wrap.remove(&base) {
            self.needs_chain_wrap.insert(id);
        }
        id
    }

    fn lower_env_access_expr(&mut self, node: &SyntaxNode) -> ExprId {
        // Desugar `env.VAR_NAME` → `baml.env.get_or_panic("VAR_NAME")`
        let range = node.span_range();

        let mut field_text = None;
        let mut seen_dot = false;
        for elem in node.children_with_tokens() {
            if let rowan::NodeOrToken::Token(token) = elem {
                if token.kind() == SyntaxKind::DOT {
                    seen_dot = true;
                } else if seen_dot && is_ident_token(token.kind()) {
                    field_text = Some(token.text().to_string());
                    break;
                }
            }
        }

        let var_name = field_text.unwrap_or_else(|| "_".to_string());
        self.env_var_refs.push(EnvVarRef {
            name: var_name.clone(),
            range,
        });
        let callee = self.alloc_expr(
            Expr::Path(vec![
                Name::new("baml"),
                Name::new("env"),
                Name::new("get_or_panic"),
            ]),
            range,
        );
        let arg = self.alloc_expr(Expr::Literal(Literal::String(var_name)), range);
        self.alloc_expr(
            Expr::Call {
                callee,
                type_args: vec![],
                args: vec![CallArg::positional(arg)],
            },
            range,
        )
    }

    fn lower_index_expr(&mut self, node: &SyntaxNode) -> ExprId {
        let mut base = None;
        let mut index = None;
        let mut seen_lbracket = false;

        for elem in node.children_with_tokens() {
            match elem {
                rowan::NodeOrToken::Node(child) => {
                    if !seen_lbracket {
                        if base.is_none() {
                            base = Some(self.lower_expr_in_chain(&child));
                        }
                    } else if index.is_none() {
                        index = Some(self.lower_expr(&child));
                    }
                }
                rowan::NodeOrToken::Token(token) => {
                    if token.kind() == SyntaxKind::L_BRACKET {
                        seen_lbracket = true;
                    } else if !seen_lbracket && base.is_none() {
                        base = self.try_lower_bare_token(&token);
                    } else if seen_lbracket && index.is_none() {
                        index = self.try_lower_bare_token(&token);
                    }
                }
            }
        }

        let base = base.unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.span_range()));
        let index = index.unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.span_range()));

        let id = self.alloc_expr(Expr::Index { base, index }, node.span_range());
        if self.needs_chain_wrap.remove(&base) {
            self.needs_chain_wrap.insert(id);
        }
        id
    }

    fn lower_optional_field_access_expr(&mut self, node: &SyntaxNode) -> ExprId {
        // OPTIONAL_FIELD_ACCESS_EXPR: <base_expr> QUESTION_DOT WORD
        // Note: base may be a Node (PATH_EXPR, CALL_EXPR, etc.) or a bare WORD token
        // when the base is a simple identifier like `user?.name`.
        let mut base = None;
        let mut field = None;
        let mut field_range = None;
        let mut seen_question_dot = false;

        for elem in node.children_with_tokens() {
            match elem {
                rowan::NodeOrToken::Node(child) => {
                    if base.is_none() {
                        base = Some(self.lower_expr_in_chain(&child));
                    }
                }
                rowan::NodeOrToken::Token(token) => {
                    if token.kind() == SyntaxKind::QUESTION_DOT {
                        seen_question_dot = true;
                    } else if !seen_question_dot && base.is_none() {
                        // Base is a bare token (e.g. `user` in `user?.name`, or a
                        // numeric literal in `7?.foo`). `try_lower_bare_token`
                        // handles identifiers, keywords, and numeric literals.
                        base = self.try_lower_bare_token(&token);
                    } else if seen_question_dot && is_ident_token(token.kind()) {
                        field = Some(Name::new(token.text()));
                        field_range = Some(token.text_range());
                    }
                }
            }
        }

        let base = base.unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.span_range()));
        let member = field.unwrap_or_else(|| Name::new("_"));

        let id = self.alloc_expr(
            Expr::OptionalMemberAccess { base, member },
            node.span_range(),
        );
        if let Some(range) = field_range {
            self.source_map.member_access_member_spans.insert(id, range);
        }
        self.needs_chain_wrap.remove(&base); // consume base's flag if any
        self.needs_chain_wrap.insert(id); // mark ourselves
        id
    }

    fn lower_optional_index_expr(&mut self, node: &SyntaxNode) -> ExprId {
        // OPTIONAL_INDEX_EXPR: <base_expr> QUESTION_DOT L_BRACKET <index_expr> R_BRACKET
        let mut base = None;
        let mut index = None;
        let mut seen_lbracket = false;

        for elem in node.children_with_tokens() {
            match elem {
                rowan::NodeOrToken::Node(child) => {
                    if !seen_lbracket {
                        if base.is_none() {
                            base = Some(self.lower_expr_in_chain(&child));
                        }
                    } else if index.is_none() {
                        index = Some(self.lower_expr(&child));
                    }
                }
                rowan::NodeOrToken::Token(token) => {
                    if token.kind() == SyntaxKind::L_BRACKET {
                        seen_lbracket = true;
                    } else if !seen_lbracket && base.is_none() {
                        base = self.try_lower_bare_token(&token);
                    } else if seen_lbracket && index.is_none() {
                        index = self.try_lower_bare_token(&token);
                    }
                }
            }
        }

        let base = base.unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.span_range()));
        let index = index.unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.span_range()));

        let id = self.alloc_expr(Expr::OptionalIndex { base, index }, node.span_range());
        self.needs_chain_wrap.remove(&base);
        self.needs_chain_wrap.insert(id);
        id
    }

    fn lower_optional_call_expr(&mut self, node: &SyntaxNode) -> ExprId {
        // OPTIONAL_CALL_EXPR: <callee_expr> QUESTION_DOT CALL_ARGS
        let callee_node = node.children().find(|n| n.kind() != SyntaxKind::CALL_ARGS);

        let callee = if let Some(n) = callee_node {
            self.lower_expr_in_chain(&n)
        } else {
            let word_token = node
                .children_with_tokens()
                .filter_map(rowan::NodeOrToken::into_token)
                .find(|t| t.kind() == SyntaxKind::WORD);

            if let Some(token) = word_token {
                self.alloc_expr(
                    Expr::Path(vec![Name::new(token.text())]),
                    token.text_range(),
                )
            } else {
                self.alloc_expr(Expr::Missing, node.span_range())
            }
        };

        let lowered_args = node
            .children()
            .find(|n| n.kind() == SyntaxKind::CALL_ARGS)
            .map(|args_node| self.lower_call_args_node(&args_node))
            .unwrap_or_default();
        let (args, label_spans) = Self::finalize_call_args(lowered_args);

        let id = self.alloc_expr(Expr::OptionalCall { callee, args }, node.span_range());
        self.record_call_arg_label_spans(id, label_spans);
        self.needs_chain_wrap.remove(&callee);
        self.needs_chain_wrap.insert(id);
        id
    }

    fn lower_string_literal(&mut self, node: &SyntaxNode) -> ExprId {
        let text = node.text().to_string();
        let content = strip_string_delimiters(&text);
        self.alloc_expr(Expr::Literal(Literal::String(content)), node.span_range())
    }

    /// Lower a BEP-049 untagged backtick string literal to a first-class
    /// [`Expr::Template`] with [`TemplateTag::Default`].
    ///
    /// Like the tagged path (`lower_tagged_template_expr`), we keep the
    /// `${for}`/`${if}`/`${expr}` structure as [`TemplateSegment`]s rather
    /// than desugaring to a concat chain here, so TIR can type-check each
    /// `${…}` natively and point diagnostics at its own span (BEP §11's
    /// implicit `.to_string()` coercion is enforced as a TIR rule and the
    /// concat lowering is MIR's job). Interp payloads are the raw inner
    /// block expressions — identical to the tagged path; the `Default` vs
    /// `Custom` tag is what drives the divergent value handling downstream.
    fn lower_backtick_string_literal(&mut self, node: &SyntaxNode) -> ExprId {
        use baml_compiler_syntax::BacktickStringLiteral;
        use rowan::ast::AstNode;

        let span = node.span_range();
        let Some(lit) = BacktickStringLiteral::cast(node.clone()) else {
            return self.alloc_expr(Expr::Missing, span);
        };
        let segments = self.lower_template_segments_checked(&lit);
        // Build the desugared realization (a `+` concat with implicit
        // `.to_string()`) from the *same* segment `ExprId`s. HIR/MIR/codegen
        // consume `elaborated`; TIR types it (quietly) and uses `segments`
        // only for the strict per-`${…}` diagnostics (BEP §11).
        //
        // The elaboration is entirely compiler-generated — mark every node it
        // allocates as synthetic so consumers (e.g. inlay hints) can skip it.
        // The segments were lowered above as real user code and keep their
        // non-synthetic ids; nested backtick strings save/restore the flag.
        let prev_synth = std::mem::replace(&mut self.synthesizing, true);
        let elaborated = self.elaborate_default_segments(&segments, span);
        self.synthesizing = prev_synth;
        self.alloc_expr(
            Expr::Template {
                tag: TemplateTag::Default { elaborated },
                segments,
            },
            span,
        )
    }

    /// Build the desugared realization of an untagged backtick template: a
    /// left-folded `+` concatenation of the segments, with each `${expr}`
    /// wrapped in an implicit `.to_string()` (BEP §11), `${for}` lowered to an
    /// accumulator block, and `${if}` to a host if-chain. Operates on the
    /// already-lowered [`TemplateSegment`]s (reusing their `ExprId`s /
    /// `PatId`s), so the interpolation expressions are shared with the node's
    /// `segments` rather than re-lowered.
    fn elaborate_default_segments(
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
                TemplateSegment::Interp(e) => self.elaborate_default_interp(*e, span),
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
                else_branch: None,
            },
            span,
        );
        let mut block_stmts: Vec<StmtId> = vec![acc_let];
        for part in parts {
            match part {
                InterpPart::Value(value) => {
                    let acc_lhs = self.alloc_expr(Expr::Path(vec![acc_name.clone()]), after_span);
                    let acc_rhs = self.alloc_expr(Expr::Path(vec![acc_name.clone()]), after_span);
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
        let acc_tail = self.alloc_expr(Expr::Path(vec![acc_name]), after_span);
        self.alloc_expr(
            Expr::Block {
                stmts: block_stmts,
                tail_expr: Some(acc_tail),
            },
            span,
        )
    }

    /// Elaborate a `${expr}` for the untagged path: wrap the inner block with
    /// `.to_string()` (BEP §11). A statement-only (unit) block — or one whose
    /// tail is itself a unit-valued expression (e.g. an `if`/`if let` with no
    /// `else`) — renders `""` while still running its statements and tail for
    /// their side effects.
    fn elaborate_default_interp(&mut self, inner: ExprId, span: TextRange) -> InterpPart {
        if let Expr::Block { stmts, tail_expr } = &self.exprs[inner] {
            // A block is unit-valued when it has no tail, or its tail is a
            // syntactically-unit expression. It renders "" but still runs its
            // statements (and its tail, for side effects). Return the raw
            // statements so the caller can splice them into the enclosing concat
            // scope — keeping any `let` they bind visible to later segments
            // (BEP-049 §4 cross-site `let`), instead of confining them to a
            // per-segment block.
            let tail_is_unit = tail_expr.map(|t| self.is_unit_tail(t)).unwrap_or(true);
            if tail_is_unit {
                let mut stmts = stmts.clone();
                if let Some(t) = *tail_expr {
                    let tail_stmt = self.alloc_stmt(Stmt::Expr(t), span);
                    stmts.push(tail_stmt);
                }
                return InterpPart::Stmts(stmts);
            }
        }
        // Render the value via `string.from(...)` — BAML's universal renderer
        // (BEP-049 §11). It dispatches `to_string` on the value's runtime class
        // when that class implements `baml.ToString`, otherwise falls back to a
        // structural rendering, so any `${expr}` renders without the type having
        // to opt into the interface.
        let callee = self.alloc_expr(
            Expr::Path(vec![Name::new("string"), Name::new("from")]),
            span,
        );
        InterpPart::Value(self.alloc_expr(
            Expr::Call {
                callee,
                type_args: Vec::new(),
                args: vec![CallArg::positional(inner)],
            },
            span,
        ))
    }

    /// Is `expr` a syntactically unit-valued expression when it sits in a
    /// block's tail position? Only expressions that TIR types as `Ty::Void`
    /// qualify: an `if`/`if let` with no `else` branch (their missing arm is
    /// `void`), or a nested block whose own tail is unit. `while`/`for`/
    /// assignment/`let`/`return`/`throw`/`break`/`continue` are lowered as
    /// `Stmt`s (never a `tail_expr`), so they're already covered by the
    /// no-tail case and need not appear here.
    fn is_unit_tail(&self, expr: ExprId) -> bool {
        match &self.exprs[expr] {
            Expr::If {
                else_branch: None, ..
            }
            | Expr::IfLet {
                else_branch: None, ..
            } => true,
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
                else_branch: None,
            },
            span,
        );

        let acc_path_lhs = self.alloc_expr(Expr::Path(vec![acc_name.clone()]), after_span);
        let acc_path_rhs = self.alloc_expr(Expr::Path(vec![acc_name.clone()]), after_span);
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
        let acc_tail = self.alloc_expr(Expr::Path(vec![acc_name]), after_span);
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
                else_branch: None,
            },
            span,
        );

        let acc_path_lhs = self.alloc_expr(Expr::Path(vec![acc_name.clone()]), after_span);
        let acc_path_rhs = self.alloc_expr(Expr::Path(vec![acc_name.clone()]), after_span);
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
                origin: LoopOrigin::For,
            },
            after_span,
        );
        let acc_tail = self.alloc_expr(Expr::Path(vec![acc_name]), after_span);
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

    /// Lower a `TAGGED_TEMPLATE_EXPR` (BEP-049 §10) — a tag expression
    /// immediately followed by a backtick string — to a first-class
    /// [`Expr::Template`] with [`TemplateTag::Custom`].
    ///
    /// CST shape (see the `parse_backtick_string` call site in the parser):
    /// the tag expression is wrapped as the first child, followed by the
    /// `BACKTICK_STRING_LITERAL`. The structure is kept as
    /// [`TemplateSegment`]s — shared verbatim with the untagged path
    /// (`lower_backtick_string_literal`) — so TIR can apply tag-aware rules
    /// and point diagnostics at the original `${…}` spans.
    fn lower_tagged_template_expr(&mut self, node: &SyntaxNode) -> ExprId {
        use baml_compiler_syntax::BacktickStringLiteral;

        let span = node.span_range();

        // The backtick literal child; the tag is the first *other* child node.
        let backtick_node = node
            .children()
            .find(|n| n.kind() == SyntaxKind::BACKTICK_STRING_LITERAL);
        let tag = node
            .children()
            .find(|n| n.kind() != SyntaxKind::BACKTICK_STRING_LITERAL)
            .map(|n| self.lower_expr(&n))
            .or_else(|| {
                // Defensive: if the parser ever leaves a bare-token tag
                // (a lone identifier/literal) un-wrapped, lower it directly.
                node.children_with_tokens()
                    .filter_map(rowan::NodeOrToken::into_token)
                    .find_map(|t| self.try_lower_bare_token(&t))
            })
            .unwrap_or_else(|| self.alloc_expr(Expr::Missing, span));

        // BEP-049 ergonomic hack: a bare `` prompt`...` `` tag resolves to the
        // stdlib `ai.prompt`. BAML has no prelude,
        // and BAML has no prelude, so the unqualified form (which the BEP §10
        // examples use) would otherwise be an unresolved name. Rewriting the bare
        // path here — same `ExprId`, so the source span is preserved — means every
        // downstream stage (TIR typing/tag-validation, MIR lowering) sees the
        // qualified tag and the body-lambda bindings (`role`, `ctx`) resolve. A
        // caller who needs a different `prompt` tag can write it qualified.
        if matches!(&self.exprs[tag], Expr::Path(segs) if segs.len() == 1 && segs[0].as_str() == "prompt")
        {
            self.exprs[tag] = Expr::Path(vec![Name::new("ai"), Name::new("prompt")]);
        }

        let segments = backtick_node
            .and_then(BacktickStringLiteral::cast)
            .map(|lit| self.lower_template_segments_checked(&lit))
            .unwrap_or_default();

        // Desugared closure body: flatten segments into a `baml.TaggedString`.
        // MIR lowers this for the dynamic (`${for}`/`${if}`) case and keeps a
        // fixed-array fast-path off `segments` for purely-static templates.
        // The desugared closure body is entirely compiler-generated — mark its
        // nodes synthetic, mirroring the untagged path. The segments were lowered
        // above as real user code and keep their non-synthetic ids.
        let prev_synth = std::mem::replace(&mut self.synthesizing, true);
        let body = self.elaborate_tagged_body(&segments, span);
        self.synthesizing = prev_synth;

        self.alloc_expr(
            Expr::Template {
                tag: TemplateTag::Custom { tag, body },
                segments,
            },
            span,
        )
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
    fn elaborate_tagged_body(&mut self, segments: &[TemplateSegment], span: TextRange) -> ExprId {
        let parts = Name::new(" __tt_parts");
        let values = Name::new(" __tt_values");
        let cur = Name::new(" __tt_cur");

        let mut stmts: Vec<StmtId> = Vec::new();
        // let __tt_parts: string[] = [];
        stmts.push(self.tt_let_typed_empty_list(
            &parts,
            TypeExprKind::String { attrs: Vec::new() }.at(span),
            span,
        ));
        // let __tt_values: unknown[] = [];
        stmts.push(self.tt_let_typed_empty_list(
            &values,
            TypeExprKind::BuiltinUnknown { attrs: Vec::new() }.at(span),
            span,
        ));
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
                type_name: baml_base::TypePath::from_dotted("baml.TaggedString"),
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
                            origin: LoopOrigin::For,
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
            attrs: Vec::new(),
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
        self.alloc_expr(Expr::Path(vec![name.clone()]), at)
    }

    /// `<name> = <value>;`
    fn tt_assign(&mut self, name: &Name, value: ExprId, span: TextRange) -> StmtId {
        let at = TextRange::empty(span.start());
        let target = self.alloc_expr(Expr::Path(vec![name.clone()]), at);
        self.alloc_stmt(Stmt::Assign { target, value }, at)
    }

    /// `<name>.push(<arg>);` as a statement.
    fn tt_push_stmt(&mut self, name: &Name, arg: ExprId, span: TextRange) -> StmtId {
        let at = TextRange::empty(span.start());
        let recv = self.alloc_expr(Expr::Path(vec![name.clone()]), at);
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

    /// Walk backtick segments into [`TemplateSegment`]s, preserving `${for}` /
    /// `${if}` structure (no desugaring). Shared by both the untagged
    /// (`Default`) and tagged (`Custom`) paths — the tag, not the segments,
    /// drives the divergent downstream handling.
    /// Lower a backtick literal's segments, first reporting structural
    /// block-tag diagnostics (unclosed / mismatched / stray `${for}`/`${if}`)
    /// so a malformed template surfaces an error instead of silently
    /// miscompiling. Use this at every top-level `segments()` call site.
    fn lower_template_segments_checked(
        &mut self,
        lit: &baml_compiler_syntax::BacktickStringLiteral,
    ) -> Vec<TemplateSegment> {
        let (segs, errors) = lit.segments_with_errors();
        for e in errors {
            self.diags.push(LoweringDiagnostic::MalformedTemplateBlock {
                kind: e.kind,
                span: e.span,
            });
        }
        self.lower_template_segments(segs)
    }

    fn lower_template_segments(
        &mut self,
        segments: Vec<baml_compiler_syntax::BacktickSegment>,
    ) -> Vec<TemplateSegment> {
        use baml_compiler_syntax::BacktickSegment;

        let mut out = Vec::with_capacity(segments.len());
        for seg in segments {
            match seg {
                BacktickSegment::Text(s) => out.push(TemplateSegment::Text(s)),
                BacktickSegment::Interp(interp_node) => {
                    out.push(TemplateSegment::Interp(
                        self.lower_template_interp(&interp_node),
                    ));
                }
                BacktickSegment::For(for_seg) => {
                    if let Some(s) = self.lower_template_for(for_seg) {
                        out.push(s);
                    }
                }
                BacktickSegment::If(if_seg) => {
                    out.push(self.lower_template_if(if_seg));
                }
            }
        }
        out
    }

    /// Lower a `${expr}` interpolation. The `TemplateSegment::Interp` payload
    /// is the *raw* lowered block expression — no `.to_string()` wrapping.
    /// For the `Default` form MIR inserts the BEP §11 coercion; for the
    /// `Custom` form the tag's body decides how each value is rendered, so it
    /// receives the unmodified inner expression (values stay typed, §10/§11).
    fn lower_template_interp(&mut self, interp_node: &SyntaxNode) -> ExprId {
        // Detect an empty `${}` / `${ }` (no expression) from the node text —
        // robust to whether it parses to a missing or an empty block.
        let raw = interp_node.text().to_string();
        let inner_empty = raw
            .strip_prefix("${")
            .and_then(|s| s.strip_suffix('}'))
            .is_some_and(|inner| inner.trim().is_empty());
        if inner_empty {
            self.diags.push(LoweringDiagnostic::EmptyInterpolation {
                span: interp_node.span_range(),
            });
        }
        match interp_node
            .children()
            .find(|c| c.kind() == SyntaxKind::BLOCK_EXPR)
        {
            Some(b) => self.lower_expr(&b),
            None => self.alloc_expr(Expr::Missing, interp_node.span_range()),
        }
    }

    /// Lower a `${for (let p in c)}…${endfor}` block, keeping structure.
    ///
    /// Extracts the loop header and emits `TemplateSegment::For`. The HIR
    /// walker (`walk_template_segment`) pushes the loop scope and registers
    /// the binding itself, so a plain `lower_pattern` `PatId` is all we emit
    /// here. A malformed header (missing binding or collection) drops the
    /// segment.
    fn lower_template_for(
        &mut self,
        for_seg: baml_compiler_syntax::BacktickForSegment,
    ) -> Option<TemplateSegment> {
        // Iterator form has an `in`; C-style (`for (let i = 0; cond; step)`)
        // does not. The header is the same one the host `for` parses
        // (`parse_for_header_only`), so we reuse `lower_let_stmt` /
        // `try_lower_assignment` for the C-style pieces (BEP §4).
        let is_cstyle = !for_seg
            .open
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .any(|t| t.kind() == SyntaxKind::KW_IN);

        if is_cstyle {
            let child_nodes: Vec<SyntaxNode> = for_seg.open.children().collect();
            // Initializer is a `let` statement (declares the loop variable).
            let init_node = child_nodes
                .iter()
                .find(|n| n.kind() == SyntaxKind::LET_STMT)?;
            let init = self.lower_let_stmt(init_node);
            // The remaining expression nodes, in order, are [cond, step].
            let expr_nodes: Vec<&SyntaxNode> = child_nodes
                .iter()
                .filter(|n| n.kind() != SyntaxKind::LET_STMT)
                .collect();
            let cond = expr_nodes
                .first()
                .map(|n| self.lower_expr(n))
                .unwrap_or_else(|| self.alloc_expr(Expr::Missing, for_seg.open.text_range()));
            // Step is an assignment (`i += 1`) or a bare expression; absent for
            // `for (let i = 0; cond; )`.
            let step = expr_nodes.get(1).map(|n| {
                let r = n.text_range();
                self.try_lower_assignment(n).unwrap_or_else(|| {
                    let e = self.lower_expr(n);
                    self.alloc_stmt(Stmt::Expr(e), r)
                })
            });
            let body = self.lower_template_segments(for_seg.body);
            return Some(TemplateSegment::CStyleFor {
                init,
                cond,
                step,
                body,
            });
        }

        let mut pattern_node = None;
        let mut collection: Option<ExprId> = None;
        let mut seen_in = false;
        for elem in for_seg.open.children_with_tokens() {
            match elem {
                rowan::NodeOrToken::Token(t) => match t.kind() {
                    SyntaxKind::KW_IN => seen_in = true,
                    _ => {
                        if seen_in && collection.is_none() {
                            collection = self.try_lower_bare_token(&t);
                        }
                    }
                },
                rowan::NodeOrToken::Node(child) => {
                    if !seen_in && pattern_node.is_none() && child.kind() == SyntaxKind::LET_STMT {
                        pattern_node = child.children().find(|n| n.kind() == SyntaxKind::PATTERN);
                    } else if seen_in && collection.is_none() {
                        collection = Some(self.lower_expr(&child));
                    }
                }
            }
        }

        let pattern_node = pattern_node?;
        let collection = collection?;
        let binding = self.lower_pattern(&pattern_node);
        let body = self.lower_template_segments(for_seg.body);

        Some(TemplateSegment::For {
            binding,
            collection,
            body,
        })
    }

    /// Lower a `${if (c)}…${else if (c)}…${else}…${endif}` chain, keeping
    /// structure; bodies recurse via `lower_template_segments`. The host
    /// if-chain fold is deliberately NOT performed — TIR/MIR consume the
    /// branch structure.
    fn lower_template_if(
        &mut self,
        if_seg: baml_compiler_syntax::BacktickIfSegment,
    ) -> TemplateSegment {
        let mut branches = Vec::with_capacity(if_seg.branches.len());
        for branch in if_seg.branches {
            let header_span = branch.header.span_range();
            let mut cond: Option<ExprId> = None;
            for elem in branch.header.children_with_tokens() {
                match elem {
                    rowan::NodeOrToken::Node(c) if c.kind() != SyntaxKind::PATTERN => {
                        cond = Some(self.lower_expr(&c));
                        break;
                    }
                    rowan::NodeOrToken::Node(_) => {}
                    rowan::NodeOrToken::Token(t) => {
                        if let Some(expr) = self.try_lower_bare_token(&t) {
                            cond = Some(expr);
                            break;
                        }
                    }
                }
            }
            let condition = cond.unwrap_or_else(|| self.alloc_expr(Expr::Missing, header_span));
            let body = self.lower_template_segments(branch.body);
            branches.push(TemplateIfBranch { condition, body });
        }

        let else_body = if_seg.else_body.map(|b| self.lower_template_segments(b));

        TemplateSegment::If {
            branches,
            else_body,
        }
    }

    fn lower_byte_string_literal(&mut self, node: &SyntaxNode) -> ExprId {
        let text = node.text().to_string();
        // Strip the b"..." delimiters: remove leading `b"` and trailing `"`
        let content = text
            .strip_prefix("b\"")
            .and_then(|s| s.strip_suffix('"'))
            .unwrap_or("");
        match parse_byte_string_escapes(content) {
            Ok(bytes) => self.alloc_expr(Expr::ByteStringLiteral(bytes), node.span_range()),
            Err(message) => {
                self.diags
                    .push(LoweringDiagnostic::InvalidByteStringEscape {
                        message,
                        span: node.span_range(),
                    });
                self.alloc_expr(Expr::Missing, node.span_range())
            }
        }
    }

    fn lower_array_literal(&mut self, node: &SyntaxNode) -> ExprId {
        let mut elements = Vec::new();
        for elem in node.children_with_tokens() {
            match elem {
                rowan::NodeOrToken::Node(child) => {
                    elements.push(self.lower_expr(&child));
                }
                rowan::NodeOrToken::Token(token) => {
                    if let Some(expr_id) = self.try_lower_bare_token(&token) {
                        elements.push(expr_id);
                    }
                }
            }
        }
        self.alloc_expr(Expr::Array { elements }, node.span_range())
    }

    fn lower_object_literal(&mut self, node: &SyntaxNode) -> ExprId {
        fn collect_constructor_path(
            node: &SyntaxNode,
            path_segments: &mut Vec<Name>,
            type_args: &mut Vec<TypeExpr>,
            diags: &mut Vec<LoweringDiagnostic>,
        ) {
            for elem in node.children_with_tokens() {
                match elem {
                    rowan::NodeOrToken::Token(token) if is_ident_token(token.kind()) => {
                        path_segments.push(Name::new(token.text()));
                    }
                    rowan::NodeOrToken::Node(args_node)
                        if args_node.kind() == SyntaxKind::GENERIC_ARGS =>
                    {
                        *type_args = args_node
                            .children()
                            .filter(|n| n.kind() == SyntaxKind::TYPE_EXPR)
                            .filter_map(baml_compiler_syntax::ast::TypeExpr::cast)
                            .map(|te| crate::lower_type_expr::lower_type_expr_node(&te, diags))
                            .collect();
                    }
                    rowan::NodeOrToken::Node(child_node) => {
                        collect_constructor_path(&child_node, path_segments, type_args, diags);
                    }
                    rowan::NodeOrToken::Token(_) => {}
                }
            }
        }

        let mut fields = Vec::new();
        let mut field_name_spans = Vec::new();
        let mut spreads = Vec::new();
        let mut position = 0;
        let mut type_args: Vec<TypeExpr> = vec![];
        let mut type_path_segments: Vec<Name> = vec![];

        // Look for the optional type name (first WORD or path before the brace):
        //   - A simple WORD token: `MyClass { ... }` → `TypePath::bare`.
        //   - A qualified path node: `baml.errors.DevOther { ... }` (parsed as
        //     PATH_EXPR) → `TypePath` of all the WORD segments.
        //   - A generic path: `Foo<int> { ... }` (parsed as PATH_EXPR with
        //     GENERIC_ARGS child) → `TypePath::bare("Foo")` + `type_args = [int]`.
        for elem in node.children_with_tokens() {
            match elem {
                rowan::NodeOrToken::Token(token) => {
                    if token.kind() == SyntaxKind::L_BRACE {
                        break;
                    }
                    if is_ident_token(token.kind()) {
                        type_path_segments.push(Name::new(token.text()));
                    }
                }
                rowan::NodeOrToken::Node(child_node) => {
                    collect_constructor_path(
                        &child_node,
                        &mut type_path_segments,
                        &mut type_args,
                        &mut self.diags,
                    );
                }
            }
        }
        debug_assert!(!type_path_segments.is_empty());
        // The parser only emits an object literal when a type name precedes the
        // brace, so the segments are always present.
        let type_name = TypePath::new(type_path_segments);

        // Object fields are child nodes after L_BRACE. They come as key-value
        // pairs (`WORD COLON expr`), shorthand (`WORD`), or spreads.
        for child in node.children() {
            match child.kind() {
                SyntaxKind::OBJECT_FIELD => {
                    // OBJECT_FIELD: WORD (DOT WORD)* COLON expr, or shorthand WORD.
                    let mut key_segments = Vec::new();
                    let mut key_span: Option<TextRange> = None;
                    let mut val = None;
                    let mut seen_colon = false;
                    for elem in child.children_with_tokens() {
                        match elem {
                            rowan::NodeOrToken::Token(t) if t.kind() == SyntaxKind::COLON => {
                                seen_colon = true;
                            }
                            rowan::NodeOrToken::Token(t)
                                if is_ident_token(t.kind()) && !seen_colon =>
                            {
                                key_span = Some(match key_span {
                                    Some(span) => {
                                        TextRange::new(span.start(), t.text_range().end())
                                    }
                                    None => t.text_range(),
                                });
                                key_segments.push(t.text().to_string());
                            }
                            rowan::NodeOrToken::Node(n) if seen_colon && val.is_none() => {
                                val = Some(self.lower_expr(&n));
                            }
                            rowan::NodeOrToken::Token(t) if seen_colon && val.is_none() => {
                                val = self.try_lower_bare_token(&t);
                            }
                            rowan::NodeOrToken::Token(_) => {}
                            rowan::NodeOrToken::Node(_) => {}
                        }
                    }
                    if !seen_colon
                        && key_segments.len() == 1
                        && let Some(span) = key_span
                    {
                        let val_id =
                            self.alloc_expr(Expr::Path(vec![Name::new(&key_segments[0])]), span);
                        val = Some(val_id);
                    }
                    let key = if key_segments.is_empty() {
                        None
                    } else {
                        Some(Name::new(key_segments.join(".")))
                    };
                    if let (Some(k), Some(val_id)) = (key, val) {
                        if let Some(span) = key_span {
                            field_name_spans.push((val_id, span));
                        }
                        let field = if seen_colon {
                            ObjectExprField::explicit(k, val_id)
                        } else {
                            ObjectExprField::shorthand(k, val_id)
                        };
                        fields.push(field);
                    }
                    position += 1;
                }
                SyntaxKind::SPREAD_ELEMENT => {
                    // SPREAD_ELEMENT: ... expr
                    let spread_expr = if let Some(expr_node) = child.children().next() {
                        Some(self.lower_expr(&expr_node))
                    } else {
                        // Try bare token: ...x where x is a single identifier
                        child
                            .children_with_tokens()
                            .filter_map(rowan::NodeOrToken::into_token)
                            .find_map(|t| self.try_lower_bare_token(&t))
                    };
                    if let Some(expr) = spread_expr {
                        spreads.push(SpreadField { expr, position });
                    }
                    position += 1;
                }
                _ => {}
            }
        }

        let object_id = self.alloc_expr(
            Expr::Object {
                type_name,
                type_args,
                fields,
                spreads,
            },
            node.span_range(),
        );
        for (value_id, field_name_span) in field_name_spans {
            self.source_map
                .object_field_name_spans
                .insert((object_id, value_id), field_name_span);
        }
        object_id
    }

    fn lower_map_literal(&mut self, node: &SyntaxNode) -> ExprId {
        // MAP_LITERAL uses OBJECT_FIELD children (same as OBJECT_LITERAL).
        // Each OBJECT_FIELD is `key: value` or shorthand `key`.
        // For maps the key can also be a string literal or expression.
        let entries = node
            .children()
            .filter(|n| n.kind() == SyntaxKind::OBJECT_FIELD)
            .filter_map(|field_node| {
                // Key: first child node that can be an expression, or first WORD token
                let mut key_expr = None;
                let mut val_expr = None;
                let mut seen_colon = false;
                let mut shorthand_name = None;

                for elem in field_node.children_with_tokens() {
                    match elem {
                        rowan::NodeOrToken::Token(t) => {
                            if t.kind() == SyntaxKind::COLON {
                                seen_colon = true;
                            } else if !seen_colon && key_expr.is_none() && is_ident_token(t.kind())
                            {
                                let span = t.text_range();
                                shorthand_name = Some((Name::new(t.text()), span));
                                key_expr = Some(self.alloc_expr(
                                    Expr::Literal(Literal::String(t.text().to_string())),
                                    span,
                                ));
                            } else if !seen_colon
                                && key_expr.is_none()
                                && (t.kind() == SyntaxKind::STRING_LITERAL
                                    || t.kind() == SyntaxKind::RAW_STRING_LITERAL)
                            {
                                let content = strip_string_delimiters(t.text());
                                let span = t.text_range();
                                key_expr = Some(
                                    self.alloc_expr(Expr::Literal(Literal::String(content)), span),
                                );
                            } else if seen_colon && val_expr.is_none() {
                                val_expr = self.try_lower_bare_token(&t);
                            }
                        }
                        rowan::NodeOrToken::Node(n) => {
                            if !seen_colon && key_expr.is_none() {
                                key_expr = Some(self.lower_expr(&n));
                            } else if seen_colon && val_expr.is_none() {
                                val_expr = Some(self.lower_expr(&n));
                            }
                        }
                    }
                }

                if !seen_colon && let Some((name, span)) = shorthand_name {
                    let value = self.alloc_expr(Expr::Path(vec![name]), span);
                    val_expr = Some(value);
                }

                match (key_expr, val_expr) {
                    (Some(k), Some(v)) => Some(if seen_colon {
                        MapExprEntry::explicit(k, v)
                    } else {
                        MapExprEntry::shorthand(k, v)
                    }),
                    _ => None,
                }
            })
            .collect();

        self.alloc_expr(Expr::Map { entries }, node.span_range())
    }

    fn lower_lambda_expr(&mut self, node: &SyntaxNode) -> ExprId {
        use baml_compiler_syntax::ast;

        // A lambda is a function *value* and cannot declare generic parameters
        // (rejected by the parser). Any leading `<...>` is left in the CST for
        // recovery and ignored here — `LambdaDef` has nowhere to put them.

        // Lower parameter list — gives us Vec<Param>
        let (params, defaults) = node
            .children()
            .find(|n| n.kind() == SyntaxKind::PARAMETER_LIST)
            .and_then(ast::ParameterList::cast)
            .map(|pl| {
                crate::lower_cst::lower_params_with_defaults(
                    &pl,
                    "lambda",
                    "lambda parameters",
                    &mut self.diags,
                    false,
                    &mut self.env_var_refs,
                )
            })
            .unwrap_or_else(|| (Vec::new(), FunctionDefaults::empty()));

        // Lower optional return type: the TYPE_EXPR that is a direct child of the
        // lambda node, appearing after PARAMETER_LIST but before THROWS_CLAUSE/BLOCK_EXPR.
        // We scan children in order, skipping items until after PARAMETER_LIST.
        let return_type = {
            let mut after_params = false;
            let mut found: Option<TypeExpr> = None;
            for child in node.children() {
                match child.kind() {
                    SyntaxKind::PARAMETER_LIST | SyntaxKind::GENERIC_PARAM_LIST => {
                        after_params = true;
                    }
                    SyntaxKind::THROWS_CLAUSE | SyntaxKind::BLOCK_EXPR => {
                        break;
                    }
                    SyntaxKind::TYPE_EXPR if after_params && found.is_none() => {
                        if let Some(te) = ast::TypeExpr::cast(child.clone()) {
                            found = Some(
                                crate::lower_type_expr::lower_type_expr_node(&te, &mut self.diags)
                                    .with_span(child.span_range()),
                            );
                        }
                    }
                    _ => {}
                }
            }
            found
        };

        // Lower optional throws clause
        let throws = node
            .children()
            .find(|n| n.kind() == SyntaxKind::THROWS_CLAUSE)
            .and_then(ast::ThrowsClause::cast)
            .and_then(|tc| tc.type_expr())
            .map(|te| {
                crate::lower_type_expr::lower_type_expr_node(&te, &mut self.diags)
                    .with_span(te.syntax().span_range())
            });

        // The body lowers into *this* arena — a lambda owns no `ExprBody`.
        let body = node
            .children()
            .find(|n| n.kind() == SyntaxKind::BLOCK_EXPR)
            .and_then(ast::BlockExpr::cast)
            .map(|block| self.lower_lambda_body(&block));

        let lambda_def = LambdaDef {
            kind: LambdaKind::Anonymous,
            params,
            defaults,
            return_type,
            throws,
            body,
            span: node.span_range(),
        };

        self.alloc_expr(Expr::Lambda(Box::new(lambda_def)), node.span_range())
    }

    fn try_lower_paren_token_content(&mut self, node: &SyntaxNode) -> Option<ExprId> {
        // Look for a single meaningful token inside the parentheses
        for elem in node.children_with_tokens() {
            if let rowan::NodeOrToken::Token(token) = elem {
                let span = token.text_range();
                match token.kind() {
                    k if is_ident_token(k) => {
                        let text = token.text();
                        let e = match text {
                            "true" => Expr::Literal(Literal::Bool(true)),
                            "false" => Expr::Literal(Literal::Bool(false)),
                            "null" => Expr::Null,
                            _ => Expr::Path(vec![Name::new(text)]),
                        };
                        return Some(self.alloc_expr(e, span));
                    }
                    SyntaxKind::BIGINT_LITERAL => {
                        let value = self.bigint_literal_value(&token);
                        return Some(self.alloc_expr(Expr::Literal(Literal::Bigint(value)), span));
                    }
                    SyntaxKind::INTEGER_LITERAL => {
                        let value = self.int_literal_value(&token);
                        return Some(self.alloc_expr(Expr::Literal(Literal::Int(value)), span));
                    }
                    SyntaxKind::FLOAT_LITERAL => {
                        let text = num_lit::normalize_float_literal(token.text());
                        return Some(self.alloc_expr(Expr::Literal(Literal::Float(text)), span));
                    }
                    SyntaxKind::STRING_LITERAL | SyntaxKind::RAW_STRING_LITERAL => {
                        let content = strip_string_delimiters(token.text());
                        return Some(
                            self.alloc_expr(Expr::Literal(Literal::String(content)), span),
                        );
                    }
                    _ => {}
                }
            }
        }
        None
    }

    fn try_lower_literal_token(&mut self, node: &SyntaxNode) -> Option<ExprId> {
        // Check if this node is a single token node that we can treat as a literal
        let mut tokens = node
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .filter(|t| !t.kind().is_trivia());

        let token = tokens.next()?;
        if tokens.next().is_some() {
            return None; // Multiple tokens — not a simple literal
        }

        let span = token.text_range();
        match token.kind() {
            SyntaxKind::BIGINT_LITERAL => {
                let value = self.bigint_literal_value(&token);
                Some(self.alloc_expr(Expr::Literal(Literal::Bigint(value)), span))
            }
            SyntaxKind::INTEGER_LITERAL => {
                let value = self.int_literal_value(&token);
                Some(self.alloc_expr(Expr::Literal(Literal::Int(value)), span))
            }
            SyntaxKind::FLOAT_LITERAL => {
                let text = num_lit::normalize_float_literal(token.text());
                Some(self.alloc_expr(Expr::Literal(Literal::Float(text)), span))
            }
            k if is_ident_token(k) => {
                let text = token.text();
                let e = match text {
                    "true" => Expr::Literal(Literal::Bool(true)),
                    "false" => Expr::Literal(Literal::Bool(false)),
                    "null" => Expr::Null,
                    _ => Expr::Path(vec![Name::new(text)]),
                };
                Some(self.alloc_expr(e, span))
            }
            _ => None,
        }
    }

    fn lower_let_stmt(&mut self, node: &SyntaxNode) -> StmtId {
        // LET_STMT shape (post-pattern-rewrite):
        //   (KW_LET|KW_CONST)? PATTERN EQUALS <init-expr> (KW_ELSE BLOCK_EXPR)? SEMICOLON?
        //
        // The pattern carries its own `: T` narrow as a Chain link, so all we
        // do here is locate the PATTERN child, the initialiser child, and an
        // optional trailing `else { … }` block for let-else.
        let mut pattern_id = None;
        let mut initializer = None;
        let mut else_branch = None;
        let mut seen_equals = false;
        let mut seen_else = false;
        self.warn_direct_const_introducers(node);

        for elem in node.children_with_tokens() {
            match elem {
                rowan::NodeOrToken::Token(token) => match token.kind() {
                    SyntaxKind::EQUALS => seen_equals = true,
                    SyntaxKind::KW_ELSE => seen_else = true,
                    _ if seen_equals && !seen_else && initializer.is_none() => {
                        if let Some(id) = self.try_lower_bare_token(&token) {
                            initializer = Some(id);
                        }
                    }
                    _ => {}
                },
                rowan::NodeOrToken::Node(child) => {
                    if !seen_equals {
                        if child.kind() == SyntaxKind::PATTERN && pattern_id.is_none() {
                            pattern_id = Some(self.lower_pattern(&child));
                        }
                    } else if !seen_else && initializer.is_none() {
                        initializer = Some(self.lower_expr(&child));
                    } else if seen_else && else_branch.is_none() {
                        else_branch = Some(self.lower_expr(&child));
                    }
                }
            }
        }

        let pattern =
            pattern_id.unwrap_or_else(|| self.alloc_pattern(Pattern::Wildcard, node.span_range()));

        self.check_pattern_void_in_annotation(pattern, "a let binding annotation");

        self.alloc_stmt(
            Stmt::Let {
                pattern,
                initializer,
                origin: LetOrigin::Source,
                else_branch,
            },
            node.span_range(),
        )
    }

    fn lower_return_stmt(&mut self, node: &SyntaxNode) -> StmtId {
        let expr = self.lower_optional_return_value(node);
        self.alloc_stmt(Stmt::Return(expr), node.span_range())
    }

    /// Lower the optional value of a `return` node — shared by `RETURN_STMT`
    /// (statement position) and `RETURN_EXPR` (expression position), which have
    /// the identical shape `KW_RETURN expr?` (the statement may also carry a
    /// trailing `;`). Returns `None` for a bare `return`.
    ///
    /// The token-level fallback mirrors `lower_throw_expr`: the parser emits
    /// bare literal/identifier tokens (not wrapper expression nodes) for simple
    /// values, so those must be reconstructed here.
    fn lower_optional_return_value(&mut self, node: &SyntaxNode) -> Option<ExprId> {
        // Try child nodes first, then fall back to token-level expressions
        if let Some(child_node) = node.children().next() {
            Some(self.lower_expr(&child_node))
        } else {
            // No child node — check for a token-level expression (e.g. `return 1;`)
            let mut result = None;
            for elem in node.children_with_tokens() {
                if let rowan::NodeOrToken::Token(token) = elem {
                    let span = token.text_range();
                    match token.kind() {
                        SyntaxKind::KW_RETURN | SyntaxKind::SEMICOLON => continue,
                        SyntaxKind::BIGINT_LITERAL => {
                            let value = self.bigint_literal_value(&token);
                            result =
                                Some(self.alloc_expr(Expr::Literal(Literal::Bigint(value)), span));
                            break;
                        }
                        SyntaxKind::INTEGER_LITERAL => {
                            let value = self.int_literal_value(&token);
                            result =
                                Some(self.alloc_expr(Expr::Literal(Literal::Int(value)), span));
                            break;
                        }
                        SyntaxKind::FLOAT_LITERAL => {
                            let text = num_lit::normalize_float_literal(token.text());
                            result =
                                Some(self.alloc_expr(Expr::Literal(Literal::Float(text)), span));
                            break;
                        }
                        SyntaxKind::STRING_LITERAL | SyntaxKind::RAW_STRING_LITERAL => {
                            let content = strip_string_delimiters(token.text());
                            result = Some(
                                self.alloc_expr(Expr::Literal(Literal::String(content)), span),
                            );
                            break;
                        }
                        k if is_ident_token(k) => {
                            let text = token.text();
                            let e = match text {
                                "true" => Expr::Literal(Literal::Bool(true)),
                                "false" => Expr::Literal(Literal::Bool(false)),
                                "null" => Expr::Null,
                                _ => Expr::Path(vec![Name::new(text)]),
                            };
                            result = Some(self.alloc_expr(e, span));
                            break;
                        }
                        _ => {}
                    }
                }
            }
            result
        }
    }

    /// Lower `return expr?` in expression position to [`Expr::Return`] — a
    /// diverging expression of type `never` (mirrors `lower_throw_expr`). Shares
    /// value extraction with `lower_return_stmt` via `lower_optional_return_value`.
    fn lower_return_expr(&mut self, node: &SyntaxNode) -> ExprId {
        let value = self.lower_optional_return_value(node);
        self.alloc_expr(Expr::Return { value }, node.span_range())
    }

    /// Lower a `break`/`continue` used in expression position (`BREAK_EXPR` /
    /// `CONTINUE_EXPR`, e.g. a bare match arm `0 => break`) into a block that
    /// holds the corresponding jump statement: `{ break; }` / `{ continue; }`.
    ///
    /// `break`/`continue` carry no value, so — unlike `return` — they need no
    /// dedicated `Expr` variant. Desugaring to a single-statement block reuses
    /// the fully-tested `Stmt::Break`/`Stmt::Continue` machinery (divergence
    /// typing to `never`, defer replay/unwatch, defer-escape diagnostics) and
    /// makes the braceless form behave identically to the already-accepted
    /// braced arm.
    fn lower_jump_expr(&mut self, node: &SyntaxNode, jump: Stmt) -> ExprId {
        let span = node.span_range();
        let stmt = self.alloc_stmt(jump, span);
        self.alloc_expr(
            Expr::Block {
                stmts: vec![stmt],
                tail_expr: None,
            },
            span,
        )
    }

    /// Lower `defer { BODY }` (BEP-042). The CST shape is
    /// `DEFER_STMT [ KW_DEFER BLOCK_EXPR ]`. Unlike `spawn`, the body is NOT a
    /// lambda — it is lowered inline as an [`Expr::Block`] in the enclosing
    /// `ExprBody`, so deferred code reads the live enclosing scope at exit
    /// rather than a captured snapshot. MIR replays this block at every exit
    /// edge of the enclosing scope.
    fn lower_defer_stmt(&mut self, node: &SyntaxNode) -> StmtId {
        let body = node
            .children()
            .find(|c| c.kind() == SyntaxKind::BLOCK_EXPR)
            .map(|block| self.lower_expr(&block))
            .unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.span_range()));
        self.alloc_stmt(Stmt::Defer { body }, node.span_range())
    }

    fn lower_while_stmt(&mut self, node: &SyntaxNode) -> StmtId {
        let mut sub_exprs = Vec::new();
        for elem in node.children_with_tokens() {
            match elem {
                rowan::NodeOrToken::Node(child) => {
                    sub_exprs.push(self.lower_expr(&child));
                }
                rowan::NodeOrToken::Token(token) => {
                    if let Some(expr_id) = self.try_lower_bare_token(&token) {
                        sub_exprs.push(expr_id);
                    }
                }
            }
        }

        let condition = sub_exprs
            .first()
            .copied()
            .unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.span_range()));
        let body = sub_exprs
            .get(1)
            .copied()
            .unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.span_range()));

        self.alloc_stmt(
            Stmt::While {
                condition,
                body,
                after: None,
                origin: LoopOrigin::While,
            },
            node.span_range(),
        )
    }

    fn lower_for_stmt(&mut self, node: &SyntaxNode) -> StmtId {
        // FOR_EXPR can take two forms:
        //   Iterator-style:  for (let var in <expr>) { <body> }
        //   C-style:         for (let i = 0; cond; update) { <body> }
        //
        // Detection: C-style has no KW_IN token among FOR_EXPR's direct children.
        // The C-style LET_STMT child contains EQUALS and SEMICOLON tokens.
        //
        // C-style is desugared to:
        //   Stmt::Let { ... }   // init
        //   Stmt::While { condition, body, after: Some(update_stmt), origin: LoopOrigin::For }
        // These two statements are wrapped in Expr::Block → Stmt::Expr so the
        // function can return a single StmtId.
        let range = node.span_range();

        // Determine if this is a C-style for loop by checking for KW_IN.
        let is_c_style = !node
            .children_with_tokens()
            .any(|e| matches!(&e, rowan::NodeOrToken::Token(t) if t.kind() == SyntaxKind::KW_IN));

        if is_c_style {
            return self.lower_c_style_for(node, range);
        }

        // --- Iterator-style for loop ---
        //
        // Parenthesized:     for (let var in <expr>) { <body> }
        // Non-parenthesized: for var in <expr> { <body> }
        //
        // In the parenthesized form, the variable binding is wrapped in a
        // LET_STMT child node (from `parse_for_in_pattern`). In the
        // non-parenthesized form, the variable is a bare WORD token.
        //
        // We emit a first-class Stmt::For (NOT desugared to While here).
        // Desugaring to index-based basic blocks happens at MIR lowering time.

        // Iterator-style for loop. The parser always wraps the binding in
        // `LET_STMT > PATTERN` via `parse_for_in_pattern`, so there's exactly
        // one valid surface form: `for (let <pattern> in <expr>) { <body> }`
        // (or its non-parenthesized variant `for let <pattern> in <expr>`).
        let mut binding_id: Option<PatId> = None;
        let mut iter_expr_opt = None;
        let mut body_opt = None;
        let mut seen_in = false;

        for elem in node.children_with_tokens() {
            match elem {
                rowan::NodeOrToken::Token(token) => match token.kind() {
                    SyntaxKind::KW_IN => {
                        seen_in = true;
                    }
                    _ => {
                        if seen_in && iter_expr_opt.is_none() {
                            iter_expr_opt = self.try_lower_bare_token(&token);
                        }
                    }
                },
                rowan::NodeOrToken::Node(child) => {
                    if !seen_in && binding_id.is_none() && child.kind() == SyntaxKind::LET_STMT {
                        self.warn_direct_const_introducers(&child);
                        if let Some(pat_node) =
                            child.children().find(|n| n.kind() == SyntaxKind::PATTERN)
                        {
                            binding_id = Some(self.lower_pattern(&pat_node));
                        }
                    } else if seen_in && iter_expr_opt.is_none() {
                        iter_expr_opt = Some(self.lower_expr(&child));
                    } else if iter_expr_opt.is_some() && body_opt.is_none() {
                        body_opt = Some(self.lower_expr(&child));
                    }
                }
            }
        }

        let collection = iter_expr_opt.unwrap_or_else(|| self.alloc_expr(Expr::Missing, range));
        let body = body_opt.unwrap_or_else(|| self.alloc_expr(Expr::Missing, range));
        let binding = binding_id.unwrap_or_else(|| self.alloc_pattern(Pattern::Wildcard, range));

        self.alloc_stmt(
            Stmt::For {
                binding,
                collection,
                body,
            },
            range,
        )
    }

    /// Desugar a C-style for loop `for (let i = 0; cond; update) { body }`
    /// into:
    ///   ```text
    ///   {
    ///     let i = 0;                // init_stmt  (Stmt::Let)
    ///     while cond {              // Stmt::While
    ///       body;
    ///     } after { update; }
    ///   }
    ///   ```
    /// The two statements are wrapped in an `Expr::Block` so this function can
    /// return a single `StmtId` (as `Stmt::Expr(block)`).
    fn lower_c_style_for(&mut self, node: &SyntaxNode, range: text_size::TextRange) -> StmtId {
        // C-style CST structure (direct children of FOR_EXPR):
        //   KW_FOR  L_PAREN  LET_STMT  BINARY_EXPR(cond)  SEMICOLON  BINARY_EXPR(update)  R_PAREN  BLOCK_EXPR
        //
        // We collect child nodes in order; the three significant nodes are:
        //   [0] LET_STMT      — initializer
        //   [1] BINARY_EXPR   — condition
        //   [2] BINARY_EXPR   — update
        //   [3] BLOCK_EXPR    — body
        let child_nodes: Vec<SyntaxNode> = node.children().collect();

        // Pull out init (LET_STMT), cond, update, body nodes by position.
        // child_nodes order: LET_STMT, BINARY_EXPR(cond), BINARY_EXPR(update), BLOCK_EXPR
        let init_node = child_nodes
            .iter()
            .find(|n| n.kind() == SyntaxKind::LET_STMT)
            .cloned();
        // BINARY_EXPRs appear in document order: first is condition, second is update.
        let binary_exprs: Vec<SyntaxNode> = child_nodes
            .iter()
            .filter(|n| n.kind() == SyntaxKind::BINARY_EXPR)
            .cloned()
            .collect();
        let block_node = child_nodes
            .iter()
            .find(|n| n.kind() == SyntaxKind::BLOCK_EXPR)
            .cloned();

        // Lower the initializer as a Let statement.
        let init_stmt = if let Some(let_node) = init_node {
            self.lower_let_stmt(&let_node)
        } else {
            self.alloc_stmt(Stmt::Missing, range)
        };

        // Lower the condition expression (first BINARY_EXPR).
        let condition = if let Some(cond_node) = binary_exprs.first() {
            self.lower_expr(cond_node)
        } else {
            self.alloc_expr(Expr::Missing, range)
        };

        // Lower the update expression (second BINARY_EXPR) as a statement.
        // `i += 1` is an assignment-op, so try_lower_assignment handles it.
        let after_stmt = if let Some(update_node) = binary_exprs.get(1) {
            let update_range = update_node.span_range();
            let stmt_opt = self.try_lower_assignment(update_node);
            Some(stmt_opt.unwrap_or_else(|| {
                // Plain expression update (e.g. function call)
                let expr_id = self.lower_expr(update_node);
                self.alloc_stmt(Stmt::Expr(expr_id), update_range)
            }))
        } else {
            None
        };

        // Lower the loop body.
        let body = if let Some(blk) = block_node {
            self.lower_expr(&blk)
        } else {
            self.alloc_expr(Expr::Missing, range)
        };

        // Build Stmt::While.
        let while_stmt = self.alloc_stmt(
            Stmt::While {
                condition,
                body,
                after: after_stmt,
                origin: LoopOrigin::For,
            },
            range,
        );

        // Wrap both statements in a block expression so we return one StmtId.
        let block_expr = self.alloc_expr(
            Expr::Block {
                stmts: vec![init_stmt, while_stmt],
                tail_expr: None,
            },
            range,
        );
        self.alloc_stmt(Stmt::Expr(block_expr), range)
    }

    /// Lower a `TEST_EXPR_DEF` node as a `<collector>.register_test(name, lambda, null)` call.
    ///
    /// Used when `test` appears inside a testset body (possibly nested inside a `for`/`if`).
    /// Requires `self.testset_collector_var` to be set.
    fn lower_test_expr_as_register_call(&mut self, node: &SyntaxNode) -> ExprId {
        let span = node.span_range();
        let collector_name = self
            .testset_collector_var
            .clone()
            .unwrap_or_else(|| Name::new("testset"));

        // Extract test name from STRING_LITERAL child (may be a BINARY_EXPR for concatenation)
        let name_expr = self.lower_test_name_expr(node, span);

        // Find the BLOCK_EXPR child (the test body)
        let body_node_opt = node.children().find(|c| c.kind() == SyntaxKind::BLOCK_EXPR);

        // `lower_lambda_body` clears the collector — test bodies don't nest.
        let lambda_body = match body_node_opt.and_then(baml_compiler_syntax::ast::BlockExpr::cast) {
            Some(block) => self.lower_lambda_body(&block),
            None => self.alloc_expr(Expr::Null, span),
        };

        let lambda_def = LambdaDef {
            kind: LambdaKind::Anonymous,
            params: vec![],
            defaults: FunctionDefaults::empty(),
            return_type: None,
            throws: None,
            body: Some(lambda_body),
            span,
        };

        // <collector>.register_test(name_expr, lambda, runner_or_null)
        let collector_ref = self.alloc_expr(Expr::Path(vec![collector_name]), span);
        let method_target = self.alloc_expr(
            Expr::MemberAccess {
                base: collector_ref,
                member: Name::new("register_test"),
            },
            span,
        );
        let lambda_arg = self.alloc_expr(Expr::Lambda(Box::new(lambda_def)), span);
        let runner_arg = match crate::lower_cst::extract_runner_element(node) {
            Some(rowan::NodeOrToken::Node(runner_node)) => self.lower_expr(&runner_node),
            Some(rowan::NodeOrToken::Token(token)) => {
                let expr = lower_bare_token_expr(self, &token);
                self.alloc_expr(expr, span)
            }
            None => self.alloc_expr(Expr::Null, span),
        };

        self.alloc_expr(
            Expr::Call {
                callee: method_target,
                type_args: vec![],
                args: vec![
                    CallArg::positional(name_expr),
                    CallArg::positional(lambda_arg),
                    CallArg::positional(runner_arg),
                ],
            },
            span,
        )
    }

    /// Lower a `TESTSET_DEF` node as a `<collector>.register_test_set(name, sub_collector_lambda, null)` call.
    ///
    /// The sub-collector lambda body is produced by recursively lowering the testset's
    /// `BLOCK_EXPR` body using a nested `LoweringContext` with `testset_collector_var = "testset"`.
    fn lower_testset_as_register_call(&mut self, node: &SyntaxNode) -> ExprId {
        let span = node.span_range();
        let collector_name = self
            .testset_collector_var
            .clone()
            .unwrap_or_else(|| Name::new("testset"));

        // Extract testset name
        let name_expr = self.lower_test_name_expr(node, span);

        // Find the BLOCK_EXPR child (the testset body)
        let body_node_opt = node.children().find(|c| c.kind() == SyntaxKind::BLOCK_EXPR);

        let sub_body = match body_node_opt.as_ref().and_then(|body_node| {
            baml_compiler_syntax::ast::BlockExpr::cast(body_node.clone())
                .map(|block| (block, body_node.span_range()))
        }) {
            Some((block, range)) => {
                self.lower_testset_collector_body(&block, Name::new("testset"), range)
            }
            None => self.alloc_expr(Expr::Null, span),
        };

        let sub_param = Param {
            name: Name::new("testset"),
            type_expr: Some(
                TypeExprKind::Path {
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

        let sub_collector_def = LambdaDef {
            kind: LambdaKind::Anonymous,
            params: vec![sub_param],
            defaults: FunctionDefaults::empty(),
            return_type: None,
            throws: None,
            body: Some(sub_body),
            span,
        };

        // <collector>.register_test_set(name_expr, sub_collector_lambda, runner_or_null)
        let collector_ref = self.alloc_expr(Expr::Path(vec![collector_name]), span);
        let method_target = self.alloc_expr(
            Expr::MemberAccess {
                base: collector_ref,
                member: Name::new("register_test_set"),
            },
            span,
        );
        let sub_collector_arg = self.alloc_expr(Expr::Lambda(Box::new(sub_collector_def)), span);
        let runner_arg = match crate::lower_cst::extract_runner_element(node) {
            Some(rowan::NodeOrToken::Node(runner_node)) => self.lower_expr(&runner_node),
            Some(rowan::NodeOrToken::Token(token)) => {
                let expr = lower_bare_token_expr(self, &token);
                self.alloc_expr(expr, span)
            }
            None => self.alloc_expr(Expr::Null, span),
        };

        self.alloc_expr(
            Expr::Call {
                callee: method_target,
                type_args: vec![],
                args: vec![
                    CallArg::positional(name_expr),
                    CallArg::positional(sub_collector_arg),
                    CallArg::positional(runner_arg),
                ],
            },
            span,
        )
    }

    /// Extract the name expression from a `TEST_EXPR_DEF` or `TESTSET_DEF` node.
    ///
    /// The name can be a plain `STRING_LITERAL` or a `BINARY_EXPR` (e.g. `"check " + case`).
    fn lower_test_name_expr(&mut self, node: &SyntaxNode, span: TextRange) -> ExprId {
        // The name is the first expression child after the keyword (KW_TEST / KW_TESTSET),
        // before KW_WITH or BLOCK_EXPR. It can be any expression node.
        let name_element = node.children_with_tokens().find(|c| {
            let kind = c.kind();
            !matches!(
                kind,
                SyntaxKind::KW_TEST
                    | SyntaxKind::KW_TESTSET
                    | SyntaxKind::KW_WITH
                    | SyntaxKind::BLOCK_EXPR
                    | SyntaxKind::WHITESPACE
                    | SyntaxKind::NEWLINE
                    | SyntaxKind::LINE_COMMENT
            )
        });

        match name_element {
            Some(rowan::NodeOrToken::Node(ref name_node)) => self.lower_expr(name_node),
            Some(rowan::NodeOrToken::Token(ref token)) => {
                let expr = lower_bare_token_expr(self, token);
                self.alloc_expr(expr, token.text_range())
            }
            None => self.alloc_expr(Expr::Literal(Literal::String(String::new())), span),
        }
    }

    fn lower_header_comment(&mut self, node: &SyntaxNode) -> StmtId {
        // HEADER_COMMENT raw text looks like: //# Title or //## Section Name
        // Strip the leading "//" prefix, then count '#' characters for the level
        // (number_of_hashes + 1 so that //# => level=2, //## => level=3, etc.),
        // and use the remaining text (trimmed) as the name.
        let raw = node.text().to_string();
        let after_slashes = raw.strip_prefix("//").unwrap_or(&raw);
        let hash_count = after_slashes.chars().take_while(|c| *c == '#').count();
        let level = hash_count;
        let title = after_slashes[hash_count..].trim();
        let name = if title.is_empty() {
            Name::new("_")
        } else {
            Name::new(title)
        };

        self.alloc_stmt(Stmt::HeaderComment { name, level }, node.span_range())
    }
}

/// Strip string delimiters from raw token text, decoding escape sequences for
/// regular quoted strings and preserving raw string contents verbatim.
fn strip_string_delimiters(text: &str) -> String {
    let text = text.trim();

    let hash_count = text.bytes().take_while(|&b| b == b'#').count();
    if hash_count > 0 {
        let rest = &text[hash_count..];
        let closing = format!("\"{}", &text[..hash_count]);
        if rest.len() >= hash_count + 2 && rest.starts_with('"') && rest.ends_with(&closing) {
            return rest[1..rest.len() - 1 - hash_count].to_string();
        }
    }

    if text.starts_with('"') && text.ends_with('"') && text.len() >= 2 {
        crate::unescape_string_literal(&text[1..text.len() - 1])
    } else {
        text.to_string()
    }
}

/// Parse escape sequences in a byte string literal body (content between the `b"` and `"`).
///
/// Supported escapes: `\n`, `\t`, `\r`, `\0`, `\\`, `\"`, `\xHH` (2 hex digits).
/// Unescaped characters must be ASCII (0-127).
fn parse_byte_string_escapes(input: &str) -> Result<Vec<u8>, String> {
    let mut result = Vec::with_capacity(input.len());
    let mut chars = input.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => result.push(b'\n'),
                Some('t') => result.push(b'\t'),
                Some('r') => result.push(b'\r'),
                Some('0') => result.push(0),
                Some('\\') => result.push(b'\\'),
                Some('"') => result.push(b'"'),
                Some('x') => {
                    let hi = chars
                        .next()
                        .ok_or_else(|| "incomplete \\x escape".to_string())?;
                    let lo = chars
                        .next()
                        .ok_or_else(|| "incomplete \\x escape".to_string())?;
                    let hex_str: String = [hi, lo].iter().collect();
                    let byte = u8::from_str_radix(&hex_str, 16)
                        .map_err(|_| format!("invalid hex escape: \\x{hex_str}"))?;
                    result.push(byte);
                }
                Some(other) => {
                    return Err(format!("unknown escape sequence: \\{other}"));
                }
                None => {
                    return Err("trailing backslash in byte string".to_string());
                }
            }
        } else if !c.is_ascii() {
            return Err(format!(
                "non-ASCII character in byte string: '{c}' (use \\xHH for bytes > 127)"
            ));
        } else {
            result.push(c as u8);
        }
    }
    Ok(result)
}
