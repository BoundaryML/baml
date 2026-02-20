//! Test utilities for BAML VM and compiler tests.
//!
//! This module provides test-friendly representations of runtime types that don't
//! rely on indices, making tests more readable and resilient to changes in the
//! order of globals, constants, and objects.

use bex_vm::{
    BexVm, VmExecState,
    vm::WatchNotification as VmWatchNotification,
    watch::{self},
};
use bex_vm_types::{
    HeapPtr, Object as VmObject, Value as VmValue,
    bytecode::{
        BinOp, BlockNotification as VmBlockNotification, BlockNotificationType, CmpOp, UnaryOp,
    },
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
    /// Convert a VM Value to a test Value by following object references.
    pub fn from_vm_value(value: &VmValue, vm: &BexVm) -> anyhow::Result<Self> {
        match value {
            VmValue::Null => Ok(Value::Null),
            VmValue::Int(i) => Ok(Value::Int(*i)),
            VmValue::Float(f) => Ok(Value::Float(*f)),
            VmValue::Bool(b) => Ok(Value::Bool(*b)),
            VmValue::Object(index) => Object::from_vm_object(*index, vm).map(Value::Object),
        }
    }

    /// Shorthand for creating a function value.
    pub fn function(name: &str) -> Self {
        Value::Object(Object::Function(name.to_string()))
    }

    /// Shorthand for creating a class value.
    pub fn class(name: &str) -> Self {
        Value::Object(Object::Class(name.to_string()))
    }

    /// Shorthand for creating an enum value.
    pub fn enm(name: &str) -> Self {
        Value::Object(Object::Enum(name.to_string()))
    }

    /// Shorthand for creating a string value.
    pub fn string(s: &str) -> Self {
        Value::Object(Object::String(s.to_string()))
    }

    /// Shorthand for creating an array value.
    pub fn array(values: Vec<Value>) -> Self {
        Value::Object(Object::Array(values))
    }

    /// Shorthand for creating a map value.
    pub fn map(values: IndexMap<String, Value>) -> Self {
        Value::Object(Object::Map(values))
    }
}

/// Test-friendly representation of VM objects.
#[derive(Debug, Clone, PartialEq)]
pub enum Object {
    String(String),
    Array(Vec<Value>),
    Map(IndexMap<String, Value>),
    Instance(Instance),
    Variant(Variant),
    Media(baml_base::MediaKind),
    /// Function name (for `LoadGlobal` instructions)
    Function(String),
    /// Class name (for `AllocInstance` instructions)
    Class(String),
    /// Enum name (for `AllocVariant` instructions)
    Enum(String),
}

impl Object {
    pub fn from_vm_object(ptr: HeapPtr, vm: &BexVm) -> anyhow::Result<Self> {
        let obj = vm.get_object(ptr);
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
                .collect::<anyhow::Result<IndexMap<String, Value>>>()
                .map(Object::Map),

            VmObject::Instance(instance) => {
                let VmObject::Class(vm_class) = vm.get_object(instance.class) else {
                    anyhow::bail!("Class not found for instance: {instance:?}");
                };

                let mut fields = IndexMap::new();

                for (i, value) in instance.fields.iter().enumerate() {
                    let value = Value::from_vm_value(value, vm)?;
                    fields.insert(vm_class.fields[i].name.clone(), value);
                }

                Ok(Object::Instance(Instance {
                    class: vm_class.name.clone(),
                    fields,
                }))
            }

            VmObject::Variant(variant) => {
                let VmObject::Enum(vm_enum) = vm.get_object(variant.enm) else {
                    anyhow::bail!("Enum not found for variant: {variant:?}");
                };

                Ok(Object::Variant(Variant {
                    enm: vm_enum.name.clone(),
                    variant: vm_enum.variants[variant.index].name.clone(),
                }))
            }

            VmObject::Media(media) => Ok(Object::Media(media.kind)),
            VmObject::Function(f) => Ok(Object::Function(f.name.clone())),

            VmObject::Class(c) => Ok(Object::Class(c.name.clone())),

            VmObject::Enum(e) => Ok(Object::Enum(e.name.clone())),

            VmObject::Future(_) => anyhow::bail!("Unsupported object type for testing: {obj:?}"),
            VmObject::Resource(_) => anyhow::bail!("Unsupported object type for testing: {obj:?}"),
            VmObject::PromptAst(_) => {
                anyhow::bail!("Unsupported object type for testing: {obj:?}")
            }
            VmObject::Collector(_) => {
                anyhow::bail!("Unsupported object type for testing: {obj:?}")
            }
            VmObject::Type(_) => anyhow::bail!("Unsupported object type for testing: {obj:?}"),
            #[cfg(feature = "heap_debug")]
            VmObject::Sentinel(_) => anyhow::bail!("Unsupported object type for testing: {obj:?}"),
        }
    }

    pub fn instance(class: &str, fields: IndexMap<&str, Value>) -> Self {
        Object::Instance(Instance {
            class: class.to_string(),
            fields: Instance::fields(fields),
        })
    }

    /// Shorthand for creating a function object reference.
    pub fn function(name: &str) -> Self {
        Object::Function(name.to_string())
    }

    /// Shorthand for creating a class object reference.
    pub fn class(name: &str) -> Self {
        Object::Class(name.to_string())
    }

    /// Shorthand for creating an enum object reference.
    pub fn enm(name: &str) -> Self {
        Object::Enum(name.to_string())
    }

    /// Shorthand for creating a string object.
    pub fn string(s: &str) -> Self {
        Object::String(s.to_string())
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

#[derive(Debug, Clone, PartialEq)]
pub struct BlockEvent {
    pub function_name: String,
    pub block_name: String,
    pub level: usize,
    pub block_type: BlockNotificationType,
    pub is_enter: bool,
}

impl BlockEvent {
    fn from_vm(notification: &VmBlockNotification) -> Self {
        Self {
            function_name: notification.function_name.as_str().to_owned(),
            block_name: notification.block_name.as_str().to_owned(),
            level: notification.level,
            block_type: notification.block_type,
            is_enter: notification.is_enter,
        }
    }
}

/// Test-friendly visualization event.
#[derive(Debug, Clone, PartialEq)]
pub struct VizEvent {
    pub function_name: String,
    pub label: String,
    pub is_enter: bool,
}

/// Test-friendly representation of `NodeId` that uses variable names and test Objects.
#[derive(Debug, Clone, PartialEq)]
pub enum Notification {
    Channel(String),
    Object(Object),
    Block(BlockEvent),
    Viz(VizEvent),
}

impl Notification {
    pub fn on_channel(name: &str) -> Self {
        Notification::Channel(name.to_string())
    }

    pub fn block(notification: &VmBlockNotification) -> Self {
        Notification::Block(BlockEvent::from_vm(notification))
    }
}

impl Notification {
    /// Convert from VM `NodeId` to test Node by resolving indices to names/objects.
    pub fn from_node_id(node_id: &watch::NodeId, vm: &BexVm) -> anyhow::Result<Self> {
        match node_id {
            watch::NodeId::LocalVar(stack_index) => vm
                .watch
                .root_state(*node_id)
                .map(|state| Notification::Channel(state.channel.clone()))
                .ok_or_else(|| {
                    anyhow::anyhow!("No root state found for local variable: {stack_index:?}")
                }),
            watch::NodeId::HeapObject(_obj_index) => {
                Ok(Notification::Object(Object::String("bogger".to_string())))
            }
        }
    }
}

/// Enhanced test execution state that supports test Value comparisons.
#[derive(Debug, Clone, PartialEq)]
pub enum ExecState {
    /// VM cannot proceed. It is awaiting a pending future to complete.
    Await(Object),
    /// VM notifies caller about a future that needs to be scheduled.
    ScheduleFuture(Object),
    /// VM has completed the execution with a test-friendly value.
    Complete(Value),

    Emit(Vec<Notification>),
}

impl ExecState {
    /// Convert from `VmExecState`, converting Value to test Value for Complete case.
    pub fn from_vm_exec_state(state: VmExecState, vm: &BexVm) -> anyhow::Result<Self> {
        match state {
            VmExecState::Await(index) => Ok(ExecState::Await(Object::from_vm_object(index, vm)?)),
            VmExecState::ScheduleFuture(index) => Ok(ExecState::ScheduleFuture(
                Object::from_vm_object(index, vm)?,
            )),
            VmExecState::Complete(value) => {
                Value::from_vm_value(&value, vm).map(ExecState::Complete)
            }
            VmExecState::SpanNotify(_) => {
                // Span notifications: treated as empty emit in test context.
                // The test runner will call exec() again to continue.
                Ok(ExecState::Emit(vec![]))
            }
            VmExecState::Notify(notification) => match notification {
                VmWatchNotification::Variables(nodes) => {
                    let notifications = nodes
                        .iter()
                        .map(|node_id| Notification::from_node_id(node_id, vm))
                        .collect::<anyhow::Result<Vec<_>>>()?;
                    Ok(ExecState::Emit(notifications))
                }
                VmWatchNotification::Block(ref notification) => {
                    Ok(ExecState::Emit(vec![Notification::block(notification)]))
                }
                VmWatchNotification::Viz {
                    function_name,
                    event,
                } => {
                    let is_enter = event.delta == bex_vm_types::bytecode::VizExecDelta::Enter;
                    Ok(ExecState::Emit(vec![Notification::Viz(VizEvent {
                        function_name,
                        label: event.label,
                        is_enter,
                    })]))
                }
            },
        }
    }
}

/// Test-friendly bytecode instruction that embeds values instead of indices.
#[derive(Clone, Debug, PartialEq)]
pub enum Instruction {
    LoadConst(Value),
    LoadVar(String),
    StoreVar(String),
    LoadGlobal(Value),
    StoreGlobal(Value),
    LoadField(usize),
    StoreField(usize),
    Pop(usize),
    Copy(usize),
    Jump(isize),
    PopJumpIfFalse(isize),
    BinOp(BinOp),
    CmpOp(CmpOp),
    UnaryOp(UnaryOp),
    AllocArray(usize),
    AllocMap(usize),
    LoadArrayElement,
    LoadMapElement,
    StoreArrayElement,
    StoreMapElement,
    AllocInstance(Value),
    AllocVariant(Value),
    /// Direct dispatch to a statically-known sys_op by name.
    DispatchFuture(String),
    Await,
    Watch(usize),
    Unwatch(usize),
    Notify(usize),
    /// Direct call to a statically-known function by name.
    Call(String),
    CallIndirect,

    Return,
    Assert,
    NotifyBlock(usize),
    VizEnter(usize),
    VizExit(usize),
    InitLocals(usize),
    JumpTable {
        table_idx: usize,
        default: isize,
    },
    Discriminant,
    TypeTag,
    Unreachable,
}
