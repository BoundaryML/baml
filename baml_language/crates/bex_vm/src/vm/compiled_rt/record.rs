//! Frame records: the frames of compiled bodies.
//!
//! Every compiled body runs with a [`FrameRecord`], a local in the Rust frame
//! that called it, so it lives on the VM's fiber, which never moves while a
//! body is running or parked on it. Records link to the previous one and
//! [`RecordChain`] holds the newest.
//!
//! A body the VM enters (from bytecode, the call path, or the fiber's entry)
//! has a VM frame too; its record is that frame's *entry record*, which only
//! carries the body's live position. A body compiled code calls directly has
//! no VM frame: its record is its frame. [`BexVm::stack_view`] merges the two
//! into one stack, and a frame's *level* is its depth in that stack, stable
//! for the frame's lifetime because nothing below a frame changes while it
//! exists.
//!
//! A record whose body returns an error stays on the stack as a boxed copy, a
//! *zombie*, exactly as a VM frame stays for the unwinder: the unwinder or a
//! compiled catch completes and removes it.

use std::{
    cell::{Cell, UnsafeCell},
    collections::HashMap,
    ptr,
};

use bex_vm_types::{HeapPtr, Object, RealizedTy, StackIndex, Value, types::Function};

use super::CompiledFn;
use crate::{
    errors::{VmError, VmPanic},
    telemetry::{CallTypeArgs, FrameTelemetry, InvocationOutcome},
    types::ObjectTrait as _,
    vm::{BexVm, BytecodeFrame, Frame},
};

/// The frame of one compiled body.
pub struct FrameRecord {
    prev: Cell<*const FrameRecord>,
    /// The callable the body runs: a function, or a closure.
    function: Cell<HeapPtr>,
    /// The runtime package the body's object indices resolve in; null for
    /// the compile-time heap.
    package: Cell<HeapPtr>,
    /// The instruction index the body last reported: its call site while a
    /// callee runs, or where it raised or suspended.
    pc: Cell<u32>,
    /// `vm.frames.len()` when the record was pushed: it sits above
    /// `vm.frames[anchor - 1]`, which an entry record belongs to.
    anchor: u32,
    /// Where the body's slots start on the VM stack.
    base: u32,
    /// The record's level; an entry record has its VM frame's.
    level: u32,
    entry: bool,
    receipt: Cell<Option<FrameTelemetry>>,
    /// Type arguments in the callee's De Bruijn order; `None` for none. An
    /// entry record's are its VM frame's.
    type_args: UnsafeCell<Option<Box<[RealizedTy]>>>,
}

impl FrameRecord {
    /// Record the instruction index of the body's current position.
    #[allow(clippy::inline_always, reason = "one store at every call site")]
    #[inline(always)]
    pub fn note_pc(&self, pc: u32) {
        self.pc.set(pc);
    }

    #[inline]
    #[must_use]
    pub fn pc(&self) -> u32 {
        self.pc.get()
    }

    #[inline]
    #[must_use]
    pub fn function(&self) -> HeapPtr {
        self.function.get()
    }

    #[inline]
    pub(crate) fn anchor(&self) -> usize {
        self.anchor as usize
    }

    #[inline]
    pub(crate) fn base(&self) -> usize {
        self.base as usize
    }

    #[inline]
    pub(crate) fn level(&self) -> usize {
        self.level as usize
    }

    #[inline]
    pub(crate) fn package(&self) -> HeapPtr {
        self.package.get()
    }

    #[inline]
    pub(crate) fn receipt(&self) -> Option<FrameTelemetry> {
        self.receipt.get()
    }

    /// Whether the record holds a receipt, reading only its tag.
    #[inline]
    fn has_receipt(&self) -> bool {
        // SAFETY: nothing else borrows the cell's contents.
        #[allow(unsafe_code)]
        unsafe {
            (*self.receipt.as_ptr()).is_some()
        }
    }

    #[inline]
    pub(crate) fn set_receipt(&self, receipt: Option<FrameTelemetry>) {
        self.receipt.set(receipt);
    }

    /// The record's own type arguments; empty for an entry record.
    #[allow(unsafe_code)]
    pub(crate) fn own_type_args(&self) -> &[RealizedTy] {
        // SAFETY: the vector is replaced or mutated only by collection
        // forwarding, which runs while no body holds a borrow of it.
        unsafe { (*self.type_args.get()).as_deref().unwrap_or(&[]) }
    }

    #[inline]
    fn prev(&self) -> *const FrameRecord {
        self.prev.get()
    }

    fn collect_roots(&self, roots: &mut Vec<HeapPtr>) {
        roots.push(self.function.get());
        if !self.package.get().as_ptr().is_null() {
            roots.push(self.package.get());
        }
        for ty in self.own_type_args() {
            ty.visit_heads(&mut |head| {
                if head.is_resolved() {
                    roots.push(head.ptr());
                }
            });
        }
    }

    #[allow(unsafe_code)]
    fn forward_roots(&self, roots: &HashMap<HeapPtr, HeapPtr>) {
        let forward = |cell: &Cell<HeapPtr>| {
            if let Some(&moved) = roots.get(&cell.get()) {
                cell.set(moved);
            }
        };
        forward(&self.function);
        forward(&self.package);
        // SAFETY: collection runs while every body is parked, so nothing
        // borrows the type arguments.
        let type_args = unsafe { &mut *self.type_args.get() };
        for ty in type_args.iter_mut().flat_map(|types| types.iter_mut()) {
            ty.visit_heads_mut(&mut |head| {
                if head.is_resolved()
                    && let Some(&moved) = roots.get(&head.ptr())
                {
                    head.forward_to(moved);
                }
            });
        }
    }
}

/// The newest record, how many records stand outside `vm.frames`, and the
/// zombies.
///
/// Boxed so its address does not move with the VM: a guard on a fiber that is
/// dropped while parked pops through it during the forced unwind, which may
/// happen after the VM has moved.
pub(crate) struct RecordChain {
    top: Cell<*const FrameRecord>,
    /// Records that are frames of their own: every record but entry records,
    /// zombies included.
    count: Cell<u32>,
    /// Records of bodies that returned an error, kept until unwound.
    #[allow(clippy::vec_box, reason = "the chain links zombies by address")]
    zombies: UnsafeCell<Vec<Box<FrameRecord>>>,
}

impl Default for RecordChain {
    fn default() -> Self {
        Self {
            top: Cell::new(ptr::null()),
            count: Cell::new(0),
            zombies: UnsafeCell::new(Vec::new()),
        }
    }
}

impl RecordChain {
    /// The records from the newest down.
    fn iter(&self) -> impl Iterator<Item = &FrameRecord> {
        let mut next = self.top.get();
        std::iter::from_fn(move || {
            // SAFETY: a linked record is alive: a live one is pinned in a
            // running or parked Rust frame below every newer record, and a
            // zombie is boxed in `zombies` until it is unlinked.
            #[allow(unsafe_code)]
            let record = unsafe { next.as_ref()? };
            next = record.prev();
            Some(record)
        })
    }

    #[allow(
        unsafe_code,
        clippy::mut_from_ref,
        clippy::vec_box,
        reason = "the chain links zombies by address"
    )]
    fn zombies(&self) -> &mut Vec<Box<FrameRecord>> {
        // SAFETY: the zombie list is only touched by the VM's thread, through
        // these short-lived borrows that never overlap.
        unsafe { &mut *self.zombies.get() }
    }

    /// Unlink `record` from the chain, wherever it is.
    fn unlink(&self, record: *const FrameRecord) {
        let prev = {
            // SAFETY: see `iter`.
            #[allow(unsafe_code)]
            let record = unsafe { &*record };
            record.prev()
        };
        self.relink(record, prev);
    }

    /// Make whatever pointed at `old` point at `new`.
    fn relink(&self, old: *const FrameRecord, new: *const FrameRecord) {
        if ptr::eq(self.top.get(), old) {
            self.top.set(new);
            return;
        }
        if let Some(above) = self.iter().find(|record| ptr::eq(record.prev(), old)) {
            above.prev.set(new);
        }
    }

    pub(crate) fn collect_roots(&self, roots: &mut Vec<HeapPtr>) {
        for record in self.iter() {
            record.collect_roots(roots);
        }
    }

    pub(crate) fn forward_roots(&self, roots: &HashMap<HeapPtr, HeapPtr>) {
        for record in self.iter() {
            record.forward_roots(roots);
        }
    }
}

// SAFETY: the chain is only read and written by the thread driving the VM;
// see `Fiber`'s `Send`.
#[allow(unsafe_code)]
unsafe impl Send for RecordChain {}

/// Pops a record when the call that pushed it ends normally or is unwound by
/// a fiber drop. An error path disarms it and leaves a zombie instead.
struct Pop {
    chain: *const RecordChain,
    prev: *const FrameRecord,
    counted: bool,
}

impl Drop for Pop {
    #[allow(
        clippy::inline_always,
        reason = "measured: the pop is part of the call fast path"
    )]
    #[inline(always)]
    #[allow(unsafe_code)]
    fn drop(&mut self) {
        // SAFETY: the chain is boxed in `CompiledState`, which drops the fiber
        // (and so unwinds this guard) before the chain.
        let chain = unsafe { &*self.chain };
        chain.top.set(self.prev);
        if self.counted {
            chain.count.set(chain.count.get() - 1);
        }
    }
}

/// A frame in the merged stack: a VM frame, or a record that is a frame of
/// its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FrameAt {
    Vm(usize),
    Record(*const FrameRecord),
}

impl BexVm {
    #[inline]
    pub(crate) fn chain(&self) -> &RecordChain {
        &self.compiled.records
    }

    /// Whether compiled callees run with frame records instead of VM frames;
    /// off with `BAML_COMPILED_FRAMES=1`.
    #[allow(clippy::inline_always, reason = "checked at every compiled call site")]
    #[inline(always)]
    #[must_use]
    pub fn direct_calls(&self) -> bool {
        #[cfg(test)]
        if super::call::FORCE_CALL_FUNNEL.with(std::cell::Cell::get)
            || super::call::FORCE_VM_FRAMES.with(std::cell::Cell::get)
        {
            return false;
        }
        // Both are set only for the call the VM makes next.
        debug_assert!(self.pending_call_trace.is_none() && !self.pending_call_passes_type_args);
        self.compiled.direct_calls
    }

    /// [`Self::direct_calls`] for a generic callee: a type argument naming a
    /// runtime declaration needs the VM frame's exact type values.
    #[inline]
    #[must_use]
    pub fn direct_calls_typed(&self, type_args: &[RealizedTy]) -> bool {
        self.direct_calls()
            && type_args
                .iter()
                .all(crate::reachable::is_statically_declared)
    }

    /// Run a compiled callee with a frame record, after `caller` notes the
    /// call site `pc`. The record is popped when the body returns; an error
    /// leaves it for the unwinder. `args` boxes the arguments for telemetry,
    /// which needs them only for an invocation it records.
    #[allow(clippy::inline_always, reason = "measured: the call fast path")]
    #[inline(always)]
    pub fn call_record<T: RecordResult, A: AsRef<[Value]>>(
        &mut self,
        caller: &FrameRecord,
        callee: HeapPtr,
        pc: u32,
        args: impl FnOnce(&mut BexVm) -> A,
        body: impl FnOnce(&mut BexVm, &FrameRecord) -> Result<T, Box<VmError>>,
    ) -> Result<T, Box<VmError>> {
        caller.note_pc(pc);
        let observe = |vm: &mut BexVm, _: &[RealizedTy]| {
            let function = vm.observed_callee(callee)?;
            let args = args(vm);
            vm.enter_record_receipt(caller, function, callee, &[], &[], args.as_ref())
        };
        self.call_record_in(callee, caller.package.get(), None, observe, body)
    }

    /// [`Self::call_record`] for a generic callee, whose frame starts with
    /// the call's type arguments.
    #[inline]
    pub fn call_record_typed<T: RecordResult, A: AsRef<[Value]>>(
        &mut self,
        caller: &FrameRecord,
        callee: HeapPtr,
        type_args: &[RealizedTy],
        pc: u32,
        args: impl FnOnce(&mut BexVm) -> A,
        body: impl FnOnce(&mut BexVm, &FrameRecord) -> Result<T, Box<VmError>>,
    ) -> Result<T, Box<VmError>> {
        caller.note_pc(pc);
        let observe = |vm: &mut BexVm, type_args: &[RealizedTy]| {
            let function = vm.observed_callee(callee)?;
            let args = args(vm);
            vm.enter_record_receipt(caller, function, callee, &[], type_args, args.as_ref())
        };
        self.call_record_in(
            callee,
            caller.package.get(),
            boxed_types(type_args.to_vec()),
            observe,
            body,
        )
    }

    /// Run a callee from a runtime-dispatched call (a closure or an
    /// interface implementation) with a frame record, after `caller` notes
    /// the call site `pc`. A closure's frame carries its captured type
    /// arguments.
    pub(crate) fn call_record_dispatched(
        &mut self,
        caller: &FrameRecord,
        callee: HeapPtr,
        body: CompiledFn,
        type_args: Vec<RealizedTy>,
        args: &[Value],
        pc: u32,
    ) -> Result<Value, Box<VmError>> {
        caller.note_pc(pc);
        let package = self.frame_package(callee);
        let observe = |vm: &mut BexVm, carried: &[RealizedTy]| {
            let function = vm.observed_callee(callee)?;
            vm.enter_record_receipt(caller, function, callee, carried, &[], args)
        };
        self.call_record_in(
            callee,
            package,
            boxed_types(type_args),
            observe,
            |vm, frame| body(vm, frame, args),
        )
    }

    #[allow(
        clippy::inline_always,
        clippy::cast_possible_truncation,
        reason = "measured: the call fast path; levels are below `MAX_FRAMES` and stack \
                  offsets fit u32"
    )]
    #[inline(always)]
    fn call_record_in<T: RecordResult>(
        &mut self,
        callee: HeapPtr,
        package: HeapPtr,
        type_args: Option<Box<[RealizedTy]>>,
        observe: impl FnOnce(&mut BexVm, &[RealizedTy]) -> Option<FrameTelemetry>,
        body: impl FnOnce(&mut BexVm, &FrameRecord) -> Result<T, Box<VmError>>,
    ) -> Result<T, Box<VmError>> {
        let chain: *const RecordChain = &raw const *self.compiled.records;
        // SAFETY: see `Pop`.
        #[allow(unsafe_code)]
        let links = unsafe { &*chain };
        let count = links.count.get();
        let level = self.frames.len() + count as usize;
        if level >= crate::vm::MAX_FRAMES {
            return Err(Box::new(self.record_overflow()));
        }
        self.stack_room()?;
        let base = self.stack.len();
        let record = FrameRecord {
            prev: Cell::new(links.top.get()),
            function: Cell::new(callee),
            package: Cell::new(package),
            pc: Cell::new(0),
            anchor: self.frames.len() as u32,
            base: base as u32,
            level: level as u32,
            entry: false,
            receipt: Cell::new(None),
            type_args: UnsafeCell::new(type_args),
        };
        // The invocation is entered with the caller still innermost, as the
        // call path enters it before pushing the callee's frame. The receipt
        // is written into the record out of line: a receipt built on this
        // path and moved in would be a store-forwarding stall on every call.
        if self.telemetry.is_some() && self.observed_callee(callee).is_some() {
            self.observe_into(&record, observe);
        }
        links.top.set(&raw const record);
        links.count.set(count + 1);
        let pop = Pop {
            chain,
            prev: record.prev(),
            counted: true,
        };
        let result = body(self, &record);
        match &result {
            Ok(value) => {
                self.stack.truncate(base);
                drop(pop);
                if record.has_receipt() {
                    self.complete_record(&record, value);
                }
            }
            Err(_) => {
                std::mem::forget(pop);
                let linked: *const FrameRecord = &raw const record;
                self.zombify(linked, record);
            }
        }
        result
    }

    /// The callee's function when telemetry may record its invocation.
    #[inline]
    fn observed_callee(&self, callee: HeapPtr) -> Option<&'static Function> {
        // SAFETY: a callee is rooted by its caller for the call.
        #[allow(unsafe_code)]
        let function = unsafe { self.load_callable(callee) }.ok()?;
        self.telemetry
            .as_ref()?
            .may_record(function)
            .then_some(function)
    }

    /// Enter a directly called callee's invocation, with the caller facts the
    /// call path reads from `caller`'s frame.
    #[cold]
    #[inline(never)]
    #[allow(unsafe_code)]
    fn enter_record_receipt(
        &mut self,
        caller: &FrameRecord,
        function: &'static Function,
        callee: HeapPtr,
        carried: &[RealizedTy],
        passed: &[RealizedTy],
        args: &[Value],
    ) -> Option<FrameTelemetry> {
        let (visible_caller, caller_pc, caller_is_observed) = self.direct_caller_facts(caller);
        let callee_fn = self.callable_function_identity(callee).unwrap_or(callee);
        self.select_current_context();
        let telemetry = self.telemetry.as_mut()?;
        // SAFETY: the heap permit is held, and the caller keeps the
        // arguments and callee live.
        unsafe {
            telemetry.enter_bytecode_with_trace(
                function,
                callee_fn,
                visible_caller,
                caller_pc,
                caller_is_observed,
                args,
                None,
                CallTypeArgs { carried, passed },
                |caller, callee| Self::register_call_path_functions(&self.heap, caller, callee),
            )
        }
    }

    /// Enter the invocation of the callee with `record`, before it is linked.
    #[cold]
    #[inline(never)]
    fn observe_into(
        &mut self,
        record: &FrameRecord,
        observe: impl FnOnce(&mut BexVm, &[RealizedTy]) -> Option<FrameTelemetry>,
    ) {
        record.receipt.set(observe(self, record.own_type_args()));
    }

    /// Complete a directly called callee's invocation with its result.
    #[cold]
    #[inline(never)]
    fn complete_record<T: RecordResult>(&mut self, record: &FrameRecord, value: &T) {
        let Some(receipt) = record.receipt.take() else {
            return;
        };
        let callee = record.function();
        let value = value.to_value(self);
        // SAFETY: the callee is rooted by its caller for the call.
        #[allow(unsafe_code)]
        let Ok(function) = (unsafe { self.load_callable(callee) }) else {
            return;
        };
        self.complete_record_telemetry(receipt, function, value);
    }

    /// Keep the record of a body that returned an error on the stack, boxed,
    /// for the unwinder.
    #[cold]
    #[inline(never)]
    fn zombify(&mut self, linked: *const FrameRecord, record: FrameRecord) {
        let chain = &*self.compiled.records;
        let zombie = Box::new(record);
        // The box holds the record the chain linked at `linked`.
        chain.relink(linked, &raw const *zombie);
        chain.zombies().push(zombie);
    }

    /// Run the body of the VM frame on top of `vm.frames`, with its entry
    /// record.
    pub(crate) fn run_entry(
        &mut self,
        body: CompiledFn,
        args: &[Value],
    ) -> Result<Value, Box<VmError>> {
        self.run_entry_with(|vm, record| body(vm, record, args))
    }

    /// [`Self::run_entry`] for any body.
    #[allow(
        clippy::cast_possible_truncation,
        reason = "levels are below `MAX_FRAMES` and stack offsets fit u32"
    )]
    pub(crate) fn run_entry_with<T>(
        &mut self,
        body: impl FnOnce(&mut BexVm, &FrameRecord) -> Result<T, Box<VmError>>,
    ) -> Result<T, Box<VmError>> {
        let idx = self.frames.len() - 1;
        let Frame::Bytecode(frame) = &self.frames[idx] else {
            unreachable!("a compiled body runs in a bytecode frame");
        };
        let function = frame.function;
        let base = frame.locals_offset.raw();
        let package = self.frame_package(function);
        let chain: *const RecordChain = &raw const *self.compiled.records;
        // SAFETY: see `Pop`.
        #[allow(unsafe_code)]
        let links = unsafe { &*chain };
        let record = FrameRecord {
            prev: Cell::new(links.top.get()),
            function: Cell::new(function),
            package: Cell::new(package),
            pc: Cell::new(0),
            anchor: self.frames.len() as u32,
            base: base as u32,
            level: (idx + links.count.get() as usize) as u32,
            entry: true,
            receipt: Cell::new(None),
            type_args: UnsafeCell::new(None),
        };
        links.top.set(&raw const record);
        let pop = Pop {
            chain,
            prev: record.prev(),
            counted: false,
        };
        let result = body(self, &record);
        if result.is_err() {
            // The VM frame stays for the unwinder at the position the body
            // reported; zombies above the entry record stay above the frame.
            std::mem::forget(pop);
            if let Some(Frame::Bytecode(frame)) = self.frames.get_mut(idx) {
                frame.faulting_pc = record.pc() as usize;
            }
            self.compiled.records.unlink(&raw const record);
        }
        result
    }

    /// The runtime package the callable `function` runs in.
    pub(crate) fn frame_package(&self, function: HeapPtr) -> HeapPtr {
        match self.get_object(function) {
            Object::Function(function) => self.runtime_package_of(function),
            Object::GenericFunction(function) => function.runtime_package,
            // SAFETY: a closure or bound method roots its function.
            #[allow(unsafe_code)]
            Object::Closure(closure) => match unsafe { closure.function.get() } {
                Object::Function(function) => self.runtime_package_of(function),
                _ => HeapPtr::null(),
            },
            #[allow(unsafe_code)]
            Object::BoundMethod(method) => match unsafe { method.function.get() } {
                Object::Function(function) => self.runtime_package_of(function),
                _ => HeapPtr::null(),
            },
            _ => HeapPtr::null(),
        }
    }

    /// The object at `idx` in the package the body of `record` runs in.
    #[allow(
        clippy::inline_always,
        reason = "resolves every object generated code names"
    )]
    #[inline(always)]
    #[must_use]
    pub fn record_object(&self, record: &FrameRecord, idx: bex_vm_types::ObjectIndex) -> HeapPtr {
        let package = record.package.get();
        if package.as_ptr().is_null() {
            return self.heap.compile_time_ptr(idx.into_raw());
        }
        self.package_object(package, idx)
    }

    #[cold]
    #[inline(never)]
    fn record_overflow(&mut self) -> VmError {
        VmError::thrown_fresh(self.panic_to_exception_value(VmPanic::StackOverflow))
    }

    /// The number of frames: VM frames and records that are frames.
    #[inline]
    pub(crate) fn level_count(&self) -> usize {
        self.frames.len() + self.chain().count.get() as usize
    }

    /// The merged stack, outermost first.
    pub(crate) fn stack_view(&self) -> Vec<FrameAt> {
        let mut out = Vec::with_capacity(self.level_count());
        let mut vm = self.frames.len();
        let mut records = self.chain().iter().peekable();
        loop {
            match records.peek() {
                Some(record) if vm <= record.anchor() => {
                    if !record.entry {
                        out.push(FrameAt::Record(ptr::from_ref(*record)));
                    }
                    records.next();
                }
                _ if vm > 0 => {
                    vm -= 1;
                    out.push(FrameAt::Vm(vm));
                }
                _ => break,
            }
        }
        out.reverse();
        debug_assert_eq!(out.len(), self.level_count());
        out
    }

    /// Where the slots of the body parked at the suspended compiled VM frame
    /// `idx` end: the innermost record above the frame, or the frame's own
    /// body at `locals_offset`. Everything above is the request's operands.
    pub(crate) fn suspended_slots_end(
        &self,
        idx: usize,
        locals_offset: usize,
    ) -> Result<usize, crate::errors::VmInternalError> {
        let (frame, base) = match self.innermost_at(idx) {
            FrameAt::Record(record) => (FrameAt::Record(record), record_ref(record).base()),
            at @ FrameAt::Vm(_) => (at, locals_offset),
        };
        // SAFETY: a parked body's frame roots its function.
        #[allow(unsafe_code)]
        let function = unsafe { self.frame_function(frame)? };
        let slots = function
            .compiled_body
            .map_or(0, |body| body.slots() as usize);
        Ok(base + slots)
    }

    /// The callable and compact pc of every frame that runs BAML code,
    /// innermost first: what the engine walks to find `defer` shielding.
    #[must_use]
    pub fn frame_positions(&self) -> Vec<(HeapPtr, usize)> {
        let live = self.live_bytecode_frame();
        self.stack_view()
            .into_iter()
            .rev()
            .filter(|frame| self.frame_is_bytecode(*frame))
            .filter_map(|frame| Some((self.frame_callable(frame), self.frame_at_pc(frame, live)?)))
            .collect()
    }

    /// The frame at `level`.
    pub(crate) fn frame_at_level(&self, level: usize) -> Option<FrameAt> {
        if self.chain().top.get().is_null() {
            return (level < self.frames.len()).then_some(FrameAt::Vm(level));
        }
        self.stack_view().get(level).copied()
    }

    /// The level of VM frame `idx`.
    pub(crate) fn level_of_vm(&self, idx: usize) -> usize {
        // The newest record below the frame fixes the levels above it.
        match self.chain().iter().find(|record| record.anchor() <= idx) {
            Some(record) => record.level() + (idx + 1 - record.anchor()),
            None => idx,
        }
    }

    pub(crate) fn level_of(&self, frame: FrameAt) -> usize {
        match frame {
            FrameAt::Vm(idx) => self.level_of_vm(idx),
            FrameAt::Record(record) => record_ref(record).level(),
        }
    }

    /// The innermost frame.
    pub(crate) fn top_frame(&self) -> Option<FrameAt> {
        match self.chain().iter().find(|record| !record.entry) {
            Some(record) if record.anchor() >= self.frames.len() => {
                Some(FrameAt::Record(ptr::from_ref(record)))
            }
            _ => self.frames.len().checked_sub(1).map(FrameAt::Vm),
        }
    }

    /// The frame executing at VM frame `idx`: the innermost record standing
    /// directly above it, or the frame itself.
    pub(crate) fn innermost_at(&self, idx: usize) -> FrameAt {
        for record in self.chain().iter() {
            if record.anchor() > idx + 1 {
                continue;
            }
            if record.anchor() == idx + 1 && !record.entry {
                return FrameAt::Record(ptr::from_ref(record));
            }
            break;
        }
        FrameAt::Vm(idx)
    }

    /// The entry record of VM frame `idx`, while its body is on the fiber.
    pub(crate) fn entry_record(&self, idx: usize) -> Option<&FrameRecord> {
        let mut found = None;
        for record in self.chain().iter() {
            if record.anchor() > idx + 1 {
                continue;
            }
            if record.anchor() < idx + 1 {
                break;
            }
            if record.entry {
                found = Some(record);
            }
        }
        found
    }

    /// The callable frame `frame` runs.
    pub(crate) fn frame_callable(&self, frame: FrameAt) -> HeapPtr {
        match frame {
            FrameAt::Vm(idx) => self.frames[idx].function(),
            FrameAt::Record(record) => record_ref(record).function(),
        }
    }

    /// The function frame `frame` runs; see `load_function`.
    #[allow(unsafe_code)]
    pub(crate) unsafe fn frame_function(
        &self,
        frame: FrameAt,
    ) -> Result<&'static Function, crate::errors::VmInternalError> {
        match frame {
            // SAFETY: forwarded to the caller.
            FrameAt::Vm(idx) => unsafe { self.load_function(idx) },
            // SAFETY: forwarded to the caller.
            FrameAt::Record(record) => unsafe { self.load_callable(record_ref(record).function()) },
        }
    }

    /// The telemetry receipt of `frame`.
    pub(crate) fn frame_receipt(&self, frame: FrameAt) -> Option<FrameTelemetry> {
        match frame {
            FrameAt::Vm(idx) => match &self.frames[idx] {
                Frame::Bytecode(frame) => frame.telemetry,
                Frame::Native(_) => None,
            },
            FrameAt::Record(record) => record_ref(record).receipt(),
        }
    }

    /// Mutate the telemetry receipt of `frame`.
    pub(crate) fn with_frame_receipt<R>(
        &mut self,
        frame: FrameAt,
        f: impl FnOnce(&mut Option<FrameTelemetry>) -> R,
    ) -> R {
        match frame {
            FrameAt::Vm(idx) => match &mut self.frames[idx] {
                Frame::Bytecode(frame) => f(&mut frame.telemetry),
                Frame::Native(_) => f(&mut None),
            },
            FrameAt::Record(record) => {
                let record = record_ref(record);
                let mut receipt = record.receipt();
                let out = f(&mut receipt);
                record.set_receipt(receipt);
                out
            }
        }
    }

    /// Whether `frame` is a bytecode frame or a record (not a native
    /// continuation).
    pub(crate) fn frame_is_bytecode(&self, frame: FrameAt) -> bool {
        match frame {
            FrameAt::Vm(idx) => matches!(self.frames[idx], Frame::Bytecode(_)),
            FrameAt::Record(_) => true,
        }
    }

    /// Whether `frame` runs a compiled body.
    pub(crate) fn frame_runs_compiled(&self, frame: FrameAt) -> bool {
        match frame {
            FrameAt::Vm(idx) => self.compiled_at(idx).is_some(),
            FrameAt::Record(_) => true,
        }
    }

    /// The compact pc `frame` stands at; see [`Self::frame_pc`].
    pub(crate) fn frame_at_pc(&self, frame: FrameAt, live: Option<usize>) -> Option<usize> {
        match frame {
            FrameAt::Vm(idx) => self.frame_pc(idx, live),
            FrameAt::Record(record) => {
                let record = record_ref(record);
                let function = self.get_object(record.function()).as_callable().ok()?;
                function
                    .bytecode
                    .compact
                    .as_ref()?
                    .offset_of(record.pc() as usize)
            }
        }
    }

    /// The instruction index a compiled frame last reported.
    pub(crate) fn compiled_instruction(&self, frame: FrameAt) -> usize {
        match frame {
            FrameAt::Vm(idx) => match self.entry_record(idx) {
                Some(record) => record.pc() as usize,
                None => match &self.frames[idx] {
                    Frame::Bytecode(frame) => frame.faulting_pc,
                    Frame::Native(_) => 0,
                },
            },
            FrameAt::Record(record) => record_ref(record).pc() as usize,
        }
    }

    /// The error context a compiled frame is handling, from its cause slot.
    pub(crate) fn frame_cause(&self, frame: FrameAt) -> Option<Value> {
        match frame {
            FrameAt::Vm(idx) => self.compiled_cause(idx),
            FrameAt::Record(record) => {
                let record = record_ref(record);
                let function = self.get_object(record.function()).as_callable().ok()?;
                let slot = function.compiled_body?.cause_slot()?;
                let context = *self.stack.0.get(record.base() + slot as usize)?;
                (!context.is_null()).then_some(context)
            }
        }
    }

    /// The frame a compiled body's record stands for.
    pub(crate) fn record_frame(record: &FrameRecord) -> FrameAt {
        if record.entry {
            FrameAt::Vm(record.anchor() - 1)
        } else {
            FrameAt::Record(ptr::from_ref(record))
        }
    }

    /// The VM frame a body's record belongs to, if it is an entry record.
    pub(crate) fn record_vm_frame(record: &FrameRecord) -> Option<usize> {
        record.entry.then(|| record.anchor() - 1)
    }

    /// Remove the zombie record `frame`, the innermost frame, once the
    /// unwinder has completed it.
    pub(crate) fn pop_zombie(&mut self, frame: *const FrameRecord) {
        let chain = &*self.compiled.records;
        debug_assert!(
            ptr::eq(chain.top.get(), frame),
            "zombies unwind innermost first"
        );
        let base = record_ref(frame).base();
        chain.unlink(frame);
        chain.count.set(chain.count.get() - 1);
        let zombies = chain.zombies();
        if let Some(at) = zombies
            .iter()
            .position(|zombie| ptr::eq(&raw const **zombie, frame))
        {
            zombies.swap_remove(at);
        }
        self.stack.truncate(base);
    }

    /// Whether the record `frame` is a zombie.
    pub(crate) fn is_zombie(&self, frame: *const FrameRecord) -> bool {
        self.chain()
            .zombies()
            .iter()
            .any(|zombie| ptr::eq(&raw const **zombie, frame))
    }

    /// Run a callee with a VM frame on the call path's terms, as before frame
    /// records; see [`Self::call_compiled`].
    pub(super) fn push_compiled_frame(
        &mut self,
        callee: HeapPtr,
        type_args: Vec<RealizedTy>,
        type_metadata: Option<Box<crate::vm::FrameTypeMetadata>>,
        telemetry: Option<FrameTelemetry>,
    ) -> StackIndex {
        let context = self.current_context().clone();
        let locals_offset = StackIndex::from_raw(self.stack.len());
        let mut frame =
            BytecodeFrame::compiled(callee, locals_offset, type_args, context, telemetry);
        frame.type_metadata = type_metadata;
        self.frames.push(Frame::Bytecode(frame));
        locals_offset
    }

    /// Caller facts for a call made by compiled code at `caller`, for the
    /// telemetry call path.
    pub(crate) fn record_caller_facts(&self, caller: FrameAt) -> (Option<HeapPtr>, u32, bool) {
        let FrameAt::Vm(idx) = caller else {
            let pc = self
                .frame_at_pc(caller, None)
                .map_or(0, |pc| u32::try_from(pc).unwrap_or(u32::MAX));
            return (
                self.frame_identity(caller),
                pc,
                self.frame_receipt(caller).is_some(),
            );
        };
        let resolved = self.bytecode_caller(idx);
        (
            self.visible_bytecode_caller(idx)
                .and_then(|at| self.frame_identity(at)),
            self.caller_pc(resolved, idx, 0),
            self.frame_receipt(resolved).is_some(),
        )
    }

    /// Caller facts for a call compiled code makes directly: the calling
    /// body's identity, the compact offset of the call site it recorded, and
    /// whether its invocation is observed, all read from its own record (an
    /// entry record's frame is observed through its VM frame). A body making
    /// a call runs; it is neither a native continuation nor a hook target
    /// whose hook is evaluating, which [`Self::record_caller_facts`] resolves.
    fn direct_caller_facts(&self, caller: &FrameRecord) -> (Option<HeapPtr>, u32, bool) {
        let ptr = caller.function();
        let identity = self.callable_function_identity(ptr);
        let pc = self
            .get_object(ptr)
            .as_callable()
            .ok()
            .and_then(|function| {
                function
                    .bytecode
                    .compact
                    .as_ref()?
                    .offset_of(caller.pc() as usize)
            })
            .map_or(0, |pc| u32::try_from(pc).unwrap_or(u32::MAX));
        let observed = match Self::record_vm_frame(caller) {
            Some(idx) => matches!(
                &self.frames[idx],
                Frame::Bytecode(frame) if frame.telemetry.is_some()
            ),
            None => caller.has_receipt(),
        };
        (identity, pc, observed)
    }

    /// Make the current context telemetry's, cloning it only when it changed.
    fn select_current_context(&mut self) {
        let Some(telemetry) = self.telemetry.as_ref() else {
            return;
        };
        if !telemetry.has_context(self.current_context()) {
            let context = self.current_context().clone();
            if let Some(telemetry) = self.telemetry.as_mut() {
                telemetry.set_context(context);
            }
        }
    }

    /// Enter a compiled callee's invocation for telemetry, with caller facts
    /// read from `caller`.
    #[allow(unsafe_code)]
    pub(crate) fn enter_record_telemetry(
        &mut self,
        caller: FrameAt,
        function: &'static Function,
        callee: HeapPtr,
        args: &[Value],
    ) -> Option<FrameTelemetry> {
        let (visible_caller, caller_pc, caller_is_observed) = self.record_caller_facts(caller);
        let context = self.current_context().clone();
        let telemetry = self.telemetry.as_mut()?;
        if !telemetry.has_context(&context) {
            telemetry.set_context(context);
        }
        // SAFETY: the heap permit is held, and the caller's locals keep the
        // arguments and callee live.
        unsafe {
            telemetry.enter_bytecode_with_trace(
                function,
                callee,
                visible_caller,
                caller_pc,
                caller_is_observed,
                args,
                None,
                CallTypeArgs::default(),
                |caller, callee| Self::register_call_path_functions(&self.heap, caller, callee),
            )
        }
    }

    /// Complete a returning callee's invocation.
    #[allow(unsafe_code)]
    pub(crate) fn complete_record_telemetry(
        &mut self,
        receipt: FrameTelemetry,
        function: &Function,
        value: Value,
    ) {
        self.select_current_context();
        let Some(telemetry) = self.telemetry.as_mut() else {
            return;
        };
        // SAFETY: the heap permit is held and the result is live.
        unsafe {
            telemetry.complete_invocation(receipt, function, InvocationOutcome::Ok, Some(value));
        }
    }
}

/// Type arguments as a record holds them.
fn boxed_types(type_args: Vec<RealizedTy>) -> Option<Box<[RealizedTy]>> {
    (!type_args.is_empty()).then(|| type_args.into_boxed_slice())
}

/// What a compiled body returns, boxed for telemetry's output capture.
pub trait RecordResult {
    fn to_value(&self, vm: &mut BexVm) -> Value;
}

impl RecordResult for Value {
    fn to_value(&self, _: &mut BexVm) -> Value {
        *self
    }
}

impl RecordResult for i64 {
    fn to_value(&self, _: &mut BexVm) -> Value {
        Value::int(*self)
    }
}

impl RecordResult for bool {
    fn to_value(&self, _: &mut BexVm) -> Value {
        Value::bool(*self)
    }
}

impl RecordResult for f64 {
    fn to_value(&self, vm: &mut BexVm) -> Value {
        vm.float_value(*self)
    }
}

/// A linked record; see [`RecordChain::iter`].
#[inline]
pub(crate) fn record_ref<'a>(record: *const FrameRecord) -> &'a FrameRecord {
    // SAFETY: `FrameAt::Record` is only built from linked records, and used
    // while they stay linked.
    #[allow(unsafe_code)]
    unsafe {
        &*record
    }
}
