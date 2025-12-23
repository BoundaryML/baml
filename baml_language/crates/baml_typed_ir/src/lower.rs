//! Lowering from HIR + THIR type information to `TypedIR`.
//!
//! This module converts the HIR expression tree (with THIR type annotations)
//! into our unified expression-based IR where everything is an expression.
//!
//! # Fallible Lowering
//!
//! Lowering is fallible - it returns `Result<ExprBody, LoweringError>`.
//! Any `Missing` nodes in the HIR will cause lowering to fail. This is
//! intentional: `TypedIR` represents only valid, complete programs suitable
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

use baml_hir::{ExprBody as HirExprBody, ExprId as HirExprId, FunctionBody, StmtId as HirStmtId};
use baml_thir::InferenceResult;
use la_arena::Arena;
use rustc_hash::FxHashMap;
use text_size::TextRange;

use crate::{AssignOp, BinaryOp, Expr, ExprBody, ExprId, Literal, PatId, Pattern, Ty, UnaryOp};

/// Error that occurs when lowering HIR to `TypedIR`.
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

/// Lower a function body from HIR to `TypedIR`.
///
/// Returns `Err` if the HIR contains any `Missing` nodes or is otherwise
/// not suitable for code generation.
///
/// Note: Takes `baml_thir::Db` instead of `baml_typed_ir::Db` for broader compatibility.
/// This allows callers with `baml_mir::Db` to use this function directly.
pub fn lower_from_hir<'db>(
    db: &'db dyn baml_thir::Db,
    body: &FunctionBody,
    inference: &InferenceResult<'db>,
) -> Result<ExprBody, LoweringError> {
    match body {
        FunctionBody::Expr(hir_body) => {
            let ctx = LoweringContext::new(db, inference);
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
}

impl ExprBodyBuilder {
    fn new() -> Self {
        Self {
            exprs: Arena::new(),
            patterns: Arena::new(),
            expr_types: FxHashMap::default(),
            expr_spans: FxHashMap::default(),
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
            root,
        }
    }
}

/// Context for lowering HIR to `TypedIR`.
struct LoweringContext<'db> {
    db: &'db dyn baml_thir::Db,
    inference: &'db InferenceResult<'db>,
    builder: ExprBodyBuilder,
}

impl<'db> LoweringContext<'db> {
    fn new(db: &'db dyn baml_thir::Db, inference: &'db InferenceResult<'db>) -> Self {
        Self {
            db,
            inference,
            builder: ExprBodyBuilder::new(),
        }
    }

    /// Lower a complete HIR expression body.
    fn lower_expr_body(mut self, hir_body: &HirExprBody) -> Result<ExprBody, LoweringError> {
        let root_hir = hir_body.root_expr.ok_or(LoweringError::NoRootExpression)?;
        let root = self.lower_expr(root_hir, hir_body)?;
        Ok(self.builder.finish(root))
    }

    /// Lower an HIR expression to `TypedIR`.
    fn lower_expr(
        &mut self,
        hir_id: HirExprId,
        hir_body: &HirExprBody,
    ) -> Result<ExprId, LoweringError> {
        use baml_hir::Expr as HirExpr;

        let hir_expr = &hir_body.exprs[hir_id];
        let span = hir_body.expr_span(hir_id);

        // Get type from THIR inference
        let ty = self
            .inference
            .expr_types
            .get(&hir_id)
            .map(|t| self.lower_ty(t))
            .unwrap_or(Ty::Unknown);

        match hir_expr {
            HirExpr::Missing => Err(LoweringError::MissingExpression { span }),

            HirExpr::Literal(lit) => {
                Ok(self
                    .builder
                    .alloc(Expr::Literal(Literal::from(lit)), ty, span))
            }

            HirExpr::Path(segments) => {
                if segments.len() == 1 {
                    Ok(self.builder.alloc(Expr::Var(segments[0].clone()), ty, span))
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
                            .alloc(Expr::Var(segments[0].clone()), first_ty, span);

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
                            span,
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
                    Ok(self.builder.alloc(Expr::Path(segments.clone()), ty, span))
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
                    span,
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
                    span,
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
                    span,
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
                    span,
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
                    span,
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
                    span,
                ))
            }

            HirExpr::Array { elements } => {
                let mut elem_ids = Vec::with_capacity(elements.len());
                for e in elements {
                    elem_ids.push(self.lower_expr(*e, hir_body)?);
                }
                Ok(self
                    .builder
                    .alloc(Expr::Array { elements: elem_ids }, ty, span))
            }

            HirExpr::Object { type_name, fields } => {
                let mut field_ids = Vec::with_capacity(fields.len());
                for (name, expr) in fields {
                    field_ids.push((name.clone(), self.lower_expr(*expr, hir_body)?));
                }
                Ok(self.builder.alloc(
                    Expr::Object {
                        type_name: type_name.clone(),
                        fields: field_ids,
                    },
                    ty,
                    span,
                ))
            }

            HirExpr::Block { stmts, tail_expr } => {
                // This is the key transformation!
                // Weave statements together into Let/Seq chains.
                self.weave_block(stmts, *tail_expr, hir_body)
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

    /// Translate a statement to a `TypedIR` expression.
    fn translate_stmt(
        &mut self,
        stmt_id: HirStmtId,
        hir_body: &HirExprBody,
    ) -> Result<ExprId, LoweringError> {
        use baml_hir::Stmt as HirStmt;

        let stmt = &hir_body.stmts[stmt_id];
        let span = hir_body.stmt_span(stmt_id);

        match stmt {
            HirStmt::Missing => Err(LoweringError::MissingStatement { span }),

            HirStmt::Let {
                pattern,
                type_annotation,
                initializer,
                ..
            } => {
                // Create a Let with dangling body
                let hir_pat = &hir_body.patterns[*pattern];
                let pat = match hir_pat {
                    baml_hir::Pattern::Binding(name) => Pattern::Binding(name.clone()),
                };
                let pat_id = self.builder.alloc_pattern(pat);

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
                    },
                    Ty::Unknown, // Will be updated when body is filled
                    span,
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
                    span,
                ))
            }

            HirStmt::Return(expr) => {
                let ret_expr = match expr {
                    Some(e) => Some(self.lower_expr(*e, hir_body)?),
                    None => None,
                };
                Ok(self.builder.alloc(Expr::Return(ret_expr), Ty::Never, span))
            }

            HirStmt::Break => Ok(self.builder.alloc(Expr::Break, Ty::Never, span)),

            HirStmt::Continue => Ok(self.builder.alloc(Expr::Continue, Ty::Never, span)),

            HirStmt::Assign { target, value } => {
                let target_id = self.lower_expr(*target, hir_body)?;
                let value_id = self.lower_expr(*value, hir_body)?;
                Ok(self.builder.alloc(
                    Expr::Assign {
                        target: target_id,
                        value: value_id,
                    },
                    Ty::Unit,
                    span,
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
                    span,
                ))
            }
        }
    }

    /// Lower a THIR type to `TypedIR` type, resolving all IDs.
    fn lower_ty(&self, thir_ty: &baml_thir::Ty<'db>) -> Ty {
        match thir_ty {
            baml_thir::Ty::Int => Ty::Int,
            baml_thir::Ty::Float => Ty::Float,
            baml_thir::Ty::String => Ty::String,
            baml_thir::Ty::Bool => Ty::Bool,
            baml_thir::Ty::Null => Ty::Null,
            baml_thir::Ty::Image => Ty::Image,
            baml_thir::Ty::Audio => Ty::Audio,
            baml_thir::Ty::Video => Ty::Video,
            baml_thir::Ty::Pdf => Ty::Pdf,

            baml_thir::Ty::Named(name) => Ty::Class(name.clone()),

            baml_thir::Ty::Class(class_id) => {
                let file = class_id.file(self.db);
                let item_tree = baml_hir::file_item_tree(self.db, file);
                let class_data = &item_tree[class_id.id(self.db)];
                Ty::Class(class_data.name.clone())
            }

            baml_thir::Ty::Enum(enum_id) => {
                let file = enum_id.file(self.db);
                let item_tree = baml_hir::file_item_tree(self.db, file);
                let enum_data = &item_tree[enum_id.id(self.db)];
                Ty::Enum(enum_data.name.clone())
            }

            baml_thir::Ty::Optional(inner) => Ty::Optional(Box::new(self.lower_ty(inner))),

            baml_thir::Ty::List(inner) => Ty::List(Box::new(self.lower_ty(inner))),

            baml_thir::Ty::Map { key, value } => Ty::Map {
                key: Box::new(self.lower_ty(key)),
                value: Box::new(self.lower_ty(value)),
            },

            baml_thir::Ty::Union(types) => {
                Ty::Union(types.iter().map(|t| self.lower_ty(t)).collect())
            }

            baml_thir::Ty::Function { params, ret } => Ty::Function {
                params: params.iter().map(|t| self.lower_ty(t)).collect(),
                ret: Box::new(self.lower_ty(ret)),
            },

            baml_thir::Ty::Unknown => Ty::Unknown,
            baml_thir::Ty::Error => Ty::Error,
            baml_thir::Ty::Void => Ty::Unit,
        }
    }

    /// Lower an HIR `TypeRef` to `TypedIR` type.
    fn lower_type_ref(&self, type_ref: &baml_hir::TypeRef) -> Ty {
        let thir_ty = baml_thir::lower_type_ref(self.db, type_ref);
        self.lower_ty(&thir_ty)
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
