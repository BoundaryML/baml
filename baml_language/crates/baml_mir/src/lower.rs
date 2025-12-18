//! Lowering from THIR/HIR to MIR.
//!
//! This module converts the tree-structured HIR (with THIR type information)
//! into the CFG-based MIR representation.
//!
//! # Key Concepts
//!
//! - **Destinations**: When lowering expressions, we specify where the result should go
//! - **Break/Continue Targets**: Loop context tracks where to jump for break/continue
//! - **Short-circuit Evaluation**: `&&` and `||` become branches rather than `BinOp`
//!
//! # Lowering Strategy
//!
//! Expressions are lowered in "destination-passing style" - we pass down where the
//! result should be stored, which avoids extra temporaries in many cases.

use std::collections::HashMap;

use baml_base::Name;
use baml_hir::{BinaryOp, ExprBody, ExprId, FunctionBody, FunctionSignature, Pattern, StmtId};
use baml_thir::{InferenceResult, Ty};

use crate::{
    AggregateKind, BinOp, BlockId, Constant, Local, MirBuilder, MirFunction, Operand, Place,
    Rvalue, UnaryOp,
};

/// Lower a function to MIR.
///
/// The `class_fields` parameter maps class names to their field name -> index mappings.
/// This is used to resolve field access expressions like `obj.field` to the correct field index.
pub fn lower_function<'db>(
    signature: &FunctionSignature,
    body: &FunctionBody,
    inference: &InferenceResult<'db>,
    db: &'db dyn crate::Db,
    class_fields: &HashMap<String, HashMap<String, usize>>,
) -> MirFunction<'db> {
    match body {
        FunctionBody::Expr(expr_body) => {
            let mut ctx =
                LoweringContext::new(db, &signature.name, signature.params.len(), class_fields);
            ctx.lower_expr_body(signature, expr_body, inference);
            ctx.finish()
        }
        FunctionBody::Llm(_) => {
            // LLM functions are handled specially - they dispatch a future
            lower_llm_function(signature, inference, db)
        }
        FunctionBody::Missing => {
            // Empty function - just return void
            let mut builder = MirBuilder::new(signature.name.to_string(), signature.params.len());
            let ret = builder.declare_local(None, Ty::Void, None);
            assert_eq!(ret, Local(0));

            let entry = builder.create_block();
            builder.set_current_block(entry);
            builder.return_();

            builder.build()
        }
    }
}

/// Lower an LLM function to MIR.
///
/// LLM functions dispatch a future to the runtime and await the result.
fn lower_llm_function<'db>(
    signature: &FunctionSignature,
    inference: &InferenceResult<'db>,
    _db: &'db dyn crate::Db,
) -> MirFunction<'db> {
    let mut builder = MirBuilder::new(signature.name.to_string(), signature.params.len());

    // _0: return place
    let ret_ty = inference.return_type.clone();
    let ret = builder.declare_local(None, ret_ty.clone(), None);
    assert_eq!(ret, Local(0));

    // _1..=_n: parameters
    for param in &signature.params {
        let param_ty = inference
            .param_types
            .get(&param.name)
            .cloned()
            .unwrap_or(Ty::Unknown);
        builder.declare_local(Some(param.name.clone()), param_ty, None);
    }

    // Temp for the future handle
    let future_local = builder.temp(Ty::Unknown);

    // bb0: dispatch the LLM call
    let entry = builder.create_block();
    let await_block = builder.create_block();
    let return_block = builder.create_block();

    builder.set_current_block(entry);

    // Build argument operands from parameters
    let args: Vec<Operand<'db>> = (1..=signature.params.len())
        .map(|i| Operand::copy_local(Local(i)))
        .collect();

    // Dispatch the LLM function itself (self-reference)
    builder.dispatch_future(
        Operand::Constant(Constant::Function(Name::new(&signature.name))),
        args,
        Place::local(future_local),
        await_block,
    );

    // bb1: await the future
    builder.set_current_block(await_block);
    builder.await_(
        Place::local(future_local),
        Place::local(ret),
        return_block,
        None, // No unwind for now
    );

    // bb2: return
    builder.set_current_block(return_block);
    builder.return_();

    builder.build()
}

/// Context for lowering HIR/THIR to MIR.
struct LoweringContext<'db, 'ctx> {
    #[allow(dead_code)]
    db: &'db dyn crate::Db,
    builder: MirBuilder<'db>,
    /// Map from HIR variable names to MIR locals.
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
        name: &str,
        arity: usize,
        class_fields: &'ctx HashMap<String, HashMap<String, usize>>,
    ) -> Self {
        Self {
            db,
            builder: MirBuilder::new(name, arity),
            locals: HashMap::new(),
            loop_context: None,
            class_fields,
        }
    }

    fn finish(self) -> MirFunction<'db> {
        self.builder.build()
    }

    /// Lower an expression function body.
    fn lower_expr_body(
        &mut self,
        signature: &FunctionSignature,
        expr_body: &ExprBody,
        inference: &InferenceResult<'db>,
    ) {
        // _0: return place
        let ret_ty = inference.return_type.clone();
        let ret = self.builder.declare_local(None, ret_ty, None);
        assert_eq!(ret, Local(0));

        // _1..=_n: parameters
        for param in &signature.params {
            let param_ty = inference
                .param_types
                .get(&param.name)
                .cloned()
                .unwrap_or(Ty::Unknown);
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
        if let Some(root) = expr_body.root_expr {
            self.lower_expr_to_place(root, Place::local(ret), expr_body, inference);
        }

        // If we haven't terminated the current block, goto exit
        if !self.builder.is_current_terminated() {
            self.builder.goto(exit);
        }

        // Exit block just returns
        self.builder.set_current_block(exit);
        self.builder.return_();
    }

    /// Lower an expression, storing the result in `dest`.
    fn lower_expr_to_place(
        &mut self,
        expr_id: ExprId,
        dest: Place,
        body: &ExprBody,
        inference: &InferenceResult<'db>,
    ) {
        use baml_hir::Expr;

        let expr = &body.exprs[expr_id];

        match expr {
            Expr::Literal(lit) => {
                let constant = Self::lower_literal(lit);
                self.builder
                    .assign(dest, Rvalue::Use(Operand::Constant(constant)));
            }

            Expr::Path(segments) => {
                if segments.len() == 1 {
                    // Simple variable reference
                    let name = &segments[0];
                    if let Some(&local) = self.locals.get(name) {
                        self.builder
                            .assign(dest, Rvalue::Use(Operand::copy_local(local)));
                    } else {
                        // Could be a function reference
                        self.builder.assign(
                            dest,
                            Rvalue::Use(Operand::Constant(Constant::Function(name.clone()))),
                        );
                    }
                } else {
                    // Multi-segment path - need to handle field access chain
                    // First segment is the variable
                    let first = &segments[0];
                    if let Some(&base_local) = self.locals.get(first) {
                        // Get segment types from inference for field indices
                        let segment_types = inference.path_segment_types.get(&expr_id);

                        // Build chain of field accesses
                        let mut current_place = Place::local(base_local);

                        for (i, field) in segments[1..].iter().enumerate() {
                            // Look up field index based on the base type and field name
                            let field_idx = if let Some(types) = segment_types {
                                // types[i] is the type of the receiver at this step
                                self.field_index_for_type_and_name(&types[i], field)
                            } else {
                                // Fallback to position if no type info (error case)
                                i
                            };
                            current_place = Place::field(current_place, field_idx);
                        }

                        self.builder
                            .assign(dest, Rvalue::Use(Operand::Copy(current_place)));
                    } else {
                        // Unknown variable - assign null as placeholder
                        self.builder
                            .assign(dest, Rvalue::Use(Operand::Constant(Constant::Null)));
                    }
                }
            }

            Expr::Binary { op, lhs, rhs } => {
                // Check for short-circuit operators
                match op {
                    BinaryOp::And => {
                        self.lower_short_circuit_and(*lhs, *rhs, dest, body, inference);
                    }
                    BinaryOp::Or => {
                        self.lower_short_circuit_or(*lhs, *rhs, dest, body, inference);
                    }
                    _ => {
                        self.lower_binary_op(*op, *lhs, *rhs, dest, body, inference);
                    }
                }
            }

            Expr::Unary { op, expr } => {
                let operand_ty = inference
                    .expr_types
                    .get(expr)
                    .cloned()
                    .unwrap_or(Ty::Unknown);
                let operand_local = self.builder.temp(operand_ty);
                self.lower_expr_to_place(*expr, Place::local(operand_local), body, inference);

                let mir_op = match op {
                    baml_hir::UnaryOp::Not => UnaryOp::Not,
                    baml_hir::UnaryOp::Neg => UnaryOp::Neg,
                };

                self.builder.assign(
                    dest,
                    Rvalue::UnaryOp {
                        op: mir_op,
                        operand: Operand::copy_local(operand_local),
                    },
                );
            }

            Expr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.lower_if(
                    *condition,
                    *then_branch,
                    *else_branch,
                    dest,
                    body,
                    inference,
                );
            }

            Expr::Call { callee, args } => {
                self.lower_call(*callee, args, dest, body, inference);
            }

            Expr::Block { stmts, tail_expr } => {
                // Lower each statement
                for &stmt_id in stmts {
                    self.lower_stmt(stmt_id, body, inference);
                    // Check if we terminated (return/break/continue)
                    if self.builder.is_current_terminated() {
                        return;
                    }
                }

                // Lower tail expression to destination
                if let Some(tail) = tail_expr {
                    self.lower_expr_to_place(*tail, dest, body, inference);
                } else {
                    // No tail expr - assign void/null
                    self.builder
                        .assign(dest, Rvalue::Use(Operand::Constant(Constant::Null)));
                }
            }

            Expr::FieldAccess { base, field } => {
                let base_ty = inference
                    .expr_types
                    .get(base)
                    .cloned()
                    .unwrap_or(Ty::Unknown);
                let base_local = self.builder.temp(base_ty.clone());
                self.lower_expr_to_place(*base, Place::local(base_local), body, inference);

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

            Expr::Index { base, index } => {
                let base_ty = inference
                    .expr_types
                    .get(base)
                    .cloned()
                    .unwrap_or(Ty::Unknown);
                let base_local = self.builder.temp(base_ty);
                self.lower_expr_to_place(*base, Place::local(base_local), body, inference);

                let index_local = self.builder.temp(Ty::Int);
                self.lower_expr_to_place(*index, Place::local(index_local), body, inference);

                // Index access - this needs special handling in codegen
                // For now, represent as a special rvalue
                self.builder.assign(
                    dest,
                    Rvalue::Use(Operand::Copy(Place::index(
                        Place::local(base_local),
                        index_local,
                    ))),
                );
            }

            Expr::Array { elements } => {
                let elem_operands: Vec<Operand<'db>> = elements
                    .iter()
                    .map(|&elem| {
                        let elem_ty = inference
                            .expr_types
                            .get(&elem)
                            .cloned()
                            .unwrap_or(Ty::Unknown);
                        let elem_local = self.builder.temp(elem_ty);
                        self.lower_expr_to_place(elem, Place::local(elem_local), body, inference);
                        Operand::copy_local(elem_local)
                    })
                    .collect();

                self.builder.assign(dest, Rvalue::Array(elem_operands));
            }

            Expr::Object { type_name, fields } => {
                let field_operands: Vec<Operand<'db>> = fields
                    .iter()
                    .map(|(_, value)| {
                        let value_ty = inference
                            .expr_types
                            .get(value)
                            .cloned()
                            .unwrap_or(Ty::Unknown);
                        let value_local = self.builder.temp(value_ty);
                        self.lower_expr_to_place(
                            *value,
                            Place::local(value_local),
                            body,
                            inference,
                        );
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

            Expr::Missing => {
                self.builder
                    .assign(dest, Rvalue::Use(Operand::Constant(Constant::Null)));
            }
        }
    }

    /// Lower a literal to a constant.
    fn lower_literal(lit: &baml_hir::Literal) -> Constant<'db> {
        match lit {
            baml_hir::Literal::Int(n) => Constant::Int(*n),
            baml_hir::Literal::Float(s) => Constant::Float(s.parse().unwrap_or(0.0)),
            baml_hir::Literal::String(s) => Constant::String(s.clone()),
            baml_hir::Literal::Bool(b) => Constant::Bool(*b),
            baml_hir::Literal::Null => Constant::Null,
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
        inference: &InferenceResult<'db>,
    ) {
        let lhs_ty = inference
            .expr_types
            .get(&lhs)
            .cloned()
            .unwrap_or(Ty::Unknown);
        let rhs_ty = inference
            .expr_types
            .get(&rhs)
            .cloned()
            .unwrap_or(Ty::Unknown);

        let lhs_local = self.builder.temp(lhs_ty);
        self.lower_expr_to_place(lhs, Place::local(lhs_local), body, inference);

        let rhs_local = self.builder.temp(rhs_ty);
        self.lower_expr_to_place(rhs, Place::local(rhs_local), body, inference);

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
            // And/Or are handled as short-circuit, but include for completeness
            BinaryOp::And => BinOp::BitAnd, // Shouldn't reach here
            BinaryOp::Or => BinOp::BitOr,   // Shouldn't reach here
        }
    }

    /// Lower short-circuit AND: `a && b`
    ///
    /// ```text
    /// bb_entry:
    ///     _lhs = <a>
    ///     branch _lhs -> bb_rhs, bb_false
    ///
    /// bb_rhs:
    ///     _dest = <b>
    ///     goto -> bb_join
    ///
    /// bb_false:
    ///     _dest = false
    ///     goto -> bb_join
    ///
    /// bb_join:
    ///     // continue
    /// ```
    fn lower_short_circuit_and(
        &mut self,
        lhs: ExprId,
        rhs: ExprId,
        dest: Place,
        body: &ExprBody,
        inference: &InferenceResult<'db>,
    ) {
        let lhs_local = self.builder.temp(Ty::Bool);
        self.lower_expr_to_place(lhs, Place::local(lhs_local), body, inference);

        let bb_rhs = self.builder.create_block();
        let bb_false = self.builder.create_block();
        let bb_join = self.builder.create_block();

        // Branch on lhs
        self.builder
            .branch(Operand::copy_local(lhs_local), bb_rhs, bb_false);

        // bb_rhs: evaluate rhs
        self.builder.set_current_block(bb_rhs);
        self.lower_expr_to_place(rhs, dest.clone(), body, inference);
        if !self.builder.is_current_terminated() {
            self.builder.goto(bb_join);
        }

        // bb_false: result is false
        self.builder.set_current_block(bb_false);
        self.builder
            .assign(dest, Rvalue::Use(Operand::Constant(Constant::Bool(false))));
        self.builder.goto(bb_join);

        // Continue from join point
        self.builder.set_current_block(bb_join);
    }

    /// Lower short-circuit OR: `a || b`
    fn lower_short_circuit_or(
        &mut self,
        lhs: ExprId,
        rhs: ExprId,
        dest: Place,
        body: &ExprBody,
        inference: &InferenceResult<'db>,
    ) {
        let lhs_local = self.builder.temp(Ty::Bool);
        self.lower_expr_to_place(lhs, Place::local(lhs_local), body, inference);

        let bb_true = self.builder.create_block();
        let bb_rhs = self.builder.create_block();
        let bb_join = self.builder.create_block();

        // Branch on lhs - if true, short circuit
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
        self.lower_expr_to_place(rhs, dest, body, inference);
        if !self.builder.is_current_terminated() {
            self.builder.goto(bb_join);
        }

        // Continue from join point
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
        inference: &InferenceResult<'db>,
    ) {
        let cond_local = self.builder.temp(Ty::Bool);
        self.lower_expr_to_place(condition, Place::local(cond_local), body, inference);

        let bb_then = self.builder.create_block();
        let bb_else = self.builder.create_block();
        let bb_join = self.builder.create_block();

        self.builder
            .branch(Operand::copy_local(cond_local), bb_then, bb_else);

        // Then branch
        self.builder.set_current_block(bb_then);
        self.lower_expr_to_place(then_branch, dest.clone(), body, inference);
        if !self.builder.is_current_terminated() {
            self.builder.goto(bb_join);
        }

        // Else branch
        self.builder.set_current_block(bb_else);
        if let Some(else_expr) = else_branch {
            self.lower_expr_to_place(else_expr, dest, body, inference);
        } else {
            // No else - result is null
            self.builder
                .assign(dest, Rvalue::Use(Operand::Constant(Constant::Null)));
        }
        if !self.builder.is_current_terminated() {
            self.builder.goto(bb_join);
        }

        // Continue from join
        self.builder.set_current_block(bb_join);
    }

    /// Lower a function call.
    fn lower_call(
        &mut self,
        callee: ExprId,
        args: &[ExprId],
        dest: Place,
        body: &ExprBody,
        inference: &InferenceResult<'db>,
    ) {
        use baml_hir::Expr;

        // Check if this is a method call (callee is FieldAccess)
        let callee_expr = &body.exprs[callee];
        if let Expr::FieldAccess { base, field } = callee_expr {
            // Check if this is a builtin method call
            if let Some(receiver_ty) = inference.expr_types.get(base) {
                if let Some((def, _)) =
                    baml_thir::builtins::lookup_method(receiver_ty, field.as_str())
                {
                    // Found a builtin method - emit as function call with receiver as first arg
                    // Lower receiver
                    let receiver_local = self.builder.temp(receiver_ty.clone());
                    self.lower_expr_to_place(*base, Place::local(receiver_local), body, inference);

                    // Lower explicit arguments
                    let mut all_args = vec![Operand::copy_local(receiver_local)];
                    for &arg in args {
                        let arg_ty = inference
                            .expr_types
                            .get(&arg)
                            .cloned()
                            .unwrap_or(Ty::Unknown);
                        let arg_local = self.builder.temp(arg_ty);
                        self.lower_expr_to_place(arg, Place::local(arg_local), body, inference);
                        all_args.push(Operand::copy_local(arg_local));
                    }

                    // Create continuation block
                    let continue_block = self.builder.create_block();

                    // Emit call with function name as constant
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
        }

        // Regular function call
        // Lower callee
        let callee_ty = inference
            .expr_types
            .get(&callee)
            .cloned()
            .unwrap_or(Ty::Unknown);
        let callee_local = self.builder.temp(callee_ty);
        self.lower_expr_to_place(callee, Place::local(callee_local), body, inference);

        // Lower arguments
        let arg_operands: Vec<Operand<'db>> = args
            .iter()
            .map(|&arg| {
                let arg_ty = inference
                    .expr_types
                    .get(&arg)
                    .cloned()
                    .unwrap_or(Ty::Unknown);
                let arg_local = self.builder.temp(arg_ty);
                self.lower_expr_to_place(arg, Place::local(arg_local), body, inference);
                Operand::copy_local(arg_local)
            })
            .collect();

        // Create continuation block
        let continue_block = self.builder.create_block();

        // Emit call
        self.builder.call(
            Operand::copy_local(callee_local),
            arg_operands,
            dest,
            continue_block,
            None, // No unwind for now
        );

        // Continue from after call
        self.builder.set_current_block(continue_block);
    }

    /// Lower a statement.
    fn lower_stmt(&mut self, stmt_id: StmtId, body: &ExprBody, inference: &InferenceResult<'db>) {
        use baml_hir::Stmt;

        let stmt = &body.stmts[stmt_id];

        match stmt {
            Stmt::Let {
                pattern,
                initializer,
                ..
            } => {
                // Get the variable name from the pattern
                let pat = &body.patterns[*pattern];
                let name = match pat {
                    Pattern::Binding(n) => n.clone(),
                };

                // Get the type from the initializer if available
                let ty = initializer
                    .and_then(|init| inference.expr_types.get(&init))
                    .cloned()
                    .unwrap_or(Ty::Unknown);

                // Create local
                let local = self.builder.declare_local(Some(name.clone()), ty, None);
                self.locals.insert(name, local);

                // Lower initializer if present
                if let Some(init) = initializer {
                    self.lower_expr_to_place(*init, Place::local(local), body, inference);
                }
            }

            Stmt::Expr(expr) => {
                // Expression statement - evaluate for side effects
                let result_ty = inference
                    .expr_types
                    .get(expr)
                    .cloned()
                    .unwrap_or(Ty::Unknown);
                let temp = self.builder.temp(result_ty);
                self.lower_expr_to_place(*expr, Place::local(temp), body, inference);
            }

            Stmt::Return(expr) => {
                if let Some(e) = expr {
                    self.lower_expr_to_place(*e, Place::local(Local(0)), body, inference);
                }
                // Jump to a new exit block
                let exit = self.builder.create_block();
                self.builder.goto(exit);
                self.builder.set_current_block(exit);
                self.builder.return_();
            }

            Stmt::While {
                condition,
                body: loop_body,
                after,
                ..
            } => {
                self.lower_while(*condition, *loop_body, *after, body, inference);
            }

            Stmt::Break => {
                if let Some(ctx) = &self.loop_context {
                    let target = ctx.break_target;
                    self.builder.goto(target);
                }
                // If no loop context, this is an error - but we've already type-checked
            }

            Stmt::Continue => {
                if let Some(ctx) = &self.loop_context {
                    let target = ctx.continue_target;
                    self.builder.goto(target);
                }
            }

            Stmt::Assign { target, value } => {
                // Lower value to a temp, then assign to target
                // Target could be a variable or field access
                let value_ty = inference
                    .expr_types
                    .get(value)
                    .cloned()
                    .unwrap_or(Ty::Unknown);
                let value_local = self.builder.temp(value_ty);
                self.lower_expr_to_place(*value, Place::local(value_local), body, inference);

                let target_place = self.lower_assignable_expr(*target, body, inference);
                self.builder
                    .assign(target_place, Rvalue::Use(Operand::copy_local(value_local)));
            }

            Stmt::AssignOp { target, op, value } => {
                // `a += b` becomes `a = a + b`
                let target_place = self.lower_assignable_expr(*target, body, inference);

                // Load current value
                let current_ty = inference
                    .expr_types
                    .get(target)
                    .cloned()
                    .unwrap_or(Ty::Unknown);
                let current_local = self.builder.temp(current_ty);
                self.builder.assign(
                    Place::local(current_local),
                    Rvalue::Use(Operand::Copy(target_place.clone())),
                );

                // Load rhs
                let rhs_ty = inference
                    .expr_types
                    .get(value)
                    .cloned()
                    .unwrap_or(Ty::Unknown);
                let rhs_local = self.builder.temp(rhs_ty);
                self.lower_expr_to_place(*value, Place::local(rhs_local), body, inference);

                // Compute new value
                let mir_op = Self::convert_assign_op(*op);
                let result_ty = inference
                    .expr_types
                    .get(target)
                    .cloned()
                    .unwrap_or(Ty::Unknown);
                let result_local = self.builder.temp(result_ty);
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
            }

            Stmt::Missing => {}
        }
    }

    fn convert_assign_op(op: baml_hir::AssignOp) -> BinOp {
        match op {
            baml_hir::AssignOp::Add => BinOp::Add,
            baml_hir::AssignOp::Sub => BinOp::Sub,
            baml_hir::AssignOp::Mul => BinOp::Mul,
            baml_hir::AssignOp::Div => BinOp::Div,
            baml_hir::AssignOp::Mod => BinOp::Mod,
            baml_hir::AssignOp::BitAnd => BinOp::BitAnd,
            baml_hir::AssignOp::BitOr => BinOp::BitOr,
            baml_hir::AssignOp::BitXor => BinOp::BitXor,
            baml_hir::AssignOp::Shl => BinOp::Shl,
            baml_hir::AssignOp::Shr => BinOp::Shr,
        }
    }

    /// Lower an expression that can be assigned to (lvalue).
    fn lower_assignable_expr(
        &mut self,
        expr_id: ExprId,
        body: &ExprBody,
        inference: &InferenceResult<'db>,
    ) -> Place {
        use baml_hir::Expr;

        let expr = &body.exprs[expr_id];

        match expr {
            Expr::Path(segments) if segments.len() == 1 => {
                let name = &segments[0];
                if let Some(&local) = self.locals.get(name) {
                    Place::local(local)
                } else {
                    // Unknown variable - create a temp (will error at runtime)
                    Place::local(self.builder.temp(Ty::Unknown))
                }
            }
            Expr::Path(segments) => {
                // Field access chain
                let first = &segments[0];
                if let Some(&base_local) = self.locals.get(first) {
                    let segment_types = inference.path_segment_types.get(&expr_id);
                    let mut place = Place::local(base_local);
                    for (i, field) in segments[1..].iter().enumerate() {
                        let field_idx = if let Some(types) = segment_types {
                            self.field_index_for_type_and_name(&types[i], field)
                        } else {
                            i
                        };
                        place = Place::field(place, field_idx);
                    }
                    place
                } else {
                    Place::local(self.builder.temp(Ty::Unknown))
                }
            }
            Expr::FieldAccess { base, field } => {
                let base_place = self.lower_assignable_expr(*base, body, inference);
                let base_ty = inference
                    .expr_types
                    .get(base)
                    .cloned()
                    .unwrap_or(Ty::Unknown);
                let field_idx = self.field_index_for_type_and_name(&base_ty, field);
                Place::field(base_place, field_idx)
            }
            Expr::Index { base, index } => {
                let base_place = self.lower_assignable_expr(*base, body, inference);
                let index_local = self.builder.temp(Ty::Int);
                self.lower_expr_to_place(*index, Place::local(index_local), body, inference);
                Place::index(base_place, index_local)
            }
            _ => {
                // Not assignable - return a dummy
                Place::local(self.builder.temp(Ty::Unknown))
            }
        }
    }

    /// Lower a while loop.
    ///
    /// ```text
    /// bb_cond:
    ///     _cond = <condition>
    ///     branch _cond -> bb_body, bb_exit
    ///
    /// bb_body:
    ///     <body>
    ///     goto -> bb_after (or bb_cond if no after)
    ///
    /// bb_after: (only if after statement exists)
    ///     <after>
    ///     goto -> bb_cond
    ///
    /// bb_exit:
    ///     // continue
    /// ```
    fn lower_while(
        &mut self,
        condition: ExprId,
        loop_body: ExprId,
        after: Option<StmtId>,
        body: &ExprBody,
        inference: &InferenceResult<'db>,
    ) {
        let bb_cond = self.builder.create_block();
        let bb_body = self.builder.create_block();
        let bb_after = if after.is_some() {
            Some(self.builder.create_block())
        } else {
            None
        };
        let bb_exit = self.builder.create_block();

        // Jump to condition check
        self.builder.goto(bb_cond);

        // Condition block
        self.builder.set_current_block(bb_cond);
        let cond_local = self.builder.temp(Ty::Bool);
        self.lower_expr_to_place(condition, Place::local(cond_local), body, inference);
        self.builder
            .branch(Operand::copy_local(cond_local), bb_body, bb_exit);

        // Set up loop context for break/continue
        let continue_target = bb_after.unwrap_or(bb_cond);
        let old_loop_ctx = self.loop_context.replace(LoopContext {
            break_target: bb_exit,
            continue_target,
        });

        // Body block
        self.builder.set_current_block(bb_body);
        let body_result = self.builder.temp(Ty::Void);
        self.lower_expr_to_place(loop_body, Place::local(body_result), body, inference);
        if !self.builder.is_current_terminated() {
            self.builder.goto(continue_target);
        }

        // After block (for C-style for loop update)
        if let Some(bb_after) = bb_after {
            self.builder.set_current_block(bb_after);
            if let Some(after_stmt) = after {
                self.lower_stmt(after_stmt, body, inference);
            }
            if !self.builder.is_current_terminated() {
                self.builder.goto(bb_cond);
            }
        }

        // Restore loop context
        self.loop_context = old_loop_ctx;

        // Continue from exit
        self.builder.set_current_block(bb_exit);
    }

    /// Get field index for a type and field name.
    fn field_index_for_type_and_name(&self, ty: &Ty<'db>, field: &Name) -> usize {
        // Extract class name from type and look up field index
        let class_name = self.class_name_from_ty(ty);

        if let Some(class_name) = class_name {
            if let Some(fields) = self.class_fields.get(&class_name) {
                if let Some(&idx) = fields.get(&field.to_string()) {
                    return idx;
                }
            }
        }

        // Default to 0 if we can't resolve (error case)
        0
    }

    /// Extract class name from a Ty.
    fn class_name_from_ty(&self, ty: &Ty<'db>) -> Option<String> {
        match ty {
            Ty::Named(name) => Some(name.to_string()),
            Ty::Class(class_id) => {
                // Look up the class name from the database
                let file = class_id.file(self.db);
                let item_tree = baml_hir::file_item_tree(self.db, file);
                let class_data = &item_tree[class_id.id(self.db)];
                Some(class_data.name.to_string())
            }
            _ => None,
        }
    }
}
