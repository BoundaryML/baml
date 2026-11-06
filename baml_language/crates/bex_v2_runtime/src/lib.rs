//! Heap/task-loop integration for generated runtime-v2 code.
//!
//! One owner thread installs a heap and root task for the duration of a call.
//! No managed pointer may escape that call's owned result boundary.
pub mod language;
use bex_v2_heap::{Descriptor, Heap, RootSource};
use bex_v2_loop::{Config, TaskCx, TaskLoop, Value};
pub use language::*;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

struct Roots<'a>(&'a bex_v2_loop::IntegrationRoots);
impl RootSource for Roots<'_> {
    fn conservative_ranges(&self, visit: &mut dyn FnMut(*const usize, *const usize)) {
        self.0.with_roots(|roots| roots.conservative_ranges(visit));
    }
    fn precise_roots(&self, visit: &mut dyn FnMut(*mut usize)) {
        self.0.with_roots(|roots| roots.precise_roots(visit));
    }
}

thread_local! {
    static STACK_LIMIT: Cell<usize> = const { Cell::new(0) };
    static ROOTS: Cell<*const bex_v2_loop::IntegrationRoots> = const { Cell::new(std::ptr::null()) };
    static HEAP: Cell<*mut Heap> = const { Cell::new(std::ptr::null_mut()) };
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
                "v2 allocation outside a root task"
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
    let result = HEAP.with(|heap| {
        assert!(
            !heap.get().is_null() && !ROOTS.get().is_null(),
            "v2 allocation outside a root task"
        );
        unsafe { (&mut *heap.get()).try_alloc(desc, bytes, &Roots(&*ROOTS.get())) }
    });
    result.map_err(|failure| language::allocation_failure(failure.bytes))
}

/// # Safety
/// Only error constructors may consume the reserved block. No collection occurs.
unsafe fn emergency_alloc(desc: u32, bytes: usize) -> *mut u8 {
    unsafe { (&mut *HEAP.get()).alloc_emergency(desc, bytes) }
        .expect("v2 emergency exception reserve exhausted")
}

/// Poll the root task and run the heap's allocation-budget/stress safepoint.
///
/// # Safety
/// The same rooting contract as `alloc` applies. This phase-one adapter supports
/// a single root task; it does not expose child spawning or heap-valued results.
pub unsafe fn safepoint() -> Result<(), bex_v2_loop::Err> {
    HEAP.with(|heap| {
        TASK.with(|task| {
            assert!(
                !heap.get().is_null() && !task.get().is_null(),
                "v2 safepoint outside a root task"
            );
            // SAFETY: scoped TLS pointers remain valid throughout this root call.
            unsafe {
                (&mut *task.get()).poll()?;
                ROOTS.with(|roots| (&mut *heap.get()).safepoint(&Roots(&*roots.get())));
            }
            Ok(())
        })
    })
}

/// A reusable, single-owner program instance. Heap capacity survives jobs.
pub struct Instance {
    heap: Heap,
    tasks: TaskLoop,
    _io: tokio::runtime::Runtime,
}
impl Instance {
    pub fn new(descriptors: &'static [Descriptor], pointer_types: &'static [bool]) -> Self {
        assert!(
            descriptors
                .iter()
                .all(|descriptor| descriptor.finalizer.is_none()),
            "runtime v2 integration seam does not support finalizers"
        );
        let io = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .build()
            .unwrap();
        let mut heap = Heap::new(descriptors, pointer_types);
        heap.set_stress(baml_env::raw_var("BAML_GC_STRESS").is_some_and(|value| value == "1"));
        heap.reserve_emergency()
            .expect("cannot reserve v2 emergency exception storage");
        if let Some(limit) = baml_env::raw_var("BAML_V2_HEAP_LIMIT") {
            heap.set_capacity_limit(limit.parse().expect("BAML_V2_HEAP_LIMIT must be bytes"));
        }
        let tasks = TaskLoop::new(
            Config {
                stack_size: 8 << 20,
                ..Config::default()
            },
            io.handle().clone(),
            Rc::new(RefCell::new(())),
        );
        Self {
            heap,
            tasks,
            _io: io,
        }
    }

    /// Execute an integer-returning entry point as a root task.
    ///
    /// # Safety
    /// The entry must uphold `alloc`'s rooting and descriptor contract and must
    /// not retain managed pointers in Rust host storage after returning.
    pub unsafe fn run(
        &mut self,
        entry: fn() -> Result<i64, bex_v2_loop::Err>,
    ) -> Result<i64, bex_v2_loop::Err> {
        unsafe { self.execute(entry) }.and_then(|result| result)
    }
    /// Execute generated BAML and preserve its language error.
    ///
    /// # Safety
    /// The entry must uphold the same rooting contract as `run`. A returned
    /// error's managed `value` is unrooted: inspect it only while this instance
    /// remains alive and before any further managed allocation.
    pub unsafe fn run_baml(&mut self, entry: fn() -> R<i64>) -> R<i64> {
        unsafe { self.execute(entry) }.map_err(|error| Error::internal(format!("{error:?}")))?
    }
    unsafe fn execute<E: 'static>(
        &mut self,
        entry: fn() -> Result<i64, E>,
    ) -> Result<Result<i64, E>, bex_v2_loop::Err> {
        let result = Rc::new(RefCell::new(None));
        let output = result.clone();
        let heap = &mut self.heap as *mut Heap;
        let roots = self.tasks.integration_roots();
        let future = self.tasks.spawn(move |cx| {
            struct Binding(
                *mut Heap,
                *mut TaskCx<'static>,
                *const bex_v2_loop::IntegrationRoots,
                usize,
            );
            impl Drop for Binding {
                fn drop(&mut self) {
                    HEAP.set(self.0);
                    TASK.set(self.1);
                    ROOTS.set(self.2);
                    STACK_LIMIT.set(self.3);
                }
            }
            let previous_heap = HEAP.replace(heap);
            // Erase the borrowed task lifetime only inside this scoped binding.
            let current: *mut TaskCx<'_> = cx;
            let previous_task = TASK.replace(current.cast::<TaskCx<'static>>());
            let previous_roots = ROOTS.replace(&roots);
            let previous_limit = STACK_LIMIT.replace(roots.stack_limit(cx.id()));
            let _binding = Binding(previous_heap, previous_task, previous_roots, previous_limit);
            *output.borrow_mut() = Some(entry());
            Ok(Value::int(0))
        });
        self.tasks.run_until(&future)?;
        let result = result.borrow_mut().take().expect("root task completed");
        Ok(result)
    }

    pub fn set_heap_limit(&mut self, bytes: usize) {
        self.heap.set_capacity_limit(bytes);
    }
    pub fn stats(&self) -> bex_v2_heap::Stats {
        self.heap.stats()
    }
    pub fn set_stress(&mut self, enabled: bool) {
        self.heap.set_stress(enabled);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bex_v2_heap::{Header, Kind, write_ptr};

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

    unsafe fn tree(depth: usize) -> Result<*mut Node, bex_v2_loop::Err> {
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

    fn job() -> Result<i64, bex_v2_loop::Err> {
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
        fn fail() -> Result<i64, bex_v2_loop::Err> {
            Err(bex_v2_loop::Err::Error {
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
