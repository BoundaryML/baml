//! Lowering from VIR to MIR.
//!
//! This module converts the expression-only VIR representation into
//! the CFG-based MIR. Because VIR has no statements (everything is an
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

use baml_base::{Name, QualifiedName, Span};
use baml_compiler_hir::FunctionSignature;
use baml_compiler_tir::{ResolvedValue, TypeResolutionContext};
use baml_compiler_vir::{
    AssignOp, BinaryOp, Expr, ExprBody, ExprId, Literal, PatId, Pattern, UnaryOp,
};
use baml_type::{Ty, TypeName};

use crate::{
    AggregateKind, BinOp, BlockId, Constant, Local, MirFunction, Operand, Place, Rvalue,
    StatementKind, UnaryOp as MirUnaryOp, VizNode, VizNodeType, builder::MirBuilder,
};

/// Source of a field value in spread expansion.
enum FieldSource {
    /// Field value comes from a named field assignment.
    Named(ExprId),
    /// Field value comes from a spread source (local containing spread value, field index).
    Spread(Local, usize),
}

/// Kind of switch optimization being performed.
///
/// This determines how the scrutinee is transformed before the switch.
enum SwitchKind {
    /// Direct integer comparison - no transformation needed.
    Integer,
    /// Enum variant switch - emit `Discriminant` to extract variant index first.
    EnumDiscriminant(Name),
    /// Type tag switch for union types - emit `TypeTag` to extract runtime type first.
    /// Only supports primitive types (int, string, bool, float, null) which have fixed tags.
    TypeTag,
}

/// Lower a function from VIR to MIR.
///
/// This is the main entry point for VIR → MIR lowering.
#[allow(clippy::too_many_arguments)]
pub fn lower<'ctx>(
    signature: &FunctionSignature,
    typed_body: &ExprBody,
    db: &dyn crate::Db,
    class_fields: &'ctx HashMap<String, HashMap<String, usize>>,
    enum_variants: &'ctx HashMap<String, HashMap<String, usize>>,
    class_type_tags: &'ctx HashMap<String, i64>,
    resolution_ctx: &'ctx TypeResolutionContext,
    type_aliases: &'ctx HashMap<Name, baml_compiler_tir::Ty>,
    recursive_aliases: &'ctx std::collections::HashSet<Name>,
) -> MirFunction {
    let mut ctx = LoweringContext::new(
        db,
        signature.params.len(),
        class_fields,
        enum_variants,
        class_type_tags,
        resolution_ctx,
        type_aliases,
        recursive_aliases,
    );
    ctx.lower_function(signature, typed_body);
    ctx.finish()
}

/// Context for lowering VIR to MIR.
struct LoweringContext<'a, 'ctx> {
    #[allow(dead_code)]
    db: &'a dyn crate::Db,
    builder: MirBuilder,
    /// Map from variable names to MIR locals.
    locals: HashMap<Name, Local>,
    /// Current loop context for break/continue.
    loop_context: Option<LoopContext>,
    /// Class field mappings (class name -> field name -> field index).
    class_fields: &'ctx HashMap<String, HashMap<String, usize>>,
    /// Enum variant mappings (enum name -> variant name -> variant index).
    enum_variants: &'ctx HashMap<String, HashMap<String, usize>>,
    /// Class type tags (class name -> type tag) for `TypeTag` switch optimization.
    class_type_tags: &'ctx HashMap<String, i64>,
    /// Type resolution context for lowering type refs.
    resolution_ctx: &'ctx TypeResolutionContext,
    /// Type alias mappings (alias name -> resolved type).
    type_aliases: &'ctx HashMap<Name, baml_compiler_tir::Ty>,
    /// Set of recursive type aliases.
    recursive_aliases: &'ctx std::collections::HashSet<Name>,
    /// Stack of watched locals for tracking scope exit.
    watched_locals_stack: Vec<Local>,
    /// Viz context for control flow visualization.
    viz_context: VizContext,
    /// Pending header for control flow visualization.
    /// When a `//# header` is seen, this is set. When control flow (if/while) follows,
    /// VizEnter/VizExit will be emitted for that control flow.
    pending_header: Option<PendingHeader>,
}

/// A pending header waiting for control flow.
#[derive(Clone)]
struct PendingHeader {
    name: String,
}

/// Context for control flow visualization.
struct VizContext {
    /// Function name for `log_filter_key` prefix.
    function_name: String,
    /// Counter for generating unique node IDs.
    next_node_id: u32,
    /// Stack of parent `log_filter_keys`.
    parent_keys: Vec<String>,
    /// Counter for ordinal within current scope (for unique paths).
    ordinal_counters: Vec<u16>,
}

impl VizContext {
    /// Get the current ordinal and increment for next use.
    fn get_and_increment_ordinal(&mut self) -> u16 {
        let ordinal = *self.ordinal_counters.last().unwrap_or(&0);
        if let Some(last) = self.ordinal_counters.last_mut() {
            *last += 1;
        }
        ordinal
    }
}

/// Context for the current loop (for break/continue).
#[derive(Clone)]
struct LoopContext {
    /// Block to jump to on `break`.
    break_target: BlockId,
    /// Block to jump to on `continue`.
    continue_target: BlockId,
    /// Depth of `watched_locals_stack` at loop entry.
    /// Used to emit Unwatch for watched locals on break/continue.
    watched_locals_depth: usize,
}

impl<'a, 'ctx> LoweringContext<'a, 'ctx> {
    #[allow(clippy::too_many_arguments)]
    fn new(
        db: &'a dyn crate::Db,
        arity: usize,
        class_fields: &'ctx HashMap<String, HashMap<String, usize>>,
        enum_variants: &'ctx HashMap<String, HashMap<String, usize>>,
        class_type_tags: &'ctx HashMap<String, i64>,
        resolution_ctx: &'ctx TypeResolutionContext,
        type_aliases: &'ctx HashMap<Name, baml_compiler_tir::Ty>,
        recursive_aliases: &'ctx std::collections::HashSet<Name>,
    ) -> Self {
        Self {
            db,
            builder: MirBuilder::new(Name::new(""), arity),
            locals: HashMap::new(),
            loop_context: None,
            class_fields,
            enum_variants,
            class_type_tags,
            resolution_ctx,
            type_aliases,
            recursive_aliases,
            watched_locals_stack: Vec::new(),
            viz_context: VizContext {
                function_name: String::new(),
                next_node_id: 0,
                parent_keys: Vec::new(),
                ordinal_counters: vec![0],
            },
            pending_header: None,
        }
    }

    fn finish(self) -> MirFunction {
        self.builder.build()
    }

    /// Convert a TIR type to `baml_type::Ty` for MIR locals.
    /// Uses the shared conversion from `baml_type` which handles FQN→TypeName conversion,
    /// alias expansion, and literal preservation.
    fn convert_tir_ty(&self, tir_ty: &baml_compiler_tir::Ty) -> Ty {
        baml_type::convert_tir_ty(tir_ty, self.type_aliases, self.recursive_aliases)
            .unwrap_or(Ty::Null)
    }

    // ========================================================================
    // Visualization Helpers
    // ========================================================================

    /// Create a new viz node and emit `VizEnter`.
    /// Returns the node index for later `VizExit`.
    /// Currently unused - kept for future use when control flow with headers needs viz nodes.
    #[allow(dead_code)]
    fn viz_enter(&mut self, node_type: VizNodeType, label: &str) -> usize {
        let ordinal = self.viz_context.get_and_increment_ordinal();
        let node_id = self.viz_context.next_node_id;
        self.viz_context.next_node_id += 1;

        // Build the segment key based on node type
        let segment = match node_type {
            VizNodeType::FunctionRoot => format!("fn:{ordinal}"),
            VizNodeType::HeaderContextEnter => format!("hdr:{ordinal}"),
            VizNodeType::BranchGroup => format!("bg:{ordinal}"),
            VizNodeType::BranchArm => format!("ba:{ordinal}"),
            VizNodeType::Loop => format!("loop:{ordinal}"),
            VizNodeType::OtherScope => format!("scope:{ordinal}"),
        };

        // Build log_filter_key from parent path + this segment
        let log_filter_key = if self.viz_context.parent_keys.is_empty() {
            format!("{}|{}", self.viz_context.function_name, segment)
        } else {
            format!(
                "{}|{}",
                self.viz_context.parent_keys.last().unwrap(),
                segment
            )
        };

        let parent_log_filter_key = self.viz_context.parent_keys.last().cloned();

        // Create the viz node
        let node = VizNode {
            node_id,
            log_filter_key: log_filter_key.clone(),
            parent_log_filter_key,
            node_type,
            label: label.to_string(),
            header_level: None,
        };

        let node_idx = self.builder.add_viz_node(node);
        self.builder.viz_enter(node_idx);

        // Push to parent stack for nested nodes
        self.viz_context.parent_keys.push(log_filter_key);
        self.viz_context.ordinal_counters.push(0);

        node_idx
    }

    /// Emit `VizExit` for a previously entered node.
    /// Currently unused - kept for future use when control flow with headers needs viz nodes.
    #[allow(dead_code)]
    fn viz_exit(&mut self, node_idx: usize) {
        self.builder.viz_exit(node_idx);
        self.viz_context.parent_keys.pop();
        self.viz_context.ordinal_counters.pop();
    }

    /// Create a new viz node and emit `VizEnter`, but don't track for `VizExit`.
    /// Used for headers which are hierarchical scopes without explicit exit.
    /// Currently unused - kept for future use when control flow with headers needs viz nodes.
    #[allow(dead_code, clippy::cast_possible_truncation)]
    fn viz_enter_header(&mut self, label: &str, level: usize) {
        let ordinal = self.viz_context.get_and_increment_ordinal();
        let node_id = self.viz_context.next_node_id;
        self.viz_context.next_node_id += 1;

        let segment = format!("hdr:{ordinal}");

        // Build log_filter_key from parent path + this segment
        let log_filter_key = if self.viz_context.parent_keys.is_empty() {
            format!("{}|{}", self.viz_context.function_name, segment)
        } else {
            format!(
                "{}|{}",
                self.viz_context.parent_keys.last().unwrap(),
                segment
            )
        };

        let parent_log_filter_key = self.viz_context.parent_keys.last().cloned();

        // Create the viz node
        let node = VizNode {
            node_id,
            log_filter_key: log_filter_key.clone(),
            parent_log_filter_key,
            node_type: VizNodeType::HeaderContextEnter,
            label: label.to_string(),
            header_level: Some(level as u8),
        };

        let node_idx = self.builder.add_viz_node(node);
        self.builder.viz_enter(node_idx);

        // Push to parent stack for nested nodes (but no VizExit will be emitted)
        self.viz_context.parent_keys.push(log_filter_key);
        self.viz_context.ordinal_counters.push(0);
    }

    /// Lower a complete function.
    fn lower_function(&mut self, signature: &FunctionSignature, body: &ExprBody) {
        self.builder = MirBuilder::new(signature.name.clone(), signature.params.len());
        self.viz_context.function_name = signature.name.to_string();

        // _0: return place
        // Use signature return type, not body root type (which may be Never for diverging bodies)
        let (ret_ty_tir, _) = self
            .resolution_ctx
            .lower_type_ref(&signature.return_type, Span::default());
        let ret_ty = self.convert_tir_ty(&ret_ty_tir);
        let ret = self.builder.declare_local(None, ret_ty, None, false);
        assert_eq!(ret, Local(0));

        // _1..=_n: parameters
        for param in &signature.params {
            let (param_ty_tir, _) = self
                .resolution_ctx
                .lower_type_ref(&param.type_ref, Span::default());
            let param_ty = self.convert_tir_ty(&param_ty_tir);
            let local = self
                .builder
                .declare_local(Some(param.name.clone()), param_ty, None, false);
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
                // Get resolution from VIR (computed in TIR).
                // TIR must resolve all variables - no fallbacks needed.
                let resolution = body
                    .resolution(expr_id)
                    .unwrap_or_else(|| panic!("Missing resolution for variable: {name}"));

                match resolution {
                    ResolvedValue::Local { name, .. } => {
                        let local = self.locals.get(name).unwrap_or_else(|| {
                            panic!("Resolved local {name} not found in MIR scope")
                        });
                        self.builder
                            .assign(dest, Rvalue::Use(Operand::copy_local(*local)));
                    }
                    ResolvedValue::Function(fqn) => {
                        self.builder.assign(
                            dest,
                            Rvalue::Use(Operand::Constant(Constant::Function(fqn.clone()))),
                        );
                    }
                    ResolvedValue::BuiltinFunction(qn) => {
                        self.builder.assign(
                            dest,
                            Rvalue::Use(Operand::Constant(Constant::Function(qn.clone()))),
                        );
                    }
                    ResolvedValue::EnumVariant { enum_fqn, variant } => {
                        self.builder.assign(
                            dest,
                            Rvalue::Use(Operand::Constant(Constant::EnumVariant {
                                enum_qn: enum_fqn.clone(),
                                variant: variant.clone(),
                            })),
                        );
                    }
                    ResolvedValue::Class(fqn)
                    | ResolvedValue::Enum(fqn)
                    | ResolvedValue::TypeAlias(fqn) => {
                        // Type used as value (constructor)
                        self.builder.assign(
                            dest,
                            Rvalue::Use(Operand::Constant(Constant::Function(fqn.clone()))),
                        );
                    }
                    ResolvedValue::Unknown => {
                        panic!("Unresolved variable reached MIR: {name}")
                    }
                    // Explicit arms for remaining variants - these shouldn't appear for Var expressions
                    ResolvedValue::Field { .. }
                    | ResolvedValue::ModuleItem { .. }
                    | ResolvedValue::TypeMethod { .. } => {
                        panic!("Unexpected resolution for Var({name}): {resolution:?}")
                    }
                }
            }

            Expr::Path(segments) => {
                // Get resolution from VIR (computed in TIR).
                // TIR must resolve all paths - no fallbacks needed.
                let resolution = body.resolution(expr_id).unwrap_or_else(|| {
                    panic!(
                        "Missing resolution for path expression: {:?}",
                        segments
                            .iter()
                            .map(smol_str::SmolStr::as_str)
                            .collect::<Vec<_>>()
                            .join(".")
                    )
                });

                match resolution {
                    ResolvedValue::Local { name, .. } => {
                        let local = self.locals.get(name).unwrap_or_else(|| {
                            panic!("Resolved local {name} not found in MIR scope")
                        });
                        self.builder
                            .assign(dest, Rvalue::Use(Operand::copy_local(*local)));
                    }
                    ResolvedValue::Function(fqn) => {
                        self.builder.assign(
                            dest,
                            Rvalue::Use(Operand::Constant(Constant::Function(fqn.clone()))),
                        );
                    }
                    ResolvedValue::BuiltinFunction(qn) => {
                        self.builder.assign(
                            dest,
                            Rvalue::Use(Operand::Constant(Constant::Function(qn.clone()))),
                        );
                    }
                    ResolvedValue::EnumVariant { enum_fqn, variant } => {
                        self.builder.assign(
                            dest,
                            Rvalue::Use(Operand::Constant(Constant::EnumVariant {
                                enum_qn: enum_fqn.clone(),
                                variant: variant.clone(),
                            })),
                        );
                    }
                    ResolvedValue::ModuleItem {
                        module_path,
                        item_name,
                    } => {
                        let qn = QualifiedName::from_module_path(module_path, item_name.clone());
                        self.builder
                            .assign(dest, Rvalue::Use(Operand::Constant(Constant::Function(qn))));
                    }
                    ResolvedValue::Class(fqn)
                    | ResolvedValue::Enum(fqn)
                    | ResolvedValue::TypeAlias(fqn) => {
                        // Type references used as values (e.g., constructor calls)
                        self.builder.assign(
                            dest,
                            Rvalue::Use(Operand::Constant(Constant::Function(fqn.clone()))),
                        );
                    }
                    ResolvedValue::TypeMethod {
                        receiver_type,
                        method_name,
                    } => {
                        // Static method call like `image.from_url`
                        let qn = QualifiedName::builtin_method(
                            receiver_type.clone(),
                            method_name.clone(),
                        );
                        self.builder
                            .assign(dest, Rvalue::Use(Operand::Constant(Constant::Function(qn))));
                    }
                    ResolvedValue::Field { .. } => {
                        panic!(
                            "Field resolution should not appear in Path expression - should be FieldAccess"
                        )
                    }
                    ResolvedValue::Unknown => {
                        panic!(
                            "Unresolved path reached MIR: {:?}",
                            segments
                                .iter()
                                .map(smol_str::SmolStr::as_str)
                                .collect::<Vec<_>>()
                                .join(".")
                        )
                    }
                }
            }

            // ========== Binding & Sequencing ==========
            // This is where TypedIR shines - no special tail expression handling!
            Expr::Let {
                pattern,
                ty: var_ty,
                value,
                body: let_body,
                is_watched,
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
                let local_ty = var_ty.clone();
                let local =
                    self.builder
                        .declare_local(Some(name.clone()), local_ty, None, *is_watched);
                self.lower_expr(*value, Place::local(local), body);

                // Bind the variable
                self.locals.insert(name, local);

                // Track watched local for scope exit
                if *is_watched {
                    self.watched_locals_stack.push(local);
                }

                // Lower the body - this IS the result
                // No special "tail expression" handling needed!
                self.lower_expr(*let_body, dest, body);

                // Emit Unwatch when watched local goes out of scope
                if *is_watched {
                    self.watched_locals_stack.pop();
                    // Only emit if body didn't diverge
                    if !self.builder.is_current_terminated() {
                        self.builder.unwatch(local);
                    }
                }
            }

            Expr::Seq { first, second } => {
                // Lower first for effect (result discarded)
                let first_ty = body.ty(*first).clone();
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
                    // Emit Unwatch for all watched locals since loop entry
                    for &local in &self.watched_locals_stack[ctx.watched_locals_depth..] {
                        self.builder.unwatch(local);
                    }
                    let target = ctx.break_target;
                    self.builder.goto(target);
                } else {
                    panic!("BUG: `break` outside of loop context");
                }
            }

            Expr::Continue => {
                if let Some(ctx) = &self.loop_context {
                    // Emit Unwatch for all watched locals since loop entry
                    for &local in &self.watched_locals_stack[ctx.watched_locals_depth..] {
                        self.builder.unwatch(local);
                    }
                    let target = ctx.continue_target;
                    self.builder.goto(target);
                } else {
                    panic!("BUG: `continue` outside of loop context");
                }
            }

            Expr::Assert { condition } => {
                // Evaluate the condition and emit assert statement
                let cond_operand = self.lower_to_operand(*condition, body);
                self.builder.assert(cond_operand);

                // Return unit
                self.builder
                    .assign(dest, Rvalue::Use(Operand::Constant(Constant::Null)));
            }

            // ========== Assignment ==========
            Expr::Assign { target, value } => {
                let value_operand = self.lower_to_operand(*value, body);
                let target_place = self.lower_lvalue(*target, body);
                self.builder
                    .assign(target_place, Rvalue::Use(value_operand));

                // Return unit
                self.builder
                    .assign(dest, Rvalue::Use(Operand::Constant(Constant::Null)));
            }

            Expr::AssignOp { target, op, value } => {
                let target_place = self.lower_lvalue(*target, body);

                // Load current value
                let current_ty = body.ty(*target).clone();
                let current_local = self.builder.temp(current_ty.clone());
                self.builder.assign(
                    Place::local(current_local),
                    Rvalue::Use(Operand::Copy(target_place.clone())),
                );

                // Evaluate rhs
                let rhs_operand = self.lower_to_operand(*value, body);

                // Compute new value
                let mir_op = Self::convert_assign_op(*op);
                let result_local = self.builder.temp(current_ty);
                self.builder.assign(
                    Place::local(result_local),
                    Rvalue::BinaryOp {
                        op: mir_op,
                        left: Operand::copy_local(current_local),
                        right: rhs_operand,
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
                let operand_val = self.lower_to_operand(*operand, body);

                let mir_op = match op {
                    UnaryOp::Not => MirUnaryOp::Not,
                    UnaryOp::Neg => MirUnaryOp::Neg,
                };

                self.builder.assign(
                    dest,
                    Rvalue::UnaryOp {
                        op: mir_op,
                        operand: operand_val,
                    },
                );
            }

            // ========== Function Calls ==========
            Expr::Call { callee, args } => {
                self.lower_call(*callee, args, dest, body, ty);
            }

            // ========== Data Structures ==========
            Expr::Array { elements } => {
                let elem_operands: Vec<Operand> = elements
                    .iter()
                    .map(|&e| self.lower_to_operand(e, body))
                    .collect();

                self.builder.assign(dest, Rvalue::Array(elem_operands));
            }

            Expr::Object {
                type_name,
                fields,
                spreads,
            } => {
                // If there are no spreads, use the simple aggregate approach
                if spreads.is_empty() {
                    let field_operands: Vec<Operand> = fields
                        .iter()
                        .map(|(_, v)| self.lower_to_operand(*v, body))
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
                } else {
                    // With spreads, we need to determine the final value for each field
                    // based on position-based override semantics (last assignment wins).
                    //
                    // Algorithm:
                    // 1. Evaluate all spread sources into temp locals
                    // 2. Get all class fields in definition order
                    // 3. For each class field, determine source (named field or spread)
                    //    based on which has the highest position
                    // 4. Generate field operands accordingly

                    let class_name = type_name
                        .as_ref()
                        .map(std::string::ToString::to_string)
                        .unwrap_or_else(|| "Anonymous".to_string());

                    // Get class fields and invert to get field_index -> field_name
                    let class_field_map = self.class_fields.get(&class_name);
                    let class_fields_ordered: Vec<(usize, String)> =
                        if let Some(field_map) = class_field_map {
                            let mut fields_vec: Vec<(usize, String)> = field_map
                                .iter()
                                .map(|(name, &idx)| (idx, name.clone()))
                                .collect();
                            fields_vec.sort_by_key(|(idx, _)| *idx);
                            fields_vec
                        } else {
                            // Fallback: use named fields order if class not found
                            fields
                                .iter()
                                .enumerate()
                                .map(|(idx, (name, _))| (idx, name.to_string()))
                                .collect()
                        };

                    // Evaluate all spread sources into temp locals
                    let spread_locals: Vec<(Local, usize)> = spreads
                        .iter()
                        .map(|spread| {
                            let spread_ty = body.ty(spread.expr).clone();
                            let spread_local = self.builder.temp(spread_ty);
                            self.lower_expr(spread.expr, Place::local(spread_local), body);
                            (spread_local, spread.position)
                        })
                        .collect();

                    // Build a map of named field name -> (value_expr_id, position)
                    // Position is the index in the fields vector
                    let named_field_map: HashMap<String, (ExprId, usize)> = fields
                        .iter()
                        .enumerate()
                        .map(|(pos, (name, value))| (name.to_string(), (*value, pos)))
                        .collect();

                    // For each class field, determine the source and generate operand
                    let field_operands: Vec<Operand> = class_fields_ordered
                        .iter()
                        .map(|(field_idx, field_name)| {
                            // Find the highest-positioned source for this field
                            let mut best_source: Option<FieldSource> = None;
                            let mut best_position: Option<usize> = None;

                            // Check named fields
                            if let Some(&(value_expr, pos)) = named_field_map.get(field_name) {
                                best_source = Some(FieldSource::Named(value_expr));
                                best_position = Some(pos);
                            }

                            // Check spreads (all spreads provide all fields of the class)
                            for (spread_local, spread_pos) in &spread_locals {
                                if best_position.is_none() || *spread_pos > best_position.unwrap() {
                                    best_source =
                                        Some(FieldSource::Spread(*spread_local, *field_idx));
                                    best_position = Some(*spread_pos);
                                }
                            }

                            // Generate the operand based on the source
                            match best_source {
                                Some(FieldSource::Named(value_expr)) => {
                                    self.lower_to_operand(value_expr, body)
                                }
                                Some(FieldSource::Spread(spread_local, field_idx)) => {
                                    // Load field from spread source
                                    Operand::Copy(Place::field(
                                        Place::local(spread_local),
                                        field_idx,
                                    ))
                                }
                                None => {
                                    // This shouldn't happen if the class has fields
                                    // Fall back to null
                                    Operand::Constant(Constant::Null)
                                }
                            }
                        })
                        .collect();

                    self.builder.assign(
                        dest,
                        Rvalue::Aggregate {
                            kind: AggregateKind::Class(class_name),
                            fields: field_operands,
                        },
                    );
                }
            }

            Expr::Map { entries } => {
                let entry_operands: Vec<(Operand, Operand)> = entries
                    .iter()
                    .map(|(k, v)| {
                        (
                            self.lower_to_operand(*k, body),
                            self.lower_to_operand(*v, body),
                        )
                    })
                    .collect();

                self.builder.assign(dest, Rvalue::Map(entry_operands));
            }

            // ========== Access ==========
            Expr::FieldAccess { base, field } => {
                let result_ty = body.ty(expr_id);

                // Check if this is a method reference (result type is a function)
                // vs an actual field access (result type is the field's type)
                if matches!(result_ty, baml_compiler_vir::Ty::Function { .. }) {
                    // Method reference - get resolution from VIR (computed in TIR)
                    let resolution = body.resolution(expr_id).unwrap_or_else(|| {
                        panic!("Missing resolution for method reference: {field}")
                    });

                    match resolution {
                        ResolvedValue::BuiltinFunction(qn) => {
                            self.builder.assign(
                                dest,
                                Rvalue::Use(Operand::Constant(Constant::Function(qn.clone()))),
                            );
                        }
                        ResolvedValue::Function(fqn) => {
                            self.builder.assign(
                                dest,
                                Rvalue::Use(Operand::Constant(Constant::Function(fqn.clone()))),
                            );
                        }
                        ResolvedValue::TypeMethod {
                            receiver_type,
                            method_name,
                        } => {
                            let qn = QualifiedName::builtin_method(
                                receiver_type.clone(),
                                method_name.clone(),
                            );
                            self.builder.assign(
                                dest,
                                Rvalue::Use(Operand::Constant(Constant::Function(qn))),
                            );
                        }
                        _ => {
                            panic!(
                                "Unexpected resolution for method reference {field}: {resolution:?}"
                            )
                        }
                    }
                } else {
                    // Actual field access
                    let base_ty = body.ty(*base).clone();
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
                let typed_ir_base_ty = body.ty(*base);
                let base_ty = typed_ir_base_ty.clone();
                let base_local = self.builder.temp(base_ty.clone());
                self.lower_expr(*base, Place::local(base_local), body);

                let index_local = self.builder.temp(Ty::Int);
                self.lower_expr(*index, Place::local(index_local), body);

                // Determine index kind based on base type
                let index_kind = if matches!(base_ty, Ty::Map { .. }) {
                    crate::IndexKind::Map
                } else {
                    crate::IndexKind::Array
                };

                self.builder.assign(
                    dest,
                    Rvalue::Use(Operand::Copy(Place::index(
                        Place::local(base_local),
                        index_local,
                        index_kind,
                    ))),
                );
            }

            Expr::Match {
                scrutinee,
                arms,
                is_exhaustive,
            } => {
                // Lower scrutinee - optimize for simple variable references
                let scrutinee_ty = body.ty(*scrutinee).clone();
                let scrutinee_local = if let Expr::Var(name) = body.expr(*scrutinee) {
                    // If scrutinee is a simple variable reference, reuse it directly
                    if let Some(&local) = self.locals.get(name) {
                        local
                    } else {
                        // Not a local variable (could be a function reference), use temp
                        let temp = self.builder.temp(scrutinee_ty.clone());
                        self.lower_expr(*scrutinee, Place::local(temp), body);
                        temp
                    }
                } else {
                    // Complex expression, lower to temp
                    let temp = self.builder.temp(scrutinee_ty.clone());
                    self.lower_expr(*scrutinee, Place::local(temp), body);
                    temp
                };

                // Create join block
                let join_block = self.builder.create_block();

                // Try switch optimization for integer or enum patterns
                // This enables jump table or binary search codegen
                if let Some((switch_kind, switch_values, wildcard_arm_idx)) =
                    self.try_extract_switch_arms(arms, body)
                {
                    self.lower_match_as_switch(
                        scrutinee_local,
                        &scrutinee_ty,
                        arms,
                        switch_kind,
                        switch_values,
                        wildcard_arm_idx,
                        *is_exhaustive,
                        join_block,
                        dest,
                        body,
                    );
                    return;
                }

                // Fall back to if-else chain lowering for complex patterns

                // For exhaustive matches, the last arm's failure path is unreachable.
                // We create a single unreachable block to use as that target, avoiding
                // the creation of an empty fallthrough block that would need to be emitted.
                let unreachable_block = if *is_exhaustive {
                    let saved_block = self.builder.current_block();
                    let block = self.builder.create_block();
                    self.builder.set_current_block(block);
                    self.builder.unreachable();
                    self.builder.set_current_block(saved_block);
                    Some(block)
                } else {
                    None
                };

                // For each arm, create test and body blocks
                let last_arm_idx = arms.len().saturating_sub(1);
                for (i, arm) in arms.iter().enumerate() {
                    let is_last_arm = i == last_arm_idx;
                    let arm_block = self.builder.create_block();

                    // For the last arm of an exhaustive match, pattern failure goes
                    // to the unreachable block. Otherwise, create a next_block.
                    let next_block = if is_last_arm && *is_exhaustive {
                        unreachable_block.expect("unreachable_block created for exhaustive match")
                    } else {
                        self.builder.create_block()
                    };

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

                    // Only set current block to next_block if it's not the unreachable block
                    if !(is_last_arm && *is_exhaustive) {
                        self.builder.set_current_block(next_block);
                    }
                }

                if !*is_exhaustive {
                    // Non-exhaustive match: fallthrough could be reached.
                    // This is typically a type error, but we generate valid MIR.
                    // TODO: Consider emitting a runtime panic instruction instead.
                    self.builder.goto(join_block);
                }
                self.builder.set_current_block(join_block);
            }

            Expr::NotifyBlock { name, level } => {
                // Set pending header for control flow visualization.
                // If an if/while follows, it will emit VizEnter/VizExit.
                self.pending_header = Some(PendingHeader {
                    name: name.to_string(),
                });

                // Emit a block notification statement
                self.builder.push_statement(
                    StatementKind::NotifyBlock {
                        name: name.clone(),
                        level: *level,
                    },
                    None,
                );
                // NotifyBlock returns unit
                self.builder
                    .assign(dest, Rvalue::Use(Operand::Constant(Constant::Null)));
            }
        }
    }

    /// Try to extract switch values from arms for switch optimization.
    ///
    /// Returns Some((kind, values, wildcard)) if all non-wildcard arms are switchable patterns
    /// (integer literals or enum variants from the same enum) without guards.
    /// Returns None if the match cannot be optimized as a switch.
    #[allow(clippy::type_complexity)]
    fn try_extract_switch_arms(
        &self,
        arms: &[baml_compiler_vir::MatchArm],
        body: &ExprBody,
    ) -> Option<(SwitchKind, Vec<(i64, usize)>, Option<usize>)> {
        // TypeTag switch requires at least 4 arms to benefit from jump table optimization.
        // For fewer arms, the InstanceOf if-else chain is more efficient since
        // extracting TypeTag has overhead.
        const MIN_TYPETAG_ARMS: usize = 4;

        let mut switch_arms = Vec::new();
        let mut wildcard_arm = None;
        let mut detected_kind: Option<SwitchKind> = None;

        for (i, arm) in arms.iter().enumerate() {
            // Guards prevent switch optimization (need runtime evaluation)
            if arm.guard.is_some() {
                return None;
            }

            let pat = body.pattern(arm.pattern);
            match pat {
                Pattern::Literal(Literal::Int(value)) => {
                    // Integer literal - ensure we're in integer mode
                    match &detected_kind {
                        None => detected_kind = Some(SwitchKind::Integer),
                        Some(SwitchKind::Integer) => {}
                        Some(SwitchKind::EnumDiscriminant(_) | SwitchKind::TypeTag) => return None, // Mixed types
                    }
                    switch_arms.push((*value, i));
                }
                Pattern::EnumVariant { enum_name, variant } => {
                    // Enum variant - lookup variant index and ensure same enum
                    let variant_idx = self.lookup_variant_index(enum_name, variant)?;
                    match &detected_kind {
                        None => {
                            detected_kind = Some(SwitchKind::EnumDiscriminant(enum_name.clone()));
                        }
                        Some(SwitchKind::EnumDiscriminant(name)) if name == enum_name => {}
                        _ => return None, // Mixed types or different enums
                    }
                    switch_arms.push((variant_idx, i));
                }
                Pattern::TypedBinding { name: _, ty } => {
                    // TypedBinding - lookup type tag for primitive or class types
                    let type_tag = self.type_tag_for_ty(ty)?;
                    match &detected_kind {
                        None => detected_kind = Some(SwitchKind::TypeTag),
                        Some(SwitchKind::TypeTag) => {}
                        _ => return None, // Mixed switch kinds
                    }
                    switch_arms.push((type_tag, i));
                }
                Pattern::Binding(name) => {
                    // Wildcard/binding pattern - must be the last arm
                    if name.as_str() == "_" || i == arms.len() - 1 {
                        if wildcard_arm.is_some() {
                            // Multiple wildcards - can't optimize
                            return None;
                        }
                        wildcard_arm = Some(i);
                    } else {
                        // Binding in the middle - can't optimize as pure switch
                        return None;
                    }
                }
                Pattern::Union(sub_pats) => {
                    // Union of patterns - extract each value
                    for sub_pat_id in sub_pats {
                        let sub_pat = body.pattern(*sub_pat_id);
                        match sub_pat {
                            Pattern::Literal(Literal::Int(value)) => {
                                match &detected_kind {
                                    None => detected_kind = Some(SwitchKind::Integer),
                                    Some(SwitchKind::Integer) => {}
                                    Some(SwitchKind::EnumDiscriminant(_) | SwitchKind::TypeTag) => {
                                        return None;
                                    }
                                }
                                switch_arms.push((*value, i));
                            }
                            Pattern::EnumVariant { enum_name, variant } => {
                                let variant_idx = self.lookup_variant_index(enum_name, variant)?;
                                match &detected_kind {
                                    None => {
                                        detected_kind =
                                            Some(SwitchKind::EnumDiscriminant(enum_name.clone()));
                                    }
                                    Some(SwitchKind::EnumDiscriminant(name))
                                        if name == enum_name => {}
                                    _ => return None,
                                }
                                switch_arms.push((variant_idx, i));
                            }
                            Pattern::TypedBinding { name: _, ty } => {
                                let type_tag = self.type_tag_for_ty(ty)?;
                                match &detected_kind {
                                    None => detected_kind = Some(SwitchKind::TypeTag),
                                    Some(SwitchKind::TypeTag) => {}
                                    _ => return None,
                                }
                                switch_arms.push((type_tag, i));
                            }
                            _ => return None, // Non-switchable in union
                        }
                    }
                }
                Pattern::Literal(_) => {
                    // Non-integer literals (strings, bools, null, etc.) - can't optimize as switch
                    return None;
                }
            }
        }

        // Need at least one arm and a detected kind for switch optimization
        let kind = detected_kind?;
        if switch_arms.is_empty() {
            return None;
        }

        // Deduplicate switch arms - union patterns like `A | A` can create duplicates.
        // We keep the first occurrence (earliest arm index) for each value.
        // This is correct because if a value appears multiple times, they all
        // map to the same arm anyway.
        let mut seen_values = std::collections::HashSet::new();
        switch_arms.retain(|(value, _)| seen_values.insert(*value));

        if matches!(kind, SwitchKind::TypeTag) && switch_arms.len() < MIN_TYPETAG_ARMS {
            return None;
        }

        Some((kind, switch_arms, wildcard_arm))
    }

    /// Look up the index of an enum variant.
    ///
    /// Returns the 0-based index of the variant within the enum definition.
    #[allow(clippy::cast_possible_wrap)]
    fn lookup_variant_index(&self, enum_name: &Name, variant: &Name) -> Option<i64> {
        let variants = self.enum_variants.get(enum_name.as_str())?;
        let index = *variants.get(variant.as_str())?;
        Some(index as i64)
    }

    /// Get the type tag for a type (primitive or class).
    ///
    /// Returns the type tag for primitives (using `baml_type::typetag` constants)
    /// or for classes (using the pre-computed `class_type_tags` map).
    ///
    /// # `TypeAlias` handling
    ///
    /// `TypeAliases` are looked up by their alias name in `class_type_tags`.
    /// This has limitations:
    /// - `TypeAliases` to primitives won't be found and will return `None`
    /// - `TypeAliases` to classes will only work if the alias name is registered
    ///
    /// When `None` is returned, the pattern falls back to non-switch optimization,
    /// which is safe but may miss optimization opportunities. Full `TypeAlias`
    /// resolution would require resolving through potentially recursive aliases,
    /// which is not yet implemented.
    fn type_tag_for_ty(&self, ty: &Ty) -> Option<i64> {
        match ty {
            Ty::Int => Some(baml_type::typetag::INT),
            Ty::String => Some(baml_type::typetag::STRING),
            Ty::Bool => Some(baml_type::typetag::BOOL),
            Ty::Null => Some(baml_type::typetag::NULL),
            Ty::Float => Some(baml_type::typetag::FLOAT),
            Ty::Class(tn) => self.class_type_tags.get(tn.name.as_str()).copied(),
            // TypeAliases: look up by alias name. See doc comment for limitations.
            Ty::TypeAlias(tn) => self.class_type_tags.get(tn.name.as_str()).copied(),
            // Literal types map to the same tag as their base type
            Ty::Literal(baml_base::Literal::Int(_)) => Some(baml_type::typetag::INT),
            Ty::Literal(baml_base::Literal::Float(_)) => Some(baml_type::typetag::FLOAT),
            Ty::Literal(baml_base::Literal::String(_)) => Some(baml_type::typetag::STRING),
            Ty::Literal(baml_base::Literal::Bool(_)) => Some(baml_type::typetag::BOOL),
            _ => None, // Not a type with a known tag
        }
    }

    /// Lower a match expression as a Switch terminator.
    ///
    /// This emits a single Switch instruction with all integer or enum variant arms,
    /// enabling jump table or binary search optimization in the codegen.
    ///
    /// For enum variants, emits a `Discriminant` instruction first to extract the
    /// variant index before the switch.
    ///
    /// If `is_exhaustive` is true and there's no wildcard arm, the switch is marked
    /// as exhaustive, allowing the codegen to skip the last arm's comparison.
    #[allow(clippy::too_many_arguments, clippy::needless_pass_by_value)]
    fn lower_match_as_switch(
        &mut self,
        scrutinee_local: Local,
        scrutinee_ty: &Ty,
        arms: &[baml_compiler_vir::MatchArm],
        switch_kind: SwitchKind,
        switch_values: Vec<(i64, usize)>,
        wildcard_arm_idx: Option<usize>,
        is_exhaustive: bool,
        join_block: BlockId,
        dest: Place,
        body: &ExprBody,
    ) {
        // Extract switch discriminant based on kind
        let switch_discriminant = match switch_kind {
            SwitchKind::Integer => {
                // Direct integer comparison - use scrutinee directly
                Operand::copy_local(scrutinee_local)
            }
            SwitchKind::EnumDiscriminant(_) => {
                // Emit Discriminant to extract variant index
                let discriminant_local = self.builder.temp(Ty::Int);
                self.builder.assign(
                    Place::local(discriminant_local),
                    Rvalue::Discriminant(Place::local(scrutinee_local)),
                );
                Operand::copy_local(discriminant_local)
            }
            SwitchKind::TypeTag => {
                // Emit TypeTag to extract runtime type tag
                let type_tag_local = self.builder.temp(Ty::Int);
                self.builder.assign(
                    Place::local(type_tag_local),
                    Rvalue::TypeTag(Place::local(scrutinee_local)),
                );
                Operand::copy_local(type_tag_local)
            }
        };

        // Create body blocks for each arm
        let arm_blocks: Vec<BlockId> = arms.iter().map(|_| self.builder.create_block()).collect();

        // Create otherwise block (for values not in switch arms)
        let otherwise_block = if let Some(idx) = wildcard_arm_idx {
            arm_blocks[idx]
        } else {
            // No wildcard - create unreachable block
            let saved_block = self.builder.current_block();
            let block = self.builder.create_block();
            self.builder.set_current_block(block);
            self.builder.unreachable();
            self.builder.set_current_block(saved_block);
            block
        };

        // Build switch arms: (value, target block)
        let switch_arms: Vec<(i64, BlockId)> = switch_values
            .iter()
            .map(|(value, arm_idx)| (*value, arm_blocks[*arm_idx]))
            .collect();

        // Switch is exhaustive when all values are explicitly enumerated (no wildcard)
        // AND the type checker marked the match as exhaustive.
        // This allows codegen to skip the last arm's comparison.
        let exhaustive = wildcard_arm_idx.is_none() && is_exhaustive;

        // Emit the switch terminator
        self.builder.switch(
            switch_discriminant,
            switch_arms,
            otherwise_block,
            exhaustive,
        );

        // Lower each arm's body
        for (i, arm) in arms.iter().enumerate() {
            self.builder.set_current_block(arm_blocks[i]);

            // If this is a binding or typed binding arm, bind the variable
            let pat = body.pattern(arm.pattern);
            match pat {
                Pattern::Binding(name) if name.as_str() != "_" => {
                    let local = self.builder.declare_local(
                        Some(name.clone()),
                        scrutinee_ty.clone(),
                        None,
                        false,
                    );
                    self.builder.assign(
                        Place::local(local),
                        Rvalue::Use(Operand::copy_local(scrutinee_local)),
                    );
                    self.locals.insert(name.clone(), local);
                }
                Pattern::TypedBinding { name, ty } => {
                    // Typed binding - bind the variable with its specific type.
                    // `_` is a discard binding and must not be materialized.
                    if name.as_str() != "_" {
                        let pattern_ty = ty.clone();
                        let local =
                            self.builder
                                .declare_local(Some(name.clone()), pattern_ty, None, false);
                        self.builder.assign(
                            Place::local(local),
                            Rvalue::Use(Operand::copy_local(scrutinee_local)),
                        );
                        self.locals.insert(name.clone(), local);
                    }
                }
                _ => {
                    // No binding needed (e.g., literal patterns, enum variants, wildcards)
                }
            }

            // Lower the arm body
            self.lower_expr(arm.body, dest.clone(), body);
            self.builder.goto(join_block);
        }

        self.builder.set_current_block(join_block);
    }

    /// Lower a pattern match test, branching to `success_block` if the pattern matches,
    /// or `fail_block` if it doesn't.
    fn lower_pattern_test(
        &mut self,
        pat_id: PatId,
        scrutinee_local: Local,
        scrutinee_ty: &Ty,
        success_block: BlockId,
        fail_block: BlockId,
        body: &ExprBody,
    ) {
        let pat = body.pattern(pat_id);
        match pat {
            Pattern::Binding(name) => {
                // Binding always matches. `_` is a discard binding and must not
                // be materialized as a usable local.
                if name.as_str() != "_" {
                    let local = self.builder.declare_local(
                        Some(name.clone()),
                        scrutinee_ty.clone(),
                        None,
                        false,
                    );
                    self.builder.assign(
                        Place::local(local),
                        Rvalue::Use(Operand::copy_local(scrutinee_local)),
                    );
                    self.locals.insert(name.clone(), local);
                }
                self.builder.goto(success_block);
            }
            Pattern::TypedBinding { name, ty } => {
                // TypedBinding checks if scrutinee is an instance of the given type
                // Convert VIR type to TIR type for IsType check
                let pattern_ty = ty.clone();

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

                // In bind block: bind the variable and go to success.
                // `_` is a discard binding and must not be materialized.
                self.builder.set_current_block(bind_block);
                if name.as_str() != "_" {
                    let local =
                        self.builder
                            .declare_local(Some(name.clone()), pattern_ty, None, false);
                    self.builder.assign(
                        Place::local(local),
                        Rvalue::Use(Operand::copy_local(scrutinee_local)),
                    );
                    self.locals.insert(name.clone(), local);
                }
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
                let enum_qn = QualifiedName::local(enum_name.clone());
                let variant_const = Constant::EnumVariant {
                    enum_qn,
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
    fn lower_literal(lit: &Literal) -> Constant {
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
        // Special case: instanceof operator - RHS is a type name, not a value
        if op == BinaryOp::Instanceof {
            let lhs_operand = self.lower_to_operand(lhs, body);

            // Extract the type name from RHS (should be a Var or single-segment Path)
            let type_name = match body.expr(rhs) {
                Expr::Var(name) => name.clone(),
                Expr::Path(segments) if segments.len() == 1 => segments[0].clone(),
                _ => panic!("instanceof RHS must be a simple type name"),
            };

            self.builder.assign(
                dest,
                Rvalue::IsType {
                    operand: lhs_operand,
                    ty: Ty::TypeAlias(TypeName::local(type_name)),
                },
            );
            return;
        }

        let lhs_operand = self.lower_to_operand(lhs, body);
        let rhs_operand = self.lower_to_operand(rhs, body);

        self.builder.assign(
            dest,
            Rvalue::BinaryOp {
                op: Self::convert_binop(op),
                left: lhs_operand,
                right: rhs_operand,
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
            BinaryOp::Instanceof => BinOp::Instanceof,
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
        // Check if preceded by a header - if so, emit VizEnter for BranchGroup
        let viz_idx = if let Some(header) = self.pending_header.take() {
            Some(self.viz_enter(VizNodeType::BranchGroup, &header.name))
        } else {
            None
        };

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

        // Join block - emit VizExit if we had a header
        self.builder.set_current_block(bb_join);
        if let Some(idx) = viz_idx {
            self.viz_exit(idx);
        }
    }

    /// Lower a while loop.
    fn lower_while(&mut self, condition: ExprId, loop_body: ExprId, body: &ExprBody) {
        // Check if preceded by a header - if so, emit VizEnter for Loop
        let viz_idx = if let Some(header) = self.pending_header.take() {
            Some(self.viz_enter(VizNodeType::Loop, &header.name))
        } else {
            None
        };

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
            watched_locals_depth: self.watched_locals_stack.len(),
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

        // Exit block - emit VizExit if we had a header
        self.builder.set_current_block(bb_exit);
        if let Some(idx) = viz_idx {
            self.viz_exit(idx);
        }
    }

    /// Lower a function call.
    fn lower_call(
        &mut self,
        callee: ExprId,
        args: &[ExprId],
        dest: Place,
        body: &ExprBody,
        _result_ty: &baml_compiler_vir::Ty,
    ) {
        let callee_expr = body.expr(callee);

        // Check if this is a $watch method call (e.g., value.$watch.options(filter))
        if let Expr::FieldAccess { base, field } = callee_expr {
            let base_ty = body.ty(*base);
            if let baml_compiler_vir::Ty::WatchAccessor(_) = base_ty {
                // This is a $watch method call
                // The base expression is var.$watch, so we need to get the var
                let watch_accessor_expr = body.expr(*base);
                if let Expr::FieldAccess {
                    base: watched_var_base,
                    field: watch_field,
                } = watch_accessor_expr
                {
                    if watch_field.as_str() == "$watch" {
                        match field.as_str() {
                            "options" => {
                                // $watch.options(filter) - emit WatchOptions statement
                                // First, find the local variable for the watched variable
                                if let Expr::Var(var_name) = body.expr(*watched_var_base) {
                                    let local =
                                        *self.locals.get(var_name).expect("variable not found");

                                    // Evaluate the filter argument
                                    if !args.is_empty() {
                                        let filter_arg = args[0];
                                        // We need to extract the 'when' field if it's a struct
                                        // For now, let's check if it's an Object with 'when' field
                                        let filter_expr = body.expr(filter_arg);
                                        if let Expr::Object { fields, .. } = filter_expr {
                                            // Look for 'when' field
                                            for (field_name, field_expr_id) in fields {
                                                if field_name.as_str() == "when" {
                                                    let filter_operand =
                                                        self.lower_to_operand(*field_expr_id, body);
                                                    self.builder
                                                        .watch_options(local, filter_operand);
                                                    break;
                                                }
                                            }
                                        } else {
                                            // Direct filter value (function or string)
                                            let filter_operand =
                                                self.lower_to_operand(filter_arg, body);
                                            self.builder.watch_options(local, filter_operand);
                                        }
                                    }
                                    // Assign null to dest (options returns void)
                                    self.builder.assign(
                                        dest,
                                        Rvalue::Use(Operand::Constant(Constant::Null)),
                                    );
                                    return;
                                }
                            }
                            "notify" => {
                                // $watch.notify() - emit WatchNotify statement
                                if let Expr::Var(var_name) = body.expr(*watched_var_base) {
                                    let local =
                                        *self.locals.get(var_name).expect("variable not found");
                                    self.builder.watch_notify(local);
                                    // Assign null to dest (notify returns void)
                                    self.builder.assign(
                                        dest,
                                        Rvalue::Use(Operand::Constant(Constant::Null)),
                                    );
                                    return;
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        // Note: Builtin method resolution is now handled in VIR lowering.
        // The lookup_method call that was here expected TIR types, but VIR now uses
        // baml_type::Ty. If VIR didn't resolve a builtin method, we fall through
        // to regular call handling below.

        // Check if this is a method call (callee is FieldAccess)
        // For method calls like `obj.method()`, we need to pass `obj` as the first argument (self)
        if let Expr::FieldAccess { base, field } = callee_expr {
            let base_ty = body.ty(*base);

            // Get the type name for method path construction
            // Works for both builtin class types and primitive types with methods
            let type_name = match &base_ty {
                Ty::Class(tn) if !tn.module_path.is_empty() => Some(tn.display_name.to_string()),
                Ty::Opaque(tn) => Some(tn.display_name.to_string()),
                Ty::List(_) => Some("baml.Array".to_string()),
                Ty::String => Some("baml.String".to_string()),
                Ty::Map { .. } => Some("baml.Map".to_string()),
                Ty::Class(_) => Self::class_name_from_ty(base_ty),
                _ => None,
            };

            if let Some(type_name) = type_name {
                // This is a method call - pass receiver as first argument
                // Must lower receiver before callee to preserve evaluation order
                let mut all_args = vec![self.lower_to_operand(*base, body)];
                all_args.extend(self.lower_args(args, body));
                let callee_operand = self.lower_to_operand(callee, body);

                // Check if this method is a SysOp builtin (e.g., baml.llm.PrimitiveClient.render_prompt)
                // Build the full path using the type name (not variable name) + method name
                let method_path = format!("{type_name}.{field}");
                let is_sys_op = baml_compiler_tir::builtins::lookup_builtin_by_path(&method_path)
                    .map(|def| def.is_sys_op)
                    .unwrap_or(false);

                if is_sys_op {
                    self.emit_sys_op_call(callee_operand, all_args, dest);
                } else {
                    self.emit_call(callee_operand, all_args, dest);
                }
                return;
            }
        }

        // Regular function call (not a method)
        // Check if this is an external builtin function (by path)
        let is_sys_op = Self::is_sys_op_builtin_path(callee_expr, body);
        let callee_operand = self.lower_to_operand(callee, body);
        let arg_operands = self.lower_args(args, body);

        if is_sys_op {
            self.emit_sys_op_call(callee_operand, arg_operands, dest);
        } else {
            self.emit_call(callee_operand, arg_operands, dest);
        }
    }

    /// Check if a callee expression refers to a `sys_op` builtin function.
    fn is_sys_op_builtin_path(callee_expr: &Expr, body: &ExprBody) -> bool {
        // Extract the path from the callee expression
        let path = match callee_expr {
            Expr::Var(name) => name.to_string(),
            Expr::Path(segments) => segments
                .iter()
                .map(smol_str::SmolStr::as_str)
                .collect::<Vec<_>>()
                .join("."),
            Expr::FieldAccess { .. } => {
                // Recursively build the path from nested field accesses
                fn build_path(expr: &Expr, body: &ExprBody) -> Option<String> {
                    match expr {
                        Expr::Var(name) => Some(name.to_string()),
                        Expr::Path(segments) => Some(
                            segments
                                .iter()
                                .map(smol_str::SmolStr::as_str)
                                .collect::<Vec<_>>()
                                .join("."),
                        ),
                        Expr::FieldAccess { base, field } => {
                            let base_path = build_path(body.expr(*base), body)?;
                            Some(format!("{base_path}.{field}"))
                        }
                        _ => None,
                    }
                }
                match build_path(callee_expr, body) {
                    Some(p) => p,
                    None => return false,
                }
            }
            _ => return false,
        };

        // Look up the builtin by path and check if it's a sys_op
        baml_compiler_tir::builtins::lookup_builtin_by_path(&path)
            .map(|def| def.is_sys_op)
            .unwrap_or(false)
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
                let base_ty = body.ty(*base).clone();
                let field_idx = self.field_index_for_type_and_name(&base_ty, field);
                Place::field(base_place, field_idx)
            }

            Expr::Index { base, index } => {
                let base_place = self.lower_lvalue(*base, body);
                let base_ty = body.ty(*base).clone();
                let index_local = self.builder.temp(Ty::Int);
                self.lower_expr(*index, Place::local(index_local), body);

                // Determine index kind based on base type
                let index_kind = if matches!(base_ty, Ty::Map { .. }) {
                    crate::IndexKind::Map
                } else {
                    crate::IndexKind::Array
                };

                Place::index(base_place, index_local, index_kind)
            }

            Expr::Call { callee, args } => {
                // For method calls used as lvalue bases (e.g., `obj.get_field().value = x`),
                // evaluate the call and store the result in a temp, then use that as base.
                // This works because objects have reference semantics in the VM.
                let call_ty = body.ty(expr_id);
                let mir_ty = call_ty.clone();
                let temp = self.builder.temp(mir_ty);
                self.lower_call(*callee, args, Place::local(temp), body, call_ty);
                Place::local(temp)
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
    fn field_index_for_type_and_name(&self, ty: &Ty, field: &Name) -> usize {
        let class_name = Self::class_name_from_ty(ty);

        if let Some(ref class_name) = class_name {
            if let Some(fields) = self.class_fields.get(class_name) {
                if let Some(&idx) = fields.get(&field.to_string()) {
                    return idx;
                }
                log::error!(
                    "BUG: Field `{}` not found in class `{}`. Available fields: {:?}",
                    field,
                    class_name,
                    fields.keys().collect::<Vec<_>>()
                );
            }
            log::error!(
                "BUG: Class `{}` not found in class_fields map. Available classes: {:?}",
                class_name,
                self.class_fields.keys().collect::<Vec<_>>()
            );
        }

        log::error!("BUG: Cannot extract class name from type {ty:?} for field access `{field}`");
        0
    }

    /// Extract class name from a Ty.
    ///
    /// For builtin classes, returns the full path (e.g., "baml.http.Response").
    /// For user classes, returns just the name (e.g., "`MyClass`").
    fn class_name_from_ty(ty: &Ty) -> Option<String> {
        match ty {
            Ty::TypeAlias(tn) | Ty::Class(tn) => {
                // Use display_name which is pre-computed with the full path for builtins
                Some(tn.display_name.to_string())
            }
            _ => None,
        }
    }

    /// Lower an expression to a temporary and return an operand that copies it.
    ///
    /// This is a convenience method for the common pattern of:
    /// 1. Creating a temporary local with the expression's type
    /// 2. Lowering the expression into that temporary
    /// 3. Creating an operand that copies from the temporary
    fn lower_to_operand(&mut self, expr: ExprId, body: &ExprBody) -> Operand {
        let ty = body.ty(expr).clone();
        let local = self.builder.temp(ty);
        self.lower_expr(expr, Place::local(local), body);
        Operand::copy_local(local)
    }

    /// Lower a slice of expressions to operands.
    fn lower_args(&mut self, args: &[ExprId], body: &ExprBody) -> Vec<Operand> {
        args.iter()
            .map(|&a| self.lower_to_operand(a, body))
            .collect()
    }

    /// Emit a function call with automatic continue block handling.
    fn emit_call(&mut self, callee: Operand, args: Vec<Operand>, dest: Place) {
        let continue_block = self.builder.create_block();
        self.builder.call(callee, args, dest, continue_block, None);
        self.builder.set_current_block(continue_block);
    }

    /// Emit an external (async) function call using `DispatchFuture` + Await.
    ///
    /// External functions are implemented outside the VM (by the embedder).
    /// This emits:
    /// 1. `DispatchFuture`: Start the async operation, get a future handle
    /// 2. Await: Wait for the future and retrieve the result
    fn emit_sys_op_call(&mut self, callee: Operand, args: Vec<Operand>, dest: Place) {
        // Create a temp to hold the future handle
        // Future handles are opaque to the VM - we use Null type
        let future_local = self.builder.temp(Ty::Null);
        let future_place = Place::local(future_local);

        // Create blocks for the dispatch and await
        let await_block = self.builder.create_block();
        let continue_block = self.builder.create_block();

        // Emit DispatchFuture: starts async op, stores future handle, resumes at await_block
        self.builder
            .dispatch_future(callee, args, future_place.clone(), await_block);

        // In await_block: wait for the future and store result in dest
        self.builder.set_current_block(await_block);
        self.builder
            .await_(future_place, dest, continue_block, None);

        // Continue execution after await completes
        self.builder.set_current_block(continue_block);
    }
}
