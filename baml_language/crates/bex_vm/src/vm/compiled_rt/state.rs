use bex_vm_types::{RealizedTy, RootHaver, RootVisitor, Weak, errors::VmThrown};
use rustc_hash::FxHashMap;

use super::{Fiber, fiber::YielderPtr};
use crate::{errors::VmError, vm::BexVm};

pub(crate) struct CompiledState {
    /// The compiled counterpart of the interpreter's `static_load_type_cache`:
    /// created-once type objects for `LoadType` constants of static functions,
    /// keyed by the function's (compile-time, never moving) payload. Weak, so
    /// a type nothing else holds is simply rebuilt.
    pub(crate) static_type_cache: FxHashMap<(usize, usize), Weak>,
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
    /// Evidence of the unwind delivered to (and continued by) a compiled body.
    pub(in crate::vm) error_evidence: Option<crate::vm::error_evidence::UnwindEvidence>,
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
            error_evidence: None,
            records: Box::default(),
            static_type_cache: FxHashMap::default(),
            direct_calls: !super::frames_forced(),
        }
    }
}

#[derive(Default)]
pub(crate) struct SavedCallTypes {
    pub(crate) args: Vec<RealizedTy>,
}

impl RootHaver for CompiledState {
    fn visit_roots(&mut self, visitor: &mut dyn RootVisitor) {
        let Self {
            static_type_cache,
            // Holds no heap pointer across a collection: a body spills its
            // values to the VM stack before it suspends.
            fiber: _,
            fiber_floor: _,
            yielder: _,
            on_fiber: _,
            builtin_type_args,
            call_type_args,
            saved_call_types,
            delivery,
            error_evidence: _,
            records,
            direct_calls: _,
        } = self;
        static_type_cache.retain(|_, ty| ty.visit(visitor));
        records.visit_roots(visitor);
        builtin_type_args.visit_roots(visitor);
        call_type_args.visit_roots(visitor);
        for SavedCallTypes { args } in saved_call_types {
            args.visit_roots(visitor);
        }
        match delivery.as_deref_mut() {
            Some(VmError::Thrown(VmThrown {
                value,
                throw_kind: _,
                context,
            })) => {
                value.visit_roots(visitor);
                context.visit_roots(visitor);
            }
            Some(VmError::ThrownUnhandled { value, trace: _ }) => value.visit_roots(visitor),
            Some(VmError::InternalError(_) | VmError::TracedInternalError { .. }) | None => {}
        }
    }
}

impl BexVm {
    pub(in crate::vm) fn save_call_types(&mut self) {
        self.compiled.saved_call_types.push(SavedCallTypes {
            args: std::mem::take(&mut self.pending_call_type_args),
        });
    }

    pub(in crate::vm) fn restore_call_types(&mut self) -> Vec<bex_vm_types::RealizedTy> {
        let previous = self
            .compiled
            .saved_call_types
            .pop()
            .expect("saved call types");
        std::mem::replace(&mut self.pending_call_type_args, previous.args)
    }
}
