//! Demonstration: Lowering `TypedIR` to MIR.
//!
//! This module shows how much cleaner MIR lowering is when starting from
//! `TypedIR`'s expression-only representation compared to HIR's statement/expression split.
//!
//! # Comparison with HIR -> MIR lowering
//!
//! ## HIR -> MIR (current approach in `baml_mir/src/lower.rs`)
//!
//! The HIR lowering requires:
//! - Separate `lower_expr_to_place` and `lower_stmt` methods
//! - Special handling for `Block { stmts, tail_expr }`:
//!   - Iterate over statements
//!   - Check if terminated after each
//!   - Handle tail expression separately
//! - `lower_expr_for_effect` helper for void-typed expressions
//! - Complex handling for the statement/expression boundary
//!
//! ## `TypedIR` -> MIR (this approach)
//!
//! With `TypedIR`, we have a single `lower_expr` method that handles everything:
//! - `Let { value, body }` → lower value, create local, lower body
//! - `Seq { first, second }` → lower first, lower second, return second
//! - `While`, `If`, etc. → just lower the sub-expressions
//!
//! No special cases, no statement handling, no "tail expression" logic.
//!
//! # Key Insight
//!
//! The expression-only representation makes the code structure mirror the
//! semantics directly. Each expression variant has exactly one lowering
//! case, and the recursive structure handles scoping automatically.

#![allow(dead_code)] // This is a demonstration module

use std::collections::HashMap;

use baml_base::Name;

use crate::{AssignOp, BinaryOp, Expr, ExprBody, ExprId, Literal, Pattern, UnaryOp};

// Re-export MIR types we'd use (in a real impl, these would come from baml_mir)
// For now, we just define the lowering structure to demonstrate the pattern.

/// Demonstration of how clean `TypedIR` -> MIR lowering is.
///
/// Compare this to the 600+ line `lower.rs` in `baml_mir` which must handle
/// the statement/expression split, tail expressions, and separate paths
/// for effect-only lowering.
pub struct TypedIrToMir<'a> {
    body: &'a ExprBody,
    /// Maps `TypedIR` variable names to MIR locals.
    locals: HashMap<String, usize>,
    /// Next local index to allocate.
    next_local: usize,
    /// Current loop context for break/continue.
    loop_context: Option<LoopContext>,
}

#[derive(Clone)]
struct LoopContext {
    break_block: usize,
    continue_block: usize,
}

impl<'a> TypedIrToMir<'a> {
    pub fn new(body: &'a ExprBody) -> Self {
        Self {
            body,
            locals: HashMap::new(),
            next_local: 0,
            loop_context: None,
        }
    }

    /// The entire lowering is just this one method!
    ///
    /// Compare to HIR lowering which needs:
    /// - `lower_expr_to_place` (handles expressions)
    /// - `lower_stmt` (handles statements separately)
    /// - `lower_expr_for_effect` (handles void expressions)
    /// - Special `Block` handling with tail expression logic
    ///
    /// With `TypedIR`, it's all just `lower_expr`. The structure is uniform.
    pub fn lower_expr(&mut self, id: ExprId, dest: Destination) -> LowerResult {
        let expr = self.body.expr(id);
        let _ty = self.body.ty(id); // Available for type-directed lowering

        match expr {
            // === Literals ===
            // Clean: just emit the constant
            Expr::Literal(lit) => {
                let value = Self::lower_literal(lit);
                self.emit_assign(dest, value);
                LowerResult::Continue
            }

            Expr::Unit => {
                // Unit is just a no-op or null assignment
                self.emit_assign(dest, MirValue::Null);
                LowerResult::Continue
            }

            // === Variables ===
            Expr::Var(name) => {
                if let Some(&local) = self.locals.get(&name.to_string()) {
                    self.emit_assign(dest, MirValue::Local(local));
                } else {
                    // Could be a function reference
                    self.emit_assign(dest, MirValue::Function(name.to_string()));
                }
                LowerResult::Continue
            }

            Expr::Path(segments) => {
                // Handle multi-segment paths (field access chains)
                self.lower_path(segments, dest)
            }

            // === Binding & Sequencing ===
            // This is the key difference from HIR!
            //
            // In HIR, we have:
            //   Block { stmts, tail_expr }
            //   Stmt::Let { pattern, initializer }
            //
            // And we need complex logic to:
            //   1. Iterate over statements
            //   2. Handle each statement type differently
            //   3. Track if we terminated early
            //   4. Then handle tail_expr specially
            //
            // In TypedIR, Let and Seq are just expressions:
            Expr::Let {
                pattern,
                ty: _var_ty,
                value,
                body,
            } => {
                // 1. Lower the value
                let local = self.alloc_local();
                self.lower_expr(*value, Destination::Local(local));

                // 2. Bind the variable
                let pat = self.body.pattern(*pattern);
                // Only handle simple binding patterns for now - match patterns are handled separately
                if let Pattern::Binding(name) = pat {
                    self.locals.insert(name.to_string(), local);
                } else if let Pattern::TypedBinding { name, .. } = pat {
                    self.locals.insert(name.to_string(), local);
                }
                // Other pattern types (Literal, EnumVariant, Union) don't introduce bindings in let statements

                // 3. Lower the body - this IS the result
                // No special "tail expression" handling needed!
                self.lower_expr(*body, dest)
            }

            Expr::Seq { first, second } => {
                // Lower first for effect (result discarded)
                let temp = self.alloc_local();
                let result = self.lower_expr(*first, Destination::Local(temp));

                // If first diverged, we're done
                if result.is_diverged() {
                    return result;
                }

                // Lower second - this is our result
                self.lower_expr(*second, dest)
            }

            // === Control Flow ===
            // These are also cleaner because If/While are expressions
            // that return values, not statements that need special handling.
            Expr::If {
                condition,
                then_branch,
                else_branch,
            } => self.lower_if(*condition, *then_branch, *else_branch, dest),

            Expr::While { condition, body } => {
                self.lower_while(*condition, *body);
                // While returns Unit
                self.emit_assign(dest, MirValue::Null);
                LowerResult::Continue
            }

            Expr::Return(expr) => {
                if let Some(e) = expr {
                    // Lower to return place (_0)
                    self.lower_expr(*e, Destination::Local(0));
                }
                self.emit_return();
                LowerResult::Diverged
            }

            Expr::Break => {
                if let Some(ctx) = &self.loop_context {
                    self.emit_goto(ctx.break_block);
                }
                LowerResult::Diverged
            }

            Expr::Continue => {
                if let Some(ctx) = &self.loop_context {
                    self.emit_goto(ctx.continue_block);
                }
                LowerResult::Diverged
            }

            // === Assignment ===
            // In HIR these are statements; in TypedIR they're expressions returning Unit
            Expr::Assign { target, value } => {
                let val_local = self.alloc_local();
                self.lower_expr(*value, Destination::Local(val_local));

                // Lower target as lvalue
                let place = self.lower_lvalue(*target);
                self.emit_assign_place(place, MirValue::Local(val_local));

                // Return unit
                self.emit_assign(dest, MirValue::Null);
                LowerResult::Continue
            }

            Expr::AssignOp { target, op, value } => {
                // target op= value → target = target op value
                let target_val = self.alloc_local();
                self.lower_expr(*target, Destination::Local(target_val));

                let rhs_val = self.alloc_local();
                self.lower_expr(*value, Destination::Local(rhs_val));

                let result = self.alloc_local();
                self.emit_binop(*op, target_val, rhs_val, result);

                let place = self.lower_lvalue(*target);
                self.emit_assign_place(place, MirValue::Local(result));

                self.emit_assign(dest, MirValue::Null);
                LowerResult::Continue
            }

            // === Operations ===
            Expr::Binary { op, lhs, rhs } => {
                // Short-circuit for && and ||
                if matches!(op, BinaryOp::And | BinaryOp::Or) {
                    return self.lower_short_circuit(*op, *lhs, *rhs, dest);
                }

                let lhs_local = self.alloc_local();
                self.lower_expr(*lhs, Destination::Local(lhs_local));

                let rhs_local = self.alloc_local();
                self.lower_expr(*rhs, Destination::Local(rhs_local));

                self.emit_binop_to_dest(*op, lhs_local, rhs_local, dest);
                LowerResult::Continue
            }

            Expr::Unary { op, operand } => {
                let operand_local = self.alloc_local();
                self.lower_expr(*operand, Destination::Local(operand_local));
                self.emit_unary_to_dest(*op, operand_local, dest);
                LowerResult::Continue
            }

            // === Calls ===
            Expr::Call { callee, args } => {
                let callee_local = self.alloc_local();
                self.lower_expr(*callee, Destination::Local(callee_local));

                let arg_locals: Vec<_> = args
                    .iter()
                    .map(|a| {
                        let local = self.alloc_local();
                        self.lower_expr(*a, Destination::Local(local));
                        local
                    })
                    .collect();

                self.emit_call(callee_local, arg_locals, dest);
                LowerResult::Continue
            }

            // === Data Structures ===
            Expr::Array { elements } => {
                let elem_locals: Vec<_> = elements
                    .iter()
                    .map(|e| {
                        let local = self.alloc_local();
                        self.lower_expr(*e, Destination::Local(local));
                        local
                    })
                    .collect();
                self.emit_array(elem_locals, dest);
                LowerResult::Continue
            }

            Expr::Object { type_name, fields } => {
                let field_vals: Vec<_> = fields
                    .iter()
                    .map(|(name, e)| {
                        let local = self.alloc_local();
                        self.lower_expr(*e, Destination::Local(local));
                        (name.to_string(), local)
                    })
                    .collect();
                self.emit_object(
                    type_name.as_ref().map(std::string::ToString::to_string),
                    field_vals,
                    dest,
                );
                LowerResult::Continue
            }

            // === Access ===
            Expr::FieldAccess { base, field } => {
                let base_local = self.alloc_local();
                self.lower_expr(*base, Destination::Local(base_local));
                self.emit_field_access(base_local, field.to_string(), dest);
                LowerResult::Continue
            }

            Expr::Index { base, index } => {
                let base_local = self.alloc_local();
                self.lower_expr(*base, Destination::Local(base_local));

                let index_local = self.alloc_local();
                self.lower_expr(*index, Destination::Local(index_local));

                self.emit_index(base_local, index_local, dest);
                LowerResult::Continue
            }

            Expr::Match { scrutinee, arms } => {
                // Lower scrutinee
                let scrutinee_local = self.alloc_local();
                self.lower_expr(*scrutinee, Destination::Local(scrutinee_local));

                // Create blocks for each arm and a join block
                let join_block = self.create_block();

                // For now, we implement match as a series of if-else branches
                // A more sophisticated implementation would use decision trees
                for arm in arms {
                    let arm_block = self.create_block();
                    let next_block = self.create_block();

                    // Emit pattern match test and branch
                    // (simplified - real impl would handle all pattern types)
                    self.emit_branch(scrutinee_local, arm_block, next_block);

                    // Arm body
                    self.set_current_block(arm_block);
                    if let Some(guard) = arm.guard {
                        let guard_local = self.alloc_local();
                        self.lower_expr(guard, Destination::Local(guard_local));
                        // Would branch on guard result
                    }
                    self.lower_expr(arm.body, dest.clone());
                    self.emit_goto(join_block);

                    self.set_current_block(next_block);
                }

                // Fallthrough (shouldn't happen with exhaustive matching)
                self.emit_goto(join_block);
                self.set_current_block(join_block);
                LowerResult::Continue
            }
        }
    }

    // === Helper methods ===
    // These would emit actual MIR in a real implementation

    fn alloc_local(&mut self) -> usize {
        let local = self.next_local;
        self.next_local += 1;
        local
    }

    fn lower_literal(lit: &Literal) -> MirValue {
        match lit {
            Literal::Int(n) => MirValue::Int(*n),
            Literal::Float(s) => MirValue::Float(s.clone()),
            Literal::String(s) => MirValue::String(s.clone()),
            Literal::Bool(b) => MirValue::Bool(*b),
            Literal::Null => MirValue::Null,
        }
    }

    #[allow(clippy::unused_self)]
    fn lower_path(&mut self, _segments: &[Name], _dest: Destination) -> LowerResult {
        // Would handle field access chains
        LowerResult::Continue
    }

    #[allow(clippy::unused_self)]
    fn lower_lvalue(&mut self, _expr: ExprId) -> MirPlace {
        // Would lower to MIR place
        MirPlace::Local(0)
    }

    fn lower_if(
        &mut self,
        condition: ExprId,
        then_branch: ExprId,
        else_branch: Option<ExprId>,
        dest: Destination,
    ) -> LowerResult {
        let cond_local = self.alloc_local();
        self.lower_expr(condition, Destination::Local(cond_local));

        // Create blocks for then, else, join
        let then_block = self.create_block();
        let else_block = self.create_block();
        let join_block = self.create_block();

        self.emit_branch(cond_local, then_block, else_block);

        // Then branch
        self.set_current_block(then_block);
        self.lower_expr(then_branch, dest.clone());
        self.emit_goto(join_block);

        // Else branch
        self.set_current_block(else_block);
        if let Some(else_e) = else_branch {
            self.lower_expr(else_e, dest);
        } else {
            self.emit_assign(dest, MirValue::Null);
        }
        self.emit_goto(join_block);

        self.set_current_block(join_block);
        LowerResult::Continue
    }

    fn lower_while(&mut self, condition: ExprId, body: ExprId) {
        let cond_block = self.create_block();
        let body_block = self.create_block();
        let exit_block = self.create_block();

        self.emit_goto(cond_block);

        // Condition
        self.set_current_block(cond_block);
        let cond_local = self.alloc_local();
        self.lower_expr(condition, Destination::Local(cond_local));
        self.emit_branch(cond_local, body_block, exit_block);

        // Body
        let old_ctx = self.loop_context.replace(LoopContext {
            break_block: exit_block,
            continue_block: cond_block,
        });
        self.set_current_block(body_block);
        let temp = self.alloc_local();
        self.lower_expr(body, Destination::Local(temp));
        self.emit_goto(cond_block);
        self.loop_context = old_ctx;

        self.set_current_block(exit_block);
    }

    #[allow(clippy::unused_self)]
    fn lower_short_circuit(
        &mut self,
        _op: BinaryOp,
        _lhs: ExprId,
        _rhs: ExprId,
        _dest: Destination,
    ) -> LowerResult {
        // Would implement && and || with branching
        LowerResult::Continue
    }

    // Stub methods for MIR emission
    // These are placeholders that will be implemented when we integrate with the real MIR builder.
    #[allow(clippy::unused_self)]
    fn emit_assign(&mut self, _dest: Destination, _value: MirValue) {}
    #[allow(clippy::unused_self)]
    fn emit_assign_place(&mut self, _place: MirPlace, _value: MirValue) {}
    #[allow(clippy::unused_self)]
    fn emit_binop(&mut self, _op: AssignOp, _lhs: usize, _rhs: usize, _dest: usize) {}
    #[allow(clippy::unused_self)]
    fn emit_binop_to_dest(&mut self, _op: BinaryOp, _lhs: usize, _rhs: usize, _dest: Destination) {}
    #[allow(clippy::unused_self)]
    fn emit_unary_to_dest(&mut self, _op: UnaryOp, _operand: usize, _dest: Destination) {}
    #[allow(clippy::unused_self)]
    fn emit_call(&mut self, _callee: usize, _args: Vec<usize>, _dest: Destination) {}
    #[allow(clippy::unused_self)]
    fn emit_array(&mut self, _elems: Vec<usize>, _dest: Destination) {}
    #[allow(clippy::unused_self)]
    fn emit_object(
        &mut self,
        _ty: Option<String>,
        _fields: Vec<(String, usize)>,
        _dest: Destination,
    ) {
    }
    #[allow(clippy::unused_self)]
    fn emit_field_access(&mut self, _base: usize, _field: String, _dest: Destination) {}
    #[allow(clippy::unused_self)]
    fn emit_index(&mut self, _base: usize, _index: usize, _dest: Destination) {}
    #[allow(clippy::unused_self)]
    fn emit_return(&mut self) {}
    #[allow(clippy::unused_self)]
    fn emit_goto(&mut self, _block: usize) {}
    #[allow(clippy::unused_self)]
    fn emit_branch(&mut self, _cond: usize, _then: usize, _else: usize) {}
    #[allow(clippy::unused_self)]
    fn create_block(&mut self) -> usize {
        0
    }
    #[allow(clippy::unused_self)]
    fn set_current_block(&mut self, _block: usize) {}
}

// Placeholder types for demonstration
#[derive(Clone)]
pub enum Destination {
    Local(usize),
    Discard,
}

pub enum LowerResult {
    Continue,
    Diverged,
}

impl LowerResult {
    fn is_diverged(&self) -> bool {
        matches!(self, LowerResult::Diverged)
    }
}

enum MirValue {
    Int(i64),
    Float(String),
    String(String),
    Bool(bool),
    Null,
    Local(usize),
    Function(String),
}

enum MirPlace {
    Local(usize),
}
