//! Managed futures and owner-thread bindings for compiled task entry points.
use crate::{Dyn, Error, R};
use baml_rt_gc::{Descriptor, Header, Kind};
pub use baml_rt_tasks::SpawnOptions;
use baml_rt_tasks::{FutureHandle, SwitchHook, TaskCx, Value};
use std::{cell::Cell, mem::size_of, ptr};

pub const FUTURE_FINALIZER: u32 = u32::MAX;
#[repr(C)]
struct Future {
    header: Header,
    environment: usize,
    result: Dyn,
    error: *mut Error,
    handle: Option<FutureHandle>,
    registration: usize,
}
pub const fn future_descriptor(type_id: u32) -> Descriptor {
    Descriptor {
        kind: Kind::Future,
        size: size_of::<Future>(),
        ptr_slots: (8, 1),
        tagged_slots: (16, 1),
        elem: None,
        type_id,
        fields: &[],
        finalizer: Some(FUTURE_FINALIZER),
    }
}

// The table is deliberately NOT a root source. Objects own their native data;
// the GC callback clears its entry, including cycles of settled future values.
#[derive(Default)]
pub(crate) struct NativeFinalizers {
    entries: Vec<usize>,
    free: Vec<usize>,
}
impl NativeFinalizers {
    fn register(&mut self, pointer: usize) -> usize {
        if let Some(i) = self.free.pop() {
            self.entries[i] = pointer;
            i
        } else {
            let i = self.entries.len();
            self.entries.push(pointer);
            i
        }
    }
    fn release(&mut self, i: usize) {
        if self.entries[i] != 0 {
            self.entries[i] = 0;
            self.free.push(i);
        }
    }
    pub(crate) unsafe fn destroy(&mut self) {
        for pointer in self.entries.drain(..).filter(|p| *p != 0) {
            unsafe {
                let f = &mut *(pointer as *mut Future);
                f.handle.take();
                if !f.error.is_null() {
                    drop(Box::from_raw(f.error));
                    f.error = ptr::null_mut();
                }
            }
        }
        self.free.clear();
    }
}
thread_local! { pub(crate) static FINALIZERS: Cell<*mut NativeFinalizers> = const { Cell::new(ptr::null_mut()) }; }

unsafe fn release_registration(f: &mut Future) {
    if f.registration != usize::MAX {
        unsafe {
            (&mut *FINALIZERS.get()).release(f.registration);
        }
        f.registration = usize::MAX;
    }
}
pub(crate) unsafe fn finalize(pointer: usize) {
    unsafe {
        let f = &mut *(pointer as *mut Future);
        f.handle.take();
        if !f.error.is_null() {
            drop(Box::from_raw(f.error));
            f.error = ptr::null_mut();
        }
        release_registration(f);
    }
}
pub(crate) unsafe fn drain_finalizers() {
    let pending = unsafe { (&mut *crate::HEAP.get()).take_pending_finalizers() };
    for token in pending {
        assert_eq!(token.cleanup, FUTURE_FINALIZER, "unknown generated cleanup");
        unsafe {
            finalize(token.object() as usize);
            (&mut *crate::HEAP.get()).complete_finalizer(token);
        }
    }
}

pub(crate) struct FiberBinding<'a> {
    cx: *mut TaskCx<'a>,
    limit: usize,
    telemetry: *mut (),
    previous_task: *mut TaskCx<'static>,
    previous_limit: usize,
    previous_telemetry: *mut (),
}
impl<'a> FiberBinding<'a> {
    pub(crate) unsafe fn new(cx: &mut TaskCx<'a>, telemetry: *mut ()) -> Self {
        unsafe {
            Self {
                cx,
                limit: (&*crate::ROOTS.get()).stack_limit(cx.id()),
                telemetry,
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
        let b = &*pointer.cast::<FiberBinding<'_>>();
        crate::TASK.set(b.cx.cast());
        crate::STACK_LIMIT.set(b.limit);
        crate::telemetry::replace_current_raw(b.telemetry);
    }
}
unsafe fn leave(_: *mut ()) {
    crate::TASK.set(ptr::null_mut());
    crate::STACK_LIMIT.set(0);
    unsafe {
        crate::telemetry::replace_current_raw(ptr::null_mut());
    }
}
impl Drop for FiberBinding<'_> {
    fn drop(&mut self) {
        unsafe {
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
    mut options: SpawnOptions,
) -> R<usize> {
    unsafe {
        let pointer = crate::try_alloc(desc, size_of::<Future>())?.cast::<Future>();
        baml_rt_gc::write_ptr(pointer.cast(), &mut (*pointer).environment, environment);
        (*pointer).result = Dyn {
            tag: u64::MAX,
            payload: 0,
        };
        (*pointer).error = ptr::null_mut();
        ptr::write(&mut (*pointer).handle, None);
        (*pointer).registration = (&mut *FINALIZERS.get()).register(pointer as usize);
        options.managed_roots = [pointer as usize, 0];
        let handle = (&mut *crate::TASK.get()).spawn_managed(
            options,
            execute,
            [pointer as usize, site, 0, 0],
        );
        (*pointer).handle = Some(handle);
        Ok(pointer as usize)
    }
}
unsafe fn execute(cx: &mut TaskCx<'_>, data: [usize; 4]) -> baml_rt_tasks::Outcome {
    unsafe {
        let f = data[0] as *mut Future;
        let mut binding = FiberBinding::new(cx, ptr::null_mut());
        binding.install();
        let result = cx
            .admit()
            .map_err(loop_error)
            .and_then(|()| crate::closures::call((*f).environment, data[1], &[]));
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
                Err(outcome)
            }
        };
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
        if (*pointer).result.tag == u64::MAX && (*pointer).error.is_null() {
            let handle = (*pointer)
                .handle
                .as_ref()
                .expect("pending producer")
                .clone();
            crate::telemetry::before_yield();
            let result = (&mut *crate::TASK.get()).await_future(&handle);
            // Admission errors still pass through the managed entry wrapper.
            if let Err(error) = result {
                if (*pointer).error.is_null() {
                    return Err(loop_error(error));
                }
            }
        }
        if let Some(handle) = (*pointer).handle.take() {
            let _ = handle.result();
        }
        if !(*pointer).error.is_null() {
            return Err(Box::new((*(*pointer).error).clone()));
        }
        release_registration(&mut *pointer);
        Ok((*pointer).result)
    }
}

/// # Safety
/// The argument names a live future in the current heap.
pub unsafe fn cancel(pointer: usize) -> bool {
    unsafe {
        let f = &*(pointer as *const Future);
        if let Some(handle) = &f.handle {
            if matches!(handle.state(), baml_rt_tasks::FutureState::Pending) {
                handle.request_cancel();
                return true;
            }
        }
        false
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
    ];
    static POINTERS: &[bool] = &[
        false, false, false, true, true, true, true, true, true, true, true, true, true, true,
        true, true,
    ];
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
            let f = spawn(12, env, 0, SpawnOptions::default())?;
            crate::closures::store(
                cell,
                Dyn {
                    tag: 0,
                    payload: 41,
                },
            );
            assert_eq!(await_future(f)?.payload, 42);
            let f = spawn(12, env, 0, SpawnOptions::default())?;
            assert!(cancel(f));
            assert_eq!(await_future(f).unwrap_err().class, "baml.panics.Cancelled");
            Ok(42)
        }
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
