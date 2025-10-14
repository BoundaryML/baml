//! Common test utilities for VM tests.

use baml_compiler::test::ast;
use baml_types::{BamlMap, BamlMedia};
use baml_vm::{
    emit::{self, Emit},
    errors::VmError,
    BamlVmProgram, Bytecode, EvalStack, Frame, Function, FunctionKind, GlobalPool, Instruction,
    Object as VmObject, ObjectIndex, ObjectPool, StackIndex, Value as VmValue, Vm, VmExecState,
};
use indexmap::IndexMap;

/// Test-friendly representation of VM values that doesn't rely on object
/// indices.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Int(i64),
    Float(f64),
    Bool(bool),
    Object(Object),
}

impl Value {
    /// Convert a VM Value to a TestValue by following object references.
    pub fn from_vm_value(value: &VmValue, vm: &Vm) -> anyhow::Result<Self> {
        match value {
            VmValue::Null => Ok(Value::Null),
            VmValue::Int(i) => Ok(Value::Int(*i)),
            VmValue::Float(f) => Ok(Value::Float(*f)),
            VmValue::Bool(b) => Ok(Value::Bool(*b)),
            VmValue::Object(index) => Object::from_vm_object(*index, vm).map(Value::Object),
        }
    }
}

/// Test-friendly representation of VM objects.
#[derive(Debug, Clone, PartialEq)]
pub enum Object {
    String(String),
    Array(Vec<Value>),
    Map(BamlMap<String, Value>),
    Instance(Instance),
    Variant(Variant),
    Media(BamlMedia),
    // We can extend this with more object types as needed
}

impl Object {
    pub fn from_vm_object(index: ObjectIndex, vm: &Vm) -> anyhow::Result<Self> {
        let obj = &vm.objects[index];
        match obj {
            VmObject::String(s) => Ok(Object::String(s.clone())),

            VmObject::Array(arr) => arr
                .iter()
                .map(|v| Value::from_vm_value(v, vm))
                .collect::<anyhow::Result<Vec<_>>>()
                .map(Object::Array),

            VmObject::Map(map) => map
                .iter()
                .map(|(key, value)| {
                    Value::from_vm_value(value, vm).map(|value| (key.clone(), value))
                })
                .collect::<anyhow::Result<BamlMap<String, Value>>>()
                .map(Object::Map),

            VmObject::Instance(instance) => {
                let VmObject::Class(vm_class) = &vm.objects[instance.class] else {
                    anyhow::bail!("Class not found for instance: {:?}", instance);
                };

                let mut fields = BamlMap::new();

                for (i, value) in instance.fields.iter().enumerate() {
                    let value = Value::from_vm_value(value, vm)?;
                    fields.insert(vm_class.field_names[i].clone(), value);
                }

                Ok(Object::Instance(Instance {
                    class: vm_class.name.clone(),
                    fields,
                }))
            }

            VmObject::Variant(variant) => {
                let VmObject::Enum(vm_enum) = &vm.objects[variant.enm] else {
                    anyhow::bail!("Enum not found for variant: {:?}", variant);
                };

                Ok(Object::Variant(Variant {
                    enm: vm_enum.name.clone(),
                    variant: vm_enum.variant_names[variant.index].clone(),
                }))
            }

            VmObject::Media(media) => Ok(Object::Media(media.clone())),

            _ => anyhow::bail!("Unsupported object type for testing: {:?}", obj),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Instance {
    pub class: String,
    pub fields: IndexMap<String, Value>,
}

impl Instance {
    pub fn fields(from: IndexMap<&str, Value>) -> IndexMap<String, Value> {
        from.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Variant {
    pub enm: String,
    pub variant: String,
}

/// Test-friendly representation of NodeId that uses variable names and test Objects.
#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Variable(String),
    Object(Object),
}

impl Node {
    pub fn variable(name: &str) -> Self {
        Node::Variable(name.to_string())
    }
}

impl Node {
    /// Convert from VM NodeId to test Node by resolving indices to names/objects.
    pub fn from_node_id(node_id: &emit::NodeId, vm: &Vm) -> anyhow::Result<Self> {
        match node_id {
            emit::NodeId::LocalVar(stack_index) => {
                // Get the current frame to access the function
                let frame = vm.frames.last().ok_or_else(|| {
                    anyhow::anyhow!("No frame available when converting LocalVar NodeId")
                })?;

                let function = vm.objects[frame.function].as_function()?;

                // Get the scope for the current instruction pointer
                let scope_idx = function
                    .bytecode
                    .scopes
                    .get(frame.instruction_ptr as usize)
                    .copied()
                    .unwrap_or(0);

                let relative_index = stack_index.raw() - frame.locals_offset.raw();

                // Get the local variable name from the scope
                let var_name = function
                    .locals_in_scope
                    .get(scope_idx)
                    .and_then(|locals| locals.get(relative_index))
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "Local variable at relative index {} (abs {}) not found in scope {}",
                            relative_index,
                            stack_index.raw(),
                            scope_idx
                        )
                    })?
                    .clone();

                Ok(Node::Variable(var_name))
            }
            emit::NodeId::HeapObject(obj_index) => {
                Ok(Node::Object(Object::from_vm_object(*obj_index, vm)?))
            }
        }
    }
}

/// Enhanced test execution state that supports TestValue comparisons.
#[derive(Debug, Clone, PartialEq)]
pub enum ExecState {
    /// VM cannot proceed. It is awaiting a pending future to complete.
    Await(Object),
    /// VM notifies caller about a future that needs to be scheduled.
    ScheduleFuture(Object),
    /// VM has completed the execution with a test-friendly value.
    Complete(Value),

    Emit(Vec<Node>),
}

impl ExecState {
    /// Convert from VmExecState, converting Value to TestValue for Complete case.
    pub fn from_vm_exec_state(state: VmExecState, vm: &Vm) -> anyhow::Result<Self> {
        match state {
            VmExecState::Await(index) => Ok(ExecState::Await(Object::from_vm_object(index, vm)?)),
            VmExecState::ScheduleFuture(index) => Ok(ExecState::ScheduleFuture(
                Object::from_vm_object(index, vm)?,
            )),
            VmExecState::Complete(value) => {
                Value::from_vm_value(&value, vm).map(ExecState::Complete)
            }
            VmExecState::Emit(roots) => {
                let nodes = roots
                    .iter()
                    .map(|node_id| Node::from_node_id(node_id, vm))
                    .collect::<anyhow::Result<Vec<_>>>()?;
                Ok(ExecState::Emit(nodes))
            }
        }
    }
}

/// Compare a VM execution result with expected test value.
pub fn assert_vm_value_equals(vm: &Vm, actual: &VmValue, expected: &Value) -> anyhow::Result<()> {
    let actual_test_value = Value::from_vm_value(actual, vm)?;
    if actual_test_value != *expected {
        anyhow::bail!(
            "VM value mismatch!\nExpected: {:?}\nActual: {:?}",
            expected,
            actual_test_value
        );
    }
    Ok(())
}

/// Helper struct for testing VM execution.
pub struct ProgramInput<Expect> {
    pub source: &'static str,
    pub function: &'static str,
    pub expected: Expect,
}

pub type Program = ProgramInput<ExecState>;
pub type FailingProgram = ProgramInput<VmError>;

pub fn assert_vm_fails(input: FailingProgram) -> anyhow::Result<()> {
    assert_vm_fails_with_inspection(input, |_vm| Ok(()))
}

pub fn assert_vm_fails_with_inspection(
    input: FailingProgram,
    inspect: impl FnOnce(&Vm) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let (vm, result) = setup_and_exec_program(input.source, input.function)?;

    assert_eq!(
        result,
        Err(input.expected),
        "VM execution result mismatch for function '{}'",
        input.function
    );

    inspect(&vm)?;

    Ok(())
}

pub fn assert_vm_executes(input: Program) -> anyhow::Result<()> {
    assert_vm_executes_with_inspection(input, |_vm| Ok(()))
}

#[track_caller]
pub fn assert_vm_executes_with_inspection(
    input: Program,
    inspect: impl FnOnce(&Vm) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let (vm, result) = setup_and_exec_program(input.source, input.function)?;
    let result = result?;

    let test_result = ExecState::from_vm_exec_state(result, &vm)?;

    assert_eq!(
        test_result, input.expected,
        "VM execution result mismatch for function '{}'",
        input.function
    );

    // Run custom inspection
    inspect(&vm)?;

    Ok(())
}

/// Collects all VM execution states by repeatedly calling exec() until completion.
///
/// This is useful for testing scenarios where you need to observe all intermediate
/// states during VM execution, such as Emit states, ScheduleFuture states, etc.
///
/// Returns a tuple of (Vm, Vec<ExecState>) where the vector contains all states
/// encountered during execution, including the final Complete state.
pub fn collect_vm_exec_states(
    source: &'static str,
    function: &str,
) -> anyhow::Result<(Vm, Vec<ExecState>)> {
    let ast = ast(source)?;
    let BamlVmProgram {
        objects,
        globals,
        resolved_function_names,
        resolved_enums_names: _,
        resolved_class_names: _,
    } = baml_compiler::compile(&ast)?;
    let (target_function_index, _) = resolved_function_names[function];
    let mut vm = Vm {
        frames: vec![Frame {
            function: target_function_index,
            instruction_ptr: 0,
            locals_offset: StackIndex::from_raw(0),
        }],
        stack: EvalStack::from_vec(vec![VmValue::Object(target_function_index)]),
        runtime_allocs_offset: ObjectIndex::from_raw(objects.len()),
        objects,
        globals,
        env_vars: Default::default(),
        emit: Emit::new(),
        emittable_vars: Default::default(),
    };

    let mut states = Vec::new();

    loop {
        let result = vm.exec()?;

        // Check if this is a Complete state before converting
        let is_complete = matches!(result, VmExecState::Complete(_));

        let test_state = ExecState::from_vm_exec_state(result, &vm)?;
        states.push(test_state);

        if is_complete {
            break;
        }
    }

    Ok((vm, states))
}

/// Helper struct for testing VM execution with expected Emit states.
pub type EmitProgram = ProgramInput<Vec<Vec<Node>>>;

/// Assert that a VM program emits the expected sequence of emit states.
///
/// This function drives the VM by repeatedly calling exec() and collects all states.
/// It then filters for Emit states and compares them against the expected sequence.
#[track_caller]
pub fn assert_vm_emits(input: EmitProgram) -> anyhow::Result<()> {
    assert_vm_emits_with_inspection(input, |_vm, _states| Ok(()))
}

/// Assert that a VM program emits the expected sequence of emit states, with custom inspection.
#[track_caller]
pub fn assert_vm_emits_with_inspection(
    input: EmitProgram,
    inspect: impl FnOnce(&Vm, &[ExecState]) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let (vm, states) = collect_vm_exec_states(input.source, input.function)?;

    // Extract only the Emit states
    let emit_states: Vec<Vec<Node>> = states
        .iter()
        .filter_map(|state| match state {
            ExecState::Emit(roots) => Some(roots.clone()),
            _ => None,
        })
        .collect();

    assert_eq!(
        emit_states, input.expected,
        "VM emit states mismatch for function '{}'",
        input.function
    );

    // Run custom inspection
    inspect(&vm, &states)?;

    Ok(())
}

fn setup_and_exec_program(
    source: &'static str,
    function: &str,
) -> Result<(Vm, Result<VmExecState, VmError>), anyhow::Error> {
    let ast = ast(source)?;
    let BamlVmProgram {
        objects,
        globals,
        resolved_function_names,
        resolved_enums_names,
        resolved_class_names,
    } = baml_compiler::compile(&ast)?;
    let (target_function_index, _) = resolved_function_names[function];
    let mut vm = Vm {
        frames: vec![Frame {
            function: target_function_index,
            instruction_ptr: 0,
            locals_offset: StackIndex::from_raw(0),
        }],
        stack: EvalStack::from_vec(vec![VmValue::Object(target_function_index)]),
        runtime_allocs_offset: ObjectIndex::from_raw(objects.len()),
        objects,
        globals,
        env_vars: Default::default(),
        emit: Emit::new(),
        emittable_vars: Default::default(),
    };
    let result = vm.exec();
    Ok((vm, result))
}

/// Helper struct for testing VM execution with direct bytecode.
pub struct BytecodeProgram {
    pub arity: usize,
    pub instructions: Vec<Instruction>,
    pub constants: Vec<VmValue>,
    pub expected: VmExecState,
}

/// Helper function for VM execution with direct bytecode.
pub fn assert_vm_executes_bytecode(input: BytecodeProgram) -> anyhow::Result<()> {
    assert_vm_executes_bytecode_with_inspection(input, |_vm, _result| Ok(()))
}

/// Helper function for VM execution with direct bytecode and custom inspection.
pub fn assert_vm_executes_bytecode_with_inspection(
    input: BytecodeProgram,
    inspect: impl FnOnce(&Vm, VmExecState) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    // Create function from bytecode
    let function = Function {
        name: "test_fn".to_string(),
        arity: input.arity,
        bytecode: Bytecode {
            source_lines: vec![1; input.instructions.len()],
            scopes: vec![0; input.instructions.len()],
            instructions: input.instructions,
            constants: input.constants,
        },
        kind: FunctionKind::Exec,
        locals_in_scope: {
            let mut names = Vec::with_capacity(input.arity + 1);
            names.push("<fn test_fn>".to_string());
            names.resize_with(names.capacity(), String::new);
            vec![names]
        },
        span: internal_baml_diagnostics::Span::fake(),
    };

    let objects = vec![VmObject::Function(function)];
    let globals = vec![VmValue::Object(ObjectIndex::from_raw(0))];

    // Create and run the VM
    let mut vm = Vm {
        frames: vec![Frame {
            function: ObjectIndex::from_raw(0),
            instruction_ptr: 0,
            locals_offset: StackIndex::from_raw(0),
        }],
        stack: EvalStack::from_vec(vec![VmValue::Object(ObjectIndex::from_raw(0))]),
        runtime_allocs_offset: ObjectIndex::from_raw(objects.len()),
        objects: ObjectPool::from_vec(objects),
        globals: GlobalPool::from_vec(globals),
        env_vars: Default::default(),
        emit: Emit::new(),
        emittable_vars: Default::default(),
    };

    let result = vm.exec()?;

    assert_eq!(
        result, input.expected,
        "VM execution result mismatch for bytecode test",
    );

    // Run custom inspection
    inspect(&vm, result)?;

    Ok(())
}
