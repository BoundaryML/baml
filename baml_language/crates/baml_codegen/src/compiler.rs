//! THIR to bytecode compiler.
//!
//! This module compiles the Typed High-level IR into VM bytecode.
//! It takes the type-checked `InferenceResult` from THIR along with
//! the expression body from HIR.

use std::collections::{HashMap, HashSet};

use baml_base::Name;
use baml_hir::{BinaryOp, ExprBody, ExprId, FunctionBody, Literal, Pattern, StmtId, UnaryOp};
use baml_thir::{InferenceResult, Ty};
use baml_vm::{BinOp, Bytecode, CmpOp, Function, FunctionKind, Instruction, Object, Value};

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
    _scope_depth: usize,
    /// Jump instruction locations to patch for break statements.
    break_patch_list: Vec<usize>,
    /// Jump instruction locations to patch for continue statements.
    continue_patch_list: Vec<usize>,
}

/// Information about a class for bytecode generation.
#[derive(Debug, Clone)]
pub struct ClassInfo {
    /// Maps field name to its index in the class.
    pub field_indices: HashMap<String, usize>,
    /// Ordered list of field names.
    pub field_names: Vec<String>,
}

/// Compiler state for generating bytecode from THIR.
pub struct Compiler<'db> {
    /// Type inference result from THIR.
    inference: &'db InferenceResult<'db>,

    /// Resolved global names to indices.
    globals: HashMap<String, usize>,

    /// Resolved class information (name -> `ClassInfo`).
    classes: HashMap<String, ClassInfo>,

    /// Pre-allocated Class object indices in the program's object pool.
    class_object_indices: HashMap<String, usize>,

    /// Resolved local variable names to stack indices.
    locals: HashMap<String, usize>,

    /// Scopes for tracking local variable lifetimes.
    scopes: Vec<Scope>,

    /// Locals in scope per scope ID (debug info).
    locals_in_scope: Vec<HashMap<String, usize>>,

    /// Current source line (for debugging).
    current_source_line: usize,

    /// Bytecode being generated.
    bytecode: Bytecode,

    /// Objects pool (for strings, etc. - NOT classes, those are pre-allocated).
    objects: Vec<Object>,

    /// Counter for generating unique compiler-internal variable names.
    /// Used to avoid collisions when the same internal variable name
    /// (like the iterator temp) appears in nested scopes.
    gensym_counter: usize,

    /// Current loop info for break/continue handling.
    current_loop: Option<LoopInfo>,
}

impl<'db> Compiler<'db> {
    /// Create a new compiler with the given type inference result and global mappings.
    pub fn new(
        inference: &'db InferenceResult<'db>,
        globals: HashMap<String, usize>,
        classes: HashMap<String, ClassInfo>,
        class_object_indices: HashMap<String, usize>,
    ) -> Self {
        Self {
            inference,
            globals,
            classes,
            class_object_indices,
            locals: HashMap::new(),
            scopes: Vec::new(),
            locals_in_scope: Vec::new(),
            current_source_line: 0,
            bytecode: Bytecode::new(),
            objects: Vec::new(),
            gensym_counter: 0,
            current_loop: None,
        }
    }

    /// Get the type of an expression from the inference result.
    fn expr_type(&self, expr_id: ExprId) -> Option<&Ty<'db>> {
        self.inference.expr_types.get(&expr_id)
    }

    /// Generate a unique compiler-internal variable name.
    /// Uses `@` prefix which is not valid in user code, ensuring no collisions.
    fn gensym(&mut self, prefix: &str) -> String {
        let name = format!("@{prefix}_{}", self.gensym_counter);
        self.gensym_counter += 1;
        name
    }

    /// Compile a function from its THIR-typed body.
    pub fn compile_function(
        &mut self,
        name: &str,
        params: &[Name],
        body: &FunctionBody,
    ) -> Function {
        // Reset state for this function
        self.locals.clear();
        self.scopes.clear();
        self.locals_in_scope.clear();
        self.bytecode = Bytecode::new();

        match body {
            FunctionBody::Expr(expr_body) => self.compile_expr_function(name, params, expr_body),
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
                }
            }
            FunctionBody::Missing => {
                // Missing body - return empty function
                Function {
                    name: name.to_string(),
                    arity: params.len(),
                    bytecode: Bytecode::new(),
                    kind: FunctionKind::Exec,
                    locals_in_scope: Vec::new(),
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

            Expr::Path(name) => {
                let name_str = name.to_string();
                if let Some(&index) = self.locals.get(&name_str) {
                    self.emit(Instruction::LoadVar(index));
                } else if let Some(&index) = self.globals.get(&name_str) {
                    self.emit(Instruction::LoadGlobal(index));
                } else {
                    // Unknown variable - this should have been caught by type checker
                    // For now, treat as global 0 (error recovery)
                    self.emit(Instruction::LoadGlobal(0));
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
                // Push function
                self.compile_expr(*callee, body);
                // Push arguments
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
                self.compile_expr(*condition, body);
                let skip_if = self.emit(Instruction::JumpIfFalse(0));
                self.emit(Instruction::Pop(1)); // Pop condition (true path)
                self.compile_expr(*then_branch, body);
                let skip_else = self.emit(Instruction::Jump(0));
                self.patch_jump(skip_if);
                self.emit(Instruction::Pop(1)); // Pop condition (false path)

                if let Some(else_expr) = else_branch {
                    self.compile_expr(*else_expr, body);
                }
                // If no else: then_branch must not produce a value (type checker ensures this).
                // Both paths pop condition and leave stack in same state.

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
                // Look up class information and pre-allocated object index
                let name_str = type_name.as_ref().map(std::string::ToString::to_string);
                let class_info = name_str
                    .as_ref()
                    .and_then(|name| self.classes.get(name).cloned());
                let class_obj_idx = name_str
                    .as_ref()
                    .and_then(|name| self.class_object_indices.get(name).copied());

                if let (Some(class_info), Some(obj_idx)) = (class_info, class_obj_idx) {
                    // Emit AllocInstance with pre-allocated Class object index
                    self.emit(Instruction::AllocInstance(obj_idx));

                    // For each field: Copy instance, compile value, StoreField
                    for (field_name, field_value) in fields {
                        // Copy the instance reference (it's at top of stack)
                        self.emit(Instruction::Copy(0));

                        // Compile the field value
                        self.compile_expr(*field_value, body);

                        // Get field index and store
                        let field_name_str: &str = field_name.as_ref();
                        let field_idx = class_info
                            .field_indices
                            .get(field_name_str)
                            .copied()
                            .unwrap_or(0);
                        self.emit(Instruction::StoreField(field_idx));
                    }
                } else {
                    // Fallback: class not found, use array (shouldn't happen in well-typed code)
                    for (_name, value) in fields {
                        self.compile_expr(*value, body);
                    }
                    self.emit(Instruction::AllocArray(fields.len()));
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
                // Emit null for missing expressions
                let idx = self.add_constant(Value::Null);
                self.emit(Instruction::LoadConst(idx));
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
                self.compile_expr(*expr, body);
                // Expression statement - discard result
                self.emit(Instruction::Pop(1));
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
            } => {
                self.compile_while_loop(
                    |ctx| ctx.compile_expr(*condition, body),
                    |ctx| {
                        ctx.compile_expr(*while_body, body);
                        // The body result is not used, but if it's a block expression
                        // it will handle its own stack through exit_scope
                    },
                    |_| {}, // No after code for simple while
                );
            }

            Stmt::ForIn {
                pattern,
                iterator,
                body: for_body,
            } => {
                // For-in loop compilation:
                // let @array = (iterator);
                // let @array_len = baml.Array.length(@array);
                // let @loop_i = 0;
                // while (@loop_i < @array_len) {
                //     let <pattern> = @array[@loop_i];
                //     @loop_i = @loop_i + 1;
                //     (body)
                // }

                // Compile iterator expression - this puts the array on the stack
                self.compile_expr(*iterator, body);

                self.enter_scope();

                // Store array in temp variable
                let array_var = self.gensym("for_array");
                let array_location = self.track_local(&array_var);

                // Get array length: baml.Array.length(@array)
                let length_var = self.gensym("for_len");
                if let Some(&len_fn_idx) = self.globals.get("baml.Array.length") {
                    self.emit(Instruction::LoadGlobal(len_fn_idx));
                    self.emit(Instruction::LoadVar(array_location));
                    self.emit(Instruction::Call(1));
                } else {
                    // Fallback: push 0 for length (loop won't execute)
                    let zero = self.add_constant(Value::Int(0));
                    self.emit(Instruction::LoadConst(zero));
                }
                let length_location = self.track_local(&length_var);

                // Initialize loop index to 0
                let idx_var = self.gensym("for_idx");
                let zero = self.add_constant(Value::Int(0));
                self.emit(Instruction::LoadConst(zero));
                let idx_location = self.track_local(&idx_var);

                // Now compile the while loop
                self.compile_while_loop(
                    |ctx| {
                        // Condition: @loop_i < @array_len
                        ctx.emit(Instruction::LoadVar(idx_location));
                        ctx.emit(Instruction::LoadVar(length_location));
                        ctx.emit(Instruction::CmpOp(CmpOp::Lt));
                    },
                    |ctx| {
                        ctx.enter_scope();

                        // Extract variable name from pattern and track it
                        let pat = &body.patterns[*pattern];
                        match pat {
                            Pattern::Binding(name) => {
                                ctx.track_local(name.as_ref());
                            }
                        }

                        // let <pattern> = @array[@loop_i]
                        ctx.emit(Instruction::LoadVar(array_location));
                        ctx.emit(Instruction::LoadVar(idx_location));
                        ctx.emit(Instruction::LoadArrayElement);

                        // Increment index: @loop_i = @loop_i + 1
                        ctx.emit(Instruction::LoadVar(idx_location));
                        let one = ctx.add_constant(Value::Int(1));
                        ctx.emit(Instruction::LoadConst(one));
                        ctx.emit(Instruction::BinOp(BinOp::Add));
                        ctx.emit(Instruction::StoreVar(idx_location));

                        // Compile body
                        ctx.compile_expr(*for_body, body);

                        ctx.exit_scope(false);
                    },
                    |_| {}, // No after code
                );

                self.exit_scope(false);
            }

            Stmt::ForCStyle {
                initializer,
                condition,
                update,
                body: for_body,
            } => {
                self.enter_scope();

                // Compile initializer
                if let Some(init_stmt) = initializer {
                    self.compile_stmt(*init_stmt, body);
                }

                match condition {
                    Some(cond) => {
                        // Loop with condition
                        self.compile_while_loop(
                            |ctx| ctx.compile_expr(*cond, body),
                            |ctx| ctx.compile_expr(*for_body, body),
                            |ctx| {
                                if let Some(upd) = update {
                                    ctx.compile_expr(*upd, body);
                                    ctx.emit(Instruction::Pop(1)); // Discard update result
                                }
                            },
                        );
                    }
                    None => {
                        // Infinite loop
                        let loop_start = self.next_insn_index();

                        let break_locs = self.wrap_loop_body(|ctx| {
                            ctx.compile_expr(*for_body, body);
                        });

                        if let Some(upd) = update {
                            self.compile_expr(*upd, body);
                            self.emit(Instruction::Pop(1));
                        }

                        self.emit(Instruction::Jump(loop_start - self.next_insn_index()));

                        // Patch break locations
                        for loc in break_locs {
                            self.patch_jump(loc);
                        }
                    }
                }

                self.exit_scope(false);
            }

            Stmt::Missing => {}
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
                let idx = self.add_constant(Value::Object(obj_idx));
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
            // At depth 0 (function body), we don't need to pop locals
            // because the function will return
            if scope.depth >= 1 && !scope.locals.is_empty() {
                if scope_has_trailing_expr {
                    self.emit(Instruction::PopReplace(scope.locals.len()));
                } else {
                    self.emit(Instruction::Pop(scope.locals.len()));
                }

                // Remove locals from this scope
                for local in &scope.locals {
                    self.locals.remove(local);
                }
            }
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
            _scope_depth: self.scopes.len(),
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
/// * `name` - Function name
/// * `params` - Parameter names
/// * `body` - HIR function body
/// * `inference` - THIR type inference result
/// * `globals` - Global name to index mapping
/// * `classes` - Class name to field information mapping
/// * `class_object_indices` - Pre-allocated Class object indices in program's object pool
///
/// # Returns
/// A tuple of (Function, `Vec<Object>`) where the objects are the object pool
/// containing strings, etc. referenced by the function's bytecode.
/// Class objects are NOT included here - they are pre-allocated in the program.
pub fn compile_function<'db>(
    name: &str,
    params: &[Name],
    body: &FunctionBody,
    inference: &'db InferenceResult<'db>,
    globals: HashMap<String, usize>,
    classes: HashMap<String, ClassInfo>,
    class_object_indices: HashMap<String, usize>,
) -> (Function, Vec<Object>) {
    let mut compiler = Compiler::new(inference, globals, classes, class_object_indices);
    let function = compiler.compile_function(name, params, body);
    (function, compiler.objects)
}
