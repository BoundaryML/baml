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
use baml_hir::{ExprBody as HirExprBody, ExprId as HirExprId, FunctionBody, StmtId as HirStmtId};
use baml_tir::{InferenceResult, TypeResolutionContext};
use la_arena::Arena;
use rustc_hash::FxHashMap;
use text_size::TextRange;

use crate::{
    AssignOp, BinaryOp, Expr, ExprBody, ExprId, Literal, MatchArm, PatId, Pattern, SpreadField, Ty,
    UnaryOp,
};

/// Error that occurs when lowering HIR to VIR.
#[derive(Debug, Clone)]
pub enum LoweringError {
    /// Encountered a Missing expression node.
    MissingExpression { span: Option<TextRange> },
    /// Encountered a Missing statement node.
    MissingStatement { span: Option<TextRange> },
    /// Function body is missing.
    MissingBody,
    /// No root expression in the body.
    NoRootExpression,
}

impl std::fmt::Display for LoweringError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoweringError::MissingExpression { span } => {
                write!(f, "missing expression")?;
                if let Some(s) = span {
                    write!(f, " at {s:?}")?;
                }
                Ok(())
            }
            LoweringError::MissingStatement { span } => {
                write!(f, "missing statement")?;
                if let Some(s) = span {
                    write!(f, " at {s:?}")?;
                }
                Ok(())
            }
            LoweringError::MissingBody => write!(f, "function body is missing"),
            LoweringError::NoRootExpression => write!(f, "no root expression in body"),
        }
    }
}

impl std::error::Error for LoweringError {}

/// Lower a function body from HIR to VIR.
///
/// Returns `Err` if the HIR contains any `Missing` nodes or is otherwise
/// not suitable for code generation.
///
/// Note: Takes `baml_tir::Db` instead of `baml_vir::Db` for broader compatibility.
/// This allows callers with `baml_mir::Db` to use this function directly.
pub fn lower_from_hir<'db>(
    db: &'db dyn baml_tir::Db,
    body: &FunctionBody,
    inference: &InferenceResult<'db>,
    resolution_ctx: &TypeResolutionContext<'db>,
) -> Result<ExprBody, LoweringError> {
    match body {
        FunctionBody::Expr(hir_body) => {
            let ctx = LoweringContext::new(db, inference, resolution_ctx);
            ctx.lower_expr_body(hir_body)
        }
        FunctionBody::Llm(_) => {
            // LLM bodies don't have expression trees - they're handled specially
            Err(LoweringError::MissingBody)
        }
        FunctionBody::Missing => Err(LoweringError::MissingBody),
    }
}

/// Sentinel value for dangling Let scopes (body not yet filled in).
const DANGLING_SCOPE: u32 = u32::MAX;

/// Builder for constructing `ExprBody`.
struct ExprBodyBuilder {
    exprs: Arena<Expr>,
    patterns: Arena<Pattern>,
    expr_types: FxHashMap<ExprId, Ty>,
    expr_spans: FxHashMap<ExprId, TextRange>,
    enum_variant_exprs: FxHashMap<ExprId, (baml_base::Name, baml_base::Name)>,
}

impl ExprBodyBuilder {
    fn new() -> Self {
        Self {
            exprs: Arena::new(),
            patterns: Arena::new(),
            expr_types: FxHashMap::default(),
            expr_spans: FxHashMap::default(),
            enum_variant_exprs: FxHashMap::default(),
        }
    }

    fn alloc(&mut self, expr: Expr, ty: Ty, span: Option<TextRange>) -> ExprId {
        let id = self.exprs.alloc(expr);
        self.expr_types.insert(id, ty);
        if let Some(s) = span {
            self.expr_spans.insert(id, s);
        }
        id
    }

    fn alloc_pattern(&mut self, pattern: Pattern) -> PatId {
        self.patterns.alloc(pattern)
    }

    fn alloc_unit(&mut self) -> ExprId {
        self.alloc(Expr::Unit, Ty::Unit, None)
    }

    fn ty(&self, id: ExprId) -> &Ty {
        self.expr_types.get(&id).unwrap_or(&Ty::Unknown)
    }

    fn finish(self, root: ExprId) -> ExprBody {
        ExprBody {
            exprs: self.exprs,
            patterns: self.patterns,
            expr_types: self.expr_types,
            expr_spans: self.expr_spans,
            enum_variant_exprs: self.enum_variant_exprs,
            root,
        }
    }

    fn record_enum_variant(
        &mut self,
        id: ExprId,
        enum_name: baml_base::Name,
        variant: baml_base::Name,
    ) {
        self.enum_variant_exprs.insert(id, (enum_name, variant));
    }
}

/// Context for lowering HIR to VIR.
struct LoweringContext<'a, 'db> {
    #[allow(dead_code)]
    db: &'db dyn baml_tir::Db,
    inference: &'db InferenceResult<'db>,
    resolution_ctx: &'a TypeResolutionContext<'db>,
    builder: ExprBodyBuilder,
}

impl<'a, 'db> LoweringContext<'a, 'db> {
    fn new(
        db: &'db dyn baml_tir::Db,
        inference: &'db InferenceResult<'db>,
        resolution_ctx: &'a TypeResolutionContext<'db>,
    ) -> Self {
        Self {
            db,
            inference,
            resolution_ctx,
            builder: ExprBodyBuilder::new(),
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
        use baml_hir::Expr as HirExpr;

        let hir_expr = &hir_body.exprs[hir_id];
        let span = hir_body.get_expr_span(hir_id);

        // Get type from TIR inference
        let ty = self
            .inference
            .expr_types
            .get(&hir_id)
            .map(|t| self.lower_ty(t))
            .unwrap_or(Ty::Unknown);

        match hir_expr {
            HirExpr::Missing => Err(LoweringError::MissingExpression {
                span: span.map(|s| s.range),
            }),

            HirExpr::Literal(lit) => {
                Ok(self
                    .builder
                    .alloc(Expr::Literal(Literal::from(lit)), ty, span.map(|s| s.range)))
            }

            HirExpr::Path(segments) => {
                let text_range = span.map(|s| s.range);
                if segments.len() == 1 {
                    Ok(self
                        .builder
                        .alloc(Expr::Var(segments[0].clone()), ty, text_range))
                } else if let Some(segment_types) = self.inference.path_segment_types.get(&hir_id) {
                    // Local variable with field accesses (e.g., obj.field.subfield)
                    // Convert to nested FieldAccess for proper type tracking.
                    // segment_types[0] = type of first segment (variable)
                    // segment_types[i] = type after i-th field access

                    // Start with the variable (first segment)
                    let first_ty = segment_types
                        .first()
                        .map(|t| self.lower_ty(t))
                        .unwrap_or_else(|| {
                            panic!("BUG: path_segment_types is empty for path {segments:?}")
                        });
                    let mut current =
                        self.builder
                            .alloc(Expr::Var(segments[0].clone()), first_ty, text_range);

                    // Build nested FieldAccess for remaining segments
                    for (i, field) in segments[1..].iter().enumerate() {
                        // Type after this field access is segment_types[i+1]
                        let result_ty = segment_types
                            .get(i + 1)
                            .map(|t| self.lower_ty(t))
                            .unwrap_or_else(|| {
                                panic!(
                                    "BUG: path_segment_types missing type at index {} for path {:?}",
                                    i + 1, segments
                                )
                            });
                        current = self.builder.alloc(
                            Expr::FieldAccess {
                                base: current,
                                field: field.clone(),
                            },
                            result_ty,
                            text_range,
                        );
                    }

                    Ok(current)
                } else {
                    // Non-local path (e.g., builtin function like baml.Array.length, enum variant)
                    // Keep as Expr::Path - will be resolved during MIR lowering.
                    //
                    // TODO: The type here may be incorrect for generic builtins like baml.Array.length
                    // which should have type `fn(Array<T>) -> int` but generics are currently hacked
                    // and not properly implemented. When real generics are added, this will need
                    // proper type instantiation.
                    let expr_id = self
                        .builder
                        .alloc(Expr::Path(segments.clone()), ty, text_range);

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
                    span.map(|s| s.range),
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
                    span.map(|s| s.range),
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
                    span.map(|s| s.range),
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
                    span.map(|s| s.range),
                ))
            }

            HirExpr::FieldAccess { base, field } => {
                let base_id = self.lower_expr(*base, hir_body)?;
                Ok(self.builder.alloc(
                    Expr::FieldAccess {
                        base: base_id,
                        field: field.clone(),
                    },
                    ty,
                    span.map(|s| s.range),
                ))
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
                    span.map(|s| s.range),
                ))
            }

            HirExpr::Array { elements } => {
                let mut elem_ids = Vec::with_capacity(elements.len());
                for e in elements {
                    elem_ids.push(self.lower_expr(*e, hir_body)?);
                }
                Ok(self.builder.alloc(
                    Expr::Array { elements: elem_ids },
                    ty,
                    span.map(|s| s.range),
                ))
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
                    span.map(|s| s.range),
                ))
            }

            HirExpr::Map { entries } => {
                let mut entry_ids = Vec::with_capacity(entries.len());
                for (key, value) in entries {
                    let key_id = self.lower_expr(*key, hir_body)?;
                    let value_id = self.lower_expr(*value, hir_body)?;
                    entry_ids.push((key_id, value_id));
                }
                Ok(self
                    .builder
                    .alloc(Expr::Map { entries: entry_ids }, ty, span.map(|s| s.range)))
            }

            HirExpr::Block { stmts, tail_expr } => {
                // This is the key transformation!
                // Weave statements together into Let/Seq chains.
                self.weave_block(stmts, *tail_expr, hir_body)
            }

            HirExpr::Match { scrutinee, arms } => {
                let scrutinee_id = self.lower_expr(*scrutinee, hir_body)?;
                let mut lowered_arms = Vec::with_capacity(arms.len());
                for arm in arms {
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
                    span.map(|s| s.range),
                ))
            }
        }
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
            let result_ty = self.builder.ty(result).clone();
            self.builder.expr_types.insert(curr, result_ty);
            return curr;
        }

        // Not a dangling Let - wrap with Seq
        let result_ty = self.builder.ty(result).clone();
        self.builder.alloc(
            Expr::Seq {
                first: curr,
                second: result,
            },
            result_ty,
            None,
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
            self.builder.expr_types.insert(expr_id, Ty::Unit);
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
        use baml_hir::Stmt as HirStmt;

        let stmt = &hir_body.stmts[stmt_id];
        let span = hir_body.get_stmt_span(stmt_id);
        let text_range = span.map(|s| s.range);

        match stmt {
            HirStmt::Missing => Err(LoweringError::MissingStatement { span: text_range }),

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
                let ty = if let Some(annot) = type_annotation {
                    self.lower_type_ref(annot)
                } else if let Some(init) = initializer {
                    self.inference
                        .expr_types
                        .get(init)
                        .map(|t| self.lower_ty(t))
                        .unwrap_or(Ty::Unknown)
                } else {
                    Ty::Unknown
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
                    Ty::Unknown, // Will be updated when body is filled
                    text_range,
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
                    Ty::Unit,
                    text_range,
                ))
            }

            HirStmt::Return(expr) => {
                let ret_expr = match expr {
                    Some(e) => Some(self.lower_expr(*e, hir_body)?),
                    None => None,
                };
                Ok(self
                    .builder
                    .alloc(Expr::Return(ret_expr), Ty::Never, text_range))
            }

            HirStmt::Break => Ok(self.builder.alloc(Expr::Break, Ty::Never, text_range)),

            HirStmt::Continue => Ok(self.builder.alloc(Expr::Continue, Ty::Never, text_range)),

            HirStmt::Assign { target, value } => {
                let target_id = self.lower_expr(*target, hir_body)?;
                let value_id = self.lower_expr(*value, hir_body)?;
                Ok(self.builder.alloc(
                    Expr::Assign {
                        target: target_id,
                        value: value_id,
                    },
                    Ty::Unit,
                    text_range,
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
                    Ty::Unit,
                    text_range,
                ))
            }

            HirStmt::Assert { condition } => {
                let condition_id = self.lower_expr(*condition, hir_body)?;
                Ok(self.builder.alloc(
                    Expr::Assert {
                        condition: condition_id,
                    },
                    Ty::Unit,
                    text_range,
                ))
            }

            HirStmt::HeaderComment { name, level } => Ok(self.builder.alloc(
                Expr::NotifyBlock {
                    name: name.clone(),
                    level: *level,
                },
                Ty::Unit,
                text_range,
            )),
        }
    }

    /// Lower a TIR type to VIR type, resolving all IDs.
    fn lower_ty(&self, thir_ty: &baml_tir::Ty<'db>) -> Ty {
        match thir_ty {
            baml_tir::Ty::Int => Ty::Int,
            baml_tir::Ty::Float => Ty::Float,
            baml_tir::Ty::String => Ty::String,
            baml_tir::Ty::Bool => Ty::Bool,
            baml_tir::Ty::Null => Ty::Null,
            baml_tir::Ty::Image => Ty::Image,
            baml_tir::Ty::Audio => Ty::Audio,
            baml_tir::Ty::Video => Ty::Video,
            baml_tir::Ty::Pdf => Ty::Pdf,

            baml_tir::Ty::Named(name) => Ty::Class(name.clone()),

            baml_tir::Ty::Class(class_id, _) => {
                let file = class_id.file(self.db);
                let item_tree = baml_hir::file_item_tree(self.db, file);
                let class_data = &item_tree[class_id.id(self.db)];
                Ty::Class(class_data.name.clone())
            }

            baml_tir::Ty::Enum(enum_id, _) => {
                let file = enum_id.file(self.db);
                let item_tree = baml_hir::file_item_tree(self.db, file);
                let enum_data = &item_tree[enum_id.id(self.db)];
                Ty::Enum(enum_data.name.clone())
            }

            baml_tir::Ty::Optional(inner) => Ty::Optional(Box::new(self.lower_ty(inner))),

            baml_tir::Ty::List(inner) => Ty::List(Box::new(self.lower_ty(inner))),

            baml_tir::Ty::Map { key, value } => Ty::Map {
                key: Box::new(self.lower_ty(key)),
                value: Box::new(self.lower_ty(value)),
            },

            baml_tir::Ty::Union(types) => {
                Ty::Union(types.iter().map(|t| self.lower_ty(t)).collect())
            }

            baml_tir::Ty::Function { params, ret } => Ty::Function {
                params: params.iter().map(|t| self.lower_ty(t)).collect(),
                ret: Box::new(self.lower_ty(ret)),
            },

            baml_tir::Ty::Unknown => Ty::Unknown,
            baml_tir::Ty::Error => Ty::Error,
            baml_tir::Ty::Void => Ty::Unit,
            // Map literal types to their underlying primitive types
            baml_tir::Ty::Literal(lit) => match lit {
                baml_tir::LiteralValue::Int(_) => Ty::Int,
                baml_tir::LiteralValue::Float(_) => Ty::Float,
                baml_tir::LiteralValue::String(_) => Ty::String,
                baml_tir::LiteralValue::Bool(_) => Ty::Bool,
            },
            // WatchAccessor is a special type that wraps another type
            baml_tir::Ty::WatchAccessor(inner) => Ty::WatchAccessor(Box::new(self.lower_ty(inner))),
        }
    }

    /// Lower an HIR `TypeRef` to VIR type.
    fn lower_type_ref(&self, type_ref: &baml_hir::TypeRef) -> Ty {
        let (thir_ty, _) = self
            .resolution_ctx
            .lower_type_ref(type_ref, Span::default());
        self.lower_ty(&thir_ty)
    }

    /// Lower an HIR pattern to VIR pattern.
    fn lower_pattern(
        &mut self,
        pat_id: baml_hir::PatId,
        hir_body: &HirExprBody,
    ) -> Result<PatId, LoweringError> {
        let hir_pat = &hir_body.patterns[pat_id];
        let pat = match hir_pat {
            baml_hir::Pattern::Binding(name) => Pattern::Binding(name.clone()),
            baml_hir::Pattern::TypedBinding { name, ty } => Pattern::TypedBinding {
                name: name.clone(),
                ty: self.lower_type_ref(ty),
            },
            baml_hir::Pattern::Literal(lit) => Pattern::Literal(Literal::from(lit)),
            baml_hir::Pattern::EnumVariant { enum_name, variant } => Pattern::EnumVariant {
                enum_name: enum_name.clone(),
                variant: variant.clone(),
            },
            baml_hir::Pattern::Union(pats) => {
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
