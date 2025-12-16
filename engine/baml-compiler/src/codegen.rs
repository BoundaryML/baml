//! Baml VM bytecode generation.

use std::collections::{HashMap, HashSet};

use baml_types::{ir_type::TypeIR, BamlMap, BamlMediaType, BamlValueWithMeta, TypeValue};
use baml_vm::{
    BamlVmProgram, BinOp, Bytecode, Class, CmpOp, Enum, Function, FunctionKind, GlobalIndex,
    GlobalPool, Instruction, Object, ObjectIndex, ObjectPool, UnaryOp, Value,
};
use internal_baml_ast::ast::WithName;
use internal_baml_diagnostics::{Diagnostics, Span};
use internal_baml_parser_database::ParserDatabase;

use crate::{
    hir::{self},
    thir::{self, ClassConstructorField},
    viz::VizNodes,
    viz_builder::build_viz_nodes,
    watch::WatchWhen,
};

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

    // TODO: THIR is built twice, once for validations, once for compilation.
    // Fix this.
    let thir = thir::typecheck::typecheck(&hir, &mut Diagnostics::new("dummy".into()));

    // eprintln!("\nTHIR:\n{:#?}", thir);

    // Stage 2: HIR -> Bytecode
    compile_thir_to_bytecode(&thir)
}

/// Compile HIR to bytecode.
///
/// This function takes an HIR Program and generates the bytecode for the VM.
fn compile_thir_to_bytecode(
    thir: &thir::THir<(Span, Option<TypeIR>)>,
) -> anyhow::Result<BamlVmProgram> {
    let mut resolved_globals = BamlMap::new();
    let mut resolved_classes = BamlMap::new();
    let mut resolved_enums = BamlMap::new();
    let mut llm_functions = HashSet::new();

    // Resolve global functions from HIR
    for func in &thir.expr_functions {
        resolved_globals.insert(
            func.name.clone(),
            GlobalIndex::from_raw(resolved_globals.len()),
        );
    }

    for func in &thir.llm_functions {
        resolved_globals.insert(
            func.name.clone(),
            GlobalIndex::from_raw(resolved_globals.len()),
        );
        llm_functions.insert(func.name.clone());
    }

    // Resolve classes from HIR
    for class in thir.classes.values() {
        resolved_globals.insert(
            class.name.clone(),
            GlobalIndex::from_raw(resolved_globals.len()),
        );

        // Resolve class fields.
        let mut class_fields = HashMap::new();
        for (field_index, field) in class.fields.iter().enumerate() {
            class_fields.insert(field.name.clone(), field_index);
        }

        resolved_classes.insert(class.name.clone(), class_fields);
    }

    for class in thir.classes.values() {
        for method in &class.methods {
            let func_name = format!("{}.{}", class.name, method.name);
            resolved_globals.insert(func_name, GlobalIndex::from_raw(resolved_globals.len()));
        }
    }

    for enm in thir.enums.values() {
        resolved_globals.insert(
            enm.name.clone(),
            GlobalIndex::from_raw(resolved_globals.len()),
        );

        let mut variant_names = HashMap::new();

        for (variant_index, variant) in enm.variants.iter().enumerate() {
            variant_names.insert(variant.name.clone(), variant_index);
        }

        resolved_enums.insert(enm.name.clone(), variant_names);
    }

    let native_fns = baml_vm::native::functions();

    for name in native_fns.keys() {
        resolved_globals.insert(name.clone(), GlobalIndex::from_raw(resolved_globals.len()));
    }
    resolved_globals.insert(
        "baml.fetch_as".to_string(),
        GlobalIndex::from_raw(resolved_globals.len()),
    );

    let mut objects = ObjectPool::from_vec(Vec::with_capacity(resolved_globals.len()));
    let mut globals = GlobalPool::from_vec(Vec::with_capacity(resolved_globals.len()));

    let mut loop_var_counter = ForLoopVarCounters::new();

    let mut fn_class_patch_lists = Vec::with_capacity(thir.expr_functions.len());

    // Compile HIR functions to bytecode
    for func in &thir.expr_functions {
        let mut class_alloc_patch_list = Vec::new();

        let bytecode_function = compile_thir_function(
            func,
            &resolved_globals,
            &resolved_classes,
            &resolved_enums,
            &llm_functions,
            &mut loop_var_counter,
            &mut objects,
            &mut class_alloc_patch_list,
        )?;

        // Add the function to the globals and objects pools.
        let object_index = objects.insert(Object::Function(bytecode_function));
        fn_class_patch_lists.push((object_index, class_alloc_patch_list));
        globals.push(Value::Object(object_index));
    }

    for func in &thir.llm_functions {
        let bytecode_llm_function = Object::Function(Function {
            name: func.name.clone(),
            arity: func.parameters.len(),
            bytecode: Bytecode::new(),
            kind: FunctionKind::Llm,
            locals_in_scope: vec![func.parameters.iter().map(|p| p.name.clone()).collect()],
            span: func.span.clone(),
            viz_nodes: Vec::new(),
        });

        let object_index = objects.insert(bytecode_llm_function);
        globals.push(Value::Object(object_index));
    }

    // Add classes to objects
    for class in thir.classes.values() {
        let bytecode_class = Class {
            name: class.name.clone(),
            field_names: class.fields.iter().map(|f| f.name.clone()).collect(),
        };

        let object_index = objects.insert(Object::Class(bytecode_class));
        globals.push(Value::Object(object_index));
    }

    for class in thir.classes.values() {
        for method in &class.methods {
            let mut class_alloc_patch_list = Vec::new();

            let mut bytecode_function = compile_thir_function(
                method,
                &resolved_globals,
                &resolved_classes,
                &resolved_enums,
                &llm_functions,
                &mut loop_var_counter,
                &mut objects,
                &mut class_alloc_patch_list,
            )?;

            bytecode_function.name = format!("{}.{}", class.name, method.name);

            // Add the function to the globals and objects pools.
            let object_index = objects.insert(Object::Function(bytecode_function));
            fn_class_patch_lists.push((object_index, class_alloc_patch_list));
            globals.push(Value::Object(object_index));
        }
    }

    for enm in thir.enums.values() {
        let bytecode_enum = Enum {
            name: enm.name.clone(),
            variant_names: enm.variants.iter().map(|v| v.name.clone()).collect(),
        };

        let object_index = objects.insert(Object::Enum(bytecode_enum));
        globals.push(Value::Object(object_index));
    }

    // resolve classes into their instance creation insns now that we've got their locations.
    // NOTE: memory locality is not great, review if it gets annoying.
    // Right now we're grouping by function & effectively random-accessing globals. Since
    // locations are pushed in compilation order, they are monotonically increasing.
    for (func_index, patch_list) in fn_class_patch_lists {
        let Object::Function(Function { bytecode, .. }) = &mut objects[func_index] else {
            panic!("should have a compiled function here!");
        };

        for AllocInstancePatch { location, global } in patch_list {
            let Value::Object(object_index) = globals[global] else {
                panic!("must have a class global here! The expected class may not be in the place resolved by `globals`");
            };

            match &mut bytecode.instructions[location] {
                Instruction::AllocInstance(index) | Instruction::AllocVariant(index) => {
                    *index = object_index;
                }

                other => panic!("alloc instance patch list must contain locations to AllocInstance or AllocVariant! Got: {other}"),
            }
        }
    }

    for (name, (func, arity)) in native_fns {
        let native_function = Object::Function(Function {
            name: name.clone(),
            arity,
            bytecode: Bytecode::new(),
            kind: FunctionKind::Native(func),
            locals_in_scope: vec![], // TODO.
            span: Span::fake_builtin_baml(),
            viz_nodes: Vec::new(),
        });

        let object_index = objects.insert(native_function);
        globals.push(Value::Object(object_index));
    }
    globals.push(Value::Object(objects.insert(Object::Function(Function {
        name: "baml.fetch_as".to_string(),
        arity: 2,
        bytecode: Bytecode::new(),
        kind: FunctionKind::Future,
        locals_in_scope: vec![],
        span: Span::fake_builtin_baml(),
        viz_nodes: Vec::new(),
    }))));

    let mut resolved_class_names = HashMap::new();
    let mut resolved_function_names = HashMap::new();
    let mut resolved_enums_names = HashMap::new();

    for (i, object) in objects.iter().enumerate() {
        match object {
            Object::Class(c) => {
                resolved_class_names.insert(c.name.clone(), ObjectIndex::from_raw(i));
            }
            Object::Function(f) => {
                resolved_function_names.insert(f.name.clone(), (ObjectIndex::from_raw(i), f.kind));
            }
            Object::Enum(e) => {
                resolved_enums_names.insert(e.name.clone(), ObjectIndex::from_raw(i));
            }
            _ => {}
        }
    }

    Ok(BamlVmProgram {
        objects,
        globals,
        resolved_function_names,
        resolved_class_names,
        resolved_enums_names,
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
/// TODO: Fix this shit.
#[allow(clippy::too_many_arguments)]
fn compile_thir_function(
    func: &thir::ExprFunction<(Span, Option<TypeIR>)>,
    globals: &BamlMap<String, GlobalIndex>,
    classes: &BamlMap<String, HashMap<String, usize>>,
    enums: &BamlMap<String, HashMap<String, usize>>,
    llm_functions: &HashSet<String>,
    loop_var_counter: &mut ForLoopVarCounters,
    objects: &mut ObjectPool,
    class_alloc_patch_list: &mut Vec<AllocInstancePatch>,
) -> anyhow::Result<Function> {
    let mut compiler = HirCompiler::new(
        globals,
        classes,
        enums,
        llm_functions,
        loop_var_counter,
        objects,
        class_alloc_patch_list,
    );
    compiler.viz_nodes = build_viz_nodes(func);
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

#[derive(Clone, Copy, Debug)]
struct AllocInstancePatch {
    location: usize,
    global: GlobalIndex,
}

/// HIR to bytecode compiler.
struct HirCompiler<'g> {
    /// Resolved global variables.
    ///
    /// Maps the name of the global variable to its index in the globals pool.
    globals: &'g BamlMap<String, GlobalIndex>,

    /// Resolved class fields.
    ///
    /// Maps the name of the class to the field resolution. Field resolution
    /// is basically a transformation of field name to an index in an array.
    ///
    /// TODO: The `g` lifetime here doesn't need to be the same as the globals
    /// lifetime.
    classes: &'g BamlMap<String, HashMap<String, usize>>,

    /// Resolved enum variants.
    enums: &'g BamlMap<String, HashMap<String, usize>>,

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
    objects: &'g mut ObjectPool,

    /// `AllocInstance` instructions that have a placeholder, which must be resolved when location
    /// of the class object is resolved.
    class_alloc_patch_list: &'g mut Vec<AllocInstancePatch>,

    /// Control-flow visualization nodes for the current function.
    viz_nodes: VizNodes,

    /// Sequential viz node index allocation while emitting instructions.
    viz_next_index: usize,

    /// Stack of open viz scope indices that need exits.
    viz_open_stack: Vec<usize>,
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
    /// Visualization stack depth before entering the loop iteration.
    pub viz_depth: usize,
}

impl<'g> HirCompiler<'g> {
    fn new(
        globals: &'g BamlMap<String, GlobalIndex>,
        classes: &'g BamlMap<String, HashMap<String, usize>>,
        enums: &'g BamlMap<String, HashMap<String, usize>>,
        llm_functions: &'g HashSet<String>,
        var_counters: &'g mut ForLoopVarCounters,
        objects: &'g mut ObjectPool,
        class_alloc_patch_list: &'g mut Vec<AllocInstancePatch>,
    ) -> Self {
        Self {
            globals,
            classes,
            enums,
            llm_functions,
            objects,
            class_alloc_patch_list,
            locals: HashMap::new(),
            var_counters,
            current_loop: None,
            bytecode: Bytecode::new(),
            scopes: Vec::new(),
            current_source_line: 0,
            locals_in_scope: Vec::new(),
            viz_nodes: VizNodes::new(),
            viz_next_index: 0,
            viz_open_stack: Vec::new(),
        }
    }

    /// Main entry point.
    ///
    /// Here we compile a source function into a [`Function`] VM struct.
    fn compile_function(
        &mut self,
        func: &thir::ExprFunction<(Span, Option<TypeIR>)>,
    ) -> anyhow::Result<Function> {
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

            span: func.span.clone(),
            viz_nodes: std::mem::take(&mut self.viz_nodes).into_vm_meta(),
        })
    }

    /// Entry for function or scope compilations.
    ///
    /// Functions have parameters so we need to track those as well.
    fn compile_block_with_parameters(
        &mut self,
        block: &thir::Block<(Span, Option<TypeIR>)>,
        parameters: &[thir::Parameter],
    ) {
        let is_root = self.scopes.is_empty();
        self.enter_scope();
        if is_root {
            self.viz_enter_scope();
        }

        for param in parameters {
            self.track_local(&param.name);
        }

        for statement in &block.statements {
            self.compile_statement(statement);
        }

        let scope_has_trailing_expr = match &block.trailing_expr {
            None => false,

            Some(trailing_expr) => {
                self.compile_expression(trailing_expr);
                true
            }
        };

        self.exit_scope(scope_has_trailing_expr);
    }

    /// Used to compile nested blocks within functions.
    fn compile_block(&mut self, block: &thir::Block<(Span, Option<TypeIR>)>) {
        self.compile_block_with_parameters(block, &[]);
    }

    /// A statement is anything that does not produce a value by itself.
    fn compile_statement(&mut self, statement: &thir::Statement<(Span, Option<TypeIR>)>) {
        match statement {
            thir::Statement::HeaderContextEnter(_header) => {
                self.viz_emit_enter(false);
            }
            thir::Statement::Let { name, value, .. } => {
                self.compile_expression_with_block_behavior(value, true);
                self.track_local(name);
            }
            thir::Statement::Declare { name, .. } => {
                self.declare_mut(name);
            }
            thir::Statement::Assign { left, value, .. } => {
                match left {
                    thir::Expr::Var(name, _) => {
                        self.compile_expression_with_block_behavior(value, true);
                        self.emit(Instruction::StoreVar(self.locals[name]));
                    }
                    thir::Expr::FieldAccess { base, field, meta: _ } => {
                        // Get class name from type metadata
                        let class_name = match base.meta().1.as_ref() {
                            Some(TypeIR::Class { name, .. }) => name,
                            _ => panic!("Field access on non-class type"),
                        };

                        // Resolve field index
                        let Some(resolved_fields) = self.classes.get(class_name) else {
                            panic!("undefined class: {class_name}");
                        };
                        let Some(&field_index) = resolved_fields.get(field) else {
                            panic!("undefined field: {class_name}.{field}");
                        };

                        // Generate bytecode: load base, load value, store field
                        self.compile_expression(base);
                        self.compile_expression_with_block_behavior(value, true);
                        self.emit(Instruction::StoreField(field_index));
                    }
                    thir::Expr::ArrayAccess {base, index, meta: _} => {

                        self.compile_expression(base);
                        self.compile_expression(index);
                        self.compile_expression_with_block_behavior(value, true);

                        self.emit(match base.meta().1.as_ref().expect("must have a resolved type") {
                            TypeIR::List(_, _) => Instruction::StoreArrayElement,
                            TypeIR::Map(_, _, _) => Instruction::StoreMapElement,
                            _ => panic!("array access should be either map or array.")
                        });

                    }
                    _ => panic!("Invalid left hand of assignment, only variables, instance fields and array elements can be assigned"),
                }
            }
            thir::Statement::AssignOp {
                left,
                value,
                assign_op,
                ..
            } => {
                let binop = match assign_op {
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
                };

                match left {
                    thir::Expr::Var(name, _) => {
                        self.emit(Instruction::LoadVar(self.locals[name]));
                        self.compile_expression(value);
                        self.emit(binop);
                        self.emit(Instruction::StoreVar(self.locals[name]));
                    }
                    thir::Expr::FieldAccess { base, field, meta: _ } => {
                        // Get class name from type metadata
                        let class_name = match base.meta().1.as_ref() {
                            Some(TypeIR::Class { name, .. }) => name,
                            _ => panic!("Field access on non-class type"),
                        };

                        // Resolve field index
                        let Some(resolved_fields) = self.classes.get(class_name) else {
                            panic!("undefined class: {class_name}");
                        };
                        let Some(&field_index) = resolved_fields.get(field) else {
                            panic!("undefined field: {class_name}.{field}");
                        };

                        // For obj.field += value, generate:
                        // 1. Load object
                        // 2. Copy object reference (Copy 0)
                        // 3. Load field value
                        // 4. Load value
                        // 5. Apply operation
                        // 6. Store back to field (uses copied object reference)

                        self.compile_expression(base);
                        self.emit(Instruction::Copy(0));  // Duplicate object reference
                        self.emit(Instruction::LoadField(field_index));
                        self.compile_expression(value);
                        self.emit(binop);
                        self.emit(Instruction::StoreField(field_index));
                    }
                    thir::Expr::ArrayAccess { base, index, meta: _ } => {
                        // Compound Assignment for array[index] or map[key]
                        //
                        // For array[index] += value (or other compound ops):
                        //
                        // Stack evolution:
                        // 1. Load array and index -> [array, index]
                        // 2. Duplicate both for load -> [array, index, array_copy, index_copy]
                        // 3. Load current value -> [array, index, current_value]
                        //    (LoadArrayElement consumes array_copy and index_copy)
                        // 4. Load value to operate with -> [array, index, current_value, value]
                        // 5. Apply binary operation -> [array, index, result]
                        //    (BinOp consumes current_value and value)
                        // 6. Store back to array[index] -> []
                        //    (StoreArrayElement consumes array, index, and result)
                        //
                        // The same pattern applies for maps with StoreMapElement

                        // Determine if it's a list or map
                        let (load_instr, store_instr) = match base.meta().1.as_ref().expect("must have a resolved type") {
                            TypeIR::List(_, _) => (Instruction::LoadArrayElement, Instruction::StoreArrayElement),
                            TypeIR::Map(_, _, _) => (Instruction::LoadMapElement, Instruction::StoreMapElement),
                            _ => panic!("array access should be either map or array.")
                        };

                        // Load array and index first
                        self.compile_expression(base);
                        self.compile_expression(index);

                        // Stack is now: [array, index]
                        // Duplicate both for the load operation
                        self.emit(Instruction::Copy(1));  // Copy array (at position 1 from top)
                        self.emit(Instruction::Copy(1));  // Copy index (at position 1 from top)

                        // Stack is now: [array, index, array_copy, index_copy]
                        // Load current value at array[index]
                        // This consumes array_copy and index_copy
                        self.emit(load_instr);

                        // Stack is now: [array, index, current_value]
                        // Load the value to apply operation with
                        self.compile_expression(value);

                        // Stack is now: [array, index, current_value, new_value]
                        // Apply the operation
                        self.emit(binop);

                        // Stack is now: [array, index, result]
                        // Store back to array[index]
                        // This consumes array, index, and result value
                        self.emit(store_instr);
                    }
                    _ => panic!("Invalid left hand of assignment, only variables, instance fields and array elements can be assigned"),
                }
            }
            thir::Statement::DeclareAndAssign {
                name, value, watch, ..
            } => {
                self.compile_expression_with_block_behavior(value, true);
                let local_index = self.track_local(name);
                if let Some(spec) = watch {
                    self.emit_string_literal(&spec.name); // This adds LoadConst

                    match &spec.when {
                        WatchWhen::FunctionName(fn_name) => {
                            if let Some(&index) = self.globals.get(fn_name.name()) {
                                self.emit(Instruction::LoadGlobal(index));
                            } else {
                                panic!("undefined function: {name}");
                            }
                        }
                        WatchWhen::Never => {}

                        WatchWhen::Manual => {
                            self.emit_string_literal("manual");
                        }

                        WatchWhen::Auto => {
                            let index = self.add_constant(Value::Null);
                            self.emit(Instruction::LoadConst(index));
                        }
                    }

                    self.emit(Instruction::Watch(local_index));
                }
            }
            thir::Statement::Return { expr, .. } => {
                self.compile_expression(expr);
                self.viz_exit_to_depth(0);
                self.emit(Instruction::Return);
            }
            thir::Statement::Expression { expr, .. } => {
                self.compile_expression_with_block_behavior(expr, true);
            }
            thir::Statement::SemicolonExpression { expr, .. } => {
                self.compile_expression_with_block_behavior(expr, true);
                // This could be a function call or any other random expression
                // like:
                //
                // 2 + 2;
                //
                // But since the result is not stored anywhere (not a let
                // binding) then implicitly drop the value.
                self.emit(Instruction::Pop(1));
            }
            thir::Statement::ForLoop {
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
                    .get("baml.Array.length")
                    .expect("native baml.Array.length() for array length is not in globals?");

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
            thir::Statement::While {
                condition, block, ..
            } => {
                self.compile_while_loop(
                    |ctx| ctx.compile_expression(condition),
                    |ctx| ctx.compile_block(block),
                    |_| {},
                );
            }
            thir::Statement::Break(_) => {
                let viz_depth = self.assert_loop("break").viz_depth;

                // since we are exiting the loop context, make sure we drop everything before
                // breaking!
                let pop_until = self.assert_loop("break").scope_depth;
                self.emit_scope_drops(pop_until);
                self.viz_exit_to_depth(viz_depth);

                let exit_jump = self.next_insn_index() as usize;
                self.assert_loop("break").break_patch_list.push(exit_jump);

                // NOTE: right now this will generate redundant code when using
                // `if condition { break }`, since `if` will generate its own jump location and we
                // will end up with a conditional jump and a regular jump together.
                self.emit(Instruction::Jump(0));
            }
            thir::Statement::Continue(_) => {
                let viz_depth = self.assert_loop("continue").viz_depth;

                let pop_until = self.assert_loop("continue").scope_depth;
                self.emit_scope_drops(pop_until);
                self.viz_exit_to_depth(viz_depth);

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
            thir::Statement::CForLoop {
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
            thir::Statement::Assert { condition, .. } => {
                self.compile_expression(condition);
                self.emit(Instruction::Assert);
            }
            thir::Statement::WatchOptions {
                variable,
                channel,
                when,
                ..
            } => {
                let Some(local_index) = self.locals.get(variable).copied() else {
                    panic!("watch codegen error: undefined variable: {variable}");
                };

                self.emit_string_literal(channel.as_ref().unwrap_or(variable).as_str()); // This adds LoadConst

                match when.as_ref() {
                    Some(WatchWhen::Manual) => {
                        self.emit_string_literal("manual");
                    }

                    Some(WatchWhen::Never) => {
                        self.emit_string_literal("never");
                    }

                    Some(WatchWhen::Auto) => {
                        // No action needed.
                    }

                    Some(WatchWhen::FunctionName(fn_name)) => {
                        if let Some(&index) = self.globals.get(fn_name.name()) {
                            self.emit(Instruction::LoadGlobal(index));
                        } else {
                            panic!("watch options codegen: undefined function: {fn_name}");
                        }
                    }

                    None => {
                        let index = self.add_constant(Value::Null);
                        self.emit(Instruction::LoadConst(index));
                    }
                }

                self.emit(Instruction::Watch(local_index));
            }
            thir::Statement::WatchNotify { variable, .. } => {
                let Some(local_index) = self.locals.get(variable).copied() else {
                    panic!("watch codegen error: undefined variable: {variable}");
                };

                self.emit(Instruction::Notify(local_index));
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
    fn compile_expression(&mut self, expr: &thir::Expr<(Span, Option<TypeIR>)>) {
        // TODO: The implementation of line number is extremely slow. It always
        // reads the entire source string to find the line number.
        self.current_source_line = expr.span().line_number();

        match expr {
            thir::Expr::Value(value) => match value {
                BamlValueWithMeta::Null(_) => {
                    let index = self.add_constant(Value::Null);
                    self.emit(Instruction::LoadConst(index));
                }

                BamlValueWithMeta::Bool(v, _) => {
                    let index = self.add_constant(Value::Bool(*v));
                    self.emit(Instruction::LoadConst(index));
                }

                BamlValueWithMeta::Int(v, _) => {
                    let index = self.add_constant(Value::Int(*v));
                    self.emit(Instruction::LoadConst(index));
                }

                BamlValueWithMeta::Float(v, _) => {
                    let index = self.add_constant(Value::Float(*v));
                    self.emit(Instruction::LoadConst(index));
                }

                BamlValueWithMeta::String(v, _) => self.emit_string_literal(v),

                _ => panic!("unsupported atom: {value:#?}"),
            },

            thir::Expr::Block(block, _) => self.compile_block(block),

            thir::Expr::ArrayAccess {
                base,
                index,
                meta: _,
            } => {
                // ArrayAccess compilation for loading elements
                //
                // Steps to compile array[index] or map[key]:
                // 1. Compile the base expression (array or map)
                // 2. Compile the index/key expression
                // 3. Determine the type from metadata (List or Map)
                // 4. Emit the appropriate load instruction:
                //    - LoadArrayElement for arrays (expects integer index)
                //    - LoadMapElement for maps (expects string key)
                //
                // Stack evolution:
                // - After base: [array_or_map]
                // - After index: [array_or_map, index_or_key]
                // - After load: [element_value]

                self.compile_expression(base);
                self.compile_expression(index);

                // Determine if it's an array or map and emit appropriate instruction
                self.emit(
                    match base.meta().1.as_ref().expect("must have a resolved type") {
                        TypeIR::List(_, _) => Instruction::LoadArrayElement,
                        TypeIR::Map(_, _, _) => Instruction::LoadMapElement,
                        _ => panic!("array access should be either map or array."),
                    },
                );
            }

            thir::Expr::FieldAccess { base, field, .. } => {
                // Direct enum access: Share.Rectangle
                if let thir::Expr::Var(name, _) = base.as_ref() {
                    if let Some(enm) = self.enums.get(name) {
                        let Some(variant_index) = enm.get(field) else {
                            panic!("undefined enum variant: {name}.{field}");
                        };

                        let Some(enum_index) = self.globals.get(name) else {
                            panic!("undefined enum: {name}");
                        };

                        let const_index = self.add_constant(Value::Int(*variant_index as i64));
                        self.emit(Instruction::LoadConst(const_index));

                        let allocation_instruction =
                            self.emit(Instruction::AllocVariant(ObjectIndex::from_raw(usize::MAX)));

                        // TODO: Confusing name because of class alloc reuse.
                        self.class_alloc_patch_list.push(AllocInstancePatch {
                            location: allocation_instruction,
                            global: *enum_index,
                        });

                        return;
                    }
                }

                // First compile the base expression
                self.compile_expression(base);

                // Now get the type of the base to resolve the field
                match base.meta().1.as_ref() {
                    Some(TypeIR::Class {
                        name: class_name, ..
                    }) => {
                        let Some(_class_index) = self.globals.get(class_name) else {
                            panic!("undefined class: {class_name}");
                        };

                        let Some(resolved_fields) = self.classes.get(class_name) else {
                            panic!("undefined class: {class_name}");
                        };

                        let Some(&field_index) = resolved_fields.get(field) else {
                            panic!("undefined field: {class_name}.{field}");
                        };

                        self.emit(Instruction::LoadField(field_index));
                    }

                    other => panic!(
                        "field access must be on classes, but expr `{}` got: {other:?}",
                        base.dump_str()
                    ),
                }
            }

            thir::Expr::Var(name, _) => {
                if let Some(&index) = self.locals.get(name) {
                    self.emit(Instruction::LoadVar(index));
                } else if let Some(class) = self.globals.get(name) {
                    self.emit(Instruction::LoadGlobal(*class));
                } else {
                    panic!("undefined variable: {name}");
                }
            }

            thir::Expr::List(elements, _) => {
                for element in elements {
                    self.compile_expression(element);
                }
                self.emit(Instruction::AllocArray(elements.len()));
            }

            thir::Expr::Map(pairs, _) => {
                // Maps are not yet implemented in bytecode
                // have N keys, N values.
                // keys are popped first, so we first compute the values.

                for (_, value) in pairs {
                    self.compile_expression(value);
                }

                for (key, _) in pairs {
                    self.emit_string_literal(key);
                }

                self.emit(Instruction::AllocMap(pairs.len()));
            }

            thir::Expr::Call {
                func,
                args,
                type_args,
                ..
            } => {
                let name = match func.as_ref() {
                    thir::Expr::Var(name, _) => name,
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

                // Type parameter. TODO: Generic way of handling this?
                if name == "baml.fetch_as" {
                    let type_index = self.objects.insert(Object::BamlType(type_args[0].clone()));
                    let const_index = self.add_constant(Value::Object(type_index));
                    self.emit(Instruction::LoadConst(const_index));
                }

                // Either async LLM call or regular function call.
                if self.llm_functions.contains(name) || name == "baml.fetch_as" {
                    let count = if name == "baml.fetch_as" {
                        2
                    } else {
                        args.len()
                    };

                    self.emit(Instruction::DispatchFuture(count));
                    self.emit(Instruction::Await);
                } else {
                    self.emit(Instruction::Call(args.len()));
                }
            }

            thir::Expr::MethodCall {
                receiver,
                method,
                args,
                ..
            } => {
                let thir::Expr::Var(method, _) = method.as_ref() else {
                    panic!("method calls must be identifiers");
                };

                let func_name = match receiver.meta().1.as_ref() {
                    Some(TypeIR::Class {
                        name: class_name, ..
                    }) => format!("{class_name}.{method}"),

                    Some(TypeIR::List(_, _)) => format!("baml.Array.{method}"),

                    Some(TypeIR::Map(_, _, _)) => format!("baml.Map.{method}"),

                    Some(TypeIR::Primitive(TypeValue::String, _)) => {
                        format!("baml.String.{method}")
                    }

                    Some(TypeIR::Primitive(TypeValue::Media(media_type), _)) => {
                        let subtype = match media_type {
                            BamlMediaType::Image => "baml.media.image",
                            BamlMediaType::Video => "baml.media.video",
                            BamlMediaType::Audio => "baml.media.audio",
                            BamlMediaType::Pdf => "baml.media.pdf",
                        };

                        format!("{subtype}.{method}")
                    }

                    other => panic!("method calls must be on classes, got: {other:#?}"),
                };

                // Push the function onto the stack
                let Some(&index) = self.globals.get(&func_name) else {
                    panic!("undefined method: {func_name}");
                };

                self.emit(Instruction::LoadGlobal(index));

                self.compile_expression(receiver);

                for arg in args {
                    self.compile_expression(arg);
                }

                // `self` counts as one argument.
                self.emit(Instruction::Call(1 + args.len()));
            }

            thir::Expr::ClassConstructor {
                name: class_name,
                fields,
                meta: _,
            } => {
                // TODO: Long-term solution - Refactor AllocInstance to consume fields from stack
                // like AllocArray does. This would eliminate the need for Copy/StoreField pattern
                // and naturally handle nested construction. The approach would compile all field
                // values onto the stack first, then AllocInstance(class, field_count) would
                // consume them all at once, creating a fully initialized instance.
                // See: Stack-Based AllocInstance approach in field_access_assignments_implementation.md

                let Some(&class_index) = self.globals.get(class_name) else {
                    panic!("undefined class: {class_name}");
                };

                let Some(resolved_fields) = self.classes.get(class_name) else {
                    panic!("undefined class: {class_name}");
                };

                // Emit allocation with bogus index. It will be patched later.
                let allocation_loc = self.emit(Instruction::AllocInstance(ObjectIndex::from_raw(
                    usize::MAX,
                )));
                self.class_alloc_patch_list.push(AllocInstancePatch {
                    location: allocation_loc,
                    global: class_index,
                });

                // Evaluate only needed expressions. For example:
                //
                // let object = Obj {
                //     ...spread_one(),
                //     ...spread_two(),
                //     x: 1,
                // }
                //
                // Would only really need to evaluate spread_two() because it
                // would override all the values in spread_one().
                let mut evaluate_fields = Vec::new();
                let mut defined_named_fields = HashSet::new();

                for field in fields.iter().rev() {
                    match field {
                        ClassConstructorField::Named { name, .. } => {
                            // Dedup named fields.
                            if defined_named_fields.insert(name.clone()) {
                                evaluate_fields.push(field);
                            }
                        }
                        ClassConstructorField::Spread { .. } => {
                            // Eval spread only if we're missing some field.
                            if resolved_fields
                                .keys()
                                .any(|name| !defined_named_fields.contains(name))
                            {
                                evaluate_fields.push(field);
                            }

                            // Short circuit on spreads.
                            break;
                        }
                    }
                }

                // Not sorted cause of hashmap, tried using sorted map and
                // it didn't work either, figure out what's going on.
                let mut sorted_fields = resolved_fields
                    .iter()
                    .map(|(name, index)| (name, *index))
                    .collect::<Vec<_>>();
                sorted_fields.sort_by_key(|(_, index)| *index);

                for field in evaluate_fields.iter().rev() {
                    match field {
                        ClassConstructorField::Named {
                            name: field_name,
                            value,
                        } => {
                            let Some(&field_index) = resolved_fields.get(field_name) else {
                                panic!("undefined field: {class_name}.{field_name}");
                            };

                            // Instance is always on top of stack after AllocInstance
                            // Copy it to work with it
                            self.emit(Instruction::Copy(0));
                            self.compile_expression(value);
                            self.emit(Instruction::StoreField(field_index));
                        }

                        ClassConstructorField::Spread { value } => {
                            self.compile_expression(value);

                            // Stack state after compiling spread:
                            // [locals..., allocated_instance, spread_value]
                            //                                       ^-- position 0 from top (Copy(0))
                            //                    ^-- position 1 from top (Copy(1))
                            //
                            // We'll use Copy to access both values regardless of nesting level
                            for (field_name, field_index) in &sorted_fields {
                                if !defined_named_fields.contains(*field_name) {
                                    // Current stack: [locals..., allocated_instance, spread_value]

                                    // Copy instance from position 1 (under spread)
                                    // Stack becomes: [locals..., allocated_instance, spread_value, allocated_instance]
                                    self.emit(Instruction::Copy(1));

                                    // Copy spread from position 1 (now under instance copy)
                                    // Stack becomes: [locals..., allocated_instance, spread_value, allocated_instance, spread_value]
                                    self.emit(Instruction::Copy(1));

                                    // Load field from spread
                                    // Stack becomes: [locals..., allocated_instance, spread_value, allocated_instance, field_value]
                                    self.emit(Instruction::LoadField(*field_index));

                                    // Store field to instance
                                    // Stack becomes: [locals..., allocated_instance, spread_value]
                                    self.emit(Instruction::StoreField(*field_index));
                                }
                            }

                            // Get rid of spread local, won't be used anymore.
                            self.emit(Instruction::Pop(1));
                        }
                    }
                }
            }

            thir::Expr::If(condition, if_branch, else_branch, _) => {
                let group_depth = self.viz_open_stack.len();
                self.viz_enter_scope();

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
                let arm_depth = self.viz_open_stack.len();
                self.viz_enter_scope();
                self.compile_expression(if_branch);
                self.viz_exit_to_depth(arm_depth);

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
                    let arm_depth = self.viz_open_stack.len();
                    self.viz_enter_scope();
                    self.compile_expression(else_branch);
                    self.viz_exit_to_depth(arm_depth);
                }

                // Patch the skip else jump. If there's no else, this will
                // simply skip the POP above, because the if branch has its
                // own POP. We can simplify this stuff by creating a specialized
                // POP_JUMP instruction like Python does, but for now I want
                // the simplest possible VM (very limited instructions).
                self.patch_jump(skip_else);

                // Close the branch group scope after both arms have been handled.
                // This ensures the branch group exits on both true and false paths.
                self.viz_exit_to_depth(group_depth);
            }

            thir::Expr::BinaryOperation {
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

                            // Instanceof operator.
                            hir::BinaryOperator::InstanceOf => Instruction::CmpOp(CmpOp::InstanceOf),

                            // Logical operators.
                            hir::BinaryOperator::And | hir::BinaryOperator::Or => unreachable!(
                                "compiler bug: logical binary operators must be handled before arithmetic and comparison operators"
                            ),
                        });
                    }
                }
            }

            thir::Expr::UnaryOperation { operator, expr, .. } => {
                self.compile_expression(expr);

                self.emit(match operator {
                    hir::UnaryOperator::Not => Instruction::UnaryOp(UnaryOp::Not),
                    hir::UnaryOperator::Neg => Instruction::UnaryOp(UnaryOp::Neg),
                });
            }

            thir::Expr::Paren(expr, _) => {
                self.compile_expression(expr);
            }

            thir::Expr::Function(_, _, _) | thir::Expr::Builtin(_, _) => {
                todo!("unsupported expression: {:#?}", expr)
            }
        }
    }

    /// Compiles an expression while mirroring viz block behavior from the viz builder.
    /// When `wrap_block_in_viz` is true and the expression is a block, we emit a viz scope
    /// around the block to keep bytecode visualization instructions aligned with the
    /// precomputed viz node order.
    fn compile_expression_with_block_behavior(
        &mut self,
        expr: &thir::Expr<(Span, Option<TypeIR>)>,
        wrap_block_in_viz: bool,
    ) {
        if wrap_block_in_viz {
            if let thir::Expr::Block(block, _) = expr {
                let depth = self.viz_open_stack.len();
                self.viz_enter_scope();
                self.compile_block(block);
                self.viz_exit_to_depth(depth);
                return;
            }
        }

        self.compile_expression(expr);
    }

    fn emit_string_literal(&mut self, v: &str) {
        // Allocate the string in the objects pool
        let object_index = self.objects.insert(Object::String(v.to_owned()));
        // Add a constant that points to the string object
        let const_index = self.add_constant(Value::Object(object_index));
        self.emit(Instruction::LoadConst(const_index));
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

    fn viz_emit_enter(&mut self, push_on_stack: bool) -> Option<usize> {
        if self.viz_next_index >= self.viz_nodes.len() {
            return None;
        }
        let idx = self.viz_next_index;
        self.viz_next_index += 1;
        self.emit(Instruction::VizEnter(idx));
        if push_on_stack {
            self.viz_open_stack.push(idx);
        }
        Some(idx)
    }

    fn viz_enter_scope(&mut self) -> Option<usize> {
        self.viz_emit_enter(true)
    }

    fn viz_exit_to_depth(&mut self, target_depth: usize) {
        while self.viz_open_stack.len() > target_depth {
            if let Some(idx) = self.viz_open_stack.pop() {
                self.emit(Instruction::VizExit(idx));
            }
        }
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
        let old = self.locals.insert(name.to_string(), index);

        debug_assert!(
            old.is_none(),
            "tracking local var {name} but it already exists"
        );

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
    fn exit_scope(&mut self, scope_has_trailing_expr: bool) {
        // Emitting an instruction requires an existing scope, so if we need to
        // emit a return we will do so before popping the current scope.
        if self.scopes.len() == 1 {
            self.viz_exit_to_depth(0);
            self.emit(Instruction::Return);
        }

        let scope = self
            .scopes
            .pop()
            .expect("compiler bug: attempt to exit scope when scope stack is empty");

        self.locals_in_scope[scope.id] = self.locals.clone();

        // Depth 0 is function body block. That one ends with return. Depth >= 1
        // are nested blocks, those need to pop all their scoped locals and
        // possibly push a value on top of the stack.
        if scope.depth >= 1 && !scope.locals.is_empty() {
            // Keep value on top of stack if block has a return expression.
            // Otherwise just pop locals.
            if scope_has_trailing_expr {
                self.emit(Instruction::PopReplace(scope.locals.len()));
            } else {
                self.emit(Instruction::Pop(scope.locals.len()));
            }

            // Drop locals in this scope.
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

        let viz_depth = self.viz_open_stack.len();
        self.viz_enter_scope();

        let old_loop_status = self.current_loop.replace(LoopInfo {
            scope_depth: self.scopes.len(),
            break_patch_list: Vec::new(),
            continue_patch_list: Vec::new(),
            viz_depth,
        });

        codegen_body(self);

        let loop_info = std::mem::replace(&mut self.current_loop, old_loop_status)
            .expect("should have been pushed before when grabbing old_status");

        self.viz_exit_to_depth(viz_depth);
        self.exit_scope(false);

        for continue_jmp in loop_info.continue_patch_list {
            self.patch_jump(continue_jmp);
        }

        loop_info.break_patch_list
    }
}
