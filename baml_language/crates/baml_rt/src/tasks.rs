//! Managed futures and owner-thread bindings for compiled task entry points.
use crate::{Dyn, Error, R};
use baml_rt_gc::{Descriptor, ElementLayout, Elements, Header, Kind};
pub use baml_rt_tasks::SpawnOptions;
use baml_rt_tasks::{SwitchHook, TaskCx, Value};
use std::{cell::Cell, mem::size_of, ptr};

pub const FUTURE_FINALIZER: u32 = u32::MAX;
// The managed future is the producer's only future record: the loop keeps
// just the live task (waiters, cancellation) and the producer settles here.
// Native state exists only while telemetry is pending or after an error, so
// only those futures are registered for finalization.
#[repr(C)]
struct Future {
    header: Header,
    environment: usize,
    result: Dyn,
    error: *mut Error,
    // Loop task id; meaningful only while the producer has not settled.
    task: usize,
    telemetry: Option<Box<crate::telemetry::Task>>,
    // Index into the native finalizer table, or NO_REGISTRATION.
    registration: u32,
    flags: u32,
}
const NO_REGISTRATION: u32 = u32::MAX;
const OBSERVED: u32 = 1;
const CANCEL_REQUESTED: u32 = 2;
// The producer unwound without its epilogue (a native panic).
const PANICKED: u32 = 4;
impl Future {
    fn settled(&self) -> bool {
        self.result.tag != u64::MAX || !self.error.is_null() || self.flags & PANICKED != 0
    }
}
// Preserve the ordinary future's 16-byte rounded footprint. Optional trailing
// tagged slots coallocate a private closure environment with its future.
const FUTURE_BYTES: usize = (size_of::<Future>() + 15) & !15;
#[cfg(target_pointer_width = "64")]
const _: () = assert!(size_of::<Future>() == 64);
pub const fn future_descriptor(type_id: u32) -> Descriptor {
    Descriptor {
        kind: Kind::Future,
        size: size_of::<Dyn>(),
        ptr_slots: (8, 1),
        tagged_slots: (16, 1),
        elem: Some(Elements {
            offset: FUTURE_BYTES,
            layout: ElementLayout::Tagged,
        }),
        type_id,
        fields: &[],
        // Attached on demand, when an error is stored.
        finalizer: None,
    }
}

pub struct UnhandledTaskError {
    pub error: Box<Error>,
    pub cancelled: bool,
}
// The table is deliberately NOT a root source. Objects own their native data;
// the GC callback clears its entry, including cycles of settled future values.
#[derive(Default)]
pub(crate) struct NativeFinalizers {
    // Registered futures with their spawn sequence numbers.
    entries: Vec<(usize, u64)>,
    free: Vec<usize>,
    resources: std::collections::HashMap<usize, Resource>,
    unhandled: Vec<(crate::CaughtError, bool, u64)>,
    next_sequence: u64,
}
impl NativeFinalizers {
    unsafe fn save_unhandled(&mut self, f: &mut Future) {
        if !f.error.is_null()
            && unsafe { &*f.error }.class != "baml.panics.Cancelled"
            && f.flags & OBSERVED == 0
        {
            let error = Box::new(unsafe { &*f.error }.clone());
            let sequence = self.entries[f.registration as usize].1;
            self.unhandled.push((
                unsafe { crate::CaughtError::new(error) },
                f.flags & CANCEL_REQUESTED != 0,
                sequence,
            ));
            f.flags |= OBSERVED;
        }
    }
    // Reports found together (one collection, or the end of an invocation)
    // follow the VM: dead futures in allocation order, i.e. spawn order,
    // whatever order their producers settled in.
    fn order_reports_since(&mut self, start: usize) {
        self.unhandled[start..].sort_by_key(|report| report.2);
    }
    pub(crate) unsafe fn finish_invocation(&mut self) {
        let start = self.unhandled.len();
        for (pointer, _) in self.entries.clone().into_iter().filter(|e| e.0 != 0) {
            unsafe {
                self.save_unhandled(&mut *(pointer as *mut Future));
            }
        }
        self.order_reports_since(start);
    }
    fn next_sequence(&mut self) -> u64 {
        let sequence = self.next_sequence;
        self.next_sequence += 1;
        sequence
    }
    pub(crate) fn take_reports(&mut self) -> Vec<UnhandledTaskError> {
        std::mem::take(&mut self.unhandled)
            .into_iter()
            .map(|(error, cancelled, _)| UnhandledTaskError {
                error: error.rethrow(),
                cancelled,
            })
            .collect()
    }
    pub(crate) fn take_unhandled(&mut self) -> Option<Box<Error>> {
        self.take_reports()
            .into_iter()
            .find(|r| !r.cancelled)
            .map(|r| r.error)
    }
    fn register(&mut self, pointer: usize, sequence: u64) -> usize {
        if let Some(i) = self.free.pop() {
            self.entries[i] = (pointer, sequence);
            i
        } else {
            let i = self.entries.len();
            self.entries.push((pointer, sequence));
            i
        }
    }
    fn release(&mut self, i: usize) {
        if self.entries[i].0 != 0 {
            self.entries[i].0 = 0;
            self.free.push(i);
        }
    }
    pub(crate) unsafe fn destroy(&mut self) {
        for (pointer, _) in self.entries.drain(..).filter(|e| e.0 != 0) {
            unsafe {
                let f = &mut *(pointer as *mut Future);
                f.telemetry.take();
                if !f.error.is_null() {
                    drop(Box::from_raw(f.error));
                    f.error = ptr::null_mut();
                }
            }
        }
        self.free.clear();
        self.resources.clear();
    }
}
thread_local! { pub(crate) static FINALIZERS: Cell<*mut NativeFinalizers> = const { Cell::new(ptr::null_mut()) }; }

unsafe fn register(pointer: usize, sequence: u64) -> u32 {
    let index = unsafe { (&mut *FINALIZERS.get()).register(pointer, sequence) };
    u32::try_from(index)
        .ok()
        .filter(|&i| i != NO_REGISTRATION)
        .expect("future registrations exhausted")
}
unsafe fn release_registration(f: &mut Future) {
    if f.registration != NO_REGISTRATION {
        unsafe {
            (&mut *FINALIZERS.get()).release(f.registration as usize);
        }
        f.registration = NO_REGISTRATION;
    }
}
pub(crate) unsafe fn finalize(pointer: usize) {
    unsafe {
        let f = &mut *(pointer as *mut Future);
        (&mut *FINALIZERS.get()).save_unhandled(f);
        f.telemetry.take();
        if !f.error.is_null() {
            drop(Box::from_raw(f.error));
            f.error = ptr::null_mut();
        }
        release_registration(f);
    }
}
pub(crate) unsafe fn drain_finalizers() {
    let pending = unsafe { (&mut *crate::HEAP.get()).take_pending_finalizers() };
    let start = unsafe { (&*FINALIZERS.get()).unhandled.len() };
    for token in pending {
        unsafe {
            if token.cleanup == FUTURE_FINALIZER {
                finalize(token.object() as usize);
            } else if token.cleanup == RESOURCE_FINALIZER {
                (&mut *FINALIZERS.get())
                    .resources
                    .remove(&(token.object() as usize));
            } else {
                let callback = CLEANUP.get().expect("generated cleanup dispatcher");
                let _shield = shield();
                let _ = callback(token.cleanup, token.object() as usize);
            }
            (&mut *crate::HEAP.get()).complete_finalizer(token);
        }
    }
    unsafe { (&mut *FINALIZERS.get()).order_reports_since(start) };
}

pub(crate) struct FiberBinding<'a> {
    cx: *mut TaskCx<'a>,
    limit: usize,
    telemetry: Option<crate::telemetry::Fiber>,
    previous_entry_error: *mut Error,
    previous_task: *mut TaskCx<'static>,
    previous_limit: usize,
    previous_telemetry: *mut (),
}
impl<'a> FiberBinding<'a> {
    pub(crate) unsafe fn new(cx: &mut TaskCx<'a>, telemetry: *mut ()) -> Self {
        unsafe {
            Self {
                cx,
                limit: cx.stack_limit(),
                telemetry: crate::telemetry::Fiber::current(telemetry),
                previous_entry_error: crate::ENTRY_ERROR.replace(ptr::null_mut()),
                previous_task: crate::TASK.get(),
                previous_limit: crate::STACK_LIMIT.get(),
                previous_telemetry: crate::telemetry::current_raw(),
            }
        }
    }
    pub(crate) unsafe fn install(&mut self) {
        unsafe {
            enter((self as *mut Self).cast());
            (*self.cx).set_switch_hook(Some(SwitchHook {
                data: (self as *mut Self).cast(),
                enter,
                leave,
            }));
        }
    }
}
unsafe fn enter(pointer: *mut ()) {
    unsafe {
        let b = &mut *pointer.cast::<FiberBinding<'_>>();
        crate::TASK.set(b.cx.cast());
        crate::STACK_LIMIT.set(b.limit);
        if let Some(telemetry) = &mut b.telemetry {
            telemetry.enter();
        } else {
            crate::telemetry::replace_current_raw(ptr::null_mut());
        }
    }
}
unsafe fn leave(pointer: *mut ()) {
    unsafe {
        if let Some(telemetry) = &mut (*pointer.cast::<FiberBinding<'_>>()).telemetry {
            telemetry.leave();
        }
    }
    crate::TASK.set(ptr::null_mut());
    crate::STACK_LIMIT.set(0);
    unsafe {
        crate::telemetry::replace_current_raw(ptr::null_mut());
    }
}
impl Drop for FiberBinding<'_> {
    fn drop(&mut self) {
        unsafe {
            crate::ADMISSION_CANCELLED.set(false);
            let error = crate::ENTRY_ERROR.replace(self.previous_entry_error);
            if !error.is_null() {
                drop(Box::from_raw(error));
            }
            (*self.cx).set_switch_hook(None);
            crate::telemetry::replace_current_raw(self.previous_telemetry);
        }
        crate::TASK.set(self.previous_task);
        crate::STACK_LIMIT.set(self.previous_limit);
    }
}

pub(crate) fn loop_error(error: baml_rt_tasks::Err) -> Box<Error> {
    match error {
        baml_rt_tasks::Err::Cancelled => {
            crate::language::from_panic(bex_vm_types::errors::VmPanic::Cancelled)
        }
        error => Error::internal(format!("{error:?}")),
    }
}

/// # Safety
/// The descriptor is a future descriptor, and the closure is live and rooted.
pub unsafe fn spawn(
    desc: u32,
    environment: usize,
    site: usize,
    entry_site: usize,
    mut options: SpawnOptions,
    name: Option<String>,
) -> R<usize> {
    unsafe {
        let slots = if crate::closures::is_private(environment) {
            crate::closures::private_slots(environment)
        } else {
            0
        };
        let pointer =
            crate::try_alloc(desc, FUTURE_BYTES + slots * size_of::<Dyn>())?.cast::<Future>();
        let environment = if slots != 0 {
            let target = pointer.cast::<u8>().add(FUTURE_BYTES).cast::<Dyn>();
            let source = (environment & !1) as *const Dyn;
            for i in 0..slots {
                crate::write_dyn(pointer.cast(), target.add(i), source.add(i).read());
            }
            (target as usize) | 1
        } else {
            environment
        };
        baml_rt_gc::write_ptr(pointer.cast(), &mut (*pointer).environment, environment);
        (*pointer).result = Dyn {
            tag: u64::MAX,
            payload: 0,
        };
        (*pointer).error = ptr::null_mut();
        (*pointer).flags = 0;
        let sequence = (&mut *FINALIZERS.get()).next_sequence();
        let telemetry = crate::telemetry::Task::spawn(site, name);
        // Until execution takes it, a telemetry record is native state that
        // shutdown must release; plain futures own nothing native.
        (*pointer).registration = if telemetry.is_some() {
            register(pointer as usize, sequence)
        } else {
            NO_REGISTRATION
        };
        ptr::write(&mut (*pointer).telemetry, telemetry);
        options.managed_roots = [pointer as usize, 0];
        (*pointer).task = (&mut *crate::TASK.get()).spawn_managed(
            &mut options,
            execute,
            [pointer as usize, site, entry_site, sequence as usize],
        );
        Ok(pointer as usize)
    }
}
unsafe fn execute(cx: &mut TaskCx<'_>, data: [usize; 4]) -> baml_rt_tasks::Outcome {
    unsafe {
        let f = data[0] as *mut Future;
        let telemetry = (*f).telemetry.take();
        release_registration(&mut *f);
        // A native panic unwinds through here without the epilogue below.
        struct Unwind(*mut Future);
        impl Drop for Unwind {
            fn drop(&mut self) {
                unsafe { (*self.0).flags |= PANICKED };
            }
        }
        let unwind = Unwind(f);
        let mut binding = FiberBinding::new(cx, ptr::null_mut());
        binding.telemetry = telemetry.as_ref().map(|task| task.fiber.clone());
        binding.install();
        if let Some(task) = &telemetry {
            task.announce();
        }
        let admission = cx.admit().map_err(loop_error);
        if let Some(task) = &telemetry {
            task.running(admission.is_err());
        }
        if let Err(error) = admission {
            crate::ENTRY_ERROR.set(Box::into_raw(error));
        }
        let mut result = crate::closures::call((*f).environment, data[2], &[]);
        if let Err(error) = &mut result {
            crate::telemetry::evidence::unhandled(error);
        }
        if let Some(task) = &telemetry {
            task.finish(&result);
        }
        if crate::closures::is_private((*f).environment) {
            // Capture cells must not stay rooted merely because a settled
            // future remains live after its producer's environment is dead.
            let env = (*f).environment;
            let slots = crate::closures::private_slots(env);
            let tail = (env & !1) as *mut Dyn;
            for i in 0..slots {
                crate::write_dyn(f.cast(), tail.add(i), Dyn { tag: 0, payload: 0 });
            }
        }
        baml_rt_gc::write_ptr(f.cast(), &mut (*f).environment, 0);
        let outcome = match result {
            Ok(value) => {
                crate::write_dyn(f.cast(), &mut (*f).result, value);
                Ok(Value::int(0))
            }
            Err(error) => {
                if error.value != 0 {
                    let desc = (*(error.value as *const Header)).desc;
                    crate::write_dyn(
                        f.cast(),
                        &mut (*f).result,
                        Dyn {
                            tag: desc as u64 + 3,
                            payload: error.value as u64,
                        },
                    );
                }
                let outcome = if error.class == "baml.panics.Cancelled" {
                    baml_rt_tasks::Err::Cancelled
                } else {
                    baml_rt_tasks::Err::Error {
                        message: error.message.clone(),
                        trace: error
                            .frames
                            .iter()
                            .map(|f| f.function.to_string())
                            .collect(),
                    }
                };
                (*f).error = Box::into_raw(error);
                // Preserve cancellation from ancestors, linked tokens and
                // deadlines, exactly as a loop-side future records it.
                if cx.cancellation_requested() {
                    (*f).flags |= CANCEL_REQUESTED;
                }
                // The error box is native state: register it for unhandled
                // reporting and release when the future dies.
                (*f).registration = register(f as usize, data[3] as u64);
                (&mut *crate::HEAP.get()).attach_finalizer(f as usize, FUTURE_FINALIZER);
                Err(outcome)
            }
        };
        std::mem::forget(unwind);
        outcome
    }
}

/// # Safety
/// The argument names a rooted future in the current heap.
pub unsafe fn await_future(pointer: usize) -> R<Dyn> {
    unsafe {
        let pointer = crate::nonnull(pointer)? as *mut Future;
        (&mut *crate::TASK.get())
            .check_cancel()
            .map_err(loop_error)?;
        if !(*pointer).settled() {
            crate::telemetry::before_yield();
            let result = (&mut *crate::TASK.get()).await_task((*pointer).task);
            // Admission errors still pass through the managed entry wrapper.
            if let Err(error) = result
                && (*pointer).error.is_null()
            {
                return Err(loop_error(error));
            }
        }
        if (*pointer).flags & PANICKED != 0 && (*pointer).error.is_null() {
            return Err(loop_error(baml_rt_tasks::Err::InternalError(
                "task panicked".into(),
            )));
        }
        (*pointer).flags |= OBSERVED;
        if !(*pointer).error.is_null() {
            let mut error = Box::new((*(*pointer).error).clone());
            // A cancelled await is a fresh raise in the awaiting task.
            error.telemetry_raise = None;
            error.telemetry_at = None;
            if error.class == "baml.panics.Cancelled" {
                error.telemetry_caught = None;
            }
            return Err(error);
        }
        Ok((*pointer).result)
    }
}

/// # Safety
/// The argument names a live future in the current heap.
pub unsafe fn cancel(pointer: usize) -> bool {
    unsafe {
        let f = &mut *(pointer as *mut Future);
        let previous = f.flags & CANCEL_REQUESTED != 0;
        f.flags |= CANCEL_REQUESTED;
        if previous || f.settled() {
            return false;
        }
        (&*crate::TASK.get()).cancel_task(f.task);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    static DESCRIPTORS: &[Descriptor] = &[
        crate::STRING_DESC,
        crate::OVERFLOW_DESC,
        crate::SHIFT_DESC,
        crate::CANCEL_DESC,
        crate::STACK_DESC,
        crate::ALLOC_DESC,
        crate::INDEX_DESC,
        crate::json::PARSE_DESC,
        crate::json::DECODE_DESC,
        crate::DIV_DESC,
        crate::closures::CELL_DESC,
        crate::closures::CLOSURE_DESC,
        future_descriptor(15),
        Descriptor {
            kind: Kind::Class,
            size: 16,
            ptr_slots: (0, 0),
            tagged_slots: (0, 0),
            elem: None,
            type_id: 16,
            fields: &[],
            finalizer: Some(42),
        },
    ];
    static POINTERS: &[bool] = &[
        false, false, false, true, true, true, true, true, true, true, true, true, true, true,
        true, true, true,
    ];
    thread_local! {
        static FINALIZED: Cell<usize> = const { Cell::new(0) };
        static ROOT_TASK: Cell<usize> = const { Cell::new(0) };
    }
    unsafe fn cleanup_dispatch(_: u32, object: usize) -> R<()> {
        unsafe {
            run_cleanup(0, object, move || {
                assert_ne!((&*crate::TASK.get()).id(), ROOT_TASK.get());
                assert!(cleanup_begin(object));
                (&mut *crate::TASK.get())
                    .sleep(std::time::Duration::ZERO)
                    .map_err(loop_error)?;
                FINALIZED.set(FINALIZED.get() + 1);
                Ok(())
            })
        }
    }
    #[test]
    fn gc_cleanup_uses_a_separate_shielded_fiber_and_completes_once() {
        fn entry() -> R<i64> {
            unsafe {
                ROOT_TASK.set((&*crate::TASK.get()).id());
                set_cleanup_dispatch(cleanup_dispatch);
                let _ = crate::try_alloc(13, 16)?;
                (&*crate::TASK.get()).cancel().cancel();
                (&mut *crate::HEAP.get()).collect(&baml_rt_gc::NoRoots);
                drain_finalizers();
                assert_eq!(FINALIZED.get(), 1);
                (&mut *crate::HEAP.get()).collect(&baml_rt_gc::NoRoots);
                drain_finalizers();
                assert_eq!(FINALIZED.get(), 1);
                Ok(1)
            }
        }
        FINALIZED.set(0);
        let mut instance = crate::Instance::new(DESCRIPTORS, POINTERS);
        assert_eq!(unsafe { instance.run_baml(entry) }.unwrap(), 1);
    }
    unsafe fn background_error(_: usize, _: usize, _: &[Dyn]) -> R<Dyn> {
        crate::add((1i64 << 62) - 1, 1).map(|n| Dyn {
            tag: 0,
            payload: n as u64,
        })
    }
    #[test]
    fn background_work_finishes_before_reuse_and_saved_errors_remain_rooted() {
        fn entry() -> R<i64> {
            unsafe {
                let env = crate::closures::new(11, 123, background_error, &[])?;
                let _ = spawn(12, env, 0, 0, SpawnOptions::default(), None)?;
            }
            Ok(7)
        }
        fn next() -> R<i64> {
            unsafe {
                collect()?;
            }
            Ok(9)
        }
        let mut instance = crate::Instance::new(DESCRIPTORS, POINTERS);
        instance.set_stress(true);
        assert_eq!(unsafe { instance.run_baml(entry) }.unwrap(), 7);
        assert_eq!(instance.tasks.task_count(), 0);
        for _ in 0..5 {
            assert_eq!(unsafe { instance.run_baml(next) }.unwrap(), 9);
        }
        let error = instance.take_unhandled().unwrap();
        assert_eq!(error.class, "baml.panics.IntegerOverflow");
        unsafe {
            assert_eq!((*(error.value as *const Header)).desc, 1);
        }
        assert!(instance.take_unhandled().is_none());
    }

    unsafe fn child(env: usize, _: usize, _: &[Dyn]) -> R<Dyn> {
        unsafe {
            (&mut *crate::TASK.get())
                .sleep(std::time::Duration::ZERO)
                .map_err(loop_error)?;
            let cell = crate::closures::capture(env, 0);
            Ok(Dyn {
                tag: 0,
                payload: crate::closures::load(cell).payload + 1,
            })
        }
    }
    fn job() -> R<i64> {
        unsafe {
            let cell = crate::closures::cell(10, Dyn { tag: 0, payload: 1 })?;
            let env = crate::closures::new(11, 123, child, &[cell])?;
            let f = spawn(12, env, 0, 0, SpawnOptions::default(), None)?;
            crate::closures::store(
                cell,
                Dyn {
                    tag: 0,
                    payload: 41,
                },
            );
            assert_eq!(await_future(f)?.payload, 42);
            let f = spawn(12, env, 0, 0, SpawnOptions::default(), None)?;
            assert!(cancel(f));
            assert!(!cancel(f));
            assert_eq!(await_future(f).unwrap_err().class, "baml.panics.Cancelled");
            Ok(42)
        }
    }
    #[test]
    fn private_environment_is_copied_traced_and_released_after_settlement() {
        struct OnlyFuture(std::cell::UnsafeCell<usize>);
        impl baml_rt_gc::RootSource for OnlyFuture {
            fn conservative_ranges(&self, _: &mut dyn FnMut(*const usize, *const usize)) {}
            fn precise_roots(&self, visit: &mut dyn FnMut(*mut usize)) {
                visit(self.0.get());
            }
        }
        fn entry() -> R<i64> {
            static META: crate::closures::PrivateMetadata =
                crate::closures::PrivateMetadata::new(123, child, 1);
            unsafe {
                let cell = crate::closures::cell(10, Dyn { tag: 0, payload: 1 })?;
                let mut env = crate::closures::PrivateEnvironment::new(&META, 13, [cell]);
                let future = spawn(12, env.as_raw(), 0, 0, SpawnOptions::default(), None)?;
                env = crate::closures::PrivateEnvironment::empty();
                std::hint::black_box(&env);
                let copied = (*(future as *const Future)).environment;
                assert!(crate::closures::is_private(copied));
                assert_eq!(crate::closures::capture(copied, 0), cell);
                // Ignore all stacks: only the future's precise trailing slots
                // can keep the captured cell alive during this collection.
                (&mut *crate::HEAP.get()).collect(&OnlyFuture(std::cell::UnsafeCell::new(future)));
                // A dead object can retain its start bit until lazy sweeping.
                // Verify the cell's mark from that collection without adding
                // it to the collection's roots.
                (&*crate::HEAP.get()).verify(&OnlyFuture(std::cell::UnsafeCell::new(cell)));
                assert_eq!((&*crate::HEAP.get()).object_start(cell), Some(cell));
                crate::closures::store(
                    cell,
                    Dyn {
                        tag: 0,
                        payload: 41,
                    },
                );
                assert_eq!(await_future(future)?.payload, 42);
                let tail = (future + FUTURE_BYTES) as *const Dyn;
                assert_eq!((*tail).payload, 0);
                assert_eq!((*tail.add(1)).payload, 0);
                Ok(42)
            }
        }
        let mut instance = crate::Instance::new(DESCRIPTORS, POINTERS);
        instance.set_stress(true);
        assert_eq!(unsafe { instance.run_baml(entry) }.unwrap(), 42);
    }

    #[test]
    fn managed_await_restores_fiber_bindings_and_roots_across_gc_and_sleeps() {
        let mut instance = crate::Instance::new(DESCRIPTORS, POINTERS);
        instance.set_stress(true);
        for _ in 0..10 {
            assert_eq!(unsafe { instance.run_baml(job) }.unwrap(), 42);
        }
    }
}

#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct FutureSlot {
    pointer: usize,
    owner: std::marker::PhantomData<std::rc::Rc<()>>,
}
impl FutureSlot {
    /// # Safety
    /// The pointer names a live managed future in the active instance.
    pub unsafe fn from_raw(pointer: usize) -> Self {
        Self {
            pointer,
            owner: std::marker::PhantomData,
        }
    }
    pub fn as_raw(self) -> usize {
        self.pointer
    }
}
impl crate::lists::sealed::Sealed for FutureSlot {}
impl crate::lists::Slot for FutureSlot {
    const POINTER: bool = true;
    fn word(self) -> usize {
        self.pointer
    }
    unsafe fn from_word(pointer: usize) -> Self {
        unsafe { Self::from_raw(pointer) }
    }
}

/// A block-scoped shield restores cancellation delivery on jumps and unwind.
pub struct Shield(*mut TaskCx<'static>);
impl Drop for Shield {
    fn drop(&mut self) {
        unsafe {
            (&mut *self.0).leave_shield();
        }
    }
}
/// # Safety
/// Drop on the same running fiber, before its runtime binding ends.
pub unsafe fn shield() -> Shield {
    let task = crate::TASK.get();
    unsafe {
        (&mut *task).enter_shield();
    }
    Shield(task)
}
/// # Safety
/// The pointer names a live future in the active heap.
pub unsafe fn state(pointer: usize) -> u32 {
    let f = unsafe { &*(pointer as *const Future) };
    if !f.error.is_null() {
        return if unsafe { &*f.error }.class == "baml.panics.Cancelled" {
            3
        } else {
            2
        };
    }
    if f.result.tag != u64::MAX { 1 } else { 0 }
}
/// Select without observing the chosen future's result.
/// # Safety
/// Each pointer names a rooted future in this loop.
pub unsafe fn await_any(pointers: &[usize]) -> R<i64> {
    unsafe {
        (&mut *crate::TASK.get())
            .check_cancel()
            .map_err(loop_error)?;
        for (i, &p) in pointers.iter().enumerate() {
            if state(p) != 0 {
                return Ok(i as i64);
            }
        }
        let tasks = pointers
            .iter()
            .map(|&p| (*(p as *const Future)).task)
            .collect::<Vec<_>>();
        crate::telemetry::before_yield();
        // The first settled input in input order, including a panicked one.
        loop {
            if let Some(i) = pointers
                .iter()
                .position(|&p| (*(p as *const Future)).settled())
            {
                return Ok(i as i64);
            }
            (&mut *crate::TASK.get())
                .await_any_task(&tasks)
                .map_err(loop_error)?;
        }
    }
}

pub type Cleanup = unsafe fn(u32, usize) -> R<()>;
thread_local! {pub(crate) static CLEANUP:Cell<Option<Cleanup>>=const {Cell::new(None)};}
/// # Safety
/// The dispatcher belongs to the active generated program for this root run.
pub unsafe fn set_cleanup_dispatch(callback: Cleanup) {
    CLEANUP.set(Some(callback));
}
/// # Safety
/// The pointer names a class instance in the active heap.
pub unsafe fn cleanup_begin(object: usize) -> bool {
    unsafe { (&mut *crate::HEAP.get()).begin_cleanup(object) }
}
/// Explicit collection waits for every callback found by this collection,
/// including callbacks that allocate or suspend.
/// # Safety
/// Managed roots obey the ordinary safepoint contract.
pub unsafe fn collect() -> R<()> {
    crate::telemetry::before_yield();
    crate::telemetry::sysop(|| unsafe {
        (&mut *crate::TASK.get())
            .check_cancel()
            .map_err(loop_error)?;
        (&mut *crate::HEAP.get()).collect(&crate::Roots(&*crate::ROOTS.get()));
        drain_finalizers();
        Ok(())
    })
}

// Duration arithmetic stays exact. The managed value contains inline limbs;
// temporary num_bigint values own no managed pointers and cannot outlive calls.
pub const fn bigint_descriptor(type_id: u32) -> Descriptor {
    Descriptor {
        kind: Kind::Opaque,
        size: 8,
        ptr_slots: (0, 0),
        tagged_slots: (0, 0),
        elem: Some(baml_rt_gc::Elements {
            offset: 24,
            layout: baml_rt_gc::ElementLayout::Scalar,
        }),
        type_id,
        fields: &[],
        finalizer: None,
    }
}
/// # Safety
/// `desc` is an opaque bigint descriptor in the active heap.
pub unsafe fn bigint(desc: u32, value: &num_bigint::BigInt) -> R<usize> {
    let (sign, limbs) = value.to_u32_digits();
    let bytes = 24 + (limbs.len() * 4).next_multiple_of(8);
    let p = unsafe { crate::try_alloc(desc, bytes)? };
    unsafe {
        p.add(8).cast::<i64>().write(match sign {
            num_bigint::Sign::Minus => -1,
            num_bigint::Sign::NoSign => 0,
            num_bigint::Sign::Plus => 1,
        });
        p.add(16).cast::<usize>().write(limbs.len());
        std::ptr::copy_nonoverlapping(limbs.as_ptr(), p.add(24).cast(), limbs.len());
    }
    Ok(p as usize)
}
/// # Safety
/// The pointer names a live managed bigint.
pub unsafe fn bigint_value(pointer: usize) -> num_bigint::BigInt {
    unsafe {
        let p = pointer as *const u8;
        let sign = match p.add(8).cast::<i64>().read() {
            -1 => num_bigint::Sign::Minus,
            0 => num_bigint::Sign::NoSign,
            _ => num_bigint::Sign::Plus,
        };
        let len = p.add(16).cast::<usize>().read();
        num_bigint::BigInt::new(
            sign,
            std::slice::from_raw_parts(p.add(24).cast(), len).to_vec(),
        )
    }
}
/// # Safety
/// The pointer names a live managed bigint; roots follow the task ABI.
pub unsafe fn sleep(nanos: usize) -> R<()> {
    use num_traits::ToPrimitive;
    let nanos = unsafe { bigint_value(nanos) };
    let duration = if nanos.sign() == num_bigint::Sign::Minus {
        std::time::Duration::ZERO
    } else {
        std::time::Duration::from_nanos(nanos.to_u64().unwrap_or(u64::MAX))
    };
    crate::telemetry::before_yield();
    crate::telemetry::sysop(|| {
        unsafe { (&mut *crate::TASK.get()).sleep(duration) }.map_err(loop_error)
    })
}

pub fn integer(value: i64) -> num_bigint::BigInt {
    num_bigint::BigInt::from(value)
}
/// # Safety
/// The descriptor is a bigint layout and text is a compiler-produced integer.
pub unsafe fn bigint_text(desc: u32, text: &str) -> R<usize> {
    unsafe {
        bigint(
            desc,
            &text.parse::<num_bigint::BigInt>().expect("integer literal"),
        )
    }
}

pub const RESOURCE_FINALIZER: u32 = u32::MAX - 1;
#[derive(Clone)]
enum Resource {
    Limit(baml_rt_tasks::Gate),
    Cancel(baml_rt_tasks::TaskCancel),
}
/// # Safety
/// The descriptor is the generated one-word native resource class.
pub unsafe fn limit_new(desc: u32, invalid_argument: u32, capacity: i64) -> R<usize> {
    if capacity <= 0 {
        return Err(crate::language::panic_value(
            invalid_argument,
            "baml.errors.InvalidArgument",
            format!("baml.spawn.Limit.new: capacity must be positive, got {capacity}"),
        ));
    }
    unsafe {
        let p = crate::try_alloc(desc, 16)? as usize;
        (&mut *FINALIZERS.get()).resources.insert(
            p,
            Resource::Limit(baml_rt_tasks::Gate::new(capacity as usize)),
        );
        Ok(p)
    }
}
/// # Safety
/// Inputs name live CancelToken objects, and desc is a CancelToken class.
pub unsafe fn token_new(desc: u32, inputs: &[usize]) -> R<usize> {
    unsafe {
        let linked = inputs
            .iter()
            .map(|p| match (&*FINALIZERS.get()).resources.get(p).unwrap() {
                Resource::Cancel(token) => token.clone(),
                _ => unreachable!(),
            })
            .collect();
        let token = (&*crate::TASK.get()).new_cancel_token(linked);
        let p = crate::try_alloc(desc, 16)? as usize;
        (&mut *FINALIZERS.get())
            .resources
            .insert(p, Resource::Cancel(token));
        Ok(p)
    }
}
/// # Safety
/// The object is a live CancelToken.
pub unsafe fn token_cancel(object: usize) -> i64 {
    unsafe {
        match &(&*FINALIZERS.get()).resources[&object] {
            Resource::Cancel(token) => {
                let was = token.is_cancelled();
                token.cancel();
                i64::from(!was)
            }
            _ => unreachable!(),
        }
    }
}
/// # Safety
/// The object is a live CancelToken.
pub unsafe fn token_is_cancelled(object: usize) -> bool {
    unsafe {
        match &(&*FINALIZERS.get()).resources[&object] {
            Resource::Cancel(token) => token.is_cancelled(),
            _ => unreachable!(),
        }
    }
}
/// # Safety
/// The object is a live Limit.
pub unsafe fn limit_counts(object: usize) -> (i64, i64) {
    unsafe {
        match &(&*FINALIZERS.get()).resources[&object] {
            Resource::Limit(gate) => {
                let (active, queued) = gate.counts();
                (active as i64, queued as i64)
            }
            _ => unreachable!(),
        }
    }
}
/// # Safety
/// All objects are live native resources in the active heap.
pub unsafe fn spawn_options(detached: bool, limits: &[usize], tokens: &[usize]) -> SpawnOptions {
    unsafe {
        let resources = &(&*FINALIZERS.get()).resources;
        SpawnOptions {
            detached,
            gates: limits
                .iter()
                .map(|p| match &resources[p] {
                    Resource::Limit(gate) => gate.clone(),
                    _ => unreachable!(),
                })
                .collect(),
            local_linked: tokens
                .iter()
                .map(|p| match &resources[p] {
                    Resource::Cancel(token) => token.clone(),
                    _ => unreachable!(),
                })
                .collect(),
            ..Default::default()
        }
    }
}

/// # Safety
/// The pointer names a live fixed-layout pointer array.
pub unsafe fn pointer_list<'a>(pointer: usize) -> &'a [usize] {
    unsafe {
        let length = *((pointer + 8) as *const usize);
        std::slice::from_raw_parts((pointer + 16) as *const usize, length)
    }
}
/// # Safety
/// Inputs are rooted fixed pointer arrays and desc is their common layout.
pub unsafe fn concat_pointers(desc: u32, a: usize, b: usize) -> R<usize> {
    unsafe {
        let a = pointer_list(a);
        let b = pointer_list(b);
        let len = a.len() + b.len();
        let p = crate::try_alloc(desc, 16 + len * 8)?;
        p.add(8).cast::<usize>().write(len);
        for (i, &value) in a.iter().chain(b).enumerate() {
            baml_rt_gc::write_ptr(p, p.add(16 + i * 8).cast(), value);
        }
        Ok(p as usize)
    }
}

/// Run an automatic finalizer on an independently named, shielded root fiber.
/// # Safety
/// The object and closure belong to the active heap and generated program.
pub unsafe fn run_cleanup(
    site: usize,
    object: usize,
    call: impl FnOnce() -> R<()> + 'static,
) -> R<()> {
    let telemetry = crate::telemetry::Task::root(site);
    let handle = unsafe {
        (&mut *crate::TASK.get()).spawn_with(
            SpawnOptions {
                detached: true,
                managed_roots: [object, 0],
                ..Default::default()
            },
            move |cx| {
                let mut binding = unsafe { FiberBinding::new(cx, ptr::null_mut()) };
                binding.telemetry = telemetry.as_ref().map(|task| task.fiber.clone());
                unsafe {
                    binding.install();
                }
                if let Some(task) = &telemetry {
                    task.announce();
                }
                let _shield = unsafe { shield() };
                let mut result = call();
                if let Err(error) = &mut result {
                    crate::telemetry::evidence::unhandled(error);
                }
                if let Some(task) = &telemetry {
                    task.finish(&result);
                }
                result
                    .map(|_| Value::int(0))
                    .map_err(|error| baml_rt_tasks::Err::Error {
                        message: error.message,
                        trace: error.frames.iter().map(|f| f.function.to_owned()).collect(),
                    })
            },
        )
    };
    crate::telemetry::before_yield();
    unsafe { (&mut *crate::TASK.get()).await_future(&handle) }
        .map(|_| ())
        .map_err(loop_error)
}

/// # Safety
/// The object is a live Limit resource in this instance.
pub unsafe fn limit_capacity(object: usize) -> i64 {
    unsafe {
        match &(&*FINALIZERS.get()).resources[&object] {
            Resource::Limit(gate) => gate.capacity() as i64,
            _ => unreachable!(),
        }
    }
}
/// # Safety
/// The descriptor is a bigint and dividend obeys the tagged-slot ABI.
pub unsafe fn divide_bigint(
    desc: u32,
    left: &num_bigint::BigInt,
    right: &num_bigint::BigInt,
    dividend: Dyn,
) -> R<usize> {
    if right.sign() == num_bigint::Sign::NoSign {
        let p = unsafe { crate::try_alloc(9, 24)? };
        unsafe {
            crate::write_dyn(p, p.add(8).cast(), dividend);
        }
        return Err(Box::new(Error {
            telemetry_raise: None,
            telemetry_at: None,
            telemetry_caught: None,
            value: p as usize,
            class: "baml.panics.DivisionByZero",
            message: format!("division by zero: {left} / 0"),
            frames: vec![],
            display_value: Some(format!("baml.panics.DivisionByZero {{dividend: {left}n}}")),
        }));
    }
    unsafe { bigint(desc, &(left / right)) }
}
