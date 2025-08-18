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

    let native_fns = baml_vm::native::functions();

    for name in native_fns.keys() {
        resolved_globals.insert(name.clone(), resolved_globals.len());
    }

    let mut objects = Vec::with_capacity(resolved_globals.len());
    let mut globals = Vec::with_capacity(resolved_globals.len());

    let mut loop_vars_counter = ForLoopVarCounters::new();

    // Compile HIR functions to bytecode
    for func in &hir.expr_functions {
        let bytecode_function = compile_hir_function(
            func,
            &resolved_globals,
            &resolved_classes,
            &llm_functions,
            &mut loop_vars_counter,
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

    for (name, (func, arity)) in native_fns {
        let native_function = Object::Function(Function {
            name: name.clone(),
            arity,
            bytecode: Bytecode::new(),
            kind: FunctionKind::Native(func),
            locals_in_scope: vec![], // TODO.
        });

        globals.push(Value::Object(objects.len()));
        objects.push(native_function);
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

/// Produces a variable of the form `__baml <name_infix> <counter>`.
/// These variables cannot be accessed by the user because they have spaces
#[derive(Default)]
struct VariableCounter {
    pub name_infix: &'static str,
    counter: usize,
}

impl VariableCounter {
    pub fn new(name_infix: &'static str) -> Self {
        Self {
            name_infix,
            counter: 0,
        }
    }

    pub fn next(&mut self) -> String {
        self.counter += 1;
        let index = self.counter - 1;
        format!("__baml {} {index}", self.name_infix)
    }
}

struct ForLoopVarCounters {
    pub loop_index: VariableCounter,
    pub array: VariableCounter,
    pub array_len: VariableCounter,
}

impl ForLoopVarCounters {
    pub fn new() -> Self {
        Self {
            loop_index: VariableCounter::new("for loop index"),
            array: VariableCounter::new("for loop iterated array"),
            array_len: VariableCounter::new("for loop array length"),
        }
    }
}

/// Compile an HIR function to bytecode.
fn compile_hir_function(
    func: &hir::ExprFunction,
    globals: &HashMap<String, usize>,
    classes: &HashMap<String, HashMap<String, usize>>,
    llm_functions: &HashSet<String>,
    loop_var_counter: &mut ForLoopVarCounters,
    objects: &mut Vec<Object>,
) -> anyhow::Result<Function> {
    let mut compiler = HirCompiler::new(globals, classes, llm_functions, loop_var_counter, objects);
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

    var_counters: &'g mut ForLoopVarCounters,

    /// Scope stack.
    scopes: Vec<Scope>,

    current_loop: Option<LoopInfo>,

    /// Locals in scope.
    locals_in_scope: Vec<HashMap<String, usize>>,

    /// Current source line.
    current_source_line: usize,

    /// Bytecode to generate.
    bytecode: Bytecode,

    /// Objects pool.
    objects: &'g mut Vec<Object>,
}

#[derive(Debug)]
struct LoopInfo {
    /// Length of [`HirCompiler::scopes`] before entering the loop body. This helps `break` and
    /// `continue` know how many scopes they have to pop.
    pub scope_depth: usize,
    /// List of jump instruction locations to be patched when loop construction is done.
    /// They will point to the loop exit. Used for admitting arbitrary `break`s.
    pub break_patch_list: Vec<usize>,
    /// List of jump instruction locations to be patched when loop construction is done.
    /// They will point to the end of the loop scope. Used for admitting arbitrary `continue`s.
    pub continue_patch_list: Vec<usize>,
}

impl<'g> HirCompiler<'g> {
    fn new(
        globals: &'g HashMap<String, usize>,
        classes: &'g HashMap<String, HashMap<String, usize>>,
        llm_functions: &'g HashSet<String>,
        var_counters: &'g mut ForLoopVarCounters,
        objects: &'g mut Vec<Object>,
    ) -> Self {
        Self {
            globals,
            classes,
            llm_functions,
            objects,
            locals: HashMap::new(),
            var_counters,
            current_loop: None,
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

        let scope_has_ending_expr = block.statements.last().is_some_and(|stmt| match stmt {
            hir::Statement::Expression { expr, .. } => expr.produces_final_value(),
            _ => false,
        });

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
                self.declare_mut(name);
            }
            hir::Statement::Assign { name, value, .. } => {
                self.compile_expression(value);
                self.emit(Instruction::StoreVar(self.locals[name]));
            }
            hir::Statement::AssignOp {
                name,
                value,
                assign_op,
                ..
            } => {
                self.emit(Instruction::LoadVar(self.locals[name]));
                self.compile_expression(value);

                self.emit(match assign_op {
                    hir::AssignOp::AddAssign => Instruction::BinOp(BinOp::Add),
                    hir::AssignOp::SubAssign => Instruction::BinOp(BinOp::Sub),
                    hir::AssignOp::MulAssign => Instruction::BinOp(BinOp::Mul),
                    hir::AssignOp::DivAssign => Instruction::BinOp(BinOp::Div),
                    hir::AssignOp::ModAssign => Instruction::BinOp(BinOp::Mod),

                    hir::AssignOp::BitAndAssign => Instruction::BinOp(BinOp::BitAnd),
                    hir::AssignOp::BitOrAssign => Instruction::BinOp(BinOp::BitOr),
                    hir::AssignOp::BitXorAssign => Instruction::BinOp(BinOp::BitXor),
                    hir::AssignOp::ShlAssign => Instruction::BinOp(BinOp::Shl),
                    hir::AssignOp::ShrAssign => Instruction::BinOp(BinOp::Shr),
                });

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
            hir::Statement::ForLoop {
                identifier,
                iterator,
                block,
                ..
            } => {
                // store array, array length & index in stack.
                // compile as:
                // let <array> = (iterator);
                // let <array len> = array.len()
                // var <loop i> = 0;
                // while (<loop i> < <array len>) {
                //      let <iterator> = <array>[<loop i>];
                //      <loop i>++;
                //      (loop body)
                // }

                let len_method = *self
                    .globals
                    .get("len")
                    .expect("native len() for array length is not in globals?");

                // {

                self.compile_expression(iterator);

                self.enter_scope();

                // stack: [<array>]

                // save array length & loop index as locals. Use spaces for variable names since
                // those can't be achieved by the user.

                let array_name = self.var_counters.array.next();
                let array_len_name = self.var_counters.array_len.next();
                let loop_i_name = self.var_counters.loop_index.next();

                // track first array, then array len
                let array_location = self.track_local(&array_name);
                let array_len_location = self.track_local(&array_len_name);
                let loop_i_location = self.track_local(&loop_i_name);

                // array.len() -> into array_len_location.
                self.emit(Instruction::LoadGlobal(len_method));
                self.emit(Instruction::LoadVar(array_location));
                self.emit(Instruction::Call(1));

                // var <loop i> = 0;
                {
                    // maintain zero at a place because otherwise we're going to add it every time
                    // a `for` loop is compiled.
                    let zero = self.find_or_add_int(0);

                    self.emit(Instruction::LoadConst(zero));
                }

                self.compile_while_loop(
                    |ctx| {
                        ctx.emit(Instruction::LoadVar(loop_i_location));
                        ctx.emit(Instruction::LoadVar(array_len_location));
                        ctx.emit(Instruction::CmpOp(CmpOp::Lt));
                    },
                    |ctx| {
                        ctx.enter_scope();

                        ctx.track_local(identifier.as_str());

                        // let <iterator name> = array[i];

                        ctx.emit(Instruction::LoadVar(array_location));
                        ctx.emit(Instruction::LoadVar(loop_i_location));
                        ctx.emit(Instruction::LoadArrayElement);

                        // <loop_i>++;
                        ctx.emit(Instruction::LoadVar(loop_i_location));
                        let one = ctx.find_or_add_int(1);
                        ctx.emit(Instruction::LoadConst(one));
                        ctx.emit(Instruction::BinOp(BinOp::Add));
                        ctx.emit(Instruction::StoreVar(loop_i_location));

                        // stack: [<array> <array len> <array iterator> <loop iterator>]

                        ctx.compile_block(block);

                        ctx.exit_scope(false);
                    },
                    |_| {},
                );

                self.exit_scope(false);
            }
            hir::Statement::While {
                condition, block, ..
            } => {
                self.compile_while_loop(
                    |ctx| ctx.compile_expression(condition),
                    |ctx| ctx.compile_block(block),
                    |_| {},
                );
            }
            hir::Statement::Break(_) => {
                let cur_loop = self.assert_loop("break");

                // since we are exiting the loop context, make sure we drop everything before
                // breaking!
                let pop_until = cur_loop.scope_depth;
                self.emit_scope_drops(pop_until);

                let exit_jump = self.next_insn_index() as usize;
                self.assert_loop("break").break_patch_list.push(exit_jump);

                // NOTE: right now this will generate redundant code when using
                // `if condition { break }`, since `if` will generate its own jump location and we
                // will end up with a conditional jump and a regular jump together.
                self.emit(Instruction::Jump(0));
            }
            hir::Statement::Continue(_) => {
                let cur_loop = self.assert_loop("continue");

                let pop_until = cur_loop.scope_depth;
                self.emit_scope_drops(pop_until);

                let exit_jump = self.next_insn_index() as usize;
                self.assert_loop("continue")
                    .continue_patch_list
                    .push(exit_jump);

                // NOTE: right now this will generate redundant code when using
                // `if condition { continue }`, since `if` will generate its own jump location and we
                // will end up with a conditional jump and a regular jump together, making the jump
                // unreachable.
                self.emit(Instruction::Jump(0));
            }
            hir::Statement::CForLoop {
                condition,
                after,
                block,
            } => match condition {
                Some(cond) => self.compile_while_loop(
                    |ctx| ctx.compile_expression(cond),
                    |ctx| ctx.compile_block(block),
                    |ctx| {
                        if let Some(after) = &after {
                            ctx.compile_statement(after);
                        }
                    },
                ),
                None => {
                    // infinite loop.

                    let loop_start = self.next_insn_index();

                    let break_locs = self.wrap_loop_body(|ctx| ctx.compile_block(block));

                    if let Some(after) = &after {
                        self.compile_statement(after);
                    }

                    self.emit(Instruction::Jump(loop_start - self.next_insn_index()));

                    for loc in break_locs {
                        self.patch_jump(loc);
                    }
                }
            },
            hir::Statement::Assert { condition, .. } => {
                self.compile_expression(condition);
                self.emit(Instruction::Assert);
            }
        }
    }

    fn assert_loop(&mut self, name: &'static str) -> &mut LoopInfo {
        match self.current_loop.as_mut() {
            None => panic!("`{name}` must have a loop wrapping it, and this should have been checked by validation"),
            Some(x) => x,
        }
    }

    fn declare_mut(&mut self, name: &str) -> usize {
        // For mutable references, we need to allocate space on the stack
        // We'll push a null/undefined value as placeholder
        let constant_index = self.add_constant(Value::Null);
        self.emit(Instruction::LoadConst(constant_index));
        self.track_local(name)
    }

    fn find_or_add_int(&mut self, wanted_int: i64) -> usize {
        let known_location = self
            .bytecode
            .constants
            .iter()
            .enumerate()
            .find_map(|(i, elem)| {
                let Value::Int(val) = elem else {
                    return None;
                };

                (val == &wanted_int).then_some(i)
            });

        known_location.unwrap_or_else(|| self.add_constant(Value::Int(wanted_int)))
    }

    fn next_insn_index(&self) -> isize {
        self.bytecode.instructions.len() as isize
    }

    /// Compiles a while loop with custom condition & block logic.
    ///
    /// Lambdas take `&mut Self` because both cannot borrow `self` at the same time.
    fn compile_while_loop(
        &mut self,
        compile_condition: impl FnOnce(&mut Self),
        compile_block: impl FnOnce(&mut Self),
        // statements that occur between exiting the loop body & beginning the next iteration.
        compile_after: impl FnOnce(&mut Self),
    ) {
        let loop_start = self.next_insn_index();

        compile_condition(self);

        // this jump needs cleaning up, so it's not the same as `break`.
        let bail_jump = self.emit(Instruction::JumpIfFalse(0));
        self.emit(Instruction::Pop(1));

        let break_locs = self.wrap_loop_body(compile_block);

        compile_after(self);

        // emit jump to start
        self.emit(Instruction::Jump(loop_start - self.next_insn_index()));

        let pop_if_condition = self.emit(Instruction::Pop(1));
        self.patch_jump_to(bail_jump, pop_if_condition);

        // make `break` jump here, since `true` branch of if already popped.
        for loc in break_locs {
            self.patch_jump(loc);
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
                unimplemented!("field access compilation")
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

            hir::Expression::MethodCall {
                receiver,
                method,
                args,
                ..
            } => {
                // Push the function onto the stack
                let Some(&index) = self.globals.get(method) else {
                    panic!("undefined method: {method}");
                };

                self.emit(Instruction::LoadGlobal(index));

                self.compile_expression(receiver);

                for arg in args {
                    self.compile_expression(arg);
                }

                // `self` counts as one argument.
                self.emit(Instruction::Call(1 + args.len()));
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
                            // Arithmetic operators.
                            hir::BinaryOperator::Add => Instruction::BinOp(BinOp::Add),
                            hir::BinaryOperator::Sub => Instruction::BinOp(BinOp::Sub),
                            hir::BinaryOperator::Mul => Instruction::BinOp(BinOp::Mul),
                            hir::BinaryOperator::Div => Instruction::BinOp(BinOp::Div),
                            hir::BinaryOperator::Mod => Instruction::BinOp(BinOp::Mod),

                            // Bitwise operators.
                            hir::BinaryOperator::BitAnd => Instruction::BinOp(BinOp::BitAnd),
                            hir::BinaryOperator::BitOr => Instruction::BinOp(BinOp::BitOr),
                            hir::BinaryOperator::BitXor => Instruction::BinOp(BinOp::BitXor),
                            hir::BinaryOperator::Shl => Instruction::BinOp(BinOp::Shl),
                            hir::BinaryOperator::Shr => Instruction::BinOp(BinOp::Shr),

                            // Comparison operators.
                            hir::BinaryOperator::Eq => Instruction::CmpOp(CmpOp::Eq),
                            hir::BinaryOperator::Neq => Instruction::CmpOp(CmpOp::NotEq),
                            hir::BinaryOperator::Lt => Instruction::CmpOp(CmpOp::Lt),
                            hir::BinaryOperator::LtEq => Instruction::CmpOp(CmpOp::LtEq),
                            hir::BinaryOperator::Gt => Instruction::CmpOp(CmpOp::Gt),
                            hir::BinaryOperator::GtEq => Instruction::CmpOp(CmpOp::GtEq),

                            // Logical operators.
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
    fn patch_jump_to(&mut self, instruction_ptr: usize, destination: usize) {
        match &mut self.bytecode.instructions[instruction_ptr] {
            Instruction::Jump(offset) | Instruction::JumpIfFalse(offset) => {
                *offset = destination as isize - instruction_ptr as isize;
            }
            _ => panic!(
                "compiler bug: expected jump instruction at index {instruction_ptr}, but got {:?}",
                self.bytecode.instructions[instruction_ptr]
            ),
        }
    }

    /// Patches a jump instruction to point to the next instruction.
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

        self.patch_jump_to(instruction_ptr, destination)
    }

    /// Keeps track of a new local and returns its index in the eval stack.
    fn track_local(&mut self, name: &str) -> usize {
        let index = self.locals.len() + 1;
        debug_assert!(self.locals.insert(name.to_string(), index).is_none());

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

    /// Emits instructions to drop scopes up-to and including `pop_until` index, but does not affect information for
    /// locals.
    /// Used in `break` & `continue` to emit appropriate popping instructions.
    fn emit_scope_drops(&mut self, pop_until: usize) {
        let scopes = &self.scopes[pop_until..];

        let local_count = scopes
            .iter()
            .map(|s| {
                // see `exit_scope`: depth 0 is function body block, and thus has `return`.
                if s.depth == 0 {
                    0
                } else {
                    s.locals.len()
                }
            })
            .sum();

        if local_count > 0 {
            self.emit(Instruction::Pop(local_count));
        }
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

    /// Wraps loop inside a scope that is fully popped on both `continue` & `break`.
    /// Returns a patch list of instruction locations for jumps to bail out of the loop, from
    /// `break`s. Note that there is no cleanup from inside the loop to perform.
    ///
    /// Does **NOT** emit the jump instruction to jump back to the beginning of the loop. This is
    /// inteded, since it allows adding arbitrary instructions to `continue`
    fn wrap_loop_body(&mut self, codegen_body: impl FnOnce(&mut Self)) -> Vec<usize> {
        self.enter_scope();

        let old_loop_status = self.current_loop.replace(LoopInfo {
            scope_depth: self.scopes.len(),
            break_patch_list: Vec::new(),
            continue_patch_list: Vec::new(),
        });

        codegen_body(self);

        let loop_info = std::mem::replace(&mut self.current_loop, old_loop_status)
            .expect("should have been pushed before when grabbing old_status");

        self.exit_scope(false);

        for continue_jmp in loop_info.continue_patch_list {
            self.patch_jump(continue_jmp);
        }

        loop_info.break_patch_list
    }
}

impl hir::Expression {
    /// Returns true if the block ends with an expression that has a final value.
    ///
    /// For example, it would return true for this block:
    ///
    /// ```ignore
    /// let a = {
    ///     let b = 1;
    ///     if b == 1 {
    ///         1
    ///     } else {
    ///         2
    ///     }
    /// };
    /// ```
    ///
    /// But false for this one:
    ///
    /// ```ignore
    /// let mut a = 0;
    /// if a == 0 {
    ///     a = 1;
    /// } else {
    ///     a = 2;
    /// }
    /// ```
    ///
    /// TODO: This seems completely unecessary, the typechecker will already
    /// check at some point that return values match the expected type. After
    /// that we should alreay have enough information to decide whether a block
    /// returns or not.
    fn produces_final_value(&self) -> bool {
        match self {
            // First call will happen on a block. Recurse on the final expression.
            hir::Expression::ExpressionBlock(block, _) => match block.statements.last() {
                Some(hir::Statement::Expression { expr, .. }) => expr.produces_final_value(),

                // Does not produce a value.
                _ => false,
            },

            // If statements as last expression need to check if they return
            // any value. We won't recurse into the else branch because both
            // need to match, if one of them returns a value the other one must
            // return the same type. This is typechecker bug if it's wrong, so
            // I won't bother here.
            hir::Expression::If { if_branch, .. } => if_branch.produces_final_value(),

            // This is an expression that produces a value, so true. We're
            // forcing non-exhaustive match here because other types of
            // expressions that we add in the future might need to be considered.
            hir::Expression::Array(_, _)
            | hir::Expression::Map(_, _)
            | hir::Expression::JinjaExpressionValue(_, _)
            | hir::Expression::ArrayAccess { .. }
            | hir::Expression::FieldAccess { .. }
            | hir::Expression::MethodCall { .. }
            | hir::Expression::BoolValue(_, _)
            | hir::Expression::NumericValue(_, _)
            | hir::Expression::Identifier(_, _)
            | hir::Expression::StringValue(_, _)
            | hir::Expression::RawStringValue(_, _)
            | hir::Expression::Call { .. }
            | hir::Expression::ClassConstructor(_, _)
            | hir::Expression::BinaryOperation { .. }
            | hir::Expression::UnaryOperation { .. }
            | hir::Expression::Paren(_, _) => true,
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

    #[test]
    fn basic_assign_add() -> anyhow::Result<()> {
        assert_compiles(Program {
            source: r#"
                fn main() -> int {
                    let mut x = 1;
                    x += 2;
                    x
                }
            "#,
            expected: vec![(
                "main",
                vec![
                    Instruction::LoadConst(0),
                    Instruction::LoadVar(1),
                    Instruction::LoadConst(1),
                    Instruction::BinOp(BinOp::Add),
                    Instruction::StoreVar(1),
                    Instruction::LoadVar(1),
                    Instruction::Return,
                ],
            )],
        })
    }

    #[test]
    fn while_loop_gcd() -> anyhow::Result<()> {
        assert_compiles(Program {
            source: r#"
                fn GCD(mut a: int, mut b: int) -> int {
                    while (a != b) {
                        if a > b {
                            a = a - b;
                        } else {
                            b = b - a;
                        }
                    }

                    a
                }
            "#,
            expected: vec![(
                "GCD",
                vec![
                    Instruction::LoadVar(1),
                    Instruction::LoadVar(2),
                    Instruction::CmpOp(CmpOp::NotEq),
                    Instruction::JumpIfFalse(18),
                    Instruction::Pop(1),
                    Instruction::LoadVar(1),
                    Instruction::LoadVar(2),
                    Instruction::CmpOp(CmpOp::Gt),
                    Instruction::JumpIfFalse(7),
                    Instruction::Pop(1),
                    Instruction::LoadVar(1),
                    Instruction::LoadVar(2),
                    Instruction::BinOp(BinOp::Sub),
                    Instruction::StoreVar(1),
                    Instruction::Jump(6),
                    Instruction::Pop(1),
                    Instruction::LoadVar(2),
                    Instruction::LoadVar(1),
                    Instruction::BinOp(BinOp::Sub),
                    Instruction::StoreVar(2),
                    Instruction::Jump(-20),
                    Instruction::Pop(1),
                    Instruction::LoadVar(1),
                    Instruction::Return,
                ],
            )],
        })
    }

    // This tests that we don't emit POP_REPLACE for if expressions when they
    // do not return values.
    #[test]
    fn nested_block_expr_with_ending_normal_if() -> anyhow::Result<()> {
        assert_compiles(Program {
            source: "
                fn main() -> int {
                    let mut a = 1;

                    {
                        let b = 2;
                        let c = 3;
                        a = b + c;

                        if a == 5 {
                            a = 10;
                        }
                    }

                    a
                }
            ",
            expected: vec![(
                "main",
                vec![
                    Instruction::LoadConst(0),
                    Instruction::LoadConst(1),
                    Instruction::LoadConst(2),
                    Instruction::LoadVar(2),
                    Instruction::LoadVar(3),
                    Instruction::BinOp(BinOp::Add),
                    Instruction::StoreVar(1),
                    Instruction::LoadVar(1),
                    Instruction::LoadConst(3),
                    Instruction::CmpOp(CmpOp::Eq),
                    Instruction::JumpIfFalse(5),
                    Instruction::Pop(1),
                    Instruction::LoadConst(4),
                    Instruction::StoreVar(1),
                    Instruction::Jump(2),
                    Instruction::Pop(1),
                    Instruction::Pop(2),
                    Instruction::LoadVar(1),
                    Instruction::Return,
                ],
            )],
        })
    }

    // This tests that we don't emit POP_REPLACE for if expressions when they
    // do not return values.
    #[test]
    fn while_loop_with_ending_if() -> anyhow::Result<()> {
        assert_compiles(Program {
            source: "
                fn main() -> int {
                    let mut a = 1;

                    while a < 5 {
                        a += 1;

                        if a == 2 {
                            break;
                        }
                    }

                    a
                }
            ",
            expected: vec![(
                "main",
                vec![
                    Instruction::LoadConst(0),
                    Instruction::LoadVar(1),
                    Instruction::LoadConst(1),
                    Instruction::CmpOp(CmpOp::Lt),
                    Instruction::JumpIfFalse(15),
                    Instruction::Pop(1),
                    Instruction::LoadVar(1),
                    Instruction::LoadConst(2),
                    Instruction::BinOp(BinOp::Add),
                    Instruction::StoreVar(1),
                    Instruction::LoadVar(1),
                    Instruction::LoadConst(3),
                    Instruction::CmpOp(CmpOp::Eq),
                    Instruction::JumpIfFalse(4),
                    Instruction::Pop(1),
                    Instruction::Jump(5),
                    Instruction::Jump(2),
                    Instruction::Pop(1),
                    Instruction::Jump(-17),
                    Instruction::Pop(1),
                    Instruction::LoadVar(1),
                    Instruction::Return,
                ],
            )],
        })
    }

    #[test]
    fn break_factorial() -> anyhow::Result<()> {
        assert_compiles(Program {
            source: r#"
                fn Factorial(mut limit: int) -> int {
                    let mut result = 1;

                    while true {
                        if limit == 0 {
                            break;
                        }
                        result = result * limit;
                        limit = limit - 1;
                    }

                    result
                }
            "#,
            expected: vec![(
                "Factorial",
                vec![
                    // let mut result = 1;
                    Instruction::LoadConst(0),
                    // while true { ... }
                    Instruction::LoadConst(1),
                    Instruction::JumpIfFalse(19),
                    Instruction::Pop(1),
                    // if limit == 0 { break; }
                    Instruction::LoadVar(1),
                    Instruction::LoadConst(2),
                    Instruction::CmpOp(CmpOp::Eq),
                    Instruction::JumpIfFalse(4),
                    Instruction::Pop(1),
                    Instruction::Jump(13),
                    Instruction::Jump(2),
                    Instruction::Pop(1),
                    // result = result * limit;
                    Instruction::LoadVar(2),
                    Instruction::LoadVar(1),
                    Instruction::BinOp(BinOp::Mul),
                    Instruction::StoreVar(2),
                    // limit = limit - 1;
                    Instruction::LoadVar(1),
                    Instruction::LoadConst(3),
                    Instruction::BinOp(BinOp::Sub),
                    Instruction::StoreVar(1),
                    // loop back and exit
                    Instruction::Jump(-19),
                    Instruction::Pop(1),
                    // return result
                    Instruction::LoadVar(2),
                    Instruction::Return,
                ],
            )],
        })
    }

    #[test]
    fn continue_factorial() -> anyhow::Result<()> {
        assert_compiles(Program {
            source: r#"
                fn Factorial(mut limit: int) -> int {
                    let mut result = 1;

                    // used to make the loop break without relying on `break` implementation.
                    let mut should_continue = true;
                    while should_continue {
                        result = result * limit;
                        limit = limit - 1;

                        if limit != 0 {
                            continue;
                        } else {
                            should_continue = false;
                        }
                    }

                    result
                }
            "#,
            expected: vec![(
                "Factorial",
                vec![
                    // let mut result = 1;
                    Instruction::LoadConst(0),
                    // let mut should_continue = true;
                    Instruction::LoadConst(1),
                    // while should_continue { ... }
                    Instruction::LoadVar(3),
                    Instruction::JumpIfFalse(21),
                    Instruction::Pop(1),
                    // result = result * limit;
                    Instruction::LoadVar(2),
                    Instruction::LoadVar(1),
                    Instruction::BinOp(BinOp::Mul),
                    Instruction::StoreVar(2),
                    // limit = limit - 1;
                    Instruction::LoadVar(1),
                    Instruction::LoadConst(2),
                    Instruction::BinOp(BinOp::Sub),
                    Instruction::StoreVar(1),
                    // if limit != 0 { continue; } else { should_continue = false; }
                    Instruction::LoadVar(1),
                    Instruction::LoadConst(3),
                    Instruction::CmpOp(CmpOp::NotEq),
                    Instruction::JumpIfFalse(4),
                    Instruction::Pop(1),
                    Instruction::Jump(5),
                    Instruction::Jump(4),
                    Instruction::Pop(1),
                    Instruction::LoadConst(4),
                    Instruction::StoreVar(3),
                    Instruction::Jump(-21),
                    Instruction::Pop(1),
                    // return result
                    Instruction::LoadVar(2),
                    Instruction::Return,
                ],
            )],
        })
    }

    #[test]
    fn continue_nested() -> anyhow::Result<()> {
        assert_compiles(Program {
            source: r#"
                fn Nested() -> int {
                    while true {
                        while false {
                            continue;
                        }
                        if false {
                            continue;
                        }
                    }
                    5
                }
            "#,
            expected: vec![(
                "Nested",
                vec![
                    Instruction::LoadConst(0),
                    Instruction::JumpIfFalse(15),
                    Instruction::Pop(1),
                    Instruction::LoadConst(1),
                    Instruction::JumpIfFalse(4),
                    Instruction::Pop(1),
                    Instruction::Jump(1),
                    Instruction::Jump(-4),
                    Instruction::Pop(1),
                    Instruction::LoadConst(2),
                    Instruction::JumpIfFalse(4),
                    Instruction::Pop(1),
                    Instruction::Jump(3),
                    Instruction::Jump(2),
                    Instruction::Pop(1),
                    Instruction::Jump(-15),
                    Instruction::Pop(1),
                    Instruction::LoadConst(3),
                    Instruction::Return,
                ],
            )],
        })
    }

    #[test]
    fn break_nested() -> anyhow::Result<()> {
        assert_compiles(Program {
            source: r#"
                fn Nested() -> int {
                    let mut a = 5;
                    while true {
                        while true {
                            a = a + 1;
                            break;
                        }
                        a = a + 1;
                        break;
                    }
                    a
                }
            "#,
            expected: vec![(
                "Nested",
                vec![
                    Instruction::LoadConst(0),
                    Instruction::LoadConst(1),
                    Instruction::JumpIfFalse(18),
                    Instruction::Pop(1),
                    Instruction::LoadConst(2),
                    Instruction::JumpIfFalse(8),
                    Instruction::Pop(1),
                    Instruction::LoadVar(1),
                    Instruction::LoadConst(3),
                    Instruction::BinOp(BinOp::Add),
                    Instruction::StoreVar(1),
                    Instruction::Jump(3),
                    Instruction::Jump(-8),
                    Instruction::Pop(1),
                    Instruction::LoadVar(1),
                    Instruction::LoadConst(4),
                    Instruction::BinOp(BinOp::Add),
                    Instruction::StoreVar(1),
                    Instruction::Jump(3),
                    Instruction::Jump(-18),
                    Instruction::Pop(1),
                    Instruction::LoadVar(1),
                    Instruction::Return,
                ],
            )],
        })
    }

    #[test]
    fn builtin_method_call() -> anyhow::Result<()> {
        assert_compiles(Program {
            source: r#"
                fn main() -> int {
                    let arr = [1, 2, 3];
                    arr.len()
                }
            "#,
            expected: vec![(
                "main",
                vec![
                    Instruction::LoadConst(0),
                    Instruction::LoadConst(1),
                    Instruction::LoadConst(2),
                    Instruction::AllocArray(3),
                    Instruction::LoadGlobal(2),
                    Instruction::LoadVar(1),
                    // call with one argument (self)
                    Instruction::Call(1),
                    Instruction::Return,
                ],
            )],
        })
    }

    #[test]
    fn for_loop_sum() -> anyhow::Result<()> {
        assert_compiles(Program {
            source: r#"
                fn Sum(xs: int[]) -> int {
                    let mut result = 0;

                    for x in xs {
                        result += x;
                    }

                    result
                }
                "#,
            expected: vec![(
                "Sum",
                vec![
                    Instruction::LoadConst(0),
                    Instruction::LoadVar(1),
                    Instruction::LoadGlobal(2),
                    Instruction::LoadVar(3),
                    Instruction::Call(1),
                    Instruction::LoadConst(0),
                    Instruction::LoadVar(5),
                    Instruction::LoadVar(4),
                    Instruction::CmpOp(CmpOp::Lt),
                    Instruction::JumpIfFalse(15),
                    Instruction::Pop(1),
                    Instruction::LoadVar(3),
                    Instruction::LoadVar(5),
                    Instruction::LoadArrayElement,
                    Instruction::LoadVar(5),
                    Instruction::LoadConst(1),
                    Instruction::BinOp(BinOp::Add),
                    Instruction::StoreVar(5),
                    Instruction::LoadVar(2),
                    Instruction::LoadVar(6),
                    Instruction::BinOp(BinOp::Add),
                    Instruction::StoreVar(2),
                    Instruction::Pop(1),
                    Instruction::Jump(-17),
                    Instruction::Pop(1),
                    Instruction::Pop(3),
                    Instruction::LoadVar(2),
                    Instruction::Return,
                ],
            )],
        })
    }

    #[test]
    fn for_with_break() -> anyhow::Result<()> {
        assert_compiles(Program {
            source: r#"
                fn ForWithBreak(xs: int[]) -> int {
                    let mut result = 0;

                    for x in xs {
                        if x > 10 {
                            break;
                        }
                        result += x;
                    }

                    result
                }
                "#,
            expected: vec![(
                "ForWithBreak",
                vec![
                    Instruction::LoadConst(0),
                    Instruction::LoadVar(1),
                    Instruction::LoadGlobal(2),
                    Instruction::LoadVar(3),
                    Instruction::Call(1),
                    Instruction::LoadConst(0),
                    Instruction::LoadVar(5),
                    Instruction::LoadVar(4),
                    Instruction::CmpOp(CmpOp::Lt),
                    Instruction::JumpIfFalse(24),
                    Instruction::Pop(1),
                    Instruction::LoadVar(3),
                    Instruction::LoadVar(5),
                    Instruction::LoadArrayElement,
                    Instruction::LoadVar(5),
                    Instruction::LoadConst(1),
                    Instruction::BinOp(BinOp::Add),
                    Instruction::StoreVar(5),
                    Instruction::LoadVar(6),
                    Instruction::LoadConst(2),
                    Instruction::CmpOp(CmpOp::Gt),
                    Instruction::JumpIfFalse(5),
                    Instruction::Pop(1),
                    Instruction::Pop(1),
                    Instruction::Jump(10),
                    Instruction::Jump(2),
                    Instruction::Pop(1),
                    Instruction::LoadVar(2),
                    Instruction::LoadVar(6),
                    Instruction::BinOp(BinOp::Add),
                    Instruction::StoreVar(2),
                    Instruction::Pop(1),
                    Instruction::Jump(-26),
                    Instruction::Pop(1),
                    Instruction::Pop(3),
                    Instruction::LoadVar(2),
                    Instruction::Return,
                ],
            )],
        })
    }

    #[test]
    fn for_with_continue() -> anyhow::Result<()> {
        assert_compiles(Program {
            source: r#"
                fn ForWithContinue(xs: int[]) -> int {
                    let mut result = 0;

                    for x in xs {
                        if x > 10 {
                            continue;
                        }
                        result += x;
                    }

                    result
                }
                "#,
            expected: vec![(
                "ForWithContinue",
                vec![
                    Instruction::LoadConst(0),
                    Instruction::LoadVar(1),
                    Instruction::LoadGlobal(2),
                    Instruction::LoadVar(3),
                    Instruction::Call(1),
                    Instruction::LoadConst(0),
                    Instruction::LoadVar(5),
                    Instruction::LoadVar(4),
                    Instruction::CmpOp(CmpOp::Lt),
                    Instruction::JumpIfFalse(24),
                    Instruction::Pop(1),
                    Instruction::LoadVar(3),
                    Instruction::LoadVar(5),
                    Instruction::LoadArrayElement,
                    Instruction::LoadVar(5),
                    Instruction::LoadConst(1),
                    Instruction::BinOp(BinOp::Add),
                    Instruction::StoreVar(5),
                    Instruction::LoadVar(6),
                    Instruction::LoadConst(2),
                    Instruction::CmpOp(CmpOp::Gt),
                    Instruction::JumpIfFalse(5),
                    Instruction::Pop(1),
                    Instruction::Pop(1),
                    Instruction::Jump(8),
                    Instruction::Jump(2),
                    Instruction::Pop(1),
                    Instruction::LoadVar(2),
                    Instruction::LoadVar(6),
                    Instruction::BinOp(BinOp::Add),
                    Instruction::StoreVar(2),
                    Instruction::Pop(1),
                    Instruction::Jump(-26),
                    Instruction::Pop(1),
                    Instruction::Pop(3),
                    Instruction::LoadVar(2),
                    Instruction::Return,
                ],
            )],
        })
    }

    #[test]
    fn for_nested() -> anyhow::Result<()> {
        assert_compiles(Program {
            source: r#"
                fn NestedFor(as: int[], bs: int[]) -> int {

                    let mut result = 0;

                    for a in as {
                        for b in bs {
                            result += a * b;
                        }
                    }

                    result
                }
                "#,
            expected: vec![(
                "NestedFor",
                vec![
                    Instruction::LoadConst(0),
                    Instruction::LoadVar(1),
                    Instruction::LoadGlobal(2),
                    Instruction::LoadVar(4),
                    Instruction::Call(1),
                    Instruction::LoadConst(0),
                    Instruction::LoadVar(6),
                    Instruction::LoadVar(5),
                    Instruction::CmpOp(CmpOp::Lt),
                    Instruction::JumpIfFalse(38),
                    Instruction::Pop(1),
                    Instruction::LoadVar(4),
                    Instruction::LoadVar(6),
                    Instruction::LoadArrayElement,
                    Instruction::LoadVar(6),
                    Instruction::LoadConst(1),
                    Instruction::BinOp(BinOp::Add),
                    Instruction::StoreVar(6),
                    Instruction::LoadVar(2),
                    Instruction::LoadGlobal(2),
                    Instruction::LoadVar(8),
                    Instruction::Call(1),
                    Instruction::LoadConst(0),
                    Instruction::LoadVar(10),
                    Instruction::LoadVar(9),
                    Instruction::CmpOp(CmpOp::Lt),
                    Instruction::JumpIfFalse(17),
                    Instruction::Pop(1),
                    Instruction::LoadVar(8),
                    Instruction::LoadVar(10),
                    Instruction::LoadArrayElement,
                    Instruction::LoadVar(10),
                    Instruction::LoadConst(1),
                    Instruction::BinOp(BinOp::Add),
                    Instruction::StoreVar(10),
                    Instruction::LoadVar(3),
                    Instruction::LoadVar(7),
                    Instruction::LoadVar(11),
                    Instruction::BinOp(BinOp::Mul),
                    Instruction::BinOp(BinOp::Add),
                    Instruction::StoreVar(3),
                    Instruction::Pop(1),
                    Instruction::Jump(-19),
                    Instruction::Pop(1),
                    Instruction::Pop(3),
                    Instruction::Pop(1),
                    Instruction::Jump(-40),
                    Instruction::Pop(1),
                    Instruction::Pop(3),
                    Instruction::LoadVar(3),
                    Instruction::Return,
                ],
            )],
        })
    }

    mod return_stmt {
        use super::*;

        #[test]
        fn early_return() -> anyhow::Result<()> {
            assert_compiles(Program {
                source: "
                fn EarlyReturn(x: int) -> int {
                  if x == 42 { return 1; }
                  
                  x + 5
                }
            ",
                expected: vec![(
                    "EarlyReturn",
                    vec![
                        Instruction::LoadVar(1),   // x
                        Instruction::LoadConst(0), // 42
                        Instruction::CmpOp(CmpOp::Eq),
                        Instruction::JumpIfFalse(5), // to 8
                        Instruction::Pop(1),
                        Instruction::LoadConst(1), // 1
                        Instruction::Return,
                        Instruction::Jump(2), // to 9
                        Instruction::Pop(1),
                        Instruction::LoadVar(1),   // x
                        Instruction::LoadConst(2), // 5
                        Instruction::BinOp(BinOp::Add),
                        Instruction::Return,
                    ],
                )],
            })
        }

        #[test]
        fn with_stack() -> anyhow::Result<()> {
            assert_compiles(Program {
                source: "
                fn WithStack(x: int) -> int {
                  let a = 1;

                  // NOTE: currently there's no empty returns.

                  if a == 0 { return 0; }
                  
                  {
                     let b = 1;
                     if a != b {
                        return 0;
                     }
                  }
                  
                  {
                     let c = 2;
                     let b = 3;
                     while b != c {
                        if true {
                           return 0;
                        }
                     }
                  }

                   7
                }
            ",
                expected: vec![(
                    "WithStack",
                    vec![
                        Instruction::LoadConst(0), // 1
                        Instruction::LoadVar(2),   // a
                        Instruction::LoadConst(1), // 0
                        Instruction::CmpOp(CmpOp::Eq),
                        Instruction::JumpIfFalse(5), // to 9
                        Instruction::Pop(1),
                        Instruction::LoadConst(2), // 0
                        Instruction::Return,
                        Instruction::Jump(2), // to 10
                        Instruction::Pop(1),
                        Instruction::LoadConst(3), // 1
                        Instruction::LoadVar(2),   // a
                        Instruction::LoadVar(3),   // b
                        Instruction::CmpOp(CmpOp::NotEq),
                        Instruction::JumpIfFalse(5), // to 19
                        Instruction::Pop(1),
                        Instruction::LoadConst(4), // 0
                        Instruction::Return,
                        Instruction::Jump(2), // to 20
                        Instruction::Pop(1),
                        Instruction::Pop(1),
                        Instruction::LoadConst(5), // 2
                        Instruction::LoadConst(6), // 3
                        Instruction::LoadVar(4),   // b
                        Instruction::LoadVar(3),   // c
                        Instruction::CmpOp(CmpOp::NotEq),
                        Instruction::JumpIfFalse(10), // to 36
                        Instruction::Pop(1),
                        Instruction::LoadConst(7),   // true
                        Instruction::JumpIfFalse(5), // to 34
                        Instruction::Pop(1),
                        Instruction::LoadConst(8), // 0
                        Instruction::Return,
                        Instruction::Jump(2), // to 35
                        Instruction::Pop(1),
                        Instruction::Jump(-12), // to 23
                        Instruction::Pop(1),
                        Instruction::Pop(2),
                        Instruction::LoadConst(9), // 7
                        Instruction::Return,
                    ],
                )],
            })
        }
    }

    #[test]
    fn assert_statement_ok() -> anyhow::Result<()> {
        assert_compiles(Program {
            source: "
                fn assertOk() -> int {
                    assert 2 + 2 == 4;
                    3
                }
            ",
            expected: vec![(
                "assertOk",
                vec![
                    Instruction::LoadConst(0), // 2
                    Instruction::LoadConst(1), // 2
                    Instruction::BinOp(BinOp::Add),
                    Instruction::LoadConst(2), // 4
                    Instruction::CmpOp(CmpOp::Eq),
                    Instruction::Assert,
                    Instruction::LoadConst(3), // 3
                    Instruction::Return,
                ],
            )],
        })
    }

    #[test]
    fn assert_statement_not_ok() -> anyhow::Result<()> {
        assert_compiles(Program {
            source: "
                fn assertNotOk() -> int {
                    assert 3 == 1;
                    2
                }
            ",
            expected: vec![(
                "assertNotOk",
                vec![
                    Instruction::LoadConst(0), // 3
                    Instruction::LoadConst(1), // 1
                    Instruction::CmpOp(CmpOp::Eq),
                    Instruction::Assert,
                    Instruction::LoadConst(2), // 2
                    Instruction::Return,
                ],
            )],
        })
    }
}
