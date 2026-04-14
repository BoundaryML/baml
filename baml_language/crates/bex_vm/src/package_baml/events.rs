//! Native implementation for `baml.events.send`.
//!
//! In practice, the compiler special-cases `baml.events.send` to emit the
//! `Instruction::SendEvent` bytecode directly, so this native function is
//! never invoked at runtime.  It exists only so that `attach_builtins` can
//! successfully resolve `baml.events.send` to a function pointer without
//! returning `MissingNativeFunction`.

use bex_vm_types::types::Value;
use indexmap::IndexMap;

use super::{BamlNamespaceEvents, PackageBamlImpl};
use crate::BexVm;

impl BamlNamespaceEvents for PackageBamlImpl {
    fn send(
        _vm: &mut BexVm,
        _event_name: &str,
        _data: &IndexMap<String, Value>,
    ) {
        // Never called at runtime: the compiler emits Instruction::SendEvent
        // for all `baml.events.send(...)` calls, which yields control to the
        // engine via VmExecState::Event without invoking this function.
    }
}
