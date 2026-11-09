//! Heap/task-loop integration for generated rt code.
//!
//! One owner thread installs a heap and root task for the duration of a call.
//! No managed pointer may escape that call's owned result boundary.
pub mod bigint;
pub mod bytes;
pub mod callbacks;
pub mod calls;
pub mod closures;
pub mod format;
pub mod globals;
pub mod json;
pub mod json_lists;
pub mod json_scalars;
pub mod json_tree;
pub mod language;
pub mod list_errors;
pub mod lists;
pub mod maps;
pub mod native;
pub mod regex;
pub mod regex_map;
pub mod strings;
pub mod tasks;
pub mod telemetry;
use baml_rt_gc::{Descriptor, Heap, RootSource};
use baml_rt_tasks::{Config, TaskCx, TaskLoop, Value};
pub use bex_vm_types::float_order::{cmp as float_cmp, eq as float_eq};
pub use bex_vm_types::format_float;
pub use language::*;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

type FunctionValues = std::collections::BTreeMap<usize, baml_rt_tasks::IntegrationRoot>;

struct Roots<'a>(&'a baml_rt_tasks::IntegrationRoots);
impl RootSource for Roots<'_> {
    fn conservative_ranges(&self, visit: &mut dyn FnMut(*const usize, *const usize)) {
        self.0.with_roots(|roots| roots.conservative_ranges(visit));
    }
    fn precise_roots(&self, visit: &mut dyn FnMut(*mut usize)) {
        self.0.with_roots(|roots| roots.precise_roots(visit));
        globals::CURRENT.with(|current| {
            if !current.get().is_null() {
                unsafe { (&*current.get()).roots(visit) };
            }
        });
    }
}

thread_local! {
    pub(crate) static ADMISSION_CANCELLED:Cell<bool>=const {Cell::new(false)};
    pub(crate) static ENTRY_ERROR:Cell<*mut Error>=const {Cell::new(std::ptr::null_mut())};
    static POLL_WORD: Cell<*const Cell<u8>> = const { Cell::new(std::ptr::null()) };
    static STACK_LIMIT: Cell<usize> = const { Cell::new(0) };
    static ROOTS: Cell<*const baml_rt_tasks::IntegrationRoots> = const { Cell::new(std::ptr::null()) };
    static HEAP: Cell<*mut Heap> = const { Cell::new(std::ptr::null_mut()) };
    static FUNCTIONS: Cell<*mut FunctionValues> = const { Cell::new(std::ptr::null_mut()) };
    static TASK: Cell<*mut TaskCx<'static>> = const { Cell::new(std::ptr::null_mut()) };
}

/// Allocate a zeroed object, possibly collecting with the loop's stack roots.
///
/// # Safety
/// Must run inside `Instance::run`. The descriptor must describe all pointer
/// fields, and all live managed pointers must be on the current stack or in
/// traced heap objects. The returned object must be initialized before another
/// collecting operation. Raw addresses must not escape the instance.
#[inline(always)]
pub unsafe fn alloc(desc: u32, bytes: usize) -> *mut u8 {
    HEAP.with(|heap| {
        TASK.with(|task| {
            assert!(
                !heap.get().is_null() && !task.get().is_null(),
                "rt allocation outside a root task"
            );
            // SAFETY: the scoped binding owns this heap exclusively on its thread.
            // Allocation visits roots synchronously and never resumes another task.
            unsafe {
                ROOTS.with(|roots| (&mut *heap.get()).alloc(desc, bytes, &Roots(&*roots.get())))
            }
        })
    })
}

/// Fallible language allocation with a reserved AllocFailure construction path.
/// # Safety
/// Same contract as `alloc`.
#[inline(always)]
pub unsafe fn try_alloc(desc: u32, bytes: usize) -> R<*mut u8> {
    // Keep the TLS getters small: a closure containing the whole allocation
    // path prevents LLVM from inlining LocalKey::with into string concat loops.
    let heap = HEAP.get();
    let roots = ROOTS.get();
    assert!(
        !heap.is_null() && !roots.is_null(),
        "rt allocation outside a root task"
    );
    let result = unsafe { (&mut *heap).try_alloc(desc, bytes, &Roots(&*roots)) };
    result.map_err(|failure| language::allocation_failure(failure.bytes))
}

/// Allocate a header-initialized class with uninitialized fields.
/// # Safety
/// Same rooting contract as `try_alloc`. The descriptor must be a class without
/// cleanup or variable elements, of exactly `bytes` bytes. Initialize every field
/// before reading it or performing another collecting operation.
#[inline(always)]
pub unsafe fn try_alloc_class(desc: u32, bytes: usize) -> R<*mut u8> {
    let heap = HEAP.get();
    let roots = ROOTS.get();
    assert!(
        !heap.is_null() && !roots.is_null(),
        "rt allocation outside a root task"
    );
    let result = unsafe { (&mut *heap).try_alloc_class(desc, bytes, &Roots(&*roots)) };
    result.map_err(|failure| language::allocation_failure(failure.bytes))
}

/// # Safety
/// Only error constructors may consume the reserved block. No collection occurs.
unsafe fn emergency_alloc(desc: u32, bytes: usize) -> *mut u8 {
    unsafe { (&mut *HEAP.get()).alloc_emergency(desc, bytes) }
        .expect("rt emergency exception reserve exhausted")
}

/// Poll the root task and run the heap's allocation-budget/stress safepoint.
///
/// # Safety
/// The same rooting contract as `alloc` applies. This adapter supports
/// a single root task; it does not expose child spawning or heap-valued results.
#[inline(always)]
pub unsafe fn safepoint() -> Result<(), baml_rt_tasks::Err> {
    let word = POLL_WORD.get();
    assert!(!word.is_null(), "rt safepoint outside a root task");
    if unsafe { (*word).get() } == 0 {
        return Ok(());
    }
    unsafe { safepoint_slow() }
}
#[cold]
#[inline(never)]
unsafe fn safepoint_slow() -> Result<(), baml_rt_tasks::Err> {
    HEAP.with(|heap| {
        TASK.with(|task| {
            assert!(
                !heap.get().is_null() && !task.get().is_null(),
                "rt safepoint outside a root task"
            );
            // SAFETY: scoped TLS pointers remain valid throughout this root call.
            unsafe {
                crate::telemetry::before_yield();
                (&mut *task.get()).poll()?;
                ROOTS.with(|roots| (&mut *heap.get()).safepoint(&Roots(&*roots.get())));
                tasks::drain_finalizers();
            }
            Ok(())
        })
    })
}

// Top-level function values are pooled by the VM. Preserve their identity
// (including aliases in telemetry captures) and trace the pool between jobs.
unsafe fn function_value(object: usize, create: impl FnOnce() -> R<usize>) -> R<usize> {
    let functions = FUNCTIONS.get();
    assert!(!functions.is_null(), "function value outside an instance");
    unsafe {
        if let Some(value) = (&*functions).get(&object) {
            return Ok(value.get());
        }
        let value = create()?;
        let root = (&*ROOTS.get()).root(value);
        (&mut *functions).insert(object, root);
        Ok(value)
    }
}

/// A reusable, single-owner program instance. Heap capacity survives jobs.
pub struct Instance {
    heap: Heap,
    globals: globals::Globals,
    tasks: TaskLoop,
    _io: tokio::runtime::Runtime,
    functions: FunctionValues,
    finalizers: tasks::NativeFinalizers,
    compact_strings: bool,
}
impl Instance {
    pub fn new(descriptors: &'static [Descriptor], pointer_types: &'static [bool]) -> Self {
        let io = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .build()
            .unwrap();
        let mut heap = Heap::new(descriptors, pointer_types);
        if btel_settings::mode::from_env().is_ok_and(|level| level.is_some()) {
            heap.set_collection_observer(Some(crate::telemetry::collection));
        }
        heap.set_stress(baml_env::raw_var("BAML_GC_STRESS").is_some_and(|value| value == "1"));
        heap.reserve_emergency()
            .expect("cannot reserve rt emergency exception storage");
        if let Some(limit) = baml_env::raw_var("BAML_RT_HEAP_LIMIT") {
            heap.set_capacity_limit(limit.parse().expect("BAML_RT_HEAP_LIMIT must be bytes"));
        }
        let tasks = TaskLoop::new(
            Config {
                stack_size: 8 << 20,
                ..Config::default()
            },
            io.handle().clone(),
            Rc::new(RefCell::new(())),
        );
        heap.bind_poll_word(tasks.integration_roots().poll_word());
        Self {
            heap,
            globals: globals::Globals::new(pointer_types),
            tasks,
            _io: io,
            functions: FunctionValues::new(),
            finalizers: tasks::NativeFinalizers::default(),
            compact_strings: strings::compact_descriptor(descriptors),
        }
    }

    /// Execute an integer-returning entry point as a root task.
    ///
    /// # Safety
    /// The entry must uphold `alloc`'s rooting and descriptor contract and must
    /// not retain managed pointers in Rust host storage after returning.
    pub unsafe fn run(
        &mut self,
        entry: fn() -> Result<i64, baml_rt_tasks::Err>,
    ) -> Result<i64, baml_rt_tasks::Err> {
        unsafe { self.execute(entry, |_| 0) }.and_then(|result| result)
    }
    /// Execute generated BAML and preserve its language error.
    ///
    /// # Safety
    /// The entry must uphold the same rooting contract as `run`. A returned
    /// error's managed `value` is unrooted: inspect it only while this instance
    /// remains alive and before any further managed allocation.
    pub unsafe fn run_baml(&mut self, entry: impl FnOnce() -> R<i64> + 'static) -> R<i64> {
        unsafe { self.run_baml_value(entry) }
    }
    /// Execute a root returning an owned host value.
    ///
    /// # Safety
    /// Captures and results obey `run_baml`'s managed-pointer rooting contract.
    pub unsafe fn run_baml_value<T: 'static>(
        &mut self,
        entry: impl FnOnce() -> R<T> + 'static,
    ) -> R<T> {
        unsafe { self.execute(entry, |error| error.value) }
            .map_err(|error| Error::internal(format!("{error:?}")))?
    }
    unsafe fn execute<T: 'static, E: 'static>(
        &mut self,
        entry: impl FnOnce() -> Result<T, E> + 'static,
        error_pointer: fn(&E) -> usize,
    ) -> Result<Result<T, E>, baml_rt_tasks::Err> {
        let result = Rc::new(RefCell::new(None));
        let output = result.clone();
        let heap = &mut self.heap as *mut Heap;
        let globals = &self.globals as *const globals::Globals;
        let roots = self.tasks.integration_roots();
        let poll_word = roots.poll_word();
        struct HostBinding(
            *mut Heap,
            *const baml_rt_tasks::IntegrationRoots,
            *const Cell<u8>,
            *mut FunctionValues,
            *mut tasks::NativeFinalizers,
            Option<tasks::Cleanup>,
            bool,
            *const globals::Globals,
        );
        impl Drop for HostBinding {
            fn drop(&mut self) {
                HEAP.set(self.0);
                ROOTS.set(self.1);
                POLL_WORD.set(self.2);
                FUNCTIONS.set(self.3);
                tasks::FINALIZERS.set(self.4);
                tasks::CLEANUP.set(self.5);
                strings::COMPACT_STRINGS.set(self.6);
                globals::CURRENT.set(self.7);
            }
        }
        let _binding = HostBinding(
            HEAP.replace(heap),
            ROOTS.replace(&roots),
            POLL_WORD.replace(Rc::as_ptr(&poll_word)),
            FUNCTIONS.replace(&mut self.functions),
            tasks::FINALIZERS.replace(&mut self.finalizers),
            tasks::CLEANUP.replace(None),
            strings::COMPACT_STRINGS.replace(self.compact_strings),
            globals::CURRENT.replace(globals),
        );
        let future = self.tasks.spawn(move |cx| {
            let telemetry = crate::telemetry::current_raw();
            let mut binding = unsafe { tasks::FiberBinding::new(cx, telemetry) };
            unsafe {
                binding.install();
            }
            let result = entry();
            let pointer = result.as_ref().err().map_or(0, error_pointer);
            let _error_root = unsafe { (&*ROOTS.get()).root(pointer) };
            if result.is_err() {
                cx.cancel().cancel();
            }
            unsafe {
                tasks::drain_finalizers();
            }
            *output.borrow_mut() = Some(result);
            Ok(Value::pointer(pointer))
        });
        let mut host_telemetry =
            unsafe { crate::telemetry::Fiber::current(crate::telemetry::current_raw()) };
        self.tasks.run_until(&future)?;
        // A CLI invocation owns its spawned work. Finish it while the producing
        // session and generated dispatch tables are still alive.
        loop {
            while self.tasks.task_count() != 0 {
                self.tasks.tick(true);
            }
            if !self.heap.has_pending_finalizers() {
                break;
            }
            let telemetry = crate::telemetry::current_raw() as usize;
            let maintenance = self.tasks.spawn(move |cx| {
                let mut binding = unsafe { tasks::FiberBinding::new(cx, telemetry as *mut ()) };
                unsafe {
                    binding.install();
                    tasks::drain_finalizers();
                }
                Ok(Value::int(0))
            });
            self.tasks.run_until(&maintenance)?;
        }
        unsafe {
            self.finalizers.finish_invocation();
        }
        let _ = self.tasks.take_unhandled();
        if let Some(host) = &mut host_telemetry {
            unsafe {
                host.enter();
            }
        }
        let result = result.borrow_mut().take().expect("root task completed");
        Ok(result)
    }

    /// Unawaited producer errors are process failures, after the root's own
    /// completion has been observed with its original outcome.
    pub fn take_unhandled_reports(&mut self) -> Vec<tasks::UnhandledTaskError> {
        self.finalizers.take_reports()
    }
    pub fn take_unhandled(&mut self) -> Option<Box<Error>> {
        self.finalizers.take_unhandled()
    }

    pub fn set_heap_limit(&mut self, bytes: usize) {
        self.heap.set_capacity_limit(bytes);
    }
    pub fn stats(&self) -> baml_rt_gc::Stats {
        self.heap.stats()
    }
    pub fn set_stress(&mut self, enabled: bool) {
        self.heap.set_stress(enabled);
    }
}

impl Drop for Instance {
    fn drop(&mut self) {
        self.tasks.shutdown(std::time::Duration::ZERO);
        unsafe {
            self.finalizers.destroy();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use baml_rt_gc::{Header, Kind, write_ptr};

    #[repr(C)]
    struct Node {
        header: Header,
        left: *mut Node,
        right: *mut Node,
        value: i64,
    }
    static DESCRIPTORS: [Descriptor; 1] = [Descriptor {
        kind: Kind::Class,
        size: std::mem::size_of::<Node>(),
        ptr_slots: (8, 2),
        tagged_slots: (0, 0),
        elem: None,
        type_id: 0,
        fields: &[],
        finalizer: None,
    }];

    unsafe fn tree(depth: usize) -> Result<*mut Node, baml_rt_tasks::Err> {
        // SAFETY: pointer locals are on the running fiber; child slots are
        // initialized using the write barrier before another allocation.
        unsafe {
            let left = if depth == 0 {
                std::ptr::null_mut()
            } else {
                tree(depth - 1)?
            };
            let right = if depth == 0 {
                std::ptr::null_mut()
            } else {
                tree(depth - 1)?
            };
            safepoint()?;
            let node = alloc(0, std::mem::size_of::<Node>()).cast::<Node>();
            write_ptr(node.cast(), (&raw mut (*node).left).cast(), left as usize);
            write_ptr(node.cast(), (&raw mut (*node).right).cast(), right as usize);
            (*node).value = depth as i64;
            Ok(node)
        }
    }

    unsafe fn check(node: *mut Node) -> i64 {
        // SAFETY: this walks a rooted, fully initialized tree.
        unsafe {
            if (*node).left.is_null() {
                assert!((*node).right.is_null());
                return 1;
            }
            assert_eq!((*(*node).left).value, (*node).value - 1);
            1 + check((*node).left) + check((*node).right)
        }
    }

    fn job() -> Result<i64, baml_rt_tasks::Err> {
        // SAFETY: both trees remain on the running stack until checked.
        unsafe {
            let retained = tree(6)?;
            let transient = tree(7)?;
            safepoint()?;
            Ok(check(retained) + check(transient))
        }
    }

    #[test]
    fn running_fiber_roots_survive_allocations_and_stress() {
        let mut instance = Instance::new(&DESCRIPTORS, &[true]);
        instance.set_stress(true);
        for _ in 0..3 {
            // SAFETY: job obeys the descriptor and stack-root contract.
            assert_eq!(unsafe { instance.run(job) }.unwrap(), 382);
        }
        assert!(instance.stats().collections >= 3 * 382);
    }

    #[test]
    fn task_failure_restores_tls_and_instance_remains_usable() {
        fn fail() -> Result<i64, baml_rt_tasks::Err> {
            Err(baml_rt_tasks::Err::Error {
                message: "probe".into(),
                trace: vec!["entry".into()],
            })
        }
        let mut instance = Instance::new(&DESCRIPTORS, &[true]);
        // SAFETY: fail allocates no managed objects and job obeys rooting.
        unsafe {
            assert_eq!(instance.run(fail), fail());
            assert!(HEAP.get().is_null());
            assert!(TASK.get().is_null());
            assert_eq!(instance.run(job).unwrap(), 382);
        }
    }
}
