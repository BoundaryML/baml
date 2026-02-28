//! Lowering from HIR + TIR type information to VIR.
//!
//! This module converts the HIR expression tree (with TIR type annotations)
//! into our unified expression-based IR where everything is an expression.
//!
//! # Fallible Lowering
//!
//! Lowering is fallible - it returns `Result<ExprBody, LoweringError>`.
//! Any `Missing` nodes in the HIR will cause lowering to fail. This is
//! intentional: VIR represents only valid, complete programs suitable
//! for code generation.
//!
//! # Weaving Strategy (inspired by xiaolong)
//!
//! We translate each statement/expression independently, then weave them together:
//!
//! 1. Translate each statement to an expression
//! 2. `Let` bindings get a placeholder body (marked with a sentinel)
//! 3. Thread expressions right-to-left:
//!    - If current is a dangling `Let`, its body becomes the accumulated result
//!    - Otherwise, wrap with `Seq(current, result)`
//! 4. If block has no tail expression, the final result is `Unit`

use baml_base::Span;
use baml_compiler_hir::{
    ExprBody as HirExprBody, ExprId as HirExprId, FunctionBody, HirSourceMap, StmtId as HirStmtId,
};
use baml_compiler_tir::{InferenceResult, TypeResolutionContext};
use la_arena::Arena;
use rustc_hash::FxHashMap;

use crate::{
    AssignOp, BinaryOp, Expr, ExprBody, ExprId, Literal, MatchArm, PatId, Pattern, SpreadField, Ty,
    UnaryOp,
};

/// Error that occurs when lowering HIR to VIR.
///
/// These errors are internal signals that codegen should be skipped.
/// Missing nodes are already reported as user-facing diagnostics by earlier
/// compiler phases (parser, HIR validation), but we include span information
/// for better debugging when errors occur at runtime initialization.
#[derive(Debug, Clone)]
pub enum LoweringError {
    /// Encountered a Missing expression node.
    MissingExpression {
        /// The span where the missing expression was expected.
        span: Span,
    },
    /// Encountered a Missing statement node.
    MissingStatement {
        /// The span where the missing statement was expected.
        span: Span,
    },
    /// Function body is missing.
    MissingBody,
    /// LLM function - no expression body to lower.
    LlmFunction,
    /// No root expression in the body.
    NoRootExpression,
    /// Invalid retry policy value encountered during compilation metadata extraction.
    InvalidRetryPolicyValue {
        policy_name: String,
        field_name: String,
        value: String,
        reason: String,
    },
    /// Error occurred while lowering a specific function.
    InFunction {
        function_name: String,
        error: Box<LoweringError>,
    },
    /// The project has diagnostics errors and cannot be compiled.
    HasDiagnosticsErrors,
}

impl LoweringError {
    /// Wrap this error with function context.
    #[must_use]
    pub fn in_function(self, function_name: impl Into<String>) -> Self {
        LoweringError::InFunction {
            function_name: function_name.into(),
            error: Box::new(self),
        }
    }

    /// Get the innermost span from the error chain, if any.
    pub fn span(&self) -> Option<Span> {
        match self {
            LoweringError::MissingExpression { span } => Some(*span),
            LoweringError::MissingStatement { span } => Some(*span),
            LoweringError::InFunction { error, .. } => error.span(),
            _ => None,
        }
    }
}

impl std::fmt::Display for LoweringError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoweringError::MissingExpression { span } => {
                write!(
                    f,
                    "missing expression at file {}:{}..{}",
                    span.file_id,
                    u32::from(span.range.start()),
                    u32::from(span.range.end())
                )
            }
            LoweringError::MissingStatement { span } => {
                write!(
                    f,
                    "missing statement at file {}:{}..{}",
                    span.file_id,
                    u32::from(span.range.start()),
                    u32::from(span.range.end())
                )
            }
            LoweringError::MissingBody => write!(f, "function body is missing"),
            LoweringError::LlmFunction => write!(f, "LLM function - no MIR"),
            LoweringError::NoRootExpression => write!(f, "no root expression in body"),
            LoweringError::InvalidRetryPolicyValue {
                policy_name,
                field_name,
                value,
                reason,
            } => write!(
                f,
                "invalid retry policy value: `{policy_name}.{field_name}` = `{value}` ({reason})"
            ),
            LoweringError::InFunction {
                function_name,
                error,
            } => {
                write!(f, "in function '{function_name}': {error}")
            }
            LoweringError::HasDiagnosticsErrors => {
                write!(
                    f,
                    "the project has diagnostics errors and cannot be compiled"
                )
            }
        }
    }
}

impl std::error::Error for LoweringError {}

impl baml_compiler_diagnostics::ToDiagnostic for LoweringError {
    fn to_diagnostic(&self) -> baml_compiler_diagnostics::Diagnostic {
        use baml_compiler_diagnostics::{Diagnostic, DiagnosticId, DiagnosticPhase};

        let diag = Diagnostic::error(DiagnosticId::LoweringError, self.to_string());

        let diag = if let Some(span) = self.span() {
            diag.with_primary_span(span)
        } else {
            diag
        };

        diag.with_phase(DiagnosticPhase::Hir)
    }
}

/// Lower a function body from HIR to VIR.
///
/// Returns `Err` if the HIR contains any `Missing` nodes or is otherwise
/// not suitable for code generation.
pub fn lower_from_hir(
    body: &FunctionBody,
    inference: &InferenceResult,
    resolution_ctx: &TypeResolutionContext,
    type_aliases: &std::collections::HashMap<baml_base::Name, baml_compiler_tir::Ty>,
    recursive_aliases: &std::collections::HashSet<baml_base::Name>,
) -> Result<ExprBody, LoweringError> {
    match body {
        FunctionBody::Expr(hir_body, source_map) => {
            let ctx = LoweringContext::new(
                inference,
                resolution_ctx,
                source_map,
                type_aliases,
                recursive_aliases,
            );
            ctx.lower_expr_body(hir_body)
        }
        FunctionBody::Llm(_) => {
            // LLM bodies don't have expression trees - they're handled specially
            Err(LoweringError::LlmFunction)
        }
        FunctionBody::Missing => Err(LoweringError::MissingBody),
    }
}

/// Sentinel value for dangling Let scopes (body not yet filled in).
const DANGLING_SCOPE: u32 = u32::MAX;

/// Default Null type used as a fallback in expression type lookups.
fn ty_null_default() -> Ty {
    Ty::Null {
        attr: baml_type::TyAttr::default(),
    }
}

/// Builder for constructing `ExprBody`.
struct ExprBodyBuilder {
    exprs: Arena<Expr>,
    patterns: Arena<Pattern>,
    expr_types: FxHashMap<ExprId, Ty>,
    enum_variant_exprs: FxHashMap<ExprId, (baml_base::Name, baml_base::Name)>,
    resolutions: FxHashMap<ExprId, baml_compiler_tir::ResolvedValue>,
    source_spans: FxHashMap<ExprId, Span>,
}

impl ExprBodyBuilder {
    fn new() -> Self {
        Self {
            exprs: Arena::new(),
            patterns: Arena::new(),
            expr_types: FxHashMap::default(),
            enum_variant_exprs: FxHashMap::default(),
            resolutions: FxHashMap::default(),
            source_spans: FxHashMap::default(),
        }
    }

    fn alloc(&mut self, expr: Expr, ty: Ty) -> ExprId {
        let id = self.exprs.alloc(expr);
        self.expr_types.insert(id, ty);
        id
    }

    fn alloc_pattern(&mut self, pattern: Pattern) -> PatId {
        self.patterns.alloc(pattern)
    }

    fn alloc_unit(&mut self) -> ExprId {
        self.alloc(
            Expr::Unit,
            Ty::Void {
                attr: baml_type::TyAttr::default(),
            },
        )
    }

    fn ty(&self, id: ExprId) -> Ty {
        self.expr_types
            .get(&id)
            .cloned()
            .unwrap_or_else(ty_null_default)
    }

    fn finish(self, root: ExprId) -> ExprBody {
        ExprBody {
            exprs: self.exprs,
            patterns: self.patterns,
            expr_types: self.expr_types,
            enum_variant_exprs: self.enum_variant_exprs,
            resolutions: self.resolutions,
            source_spans: self.source_spans,
            root,
        }
    }

    fn record_source_span(&mut self, id: ExprId, span: Span) {
        self.source_spans.insert(id, span);
    }

    fn record_enum_variant(
        &mut self,
        id: ExprId,
        enum_name: baml_base::Name,
        variant: baml_base::Name,
    ) {
        self.enum_variant_exprs.insert(id, (enum_name, variant));
    }

    /// Record the resolution for a VIR expression.
    fn record_resolution(&mut self, id: ExprId, resolution: baml_compiler_tir::ResolvedValue) {
        self.resolutions.insert(id, resolution);
    }
}

/// Context for lowering HIR to VIR.
struct LoweringContext<'a> {
    inference: &'a InferenceResult,
    resolution_ctx: &'a TypeResolutionContext,
    source_map: &'a HirSourceMap,
    builder: ExprBodyBuilder,
    type_aliases: &'a std::collections::HashMap<baml_base::Name, baml_compiler_tir::Ty>,
    recursive_aliases: &'a std::collections::HashSet<baml_base::Name>,
}

impl<'a> LoweringContext<'a> {
    fn new(
        inference: &'a InferenceResult,
        resolution_ctx: &'a TypeResolutionContext,
        source_map: &'a HirSourceMap,
        type_aliases: &'a std::collections::HashMap<baml_base::Name, baml_compiler_tir::Ty>,
        recursive_aliases: &'a std::collections::HashSet<baml_base::Name>,
    ) -> Self {
        Self {
            inference,
            resolution_ctx,
            source_map,
            builder: ExprBodyBuilder::new(),
            type_aliases,
            recursive_aliases,
        }
    }

    /// Lower a complete HIR expression body.
    fn lower_expr_body(mut self, hir_body: &HirExprBody) -> Result<ExprBody, LoweringError> {
        let root_hir = hir_body.root_expr.ok_or(LoweringError::NoRootExpression)?;
        let root = self.lower_expr(root_hir, hir_body)?;
        Ok(self.builder.finish(root))
    }

    /// Lower an HIR expression to VIR.
    fn lower_expr(
        &mut self,
        hir_id: HirExprId,
        hir_body: &HirExprBody,
    ) -> Result<ExprId, LoweringError> {
        use baml_compiler_hir::Expr as HirExpr;

        let hir_expr = &hir_body.exprs[hir_id];

        // Get type from TIR inference
        let tir_ty = self.inference.expr_types.get(&hir_id);
        let ty = tir_ty.map(|ty| self.lower_ty(ty)).unwrap_or(Ty::Null {
            attr: baml_type::TyAttr::default(),
        });

        let result = match hir_expr {
            HirExpr::Missing => {
                let span = self.source_map.expr_span(hir_id).unwrap_or_default();
                Err(LoweringError::MissingExpression { span })
            }

            HirExpr::Literal(lit) => Ok(self.builder.alloc(Expr::Literal(Literal::from(lit)), ty)),

            HirExpr::Path(segments) => {
                if segments.len() == 1 {
                    let expr_id = self.builder.alloc(Expr::Var(segments[0].clone()), ty);
                    // Copy resolution info if available (for local variables, functions, etc.)
                    if let Some(resolution) = self.inference.expr_resolutions.get(&hir_id) {
                        self.builder.record_resolution(expr_id, resolution.clone());
                    }
                    Ok(expr_id)
                } else if let Some(segment_types) = self.inference.path_segment_types.get(&hir_id) {
                    // Local variable with field accesses (e.g., obj.field.subfield)
                    // Convert to nested FieldAccess for proper type tracking.
                    // segment_types[0] = type of first segment (variable)
                    // segment_types[i] = type after i-th field access

                    // Get the corresponding resolutions (computed in TIR)
                    let segment_resolutions = self.inference.path_segment_resolutions.get(&hir_id);

                    // Start with the variable (first segment)
                    let first_ty = segment_types
                        .first()
                        .map(|ty| self.lower_ty(ty))
                        .unwrap_or_else(|| {
                            panic!("BUG: path_segment_types is empty for path {segments:?}")
                        });
                    let mut current = self.builder.alloc(Expr::Var(segments[0].clone()), first_ty);

                    // Record resolution for the first segment (from TIR)
                    if let Some(resolutions) = segment_resolutions {
                        if let Some(first_resolution) = resolutions.first() {
                            self.builder
                                .record_resolution(current, first_resolution.clone());
                        }
                    }

                    // Build nested FieldAccess for remaining segments
                    for (i, field) in segments[1..].iter().enumerate() {
                        // Type after this field access is segment_types[i+1]
                        let result_ty = segment_types
                            .get(i + 1)
                            .map(|ty| self.lower_ty(ty))
                            .unwrap_or_else(|| {
                                panic!(
                                    "BUG: path_segment_types missing type at index {} for path {:?}",
                                    i + 1, segments
                                )
                            });
                        let expr_id = self.builder.alloc(
                            Expr::FieldAccess {
                                base: current,
                                field: field.clone(),
                            },
                            result_ty,
                        );

                        // Record resolution for this segment (from TIR)
                        // Resolution index is i+1 (since first segment is at index 0)
                        if let Some(resolutions) = segment_resolutions {
                            if let Some(resolution) = resolutions.get(i + 1) {
                                self.builder.record_resolution(expr_id, resolution.clone());
                            }
                        }

                        current = expr_id;
                    }

                    Ok(current)
                } else {
                    // Non-local path (e.g., builtin function like baml.Array.length, enum variant)
                    // Keep as Expr::Path - resolution info is carried for MIR lowering.
                    //
                    // TODO: The type here may be incorrect for generic builtins like baml.Array.length
                    // which should have type `fn(Array<T>) -> int` but generics are currently hacked
                    // and not properly implemented. When real generics are added, this will need
                    // proper type instantiation.
                    let expr_id = self.builder.alloc(Expr::Path(segments.clone()), ty);

                    // Copy resolution info (for builtins, functions, enum variants, etc.)
                    if let Some(resolution) = self.inference.expr_resolutions.get(&hir_id) {
                        self.builder.record_resolution(expr_id, resolution.clone());
                    }

                    // Check if this path is an enum variant and record it for MIR lowering
                    if let Some((enum_name, variant)) =
                        self.inference.enum_variant_exprs.get(&hir_id)
                    {
                        self.builder.record_enum_variant(
                            expr_id,
                            enum_name.clone(),
                            variant.clone(),
                        );
                    }

                    Ok(expr_id)
                }
            }

            HirExpr::Binary { op, lhs, rhs } => {
                let lhs_id = self.lower_expr(*lhs, hir_body)?;
                let rhs_id = self.lower_expr(*rhs, hir_body)?;
                Ok(self.builder.alloc(
                    Expr::Binary {
                        op: BinaryOp::from(*op),
                        lhs: lhs_id,
                        rhs: rhs_id,
                    },
                    ty,
                ))
            }

            HirExpr::Unary { op, expr } => {
                let operand = self.lower_expr(*expr, hir_body)?;
                Ok(self.builder.alloc(
                    Expr::Unary {
                        op: UnaryOp::from(*op),
                        operand,
                    },
                    ty,
                ))
            }

            HirExpr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let cond = self.lower_expr(*condition, hir_body)?;
                let then_b = self.lower_expr(*then_branch, hir_body)?;
                let else_b = match else_branch {
                    Some(e) => Some(self.lower_expr(*e, hir_body)?),
                    None => None,
                };
                Ok(self.builder.alloc(
                    Expr::If {
                        condition: cond,
                        then_branch: then_b,
                        else_branch: else_b,
                    },
                    ty,
                ))
            }

            HirExpr::Call { callee, args } => {
                let callee_id = self.lower_expr(*callee, hir_body)?;
                let mut arg_ids = Vec::with_capacity(args.len());
                for a in args {
                    arg_ids.push(self.lower_expr(*a, hir_body)?);
                }
                Ok(self.builder.alloc(
                    Expr::Call {
                        callee: callee_id,
                        args: arg_ids,
                    },
                    ty,
                ))
            }

            HirExpr::FieldAccess { base, field } => {
                let base_id = self.lower_expr(*base, hir_body)?;
                let expr_id = self.builder.alloc(
                    Expr::FieldAccess {
                        base: base_id,
                        field: field.clone(),
                    },
                    ty,
                );
                // Copy resolution info (for method references like arr.length)
                if let Some(resolution) = self.inference.expr_resolutions.get(&hir_id) {
                    self.builder.record_resolution(expr_id, resolution.clone());
                }
                Ok(expr_id)
            }

            HirExpr::Index { base, index } => {
                let base_id = self.lower_expr(*base, hir_body)?;
                let index_id = self.lower_expr(*index, hir_body)?;
                Ok(self.builder.alloc(
                    Expr::Index {
                        base: base_id,
                        index: index_id,
                    },
                    ty,
                ))
            }

            HirExpr::Array { elements } => {
                let mut elem_ids = Vec::with_capacity(elements.len());
                for e in elements {
                    elem_ids.push(self.lower_expr(*e, hir_body)?);
                }
                Ok(self.builder.alloc(Expr::Array { elements: elem_ids }, ty))
            }

            HirExpr::Object {
                type_name,
                fields,
                spreads,
            } => {
                let mut field_ids = Vec::with_capacity(fields.len());
                for (name, expr) in fields {
                    field_ids.push((name.clone(), self.lower_expr(*expr, hir_body)?));
                }
                let mut spread_ids = Vec::with_capacity(spreads.len());
                for spread in spreads {
                    spread_ids.push(SpreadField {
                        expr: self.lower_expr(spread.expr, hir_body)?,
                        position: spread.position,
                    });
                }
                Ok(self.builder.alloc(
                    Expr::Object {
                        type_name: type_name.clone(),
                        fields: field_ids,
                        spreads: spread_ids,
                    },
                    ty,
                ))
            }

            HirExpr::Map { entries } => {
                let mut entry_ids = Vec::with_capacity(entries.len());
                for (key, value) in entries {
                    let key_id = self.lower_expr(*key, hir_body)?;
                    let value_id = self.lower_expr(*value, hir_body)?;
                    entry_ids.push((key_id, value_id));
                }
                Ok(self.builder.alloc(Expr::Map { entries: entry_ids }, ty))
            }

            HirExpr::Block { stmts, tail_expr } => {
                // This is the key transformation!
                // Weave statements together into Let/Seq chains.
                self.weave_block(stmts, *tail_expr, hir_body)
            }

            HirExpr::Match {
                scrutinee, arms, ..
            } => {
                let scrutinee_id = self.lower_expr(*scrutinee, hir_body)?;
                let mut lowered_arms = Vec::with_capacity(arms.len());
                for arm_id in arms {
                    let arm = &hir_body.match_arms[*arm_id];
                    let pattern_id = self.lower_pattern(arm.pattern, hir_body)?;
                    let guard = match arm.guard {
                        Some(g) => Some(self.lower_expr(g, hir_body)?),
                        None => None,
                    };
                    let body = self.lower_expr(arm.body, hir_body)?;
                    lowered_arms.push(MatchArm {
                        pattern: pattern_id,
                        guard,
                        body,
                    });
                }
                // Check if this match was determined to be exhaustive during type checking
                let is_exhaustive = self.inference.exhaustive_matches.contains(&hir_id);
                Ok(self.builder.alloc(
                    Expr::Match {
                        scrutinee: scrutinee_id,
                        arms: lowered_arms,
                        is_exhaustive,
                    },
                    ty,
                ))
            }
        };

        // Record source span from HIR source map onto VIR expression.
        if let Ok(vir_id) = &result {
            if let Some(span) = self.source_map.expr_span(hir_id) {
                self.builder.record_source_span(*vir_id, span);
            }
        }

        result
    }

    /// Weave a block's statements and optional tail expression into Let/Seq chains.
    fn weave_block(
        &mut self,
        stmts: &[HirStmtId],
        tail_expr: Option<HirExprId>,
        hir_body: &HirExprBody,
    ) -> Result<ExprId, LoweringError> {
        // Collect translated expressions for each statement
        let mut translated: Vec<ExprId> = Vec::with_capacity(stmts.len() + 1);

        for &stmt_id in stmts {
            let expr_id = self.translate_stmt(stmt_id, hir_body)?;
            translated.push(expr_id);
        }

        // Add tail expression or unit
        let tail = if let Some(tail_hir) = tail_expr {
            self.lower_expr(tail_hir, hir_body)?
        } else {
            self.builder.alloc_unit()
        };
        translated.push(tail);

        // Weave together right-to-left
        Ok(self.weave_expressions(translated))
    }

    /// Weave a list of expressions together, threading Let scopes and Seq nodes.
    fn weave_expressions(&mut self, mut exprs: Vec<ExprId>) -> ExprId {
        if exprs.is_empty() {
            return self.builder.alloc_unit();
        }

        if exprs.len() == 1 {
            let single = exprs[0];
            // Fix any dangling Let
            self.fix_dangling_let(single);
            return single;
        }

        // Start from the rightmost expression
        let mut result = exprs.pop().unwrap();

        // Thread from right to left
        while let Some(curr) = exprs.pop() {
            result = self.thread_expression(curr, result);
        }

        result
    }

    /// Thread current expression with the accumulated result.
    fn thread_expression(&mut self, curr: ExprId, result: ExprId) -> ExprId {
        // Check if this is a dangling Let
        if self.is_dangling_let(curr) {
            // Fill in the dangling scope with result
            self.fill_let_body(curr, result);
            // Update the Let's type to match the body's type
            let result_ty = self.builder.ty(result);
            self.builder.expr_types.insert(curr, result_ty);
            return curr;
        }

        // Not a dangling Let - wrap with Seq
        let result_ty = self.builder.ty(result);
        self.builder.alloc(
            Expr::Seq {
                first: curr,
                second: result,
            },
            result_ty,
        )
    }

    /// Check if an expression is a dangling Let.
    fn is_dangling_let(&self, expr_id: ExprId) -> bool {
        if let Expr::Let { body, .. } = &self.builder.exprs[expr_id] {
            is_dangling(*body)
        } else {
            false
        }
    }

    /// Fix a dangling Let by setting its body to Unit.
    fn fix_dangling_let(&mut self, expr_id: ExprId) {
        if self.is_dangling_let(expr_id) {
            let unit = self.builder.alloc_unit();
            self.fill_let_body(expr_id, unit);
            self.builder.expr_types.insert(
                expr_id,
                Ty::Void {
                    attr: baml_type::TyAttr::default(),
                },
            );
        }
    }

    /// Fill in the body of a Let expression.
    fn fill_let_body(&mut self, let_id: ExprId, new_body: ExprId) {
        if let Expr::Let { body, .. } = &mut self.builder.exprs[let_id] {
            *body = new_body;
        }
    }

    /// Translate a statement to a VIR expression.
    fn translate_stmt(
        &mut self,
        stmt_id: HirStmtId,
        hir_body: &HirExprBody,
    ) -> Result<ExprId, LoweringError> {
        use baml_compiler_hir::Stmt as HirStmt;

        let stmt = &hir_body.stmts[stmt_id];

        let result = match stmt {
            HirStmt::Missing => {
                let span = self.source_map.stmt_span(stmt_id).unwrap_or_default();
                Err(LoweringError::MissingStatement { span })
            }

            HirStmt::Let {
                pattern,
                type_annotation,
                initializer,
                is_watched,
                ..
            } => {
                // Create a Let with dangling body
                let pat_id = self.lower_pattern(*pattern, hir_body)?;

                // Get the type from annotation or initializer
                let ty = if let Some(type_id) = type_annotation {
                    let type_ref = &hir_body.types[*type_id];
                    self.lower_type_ref(type_ref)
                } else if let Some(init) = initializer {
                    self.inference
                        .expr_types
                        .get(init)
                        .map(|ty| self.lower_ty(ty))
                        .unwrap_or(Ty::Null {
                            attr: baml_type::TyAttr::default(),
                        })
                } else {
                    Ty::Null {
                        attr: baml_type::TyAttr::default(),
                    }
                };

                // Lower the initializer (or unit if missing)
                let value = if let Some(init) = initializer {
                    self.lower_expr(*init, hir_body)?
                } else {
                    self.builder.alloc_unit()
                };

                // Create dangling Let - body will be filled in during weaving
                let dangling_body = dangling_expr_id();

                Ok(self.builder.alloc(
                    Expr::Let {
                        pattern: pat_id,
                        ty,
                        value,
                        body: dangling_body,
                        is_watched: *is_watched,
                    },
                    Ty::Null {
                        attr: baml_type::TyAttr::default(),
                    }, // Will be updated when body is filled
                ))
            }

            HirStmt::Expr(expr_id) => self.lower_expr(*expr_id, hir_body),

            HirStmt::While {
                condition,
                body,
                after,
                ..
            } => {
                let cond = self.lower_expr(*condition, hir_body)?;
                let while_body = self.lower_expr(*body, hir_body)?;

                // Handle the after statement (for C-style for loops)
                let final_body = if let Some(after_stmt) = after {
                    let after_expr = self.translate_stmt(*after_stmt, hir_body)?;
                    self.weave_expressions(vec![while_body, after_expr])
                } else {
                    while_body
                };

                Ok(self.builder.alloc(
                    Expr::While {
                        condition: cond,
                        body: final_body,
                    },
                    Ty::Void {
                        attr: baml_type::TyAttr::default(),
                    },
                ))
            }

            HirStmt::Return(expr) => {
                let ret_expr = match expr {
                    Some(e) => Some(self.lower_expr(*e, hir_body)?),
                    None => None,
                };
                Ok(self.builder.alloc(
                    Expr::Return(ret_expr),
                    Ty::Void {
                        attr: baml_type::TyAttr::default(),
                    },
                ))
            }

            HirStmt::Break => Ok(self.builder.alloc(
                Expr::Break,
                Ty::Void {
                    attr: baml_type::TyAttr::default(),
                },
            )),

            HirStmt::Continue => Ok(self.builder.alloc(
                Expr::Continue,
                Ty::Void {
                    attr: baml_type::TyAttr::default(),
                },
            )),

            HirStmt::Assign { target, value } => {
                let target_id = self.lower_expr(*target, hir_body)?;
                let value_id = self.lower_expr(*value, hir_body)?;
                Ok(self.builder.alloc(
                    Expr::Assign {
                        target: target_id,
                        value: value_id,
                    },
                    Ty::Void {
                        attr: baml_type::TyAttr::default(),
                    },
                ))
            }

            HirStmt::AssignOp { target, op, value } => {
                let target_id = self.lower_expr(*target, hir_body)?;
                let value_id = self.lower_expr(*value, hir_body)?;
                Ok(self.builder.alloc(
                    Expr::AssignOp {
                        target: target_id,
                        op: AssignOp::from(*op),
                        value: value_id,
                    },
                    Ty::Void {
                        attr: baml_type::TyAttr::default(),
                    },
                ))
            }

            HirStmt::Assert { condition } => {
                let condition_id = self.lower_expr(*condition, hir_body)?;
                Ok(self.builder.alloc(
                    Expr::Assert {
                        condition: condition_id,
                    },
                    Ty::Void {
                        attr: baml_type::TyAttr::default(),
                    },
                ))
            }

            HirStmt::HeaderComment { name, level } => Ok(self.builder.alloc(
                Expr::NotifyBlock {
                    name: name.clone(),
                    level: *level,
                },
                Ty::Void {
                    attr: baml_type::TyAttr::default(),
                },
            )),
        };

        // Record source span from HIR source map onto VIR expression.
        if let Ok(vir_id) = &result {
            if let Some(span) = self.source_map.stmt_span(stmt_id) {
                self.builder.record_source_span(*vir_id, span);
            }
        }

        result
    }

    /// Lower a TIR type to the unified `baml_type::Ty`.
    ///
    /// Uses the shared conversion from `baml_type::convert_tir_ty` which handles:
    /// - FQN → `TypeName` conversion
    /// - Non-recursive type alias expansion
    /// - Literal type preservation (no erasure)
    /// - TIR Unknown/Error → Null (error recovery types don't propagate)
    fn lower_ty(&self, thir_ty: &baml_compiler_tir::Ty) -> Ty {
        baml_type::convert_tir_ty(thir_ty, self.type_aliases, self.recursive_aliases).unwrap_or(
            Ty::Null {
                attr: baml_type::TyAttr::default(),
            },
        )
    }

    /// Lower an HIR `TypeRef` to VIR type.
    fn lower_type_ref(&self, type_ref: &baml_compiler_hir::TypeRef) -> Ty {
        let (thir_ty, _) = self
            .resolution_ctx
            .lower_type_ref(type_ref, Span::default());
        self.lower_ty(&thir_ty)
    }

    /// Lower an HIR pattern to VIR pattern.
    fn lower_pattern(
        &mut self,
        pat_id: baml_compiler_hir::PatId,
        hir_body: &HirExprBody,
    ) -> Result<PatId, LoweringError> {
        let hir_pat = &hir_body.patterns[pat_id];
        let pat = match hir_pat {
            baml_compiler_hir::Pattern::Binding(name) => Pattern::Binding(name.clone()),
            baml_compiler_hir::Pattern::TypedBinding { name, ty } => Pattern::TypedBinding {
                name: name.clone(),
                ty: self.lower_type_ref(ty),
            },
            baml_compiler_hir::Pattern::Literal(lit) => Pattern::Literal(Literal::from(lit)),
            baml_compiler_hir::Pattern::EnumVariant { enum_name, variant } => {
                Pattern::EnumVariant {
                    enum_name: enum_name.clone(),
                    variant: variant.clone(),
                }
            }
            baml_compiler_hir::Pattern::Union(pats) => {
                let mut lowered_pats = Vec::with_capacity(pats.len());
                for &p in pats {
                    lowered_pats.push(self.lower_pattern(p, hir_body)?);
                }
                Pattern::Union(lowered_pats)
            }
        };
        Ok(self.builder.alloc_pattern(pat))
    }
}

// ============================================================================
// Dangling scope helpers
// ============================================================================

/// Create an `ExprId` that represents a dangling (unfilled) scope.
fn dangling_expr_id() -> ExprId {
    ExprId::from_raw(la_arena::RawIdx::from_u32(DANGLING_SCOPE))
}

/// Check if an `ExprId` is dangling.
fn is_dangling(id: ExprId) -> bool {
    id.into_raw().into_u32() == DANGLING_SCOPE
}
