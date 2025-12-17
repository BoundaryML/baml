//! THIR to bytecode compiler.
//!
//! This module compiles the Typed High-level IR into VM bytecode.
//! It takes the type-checked `InferenceResult` from THIR along with
//! the expression body from HIR.

use std::collections::{HashMap, HashSet};

use baml_base::Name;
use baml_hir::{
    AssignOp, BinaryOp, Expr, ExprBody, ExprId, FunctionBody, FunctionSignature, Literal, Pattern,
    StmtId, UnaryOp,
};
use baml_thir::{InferenceResult, Ty};
use baml_vm::{
    BinOp, Bytecode, CmpOp, Function, FunctionKind, GlobalIndex, Instruction, Object, ObjectIndex,
    ObjectPool, Value,
};

/// Context for compiling functions to bytecode.
///
/// Contains all the shared state needed during compilation:
/// type inference results, global mappings, class information, and the shared object pool.
pub struct CodegenContext<'db, 'ctx, 'obj> {
    /// Type inference result from THIR.
    pub inference: &'db InferenceResult<'db>,
    /// Resolved global names to indices.
    pub globals: &'ctx HashMap<String, usize>,
    /// Resolved class field indices (class name -> field name -> field index).
    pub classes: &'ctx HashMap<String, HashMap<String, usize>>,
    /// Pre-allocated Class object indices in the program's object pool.
    pub class_object_indices: &'ctx HashMap<String, usize>,
    /// Shared object pool for strings, etc.
    /// Objects are added directly here with correct indices, eliminating remapping.
    pub objects: &'obj mut ObjectPool,
}

/// Block scope for tracking local variables.
#[derive(Debug, Default)]
struct Scope {
    /// Scope depth (0 is function body).
    depth: usize,
    /// Variables declared in this scope only.
    locals: HashSet<String>,
    /// Scope ID for debug info.
    id: usize,
}

/// Information about the current loop for break/continue handling.
#[derive(Debug)]
struct LoopInfo {
    /// Length of scopes vec before entering loop body.
    /// Used by break/continue to know how many scopes to pop.
    scope_depth: usize,
    /// Jump instruction locations to patch for break statements.
    break_patch_list: Vec<usize>,
    /// Jump instruction locations to patch for continue statements.
    continue_patch_list: Vec<usize>,
}

/// Compiler state for generating bytecode from THIR.
pub struct Compiler<'db, 'ctx, 'obj> {
    /// Type inference result from THIR.
    inference: &'db InferenceResult<'db>,

    /// Resolved global names to indices.
    globals: &'ctx HashMap<String, usize>,

    /// Resolved class field indices (class name -> field name -> field index).
    classes: &'ctx HashMap<String, HashMap<String, usize>>,

    /// Pre-allocated Class object indices in the program's object pool.
    class_object_indices: &'ctx HashMap<String, usize>,

    /// Local variable names to stack indices.
    locals: HashMap<String, usize>,

    /// Scopes for tracking local variable lifetimes.
    scopes: Vec<Scope>,

    /// Locals in scope per scope ID (debug info).
    locals_in_scope: Vec<HashMap<String, usize>>,

    /// Current source line (for debugging).
    current_source_line: usize,

    /// Bytecode being generated.
    bytecode: Bytecode,

    /// Shared objects pool (for strings, etc. - NOT classes, those are pre-allocated).
    objects: &'obj mut ObjectPool,

    /// Current loop info for break/continue handling.
    current_loop: Option<LoopInfo>,
}

impl<'db, 'ctx, 'obj> Compiler<'db, 'ctx, 'obj> {
    /// Create a new compiler with the given codegen context.
    // CodegenContext contains `&mut ObjectPool` which must be moved, not borrowed.
    #[allow(clippy::needless_pass_by_value)]
    pub fn new(ctx: CodegenContext<'db, 'ctx, 'obj>) -> Self {
        Self {
            inference: ctx.inference,
            globals: ctx.globals,
            classes: ctx.classes,
            class_object_indices: ctx.class_object_indices,
            locals: HashMap::new(),
            scopes: Vec::new(),
            locals_in_scope: Vec::new(),
            current_source_line: 0,
            bytecode: Bytecode::new(),
            objects: ctx.objects,
            current_loop: None,
        }
    }

    /// Get the type of an expression from the inference result.
    fn expr_type(&self, expr_id: ExprId) -> Option<&Ty<'db>> {
        self.inference.expr_types.get(&expr_id)
    }

    /// Extract the class name from a type, if it's a class type.
    fn class_name_from_ty(ty: &Ty<'db>) -> Option<String> {
        match ty {
            Ty::Named(name) => Some(name.to_string()),
            Ty::Class(class_id) => {
                // For resolved class types, we'd need to look up the name
                // For now, fall back to debug representation
                Some(format!("{class_id:?}"))
            }
            _ => None,
        }
    }

    /// Compile a function from its THIR-typed body.
    pub fn compile_function(
        &mut self,
        signature: &FunctionSignature,
        body: &FunctionBody,
    ) -> Function {
        // Reset state for this function
        self.locals.clear();
        self.scopes.clear();
        self.locals_in_scope.clear();
        self.bytecode = Bytecode::new();

        let name = signature.name.as_str();
        let params: Vec<Name> = signature.params.iter().map(|p| p.name.clone()).collect();

        match body {
            FunctionBody::Expr(expr_body) => self.compile_expr_function(name, &params, expr_body),
            FunctionBody::Llm(_) => {
                // LLM functions have no bytecode to compile
                Function {
                    name: name.to_string(),
                    arity: params.len(),
                    bytecode: Bytecode::new(),
                    kind: FunctionKind::Llm,
                    locals_in_scope: vec![
                        params
                            .iter()
                            .map(std::string::ToString::to_string)
                            .collect(),
                    ],
                    span: baml_base::Span::fake(),
                    block_notifications: Vec::new(),
                }
            }
            FunctionBody::Missing => {
                // TODO: cannot compile function with missing body: {name}
                // Return an empty function as a placeholder
                Function {
                    name: name.to_string(),
                    arity: params.len(),
                    bytecode: Bytecode::new(),
                    kind: FunctionKind::Exec,
                    locals_in_scope: vec![
                        params
                            .iter()
                            .map(std::string::ToString::to_string)
                            .collect(),
                    ],
                    span: baml_base::Span::fake(),
                    block_notifications: Vec::new(),
                }
            }
        }
    }

    fn compile_expr_function(&mut self, name: &str, params: &[Name], body: &ExprBody) -> Function {
        self.enter_scope();

        // Register parameters as locals
        for param in params {
            self.track_local(param.as_ref());
        }

        // Compile the root expression (usually a block)
        if let Some(root_expr) = body.root_expr {
            self.compile_expr(root_expr, body);
        }

        // Emit return at end of function body
        self.emit(Instruction::Return);

        self.exit_scope(false);

        Function {
            name: name.to_string(),
            arity: params.len(),
            bytecode: self.bytecode.clone(),
            kind: FunctionKind::Exec,
            locals_in_scope: self
                .locals_in_scope
                .iter()
                .map(|locals| {
                    let mut names = Vec::with_capacity(locals.len() + 1);
                    // Function reference is at stack position 0
                    names.push(format!("<fn {name}>"));
                    names.resize_with(names.capacity(), String::new);
                    for (var_name, index) in locals {
                        if *index < names.len() {
                            names[*index].clone_from(var_name);
                        }
                    }
                    names
                })
                .collect(),
            span: baml_base::Span::fake(),
            block_notifications: Vec::new(),
        }
    }

    /// Check if an expression produces a value on the stack.
    ///
    /// Most expressions produce values, but some don't:
    /// - If without else: never produces a value
    /// - If with else: produces a value only if BOTH branches produce values
    /// - Block without tail expression: never produces a value
    /// - Block with tail expression: produces a value if tail produces a value
    fn expr_produces_value(expr_id: ExprId, body: &ExprBody) -> bool {
        match &body.exprs[expr_id] {
            Expr::If {
                then_branch,
                else_branch,
                ..
            } => {
                // If-without-else never produces a value
                let Some(else_expr) = else_branch else {
                    return false;
                };
                // If-with-else produces a value only if both branches do
                Self::expr_produces_value(*then_branch, body)
                    && Self::expr_produces_value(*else_expr, body)
            }
            Expr::Block { tail_expr, .. } => {
                // Block produces a value only if it has a tail that produces a value
                tail_expr
                    .map(|tail| Self::expr_produces_value(tail, body))
                    .unwrap_or(false)
            }
            _ => true,
        }
    }

    /// Compile an expression and emit bytecode.
    ///
    /// The expression's type is available via `self.expr_type(expr_id)`.
    fn compile_expr(&mut self, expr_id: ExprId, body: &ExprBody) {
        use baml_hir::Expr;

        let expr = &body.exprs[expr_id];

        // Type information is available for code generation decisions
        let _ty = self.expr_type(expr_id);

        match expr {
            Expr::Literal(lit) => self.compile_literal(lit),

            Expr::Path(segments) => {
                if segments.is_empty() {
                    // TODO: Error case - empty path should not reach codegen,
                    // should be caught during parsing or type checking
                } else if segments.len() >= 2 {
                    // Multi-segment path: could be a builtin function or variable + fields
                    // First, check if the full path is a global (e.g., "baml.Array.length")
                    let full_path = segments
                        .iter()
                        .map(std::string::ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(".");
                    if let Some(&index) = self.globals.get(&full_path) {
                        // It's a builtin function - load it directly
                        self.emit(Instruction::LoadGlobal(GlobalIndex::from_raw(index)));
                    } else {
                        // Treat as variable + field accesses
                        let first_name = segments[0].to_string();
                        if let Some(&index) = self.locals.get(&first_name) {
                            self.emit(Instruction::LoadVar(index));
                        } else if let Some(&index) = self.globals.get(&first_name) {
                            self.emit(Instruction::LoadGlobal(GlobalIndex::from_raw(index)));
                        } else {
                            panic!(
                                "unknown variable or function: '{}' (not in locals {:?} or globals {:?})",
                                first_name,
                                self.locals.keys().collect::<Vec<_>>(),
                                self.globals.keys().collect::<Vec<_>>()
                            );
                        }

                        // Get segment types computed during type inference
                        let segment_types = self.inference.path_segment_types.get(&expr_id);

                        for (i, field) in segments[1..].iter().enumerate() {
                            let field_name = field.to_string();

                            // Get the type of the object we're accessing the field on
                            let field_index = segment_types
                                .and_then(|types| types.get(i))
                                .and_then(Self::class_name_from_ty)
                                .and_then(|class_name| self.classes.get(&class_name))
                                .and_then(|fields| fields.get(&field_name))
                                .copied()
                                .unwrap_or(0); // Default to 0 if not found (error case)

                            self.emit(Instruction::LoadField(field_index));
                        }
                    }
                } else {
                    // Single segment: simple variable or function lookup
                    let first_name = segments[0].to_string();
                    if let Some(&index) = self.locals.get(&first_name) {
                        self.emit(Instruction::LoadVar(index));
                    } else if let Some(&index) = self.globals.get(&first_name) {
                        self.emit(Instruction::LoadGlobal(GlobalIndex::from_raw(index)));
                    } else {
                        panic!(
                            "unknown variable or function: '{}' (not in locals {:?} or globals {:?})",
                            first_name,
                            self.locals.keys().collect::<Vec<_>>(),
                            self.globals.keys().collect::<Vec<_>>()
                        );
                    }
                }
            }

            Expr::Binary { op, lhs, rhs } => {
                // Handle short-circuit operators specially
                match op {
                    BinaryOp::And => {
                        self.compile_expr(*lhs, body);
                        let skip_right = self.emit(Instruction::JumpIfFalse(0));
                        self.emit(Instruction::Pop(1));
                        self.compile_expr(*rhs, body);
                        self.patch_jump(skip_right);
                    }
                    BinaryOp::Or => {
                        self.compile_expr(*lhs, body);
                        let eval_right = self.emit(Instruction::JumpIfFalse(0));
                        let skip_right = self.emit(Instruction::Jump(0));
                        self.patch_jump(eval_right);
                        self.emit(Instruction::Pop(1));
                        self.compile_expr(*rhs, body);
                        self.patch_jump(skip_right);
                    }
                    _ => {
                        self.compile_expr(*lhs, body);
                        self.compile_expr(*rhs, body);
                        self.emit(Self::binary_op_instruction(*op));
                    }
                }
            }

            Expr::Unary { op, expr } => {
                self.compile_expr(*expr, body);
                self.emit(Self::unary_op_instruction(*op));
            }

            Expr::Call { callee, args } => {
                // Check if this is a method call (callee is FieldAccess)
                if let Expr::FieldAccess { base, field } = &body.exprs[*callee] {
                    // Method call: receiver.method(args) -> builtin(receiver, args)
                    // Get the type of the receiver to look up the builtin method
                    if let Some(receiver_ty) = self.expr_type(*base) {
                        if let Some((def, _)) =
                            baml_thir::builtins::lookup_method(receiver_ty, field.as_str())
                        {
                            // Found a builtin method - compile as function call
                            if let Some(&global_idx) = self.globals.get(def.path) {
                                // Emit: LOAD_GLOBAL method
                                self.emit(Instruction::LoadGlobal(GlobalIndex::from_raw(
                                    global_idx,
                                )));
                                // Emit: compile receiver (first argument)
                                self.compile_expr(*base, body);
                                // Emit: compile explicit arguments
                                for arg in args {
                                    self.compile_expr(*arg, body);
                                }
                                // Emit: CALL with receiver + explicit args
                                self.emit(Instruction::Call(args.len() + 1));
                                return;
                            }
                        }
                    }
                }

                // Regular function call (not a method call)
                self.compile_expr(*callee, body);
                for arg in args {
                    self.compile_expr(*arg, body);
                }
                self.emit(Instruction::Call(args.len()));
            }

            Expr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                // Compile condition - leaves result on stack
                self.compile_expr(*condition, body);

                // Skip the if branch when condition is false
                let skip_if = self.emit(Instruction::JumpIfFalse(0));

                // Pop condition (true path)
                self.emit(Instruction::Pop(1));

                // Compile the if branch
                self.compile_expr(*then_branch, body);

                // Skip the else branch (or just the false-path pop if no else)
                let skip_else = self.emit(Instruction::Jump(0));

                // Patch skip_if to jump here (false path)
                self.patch_jump(skip_if);

                // Pop condition (false path)
                self.emit(Instruction::Pop(1));

                // Compile else branch if it exists
                if let Some(else_expr) = else_branch {
                    self.compile_expr(*else_expr, body);
                }

                // Patch skip_else - if no else, this just skips the false-path pop
                self.patch_jump(skip_else);
            }

            Expr::Block { stmts, tail_expr } => {
                self.enter_scope();
                for stmt_id in stmts {
                    self.compile_stmt(*stmt_id, body);
                }
                let has_tail = if let Some(tail) = tail_expr {
                    self.compile_expr(*tail, body);
                    true
                } else {
                    false
                };
                self.exit_scope(has_tail);
            }

            Expr::Array { elements } => {
                for elem in elements {
                    self.compile_expr(*elem, body);
                }
                self.emit(Instruction::AllocArray(elements.len()));
            }

            Expr::Object { type_name, fields } => {
                // Look up class field indices and pre-allocated object index
                let name_str = type_name.as_ref().map(std::string::ToString::to_string);
                let field_indices = name_str.as_ref().and_then(|name| self.classes.get(name));
                let class_obj_idx = name_str
                    .as_ref()
                    .and_then(|name| self.class_object_indices.get(name).copied());

                let (Some(field_indices), Some(obj_idx)) = (field_indices, class_obj_idx) else {
                    panic!(
                        "undefined class: {}",
                        name_str.as_deref().unwrap_or("<anonymous>")
                    );
                };

                // Emit AllocInstance with pre-allocated Class object index
                self.emit(Instruction::AllocInstance(ObjectIndex::from_raw(obj_idx)));

                // For each field: Copy instance, compile value, StoreField
                for (field_name, field_value) in fields {
                    // Copy the instance reference (it's at top of stack)
                    self.emit(Instruction::Copy(0));

                    // Compile the field value
                    self.compile_expr(*field_value, body);

                    // Get field index and store
                    let field_name_str: &str = field_name.as_ref();
                    let field_idx =
                        field_indices
                            .get(field_name_str)
                            .copied()
                            .unwrap_or_else(|| {
                                panic!(
                                    "undefined field '{}' in class '{}'",
                                    field_name_str,
                                    name_str.as_deref().unwrap_or("<anonymous>")
                                )
                            });
                    self.emit(Instruction::StoreField(field_idx));
                }
            }

            Expr::FieldAccess { base, field: _ } => {
                self.compile_expr(*base, body);
                // TODO: Resolve field index when class system is complete
                self.emit(Instruction::LoadField(0));
            }

            Expr::Index { base, index } => {
                self.compile_expr(*base, body);
                self.compile_expr(*index, body);
                self.emit(Instruction::LoadArrayElement);
            }

            Expr::Missing => {
                // TODO: cannot compile missing expression - skip
            }
        }
    }

    /// Compile a statement.
    fn compile_stmt(&mut self, stmt_id: StmtId, body: &ExprBody) {
        use baml_hir::Stmt;

        let stmt = &body.stmts[stmt_id];

        match stmt {
            Stmt::Let {
                pattern,
                type_annotation: _,
                initializer,
                ..
            } => {
                if let Some(init) = initializer {
                    self.compile_expr(*init, body);
                } else {
                    // No initializer, push null
                    let idx = self.add_constant(Value::Null);
                    self.emit(Instruction::LoadConst(idx));
                }

                // Extract variable name from pattern
                let pat = &body.patterns[*pattern];
                match pat {
                    Pattern::Binding(name) => {
                        self.track_local(name.as_ref());
                    }
                }
            }

            Stmt::Expr(expr) => {
                let produces_value = Self::expr_produces_value(*expr, body);
                self.compile_expr(*expr, body);
                // Only pop if the expression produced a value
                if produces_value {
                    self.emit(Instruction::Pop(1));
                }
            }

            Stmt::Return(expr) => {
                if let Some(e) = expr {
                    self.compile_expr(*e, body);
                } else {
                    // Return null
                    let idx = self.add_constant(Value::Null);
                    self.emit(Instruction::LoadConst(idx));
                }
                self.emit(Instruction::Return);
            }

            Stmt::While {
                condition,
                body: while_body,
                after,
                origin: _, // Not needed for bytecode generation
            } => {
                self.compile_while_loop(
                    |ctx| ctx.compile_expr(*condition, body),
                    |ctx| {
                        ctx.compile_expr(*while_body, body);
                        // The body result is not used, but if it's a block expression
                        // it will handle its own stack through exit_scope
                    },
                    |ctx| {
                        // Compile the after statement (e.g., update in C-style for loops)
                        // This runs after each iteration, including on `continue`
                        if let Some(after_stmt) = after {
                            ctx.compile_stmt(*after_stmt, body);
                        }
                    },
                );
            }

            Stmt::Break => {
                let loop_info = self
                    .current_loop
                    .as_ref()
                    .expect("break statement outside of loop");
                let pop_until = loop_info.scope_depth;

                // Pop locals from nested scopes before jumping out
                self.emit_scope_drops(pop_until);

                let jump_loc = self.emit(Instruction::Jump(0));
                self.current_loop
                    .as_mut()
                    .unwrap()
                    .break_patch_list
                    .push(jump_loc);
            }

            Stmt::Continue => {
                let loop_info = self
                    .current_loop
                    .as_ref()
                    .expect("continue statement outside of loop");
                let pop_until = loop_info.scope_depth;

                // Pop locals from nested scopes before jumping back
                self.emit_scope_drops(pop_until);

                let jump_loc = self.emit(Instruction::Jump(0));
                self.current_loop
                    .as_mut()
                    .unwrap()
                    .continue_patch_list
                    .push(jump_loc);
            }

            Stmt::Assign { target, value } => {
                let Expr::Path(segments) = &body.exprs[*target] else {
                    panic!(
                        "assignment target must be a variable (field/array assignment not yet implemented)"
                    );
                };
                assert!(
                    (segments.len() == 1),
                    "assignment target must be a simple variable (field assignment not yet implemented)"
                );
                let name_str = segments[0].to_string();
                let Some(&index) = self.locals.get(&name_str) else {
                    panic!("cannot assign to undefined variable: {name_str}");
                };

                self.compile_expr(*value, body);
                self.emit(Instruction::StoreVar(index));
            }

            Stmt::AssignOp { target, op, value } => {
                let Expr::Path(segments) = &body.exprs[*target] else {
                    panic!(
                        "assignment target must be a variable (field/array assignment not yet implemented)"
                    );
                };
                assert!(
                    (segments.len() == 1),
                    "assignment target must be a simple variable (field assignment not yet implemented)"
                );
                let name_str = segments[0].to_string();
                let Some(&index) = self.locals.get(&name_str) else {
                    panic!("cannot assign to undefined variable: {name_str}");
                };

                // Load current value, apply operation, store result
                self.emit(Instruction::LoadVar(index));
                self.compile_expr(*value, body);
                let instr = match op {
                    AssignOp::Add => Instruction::BinOp(BinOp::Add),
                    AssignOp::Sub => Instruction::BinOp(BinOp::Sub),
                    AssignOp::Mul => Instruction::BinOp(BinOp::Mul),
                    AssignOp::Div => Instruction::BinOp(BinOp::Div),
                    AssignOp::Mod => Instruction::BinOp(BinOp::Mod),
                    AssignOp::BitAnd => Instruction::BinOp(BinOp::BitAnd),
                    AssignOp::BitOr => Instruction::BinOp(BinOp::BitOr),
                    AssignOp::BitXor => Instruction::BinOp(BinOp::BitXor),
                    AssignOp::Shl => Instruction::BinOp(BinOp::Shl),
                    AssignOp::Shr => Instruction::BinOp(BinOp::Shr),
                };
                self.emit(instr);
                self.emit(Instruction::StoreVar(index));
            }

            Stmt::Missing => {
                // TODO: cannot compile missing statement - skip
            }
        }
    }

    /// Compile a literal value.
    fn compile_literal(&mut self, lit: &Literal) {
        match lit {
            Literal::Int(v) => {
                let idx = self.add_constant(Value::Int(*v));
                self.emit(Instruction::LoadConst(idx));
            }
            Literal::Float(v) => {
                let f = v.parse::<f64>().unwrap_or(0.0);
                let idx = self.add_constant(Value::Float(f));
                self.emit(Instruction::LoadConst(idx));
            }
            Literal::String(v) => {
                let obj_idx = self.objects.len();
                self.objects.push(Object::String(v.clone()));
                let idx = self.add_constant(Value::Object(ObjectIndex::from_raw(obj_idx)));
                self.emit(Instruction::LoadConst(idx));
            }
            Literal::Bool(v) => {
                let idx = self.add_constant(Value::Bool(*v));
                self.emit(Instruction::LoadConst(idx));
            }
            Literal::Null => {
                let idx = self.add_constant(Value::Null);
                self.emit(Instruction::LoadConst(idx));
            }
        }
    }

    /// Convert HIR binary op to bytecode instruction.
    fn binary_op_instruction(op: BinaryOp) -> Instruction {
        match op {
            BinaryOp::Add => Instruction::BinOp(BinOp::Add),
            BinaryOp::Sub => Instruction::BinOp(BinOp::Sub),
            BinaryOp::Mul => Instruction::BinOp(BinOp::Mul),
            BinaryOp::Div => Instruction::BinOp(BinOp::Div),
            BinaryOp::Mod => Instruction::BinOp(BinOp::Mod),
            BinaryOp::Eq => Instruction::CmpOp(CmpOp::Eq),
            BinaryOp::Ne => Instruction::CmpOp(CmpOp::NotEq),
            BinaryOp::Lt => Instruction::CmpOp(CmpOp::Lt),
            BinaryOp::Le => Instruction::CmpOp(CmpOp::LtEq),
            BinaryOp::Gt => Instruction::CmpOp(CmpOp::Gt),
            BinaryOp::Ge => Instruction::CmpOp(CmpOp::GtEq),
            BinaryOp::BitAnd => Instruction::BinOp(BinOp::BitAnd),
            BinaryOp::BitOr => Instruction::BinOp(BinOp::BitOr),
            BinaryOp::BitXor => Instruction::BinOp(BinOp::BitXor),
            BinaryOp::Shl => Instruction::BinOp(BinOp::Shl),
            BinaryOp::Shr => Instruction::BinOp(BinOp::Shr),
            // And/Or are handled specially for short-circuit
            BinaryOp::And | BinaryOp::Or => unreachable!("handled specially"),
        }
    }

    /// Convert HIR unary op to bytecode instruction.
    fn unary_op_instruction(op: UnaryOp) -> Instruction {
        match op {
            UnaryOp::Not => Instruction::UnaryOp(baml_vm::UnaryOp::Not),
            UnaryOp::Neg => Instruction::UnaryOp(baml_vm::UnaryOp::Neg),
        }
    }

    /// Emit an instruction and return its index.
    fn emit(&mut self, instruction: Instruction) -> usize {
        let index = self.bytecode.instructions.len();
        self.bytecode.instructions.push(instruction);
        self.bytecode.source_lines.push(self.current_source_line);

        let scope_id = self.scopes.last().map(|s| s.id).unwrap_or(0);
        self.bytecode.scopes.push(scope_id);

        index
    }

    /// Add a constant to the pool and return its index.
    fn add_constant(&mut self, value: Value) -> usize {
        // Try to find existing constant
        for (i, existing) in self.bytecode.constants.iter().enumerate() {
            if *existing == value {
                return i;
            }
        }
        self.bytecode.constants.push(value);
        self.bytecode.constants.len() - 1
    }

    /// Get the next instruction index.
    #[allow(clippy::cast_possible_wrap)]
    fn next_insn_index(&self) -> isize {
        self.bytecode.instructions.len() as isize
    }

    /// Patch a jump instruction to point to the current position.
    #[allow(clippy::cast_possible_wrap)]
    fn patch_jump(&mut self, instruction_ptr: usize) {
        let destination = self.bytecode.instructions.len();
        match &mut self.bytecode.instructions[instruction_ptr] {
            Instruction::Jump(offset) | Instruction::JumpIfFalse(offset) => {
                *offset = destination as isize - instruction_ptr as isize;
            }
            _ => panic!("expected jump instruction at index {instruction_ptr}"),
        }
    }

    /// Track a new local variable.
    fn track_local(&mut self, name: &str) -> usize {
        let index = self.locals.len() + 1; // +1 because function is at index 0
        self.locals.insert(name.to_string(), index);

        if let Some(scope) = self.scopes.last_mut() {
            scope.locals.insert(name.to_string());
        }

        index
    }

    /// Enter a new scope.
    fn enter_scope(&mut self) {
        self.scopes.push(Scope {
            depth: self.scopes.len(),
            locals: HashSet::new(),
            id: self.locals_in_scope.len(),
        });
        self.locals_in_scope.push(HashMap::new());
    }

    /// Exit the current scope.
    fn exit_scope(&mut self, scope_has_trailing_expr: bool) {
        // Save locals for debug info before popping
        if let Some(scope) = self.scopes.last() {
            if scope.id < self.locals_in_scope.len() {
                self.locals_in_scope[scope.id].clone_from(&self.locals);
            }
        }

        if let Some(scope) = self.scopes.pop() {
            // depth 0 = function params, depth 1 = function body block
            // Only emit Pop/PopReplace for nested blocks (depth > 1).
            // Function body cleanup is handled by Return.
            if scope.depth > 1 && !scope.locals.is_empty() {
                if scope_has_trailing_expr {
                    self.emit(Instruction::PopReplace(scope.locals.len()));
                } else {
                    self.emit(Instruction::Pop(scope.locals.len()));
                }
            }

            // Always remove locals from tracking (regardless of depth)
            for local in &scope.locals {
                self.locals.remove(local);
            }
        }
    }

    /// Emit instructions to drop scopes from `pop_until` to current.
    ///
    /// Used by break/continue to pop locals before jumping out of nested scopes.
    /// Does NOT modify the scope stack - just emits Pop instructions.
    fn emit_scope_drops(&mut self, pop_until: usize) {
        let scopes = &self.scopes[pop_until..];

        let local_count: usize = scopes
            .iter()
            .map(|s| {
                // depth 0 is function body block, which will be cleaned up by return
                if s.depth == 0 { 0 } else { s.locals.len() }
            })
            .sum();

        if local_count > 0 {
            self.emit(Instruction::Pop(local_count));
        }
    }

    /// Compile a while loop with proper break/continue support.
    ///
    /// The loop structure is:
    /// ```text
    /// loop_start:
    ///   compile_condition
    ///   JumpIfFalse exit_pop
    ///   Pop 1  // pop condition
    ///   compile_body
    ///   compile_after (for continue handling)
    ///   Jump loop_start
    /// exit_pop:
    ///   Pop 1  // pop condition
    /// ```
    fn compile_while_loop(
        &mut self,
        compile_condition: impl FnOnce(&mut Self),
        compile_body: impl FnOnce(&mut Self),
        compile_after: impl FnOnce(&mut Self),
    ) {
        let loop_start = self.next_insn_index();

        compile_condition(self);

        // This jump needs patching - it jumps to exit when condition is false
        let bail_jump = self.emit(Instruction::JumpIfFalse(0));
        self.emit(Instruction::Pop(1)); // Pop condition (true case)

        // Wrap body in loop context for break/continue
        let break_locs = self.wrap_loop_body(compile_body);

        // Code that runs after each iteration (for continue targets)
        compile_after(self);

        // Jump back to loop start
        self.emit(Instruction::Jump(loop_start - self.next_insn_index()));

        // Exit point: pop condition (false case)
        let pop_if_condition = self.emit(Instruction::Pop(1));
        self.patch_jump_to(bail_jump, pop_if_condition);

        // Patch all break statements to jump here (after condition pop)
        for loc in break_locs {
            self.patch_jump(loc);
        }
    }

    /// Wrap a loop body to handle break/continue.
    ///
    /// Returns the break patch list - locations that need to be patched
    /// to point to the loop exit.
    fn wrap_loop_body(&mut self, compile_body: impl FnOnce(&mut Self)) -> Vec<usize> {
        self.enter_scope();

        let old_loop = self.current_loop.replace(LoopInfo {
            scope_depth: self.scopes.len(),
            break_patch_list: Vec::new(),
            continue_patch_list: Vec::new(),
        });

        compile_body(self);

        let loop_info = std::mem::replace(&mut self.current_loop, old_loop)
            .expect("loop info should exist after compile_body");

        self.exit_scope(false);

        // Patch continue jumps to point to current position
        // (which is right before the "after" code and jump back to start)
        for continue_jmp in loop_info.continue_patch_list {
            self.patch_jump(continue_jmp);
        }

        loop_info.break_patch_list
    }

    /// Patch a jump instruction to point to a specific destination.
    #[allow(clippy::cast_possible_wrap)]
    fn patch_jump_to(&mut self, instruction_ptr: usize, destination: usize) {
        match &mut self.bytecode.instructions[instruction_ptr] {
            Instruction::Jump(offset) | Instruction::JumpIfFalse(offset) => {
                *offset = destination as isize - instruction_ptr as isize;
            }
            _ => panic!("expected jump instruction at index {instruction_ptr}"),
        }
    }
}

/// Compile a function to bytecode using THIR type information.
///
/// This is the main entry point for compiling a single function.
///
/// # Arguments
/// * `signature` - Function signature (name, parameters, return type)
/// * `body` - HIR function body
/// * `ctx` - Codegen context containing type inference, globals, class info, and shared object pool
///
/// Objects (strings, etc.) are added directly to `ctx.objects` with correct indices,
/// eliminating the need for post-compilation index remapping.
pub fn compile_function(
    signature: &FunctionSignature,
    body: &FunctionBody,
    ctx: CodegenContext<'_, '_, '_>,
) -> Function {
    let mut compiler = Compiler::new(ctx);
    compiler.compile_function(signature, body)
}
