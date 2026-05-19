//! CST `ExprFunctionBody` → `(ExprBody, AstSourceMap)`.
//!
//! Adapts the `LoweringContext` from `baml_compiler_hir/src/body.rs` which creates arenas,
//! walks block expressions, etc. Produces `ExprBody` (semantic data) and `AstSourceMap`
//! (parallel span storage) in one pass.

use baml_base::{Name, TypePath};
use baml_compiler_syntax::{SyntaxKind, SyntaxNode};
use la_arena::Arena;
use rowan::ast::AstNode;
use text_size::TextRange;

use crate::{
    LoweringDiagnostic,
    ast::{
        ArrayRestPat, AssignOp, AstSourceMap, BinaryOp, CallArg, CatchArm, CatchArmId, CatchClause,
        CatchClauseKind, DefaultExprId, Expr, ExprBody, ExprId, FieldPat, FunctionBodyDef,
        FunctionDef, FunctionDefaults, LetOrigin, Literal, LoopOrigin, MatchArm, MatchArmId, Param,
        PatId, Pattern, SpannedTypeExpr, SpreadField, Stmt, StmtId, TypeAnnotId, TypeExpr, UnaryOp,
    },
};

/// A reference to an environment variable found in source code (`env.VAR_NAME`).
#[derive(Debug, Clone)]
pub struct EnvVarRef {
    /// The variable name (e.g., `"OPENAI_API_KEY"`).
    pub name: String,
    /// The text range of the entire `env.VAR_NAME` expression in the source.
    pub range: TextRange,
}

/// Returns true if `kind` can serve as an identifier token in expression position.
///
/// The parser allows `KW_CLIENT` (and `WORD`) inside `PATH_EXPR` / `FIELD_ACCESS_EXPR`
/// nodes when `client` is used as a variable or field name. This must match
/// exactly what `parse_path_or_ident` accepts; adding a new keyword there
/// requires adding it here too.
fn is_ident_token(kind: SyntaxKind) -> bool {
    kind == SyntaxKind::WORD || kind == SyntaxKind::KW_CLIENT
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
            find_callee_generic_args(&base)
        }
        _ => None,
    }
}

/// Lower a CST `ExprFunctionBody` to an owned `ExprBody` + parallel `AstSourceMap`.
pub(crate) fn lower(
    expr_body: &baml_compiler_syntax::ast::ExprFunctionBody,
    param_names: &[Name],
    diags: &mut Vec<LoweringDiagnostic>,
    env_var_refs: &mut Vec<EnvVarRef>,
) -> (ExprBody, AstSourceMap) {
    let mut ctx = LoweringContext::new();

    // Add function parameters to scope tracking (for gensym avoidance)
    for name in param_names {
        ctx.names_in_scope.insert(name.to_string());
    }

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

/// Lower a `BLOCK_EXPR` node directly to an owned `ExprBody` + parallel `AstSourceMap`.
///
/// Used by `lower_cst` when synthesizing lambda bodies from `TEST_EXPR_DEF` / `TESTSET_DEF`
/// blocks, where there is no wrapping `EXPR_FUNCTION_BODY` node.
pub(crate) fn lower_block_node(
    block_node: &SyntaxNode,
    param_names: &[Name],
) -> (
    ExprBody,
    AstSourceMap,
    Vec<LoweringDiagnostic>,
    Vec<EnvVarRef>,
) {
    let mut ctx = LoweringContext::new();
    for name in param_names {
        ctx.names_in_scope.insert(name.to_string());
    }
    let root_expr = baml_compiler_syntax::ast::BlockExpr::cast(block_node.clone())
        .map(|block| ctx.lower_block_expr(&block));
    ctx.finish(root_expr)
}

pub(crate) fn lower_default_expr_nodes(
    defaults: &[(usize, baml_compiler_syntax::SyntaxElement)],
    param_names: &[Name],
    diags: &mut Vec<LoweringDiagnostic>,
    env_var_refs: &mut Vec<EnvVarRef>,
) -> (FunctionDefaults, Vec<(usize, DefaultExprId)>) {
    let mut ctx = LoweringContext::new();
    for name in param_names {
        ctx.names_in_scope.insert(name.to_string());
    }

    let mut lowered = Vec::with_capacity(defaults.len());
    for (idx, element) in defaults {
        let expr = match element {
            rowan::NodeOrToken::Node(node) => ctx.lower_expr(node),
            rowan::NodeOrToken::Token(token) => ctx.alloc_expr(
                lower_bare_token_expr(token.kind(), token.text()),
                token.text_range(),
            ),
        };
        lowered.push((*idx, DefaultExprId::new(expr)));
    }

    let (exprs, source_map, ctx_diags, ctx_env_refs) = ctx.finish(None);
    diags.extend(ctx_diags);
    env_var_refs.extend(ctx_env_refs);
    (FunctionDefaults { exprs, source_map }, lowered)
}

/// Lower a testset `BLOCK_EXPR` body node to an owned `ExprBody` + `AstSourceMap`.
///
/// The body may contain a mix of regular statements (let bindings, for loops, if conditions)
/// and `TEST_EXPR_DEF` / `TESTSET_DEF` nodes. The latter are converted to
/// `<collector_var>.register_test(...)` / `<collector_var>.register_test_set(...)` calls
/// so that the resulting body is a valid expression body for the testset collector lambda.
///
/// `collector_var` is the name of the `testing.TestSetCollector` parameter in scope.
/// `param_names` are additional parameters to seed `names_in_scope` (e.g. the parent scope).
///
/// The returned body always has a `null` tail expression so the collector lambda satisfies
/// the type checker's expectation that the body evaluates to `null`.
pub(crate) fn lower_testset_block_node(
    block_node: &SyntaxNode,
    collector_var: &Name,
    param_names: &[Name],
) -> (
    ExprBody,
    AstSourceMap,
    Vec<LoweringDiagnostic>,
    Vec<EnvVarRef>,
) {
    let mut ctx = LoweringContext::new_testset_collector(collector_var.clone());
    ctx.names_in_scope.insert(collector_var.to_string());
    for name in param_names {
        ctx.names_in_scope.insert(name.to_string());
    }
    let range = block_node.text_range();
    let root_expr = baml_compiler_syntax::ast::BlockExpr::cast(block_node.clone()).map(|block| {
        let inner_block_id = ctx.lower_block_expr(&block);
        ctx.ensure_null_tail(inner_block_id, range)
    });
    ctx.finish(root_expr)
}

/// Lower a runner `SyntaxElement` (node or token) into an `ExprId` within the given context.
///
/// If the element is a node (e.g. `CALL_EXPR`, `OBJECT_LITERAL`), delegates to `lower_expr`.
/// If the element is a bare token (e.g. `INTEGER_LITERAL`, `WORD`), lowers inline.
/// Lower a bare token (not wrapped in a CST node) into an `Expr`.
/// Used for runner expressions that are simple literals or identifiers.
fn lower_bare_token_expr(kind: SyntaxKind, text: &str) -> Expr {
    match kind {
        SyntaxKind::INTEGER_LITERAL => {
            if let Ok(v) = text.parse::<i64>() {
                Expr::Literal(Literal::Int(v))
            } else {
                Expr::Missing
            }
        }
        SyntaxKind::FLOAT_LITERAL => Expr::Literal(Literal::Float(text.to_string())),
        k if is_ident_token(k) => match text {
            "null" => Expr::Null,
            "true" => Expr::Literal(Literal::Bool(true)),
            "false" => Expr::Literal(Literal::Bool(false)),
            _ => Expr::Path(vec![Name::new(text)]),
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
            let expr = lower_bare_token_expr(token.kind(), token.text());
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
    /// Counter for generating unique synthetic spans for lambda expressions.
    /// Synthesized lambdas all share span `0..0`, which causes the HIR scope
    /// builder and MIR lowering to confuse them. Each lambda gets a unique
    /// synthetic span at offset `(counter * 2)..(counter * 2 + 1)` to make
    /// them distinguishable.
    synthetic_lambda_counter: u32,
}

impl InitTestContext {
    pub(crate) fn new() -> Self {
        let mut inner = LoweringContext::new();
        inner.names_in_scope.insert("registry".to_string());
        Self {
            inner,
            synthetic_lambda_counter: 0,
        }
    }

    /// Generate a unique synthetic span for a lambda expression.
    /// Each call returns a different 1-byte span to ensure that HIR Lambda
    /// scopes can be distinguished by their `range` field.
    pub(crate) fn next_lambda_span(&mut self) -> text_size::TextRange {
        let offset = self.synthetic_lambda_counter;
        self.synthetic_lambda_counter += 1;
        // Use offsets starting at 1 to avoid collision with the default 0..0 span
        // used for the function itself and non-lambda expressions.
        let start = text_size::TextSize::from((offset + 1) * 2);
        let end = start + text_size::TextSize::from(1);
        text_size::TextRange::new(start, end)
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

struct LoweringContext {
    exprs: Arena<Expr>,
    stmts: Arena<Stmt>,
    patterns: Arena<Pattern>,
    match_arms: Arena<MatchArm>,
    catch_arms: Arena<CatchArm>,
    type_annotations: Arena<TypeExpr>,
    /// Parallel span storage
    source_map: AstSourceMap,
    /// All names used, for generating unique synthetic variable names.
    names_in_scope: std::collections::HashSet<String>,
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
            names_in_scope: std::collections::HashSet::new(),
            testset_collector_var: None,
            diags: Vec::new(),
            env_var_refs: Vec::new(),
            needs_chain_wrap: std::collections::HashSet::new(),
        }
    }

    fn new_testset_collector(collector_var: Name) -> Self {
        let mut ctx = Self::new();
        ctx.testset_collector_var = Some(collector_var);
        ctx
    }

    fn alloc_expr(&mut self, expr: Expr, range: TextRange) -> ExprId {
        let id = self.exprs.alloc(expr);
        self.source_map.expr_spans.alloc(range);
        id
    }

    fn alloc_stmt(&mut self, stmt: Stmt, range: TextRange) -> StmtId {
        let id = self.stmts.alloc(stmt);
        self.source_map.stmt_spans.alloc(range);
        id
    }

    fn alloc_pattern(&mut self, pattern: Pattern, range: TextRange) -> PatId {
        let id = self.patterns.alloc(pattern);
        self.source_map.pattern_spans.alloc(range);
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
            SyntaxKind::INTEGER_LITERAL => {
                let value = token.text().parse::<i64>().unwrap_or(0);
                Some(self.alloc_expr(Expr::Literal(Literal::Int(value)), span))
            }
            SyntaxKind::FLOAT_LITERAL => Some(self.alloc_expr(
                Expr::Literal(Literal::Float(token.text().to_string())),
                span,
            )),
            _ => None,
        }
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
                        SyntaxKind::LET_STMT => self.lower_let_stmt(node, false),
                        SyntaxKind::WATCH_LET => self.lower_let_stmt(node, true),
                        SyntaxKind::RETURN_STMT => self.lower_return_stmt(node),
                        SyntaxKind::THROW_STMT => self.lower_throw_stmt(node),
                        SyntaxKind::WHILE_STMT => self.lower_while_stmt(node),
                        SyntaxKind::FOR_EXPR => self.lower_for_stmt(node),
                        SyntaxKind::BREAK_STMT => self.alloc_stmt(Stmt::Break, node.text_range()),
                        SyntaxKind::CONTINUE_STMT => {
                            self.alloc_stmt(Stmt::Continue, node.text_range())
                        }
                        SyntaxKind::TEST_EXPR_DEF => {
                            if self.testset_collector_var.is_some() {
                                let expr_id = self.lower_test_expr_as_register_call(node);
                                self.alloc_stmt(Stmt::Expr(expr_id), node.text_range())
                            } else {
                                // Invalid context — parser already emitted a diagnostic
                                self.alloc_stmt(Stmt::Missing, node.text_range())
                            }
                        }
                        SyntaxKind::TESTSET_DEF => {
                            if self.testset_collector_var.is_some() {
                                let expr_id = self.lower_testset_as_register_call(node);
                                self.alloc_stmt(Stmt::Expr(expr_id), node.text_range())
                            } else {
                                // Invalid context — parser already emitted a diagnostic
                                self.alloc_stmt(Stmt::Missing, node.text_range())
                            }
                        }
                        _ => self.alloc_stmt(Stmt::Missing, node.text_range()),
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
                        stmts.push(self.alloc_stmt(Stmt::Expr(expr_id), node.text_range()));
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
                        SyntaxKind::INTEGER_LITERAL => {
                            let value = token.text().parse::<i64>().unwrap_or(0);
                            self.alloc_expr(Expr::Literal(Literal::Int(value)), span)
                        }
                        SyntaxKind::FLOAT_LITERAL => {
                            let text = token.text().to_string();
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
            block.syntax().text_range(),
        )
    }

    /// General entry point — wraps any unwrapped optional chain.
    fn lower_expr(&mut self, node: &SyntaxNode) -> ExprId {
        let id = self.lower_expr_inner(node);
        if self.needs_chain_wrap.remove(&id) {
            self.alloc_expr(Expr::OptionalChain { expr: id }, node.text_range())
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
            SyntaxKind::MATCH_EXPR => self.lower_match_expr(node),
            SyntaxKind::CATCH_EXPR => self.lower_catch_expr(node),
            SyntaxKind::THROW_EXPR => self.lower_throw_expr(node),
            SyntaxKind::BLOCK_EXPR => {
                if let Some(block) = baml_compiler_syntax::ast::BlockExpr::cast(node.clone()) {
                    self.lower_block_expr(&block)
                } else {
                    self.alloc_expr(Expr::Missing, node.text_range())
                }
            }
            SyntaxKind::PATH_EXPR => self.lower_path_expr(node),
            SyntaxKind::FIELD_ACCESS_EXPR => self.lower_field_access_expr(node),
            SyntaxKind::OPTIONAL_FIELD_ACCESS_EXPR => self.lower_optional_field_access_expr(node),
            SyntaxKind::ENV_ACCESS_EXPR => self.lower_env_access_expr(node),
            SyntaxKind::INDEX_EXPR => self.lower_index_expr(node),
            SyntaxKind::OPTIONAL_INDEX_EXPR => self.lower_optional_index_expr(node),
            SyntaxKind::OPTIONAL_CALL_EXPR => self.lower_optional_call_expr(node),
            SyntaxKind::PAREN_EXPR => {
                if let Some(inner) = node.children().next() {
                    self.lower_expr(&inner)
                } else {
                    self.try_lower_paren_token_content(node)
                        .unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.text_range()))
                }
            }
            SyntaxKind::STRING_LITERAL | SyntaxKind::RAW_STRING_LITERAL => {
                self.lower_string_literal(node)
            }
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
                    self.alloc_expr(Expr::Missing, node.text_range())
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
    fn lower_spawn_expr(&mut self, node: &SyntaxNode) -> ExprId {
        use baml_compiler_syntax::ast as cst_ast;

        let mut name: Option<ExprId> = None;
        let mut body_lambda: Option<ExprId> = None;

        for child in node.children() {
            if child.kind() == SyntaxKind::BLOCK_EXPR {
                // Synthesize a 0-arg lambda whose body is this block —
                // mirroring `lower_lambda_expr` so the existing
                // capture / scope / MIR plumbing applies unchanged.
                let block = cst_ast::BlockExpr::cast(child.clone());
                let func_def = block.map(|block| {
                    let mut lambda_ctx = LoweringContext::new();
                    let root_expr = lambda_ctx.lower_block_expr(&block);
                    let (lbody, source_map, lambda_diags, lambda_env_refs) =
                        lambda_ctx.finish(Some(root_expr));
                    self.diags.extend(lambda_diags);
                    self.env_var_refs.extend(lambda_env_refs);
                    FunctionDef {
                        name: Name::new("<spawn>"),
                        generic_params: Vec::new(),
                        params: Vec::new(),
                        defaults: crate::ast::FunctionDefaults::empty(),
                        return_type: None,
                        throws: None,
                        body: Some(FunctionBodyDef::Expr(lbody, source_map)),
                        declarative_meta: None,
                        origin: crate::ast::FunctionOrigin::Internal,
                        attributes: Vec::new(),
                        docstring: None,
                        span: child.text_range(),
                        name_span: child.text_range(),
                    }
                });
                if let Some(fd) = func_def {
                    body_lambda =
                        Some(self.alloc_expr(Expr::Lambda(Box::new(fd)), child.text_range()));
                }
            } else if name.is_none() {
                name = Some(self.lower_expr(&child));
            }
        }

        let body = body_lambda.unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.text_range()));
        self.alloc_expr(Expr::Spawn { name, body }, node.text_range())
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
                    return self.alloc_expr(Expr::Await { future }, node.text_range());
                }
                rowan::NodeOrToken::Token(token) if token.kind() != SyntaxKind::KW_AWAIT => {
                    if let Some(future) = self.try_lower_bare_token(&token) {
                        return self.alloc_expr(Expr::Await { future }, node.text_range());
                    }
                }
                _ => {}
            }
        }
        let future = self.alloc_expr(Expr::Missing, node.text_range());
        self.alloc_expr(Expr::Await { future }, node.text_range())
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
                            return self.alloc_expr(Expr::Missing, node.text_range());
                        }
                        SyntaxKind::QUESTION_QUESTION => op = Some(BinaryOp::NullCoalesce),
                        SyntaxKind::QUESTION if op.is_none() => {
                            // Two consecutive QUESTION tokens = null coalescing (??)
                            // The parser emits them as two separate tokens in BINARY_EXPR.
                            // First QUESTION sets a provisional op; second one confirms.
                            op = Some(BinaryOp::NullCoalesce);
                        }
                        SyntaxKind::INTEGER_LITERAL => {
                            let value = token.text().parse::<i64>().unwrap_or(0);
                            let expr_id = self.alloc_expr(Expr::Literal(Literal::Int(value)), span);
                            if lhs.is_none() {
                                lhs = Some(expr_id);
                            } else {
                                rhs = Some(expr_id);
                            }
                        }
                        SyntaxKind::FLOAT_LITERAL => {
                            let expr_id = self.alloc_expr(
                                Expr::Literal(Literal::Float(token.text().to_string())),
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
                        // which is not a valid expression — emit Missing instead of
                        // silently defaulting to BinaryOp::Add.
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
                            return self.alloc_expr(Expr::Missing, node.text_range());
                        }
                        _ => {}
                    }
                }
            }
        }

        let lhs = lhs.unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.text_range()));
        let rhs = rhs.unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.text_range()));
        let op = op.unwrap_or(BinaryOp::Add);

        self.alloc_expr(Expr::Binary { op, lhs, rhs }, node.text_range())
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
                            SyntaxKind::INTEGER_LITERAL => {
                                let value = token.text().parse::<i64>().unwrap_or(0);
                                scrutinee =
                                    Some(self.alloc_expr(Expr::Literal(Literal::Int(value)), span));
                            }
                            SyntaxKind::FLOAT_LITERAL => {
                                scrutinee = Some(self.alloc_expr(
                                    Expr::Literal(Literal::Float(token.text().to_string())),
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

        let span = node.text_range();
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
                        SyntaxKind::INTEGER_LITERAL => {
                            let value = token.text().parse::<i64>().unwrap_or(0);
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

        let target = lhs.unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.text_range()));
        let value = rhs.unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.text_range()));

        let stmt = match assign_op {
            None => Stmt::Assign { target, value },
            Some(op) => Stmt::AssignOp { target, op, value },
        };

        Some(self.alloc_stmt(stmt, node.text_range()))
    }

    fn lower_unary_expr(&mut self, node: &SyntaxNode) -> ExprId {
        let mut op = None;
        let mut operand = None;
        let mut double_op = false;

        for elem in node.children_with_tokens() {
            match elem {
                rowan::NodeOrToken::Node(child_node) => {
                    operand = Some(self.lower_expr(&child_node));
                }
                rowan::NodeOrToken::Token(token) => {
                    let span = token.text_range();
                    match token.kind() {
                        SyntaxKind::NOT => op = Some(UnaryOp::Not),
                        SyntaxKind::MINUS => op = Some(UnaryOp::Neg),
                        SyntaxKind::MINUS_MINUS => {
                            op = Some(UnaryOp::Neg);
                            double_op = true;
                        }
                        SyntaxKind::INTEGER_LITERAL => {
                            let value = token.text().parse::<i64>().unwrap_or(0);
                            operand =
                                Some(self.alloc_expr(Expr::Literal(Literal::Int(value)), span));
                        }
                        SyntaxKind::FLOAT_LITERAL => {
                            operand = Some(self.alloc_expr(
                                Expr::Literal(Literal::Float(token.text().to_string())),
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

        let expr = operand.unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.text_range()));

        let Some(op) = op else {
            return expr;
        };

        let result = self.alloc_expr(Expr::Unary { op, expr }, node.text_range());

        if double_op {
            self.alloc_expr(Expr::Unary { op, expr: result }, node.text_range())
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
            .unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.text_range()));

        let then_branch = sub_exprs
            .get(1)
            .copied()
            .unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.text_range()));

        let else_branch = sub_exprs.get(2).copied();

        self.alloc_expr(
            Expr::If {
                condition,
                then_branch,
                else_branch,
            },
            node.text_range(),
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
                            let span = child.text_range();
                            let ty = crate::lower_type_expr::lower_type_expr_node(&type_expr);
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
                            SyntaxKind::INTEGER_LITERAL => {
                                let value = token.text().parse::<i64>().unwrap_or(0);
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
            node.text_range(),
        )
    }

    fn lower_match_arm(&mut self, node: &SyntaxNode) -> MatchArmId {
        let arm_span = node.text_range();
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
                                        SyntaxKind::INTEGER_LITERAL => {
                                            let value = t.text().parse::<i64>().unwrap_or(0);
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
                    SyntaxKind::INTEGER_LITERAL if seen_fat_arrow && body.is_none() => {
                        let value = token.text().parse::<i64>().unwrap_or(0);
                        body = Some(
                            self.alloc_expr(Expr::Literal(Literal::Int(value)), token.text_range()),
                        );
                    }
                    SyntaxKind::FLOAT_LITERAL if seen_fat_arrow && body.is_none() => {
                        let text = token.text().to_string();
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
            None => self.alloc_pattern(Pattern::Wildcard, node.text_range()),
        }
    }

    /// Dispatch on the kind of an atom-shaped pattern node
    /// (`UNION_PATTERN`, `BINDING_PATTERN`, `WILDCARD_PATTERN`,
    /// `DESTRUCTURE_PATTERN`, `ARRAY_PATTERN`, `TYPE_PATTERN`,
    /// `PAREN_PATTERN`). Returns a fresh `PatId`.
    fn lower_pattern_atom_node(&mut self, node: &SyntaxNode) -> PatId {
        match node.kind() {
            SyntaxKind::UNION_PATTERN => self.lower_union_pattern(node),
            SyntaxKind::BINDING_PATTERN => self.lower_binding_pattern(node),
            SyntaxKind::WILDCARD_PATTERN => {
                self.alloc_pattern(Pattern::Wildcard, node.text_range())
            }
            SyntaxKind::DESTRUCTURE_PATTERN => self.lower_destructure_pattern(node),
            SyntaxKind::ARRAY_PATTERN => self.lower_array_pattern(node),
            SyntaxKind::TYPE_PATTERN => self.lower_type_pattern(node),
            SyntaxKind::PAREN_PATTERN => {
                match node.children().find(|n| n.kind() == SyntaxKind::PATTERN) {
                    Some(inner) => self.lower_pattern(&inner),
                    None => self.alloc_pattern(Pattern::Wildcard, node.text_range()),
                }
            }
            // Defensive: an unexpected node where a pattern atom should be.
            // Lower as wildcard so downstream doesn't crash; the parse error
            // (if any) will surface elsewhere.
            _ => self.alloc_pattern(Pattern::Wildcard, node.text_range()),
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
            0 => self.alloc_pattern(Pattern::Wildcard, node.text_range()),
            1 => parts[0],
            _ => self.alloc_pattern(Pattern::Or(parts), node.text_range()),
        }
    }

    /// Lower a `BINDING_PATTERN` (`let WORD`). The parser routes `let _` to
    /// `WILDCARD_PATTERN` before it ever reaches here, so the WORD's text is
    /// never `_`. The only defensive case is a malformed `let` without a
    /// following WORD (parse error like `let = 1`), which we recover as
    /// wildcard.
    fn lower_binding_pattern(&mut self, node: &SyntaxNode) -> PatId {
        let name = node
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .find(|t| t.kind() == SyntaxKind::WORD)
            .map(|t| Name::new(t.text()));

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
        self.alloc_pattern(pat, node.text_range())
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
            return self.alloc_pattern(Pattern::Wildcard, node.text_range());
        };
        let ty = crate::lower_type_expr::lower_type_expr_node(&type_expr);
        self.alloc_pattern(Pattern::Type(ty), node.text_range())
    }

    /// Lower a `DESTRUCTURE_PATTERN` (`(let)? PATH ('<' types '>')? '{' field_list '}'`).
    fn lower_destructure_pattern(&mut self, node: &SyntaxNode) -> PatId {
        // Path tokens live between (the optional) `KW_LET` and either
        // `GENERIC_ARGS` or `L_BRACE`.
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
                rowan::NodeOrToken::Node(n) if n.kind() == SyntaxKind::GENERIC_ARGS => break,
                rowan::NodeOrToken::Node(_) => {}
            }
        }

        let generic_args: Vec<TypeExpr> = node
            .children()
            .find(|n| n.kind() == SyntaxKind::GENERIC_ARGS)
            .into_iter()
            .flat_map(|args_node| args_node.children())
            .filter(|n| n.kind() == SyntaxKind::TYPE_EXPR)
            .filter_map(baml_compiler_syntax::ast::TypeExpr::cast)
            .map(|te| crate::lower_type_expr::lower_type_expr_node(&te))
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
                fields,
            },
            node.text_range(),
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
            .unwrap_or_else(|| node.text_range());
        let field_name = field_token
            .map(|t| Name::new(t.text()))
            .unwrap_or_else(|| Name::new("_"));

        let value_pattern = node.children().find(|n| n.kind() == SyntaxKind::PATTERN);

        let pat = match value_pattern {
            Some(child) => self.lower_pattern(&child),
            None => {
                // Shorthand `{ f }` → bind to a local of the same name. `_`
                // canonicalises to `Wildcard`, same rule as elsewhere.
                let synth = if field_name.as_str() == "_" {
                    Pattern::Wildcard
                } else {
                    Pattern::Bind {
                        name: field_name.clone(),
                        subpat: None,
                    }
                };
                self.alloc_pattern(synth, node.text_range())
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
            .map(|type_expr| crate::lower_type_expr::lower_type_expr_node(&type_expr));

        self.alloc_pattern(
            Pattern::Array {
                prefix,
                rest,
                suffix,
                ascription,
            },
            node.text_range(),
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

        let base = base.unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.text_range()));
        self.alloc_expr(Expr::Catch { base, clauses }, node.text_range())
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
                    SyntaxKind::WORD if token.text() == "catch_all_panics" => {
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
                            child.text_range(),
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
                            child.text_range(),
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
                .unwrap_or_else(|| self.alloc_pattern(Pattern::Wildcard, node.text_range())),
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
                    SyntaxKind::INTEGER_LITERAL if seen_fat_arrow && body.is_none() => {
                        let value = token.text().parse::<i64>().unwrap_or(0);
                        body = Some(
                            self.alloc_expr(Expr::Literal(Literal::Int(value)), token.text_range()),
                        );
                    }
                    SyntaxKind::FLOAT_LITERAL if seen_fat_arrow && body.is_none() => {
                        body = Some(self.alloc_expr(
                            Expr::Literal(Literal::Float(token.text().to_string())),
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
            None => self.alloc_pattern(Pattern::Wildcard, node.text_range()),
        };
        let body = match body {
            Some(body) => body,
            None => self.alloc_expr(Expr::Missing, node.text_range()),
        };

        self.alloc_catch_arm(CatchArm { pattern, body }, node.text_range())
    }

    fn lower_throw_expr(&mut self, node: &SyntaxNode) -> ExprId {
        let value = if let Some(child) = node.children().next() {
            self.lower_expr(&child)
        } else {
            self.lower_throw_value_token(node)
                .unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.text_range()))
        };
        self.alloc_expr(Expr::Throw { value }, node.text_range())
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
                    | SyntaxKind::ENV_ACCESS_EXPR
                    | SyntaxKind::INDEX_EXPR
                    | SyntaxKind::IF_EXPR
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
                return self.alloc_stmt(Stmt::Expr(expr_id), node.text_range());
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
                            self.alloc_expr(Expr::Missing, throw_expr_node.text_range())
                        })
                }
            })
            .unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.text_range()));

        self.alloc_stmt(Stmt::Throw { value }, node.text_range())
    }

    fn lower_throw_value_token(&mut self, node: &SyntaxNode) -> Option<ExprId> {
        use baml_compiler_syntax::SyntaxKind;

        for token in node
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
        {
            match token.kind() {
                SyntaxKind::KW_THROW => continue,
                SyntaxKind::INTEGER_LITERAL => {
                    let value = token.text().parse::<i64>().unwrap_or(0);
                    return Some(
                        self.alloc_expr(Expr::Literal(Literal::Int(value)), token.text_range()),
                    );
                }
                SyntaxKind::FLOAT_LITERAL => {
                    return Some(self.alloc_expr(
                        Expr::Literal(Literal::Float(token.text().to_string())),
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
        let type_args: Vec<TypeExpr> = callee_node
            .as_ref()
            .and_then(find_callee_generic_args)
            .into_iter()
            .flat_map(|args_node| args_node.children())
            .filter(|n| n.kind() == SyntaxKind::TYPE_EXPR)
            .filter_map(baml_compiler_syntax::ast::TypeExpr::cast)
            .map(|te| crate::lower_type_expr::lower_type_expr_node(&te))
            .collect();

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
                self.alloc_expr(Expr::Missing, node.text_range())
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
            node.text_range(),
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
            self.alloc_expr(
                lower_bare_token_expr(expr_token.kind(), expr_token.text()),
                expr_token.text_range(),
            )
        };

        Some((CallArg { label, expr }, label_span))
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
            // Check for a nested PATH_EXPR child (produced by the parser when
            // `foo.bar<T>` wraps the `foo.bar` PATH_EXPR in an outer PATH_EXPR).
            if let Some(inner) = node.children().find(|n| n.kind() == SyntaxKind::PATH_EXPR) {
                return self.lower_path_expr(&inner);
            }
            return self.alloc_expr(Expr::Missing, node.text_range());
        }

        // Check if single segment is a literal keyword
        if segments.len() == 1 {
            match segments[0].0.as_str() {
                "true" => {
                    return self.alloc_expr(Expr::Literal(Literal::Bool(true)), node.text_range());
                }
                "false" => {
                    return self.alloc_expr(Expr::Literal(Literal::Bool(false)), node.text_range());
                }
                "null" => return self.alloc_expr(Expr::Null, node.text_range()),
                _ => {}
            }
        }

        // Multi-segment paths stay as Path(["a", "b", "c"]).
        // Record per-segment spans for diagnostics and LSP.
        let names: Vec<Name> = segments.iter().map(|(n, _)| n.clone()).collect();
        let id = self.alloc_expr(Expr::Path(names), node.text_range());
        if segments.len() > 1 {
            let spans: Vec<TextRange> = segments.iter().map(|(_, r)| *r).collect();
            self.source_map.path_segment_spans.insert(id, spans);
        }
        id
    }

    fn lower_field_access_expr(&mut self, node: &SyntaxNode) -> ExprId {
        let mut base = None;
        let mut field = None;
        let mut field_range = None;

        for elem in node.children_with_tokens() {
            match elem {
                rowan::NodeOrToken::Node(child) => {
                    if base.is_none() {
                        base = Some(self.lower_expr_in_chain(&child));
                    }
                }
                rowan::NodeOrToken::Token(token) => {
                    if is_ident_token(token.kind()) && base.is_some() {
                        field = Some(Name::new(token.text()));
                        field_range = Some(token.text_range());
                    }
                }
            }
        }

        let base = base.unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.text_range()));
        let member = field.unwrap_or_else(|| Name::new("_"));

        let id = self.alloc_expr(Expr::MemberAccess { base, member }, node.text_range());
        if let Some(range) = field_range {
            self.source_map.member_access_member_spans.insert(id, range);
        }
        if self.needs_chain_wrap.remove(&base) {
            self.needs_chain_wrap.insert(id);
        }
        id
    }

    fn lower_env_access_expr(&mut self, node: &SyntaxNode) -> ExprId {
        // Desugar `env.VAR_NAME` → `baml.env.get_or_panic("VAR_NAME")`
        let range = node.text_range();

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

        let base = base.unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.text_range()));
        let index = index.unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.text_range()));

        let id = self.alloc_expr(Expr::Index { base, index }, node.text_range());
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
                    } else if is_ident_token(token.kind()) {
                        if !seen_question_dot && base.is_none() {
                            // Base is a bare WORD token (e.g. `user` in `user?.name`)
                            base = Some(self.alloc_expr(
                                Expr::Path(vec![Name::new(token.text())]),
                                token.text_range(),
                            ));
                        } else if seen_question_dot {
                            field = Some(Name::new(token.text()));
                            field_range = Some(token.text_range());
                        }
                    }
                }
            }
        }

        let base = base.unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.text_range()));
        let member = field.unwrap_or_else(|| Name::new("_"));

        let id = self.alloc_expr(
            Expr::OptionalMemberAccess { base, member },
            node.text_range(),
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

        let base = base.unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.text_range()));
        let index = index.unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.text_range()));

        let id = self.alloc_expr(Expr::OptionalIndex { base, index }, node.text_range());
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
                self.alloc_expr(Expr::Missing, node.text_range())
            }
        };

        let lowered_args = node
            .children()
            .find(|n| n.kind() == SyntaxKind::CALL_ARGS)
            .map(|args_node| self.lower_call_args_node(&args_node))
            .unwrap_or_default();
        let (args, label_spans) = Self::finalize_call_args(lowered_args);

        let id = self.alloc_expr(Expr::OptionalCall { callee, args }, node.text_range());
        self.record_call_arg_label_spans(id, label_spans);
        self.needs_chain_wrap.remove(&callee);
        self.needs_chain_wrap.insert(id);
        id
    }

    fn lower_string_literal(&mut self, node: &SyntaxNode) -> ExprId {
        let text = node.text().to_string();
        let content = strip_string_delimiters(&text);
        self.alloc_expr(Expr::Literal(Literal::String(content)), node.text_range())
    }

    fn lower_byte_string_literal(&mut self, node: &SyntaxNode) -> ExprId {
        let text = node.text().to_string();
        // Strip the b"..." delimiters: remove leading `b"` and trailing `"`
        let content = text
            .strip_prefix("b\"")
            .and_then(|s| s.strip_suffix('"'))
            .unwrap_or("");
        match parse_byte_string_escapes(content) {
            Ok(bytes) => self.alloc_expr(Expr::ByteStringLiteral(bytes), node.text_range()),
            Err(message) => {
                self.diags
                    .push(LoweringDiagnostic::InvalidByteStringEscape {
                        message,
                        span: node.text_range(),
                    });
                self.alloc_expr(Expr::Missing, node.text_range())
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
        self.alloc_expr(Expr::Array { elements }, node.text_range())
    }

    fn lower_object_literal(&mut self, node: &SyntaxNode) -> ExprId {
        let mut fields = Vec::new();
        let mut spreads = Vec::new();
        let mut position = 0;
        let mut type_name = None;
        let mut type_args: Vec<TypeExpr> = vec![];

        // Look for the optional type name (first WORD or path before the brace):
        //   - A simple WORD token: `MyClass { ... }` → `TypePath::bare`.
        //   - A qualified path node: `baml.errors.DevOther { ... }` (parsed as
        //     PATH_EXPR) → `TypePath` of all the WORD segments.
        //   - A generic path: `Foo<int> { ... }` (parsed as PATH_EXPR with
        //     GENERIC_ARGS child) → `TypePath::bare("Foo")` + `type_args = [int]`.
        'outer: for elem in node.children_with_tokens() {
            match elem {
                rowan::NodeOrToken::Token(token) => {
                    if token.kind() == SyntaxKind::L_BRACE {
                        break;
                    }
                    if is_ident_token(token.kind()) && type_name.is_none() {
                        type_name = Some(TypePath::bare(Name::new(token.text())));
                    }
                }
                rowan::NodeOrToken::Node(child_node) => {
                    let segments: Vec<Name> = child_node
                        .children_with_tokens()
                        .filter_map(rowan::NodeOrToken::into_token)
                        .filter(|t| is_ident_token(t.kind()))
                        .map(|t| Name::new(t.text()))
                        .collect();
                    if !segments.is_empty() {
                        type_name = Some(TypePath::new(segments));
                    }
                    // Also extract explicit generic type args from `Foo<int>` syntax:
                    // PATH_EXPR contains a GENERIC_ARGS child with TYPE_EXPR children.
                    type_args = child_node
                        .children()
                        .find(|n| n.kind() == SyntaxKind::GENERIC_ARGS)
                        .into_iter()
                        .flat_map(|args_node| args_node.children())
                        .filter(|n| n.kind() == SyntaxKind::TYPE_EXPR)
                        .filter_map(baml_compiler_syntax::ast::TypeExpr::cast)
                        .map(|te| crate::lower_type_expr::lower_type_expr_node(&te))
                        .collect();
                    break 'outer;
                }
            }
        }

        // Object fields are child nodes after L_BRACE
        // They come as key-value pairs: WORD COLON expr or SPREAD expr
        for child in node.children() {
            match child.kind() {
                SyntaxKind::OBJECT_FIELD => {
                    // OBJECT_FIELD: WORD COLON expr
                    let mut key = None;
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
                                if key.is_none() {
                                    key = Some(Name::new(t.text()));
                                }
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
                    if let (Some(k), Some(val_id)) = (key, val) {
                        fields.push((k, val_id));
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

        self.alloc_expr(
            Expr::Object {
                type_name,
                type_args,
                fields,
                spreads,
            },
            node.text_range(),
        )
    }

    fn lower_map_literal(&mut self, node: &SyntaxNode) -> ExprId {
        // MAP_LITERAL uses OBJECT_FIELD children (same as OBJECT_LITERAL).
        // Each OBJECT_FIELD: key (WORD or expr), COLON, value expr.
        // For maps the key can also be a string literal or expression.
        let entries = node
            .children()
            .filter(|n| n.kind() == SyntaxKind::OBJECT_FIELD)
            .filter_map(|field_node| {
                // Key: first child node that can be an expression, or first WORD token
                let mut key_expr = None;
                let mut val_expr = None;
                let mut seen_colon = false;

                for elem in field_node.children_with_tokens() {
                    match elem {
                        rowan::NodeOrToken::Token(t) => {
                            if t.kind() == SyntaxKind::COLON {
                                seen_colon = true;
                            } else if !seen_colon && key_expr.is_none() && is_ident_token(t.kind())
                            {
                                let span = t.text_range();
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

                match (key_expr, val_expr) {
                    (Some(k), Some(v)) => Some((k, v)),
                    _ => None,
                }
            })
            .collect();

        self.alloc_expr(Expr::Map { entries }, node.text_range())
    }

    fn lower_lambda_expr(&mut self, node: &SyntaxNode) -> ExprId {
        use baml_compiler_syntax::ast;

        // Extract optional generic params: <T>, <K, V>, etc.
        let generic_params = crate::lower_cst::extract_generic_params(node);

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

        let param_names: Vec<Name> = params.iter().map(|p| p.name.clone()).collect();

        // Lower optional return type: the TYPE_EXPR that is a direct child of the
        // lambda node, appearing after PARAMETER_LIST but before THROWS_CLAUSE/BLOCK_EXPR.
        // We scan children in order, skipping items until after PARAMETER_LIST.
        let return_type = {
            let mut after_params = false;
            let mut found: Option<SpannedTypeExpr> = None;
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
                            found = Some(SpannedTypeExpr {
                                expr: crate::lower_type_expr::lower_type_expr_node(&te),
                                span: child.text_range(),
                            });
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
            .map(|te| SpannedTypeExpr {
                span: te.syntax().text_range(),
                expr: crate::lower_type_expr::lower_type_expr_node(&te),
            });

        // Lower body via a FRESH LoweringContext — lambda gets its own ExprBody.
        let body = node
            .children()
            .find(|n| n.kind() == SyntaxKind::BLOCK_EXPR)
            .and_then(ast::BlockExpr::cast)
            .map(|block| {
                let mut lambda_ctx = LoweringContext::new();
                for name in &param_names {
                    lambda_ctx.names_in_scope.insert(name.to_string());
                }
                let root_expr = lambda_ctx.lower_block_expr(&block);
                let (body, source_map, lambda_diags, lambda_env_refs) =
                    lambda_ctx.finish(Some(root_expr));
                self.diags.extend(lambda_diags);
                self.env_var_refs.extend(lambda_env_refs);
                FunctionBodyDef::Expr(body, source_map)
            });

        let func_def = FunctionDef {
            name: Name::new("<anonymous function>"),
            generic_params,
            params,
            defaults,
            return_type,
            throws,
            body,
            declarative_meta: None,
            origin: crate::ast::FunctionOrigin::Internal,
            attributes: Vec::new(),
            docstring: None,
            span: node.text_range(),
            name_span: node.text_range(), // synthetic: use the lambda span
        };

        self.alloc_expr(Expr::Lambda(Box::new(func_def)), node.text_range())
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
                    SyntaxKind::INTEGER_LITERAL => {
                        let value = token.text().parse::<i64>().unwrap_or(0);
                        return Some(self.alloc_expr(Expr::Literal(Literal::Int(value)), span));
                    }
                    SyntaxKind::FLOAT_LITERAL => {
                        let text = token.text().to_string();
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
            SyntaxKind::INTEGER_LITERAL => {
                let value = token.text().parse::<i64>().unwrap_or(0);
                Some(self.alloc_expr(Expr::Literal(Literal::Int(value)), span))
            }
            SyntaxKind::FLOAT_LITERAL => {
                let text = token.text().to_string();
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

    fn lower_let_stmt(&mut self, node: &SyntaxNode, is_watched: bool) -> StmtId {
        // LET_STMT shape (post-pattern-rewrite):
        //   KW_WATCH? KW_LET? PATTERN EQUALS <init-expr> SEMICOLON?
        //
        // The pattern carries its own `: T` narrow as a Chain link, so all we
        // do here is locate the PATTERN child and the initialiser child.
        let mut pattern_id = None;
        let mut initializer = None;
        let mut seen_equals = false;

        for elem in node.children_with_tokens() {
            match elem {
                rowan::NodeOrToken::Token(token) => match token.kind() {
                    SyntaxKind::EQUALS => seen_equals = true,
                    _ if seen_equals && initializer.is_none() => {
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
                    } else if initializer.is_none() {
                        initializer = Some(self.lower_expr(&child));
                    }
                }
            }
        }

        let pattern =
            pattern_id.unwrap_or_else(|| self.alloc_pattern(Pattern::Wildcard, node.text_range()));

        self.check_pattern_void_in_annotation(pattern, "a let binding annotation");

        let origin = if is_watched {
            // TODO: Handle watched let statements
            LetOrigin::Source
        } else {
            LetOrigin::Source
        };

        self.alloc_stmt(
            Stmt::Let {
                pattern,
                initializer,
                is_watched,
                origin,
            },
            node.text_range(),
        )
    }

    fn lower_return_stmt(&mut self, node: &SyntaxNode) -> StmtId {
        // RETURN_STMT: KW_RETURN expr?
        // Try child nodes first, then fall back to token-level expressions
        let expr = if let Some(child_node) = node.children().next() {
            Some(self.lower_expr(&child_node))
        } else {
            // No child node — check for a token-level expression (e.g. `return 1;`)
            let mut result = None;
            for elem in node.children_with_tokens() {
                if let rowan::NodeOrToken::Token(token) = elem {
                    let span = token.text_range();
                    match token.kind() {
                        SyntaxKind::KW_RETURN | SyntaxKind::SEMICOLON => continue,
                        SyntaxKind::INTEGER_LITERAL => {
                            let value = token.text().parse::<i64>().unwrap_or(0);
                            result =
                                Some(self.alloc_expr(Expr::Literal(Literal::Int(value)), span));
                            break;
                        }
                        SyntaxKind::FLOAT_LITERAL => {
                            let text = token.text().to_string();
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
        };
        self.alloc_stmt(Stmt::Return(expr), node.text_range())
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
            .unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.text_range()));
        let body = sub_exprs
            .get(1)
            .copied()
            .unwrap_or_else(|| self.alloc_expr(Expr::Missing, node.text_range()));

        self.alloc_stmt(
            Stmt::While {
                condition,
                body,
                after: None,
                origin: LoopOrigin::While,
            },
            node.text_range(),
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
        let range = node.text_range();

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
            self.lower_let_stmt(&let_node, false)
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
            let update_range = update_node.text_range();
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
        let span = node.text_range();
        let collector_name = self
            .testset_collector_var
            .clone()
            .unwrap_or_else(|| Name::new("testset"));

        // Extract test name from STRING_LITERAL child (may be a BINARY_EXPR for concatenation)
        let name_expr = self.lower_test_name_expr(node, span);

        // Find the BLOCK_EXPR child (the test body)
        let body_node_opt = node.children().find(|c| c.kind() == SyntaxKind::BLOCK_EXPR);

        let (lambda_body, lambda_source_map, lambda_diags, lambda_env_refs) =
            if let Some(body_node) = body_node_opt {
                // Lower the body using a fresh context (no collector var — test bodies don't nest)
                crate::lower_expr_body::lower_block_node(
                    &body_node,
                    std::slice::from_ref(&collector_name),
                )
            } else {
                // Empty body: produce null
                let mut sub_ctx = LoweringContext::new();
                let null_expr = sub_ctx.alloc_expr(Expr::Null, span);
                sub_ctx.finish(Some(null_expr))
            };
        self.diags.extend(lambda_diags);
        self.env_var_refs.extend(lambda_env_refs);

        let lambda_def = FunctionDef {
            name: Name::new("<test body>"),
            generic_params: vec![],
            params: vec![],
            defaults: FunctionDefaults::empty(),
            return_type: None,
            throws: None,
            body: Some(FunctionBodyDef::Expr(lambda_body, lambda_source_map)),
            declarative_meta: None,
            origin: crate::ast::FunctionOrigin::Internal,
            attributes: vec![],
            docstring: None,
            span,
            name_span: span,
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
                let expr = lower_bare_token_expr(token.kind(), token.text());
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
        let span = node.text_range();
        let collector_name = self
            .testset_collector_var
            .clone()
            .unwrap_or_else(|| Name::new("testset"));

        // Extract testset name
        let name_expr = self.lower_test_name_expr(node, span);

        // Find the BLOCK_EXPR child (the testset body)
        let body_node_opt = node.children().find(|c| c.kind() == SyntaxKind::BLOCK_EXPR);

        let (sub_body, sub_source_map, sub_diags, sub_env_refs) =
            if let Some(body_node) = body_node_opt {
                crate::lower_expr_body::lower_testset_block_node(
                    &body_node,
                    &Name::new("testset"),
                    std::slice::from_ref(&collector_name),
                )
            } else {
                let mut sub_ctx = LoweringContext::new();
                let null_expr = sub_ctx.alloc_expr(Expr::Null, span);
                sub_ctx.finish(Some(null_expr))
            };
        self.diags.extend(sub_diags);
        self.env_var_refs.extend(sub_env_refs);

        let sub_param = Param {
            name: Name::new("testset"),
            type_expr: Some(SpannedTypeExpr {
                expr: TypeExpr::Path {
                    segments: vec![Name::new("testing"), Name::new("TestCollector")],
                    generic_args: vec![],
                    attrs: vec![],
                },
                span,
            }),
            default: None,
            span,
            name_span: span,
        };

        let sub_collector_def = FunctionDef {
            name: Name::new("<testset collector>"),
            generic_params: vec![],
            params: vec![sub_param],
            defaults: FunctionDefaults::empty(),
            return_type: None,
            throws: None,
            body: Some(FunctionBodyDef::Expr(sub_body, sub_source_map)),
            declarative_meta: None,
            origin: crate::ast::FunctionOrigin::Internal,
            attributes: vec![],
            docstring: None,
            span,
            name_span: span,
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
                let expr = lower_bare_token_expr(token.kind(), token.text());
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
                let expr = lower_bare_token_expr(token.kind(), token.text());
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

        self.alloc_stmt(Stmt::HeaderComment { name, level }, node.text_range())
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
