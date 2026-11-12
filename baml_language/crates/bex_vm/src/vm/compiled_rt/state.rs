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
    /// The fiber's whole stack ([`Fiber::stack_range`]), kept here as
    /// `fiber_floor` is: what [`crate::BexVm::on_fiber_stack`] checks.
    pub(crate) fiber_stack: std::ops::Range<usize>,
    #[cfg_attr(
        any(target_arch = "wasm32", all(windows, target_arch = "aarch64")),
        allow(dead_code)
    )]
    pub(crate) yielder: Option<YielderPtr>,
    pub(crate) on_fiber: bool,
    /// The stack pointer of the innermost `suspend` while a body is parked;
    /// the conservative range starts there. Zero while the fiber is idle:
    /// `fiber_resume` clears it when the last body completes.
    pub(crate) parked_sp: usize,
    pub(crate) builtin_type_args: Vec<RealizedTy>,
    /// Compiled JSON decode: realized field types of compile-time classes,
    /// by (class object index, field), for fields holding lists and maps.
    pub(crate) json_field_types: FxHashMap<(usize, usize), RealizedTy>,
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
    /// Per class object index, how compiled code allocates its instances,
    /// once looked up; see [`crate::BexVm::instance_shape_at`].
    pub(crate) shapes: Vec<Option<Option<super::InstanceShape>>>,
    /// How compiled calls run this slice; see `BexVm::settle_call_mode`.
    pub(crate) call_mode: CallMode,
}

/// How a compiled call to a plain body runs.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum CallMode {
    Plain,
    Timed,
    Recorded,
}

impl Default for CompiledState {
    fn default() -> Self {
        Self {
            fiber: None,
            fiber_floor: 0,
            fiber_stack: 0..0,
            yielder: None,
            on_fiber: false,
            parked_sp: 0,
            builtin_type_args: Vec::new(),
            json_field_types: FxHashMap::default(),
            call_type_args: Vec::new(),
            saved_call_types: Vec::new(),
            delivery: None,
            error_evidence: None,
            records: Box::default(),
            static_type_cache: FxHashMap::default(),
            direct_calls: !super::frames_forced(),
            shapes: Vec::new(),
            call_mode: CallMode::Plain,
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
            fiber,
            fiber_floor: _,
            fiber_stack: _,
            yielder: _,
            on_fiber: _,
            parked_sp,
            builtin_type_args,
            // Field types of compile-time classes, which never move or die.
            json_field_types: _,
            call_type_args,
            saved_call_types,
            delivery,
            error_evidence: _,
            records,
            direct_calls: _,
            // Shapes name compile-time classes, which never move or die.
            shapes: _,
            call_mode: _,
        } = self;
        // Parked bodies keep their values in Rust locals and registers, all
        // on the fiber's stack while it is parked.
        if let Some(range) = fiber.as_ref().and_then(|f| f.parked_range(*parked_sp)) {
            visitor.conservative(range);
        }
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
    /// Whether `address` lies in the fiber stack a parked collection scans,
    /// and a body is running on it now: what a stack-placed object's frame
    /// must satisfy ([`crate::compiled_rt::StackCell`]).
    #[must_use]
    pub fn on_fiber_stack(&self, address: usize) -> bool {
        self.compiled.on_fiber && self.compiled.fiber_stack.contains(&address)
    }

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
