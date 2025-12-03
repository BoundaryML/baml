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

/// Compiler state for generating bytecode from THIR.
pub struct Compiler<'db> {
    /// Type inference result from THIR.
    inference: &'db InferenceResult<'db>,

    /// Resolved global names to indices.
    globals: HashMap<String, usize>,

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

    /// Objects pool (for strings, classes, etc.).
    objects: Vec<Object>,

    /// Counter for generating unique compiler-internal variable names.
    /// Used to avoid collisions when the same internal variable name
    /// (like the iterator temp) appears in nested scopes.
    gensym_counter: usize,
}

impl<'db> Compiler<'db> {
    /// Create a new compiler with the given type inference result and global mappings.
    pub fn new(inference: &'db InferenceResult<'db>, globals: HashMap<String, usize>) -> Self {
        Self {
            inference,
            globals,
            locals: HashMap::new(),
            scopes: Vec::new(),
            locals_in_scope: Vec::new(),
            current_source_line: 0,
            bytecode: Bytecode::new(),
            objects: Vec::new(),
            gensym_counter: 0,
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
                self.emit(Instruction::Pop(1));
                self.compile_expr(*then_branch, body);
                let skip_else = self.emit(Instruction::Jump(0));
                self.patch_jump(skip_if);
                self.emit(Instruction::Pop(1));
                if let Some(else_expr) = else_branch {
                    self.compile_expr(*else_expr, body);
                }
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

            Expr::Object {
                type_name: _,
                fields,
            } => {
                // For now, just compile fields in order
                // TODO: Proper class allocation when class system is complete
                for (_name, value) in fields {
                    self.compile_expr(*value, body);
                }
                self.emit(Instruction::AllocArray(fields.len()));
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
                let loop_start = self.next_insn_index();
                self.compile_expr(*condition, body);
                let exit_jump = self.emit(Instruction::JumpIfFalse(0));
                self.emit(Instruction::Pop(1));
                self.compile_expr(*while_body, body);
                self.emit(Instruction::Pop(1)); // Discard body result
                self.emit(Instruction::Jump(loop_start - self.next_insn_index()));
                self.patch_jump(exit_jump);
                self.emit(Instruction::Pop(1)); // Pop condition on exit
            }

            Stmt::ForIn {
                pattern,
                iterator,
                body: for_body,
            } => {
                // Compile iterator
                self.compile_expr(*iterator, body);

                self.enter_scope();

                // Store iterator in temp variable (unique name to avoid collisions in nested loops)
                let iter_var = self.gensym("iter");
                self.track_local(&iter_var);

                // Store length
                // TODO: Call length method when native functions are available

                // Store index = 0 (unique name to avoid collisions in nested loops)
                let idx_var = self.gensym("idx");
                let zero_idx = self.add_constant(Value::Int(0));
                self.emit(Instruction::LoadConst(zero_idx));
                self.track_local(&idx_var);

                let loop_start = self.next_insn_index();

                // Check if index < length
                // For now, this is a simplified implementation
                // TODO: Proper iterator protocol

                // Extract variable name from pattern
                let pat = &body.patterns[*pattern];
                match pat {
                    Pattern::Binding(name) => {
                        self.track_local(name.as_ref());
                    }
                }

                // Compile body
                self.compile_expr(*for_body, body);
                self.emit(Instruction::Pop(1)); // Discard body result

                // Increment index
                // TODO: Proper loop mechanics

                self.emit(Instruction::Jump(loop_start - self.next_insn_index()));

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

                let loop_start = self.next_insn_index();

                // Compile condition
                if let Some(cond) = condition {
                    self.compile_expr(*cond, body);
                    let exit_jump = self.emit(Instruction::JumpIfFalse(0));
                    self.emit(Instruction::Pop(1));

                    // Compile body
                    self.compile_expr(*for_body, body);
                    self.emit(Instruction::Pop(1));

                    // Compile update
                    if let Some(upd) = update {
                        self.compile_expr(*upd, body);
                        self.emit(Instruction::Pop(1));
                    }

                    self.emit(Instruction::Jump(loop_start - self.next_insn_index()));
                    self.patch_jump(exit_jump);
                    self.emit(Instruction::Pop(1));
                } else {
                    // Infinite loop
                    self.compile_expr(*for_body, body);
                    self.emit(Instruction::Pop(1));

                    if let Some(upd) = update {
                        self.compile_expr(*upd, body);
                        self.emit(Instruction::Pop(1));
                    }

                    self.emit(Instruction::Jump(loop_start - self.next_insn_index()));
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
///
/// # Returns
/// A tuple of (Function, `Vec<Object>`) where the objects are the object pool
/// containing strings, classes, etc. referenced by the function's bytecode.
pub fn compile_function<'db>(
    name: &str,
    params: &[Name],
    body: &FunctionBody,
    inference: &'db InferenceResult<'db>,
    globals: HashMap<String, usize>,
) -> (Function, Vec<Object>) {
    let mut compiler = Compiler::new(inference, globals);
    let function = compiler.compile_function(name, params, body);
    (function, compiler.objects)
}
