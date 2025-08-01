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

use baml_vm::{BamlVmProgram, Bytecode, Class, Function, FunctionKind, Instruction, Object, Value};
use internal_baml_parser_database::ParserDatabase;

/// Compile a Baml AST into bytecode.
///
/// This now uses a two-stage compilation process:
/// 1. AST -> HIR
/// 2. HIR -> Bytecode
pub fn compile(ast: &ParserDatabase) -> anyhow::Result<BamlVmProgram> {
    // Stage 1: AST -> HIR
    let hir = hir::Hir::from_ast(&ast.ast);

    println!("HIR: {:#?}", hir);

    // Stage 2: HIR -> Bytecode
    compile_hir_to_bytecode(&hir)
}

/// Compile HIR to bytecode.
///
/// This function takes an HIR Program and generates the bytecode for the VM.
fn compile_hir_to_bytecode(hir: &hir::Hir) -> anyhow::Result<BamlVmProgram> {
    let mut resolved_globals = HashMap::new();
    let mut resolved_classes = HashMap::new();

    // Resolve global functions from HIR
    let mut global_index = 0;
    for func in &hir.expr_functions {
        resolved_globals.insert(func.name.clone(), global_index);
        global_index += 1;
    }

    // Resolve classes from HIR
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

    let mut objects = Vec::with_capacity(resolved_globals.len());
    let mut globals = Vec::with_capacity(resolved_globals.len());

    // Compile HIR functions to bytecode
    for func in &hir.expr_functions {
        let bytecode_function =
            compile_hir_function(func, &resolved_globals, &resolved_classes, &mut objects)?;

        // Add the function to the globals and objects pools.
        globals.push(Value::Object(objects.len()));
        objects.push(Object::Function(bytecode_function));
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
    objects: &mut Vec<Object>,
) -> anyhow::Result<Function> {
    let mut compiler = HirCompiler::new(globals, classes, objects);
    compiler.compile_function(func)
}

/// Block scope tracker.
///
/// The scope increments with each nested block. Example:
///
/// ```ignore
/// fn example() {          // Scope is 1. Locals: [a]
///     let a = 1;
///     {                   // Scope is 2. Locals: [a, b]
///         let b = 2;
///         {               // Scope is 3. Locals: [a, b, c]
///             let c  = 3;
///         }
///     }
/// }
/// ```
///
/// This is used to keep track of local variables present in the evaluation
/// stack.
#[derive(Debug, Default)]
struct Scope {
    /// Current scope depth.
    depth: usize,

    /// Stack of locals in each scope we're diving into.
    locals: Vec<HashSet<String>>,

    /// Stack of scope ids.
    ids: Vec<usize>,
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

    /// Resolved local variables.
    ///
    /// Maps the name of the variable to its final index in the eval stack.
    locals: HashMap<String, usize>,

    /// Scope tracking. Current depth and stack of visited scopes.
    scope: Scope,

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
        objects: &'g mut Vec<Object>,
    ) -> Self {
        Self {
            globals,
            classes,
            objects,
            locals: HashMap::new(),
            bytecode: Bytecode::new(),
            scope: Scope::default(),
            current_source_line: 0,
            locals_in_scope: Vec::new(),
        }
    }

    fn compile_function(&mut self, func: &hir::ExprFunction) -> anyhow::Result<Function> {
        // Resolve parameters.
        for param in &func.parameters {
            self.track_local(&param.name);
        }

        // Compile statements in the function body.
        self.compile_block(&func.body);

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

    fn compile_block(&mut self, block: &hir::Block) {
        self.enter_scope();

        for statement in &block.statements {
            self.compile_statement(statement);
        }

        self.exit_scope();
    }

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
                self.emit(Instruction::Pop);

                // Compile loop body
                self.compile_block(block);

                // Jump back to start
                let loop_end = self.bytecode.instructions.len() as isize;
                let offset = -(loop_end - loop_start);
                self.emit(Instruction::Jump(offset));

                // Patch exit jump
                self.patch_jump(exit_jump);

                // Pop condition
                self.emit(Instruction::Pop);
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

                // Call the function
                self.emit(Instruction::Call(args.len()));
            }

            hir::Expression::ClassConstructor(cc, _) => {
                // Allocate instance
                if let Some(&class_index) = self.globals.get(&cc.class_name) {
                    self.emit(Instruction::AllocInstance(class_index));

                    // Set fields
                    for field in &cc.fields {
                        self.compile_expression(&field.value);
                        if let Some(class_fields) = self.classes.get(&cc.class_name) {
                            if let Some(&field_index) = class_fields.get(&field.name) {
                                self.emit(Instruction::StoreField(field_index));
                            } else {
                                panic!("undefined field: {}.{}", cc.class_name, field.name);
                            }
                        } else {
                            panic!("undefined class: {}", cc.class_name);
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
                // Compile condition
                self.compile_expression(condition);

                // Jump if false
                let skip_if = self.emit(Instruction::JumpIfFalse(0));

                // Pop condition and compile then branch
                self.emit(Instruction::Pop);
                self.compile_expression(if_branch);

                // Jump over else
                let skip_else = self.emit(Instruction::Jump(0));

                // Patch the skip_if jump
                self.patch_jump(skip_if);

                // Pop condition
                self.emit(Instruction::Pop);

                // Compile else branch if present
                if let Some(else_branch) = else_branch {
                    self.compile_expression(else_branch);
                }

                // Patch the skip_else jump
                self.patch_jump(skip_else);
            }

            hir::Expression::ExpressionBlock(block, _) => {
                self.compile_block(block);
            }
        }
    }

    fn emit(&mut self, instruction: Instruction) -> usize {
        let index = self.bytecode.instructions.len();

        self.bytecode.instructions.push(instruction);
        self.bytecode.source_lines.push(self.current_source_line);
        self.bytecode.scopes.push(*self.scope.ids.last().expect(
            "compiler bug: attempt to read scope ID of instruction when scope stack is empty",
        ));

        index
    }

    fn add_constant(&mut self, value: Value) -> usize {
        self.bytecode.constants.push(value);
        self.bytecode.constants.len() - 1
    }

    fn patch_jump(&mut self, instruction_ptr: usize) {
        let destination = self.bytecode.instructions.len();

        match &mut self.bytecode.instructions[instruction_ptr] {
            Instruction::Jump(offset) | Instruction::JumpIfFalse(offset) => {
                *offset = (destination - instruction_ptr) as isize;
            }
            _ => unreachable!(
                "expected jump instruction at index {instruction_ptr}, but got {:?}",
                self.bytecode.instructions[instruction_ptr]
            ),
        }
    }

    fn track_local(&mut self, name: &str) -> usize {
        let index = self.locals.len() + 1;
        self.locals.insert(name.to_string(), index);

        self.scope
            .locals
            .last_mut()
            .unwrap()
            .insert(name.to_string());

        index
    }

    fn enter_scope(&mut self) {
        self.scope.depth += 1;
        self.scope.locals.push(HashSet::new());
        self.scope.ids.push(self.locals_in_scope.len());

        self.locals_in_scope.push(HashMap::new());
    }

    fn exit_scope(&mut self) {
        let scope_id = self
            .scope
            .ids
            .pop()
            .expect("failed to keep track of the current scope id (compiler bug)");

        self.locals_in_scope[scope_id] = self.locals.clone();

        let scope_locals = self.scope.locals.pop().unwrap();

        if self.scope.depth > 1 && !self.scope.locals.is_empty() {
            self.emit(Instruction::EndBlock(scope_locals.len()));

            for local in scope_locals {
                self.locals.remove(&local);
            }
        }

        self.scope.depth -= 1;
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
                    Instruction::AllocInstance(1),
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
                    Instruction::EndBlock(1),
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
                        3
                    };
                    let e = {
                        let f = 4;
                        let g = 5;
                        6
                    };

                    7
                };

                let h = {
                    let z = 0;

                    let i = {
                        let w = 0;
                        let j = 8;
                        9
                    };

                    10
                };

                a
            }
        "#)?;

        let BamlVmProgram {
            objects,
            resolved_function_names,
            ..
        } = compile(&ast)?;

        let main = objects[resolved_function_names["main"].0].as_function()?;

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
}
