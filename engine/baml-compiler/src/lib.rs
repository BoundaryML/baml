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

use std::collections::{HashMap, HashSet};

use baml_vm::{Bytecode, Class, Function, FunctionKind, Instruction, Object, Value};
use internal_baml_core::ast::{
    self, ClassConstructorField, Expression, ExpressionBlock, WithName, WithSpan,
};
use internal_baml_parser_database::ParserDatabase;

/// Baml compiler.
///
/// This struct compiles a single AST function into bytecode.
///
/// **IMPORTANT**: The compiler DOES NOT validate anything, AST must already be
/// validated before calling this otherwise it will issue incorrect bytecode,
/// the VM will break and the universe will collapse.
struct Compiler<'g> {
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

impl<'g> Compiler<'g> {
    /// Create a new compiler.
    pub fn new(
        globals: &'g HashMap<String, usize>,
        class_fields: &'g HashMap<String, HashMap<String, usize>>,
        objects: &'g mut Vec<Object>,
    ) -> Self {
        Self {
            globals,
            class_fields,
            objects,
            scope: 0,
            locals: HashMap::new(),
            bytecode: Bytecode::new(),
            current_source_line: 0,
        }
    }

    /// Compile AST function into VM function.
    pub fn compile(mut self, function: &ast::ExprFn) -> anyhow::Result<Function> {
        // Resolve parameters.
        for (param_name, _) in &function.args.args {
            // Note the len() + 1 here. The first local is the function itself,
            // arguments start at index 1.
            self.locals
                .insert(param_name.to_string(), self.locals.len() + 1);
        }

        // Expr block.
        self.compile_expression_block(&function.body);

        // Pop off the stack.
        self.emit(Instruction::Return);

        Ok(Function {
            name: function.name.to_string(),
            arity: function.args.args.len(),
            bytecode: self.bytecode,
            kind: FunctionKind::Exec,

            // Debugging stuff.
            local_var_names: {
                let mut names = Vec::with_capacity(self.locals.len() + 1);
                // Function is pushed onto the stack.
                names.push(format!("<fn {}>", function.name));
                // Locals come after.
                names.resize_with(names.capacity(), String::new);

                for (name, index) in &self.locals {
                    names[*index] = name.to_string();
                }

                names
            },
        })
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

    /// Updates the current source line from a span.
    fn set_source_line_from_span(&mut self, span: &ast::Span) {
        let (start_line, _) = span.line_and_column();
        self.current_source_line = start_line.0;
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

    /// Compiles [`ExpressionBlock`] instances.
    ///
    /// [`ExpressionBlock`] is not a variant of [`Expression`], instead it's
    /// part of [`ast::ExprFn`] and [`Expression::ExprBlock`], and since
    /// compilation is recursive it needs it's own separate function.
    fn compile_expression_block(&mut self, block: &ExpressionBlock) {
        // Start new scope.
        self.scope += 1;

        let mut scope_locals = HashSet::new();

        // Compile statements and resolve locals.
        for statement in &block.stmts {
            match statement {
                ast::Stmt::Let(stmt) => {
                    // Compile the assignment expression.
                    self.compile_expression(&stmt.expr);

                    // Resolve the index of the local variable at runtime.
                    self.locals
                        .insert(statement.identifier().to_string(), self.locals.len() + 1);

                    // We'll remove scoped locals so that outer local indexes are not
                    // affected.
                    scope_locals.insert(stmt.identifier.name());

                    // We don't need to emit Instruction::StoreVar because when the
                    // expression is executed and leaves the value on top of the stack,
                    // that index in the stack will be the index of the local variable.
                    // It's already "stored".
                }
                ast::Stmt::ForLoop(stmt) => {
                    // Compile the iterator expression (array) - leaves array on stack
                    self.compile_expression(&stmt.iterator);

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
                    self.locals
                        .insert(statement.identifier().name().to_string(), item_local);

                    // Compile the loop body (nested expression block)
                    self.compile_expression_block(&stmt.body);

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
                    self.locals.remove(stmt.identifier.name());

                    // Push null as the for loop result
                    let null_index = self.add_constant(Value::Null);
                    self.emit(Instruction::LoadConst(null_index));

                    // Track this as a local so it gets cleaned up in EndBlock
                    scope_locals.insert(stmt.identifier.name());
                }
            }
        }

        // Compile the return expression.
        self.compile_expression(&block.expr);

        // Scope 1 is the function's body. After that we have subblocks. If
        // those subblocks contain locals, then we have to pop them from the
        // stack. Otherwise we do nothing, we simply leave the resulting value
        // on top of the stack and that will be exactly the slot of the outer
        // local variable assignment.
        if self.scope > 1 && !block.stmts.is_empty() {
            self.emit(Instruction::EndBlock(block.stmts.len()));

            for local in scope_locals {
                self.locals.remove(local);
            }
        }

        // End scope.
        self.scope -= 1;
    }

    /// Generate bytecode for an expression.
    ///
    /// # Dev notes
    ///
    /// Be cautious with "abstractions" here. It's better to be explicit so that
    /// we can see exactly what instructions are being emitted in each scenario.
    /// We should not create `emit_some_crazy_stuff` functions unless they can
    /// be reused many times.
    fn compile_expression(&mut self, expression: &Expression) {
        self.set_source_line_from_span(expression.span());

        match expression {
            // Constants.
            Expression::BoolValue(bool, _) => {
                let index = self.add_constant(Value::Bool(*bool));
                self.emit(Instruction::LoadConst(index));
            }

            Expression::NumericValue(num, _) => {
                let value = num
                    .parse::<i64>()
                    .map(Value::Int)
                    .or_else(|_| num.parse::<f64>().map(Value::Float))
                    .unwrap_or_else(|_| panic!("failed to parse number: {num}"));

                let index = self.add_constant(value);
                self.emit(Instruction::LoadConst(index));
            }

            Expression::StringValue(string, _) => {
                // Allocate the string in the objects pool
                self.objects.push(Object::String(string.to_string()));
                let object_index = self.objects.len() - 1;

                // Add a constant that points to the string object
                let const_index = self.add_constant(Value::Object(object_index));
                self.emit(Instruction::LoadConst(const_index));
            }

            Expression::RawStringValue(raw_string) => todo!(),

            // Variables.
            Expression::Identifier(identifier) => {
                self.emit(Instruction::LoadVar(self.locals[identifier.name()]));
            }

            // Compound objects.
            Expression::Array(expressions, _) => {
                for expression in expressions {
                    self.compile_expression(expression);
                }

                self.emit(Instruction::AllocArray(expressions.len()));
            }

            Expression::Map(items, _) => todo!(),

            // Some notes on how class constructors work.
            //
            // Fields at runtime are accessed by index, not through a string
            // lookup, because we are a compiled language and we know the exact
            // shape of all instances of classes. But that comes with some
            // problems when constructing instances. How exactly do we set each
            // field to the value specified in the source code AND in the same
            // order specified in the source code? We have our own internal
            // field order for index accessing, but source code can use the
            // names of fields in any arbitrary order:
            //
            // ```baml
            // // Turned into an array, order is [x, y]
            // class Point {
            //     x int
            //     y int
            // }
            //
            // // User does whatever they want, set y then x.
            // let p = Point {
            //     y: 2,
            //     x: 1,
            // };
            // ```
            //
            // You might think, well, what's so hard about this? Just issue
            // the LOAD_CONST instructions in the order that the class
            // definition expects, then we'll have all the necessary values
            // on the eval stack in the order we want and we can call some
            // sort of ALLOC_INSTANCE instruction that would behave like
            // the ALLOC_ARRAY instruction, but for instances:
            //
            // ```text
            // LOAD_CONST x (1)   // Put value of x on the stack.
            // LOAD_CONST y (2)   // Put value of y on the stack.
            // ALLOC_INSTANCE 2   // Just like defining an array of 2 elements.
            // ```
            //
            // Well, not so fast. Assignments are expressions. Expressions
            // can have side effects (set the value of a global variable,
            // print some stuff to the program's output, etc). Picture this:
            //
            // ```baml
            // let p = Point {
            //     y: side_effect(),
            //     x: another_side_effect(),
            // }
            // ```
            //
            // We can't just tell the VM to execute `another_side_effect()`
            // then execute `side_effect()` so that we have the constructor
            // parameters in order [x, y] on the stack. The user expects
            // `side_effect()` to be executed first.
            //
            // Given the statement above, we know that we must issue bytecode
            // instructions in the same order defined in the source code, we
            // can't reorder field assignments. We'll need something like:
            //
            // ```text
            // ALLOC_INSTANCE Point  // Allocate an instance of Point.
            //
            // side_effect()         // Compiled instructions for y.
            //
            // STORE_FIELD 1         // Store y in the instance (at index 1).
            //
            // another_side_effect() // Compiled instructions for x.
            //
            // STORE_FIELD 0         // Store x in the instance (at index 0).
            // ```
            //
            // Not "ideal", we'd want something similar to arrays:
            //
            // ```text
            // another_side_effect() // Compiled instructions for x.
            // side_effect()         // Compiled instructions for y.
            // ALLOC_INSTANCE Point  // Allocate an instance of Point.
            // ```
            //
            // That's more efficient and requires only 1 VM cycle for the
            // entire instance construction, but it messes up execution
            // ordering. Therefore, that can only be done if we are sure that
            // the assignments contain only constant values or at most one side
            // effect. So...
            //
            // TODO: If someone wants to work on bytecode optimization, heres's
            // one for you...
            //
            // For the time being, we'll just desugar class constructors to
            // individual assignments. Basically this:
            //
            // ```baml
            // let p = Point {};
            // p.y = side_effect();
            // p.x = another_side_effect();
            // ```
            //
            // TODO: There's room for adding an HIR (High Level IR) here just
            // to desugar stuff like this, but for now we can treat class
            // constructors in the AST as already desugared assignments.
            //
            // TODO: Explain what's going on with the spread operator.
            Expression::ClassConstructor(constructor, _) => {
                self.emit(Instruction::AllocInstance(
                    self.globals[constructor.class_name.name()],
                ));

                let mut defined_named_fields = HashSet::new();

                for field in &constructor.fields {
                    match field {
                        ClassConstructorField::Named(name, expr) => {
                            self.compile_expression(expr);
                            self.emit(Instruction::StoreField(
                                self.class_fields[constructor.class_name.name()][name.name()],
                            ));
                            defined_named_fields.insert(name.name());
                        }
                        ClassConstructorField::Spread(expr) => {
                            self.compile_expression(expr);

                            // Pseudo local, user didn't declare it.
                            let spread_local = self.locals.len() + 2;
                            self.emit(Instruction::LoadVar(spread_local - 1));

                            for (field, index) in &self.class_fields[constructor.class_name.name()]
                            {
                                if !defined_named_fields.contains(field.as_str()) {
                                    self.emit(Instruction::LoadVar(spread_local));
                                    self.emit(Instruction::LoadField(*index));
                                    self.emit(Instruction::StoreField(*index));
                                }
                            }
                        }
                    }
                }
            }

            // Functions.
            Expression::Lambda(arguments_list, expression_block, _) => todo!(),

            Expression::App(app) => {
                // Push the function onto the stack.
                self.emit(Instruction::LoadGlobal(self.globals[app.name.name()]));

                // Push the arguments onto the stack.
                for arg in &app.args {
                    self.compile_expression(arg);
                }

                // Call the function.
                self.emit(Instruction::Call(app.args.len()));
            }

            Expression::JinjaExpressionValue(jinja_expression, _) => todo!(),

            Expression::ExprBlock(block, _) => {
                self.compile_expression_block(block);
            }

            // Branching.
            Expression::If(condition, r#if, r#else, _) => {
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
                self.compile_expression(r#if);

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
                if let Some(r#else) = r#else {
                    self.compile_expression(r#else);
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
}

/// Compile a Baml AST into bytecode.
///
/// For now, these creates a couple data structures that the VM needs in order
/// to run. First, it returns the object pool or object arena, which contains
/// all the functions, then it returns the globals pool (functions are basically
/// global "variables").
pub fn compile(ast: ParserDatabase) -> anyhow::Result<(Vec<Object>, Vec<Value>)> {
    // eprintln!("{:#?}", ast.ast);

    // Name -> Index
    let mut resolved_globals = HashMap::new();
    // Class Name -> (Field Name -> Index)
    let mut resolved_class_fields = HashMap::new();

    // Name resolution phase.
    for top in &ast.ast.tops {
        match top {
            ast::Top::Class(class) => {
                // Resolve class name.
                resolved_globals.insert(class.name.to_string(), resolved_globals.len());

                // Resolve class fields.
                resolved_class_fields.insert(
                    class.name().to_string(),
                    class
                        .fields
                        .iter()
                        .enumerate()
                        .map(|(i, field)| (field.name().to_string(), i))
                        .collect(),
                );
            }

            ast::Top::ExprFn(function) => {
                resolved_globals.insert(function.name.to_string(), resolved_globals.len());
            }

            _ => todo!("name resolution: unhandled Top variant: {top:?}"),
        }
    }

    let mut objects = Vec::with_capacity(resolved_globals.len());
    let mut globals = Vec::with_capacity(resolved_globals.len());

    // Compilation phase.
    for top in &ast.ast.tops {
        let object = match top {
            ast::Top::Class(class) => Object::Class(Class {
                name: class.name().to_string(),
                field_names: class
                    .fields
                    .iter()
                    .map(|field| field.name().to_string())
                    .collect(),
            }),

            ast::Top::ExprFn(function) => Object::Function(
                Compiler::new(&resolved_globals, &resolved_class_fields, &mut objects)
                    .compile(function)?,
            ),

            _ => todo!("compilation: unhandled Top variant: {top:?}"),
        };

        globals.push(Value::Object(objects.len()));
        objects.push(object);
    }

    Ok((objects, globals))
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
        let (objects, globals) = compile(ast)?;

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
                baml_vm::debug::display_bytecode(function, &[], &objects, &globals)
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
