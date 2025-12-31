//! Lowering from `TypedIR` to MIR.
//!
//! This module converts the expression-only `TypedIR` representation into
//! the CFG-based MIR. Because `TypedIR` has no statements (everything is an
//! expression), this lowering is much simpler than HIR → MIR lowering.
//!
//! # Key Simplifications
//!
//! Compared to `lower.rs` (HIR → MIR):
//! - Single `lower_expr` method (no `lower_stmt` or `lower_expr_for_effect`)
//! - `Let { value, body }` handles scoping naturally
//! - `Seq { first, second }` handles sequencing
//! - Types are embedded in the IR (no `HashMap` lookup)
//! - No `Block { stmts, tail_expr }` or tail expression handling
//! - No `Missing` nodes to handle

use std::collections::HashMap;

use baml_base::Name;
use baml_hir::FunctionSignature;
use baml_thir::Ty;
use baml_typed_ir::{AssignOp, BinaryOp, Expr, ExprBody, ExprId, Literal, PatId, Pattern, UnaryOp};

use crate::{
    AggregateKind, BinOp, BlockId, Constant, Local, MirBuilder, MirFunction, Operand, Place,
    Rvalue, UnaryOp as MirUnaryOp,
};

/// Lower a function from `TypedIR` to MIR.
///
/// This is the main entry point for `TypedIR` → MIR lowering.
pub fn lower_from_typed_ir<'db>(
    signature: &FunctionSignature,
    typed_body: &ExprBody,
    db: &'db dyn crate::Db,
    class_fields: &HashMap<String, HashMap<String, usize>>,
) -> MirFunction<'db> {
    let mut ctx = LoweringContext::new(db, signature.params.len(), class_fields);
    ctx.lower_function(signature, typed_body);
    ctx.finish()
}

/// Context for lowering `TypedIR` to MIR.
struct LoweringContext<'db, 'ctx> {
    #[allow(dead_code)]
    db: &'db dyn crate::Db,
    builder: MirBuilder<'db>,
    /// Map from variable names to MIR locals.
    locals: HashMap<Name, Local>,
    /// Current loop context for break/continue.
    loop_context: Option<LoopContext>,
    /// Class field mappings (class name -> field name -> field index).
    class_fields: &'ctx HashMap<String, HashMap<String, usize>>,
}

/// Context for the current loop (for break/continue).
#[derive(Clone)]
struct LoopContext {
    /// Block to jump to on `break`.
    break_target: BlockId,
    /// Block to jump to on `continue`.
    continue_target: BlockId,
}

impl<'db, 'ctx> LoweringContext<'db, 'ctx> {
    fn new(
        db: &'db dyn crate::Db,
        arity: usize,
        class_fields: &'ctx HashMap<String, HashMap<String, usize>>,
    ) -> Self {
        Self {
            db,
            builder: MirBuilder::new("", arity),
            locals: HashMap::new(),
            loop_context: None,
            class_fields,
        }
    }

    fn finish(self) -> MirFunction<'db> {
        self.builder.build()
    }

    /// Lower a complete function.
    fn lower_function(&mut self, signature: &FunctionSignature, body: &ExprBody) {
        self.builder = MirBuilder::new(signature.name.to_string(), signature.params.len());

        // _0: return place
        // Use signature return type, not body root type (which may be Never for diverging bodies)
        let ret_ty = baml_thir::lower_type_ref(self.db, &signature.return_type);
        let ret = self.builder.declare_local(None, ret_ty, None);
        assert_eq!(ret, Local(0));

        // _1..=_n: parameters
        for param in &signature.params {
            let param_ty = baml_thir::lower_type_ref(self.db, &param.type_ref);
            let local = self
                .builder
                .declare_local(Some(param.name.clone()), param_ty, None);
            self.locals.insert(param.name.clone(), local);
        }

        // Create entry and exit blocks
        let entry = self.builder.create_block();
        let exit = self.builder.create_block();

        self.builder.set_current_block(entry);

        // Lower the root expression, storing result in _0
        self.lower_expr(body.root, Place::local(ret), body);

        // If we haven't terminated, goto exit
        if !self.builder.is_current_terminated() {
            self.builder.goto(exit);
        }

        // Exit block just returns
        self.builder.set_current_block(exit);
        self.builder.return_();
    }

    /// Lower an expression, storing the result in `dest`.
    ///
    /// This is the core of `TypedIR` lowering. Unlike HIR lowering which needs
    /// separate `lower_expr_to_place` and `lower_stmt`, here we have just one method.
    fn lower_expr(&mut self, expr_id: ExprId, dest: Place, body: &ExprBody) {
        let expr = body.expr(expr_id);
        let ty = body.ty(expr_id);

        match expr {
            // ========== Literals ==========
            Expr::Literal(lit) => {
                let constant = Self::lower_literal(lit);
                self.builder
                    .assign(dest, Rvalue::Use(Operand::Constant(constant)));
            }

            Expr::Unit => {
                self.builder
                    .assign(dest, Rvalue::Use(Operand::Constant(Constant::Null)));
            }

            // ========== Variables & Paths ==========
            Expr::Var(name) => {
                if let Some(&local) = self.locals.get(name) {
                    self.builder
                        .assign(dest, Rvalue::Use(Operand::copy_local(local)));
                } else {
                    // Function reference
                    self.builder.assign(
                        dest,
                        Rvalue::Use(Operand::Constant(Constant::Function(name.clone()))),
                    );
                }
            }

            Expr::Path(segments) => {
                // Note: Multi-segment paths that are local variable field accesses should have
                // been converted to nested FieldAccess during HIR → TypedIR lowering.
                // TODO: Multi-segment paths that reach here are non-local paths like builtin functions
                // (e.g., baml.Array.length) which need special handling.
                //
                // TODO: This is a workaround for the lack of proper module/namespace support.
                // When we have proper modules, builtin paths should be resolved earlier in the
                // pipeline and represented differently (not as Expr::Path).
                if segments.len() == 1 {
                    // Simple variable reference
                    let name = &segments[0];
                    if let Some(&local) = self.locals.get(name) {
                        self.builder
                            .assign(dest, Rvalue::Use(Operand::copy_local(local)));
                    } else {
                        // Assume it's a function reference
                        self.builder.assign(
                            dest,
                            Rvalue::Use(Operand::Constant(Constant::Function(name.clone()))),
                        );
                    }
                } else {
                    // Multi-segment path that's not a field access chain (e.g., baml.Array.length).
                    // TODO: This is a hack - we're treating these as builtin function references
                    // by joining segments into a dotted path. Proper module resolution should
                    // handle this case earlier in the pipeline.
                    let full_path = segments
                        .iter()
                        .map(smol_str::SmolStr::as_str)
                        .collect::<Vec<_>>()
                        .join(".");
                    self.builder.assign(
                        dest,
                        Rvalue::Use(Operand::Constant(Constant::Function(Name::new(full_path)))),
                    );
                }
            }

            // ========== Binding & Sequencing ==========
            // This is where TypedIR shines - no special tail expression handling!
            Expr::Let {
                pattern,
                ty: var_ty,
                value,
                body: let_body,
            } => {
                // Extract the variable name from the pattern first
                let pat = body.pattern(*pattern);
                let name = match pat {
                    Pattern::Binding(name) => name.clone(),
                    Pattern::TypedBinding { name, .. } => name.clone(),
                    // Literal/EnumVariant/Union patterns don't make sense in let bindings,
                    // but we handle them gracefully
                    Pattern::Literal(_) | Pattern::EnumVariant { .. } | Pattern::Union(_) => {
                        panic!("BUG: non-binding pattern in let statement: {pat:?}")
                    }
                };

                // Lower the value with the actual variable name
                let local_ty = Self::lower_typed_ir_ty(var_ty);
                let local = self
                    .builder
                    .declare_local(Some(name.clone()), local_ty, None);
                self.lower_expr(*value, Place::local(local), body);

                // Bind the variable
                self.locals.insert(name, local);

                // Lower the body - this IS the result
                // No special "tail expression" handling needed!
                self.lower_expr(*let_body, dest, body);
            }

            Expr::Seq { first, second } => {
                // Lower first for effect (result discarded)
                let first_ty = Self::lower_typed_ir_ty(body.ty(*first));
                let temp = self.builder.temp(first_ty);
                self.lower_expr(*first, Place::local(temp), body);

                // If first diverged, we're done
                if self.builder.is_current_terminated() {
                    return;
                }

                // Lower second - this is our result
                self.lower_expr(*second, dest, body);
            }

            // ========== Control Flow ==========
            Expr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.lower_if(*condition, *then_branch, *else_branch, dest, body);
            }

            Expr::While {
                condition,
                body: loop_body,
            } => {
                self.lower_while(*condition, *loop_body, body);
                // While returns Unit
                self.builder
                    .assign(dest, Rvalue::Use(Operand::Constant(Constant::Null)));
            }

            Expr::Return(ret_expr) => {
                if let Some(e) = ret_expr {
                    self.lower_expr(*e, Place::local(Local(0)), body);
                }
                // Create exit and return
                let exit = self.builder.create_block();
                self.builder.goto(exit);
                self.builder.set_current_block(exit);
                self.builder.return_();
            }

            Expr::Break => {
                if let Some(ctx) = &self.loop_context {
                    let target = ctx.break_target;
                    self.builder.goto(target);
                } else {
                    panic!("BUG: `break` outside of loop context");
                }
            }

            Expr::Continue => {
                if let Some(ctx) = &self.loop_context {
                    let target = ctx.continue_target;
                    self.builder.goto(target);
                } else {
                    panic!("BUG: `continue` outside of loop context");
                }
            }

            // ========== Assignment ==========
            Expr::Assign { target, value } => {
                let value_ty = Self::lower_typed_ir_ty(body.ty(*value));
                let value_local = self.builder.temp(value_ty);
                self.lower_expr(*value, Place::local(value_local), body);

                let target_place = self.lower_lvalue(*target, body);
                self.builder
                    .assign(target_place, Rvalue::Use(Operand::copy_local(value_local)));

                // Return unit
                self.builder
                    .assign(dest, Rvalue::Use(Operand::Constant(Constant::Null)));
            }

            Expr::AssignOp { target, op, value } => {
                let target_place = self.lower_lvalue(*target, body);

                // Load current value
                let current_ty = Self::lower_typed_ir_ty(body.ty(*target));
                let current_local = self.builder.temp(current_ty.clone());
                self.builder.assign(
                    Place::local(current_local),
                    Rvalue::Use(Operand::Copy(target_place.clone())),
                );

                // Load rhs
                let rhs_ty = Self::lower_typed_ir_ty(body.ty(*value));
                let rhs_local = self.builder.temp(rhs_ty);
                self.lower_expr(*value, Place::local(rhs_local), body);

                // Compute new value
                let mir_op = Self::convert_assign_op(*op);
                let result_local = self.builder.temp(current_ty);
                self.builder.assign(
                    Place::local(result_local),
                    Rvalue::BinaryOp {
                        op: mir_op,
                        left: Operand::copy_local(current_local),
                        right: Operand::copy_local(rhs_local),
                    },
                );

                // Store back
                self.builder
                    .assign(target_place, Rvalue::Use(Operand::copy_local(result_local)));

                // Return unit
                self.builder
                    .assign(dest, Rvalue::Use(Operand::Constant(Constant::Null)));
            }

            // ========== Operations ==========
            Expr::Binary { op, lhs, rhs } => {
                // Check for short-circuit operators
                match op {
                    BinaryOp::And => {
                        self.lower_short_circuit_and(*lhs, *rhs, dest, body);
                    }
                    BinaryOp::Or => {
                        self.lower_short_circuit_or(*lhs, *rhs, dest, body);
                    }
                    _ => {
                        self.lower_binary_op(*op, *lhs, *rhs, dest, body);
                    }
                }
            }

            Expr::Unary { op, operand } => {
                let operand_ty = Self::lower_typed_ir_ty(body.ty(*operand));
                let operand_local = self.builder.temp(operand_ty);
                self.lower_expr(*operand, Place::local(operand_local), body);

                let mir_op = match op {
                    UnaryOp::Not => MirUnaryOp::Not,
                    UnaryOp::Neg => MirUnaryOp::Neg,
                };

                self.builder.assign(
                    dest,
                    Rvalue::UnaryOp {
                        op: mir_op,
                        operand: Operand::copy_local(operand_local),
                    },
                );
            }

            // ========== Function Calls ==========
            Expr::Call { callee, args } => {
                self.lower_call(*callee, args, dest, body, ty);
            }

            // ========== Data Structures ==========
            Expr::Array { elements } => {
                let elem_operands: Vec<Operand<'db>> = elements
                    .iter()
                    .map(|&elem| {
                        let elem_ty = Self::lower_typed_ir_ty(body.ty(elem));
                        let elem_local = self.builder.temp(elem_ty);
                        self.lower_expr(elem, Place::local(elem_local), body);
                        Operand::copy_local(elem_local)
                    })
                    .collect();

                self.builder.assign(dest, Rvalue::Array(elem_operands));
            }

            Expr::Object { type_name, fields } => {
                let field_operands: Vec<Operand<'db>> = fields
                    .iter()
                    .map(|(_, value)| {
                        let value_ty = Self::lower_typed_ir_ty(body.ty(*value));
                        let value_local = self.builder.temp(value_ty);
                        self.lower_expr(*value, Place::local(value_local), body);
                        Operand::copy_local(value_local)
                    })
                    .collect();

                let kind = if let Some(name) = type_name {
                    AggregateKind::Class(name.to_string())
                } else {
                    AggregateKind::Class("Anonymous".to_string())
                };

                self.builder.assign(
                    dest,
                    Rvalue::Aggregate {
                        kind,
                        fields: field_operands,
                    },
                );
            }

            // ========== Access ==========
            Expr::FieldAccess { base, field } => {
                let result_ty = body.ty(expr_id);

                // Check if this is a method reference (result type is a function)
                // vs an actual field access (result type is the field's type)
                if matches!(result_ty, baml_typed_ir::Ty::Function { .. }) {
                    // Method reference - emit as a function constant
                    // The method name is just the field name (methods are desugared to top-level functions)
                    self.builder.assign(
                        dest,
                        Rvalue::Use(Operand::Constant(Constant::Function(field.clone()))),
                    );
                } else {
                    // Actual field access
                    let base_ty = Self::lower_typed_ir_ty(body.ty(*base));
                    let base_local = self.builder.temp(base_ty.clone());
                    self.lower_expr(*base, Place::local(base_local), body);

                    // Look up field index
                    let field_idx = self.field_index_for_type_and_name(&base_ty, field);

                    self.builder.assign(
                        dest,
                        Rvalue::Use(Operand::Copy(Place::field(
                            Place::local(base_local),
                            field_idx,
                        ))),
                    );
                }
            }

            Expr::Index { base, index } => {
                let base_ty = Self::lower_typed_ir_ty(body.ty(*base));
                let base_local = self.builder.temp(base_ty);
                self.lower_expr(*base, Place::local(base_local), body);

                let index_local = self.builder.temp(Ty::Int);
                self.lower_expr(*index, Place::local(index_local), body);

                self.builder.assign(
                    dest,
                    Rvalue::Use(Operand::Copy(Place::index(
                        Place::local(base_local),
                        index_local,
                    ))),
                );
            }

            Expr::Match { scrutinee, arms } => {
                // Lower scrutinee to a temp
                let scrutinee_ty = Self::lower_typed_ir_ty(body.ty(*scrutinee));
                let scrutinee_local = self.builder.temp(scrutinee_ty.clone());
                self.lower_expr(*scrutinee, Place::local(scrutinee_local), body);

                // Create join block
                let join_block = self.builder.create_block();

                // For each arm, create test and body blocks
                for arm in arms {
                    let arm_block = self.builder.create_block();
                    let next_block = self.builder.create_block();

                    // Generate pattern test
                    self.lower_pattern_test(
                        arm.pattern,
                        scrutinee_local,
                        &scrutinee_ty,
                        arm_block,
                        next_block,
                        body,
                    );

                    // Arm body
                    self.builder.set_current_block(arm_block);
                    if let Some(guard) = arm.guard {
                        // Create a separate block for the guarded body
                        let body_block = self.builder.create_block();

                        // Lower guard expression
                        let guard_local = self.builder.temp(Ty::Bool);
                        self.lower_expr(guard, Place::local(guard_local), body);

                        // Branch: if guard is true go to body_block, else go to next_block
                        // Use branch instead of switch for boolean conditions
                        self.builder.branch(
                            Operand::copy_local(guard_local),
                            body_block,
                            next_block,
                        );

                        // Continue in body_block
                        self.builder.set_current_block(body_block);
                    }
                    self.lower_expr(arm.body, dest.clone(), body);
                    self.builder.goto(join_block);

                    self.builder.set_current_block(next_block);
                }

                // Fallthrough (should be unreachable with exhaustive matching)
                self.builder.goto(join_block);
                self.builder.set_current_block(join_block);
            }
        }
    }

    /// Lower a pattern match test, branching to `success_block` if the pattern matches,
    /// or `fail_block` if it doesn't.
    fn lower_pattern_test(
        &mut self,
        pat_id: PatId,
        scrutinee_local: Local,
        scrutinee_ty: &Ty<'db>,
        success_block: BlockId,
        fail_block: BlockId,
        body: &ExprBody,
    ) {
        let pat = body.pattern(pat_id);
        match pat {
            Pattern::Binding(name) => {
                // Binding always matches - bind the variable and go to success
                let local =
                    self.builder
                        .declare_local(Some(name.clone()), scrutinee_ty.clone(), None);
                self.builder.assign(
                    Place::local(local),
                    Rvalue::Use(Operand::copy_local(scrutinee_local)),
                );
                self.locals.insert(name.clone(), local);
                self.builder.goto(success_block);
            }
            Pattern::TypedBinding { name, ty } => {
                // TypedBinding checks if scrutinee is an instance of the given type
                // Convert TypedIR type to THIR type for IsType check
                let pattern_ty = Self::lower_typed_ir_ty(ty);

                // Emit instanceof check
                let check_local = self.builder.temp(Ty::Bool);
                self.builder.assign(
                    Place::local(check_local),
                    Rvalue::IsType {
                        operand: Operand::copy_local(scrutinee_local),
                        ty: pattern_ty.clone(),
                    },
                );

                // Branch on the check result
                // If type matches, bind the variable and go to success
                // If not, go to fail block
                let bind_block = self.builder.create_block();
                self.builder
                    .branch(Operand::copy_local(check_local), bind_block, fail_block);

                // In bind block: bind the variable and go to success
                self.builder.set_current_block(bind_block);
                let local = self
                    .builder
                    .declare_local(Some(name.clone()), pattern_ty, None);
                self.builder.assign(
                    Place::local(local),
                    Rvalue::Use(Operand::copy_local(scrutinee_local)),
                );
                self.locals.insert(name.clone(), local);
                self.builder.goto(success_block);
            }
            Pattern::Literal(lit) => {
                // Compare scrutinee with literal
                let lit_const = Self::lower_literal(lit);
                let cmp_local = self.builder.temp(Ty::Bool);
                self.builder.assign(
                    Place::local(cmp_local),
                    Rvalue::BinaryOp {
                        op: BinOp::Eq,
                        left: Operand::copy_local(scrutinee_local),
                        right: Operand::Constant(lit_const),
                    },
                );
                // Use branch instead of switch for boolean conditions
                // (Switch compares against Int which doesn't match Bool)
                self.builder
                    .branch(Operand::copy_local(cmp_local), success_block, fail_block);
            }
            Pattern::EnumVariant { enum_name, variant } => {
                // Compare scrutinee (enum value) with the variant
                let variant_const = Constant::EnumVariant {
                    enum_name: enum_name.clone(),
                    variant: variant.clone(),
                };
                let cmp_local = self.builder.temp(Ty::Bool);
                self.builder.assign(
                    Place::local(cmp_local),
                    Rvalue::BinaryOp {
                        op: BinOp::Eq,
                        left: Operand::copy_local(scrutinee_local),
                        right: Operand::Constant(variant_const),
                    },
                );
                // Use branch instead of switch for boolean conditions
                self.builder
                    .branch(Operand::copy_local(cmp_local), success_block, fail_block);
            }
            Pattern::Union(pats) => {
                // Union pattern matches if any sub-pattern matches
                // Try each pattern in order
                for (i, &sub_pat_id) in pats.iter().enumerate() {
                    let next_try = if i + 1 < pats.len() {
                        self.builder.create_block()
                    } else {
                        fail_block
                    };
                    self.lower_pattern_test(
                        sub_pat_id,
                        scrutinee_local,
                        scrutinee_ty,
                        success_block,
                        next_try,
                        body,
                    );
                    if i + 1 < pats.len() {
                        self.builder.set_current_block(next_try);
                    }
                }
            }
        }
    }

    /// Lower a literal to a constant.
    fn lower_literal(lit: &Literal) -> Constant<'db> {
        match lit {
            Literal::Int(n) => Constant::Int(*n),
            Literal::Float(s) => Constant::Float(s.parse().unwrap_or(0.0)),
            Literal::String(s) => Constant::String(s.clone()),
            Literal::Bool(b) => Constant::Bool(*b),
            Literal::Null => Constant::Null,
        }
    }

    /// Lower a binary operation (non-short-circuit).
    fn lower_binary_op(
        &mut self,
        op: BinaryOp,
        lhs: ExprId,
        rhs: ExprId,
        dest: Place,
        body: &ExprBody,
    ) {
        let lhs_ty = Self::lower_typed_ir_ty(body.ty(lhs));
        let rhs_ty = Self::lower_typed_ir_ty(body.ty(rhs));

        let lhs_local = self.builder.temp(lhs_ty);
        self.lower_expr(lhs, Place::local(lhs_local), body);

        let rhs_local = self.builder.temp(rhs_ty);
        self.lower_expr(rhs, Place::local(rhs_local), body);

        let mir_op = Self::convert_binop(op);

        self.builder.assign(
            dest,
            Rvalue::BinaryOp {
                op: mir_op,
                left: Operand::copy_local(lhs_local),
                right: Operand::copy_local(rhs_local),
            },
        );
    }

    fn convert_binop(op: BinaryOp) -> BinOp {
        match op {
            BinaryOp::Add => BinOp::Add,
            BinaryOp::Sub => BinOp::Sub,
            BinaryOp::Mul => BinOp::Mul,
            BinaryOp::Div => BinOp::Div,
            BinaryOp::Mod => BinOp::Mod,
            BinaryOp::Eq => BinOp::Eq,
            BinaryOp::Ne => BinOp::Ne,
            BinaryOp::Lt => BinOp::Lt,
            BinaryOp::Le => BinOp::Le,
            BinaryOp::Gt => BinOp::Gt,
            BinaryOp::Ge => BinOp::Ge,
            BinaryOp::BitAnd => BinOp::BitAnd,
            BinaryOp::BitOr => BinOp::BitOr,
            BinaryOp::BitXor => BinOp::BitXor,
            BinaryOp::Shl => BinOp::Shl,
            BinaryOp::Shr => BinOp::Shr,
            // These are handled separately as short-circuit
            BinaryOp::And => BinOp::BitAnd,
            BinaryOp::Or => BinOp::BitOr,
        }
    }

    fn convert_assign_op(op: AssignOp) -> BinOp {
        match op {
            AssignOp::Add => BinOp::Add,
            AssignOp::Sub => BinOp::Sub,
            AssignOp::Mul => BinOp::Mul,
            AssignOp::Div => BinOp::Div,
            AssignOp::Mod => BinOp::Mod,
            AssignOp::BitAnd => BinOp::BitAnd,
            AssignOp::BitOr => BinOp::BitOr,
            AssignOp::BitXor => BinOp::BitXor,
            AssignOp::Shl => BinOp::Shl,
            AssignOp::Shr => BinOp::Shr,
        }
    }

    /// Lower short-circuit AND: `a && b`
    fn lower_short_circuit_and(&mut self, lhs: ExprId, rhs: ExprId, dest: Place, body: &ExprBody) {
        let lhs_local = self.builder.temp(Ty::Bool);
        self.lower_expr(lhs, Place::local(lhs_local), body);

        let bb_rhs = self.builder.create_block();
        let bb_false = self.builder.create_block();
        let bb_join = self.builder.create_block();

        self.builder
            .branch(Operand::copy_local(lhs_local), bb_rhs, bb_false);

        // bb_rhs: evaluate rhs
        self.builder.set_current_block(bb_rhs);
        self.lower_expr(rhs, dest.clone(), body);
        if !self.builder.is_current_terminated() {
            self.builder.goto(bb_join);
        }

        // bb_false: result is false
        self.builder.set_current_block(bb_false);
        self.builder
            .assign(dest, Rvalue::Use(Operand::Constant(Constant::Bool(false))));
        self.builder.goto(bb_join);

        self.builder.set_current_block(bb_join);
    }

    /// Lower short-circuit OR: `a || b`
    fn lower_short_circuit_or(&mut self, lhs: ExprId, rhs: ExprId, dest: Place, body: &ExprBody) {
        let lhs_local = self.builder.temp(Ty::Bool);
        self.lower_expr(lhs, Place::local(lhs_local), body);

        let bb_true = self.builder.create_block();
        let bb_rhs = self.builder.create_block();
        let bb_join = self.builder.create_block();

        self.builder
            .branch(Operand::copy_local(lhs_local), bb_true, bb_rhs);

        // bb_true: result is true
        self.builder.set_current_block(bb_true);
        self.builder.assign(
            dest.clone(),
            Rvalue::Use(Operand::Constant(Constant::Bool(true))),
        );
        self.builder.goto(bb_join);

        // bb_rhs: evaluate rhs
        self.builder.set_current_block(bb_rhs);
        self.lower_expr(rhs, dest, body);
        if !self.builder.is_current_terminated() {
            self.builder.goto(bb_join);
        }

        self.builder.set_current_block(bb_join);
    }

    /// Lower an if expression.
    fn lower_if(
        &mut self,
        condition: ExprId,
        then_branch: ExprId,
        else_branch: Option<ExprId>,
        dest: Place,
        body: &ExprBody,
    ) {
        let cond_local = self.builder.temp(Ty::Bool);
        self.lower_expr(condition, Place::local(cond_local), body);

        let bb_then = self.builder.create_block();
        let bb_else = self.builder.create_block();
        let bb_join = self.builder.create_block();

        self.builder
            .branch(Operand::copy_local(cond_local), bb_then, bb_else);

        // Then branch
        self.builder.set_current_block(bb_then);
        self.lower_expr(then_branch, dest.clone(), body);
        if !self.builder.is_current_terminated() {
            self.builder.goto(bb_join);
        }

        // Else branch
        self.builder.set_current_block(bb_else);
        if let Some(else_expr) = else_branch {
            self.lower_expr(else_expr, dest, body);
        } else {
            self.builder
                .assign(dest, Rvalue::Use(Operand::Constant(Constant::Null)));
        }
        if !self.builder.is_current_terminated() {
            self.builder.goto(bb_join);
        }

        self.builder.set_current_block(bb_join);
    }

    /// Lower a while loop.
    fn lower_while(&mut self, condition: ExprId, loop_body: ExprId, body: &ExprBody) {
        let bb_cond = self.builder.create_block();
        let bb_body = self.builder.create_block();
        let bb_exit = self.builder.create_block();

        self.builder.goto(bb_cond);

        // Condition block
        self.builder.set_current_block(bb_cond);
        let cond_local = self.builder.temp(Ty::Bool);
        self.lower_expr(condition, Place::local(cond_local), body);
        self.builder
            .branch(Operand::copy_local(cond_local), bb_body, bb_exit);

        // Set up loop context
        let old_loop_ctx = self.loop_context.replace(LoopContext {
            break_target: bb_exit,
            continue_target: bb_cond,
        });

        // Body block
        self.builder.set_current_block(bb_body);
        let body_result = self.builder.temp(Ty::Void);
        self.lower_expr(loop_body, Place::local(body_result), body);
        if !self.builder.is_current_terminated() {
            self.builder.goto(bb_cond);
        }

        // Restore loop context
        self.loop_context = old_loop_ctx;

        self.builder.set_current_block(bb_exit);
    }

    /// Lower a function call.
    fn lower_call(
        &mut self,
        callee: ExprId,
        args: &[ExprId],
        dest: Place,
        body: &ExprBody,
        _result_ty: &baml_typed_ir::Ty,
    ) {
        let callee_expr = body.expr(callee);

        // Check if this is a method call (callee is FieldAccess)
        if let Expr::FieldAccess { base, field } = callee_expr {
            let base_ty = body.ty(*base);
            let thir_base_ty = Self::lower_typed_ir_ty(base_ty);

            if let Some((def, _)) =
                baml_thir::builtins::lookup_method(&thir_base_ty, field.as_str())
            {
                // Found a builtin method
                let receiver_local = self.builder.temp(thir_base_ty);
                self.lower_expr(*base, Place::local(receiver_local), body);

                let mut all_args = vec![Operand::copy_local(receiver_local)];
                for &arg in args {
                    let arg_ty = Self::lower_typed_ir_ty(body.ty(arg));
                    let arg_local = self.builder.temp(arg_ty);
                    self.lower_expr(arg, Place::local(arg_local), body);
                    all_args.push(Operand::copy_local(arg_local));
                }

                let continue_block = self.builder.create_block();

                self.builder.call(
                    Operand::Constant(Constant::Function(Name::new(def.path))),
                    all_args,
                    dest,
                    continue_block,
                    None,
                );

                self.builder.set_current_block(continue_block);
                return;
            }
        }

        // Regular function call
        let callee_ty = Self::lower_typed_ir_ty(body.ty(callee));
        let callee_local = self.builder.temp(callee_ty);
        self.lower_expr(callee, Place::local(callee_local), body);

        let arg_operands: Vec<Operand<'db>> = args
            .iter()
            .map(|&arg| {
                let arg_ty = Self::lower_typed_ir_ty(body.ty(arg));
                let arg_local = self.builder.temp(arg_ty);
                self.lower_expr(arg, Place::local(arg_local), body);
                Operand::copy_local(arg_local)
            })
            .collect();

        let continue_block = self.builder.create_block();

        self.builder.call(
            Operand::copy_local(callee_local),
            arg_operands,
            dest,
            continue_block,
            None,
        );

        self.builder.set_current_block(continue_block);
    }

    /// Lower an expression that can be assigned to (lvalue).
    fn lower_lvalue(&mut self, expr_id: ExprId, body: &ExprBody) -> Place {
        let expr = body.expr(expr_id);

        match expr {
            Expr::Var(name) => {
                if let Some(&local) = self.locals.get(name) {
                    Place::local(local)
                } else {
                    panic!("BUG: Variable `{name}` not found in lvalue context");
                }
            }

            Expr::Path(segments) if segments.len() == 1 => {
                let name = &segments[0];
                if let Some(&local) = self.locals.get(name) {
                    Place::local(local)
                } else {
                    panic!("BUG: Variable `{name}` not found in lvalue context");
                }
            }

            Expr::Path(segments) => {
                // Multi-segment paths should have been converted to FieldAccess.
                panic!(
                    "BUG: Multi-segment path {segments:?} should have been converted to FieldAccess"
                );
            }

            Expr::FieldAccess { base, field } => {
                let base_place = self.lower_lvalue(*base, body);
                let base_ty = Self::lower_typed_ir_ty(body.ty(*base));
                let field_idx = self.field_index_for_type_and_name(&base_ty, field);
                Place::field(base_place, field_idx)
            }

            Expr::Index { base, index } => {
                let base_place = self.lower_lvalue(*base, body);
                let index_local = self.builder.temp(Ty::Int);
                self.lower_expr(*index, Place::local(index_local), body);
                Place::index(base_place, index_local)
            }

            _ => {
                panic!(
                    "BUG: Expression {:?} is not a valid lvalue",
                    std::mem::discriminant(expr)
                );
            }
        }
    }

    /// Get field index for a type and field name.
    fn field_index_for_type_and_name(&self, ty: &Ty<'db>, field: &Name) -> usize {
        let class_name = self.class_name_from_ty(ty);

        if let Some(ref class_name) = class_name {
            if let Some(fields) = self.class_fields.get(class_name) {
                if let Some(&idx) = fields.get(&field.to_string()) {
                    return idx;
                }
                panic!(
                    "BUG: Field `{}` not found in class `{}`. Available fields: {:?}",
                    field,
                    class_name,
                    fields.keys().collect::<Vec<_>>()
                );
            }
            panic!(
                "BUG: Class `{}` not found in class_fields map. Available classes: {:?}",
                class_name,
                self.class_fields.keys().collect::<Vec<_>>()
            );
        }

        panic!("BUG: Cannot extract class name from type {ty:?} for field access `{field}`");
    }

    /// Extract class name from a Ty.
    fn class_name_from_ty(&self, ty: &Ty<'db>) -> Option<String> {
        match ty {
            Ty::Named(name) => Some(name.to_string()),
            Ty::Class(class_id) => {
                let file = class_id.file(self.db);
                let item_tree = baml_hir::file_item_tree(self.db, file);
                let class_data = &item_tree[class_id.id(self.db)];
                Some(class_data.name.to_string())
            }
            _ => None,
        }
    }

    /// Convert a `TypedIR` type to a THIR type for MIR locals.
    fn lower_typed_ir_ty(ty: &baml_typed_ir::Ty) -> Ty<'db> {
        match ty {
            baml_typed_ir::Ty::Int => Ty::Int,
            baml_typed_ir::Ty::Float => Ty::Float,
            baml_typed_ir::Ty::String => Ty::String,
            baml_typed_ir::Ty::Bool => Ty::Bool,
            baml_typed_ir::Ty::Null => Ty::Null,
            baml_typed_ir::Ty::Image => Ty::Image,
            baml_typed_ir::Ty::Audio => Ty::Audio,
            baml_typed_ir::Ty::Video => Ty::Video,
            baml_typed_ir::Ty::Pdf => Ty::Pdf,
            baml_typed_ir::Ty::Class(name) | baml_typed_ir::Ty::Enum(name) => {
                Ty::Named(name.clone())
            }
            baml_typed_ir::Ty::Optional(inner) => {
                Ty::Optional(Box::new(Self::lower_typed_ir_ty(inner)))
            }
            baml_typed_ir::Ty::List(inner) => Ty::List(Box::new(Self::lower_typed_ir_ty(inner))),
            baml_typed_ir::Ty::Map { key, value } => Ty::Map {
                key: Box::new(Self::lower_typed_ir_ty(key)),
                value: Box::new(Self::lower_typed_ir_ty(value)),
            },
            baml_typed_ir::Ty::Union(types) => {
                Ty::Union(types.iter().map(Self::lower_typed_ir_ty).collect())
            }
            baml_typed_ir::Ty::Function { params, ret } => Ty::Function {
                params: params.iter().map(Self::lower_typed_ir_ty).collect(),
                ret: Box::new(Self::lower_typed_ir_ty(ret)),
            },
            baml_typed_ir::Ty::Unknown => Ty::Unknown,
            baml_typed_ir::Ty::Error => Ty::Error,
            baml_typed_ir::Ty::Unit => Ty::Void,
            baml_typed_ir::Ty::Never => Ty::Void, // Never is used for diverging expressions
        }
    }
}
