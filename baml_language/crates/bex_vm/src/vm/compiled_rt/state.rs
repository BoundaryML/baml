use std::{collections::HashMap, sync::Arc};

use bex_vm_types::{HeapPtr, RealizedTy, RootHaver, Value, types::TypeValue};

use super::{CompiledCollector, PendingLog};
use crate::errors::VmError;

/// VM state that only compiled code uses.
#[derive(Default)]
pub struct CompiledState {
    /// Logs drained by the engine before it handles the result of `exec`.
    pub pending_logs: Vec<PendingLog>,
    /// The engine's collector, or `None` to disable collection.
    pub collector: Option<Arc<dyn CompiledCollector>>,
    /// Type arguments compiled code passes to a generic builtin.
    pub(crate) builtin_type_args: Vec<RealizedTy>,
    /// Type arguments of the call a resumable body requests.
    pub(crate) call_type_args: Vec<RealizedTy>,
    /// Pending type arguments preserved across nested compiled calls.
    pub(crate) saved_call_types: Vec<SavedCallTypes>,
    /// Spare frame type-argument vectors, reused across generic calls.
    pub(crate) type_arg_pool: Vec<Vec<RealizedTy>>,
    /// An error the unwinder delivered to a suspended compiled body.
    pub(crate) delivery: Option<Box<VmError>>,
    /// Lowest stack address compiled code may grow down to, or 0 if unknown.
    pub(crate) stack_floor: usize,
}

#[derive(Default)]
pub(crate) struct SavedCallTypes {
    pub(crate) args: Vec<RealizedTy>,
    pub(crate) values: Vec<Option<TypeValue>>,
}

impl CompiledState {
    fn types(&self) -> impl Iterator<Item = &RealizedTy> {
        self.builtin_type_args
            .iter()
            .chain(&self.call_type_args)
            .chain(self.saved_call_types.iter().flat_map(|saved| {
                saved
                    .args
                    .iter()
                    .chain(saved.values.iter().flatten().map(|value| &value.ty))
            }))
    }

    fn values_mut(&mut self) -> impl Iterator<Item = &mut Value> {
        let delivered = match self.delivery.as_deref_mut() {
            Some(VmError::Thrown(thrown)) => Some(thrown),
            _ => None,
        };
        self.pending_logs.iter_mut().map(|log| &mut log.data).chain(
            delivered.into_iter().flat_map(|thrown| {
                std::iter::once(&mut thrown.value).chain(thrown.context.iter_mut())
            }),
        )
    }
}

impl RootHaver for CompiledState {
    fn collect_roots(&self, roots: &mut Vec<HeapPtr>) {
        for ty in self.types() {
            ty.visit_heads(&mut |head| {
                if head.is_resolved() {
                    roots.push(head.ptr());
                }
            });
        }
        roots.extend(
            self.pending_logs
                .iter()
                .filter_map(|log| log.data.as_object_ptr()),
        );
        if let Some(VmError::Thrown(thrown)) = self.delivery.as_deref() {
            roots.extend(thrown.value.as_object_ptr());
            roots.extend(thrown.context.and_then(|value| value.as_object_ptr()));
        }
    }

    fn forward_roots(&mut self, roots: &HashMap<HeapPtr, HeapPtr>) {
        let forward_ty = |ty: &mut RealizedTy| {
            ty.visit_heads_mut(&mut |head| {
                if head.is_resolved()
                    && let Some(&moved) = roots.get(&head.ptr())
                {
                    head.forward_to(moved);
                }
            });
        };
        self.builtin_type_args.iter_mut().for_each(forward_ty);
        self.call_type_args.iter_mut().for_each(forward_ty);
        for saved in &mut self.saved_call_types {
            saved.args.iter_mut().for_each(forward_ty);
            saved
                .values
                .iter_mut()
                .flatten()
                .for_each(|value| forward_ty(&mut value.ty));
        }
        for value in self.values_mut() {
            if let Some(ptr) = value.as_object_ptr()
                && let Some(&moved) = roots.get(&ptr)
            {
                *value = Value::object(moved);
            }
        }
    }
}
