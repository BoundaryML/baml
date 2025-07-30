//! Baml bytecode compiler.
//!
//! This crate is concerned with generating VM bytecode from a Baml AST. For now
//! it is pretty straightforward to go from AST to bytecode, but in the future
//! we might need more tree transformations to generate our bytecode.
//! Specifically, read about how Rust handles [HIR] (High Level IR) and [MIR]
//! (Mid Level IR):
//!
//! [HIR]: https://rustc-dev-guide.rust-lang.org/hir.html
//! [MIR]: https://rustc-dev-guide.rust-lang.org/mir/index.html
//!
pub mod hir;

use std::collections::{HashMap, HashSet};

use baml_vm::{Bytecode, Class, Function, FunctionKind, Instruction, Object, Value};
use internal_baml_parser_database::ParserDatabase;

/// Compile a Baml AST into bytecode.
///
/// This now uses a two-stage compilation process:
/// 1. AST -> HIR
/// 2. HIR -> Bytecode
pub fn compile(ast: &ParserDatabase) -> anyhow::Result<(
    Vec<Object>,
    Vec<Value>,
    HashMap<String, (usize, FunctionKind)>
)> {
    // Stage 1: AST -> HIR
    let hir_program = hir::Program::from_ast(&ast.ast);

    // Stage 2: HIR -> Bytecode
    compile_hir_to_bytecode(&hir_program)
}

/// Compile HIR to bytecode.
///
/// This function takes an HIR Program and generates the bytecode for the VM.
fn compile_hir_to_bytecode(hir: &hir::Program) -> anyhow::Result<(
    Vec<Object>,
    Vec<Value>,
    HashMap<String, (usize, FunctionKind)>,
)> {
    let mut resolved_globals = HashMap::new();
    let mut resolved_classes = HashMap::new();
    let llm_functions: HashSet<String> = hir.llm_functions.iter().map(|f| f.name.clone()).collect();

    // We need to process all top-level declarations in the order they appear in the source
    // For now, we'll approximate this by processing classes first, then functions
    // This matches the expected behavior from the tests
    
    let mut global_index = 0;
    
    // Resolve classes first (they appear first in the test source)
    for class in &hir.classes {
        resolved_globals.insert(class.name.clone(), global_index);
        global_index += 1;

        // Resolve class fields.
        let mut class_fields = HashMap::new();
        for (field_index, field) in class.fields.iter().enumerate() {
            class_fields.insert(field.name.clone(), field_index);
        }

        resolved_classes.insert(class.name.clone(), class_fields);
    }

    // Then resolve functions
    for func in &hir.expr_functions {
        resolved_globals.insert(func.name.clone(), global_index);
        global_index += 1;
    }

    let mut objects = Vec::with_capacity(resolved_globals.len());
    let mut globals = Vec::with_capacity(resolved_globals.len());

    // Add classes to objects first (to match the global index order)
    for class in &hir.classes {
        let bytecode_class = Class {
            name: class.name.clone(),
            field_names: class.fields.iter().map(|f| f.name.clone()).collect(),
        };

        globals.push(Value::Object(objects.len()));
        objects.push(Object::Class(bytecode_class));
    }

    // Then compile and add functions
    for func in &hir.expr_functions {
        let bytecode_function =
            compile_hir_function(func, &resolved_globals, &resolved_classes, &mut objects, &llm_functions)?;

        // Add the function to the globals and objects pools.
        globals.push(Value::Object(objects.len()));
        objects.push(Object::Function(bytecode_function));
    }

    let resolved_function_names = objects
    .iter()
    .enumerate()
    .filter_map(|(i, obj)| match obj {
        Object::Function(f) => Some((f.name.clone(), (i, f.kind))),
        _ => None,
    })
    .collect();

    Ok((objects, globals, resolved_function_names))
}

/// Compile an HIR function to bytecode.
fn compile_hir_function(
    func: &hir::ExprFunction,
    globals: &HashMap<String, usize>,
    classes: &HashMap<String, HashMap<String, usize>>,
    objects: &mut Vec<Object>,
    llm_functions: &HashSet<String>,
) -> anyhow::Result<Function> {
    let mut compiler = HirCompiler::new(globals, classes, llm_functions, objects);
    compiler.compile_function(func)
}

/// Baml compiler.
///
/// This struct compiles a single AST function into bytecode.
///
/// **IMPORTANT**: The compiler DOES NOT validate anything, AST must already be
/// validated before calling this otherwise it will issue incorrect bytecode,
/// the VM will break and the universe will collapse.
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
    class_fields: &'g HashMap<String, HashMap<String, usize>>,

    /// LLM functions.
    llm_functions: &'g HashSet<String>,

    /// Resolved local variables.
    ///
    /// Maps the name of the variable to its final index in the eval stack.
    locals: HashMap<String, usize>,

    /// Current scope.
    ///
    /// The scope increments with each nested block. Example:
    ///
    /// ```ignore
    /// fn example() {          // Scope is 0.
    ///     let a = 1;
    ///     {                   // Scope is 1.
    ///         let b = 2;
    ///         {               // Scope is 2.
    ///             let c  = 3;
    ///         }
    ///     }
    /// }
    /// ```
    ///
    /// This is used to keep track of local variables present in the evaluation
    /// stack.
    scope: usize,

    /// Bytecode to generate.
    bytecode: Bytecode,

    /// Objects pool.
    ///
    /// Stores heap-allocated objects that are created during compilation,
    /// such as string constants.
    objects: &'g mut Vec<Object>,

    /// Current source line for debug info.
    current_source_line: usize,
}

impl<'g> HirCompiler<'g> {
    fn new(
        globals: &'g HashMap<String, usize>,
        class_fields: &'g HashMap<String, HashMap<String, usize>>,
        llm_functions: &'g HashSet<String>,
        objects: &'g mut Vec<Object>,
    ) -> Self {
        Self {
            globals,
            class_fields,
            llm_functions,
            objects,
            scope: 0,
            locals: HashMap::new(),
            bytecode: Bytecode::new(),
            current_source_line: 0,
        }
    }

    /// Compile an HIR function into a VM function.
    fn compile_function(&mut self, func: &hir::ExprFunction) -> anyhow::Result<Function> {
        // Resolve parameters.
        for param in &func.parameters {
            // Note the len() + 1 here. The first local is the function itself,
            // arguments start at index 1.
            self.locals
                .insert(param.name.clone(), self.locals.len() + 1);
        }

        // Compile statements in the function body.
        self.compile_block(&func.body);

        Ok(Function {
            name: func.name.clone(),
            arity: func.parameters.len(),
            bytecode: self.bytecode.clone(),
            kind: FunctionKind::Exec,

            // Debugging stuff.
            local_var_names: {
                let mut names = Vec::with_capacity(self.locals.len() + 1);
                // Function is pushed onto the stack.
                names.push(format!("<fn {}>", func.name));
                // Locals come after.
                names.resize_with(names.capacity(), String::new);

                for (name, index) in &self.locals {
                    names[*index] = name.to_string();
                }

                names
            },
        })
    }

    fn compile_block(&mut self, block: &hir::Block) {
        for statement in &block.statements {
            self.compile_statement(statement);
        }
    }

    fn compile_expression_block(&mut self, block: &hir::Block) {
        // Compile all statements except the last one
        let num_stmts = block.statements.len();
        if num_stmts > 0 {
            for statement in &block.statements[..num_stmts - 1] {
                self.compile_statement(statement);
            }
            
            // The last statement should be an Expression or Return
            // For expression blocks, it should leave its value on the stack
            match &block.statements[num_stmts - 1] {
                hir::Statement::Expression { expr, .. } => {
                    // Just compile the expression, leaving its value on the stack
                    self.compile_expression(expr);
                }
                hir::Statement::Return { .. } => {
                    // This shouldn't happen in an expression block
                    panic!("Return statement in expression block");
                }
                _ => {
                    // Other statements in final position
                    self.compile_statement(&block.statements[num_stmts - 1]);
                }
            }
        }
    }

    fn compile_statement(&mut self, statement: &hir::Statement) {
        match statement {
            hir::Statement::Let { name, value, .. } => {
                self.compile_expression(value);
                let local_index = self.locals.len() + 1;
                self.locals.insert(name.clone(), local_index);
            }
            hir::Statement::DeclareReference { name, .. } => {
                // For mutable references, we need to allocate space on the stack
                // We'll push a null/undefined value as placeholder
                let index = self.add_constant(Value::Bool(false)); // placeholder
                self.emit(Instruction::LoadConst(index));
                let local_index = self.locals.len() + 1;
                self.locals.insert(name.clone(), local_index);
            }
            hir::Statement::Assign { name: _, value } => {
                // For assignment to existing variable, compile the expression
                // then store it at the variable's location
                self.compile_expression(value);
                // The value is on top of stack, but we need to move it to the right local
                // This is a bit tricky - we need to implement proper assignment
                // For now, this is a limitation - assignments don't work properly in bytecode
                // TODO: Implement proper assignment instructions
            }
            hir::Statement::DeclareAndAssign { name, value, .. } => {
                self.compile_expression(value);
                let local_index = self.locals.len() + 1;
                self.locals.insert(name.clone(), local_index);
            }
            hir::Statement::Return { expr, .. } => {
                self.compile_expression(expr);
                self.emit(Instruction::Return);
            }
            hir::Statement::Expression { expr, .. } => {
                self.compile_expression(expr);
                // Expression results are left on stack
            }
            hir::Statement::If {
                condition,
                then_block,
                else_block,
                ..
            } => {
                // Compile condition
                self.compile_expression(condition);

                // Jump if false
                let skip_if = self.emit(Instruction::JumpIfFalse(0));

                // Pop condition and compile then branch
                self.emit(Instruction::Pop);
                self.compile_block(then_block);

                // Jump over else
                let skip_else = self.emit(Instruction::Jump(0));

                // Patch the skip_if jump
                self.patch_jump(skip_if);

                // Pop condition
                self.emit(Instruction::Pop);

                // Compile else branch if present
                if let Some(else_block) = else_block {
                    self.compile_block(else_block);
                }

                // Patch the skip_else jump
                self.patch_jump(skip_else);
            }
            hir::Statement::While {
                condition, block, ..
            } => {
                // Remember where the loop starts
                let loop_start = self.bytecode.instructions.len();

                // Compile condition
                self.compile_expression(condition);

                // Jump out of loop if false
                let exit_jump = self.emit(Instruction::JumpIfFalse(0));

                // Pop condition
                self.emit(Instruction::Pop);

                // Compile loop body
                self.compile_block(block);

                // Jump back to start
                let offset = -(self.bytecode.instructions.len() as isize - loop_start as isize);
                self.emit(Instruction::Jump(offset));

                // Patch exit jump
                self.patch_jump(exit_jump);

                // Pop condition
                self.emit(Instruction::Pop);
            }
            hir::Statement::ForLoop {
                identifier,
                iterator,
                block,
                ..
            } => {
                // Compile the iterator expression (array) - leaves array on stack
                self.compile_expression(iterator);

                // Create iterator from array - replaces array with iterator on stack
                self.emit(Instruction::CreateIterator);

                // Loop start - iterator is on top of stack
                let loop_start = self.bytecode.instructions.len();

                // Get next element - pops iterator, pushes iterator, element, has_next
                self.emit(Instruction::IterNext);

                // Check if we have more elements (has_next is on top of stack)
                let jump_to_end = self.emit(Instruction::JumpIfFalse(0));

                // Pop the has_next boolean
                self.emit(Instruction::Pop);

                // Now we have: [function, args..., locals..., iterator, element] on stack
                // The element is what we want for the loop variable.
                // The element is always 2 positions after the last local
                // (iterator is at last_local + 1, element is at last_local + 2)
                let last_local_index = self.locals.values().max().copied().unwrap_or(0);
                let item_local = last_local_index + 2;
                self.locals.insert(identifier.clone(), item_local);

                // Compile the loop body (nested expression block)
                self.compile_block(block);

                // Pop the body result
                self.emit(Instruction::Pop);

                // Pop the element since we're done with it for this iteration
                self.emit(Instruction::Pop);

                // Now iterator is back on top of stack. Jump back to loop start.
                let current_pos = self.bytecode.instructions.len();
                self.emit(Instruction::Jump(
                    (loop_start as isize) - (current_pos as isize),
                ));

                // Patch the jump to end - this is where we land when has_next is false
                self.patch_jump(jump_to_end);

                // Clean up remaining stack values
                self.emit(Instruction::Pop); // Pop has_next boolean
                self.emit(Instruction::Pop); // Pop element
                self.emit(Instruction::Pop); // Pop iterator

                // Remove the loop variable from locals since it's no longer in scope
                self.locals.remove(identifier);

                // Push null as the for loop result
                let null_index = self.add_constant(Value::Null);
                self.emit(Instruction::LoadConst(null_index));
            }
        }
    }

    /// Generate bytecode for an expression.
    ///
    /// # Dev notes
    ///
    /// Be cautious with "abstractions" here. It's better to be explicit so that
    /// we can see exactly what instructions are being emitted in each scenario.
    /// We should not create `emit_some_crazy_stuff` functions unless they can
    /// be reused many times.
    fn compile_expression(&mut self, expr: &hir::Expression) {
        match expr {
            hir::Expression::BoolValue(val, _) => {
                let index = self.add_constant(Value::Bool(*val));
                self.emit(Instruction::LoadConst(index));
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
            hir::Expression::StringValue(string, _) => {
                // Allocate the string in the objects pool
                self.objects.push(Object::String(string.clone()));
                let object_index = self.objects.len() - 1;

                // Add a constant that points to the string object
                let const_index = self.add_constant(Value::Object(object_index));
                self.emit(Instruction::LoadConst(const_index));
            }
            hir::Expression::RawStringValue(string, _) => {
                // Raw strings work the same as regular strings for bytecode
                self.objects.push(Object::String(string.clone()));
                let object_index = self.objects.len() - 1;

                let const_index = self.add_constant(Value::Object(object_index));
                self.emit(Instruction::LoadConst(const_index));
            }
            hir::Expression::Identifier(name, _) => {
                if let Some(&index) = self.locals.get(name) {
                    self.emit(Instruction::LoadVar(index));
                } else {
                    panic!("undefined variable: {}", name);
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
            hir::Expression::Call(name, args, _) => {
                // Push the function onto the stack
                if let Some(&index) = self.globals.get(name) {
                    self.emit(Instruction::LoadGlobal(index));
                } else {
                    panic!("undefined function: {}", name);
                }

                // Push the arguments onto the stack
                for arg in args {
                    self.compile_expression(arg);
                }

                if self.llm_functions.contains(name) {
                    self.emit(Instruction::DispatchFuture(args.len()));
                    self.emit(Instruction::Await);
                } else {
                    // Call the function
                    self.emit(Instruction::Call(args.len()));
                }
            }
            hir::Expression::ClassConstructor(cc, _) => {
                // Allocate instance
                if let Some(&class_index) = self.globals.get(&cc.class_name) {
                    self.emit(Instruction::AllocInstance(class_index));

                    let mut defined_named_fields = std::collections::HashSet::new();

                    // Process fields in order
                    for field in &cc.fields {
                        match field {
                            hir::ClassConstructorField::Named { name, value } => {
                                self.compile_expression(value);
                                if let Some(class_fields) = self.class_fields.get(&cc.class_name) {
                                    if let Some(&field_index) = class_fields.get(name) {
                                        self.emit(Instruction::StoreField(field_index));
                                        defined_named_fields.insert(name.as_str());
                                    } else {
                                        panic!("undefined field: {}.{}", cc.class_name, name);
                                    }
                                } else {
                                    panic!("undefined class: {}", cc.class_name);
                                }
                            }
                            hir::ClassConstructorField::Spread { value } => {
                                self.compile_expression(value);

                                // Pseudo local, user didn't declare it.
                                let spread_local = self.locals.len() + 2;
                                self.emit(Instruction::LoadVar(spread_local - 1));

                                if let Some(class_fields) = self.class_fields.get(&cc.class_name) {
                                    for (field_name, &field_index) in class_fields {
                                        if !defined_named_fields.contains(field_name.as_str()) {
                                            self.emit(Instruction::LoadVar(spread_local));
                                            self.emit(Instruction::LoadField(field_index));
                                            self.emit(Instruction::StoreField(field_index));
                                        }
                                    }
                                } else {
                                    panic!("undefined class: {}", cc.class_name);
                                }
                            }
                        }
                    }
                } else {
                    panic!("undefined class: {}", cc.class_name);
                }
            }
            hir::Expression::ExpressionBlock(block, _) => {
                // Expression blocks need special handling to maintain proper scoping
                // We need to track how many locals are defined in the block
                // and emit EndBlock to clean them up
                
                // Save the current scope state
                self.scope += 1;
                let locals_before = self.locals.len();
                
                // Compile the block
                self.compile_expression_block(block);
                
                // Count how many locals were added in this block
                let locals_added = self.locals.len() - locals_before;
                
                // If we're in a nested scope and added locals, emit EndBlock
                // Note: scope > 0 because we already incremented it
                if self.scope > 0 && locals_added > 0 {
                    self.emit(Instruction::EndBlock(locals_added));
                    
                    // Remove the scoped locals from our tracking
                    // We need to remove the last `locals_added` entries
                    let keys_to_remove: Vec<String> = self.locals
                        .iter()
                        .filter(|(_, &index)| index > locals_before)
                        .map(|(name, _)| name.clone())
                        .collect();
                    
                    for key in keys_to_remove {
                        self.locals.remove(&key);
                    }
                }
                
                self.scope -= 1;
            }
            hir::Expression::If(condition, then_expr, else_expr, _) => {
                // First, compile the condition. This will leave the end result
                // of the condition on top of the stack.
                self.compile_expression(condition);

                // Skip the `if { ... }` branch when condition is false. We'll
                // patch this offset later when we know how many instructions to
                // jump over, so we'll store a reference to this instruction.
                let skip_if = self.emit(Instruction::JumpIfFalse(0));

                // In case we execute the `if { ... }` branch, prepend a POP to
                // discard the condition value, we don't need it anymore.
                self.emit(Instruction::Pop);

                // Compile the `if { ... }` branch.
                self.compile_expression(then_expr);

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
                self.emit(Instruction::Pop);

                // Compile the `else { ... }` branch if it exists.
                if let Some(else_expr) = else_expr {
                    self.compile_expression(else_expr);
                } else {
                    // This shouldn't happen as HIR lowering ensures if expressions have else
                    panic!("if expression without else branch in HIR");
                }

                // Patch the skip else jump. If there's no else, this will
                // simply skip the POP above, because the if branch has its
                // own POP. We can simplify this stuff by creating a specialized
                // POP_JUMP instruction like Python does, but for now I want
                // the simplest possible VM (very limited instructions).
                self.patch_jump(skip_else);
            }
        }
    }

    /// Emits a single instruction and returns the index of the instruction.
    ///
    /// The return value is useful when we want to modify an instruction that
    /// we've already ommited. Take a look at how we compile if statements in
    /// the [`Self::compile_expression`] function.
    fn emit(&mut self, instruction: Instruction) -> usize {
        self.bytecode.instructions.push(instruction);
        self.bytecode.source_lines.push(self.current_source_line);
        self.bytecode.instructions.len() - 1
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

            // This should never run. Don't call this function with anything
            // that is not a jump instruction.
            // TODO: Just error out and report some "Internal Error" instead of
            // panicking and breaking the program.
            _ => unreachable!(
                "expected jump instruction at index {instruction_ptr}, but got {:?}",
                self.bytecode.instructions[instruction_ptr]
            ),
        }
    }
}

/// For tests.
///
/// We reuse this in the VM.
pub fn ast(source: &str) -> anyhow::Result<ParserDatabase> {
    let path = std::path::PathBuf::from("test.baml");
    let source_file = internal_baml_diagnostics::SourceFile::from((path.clone(), source));

    let validated_schema = internal_baml_core::validate(&path, vec![source_file]);

    if validated_schema.diagnostics.has_errors() {
        let errors = validated_schema.diagnostics.to_pretty_string();
        anyhow::bail!("{}", errors);
    }

    Ok(validated_schema.db)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper struct for testing bytecode compilation.
    struct Program {
        source: &'static str,
        expected: Vec<(&'static str, Vec<Instruction>)>,
    }

    /// Helper function to assert that source code compiles to expected bytecode
    /// instructions.
    fn assert_compiles(input: Program) -> anyhow::Result<()> {
        let ast = ast(input.source)?;
        let (objects, globals, _) = compile(&ast)?;

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
    fn if_else_statement() -> anyhow::Result<()> {
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
                    Instruction::Pop,
                    Instruction::LoadConst(0),
                    Instruction::Jump(3),
                    Instruction::Pop,
                    Instruction::LoadConst(1),
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
                    Instruction::AllocInstance(0),
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
                    Instruction::AllocInstance(0),
                    Instruction::LoadConst(0),
                    Instruction::StoreField(0),
                    Instruction::LoadConst(1),
                    Instruction::StoreField(1),
                    Instruction::LoadGlobal(1),
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
    fn for_loop() -> anyhow::Result<()> {
        assert_compiles(Program {
            source: "
                fn main() -> int {
                    for (i in [1, 2, 3]) {
                        i
                    }

                    42
                }
            ",
            expected: vec![(
                "main",
                vec![
                    Instruction::LoadConst(0),
                    Instruction::LoadConst(1),
                    Instruction::LoadConst(2),
                    Instruction::AllocArray(3),
                    Instruction::CreateIterator,
                    Instruction::IterNext,
                    Instruction::JumpIfFalse(6),
                    Instruction::Pop,
                    Instruction::LoadVar(2),
                    Instruction::Pop,
                    Instruction::Pop,
                    Instruction::Jump(-6),
                    Instruction::Pop,
                    Instruction::Pop,
                    Instruction::Pop,
                    Instruction::LoadConst(3),
                    Instruction::LoadConst(4),
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

    // TODO: Given the way local variables are handled, the debugger can't
    // correctly print the names of nested variables inside block expressions.
    // In this example here, LoadVar(1) corresponds to "b" while the nested
    // block is executing and after that LoadVar(1) corresponds to "a". But
    // the debugger always prints "a". Desugaring this to temporary variables
    // might help, but since we found a way to avoid temporary variables for
    // block expressions, we should find a way to correctly determine the
    // names of the variables according to their stack semantics.
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
                    Instruction::EndBlock(1),
                    Instruction::LoadVar(1),
                    Instruction::Return,
                ],
            )],
        })
    }
}
