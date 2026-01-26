//! Test utilities for BAML VM and compiler tests.
//!
//! This module provides test-friendly representations of runtime types that don't
//! rely on indices, making tests more readable and resilient to changes in the
//! order of globals, constants, and objects.

use baml_types::{BamlMap, BamlMedia};
use baml_viz_events::VizExecEvent;
use indexmap::IndexMap;

use crate::{
    bytecode::{BinOp, CmpOp, UnaryOp},
    vm::WatchNotification as VmWatchNotification,
    watch::{self},
    Object as VmObject, ObjectIndex, Value as VmValue, Vm, VmExecState,
};

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
    pub fn from_vm_value(value: &VmValue, vm: &Vm) -> anyhow::Result<Self> {
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
    /// Function name (for LoadGlobal instructions)
    Function(String),
    /// Class name (for AllocInstance instructions)
    Class(String),
    /// Enum name (for AllocVariant instructions)
    Enum(String),
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

            VmObject::Function(f) => Ok(Object::Function(f.name.clone())),

            VmObject::Class(c) => Ok(Object::Class(c.name.clone())),

            VmObject::Enum(e) => Ok(Object::Enum(e.name.clone())),

            _ => anyhow::bail!("Unsupported object type for testing: {:?}", obj),
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

/// Test-friendly representation of NodeId that uses variable names and test Objects.
#[derive(Debug, Clone, PartialEq)]
pub enum Notification {
    Channel(String),
    Object(Object),
    Viz {
        function_name: String,
        event: VizExecEvent,
    },
}

impl Notification {
    pub fn on_channel(name: &str) -> Self {
        Notification::Channel(name.to_string())
    }

    pub fn viz(function_name: String, event: VizExecEvent) -> Self {
        Notification::Viz {
            function_name,
            event,
        }
    }
}

impl Notification {
    /// Convert from VM NodeId to test Node by resolving indices to names/objects.
    pub fn from_node_id(node_id: &watch::NodeId, vm: &Vm) -> anyhow::Result<Self> {
        match node_id {
            watch::NodeId::LocalVar(stack_index) => vm
                .watch
                .root_state(*node_id)
                .map(|state| Notification::Channel(state.channel.clone()))
                .ok_or_else(|| {
                    anyhow::anyhow!("No root state found for local variable: {:?}", stack_index)
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
    /// Convert from VmExecState, converting Value to test Value for Complete case.
    pub fn from_vm_exec_state(state: VmExecState, vm: &Vm) -> anyhow::Result<Self> {
        match state {
            VmExecState::Await(index) => Ok(ExecState::Await(Object::from_vm_object(index, vm)?)),
            VmExecState::ScheduleFuture(index) => Ok(ExecState::ScheduleFuture(
                Object::from_vm_object(index, vm)?,
            )),
            VmExecState::Complete(value) => {
                Value::from_vm_value(&value, vm).map(ExecState::Complete)
            }
            VmExecState::Notify(notification) => match notification {
                VmWatchNotification::Variables(nodes) => {
                    let notifications = nodes
                        .iter()
                        .map(|node_id| Notification::from_node_id(node_id, vm))
                        .collect::<anyhow::Result<Vec<_>>>()?;
                    Ok(ExecState::Emit(notifications))
                }
                VmWatchNotification::Viz {
                    function_name,
                    event,
                } => Ok(ExecState::Emit(vec![Notification::viz(
                    function_name,
                    event,
                )])),
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
    PopReplace(usize),
    Jump(isize),
    JumpIfFalse(isize),
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
    DispatchFuture(usize),
    Await,
    Watch(usize),
    Notify(usize),
    VizEnter(usize),
    VizExit(usize),
    Call(usize),
    Return,
    Assert,
}
