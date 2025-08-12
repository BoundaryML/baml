//! Baml VM bytecode generation.

use std::collections::{HashMap, HashSet};

use baml_vm::{
    BamlVmProgram, BinOp, Bytecode, Class, CmpOp, Function, FunctionKind, Instruction, Object,
    UnaryOp, Value,
};
use internal_baml_parser_database::ParserDatabase;

use crate::hir;

/// Compile a Baml AST into bytecode.
///
/// This now uses a two-stage compilation process:
/// 1. AST -> HIR
/// 2. HIR -> Bytecode
pub fn compile(ast: &ParserDatabase) -> anyhow::Result<BamlVmProgram> {
    // Stage 1: AST -> HIR
    // eprintln!("AST:\n{:#?}", ast.ast);
    let hir = hir::Hir::from_ast(&ast.ast);

    // eprintln!("\nHIR:\n{:#?}", hir);

    // Stage 2: HIR -> Bytecode
    compile_hir_to_bytecode(&hir)
}

/// Compile HIR to bytecode.
///
/// This function takes an HIR Program and generates the bytecode for the VM.
fn compile_hir_to_bytecode(hir: &hir::Hir) -> anyhow::Result<BamlVmProgram> {
    let mut resolved_globals = HashMap::new();
    let mut resolved_classes = HashMap::new();
    let mut llm_functions = HashSet::new();

    // Resolve global functions from HIR
    for func in &hir.expr_functions {
        resolved_globals.insert(func.name.clone(), resolved_globals.len());
    }

    for func in &hir.llm_functions {
        resolved_globals.insert(func.name.clone(), resolved_globals.len());
        llm_functions.insert(func.name.clone());
    }

    // Resolve classes from HIR
    for class in &hir.classes {
        resolved_globals.insert(class.name.clone(), resolved_globals.len());

        // Resolve class fields.
        let mut class_fields = HashMap::new();
        for (field_index, field) in class.fields.iter().enumerate() {
            class_fields.insert(field.name.clone(), field_index);
        }

        resolved_classes.insert(class.name.clone(), class_fields);
    }

    let mut objects = Vec::with_capacity(resolved_globals.len());
    let mut globals = Vec::with_capacity(resolved_globals.len());

    // Compile HIR functions to bytecode
    for func in &hir.expr_functions {
        let bytecode_function = compile_hir_function(
            func,
            &resolved_globals,
            &resolved_classes,
            &llm_functions,
            &mut objects,
        )?;

        // Add the function to the globals and objects pools.
        globals.push(Value::Object(objects.len()));
        objects.push(Object::Function(bytecode_function));
    }

    for func in &hir.llm_functions {
        let bytecode_llm_function = Object::Function(Function {
            name: func.name.clone(),
            arity: func.parameters.len(),
            bytecode: Bytecode::new(),
            kind: FunctionKind::Llm,
            locals_in_scope: vec![func.parameters.iter().map(|p| p.name.clone()).collect()],
        });

        globals.push(Value::Object(objects.len()));
        objects.push(bytecode_llm_function);
    }

    // Add classes to objects
    for class in &hir.classes {
        let bytecode_class = Class {
            name: class.name.clone(),
            field_names: class.fields.iter().map(|f| f.name.clone()).collect(),
        };

        globals.push(Value::Object(objects.len()));
        objects.push(Object::Class(bytecode_class));
    }

    let resolved_function_names = objects
        .iter()
        .enumerate()
        .filter_map(|(i, obj)| match obj {
            Object::Function(f) => Some((f.name.clone(), (i, f.kind))),
            _ => None,
        })
        .collect();

    Ok(BamlVmProgram {
        objects,
        globals,
        resolved_function_names,
    })
}

/// Compile an HIR function to bytecode.
fn compile_hir_function(
    func: &hir::ExprFunction,
    globals: &HashMap<String, usize>,
    classes: &HashMap<String, HashMap<String, usize>>,
    llm_functions: &HashSet<String>,
    objects: &mut Vec<Object>,
) -> anyhow::Result<Function> {
    let mut compiler = HirCompiler::new(globals, classes, llm_functions, objects);
    compiler.compile_function(func)
}

/// Block scope.
///
/// The scope increments with each nested block. Example:
///
/// ```ignore
/// fn example() {          // Scope { id: 0, depth: 0, locals: [a, d] }
///     let a = 1;
///     {                   // Scope { id: 1, depth: 1, locals: [a, b] }
///         let b = 2;
///         {               // Scope { id: 2, depth: 2, locals: [a, b, c] }
///             let c  = 3;
///         }
///     }
///
///     let d = 4;
///
///     {                   // Scope { id: 3, depth: 1, locals: [a, d, e] }
///         let e = 4;
///     }
/// }
/// ```
///
/// This is used to keep track of local variables present in the evaluation
/// stack.
#[derive(Debug, Default)]
struct Scope {
    /// Scope depth.
    depth: usize,

    /// Locals in this scope only. Parent scopes locals are not included.
    locals: HashSet<String>,

    /// ID of this scope.
    id: usize,
}

/// HIR to bytecode compiler.
struct HirCompiler<'g> {
    /// Resolved global variables.
    ///
    /// Maps the name of the global variable to its index in the globals pool.
    globals: &'g HashMap<String, usize>,

    /// Resolved class fields.
    ///
    /// Maps the name of the class to the field resolution. Field resolution
    /// is basically a transformation of field name to an index in an array.
    ///
    /// TODO: The `g` lifetime here doesn't need to be the same as the globals
    /// lifetime.
    classes: &'g HashMap<String, HashMap<String, usize>>,

    llm_functions: &'g HashSet<String>,

    /// Resolved local variables.
    ///
    /// Maps the name of the variable to its final index in the eval stack.
    locals: HashMap<String, usize>,

    /// Scope stack.
    scopes: Vec<Scope>,

    /// Locals in scope.
    locals_in_scope: Vec<HashMap<String, usize>>,

    /// Current source line.
    current_source_line: usize,

    /// Bytecode to generate.
    bytecode: Bytecode,

    /// Objects pool.
    objects: &'g mut Vec<Object>,
}

impl<'g> HirCompiler<'g> {
    fn new(
        globals: &'g HashMap<String, usize>,
        classes: &'g HashMap<String, HashMap<String, usize>>,
        llm_functions: &'g HashSet<String>,
        objects: &'g mut Vec<Object>,
    ) -> Self {
        Self {
            globals,
            classes,
            llm_functions,
            objects,
            locals: HashMap::new(),
            bytecode: Bytecode::new(),
            scopes: Vec::new(),
            current_source_line: 0,
            locals_in_scope: Vec::new(),
        }
    }

    /// Main entry point.
    ///
    /// Here we compile a source function into a [`Function`] VM struct.
    fn compile_function(&mut self, func: &hir::ExprFunction) -> anyhow::Result<Function> {
        // Compile statements in the function body.
        self.compile_block_with_parameters(&func.body, &func.parameters);

        Ok(Function {
            name: func.name.clone(),
            arity: func.parameters.len(),
            bytecode: self.bytecode.clone(),
            kind: FunctionKind::Exec,

            // Debug info.
            locals_in_scope: Vec::from_iter(self.locals_in_scope.iter().map(|locals| {
                let mut names = Vec::with_capacity(locals.len() + 1);

                // Function is pushed onto the stack.
                names.push(format!("<fn {}>", func.name));

                // Locals come after.
                names.resize_with(names.capacity(), String::new);

                // Distribute locals to their respective indexes.
                for (name, index) in locals {
                    names[*index] = name.to_string();
                }

                names
            })),
        })
    }

    /// Entry for function or scope compilations.
    ///
    /// Functions have parameters so we need to track those as well.
    fn compile_block_with_parameters(&mut self, block: &hir::Block, parameters: &[hir::Parameter]) {
        self.enter_scope();

        for param in parameters {
            self.track_local(&param.name);
        }

        for statement in &block.statements {
            self.compile_statement(statement);
        }

        let scope_has_ending_expr = matches!(
            block.statements.last(),
            Some(hir::Statement::Expression { .. })
        );

        self.exit_scope(scope_has_ending_expr);
    }

    /// Used to compile nested blocks within functions.
    fn compile_block(&mut self, block: &hir::Block) {
        self.compile_block_with_parameters(block, &[]);
    }

    /// A statement is anything that does not produce a value by itself.
    fn compile_statement(&mut self, statement: &hir::Statement) {
        match statement {
            hir::Statement::Let { name, value, .. } => {
                self.compile_expression(value);
                self.track_local(name);
            }

            hir::Statement::Declare { name, .. } => {
                // For mutable references, we need to allocate space on the stack
                // We'll push a null/undefined value as placeholder
                let constant_index = self.add_constant(Value::Null);
                self.emit(Instruction::LoadConst(constant_index));
                self.track_local(name);
            }

            hir::Statement::Assign { name, value } => {
                self.compile_expression(value);
                self.emit(Instruction::StoreVar(self.locals[name]));
            }

            hir::Statement::DeclareAndAssign { name, value, .. } => {
                self.compile_expression(value);
                self.track_local(name);
            }

            hir::Statement::Return { expr, .. } => {
                self.compile_expression(expr);
                self.emit(Instruction::Return);
            }

            hir::Statement::Expression { expr, .. } => {
                self.compile_expression(expr);
            }

            hir::Statement::SemicolonExpression { expr, .. } => {
                self.compile_expression(expr);
                // This could be a function call or any other random expression
                // like:
                //
                // 2 + 2;
                //
                // But since the result is not stored anywhere (not a let
                // binding) then implicitly drop the value.
                self.emit(Instruction::Pop(1));
            }

            hir::Statement::ForLoop { .. } => {
                todo!()
            }

            hir::Statement::While {
                condition, block, ..
            } => {
                // Remember where the loop starts
                let loop_start = self.bytecode.instructions.len() as isize;

                // Compile condition
                self.compile_expression(condition);

                // Jump out of loop if false
                let exit_jump = self.emit(Instruction::JumpIfFalse(0));

                // Pop condition
                self.emit(Instruction::Pop(1));

                // Compile loop body
                self.compile_block(block);

                // Jump back to start
                let loop_end = self.bytecode.instructions.len() as isize;
                let offset = -(loop_end - loop_start);
                self.emit(Instruction::Jump(offset));

                // Patch exit jump
                self.patch_jump(exit_jump);

                // Pop condition
                self.emit(Instruction::Pop(1));
            }
        }
    }

    /// Generate bytecode for an expression.
    fn compile_expression(&mut self, expr: &hir::Expression) {
        // TODO: The implementation of line number is extremely slow. It always
        // reads the entire source string to find the line number.
        self.current_source_line = expr.span().line_number();

        match expr {
            hir::Expression::BoolValue(val, _) => {
                let index = self.add_constant(Value::Bool(*val));
                self.emit(Instruction::LoadConst(index));
            }

            hir::Expression::ArrayAccess { base, index, .. } => {
                // Compile the base expression (the array)
                self.compile_expression(base);

                // Compile the index expression
                self.compile_expression(index);

                // Emit the LoadArrayElement instruction
                // Stack will be [array, index] and LoadArrayElement will consume both
                // and push the result element
                self.emit(Instruction::LoadArrayElement);
            }

            hir::Expression::FieldAccess { .. } => {
                unimplemented!("Array access compilation")
            }

            hir::Expression::NumericValue(num, _) => {
                let value = num
                    .parse::<i64>()
                    .map(Value::Int)
                    .or_else(|_| num.parse::<f64>().map(Value::Float))
                    .unwrap_or_else(|_| panic!("failed to parse number: {num}"));

                let index = self.add_constant(value);
                self.emit(Instruction::LoadConst(index));
            }

            hir::Expression::StringValue(string, _)
            | hir::Expression::RawStringValue(string, _) => {
                // Allocate the string in the objects pool
                self.objects.push(Object::String(string.clone()));
                let object_index = self.objects.len() - 1;

                // Add a constant that points to the string object
                let const_index = self.add_constant(Value::Object(object_index));
                self.emit(Instruction::LoadConst(const_index));
            }

            hir::Expression::Identifier(name, _) => {
                if let Some(&index) = self.locals.get(name) {
                    self.emit(Instruction::LoadVar(index));
                } else {
                    panic!("undefined variable: {name}");
                }
            }

            hir::Expression::Array(elements, _) => {
                for element in elements {
                    self.compile_expression(element);
                }
                self.emit(Instruction::AllocArray(elements.len()));
            }

            hir::Expression::Map(_pairs, _) => {
                // Maps are not yet implemented in bytecode
                todo!("map compilation")
            }

            hir::Expression::JinjaExpressionValue(_, _) => {
                todo!("jinja expression compilation")
            }

            hir::Expression::Call { function, args, .. } => {
                let name = match function.as_ref() {
                    hir::Expression::Identifier(name, _) => name,
                    _ => panic!("expressions that evaluate to functions are not supported yet"),
                };

                // Push the function onto the stack
                if let Some(&index) = self.globals.get(name) {
                    self.emit(Instruction::LoadGlobal(index));
                } else {
                    panic!("undefined function: {name}");
                }

                // Push the arguments onto the stack
                for arg in args {
                    self.compile_expression(arg);
                }

                // Either async LLM call or regular function call.
                if self.llm_functions.contains(name) {
                    self.emit(Instruction::DispatchFuture(args.len()));
                    self.emit(Instruction::Await);
                } else {
                    self.emit(Instruction::Call(args.len()));
                }
            }

            hir::Expression::ClassConstructor(constructor, _) => {
                let Some(&class_index) = self.globals.get(&constructor.class_name) else {
                    panic!("undefined class: {}", constructor.class_name);
                };

                // Allocate instance
                self.emit(Instruction::AllocInstance(class_index));

                let mut defined_named_fields = std::collections::HashSet::new();

                // Process fields in order
                for field in &constructor.fields {
                    match field {
                        hir::ClassConstructorField::Named { name, value } => {
                            self.compile_expression(value);

                            let Some(classes) = self.classes.get(&constructor.class_name) else {
                                panic!("undefined class: {}", constructor.class_name);
                            };

                            let Some(&field_index) = classes.get(name) else {
                                panic!("undefined field: {}.{}", constructor.class_name, name);
                            };

                            self.emit(Instruction::StoreField(field_index));
                            defined_named_fields.insert(name.as_str());
                        }
                        hir::ClassConstructorField::Spread { value } => {
                            // TODO: @antonio: Variable tracking here is wrong.
                            self.compile_expression(value);

                            // Pseudo local, user didn't declare it.
                            let spread_local = self.locals.len() + 2;
                            self.emit(Instruction::LoadVar(spread_local - 1));

                            let Some(classes) = self.classes.get(&constructor.class_name) else {
                                panic!("undefined class: {}", constructor.class_name);
                            };

                            for (field_name, &field_index) in classes {
                                if !defined_named_fields.contains(field_name.as_str()) {
                                    self.emit(Instruction::LoadVar(spread_local));
                                    self.emit(Instruction::LoadField(field_index));
                                    self.emit(Instruction::StoreField(field_index));
                                }
                            }
                        }
                    }
                }
            }

            hir::Expression::If {
                condition,
                if_branch,
                else_branch,
                ..
            } => {
                // First, compile the condition. This will leave the end result
                // of the condition on top of the stack.
                self.compile_expression(condition);

                // Skip the `if { ... }` branch when condition is false. We'll
                // patch this offset later when we know how many instructions to
                // jump over, so we'll store a reference to this instruction.
                let skip_if = self.emit(Instruction::JumpIfFalse(0));

                // Skip the `if { ... }` branch when condition is false. We'll
                // patch this offset later when we know how many instructions to
                // jump over, so we'll store a reference to this instruction.
                self.emit(Instruction::Pop(1));

                // Compile the `if { ... }` branch.
                self.compile_expression(if_branch);

                // Now skip the potential `else { ... }` branch. We'll patch the
                // jump later.
                let skip_else = self.emit(Instruction::Jump(0));

                // We now know where the `if { ... }` branch ends so we can
                // patch the JUMP_IF_FALSE instruction above.
                self.patch_jump(skip_if);

                // This is either the start of the `else { ... }` branch or the
                // start of whatever code we have after an `if { ... }` branch
                // without an `else` statement. Either way, we still have to
                // discard the condition value.
                self.emit(Instruction::Pop(1));

                // Compile the `else { ... }` branch if it exists.
                if let Some(else_branch) = else_branch {
                    self.compile_expression(else_branch);
                }

                // Patch the skip else jump. If there's no else, this will
                // simply skip the POP above, because the if branch has its
                // own POP. We can simplify this stuff by creating a specialized
                // POP_JUMP instruction like Python does, but for now I want
                // the simplest possible VM (very limited instructions).
                self.patch_jump(skip_else);
            }

            hir::Expression::ExpressionBlock(block, _) => {
                self.compile_block(block);
            }

            hir::Expression::BinaryOperation {
                left,
                operator,
                right,
                ..
            } => {
                self.compile_expression(left);

                // Logical operators must short-circuit. They are implemented
                // in terms of jump instructions, there is no special VM
                // instruction for logical AND / OR.
                match operator {
                    hir::BinaryOperator::And => {
                        let skip_right = self.emit(Instruction::JumpIfFalse(0));
                        self.emit(Instruction::Pop(1));
                        self.compile_expression(right);
                        self.patch_jump(skip_right);
                    }

                    hir::BinaryOperator::Or => {
                        let eval_right = self.emit(Instruction::JumpIfFalse(0));
                        let skip_right = self.emit(Instruction::Jump(0));

                        self.patch_jump(eval_right);

                        self.emit(Instruction::Pop(1));
                        self.compile_expression(right);

                        self.patch_jump(skip_right);
                    }

                    other => {
                        self.compile_expression(right);

                        self.emit(match other {
                            hir::BinaryOperator::Add => Instruction::BinOp(BinOp::Add),
                            hir::BinaryOperator::Sub => Instruction::BinOp(BinOp::Sub),
                            hir::BinaryOperator::Mul => Instruction::BinOp(BinOp::Mul),
                            hir::BinaryOperator::Div => Instruction::BinOp(BinOp::Div),

                            hir::BinaryOperator::Eq => Instruction::CmpOp(CmpOp::Eq),
                            hir::BinaryOperator::Neq => Instruction::CmpOp(CmpOp::NotEq),
                            hir::BinaryOperator::Lt => Instruction::CmpOp(CmpOp::Lt),
                            hir::BinaryOperator::LtEq => Instruction::CmpOp(CmpOp::LtEq),
                            hir::BinaryOperator::Gt => Instruction::CmpOp(CmpOp::Gt),
                            hir::BinaryOperator::GtEq => Instruction::CmpOp(CmpOp::GtEq),

                            hir::BinaryOperator::And | hir::BinaryOperator::Or => unreachable!(
                                "compiler bug: logical binary operators must be handled before arithmetic and comparison operators"
                            ),
                        });
                    }
                }
            }

            hir::Expression::UnaryOperation { operator, expr, .. } => {
                self.compile_expression(expr);

                self.emit(match operator {
                    hir::UnaryOperator::Not => Instruction::UnaryOp(UnaryOp::Not),
                    hir::UnaryOperator::Neg => Instruction::UnaryOp(UnaryOp::Neg),
                });
            }

            hir::Expression::Paren(expr, _) => {
                self.compile_expression(expr);
            }
        }
    }

    /// Emits a single instruction and returns the index of the instruction.
    ///
    /// The return value is useful when we want to modify an instruction that
    /// we've already emitted. Take a look at how we compile if statements in
    /// the [`Self::compile_expression`] function.
    fn emit(&mut self, instruction: Instruction) -> usize {
        let index = self.bytecode.instructions.len();

        self.bytecode.instructions.push(instruction);
        self.bytecode.source_lines.push(self.current_source_line);

        let scope = self.scopes.last().expect(
            "compiler bug: attempt to read scope ID of instruction when scope stack is empty",
        );

        self.bytecode.scopes.push(scope.id);

        index
    }

    /// Adds a new constant to the constants pool and returns its index.
    fn add_constant(&mut self, value: Value) -> usize {
        self.bytecode.constants.push(value);
        self.bytecode.constants.len() - 1
    }

    /// Patches a jump instruction to point to the correct destination.
    ///
    /// When we first emit a jump instruction, we do not know what offset to use
    /// because we don't know how many instructions the block we want to jump
    /// over will emit. In order to solve that, we emit the jump instruction
    /// with a placeholder offset (like 0), then we compile the jump target,
    /// and finally we call this function passing the index of the jump
    /// instruction to adjust the offset and make it point to the end of the
    /// target block.
    fn patch_jump(&mut self, instruction_ptr: usize) {
        let destination = self.bytecode.instructions.len();

        match &mut self.bytecode.instructions[instruction_ptr] {
            Instruction::Jump(offset) | Instruction::JumpIfFalse(offset) => {
                *offset = (destination - instruction_ptr) as isize;
            }
            _ => panic!(
                "compiler bug: expected jump instruction at index {instruction_ptr}, but got {:?}",
                self.bytecode.instructions[instruction_ptr]
            ),
        }
    }

    /// Keeps track of a new local and returns its index in the eval stack.
    fn track_local(&mut self, name: &str) -> usize {
        let index = self.locals.len() + 1;
        self.locals.insert(name.to_string(), index);

        self.scopes
            .last_mut()
            .expect("compiler bug: attempt to track local when scope stack is empty")
            .locals
            .insert(name.to_string());

        index
    }

    /// Creates and enters a new block scope.
    fn enter_scope(&mut self) {
        self.scopes.push(Scope {
            depth: self.scopes.len(),
            locals: HashSet::new(),
            id: self.locals_in_scope.len(),
        });

        self.locals_in_scope.push(HashMap::new());
    }

    /// Drops the current block scope we're in.
    fn exit_scope(&mut self, scope_has_ending_expr: bool) {
        let scope = self
            .scopes
            .pop()
            .expect("compiler bug: attempt to exit scope when scope stack is empty");

        self.locals_in_scope[scope.id] = self.locals.clone();

        // Depth 0 is function body block. That one ends with return.
        if scope.depth >= 1 && !scope.locals.is_empty() {
            // Keep value on top of stack if block has a return expression.
            // Otherwise just pop locals.
            if scope_has_ending_expr {
                self.emit(Instruction::PopReplace(scope.locals.len()));
            } else {
                self.emit(Instruction::Pop(scope.locals.len()));
            }

            for local in scope.locals {
                self.locals.remove(&local);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test::ast;

    /// Helper struct for testing bytecode compilation.
    struct Program {
        source: &'static str,
        expected: Vec<(&'static str, Vec<Instruction>)>,
    }

    /// Helper function to assert that source code compiles to expected bytecode
    /// instructions.
    fn assert_compiles(input: Program) -> anyhow::Result<()> {
        let ast = ast(input.source)?;

        let BamlVmProgram {
            objects, globals, ..
        } = compile(&ast)?;

        // Create a map of function name to function for easy lookup
        let functions: std::collections::HashMap<&str, &baml_vm::Function> = objects
            .iter()
            .filter_map(|obj| match obj {
                Object::Function(f) => Some((f.name.as_str(), f)),
                _ => None,
            })
            .collect();

        // Check each expected function
        for (function_name, expected_instructions) in input.expected {
            let function = functions
                .get(function_name)
                .ok_or_else(|| anyhow::anyhow!("function '{}' not found", function_name))?;

            eprintln!(
                "---- fn {function_name}() ----\n{}",
                baml_vm::debug::display_bytecode(function, &[], &objects, &globals, true)
            );

            assert_eq!(
                function.bytecode.instructions, expected_instructions,
                "Bytecode mismatch for function '{function_name}'"
            );
        }

        Ok(())
    }

    #[test]
    fn return_function_call() -> anyhow::Result<()> {
        assert_compiles(Program {
            source: "
                fn one() -> int {
                    1
                }

                fn main() -> int {
                    one()
                }
            ",
            expected: vec![
                ("one", vec![Instruction::LoadConst(0), Instruction::Return]),
                (
                    "main",
                    vec![
                        Instruction::LoadGlobal(0),
                        Instruction::Call(0),
                        Instruction::Return,
                    ],
                ),
            ],
        })
    }

    #[test]
    fn call_function() -> anyhow::Result<()> {
        assert_compiles(Program {
            source: "
                fn two() -> int {
                    2
                }

                fn main() -> int {
                    let a = two();
                    a
                }
            ",
            expected: vec![
                ("two", vec![Instruction::LoadConst(0), Instruction::Return]),
                (
                    "main",
                    vec![
                        Instruction::LoadGlobal(0),
                        Instruction::Call(0),
                        Instruction::LoadVar(1),
                        Instruction::Return,
                    ],
                ),
            ],
        })
    }

    #[test]
    fn if_else_return_expr() -> anyhow::Result<()> {
        assert_compiles(Program {
            source: "
                fn main(b: bool) -> int {
                    if b { 1 } else { 2 }
                }
            ",
            expected: vec![(
                "main",
                vec![
                    Instruction::LoadVar(1),
                    Instruction::JumpIfFalse(4),
                    Instruction::Pop(1),
                    Instruction::LoadConst(0),
                    Instruction::Jump(3),
                    Instruction::Pop(1),
                    Instruction::LoadConst(1),
                    Instruction::Return,
                ],
            )],
        })
    }

    #[test]
    fn if_else_return_expr_with_locals() -> anyhow::Result<()> {
        assert_compiles(Program {
            source: "
                fn main(b: bool) -> int {
                    if b {
                        let a = 1;
                        a
                    } else {
                        let a = 2;
                        a
                    }
                }
            ",
            expected: vec![(
                "main",
                vec![
                    Instruction::LoadVar(1),
                    Instruction::JumpIfFalse(6),
                    Instruction::Pop(1),
                    Instruction::LoadConst(0),
                    Instruction::LoadVar(2),
                    Instruction::PopReplace(1),
                    Instruction::Jump(5),
                    Instruction::Pop(1),
                    Instruction::LoadConst(1),
                    Instruction::LoadVar(2),
                    Instruction::PopReplace(1),
                    Instruction::Return,
                ],
            )],
        })
    }

    #[test]
    fn if_else_assignment() -> anyhow::Result<()> {
        assert_compiles(Program {
            source: "
                fn main(b: bool) -> int {
                    let i = if b { 1 } else { 2 };
                    i
                }
            ",
            expected: vec![(
                "main",
                vec![
                    Instruction::LoadVar(1),
                    Instruction::JumpIfFalse(4),
                    Instruction::Pop(1),
                    Instruction::LoadConst(0),
                    Instruction::Jump(3),
                    Instruction::Pop(1),
                    Instruction::LoadConst(1),
                    Instruction::LoadVar(2),
                    Instruction::Return,
                ],
            )],
        })
    }

    #[test]
    fn if_else_assignment_with_locals() -> anyhow::Result<()> {
        assert_compiles(Program {
            source: "
                fn main(b: bool) -> int {
                    let i = if b {
                        let a = 1;
                        a
                    } else {
                        let a = 2;
                        a
                    };

                    i
                }
            ",
            expected: vec![(
                "main",
                vec![
                    Instruction::LoadVar(1),
                    Instruction::JumpIfFalse(6),
                    Instruction::Pop(1),
                    Instruction::LoadConst(0),
                    Instruction::LoadVar(2),
                    Instruction::PopReplace(1),
                    Instruction::Jump(5),
                    Instruction::Pop(1),
                    Instruction::LoadConst(1),
                    Instruction::LoadVar(2),
                    Instruction::PopReplace(1),
                    Instruction::LoadVar(2),
                    Instruction::Return,
                ],
            )],
        })
    }

    #[test]
    fn if_else_normal_statement() -> anyhow::Result<()> {
        assert_compiles(Program {
            source: "
                fn identity(i: int) -> int {
                    i
                }

                fn main(b: bool) -> int {
                    let a = 1;

                    if b {
                        let x = 1;
                        let y = 2;
                        identity(x);
                    } else {
                        let x = 3;
                        let y = 4;
                        identity(y);
                    }

                    a
                }
            ",
            expected: vec![(
                "main",
                vec![
                    Instruction::LoadConst(0),
                    Instruction::LoadVar(1),
                    Instruction::JumpIfFalse(10),
                    Instruction::Pop(1),
                    Instruction::LoadConst(1),
                    Instruction::LoadConst(2),
                    Instruction::LoadGlobal(0),
                    Instruction::LoadVar(3),
                    Instruction::Call(1),
                    Instruction::Pop(1),
                    Instruction::Pop(2),
                    Instruction::Jump(9),
                    Instruction::Pop(1),
                    Instruction::LoadConst(3),
                    Instruction::LoadConst(4),
                    Instruction::LoadGlobal(0),
                    Instruction::LoadVar(4),
                    Instruction::Call(1),
                    Instruction::Pop(1),
                    Instruction::Pop(2),
                    Instruction::LoadVar(2),
                    Instruction::Return,
                ],
            )],
        })
    }

    #[test]
    fn else_if_return_expr() -> anyhow::Result<()> {
        assert_compiles(Program {
            source: "
                fn main(a: bool, b: bool) -> int {
                    if a {
                        1
                    } else if b {
                        2
                    } else {
                        3
                    }
                }
            ",
            expected: vec![(
                "main",
                vec![
                    Instruction::LoadVar(1),
                    Instruction::JumpIfFalse(4),
                    Instruction::Pop(1),
                    Instruction::LoadConst(0),
                    Instruction::Jump(9),
                    Instruction::Pop(1),
                    Instruction::LoadVar(2),
                    Instruction::JumpIfFalse(4),
                    Instruction::Pop(1),
                    Instruction::LoadConst(1),
                    Instruction::Jump(3),
                    Instruction::Pop(1),
                    Instruction::LoadConst(2),
                    Instruction::Return,
                ],
            )],
        })
    }

    #[test]
    fn else_if_return_expr_with_locals() -> anyhow::Result<()> {
        assert_compiles(Program {
            source: "
                fn main(a: bool, b: bool) -> int {
                    if a {
                        let x = 1;
                        x
                    } else if b {
                        let y = 2;
                        y
                    } else {
                        let z = 3;
                        z
                    }
                }
            ",
            expected: vec![(
                "main",
                vec![
                    Instruction::LoadVar(1),
                    Instruction::JumpIfFalse(6),
                    Instruction::Pop(1),
                    Instruction::LoadConst(0),
                    Instruction::LoadVar(3),
                    Instruction::PopReplace(1),
                    Instruction::Jump(13),
                    Instruction::Pop(1),
                    Instruction::LoadVar(2),
                    Instruction::JumpIfFalse(6),
                    Instruction::Pop(1),
                    Instruction::LoadConst(1),
                    Instruction::LoadVar(3),
                    Instruction::PopReplace(1),
                    Instruction::Jump(5),
                    Instruction::Pop(1),
                    Instruction::LoadConst(2),
                    Instruction::LoadVar(3),
                    Instruction::PopReplace(1),
                    Instruction::Return,
                ],
            )],
        })
    }

    #[test]
    fn else_if_assignment() -> anyhow::Result<()> {
        assert_compiles(Program {
            source: "
                fn main(a: bool, b: bool) -> int {
                    let result = if a {
                        1
                    } else if b {
                        2
                    } else {
                        3
                    };

                    result
                }
            ",
            expected: vec![(
                "main",
                vec![
                    Instruction::LoadVar(1),
                    Instruction::JumpIfFalse(4),
                    Instruction::Pop(1),
                    Instruction::LoadConst(0),
                    Instruction::Jump(9),
                    Instruction::Pop(1),
                    Instruction::LoadVar(2),
                    Instruction::JumpIfFalse(4),
                    Instruction::Pop(1),
                    Instruction::LoadConst(1),
                    Instruction::Jump(3),
                    Instruction::Pop(1),
                    Instruction::LoadConst(2),
                    Instruction::LoadVar(3),
                    Instruction::Return,
                ],
            )],
        })
    }

    #[test]
    fn else_if_assignment_with_locals() -> anyhow::Result<()> {
        assert_compiles(Program {
            source: "
                fn main(a: bool, b: bool) -> int {
                    let result = if a {
                        let x = 1;
                        x
                    } else if b {
                        let y = 2;
                        y
                    } else {
                        let z = 3;
                        z
                    };

                    result
                }
            ",
            expected: vec![(
                "main",
                vec![
                    Instruction::LoadVar(1),
                    Instruction::JumpIfFalse(6),
                    Instruction::Pop(1),
                    Instruction::LoadConst(0),
                    Instruction::LoadVar(3),
                    Instruction::PopReplace(1),
                    Instruction::Jump(13),
                    Instruction::Pop(1),
                    Instruction::LoadVar(2),
                    Instruction::JumpIfFalse(6),
                    Instruction::Pop(1),
                    Instruction::LoadConst(1),
                    Instruction::LoadVar(3),
                    Instruction::PopReplace(1),
                    Instruction::Jump(5),
                    Instruction::Pop(1),
                    Instruction::LoadConst(2),
                    Instruction::LoadVar(3),
                    Instruction::PopReplace(1),
                    Instruction::LoadVar(3),
                    Instruction::Return,
                ],
            )],
        })
    }

    #[test]
    fn array_constructor() -> anyhow::Result<()> {
        assert_compiles(Program {
            source: "
                fn main() -> int[] {
                    let a = [1, 2, 3];
                    a
                }
            ",
            expected: vec![(
                "main",
                vec![
                    Instruction::LoadConst(0),
                    Instruction::LoadConst(1),
                    Instruction::LoadConst(2),
                    Instruction::AllocArray(3),
                    Instruction::LoadVar(1),
                    Instruction::Return,
                ],
            )],
        })
    }

    #[test]
    fn class_constructor() -> anyhow::Result<()> {
        assert_compiles(Program {
            source: "
                class Point {
                    x int
                    y int
                }

                fn main() -> Point {
                    let p = Point { x: 1, y: 2 };
                    p
                }
            ",
            expected: vec![(
                "main",
                vec![
                    Instruction::AllocInstance(2),
                    Instruction::LoadConst(0),
                    Instruction::StoreField(0),
                    Instruction::LoadConst(1),
                    Instruction::StoreField(1),
                    Instruction::LoadVar(1),
                    Instruction::Return,
                ],
            )],
        })
    }

    #[test]
    #[ignore = "HIR doesn't support spread operators yet"]
    fn class_constructor_with_spread_operator() -> anyhow::Result<()> {
        assert_compiles(Program {
            source: r#"
                class Point {
                    x int
                    y int
                    z int
                }

                fn default_point() -> Point {
                    Point { x: 0, y: 0, z: 0 }
                }

                fn main() -> Point {
                    let p = Point { x: 1, y: 2, ..default_point() };
                    p
                }
            "#,
            expected: vec![(
                "main",
                vec![
                    Instruction::AllocInstance(2),
                    Instruction::LoadConst(0),
                    Instruction::StoreField(0),
                    Instruction::LoadConst(1),
                    Instruction::StoreField(1),
                    Instruction::LoadGlobal(0),
                    Instruction::Call(0),
                    Instruction::LoadVar(1),
                    Instruction::LoadVar(2),
                    Instruction::LoadField(2),
                    Instruction::StoreField(2),
                    Instruction::LoadVar(1),
                    Instruction::Return,
                ],
            )],
        })
    }

    #[test]
    fn function_returning_string() -> anyhow::Result<()> {
        assert_compiles(Program {
            source: r#"
                fn main() -> string {
                    "hello"
                }
            "#,
            expected: vec![("main", vec![Instruction::LoadConst(0), Instruction::Return])],
        })
    }

    #[test]
    fn block_expr() -> anyhow::Result<()> {
        assert_compiles(Program {
            source: "
                fn main() -> int {
                    let a = {
                        let b = 1;
                        b
                    };

                    a
                }
            ",
            expected: vec![(
                "main",
                vec![
                    Instruction::LoadConst(0),
                    Instruction::LoadVar(1),
                    Instruction::PopReplace(1),
                    Instruction::LoadVar(1),
                    Instruction::Return,
                ],
            )],
        })
    }

    #[test]
    fn locals_in_scope() -> anyhow::Result<()> {
        let ast = ast(r#"
            fn main() -> int {
                let x = 0;

                let a = {
                    let y = 0;

                    let b = {
                        let c = 1;
                        let d = 2;
                        [c, d]
                    };
                    let e = {
                        let f = 4;
                        let g = 5;
                        [f, g]
                    };

                    [b, e]
                };

                let h = {
                    let z = 0;

                    let i = {
                        let w = 0;
                        let j = 8;
                        [w, j]
                    };

                    [i]
                };

                [a, h]
            }
        "#)?;

        let BamlVmProgram {
            objects,
            resolved_function_names,
            globals,
            ..
        } = compile(&ast)?;

        let main = objects[resolved_function_names["main"].0].as_function()?;
        baml_vm::debug::disassemble(main, &[], &objects, &globals);

        let expected_locals_in_scope = [
            vec!["<fn main>", "x", "a", "h"],
            vec!["<fn main>", "x", "y", "b", "e"],
            vec!["<fn main>", "x", "y", "c", "d"],
            vec!["<fn main>", "x", "y", "b", "f", "g"],
            vec!["<fn main>", "x", "a", "z", "i"],
            vec!["<fn main>", "x", "a", "z", "w", "j"],
        ];

        assert_eq!(
            main.locals_in_scope,
            expected_locals_in_scope
                .iter()
                .map(|scope| scope.iter().map(ToString::to_string).collect::<Vec<_>>())
                .collect::<Vec<_>>()
        );

        Ok(())
    }

    #[test]
    fn mutable_variables() -> anyhow::Result<()> {
        assert_compiles(Program {
            source: r#"
                fn DeclareMutableInFunction(x: int) -> int {

                    let mut y = 3;

                    y = 5;

                    y
                }

                fn MutableInArg(mut x: int) -> int {
                    x = 3;
                    x
                }
            "#,
            expected: vec![
                (
                    "DeclareMutableInFunction",
                    vec![
                        Instruction::LoadConst(0),
                        Instruction::LoadConst(1),
                        Instruction::StoreVar(2),
                        Instruction::LoadVar(2),
                        Instruction::Return,
                    ],
                ),
                (
                    "MutableInArg",
                    vec![
                        Instruction::LoadConst(0),
                        Instruction::StoreVar(1),
                        Instruction::LoadVar(1),
                        Instruction::Return,
                    ],
                ),
            ],
        })
    }

    #[test]
    fn basic_and() -> anyhow::Result<()> {
        assert_compiles(Program {
            source: r#"
                fn ret_bool() -> bool {
                    true
                }

                fn main() -> bool {
                    true && ret_bool()
                }
            "#,
            expected: vec![(
                "main",
                vec![
                    Instruction::LoadConst(0),
                    Instruction::JumpIfFalse(4),
                    Instruction::Pop(1),
                    Instruction::LoadGlobal(0),
                    Instruction::Call(0),
                    Instruction::Return,
                ],
            )],
        })
    }

    #[test]
    fn basic_or() -> anyhow::Result<()> {
        assert_compiles(Program {
            source: r#"
                fn ret_bool() -> bool {
                    true
                }

                fn main() -> bool {
                    true || ret_bool()
                }
            "#,
            expected: vec![(
                "main",
                vec![
                    Instruction::LoadConst(0),
                    Instruction::JumpIfFalse(2),
                    Instruction::Jump(4),
                    Instruction::Pop(1),
                    Instruction::LoadGlobal(0),
                    Instruction::Call(0),
                    Instruction::Return,
                ],
            )],
        })
    }

    #[test]
    fn basic_add() -> anyhow::Result<()> {
        assert_compiles(Program {
            source: r#"
                fn main() -> int {
                    let a = 1 + 2;
                    a
                }
            "#,
            expected: vec![(
                "main",
                vec![
                    Instruction::LoadConst(0),
                    Instruction::LoadConst(1),
                    Instruction::BinOp(BinOp::Add),
                    Instruction::LoadVar(1),
                    Instruction::Return,
                ],
            )],
        })
    }
}
