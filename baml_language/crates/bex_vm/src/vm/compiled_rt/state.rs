use std::collections::HashMap;

use bex_vm_types::{HeapPtr, RealizedTy, RootHaver, Value, types::TypeValue};

use super::{Fiber, fiber::YielderPtr};
use crate::{errors::VmError, vm::BexVm};

pub(crate) struct CompiledState {
    pub(crate) static_type_cache: HashMap<(usize, usize), HeapPtr>,
    pub(crate) fiber: Option<Fiber>,
    /// The fiber's [`Fiber::floor`], kept here so `stack_room` reads it while
    /// the fiber is out of `self` running a body.
    pub(crate) fiber_floor: usize,
    #[cfg_attr(
        any(target_arch = "wasm32", all(windows, target_arch = "aarch64")),
        allow(dead_code)
    )]
    pub(crate) yielder: Option<YielderPtr>,
    pub(crate) on_fiber: bool,
    pub(crate) builtin_type_args: Vec<RealizedTy>,
    pub(crate) call_type_args: Vec<RealizedTy>,
    pub(crate) saved_call_types: Vec<SavedCallTypes>,
    pub(crate) delivery: Option<Box<VmError>>,
    /// Frame records of compiled bodies on the fiber. Declared after `fiber`
    /// so a parked fiber unwinds into it before it is dropped.
    pub(crate) records: Box<super::RecordChain>,
    /// Whether compiled callees may run with frame records; see
    /// [`crate::BexVm::direct_calls`].
    pub(crate) direct_calls: bool,
}

impl Default for CompiledState {
    fn default() -> Self {
        Self {
            fiber: None,
            fiber_floor: 0,
            yielder: None,
            on_fiber: false,
            builtin_type_args: Vec::new(),
            call_type_args: Vec::new(),
            saved_call_types: Vec::new(),
            delivery: None,
            records: Box::default(),
            static_type_cache: HashMap::new(),
            direct_calls: !super::frames_forced(),
        }
    }
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
}

impl RootHaver for CompiledState {
    fn collect_roots(&self, roots: &mut Vec<HeapPtr>) {
        roots.extend(self.static_type_cache.values().copied());
        self.records.collect_roots(roots);
        for ty in self.types() {
            ty.visit_heads(&mut |head| {
                if head.is_resolved() {
                    roots.push(head.ptr());
                }
            });
        }
        if let Some(VmError::Thrown(thrown)) = self.delivery.as_deref() {
            roots.extend(thrown.value.as_object_ptr());
            roots.extend(thrown.context.and_then(|value| value.as_object_ptr()));
        }
    }

    fn forward_roots(&mut self, roots: &HashMap<HeapPtr, HeapPtr>) {
        for ptr in self.static_type_cache.values_mut() {
            *ptr = roots.get(ptr).copied().unwrap_or(*ptr);
        }
        self.records.forward_roots(roots);
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
        if let Some(VmError::Thrown(thrown)) = self.delivery.as_deref_mut() {
            for value in std::iter::once(&mut thrown.value).chain(thrown.context.iter_mut()) {
                if let Some(ptr) = value.as_object_ptr().and_then(|ptr| roots.get(&ptr)) {
                    *value = Value::object(*ptr);
                }
            }
        }
    }
}

impl BexVm {
    pub(in crate::vm) fn save_call_types(&mut self) {
        self.compiled.saved_call_types.push(SavedCallTypes {
            args: std::mem::take(&mut self.pending_call_type_args),
            values: std::mem::take(&mut self.pending_call_type_values),
        });
    }

    pub(in crate::vm) fn restore_call_types(&mut self) -> Vec<bex_vm_types::RealizedTy> {
        let previous = self
            .compiled
            .saved_call_types
            .pop()
            .expect("saved call types");
        self.pending_call_type_values = previous.values;
        std::mem::replace(&mut self.pending_call_type_args, previous.args)
    }
}
