//! The stack a VM runs its compiled bodies on.
//!
//! A compiled body is plain Rust, so it cannot hand the thread back to the
//! engine from the middle of its call chain the way bytecode does between
//! instructions. Running every compiled body on a fiber makes that possible:
//! to pause, [`BexVm::suspend`] switches back to the VM's stack with a
//! [`Request`], the VM carries it out exactly as it would for bytecode (parking
//! the frame, yielding to the engine, collecting, cancelling), and the next
//! `resume` switches back with the outcome. The Rust frames wait on the fiber;
//! everything the collector must see is in the VM's frame slots, which is what
//! generated code spills to before it suspends.
//!
//! The VM loop stays the only driver. Compiled code calls bytecode by
//! suspending with [`Step::Call`]; if that callee calls compiled code in turn,
//! the VM resumes the fiber with [`Message::Enter`] and the new body runs
//! below the waiting one, so nesting on the fiber always mirrors the VM's
//! frame stack.
//!
//! Every VM that runs compiled code has its own fiber, and a program that
//! spawns many short tasks makes many VMs. Their stacks come from a process-wide
//! pool: a fiber dropped with no body on it hands its stack back, and the next
//! fiber takes it instead of mapping a fresh one.
//!
//! Compiled bodies are packed for native hosts only. Targets without stack
//! switching (wasm, Windows on aarch64) build the VM with a fiber that is never
//! entered.

use std::ptr::NonNull;

use bex_vm_types::Value;
use smallvec::SmallVec;

use super::{CompiledFn, Step};
use crate::{errors::VmError, vm::BexVm};

/// What the VM sends into the fiber. The VM pointer is passed on every message
/// because the fiber only touches the VM while the VM is driving it.
#[cfg_attr(
    any(target_arch = "wasm32", all(windows, target_arch = "aarch64")),
    allow(dead_code)
)]
pub(crate) enum Message {
    /// Run `body` for the frame the VM just pushed.
    Enter {
        vm: NonNull<BexVm>,
        body: CompiledFn,
        args: SmallVec<[Value; 4]>,
    },
    /// The outcome of the [`Step`] the innermost body is waiting on.
    Resume {
        vm: NonNull<BexVm>,
        outcome: Result<Value, Box<VmError>>,
    },
    /// Leave the fiber. Sent only with no body on it, so the coroutine returns
    /// and its stack can be reused.
    Stop,
}

/// What the fiber sends back to the VM, always about the innermost body.
#[cfg_attr(
    any(target_arch = "wasm32", all(windows, target_arch = "aarch64")),
    allow(dead_code)
)]
pub(crate) enum Request {
    /// Do this, then resume with its outcome.
    Step(Step),
    /// The body returned.
    Done(Result<Value, Box<VmError>>),
}

impl BexVm {
    /// Drive the fiber with `message` until it suspends or its body returns.
    pub(crate) fn fiber_resume(&mut self, message: Message) -> Request {
        let fiber = self.compiled.fiber.get_or_insert_with(Fiber::new);
        let request = fiber.resume(message);
        self.compiled.on_fiber = false;
        request
    }

    pub(crate) fn enter_compiled(
        &mut self,
        body: CompiledFn,
        args: SmallVec<[Value; 4]>,
    ) -> Request {
        let vm = NonNull::from(&mut *self);
        self.fiber_resume(Message::Enter { vm, body, args })
    }

    pub(crate) fn resume_body(&mut self, outcome: Result<Value, Box<VmError>>) -> Request {
        let vm = NonNull::from(&mut *self);
        self.fiber_resume(Message::Resume { vm, outcome })
    }
}

#[cfg(not(any(target_arch = "wasm32", all(windows, target_arch = "aarch64"))))]
pub(crate) use stackful::{Fiber, YielderPtr};
#[cfg(all(
    test,
    not(any(target_arch = "wasm32", all(windows, target_arch = "aarch64")))
))]
pub(crate) use stackful::{POOL_LIMIT, StackPool};
#[cfg(any(target_arch = "wasm32", all(windows, target_arch = "aarch64")))]
pub(crate) use stackless::{Fiber, YielderPtr};

#[cfg(not(any(target_arch = "wasm32", all(windows, target_arch = "aarch64"))))]
mod stackful {
    use std::{
        ptr::NonNull,
        sync::{Mutex, PoisonError},
    };

    use bex_vm_types::Value;
    use corosensei::{
        Coroutine, CoroutineResult, Yielder,
        stack::{DefaultStack, Stack as _},
    };

    use super::{Message, Request};
    use crate::{
        compiled_rt::{CompiledFn, Step},
        errors::{VmError, VmPanic},
        vm::BexVm,
    };

    /// Address space per fiber; the OS commits pages as they are touched. The
    /// VM's frame limit bounds how much is ever used.
    const STACK_SIZE: usize = 4 << 20;

    /// Fiber stack a compiled callee must still have below it: its builtins
    /// and unwinding.
    const STACK_RED_ZONE: usize = 256 << 10;

    /// Idle stacks kept mapped; any beyond this many are unmapped.
    pub(crate) const POOL_LIMIT: usize = 64;

    /// Stacks of fibers dropped with no body on them, for the fibers that
    /// come next. Mapping and unmapping a stack per VM is measurable system
    /// time in a program that spawns many short tasks.
    ///
    /// A pooled stack holds no frames: the fiber that returns it has made its
    /// coroutine return first (see [`Fiber::drop`]). The pages keep whatever
    /// the last fiber wrote, which the next coroutine overwrites as it runs.
    pub(crate) struct StackPool(Mutex<Vec<DefaultStack>>);

    static POOL: StackPool = StackPool::new();

    impl StackPool {
        pub(crate) const fn new() -> Self {
            Self(Mutex::new(Vec::new()))
        }

        /// A pooled stack, or a freshly mapped one.
        fn take(&self) -> DefaultStack {
            let pooled = self.0.lock().unwrap_or_else(PoisonError::into_inner).pop();
            pooled.unwrap_or_else(|| DefaultStack::new(STACK_SIZE).expect("map a fiber stack"))
        }

        /// Keep `stack` for the next fiber, or unmap it when the pool is full.
        fn put(&self, stack: DefaultStack) {
            let mut stacks = self.0.lock().unwrap_or_else(PoisonError::into_inner);
            if stacks.len() < POOL_LIMIT {
                stacks.push(stack);
            }
            // Otherwise `stack` is unmapped on return, after the lock is released.
        }

        #[cfg(test)]
        pub(crate) fn len(&self) -> usize {
            self.0.lock().unwrap_or_else(PoisonError::into_inner).len()
        }
    }

    pub(crate) struct Fiber {
        /// Taken only by `drop`, which needs the coroutine by value.
        coroutine: Option<Coroutine<Message, Request, ()>>,
        /// Lowest address a compiled callee may start at; see [`BexVm::stack_room`].
        floor: usize,
        /// Bodies running or waiting on the fiber. Each [`Message::Enter`]
        /// starts one and each [`Request::Done`] ends one, so at zero the
        /// coroutine is parked in its top-level loop (or has never started)
        /// and the stack holds no frame of ours.
        bodies: usize,
        /// Where the stack goes when the fiber is dropped idle.
        pool: &'static StackPool,
    }

    // SAFETY: while suspended, the fiber's stack holds only frames of compiled
    // bodies, VM code and builtins, whose locals are `Value`s and other `Send`
    // data; nothing thread-bound is held across `suspend`. The VM itself moves
    // between threads between `exec` calls, and the fiber moves with it.
    #[allow(unsafe_code)]
    unsafe impl Send for Fiber {}

    impl Fiber {
        pub(crate) fn new() -> Self {
            Self::with_pool(&POOL)
        }

        /// A fiber whose stack comes from, and returns to, `pool`.
        pub(crate) fn with_pool(pool: &'static StackPool) -> Self {
            let stack = pool.take();
            let floor = stack.limit().get() + STACK_RED_ZONE;
            let coroutine = Coroutine::with_stack(stack, |yielder, mut message| {
                loop {
                    let (vm, body, args) = match message {
                        Message::Enter { vm, body, args } => (vm, body, args),
                        Message::Stop => return,
                        Message::Resume { .. } => {
                            unreachable!("a fiber with no body running is resumed with one to run")
                        }
                    };
                    message = yielder.suspend(Request::Done(run(yielder, vm, body, &args)));
                }
            });
            Self {
                coroutine: Some(coroutine),
                floor,
                bodies: 0,
                pool,
            }
        }

        pub(crate) fn resume(&mut self, message: Message) -> Request {
            debug_assert!(
                !matches!(message, Message::Stop),
                "only `drop` stops a fiber"
            );
            if matches!(message, Message::Enter { .. }) {
                self.bodies += 1;
            }
            let coroutine = self
                .coroutine
                .as_mut()
                .expect("a fiber keeps its coroutine until it is dropped");
            match coroutine.resume(message) {
                CoroutineResult::Yield(request) => {
                    if matches!(request, Request::Done(_)) {
                        self.bodies -= 1;
                    }
                    request
                }
                CoroutineResult::Return(()) => unreachable!("a fiber returns only when stopped"),
            }
        }

        #[cfg(test)]
        pub(crate) fn bodies(&self) -> usize {
            self.bodies
        }

        #[cfg(test)]
        pub(crate) fn floor(&self) -> usize {
            self.floor
        }
    }

    impl Drop for Fiber {
        fn drop(&mut self) {
            let Some(mut coroutine) = self.coroutine.take() else {
                return;
            };
            // A waiting body keeps frames on the stack: dropping the coroutine
            // unwinds them, and the stack is unmapped with it. A panic in
            // flight gets the same, with no stack switch during unwinding.
            if self.bodies != 0 || std::thread::panicking() {
                return;
            }
            // The coroutine is parked in its top-level loop, or has never run:
            // either way `Stop` makes it return at once, and a returned
            // coroutine gives up its stack with nothing on it.
            match coroutine.resume(Message::Stop) {
                CoroutineResult::Return(()) => {}
                CoroutineResult::Yield(_) => unreachable!("a stopped fiber returns"),
            }
            self.pool.put(coroutine.into_stack());
        }
    }

    /// Run `body` on the fiber with the VM driving.
    #[allow(unsafe_code)]
    fn run(
        yielder: &Yielder<Message, Request>,
        vm: NonNull<BexVm>,
        body: CompiledFn,
        args: &[Value],
    ) -> Result<Value, Box<VmError>> {
        // SAFETY: the VM is parked in `Fiber::resume` and touches nothing until
        // the fiber suspends; its address is stable while its fiber exists.
        let vm = unsafe { &mut *vm.as_ptr() };
        vm.compiled.yielder = Some(YielderPtr(yielder.into()));
        vm.compiled.on_fiber = true;
        body(vm, args)
    }

    /// The running fiber's yielder, reachable from the VM so [`BexVm::suspend`]
    /// can switch out from any depth.
    #[derive(Clone, Copy)]
    pub(crate) struct YielderPtr(NonNull<Yielder<Message, Request>>);

    // SAFETY: the pointer is only dereferenced by code running on the fiber it
    // belongs to, on whichever thread is driving that fiber.
    #[allow(unsafe_code)]
    unsafe impl Send for YielderPtr {}

    #[inline]
    fn stack_address() -> usize {
        let marker = 0u8;
        std::ptr::from_ref(&marker) as usize
    }

    impl BexVm {
        /// Hand `step` to the VM and continue with its outcome once the VM
        /// resumes this body. Heap values held in Rust locals across this call
        /// are stale afterwards: generated code keeps them in frame slots and
        /// reloads them.
        #[allow(unsafe_code)]
        pub fn suspend(&mut self, step: Step) -> Result<Value, Box<VmError>> {
            let yielder = self
                .compiled
                .yielder
                .expect("compiled code runs on its VM's fiber");
            let mut request = Request::Step(step);
            loop {
                self.compiled.on_fiber = false;
                // SAFETY: a yielder lives as long as its fiber, and this body is on it.
                let message = unsafe { yielder.0.as_ref() }.suspend(request);
                match message {
                    Message::Resume { vm, outcome } => {
                        // SAFETY: as in `run`.
                        unsafe { &mut *vm.as_ptr() }.compiled.on_fiber = true;
                        return outcome;
                    }
                    Message::Enter { vm, body, args } => {
                        // SAFETY: see above.
                        let yielder = unsafe { yielder.0.as_ref() };
                        request = Request::Done(run(yielder, vm, body, &args));
                    }
                    Message::Stop => unreachable!("a fiber is stopped only with no body on it"),
                }
            }
        }

        /// Whether the fiber has room for a compiled callee, or the error to raise.
        #[inline]
        pub(crate) fn stack_room(&mut self) -> Result<(), Box<VmError>> {
            let floor = self.compiled.fiber.as_ref().map_or(0, |fiber| fiber.floor);
            if stack_address() >= floor {
                return Ok(());
            }
            Err(Box::new(VmError::thrown_fresh(
                self.panic_to_exception_value(VmPanic::StackOverflow),
            )))
        }
    }
}

#[cfg(any(target_arch = "wasm32", all(windows, target_arch = "aarch64")))]
mod stackless {
    use bex_vm_types::Value;

    use super::{Message, Request};
    use crate::{compiled_rt::Step, errors::VmError, vm::BexVm};

    const NEVER: &str = "compiled bodies do not run on this target";

    pub(crate) struct Fiber;

    // The signatures match the stackful fiber's.
    #[allow(clippy::unused_self, clippy::unnecessary_wraps)]
    impl Fiber {
        pub(crate) fn new() -> Self {
            unreachable!("{NEVER}")
        }

        pub(crate) fn resume(&mut self, _: Message) -> Request {
            unreachable!("{NEVER}")
        }
    }

    #[derive(Clone, Copy)]
    pub(crate) struct YielderPtr;

    #[allow(clippy::unused_self, clippy::unnecessary_wraps)]
    impl BexVm {
        /// See the native implementation.
        pub fn suspend(&mut self, _: Step) -> Result<Value, Box<VmError>> {
            unreachable!("{NEVER}")
        }

        pub(crate) fn stack_room(&mut self) -> Result<(), Box<VmError>> {
            Ok(())
        }
    }
}
