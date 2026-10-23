//! Compiled bodies run on a lazy, VM-owned fiber. Suspensions return control
//! to the VM driver; values surviving a suspension must live in GC-visible slots.
//! Nested bytecode-to-compiled calls enter below the waiting Rust frames.
//! wasm and Windows/aarch64 cannot enter compiled bodies.

use std::ptr::NonNull;

use bex_vm_types::Value;
use smallvec::SmallVec;

use super::{CompiledFn, Step};
use crate::{
    errors::{VmError, VmInternalError},
    vm::BexVm,
};

/// The VM cannot move while a body is parked: Rust frames retain its borrow.
/// Every resume supplies the same VM pointer; `suspend` checks this invariant.
#[cfg_attr(
    any(target_arch = "wasm32", all(windows, target_arch = "aarch64")),
    allow(dead_code)
)]
pub(crate) enum Message {
    Enter {
        vm: NonNull<BexVm>,
        body: CompiledFn,
        args: SmallVec<[Value; 4]>,
    },
    Resume {
        vm: NonNull<BexVm>,
        outcome: Result<Value, Box<VmError>>,
    },
    /// Leave the fiber. Sent only with no body on it, so the coroutine returns.
    Stop,
}

#[cfg_attr(
    any(target_arch = "wasm32", all(windows, target_arch = "aarch64")),
    allow(dead_code)
)]
pub(crate) enum Request {
    Step(Step),
    Done(Result<Value, Box<VmError>>),
}

struct FiberReset<'a>(&'a mut BexVm);

impl Drop for FiberReset<'_> {
    fn drop(&mut self) {
        self.0.compiled.on_fiber = false;
        self.0.compiled.yielder = None;
    }
}

impl BexVm {
    /// Move the fiber out before switching: its body must hold the only VM borrow.
    pub(crate) fn fiber_resume(
        &mut self,
        message: impl FnOnce(NonNull<BexVm>) -> Message,
    ) -> Request {
        debug_assert!(
            !self.compiled.on_fiber,
            "only the VM's own stack drives the fiber"
        );
        let mut fiber = match self.compiled.fiber.take() {
            Some(fiber) => fiber,
            None => match Fiber::new() {
                Ok(fiber) => {
                    self.compiled.fiber_floor = fiber.floor();
                    fiber
                }
                Err(error) => {
                    return Request::Done(Err(Box::new(
                        VmInternalError::CompiledBody {
                            message: format!("could not map a stack for compiled code: {error}"),
                        }
                        .into(),
                    )));
                }
            },
        };
        let reset = FiberReset(self);
        let request = fiber.resume(message(NonNull::from(&mut *reset.0)));
        drop(reset);
        self.compiled.fiber = Some(fiber);
        request
    }

    pub(crate) fn resume_body(&mut self, outcome: Result<Value, Box<VmError>>) -> Request {
        self.fiber_resume(|vm| Message::Resume { vm, outcome })
    }
}

#[cfg(not(any(target_arch = "wasm32", all(windows, target_arch = "aarch64"))))]
pub(crate) use stackful::{Fiber, YielderPtr};
#[cfg(any(target_arch = "wasm32", all(windows, target_arch = "aarch64")))]
pub(crate) use stackless::{Fiber, YielderPtr};

#[cfg(not(any(target_arch = "wasm32", all(windows, target_arch = "aarch64"))))]
mod stackful {
    use std::ptr::NonNull;

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

    // corosensei unwinds a fiber's stack to drop a parked coroutine and
    // forwards panics across the switch; neither works under `panic = "abort"`.
    #[cfg(panic = "abort")]
    compile_error!("compiled bodies run on corosensei fibers, which need `panic = \"unwind\"`");

    /// Virtual reservation; the OS commits pages on demand.
    const STACK_SIZE: usize = 4 << 20;

    /// Reserve room for builtins and unwinding below compiled callees.
    const STACK_RED_ZONE: usize = 256 << 10;

    pub(crate) struct Fiber {
        coroutine: Coroutine<Message, Request, ()>,
        floor: usize,
        /// Zero means no Rust body remains, so `Stop` can finish without unwinding.
        bodies: usize,
    }

    // SAFETY: a fiber is only resumed by the thread that owns its VM, and
    // moves between threads with the VM between `exec` calls. That is sound
    // only because nothing on a parked fiber's stack is thread-bound: the
    // frames there are generated bodies, VM code and non-yielding builtins,
    // and none of them holds a thread-local reference, a lock guard, an `Rc`
    // or other `!Send` value across `BexVm::suspend`. Generated code keeps
    // only `Value`s and scalars live across a suspension.
    #[allow(unsafe_code)]
    unsafe impl Send for Fiber {}

    impl Fiber {
        pub(crate) fn new() -> std::io::Result<Self> {
            let stack = DefaultStack::new(STACK_SIZE)?;
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
                    // SAFETY: the VM is parked in `Fiber::resume` and touches
                    // nothing until the fiber suspends; see `Message`.
                    let vm = unsafe { &mut *vm.as_ptr() };
                    message = yielder.suspend(Request::Done(run(vm, yielder, body, &args)));
                }
            });
            Ok(Self {
                coroutine,
                floor,
                bodies: 0,
            })
        }

        pub(crate) fn resume(&mut self, message: Message) -> Request {
            debug_assert!(
                !matches!(message, Message::Stop),
                "only `drop` stops a fiber"
            );
            if matches!(message, Message::Enter { .. }) {
                self.bodies += 1;
            }
            match self.coroutine.resume(message) {
                CoroutineResult::Yield(request) => {
                    if matches!(request, Request::Done(_)) {
                        self.bodies -= 1;
                    }
                    request
                }
                CoroutineResult::Return(()) => unreachable!("a fiber returns only when stopped"),
            }
        }

        pub(crate) fn floor(&self) -> usize {
            self.floor
        }
    }

    impl Drop for Fiber {
        fn drop(&mut self) {
            // Parked bodies need forced unwinding; never interfere with panic unwinding.
            if self.bodies != 0 || std::thread::panicking() || !self.coroutine.started() {
                return;
            }
            // Finishing an idle coroutine avoids corosensei's more expensive forced unwind.
            match self.coroutine.resume(Message::Stop) {
                CoroutineResult::Return(()) => {}
                CoroutineResult::Yield(_) => unreachable!("a stopped fiber returns"),
            }
        }
    }

    /// Bytecode reentry also consumes fiber stack, so it must check for room.
    fn run(
        vm: &mut BexVm,
        yielder: &Yielder<Message, Request>,
        body: CompiledFn,
        args: &[Value],
    ) -> Result<Value, Box<VmError>> {
        vm.compiled.yielder = Some(YielderPtr(yielder.into()));
        vm.compiled.on_fiber = true;
        vm.stack_room()?;
        vm.run_entry(body, args)
    }

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
        /// Heap values in Rust locals become stale across suspension; spill and reload them.
        #[inline(never)]
        #[allow(unsafe_code)]
        pub fn suspend(&mut self, step: Step) -> Result<Value, Box<VmError>> {
            let yielder = self
                .compiled
                .yielder
                .expect("compiled code runs on its VM's fiber");
            let mut vm = NonNull::from(self);
            let mut request = Request::Step(step);
            loop {
                // SAFETY: only the fiber touches the VM until it suspends.
                unsafe { vm.as_mut() }.compiled.on_fiber = false;
                // SAFETY: a yielder lives as long as its fiber, and this body is on it.
                let message = unsafe { yielder.0.as_ref() }.suspend(request);
                let resumed_vm = match &message {
                    Message::Resume { vm, .. } | Message::Enter { vm, .. } => *vm,
                    Message::Stop => unreachable!("a fiber is stopped only with no body on it"),
                };
                // A parked body's `&mut BexVm` must still name the VM driving it.
                assert_eq!(
                    resumed_vm.as_ptr(),
                    vm.as_ptr(),
                    "a VM moved while a compiled body was parked on its fiber"
                );
                // Reborrow from the resumed message across this non-inlined switch,
                // so the compiler cannot cache VM state across the driver's mutations.
                vm = resumed_vm;
                // SAFETY: the driver is parked again and passed this VM back to us.
                let resumed_vm = unsafe { vm.as_mut() };
                match message {
                    Message::Resume { outcome, .. } => {
                        resumed_vm.compiled.yielder = Some(yielder);
                        resumed_vm.compiled.on_fiber = true;
                        return outcome;
                    }
                    Message::Enter { body, args, .. } => {
                        // SAFETY: see above.
                        let yielder = unsafe { yielder.0.as_ref() };
                        request = Request::Done(run(resumed_vm, yielder, body, &args));
                    }
                    Message::Stop => unreachable!("a fiber is stopped only with no body on it"),
                }
            }
        }

        #[inline]
        pub(crate) fn stack_room(&mut self) -> Result<(), Box<VmError>> {
            if stack_address() >= self.compiled.fiber_floor {
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
        pub(crate) fn new() -> std::io::Result<Self> {
            unreachable!("{NEVER}")
        }

        pub(crate) fn floor(&self) -> usize {
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
        pub fn suspend(&mut self, _: Step) -> Result<Value, Box<VmError>> {
            unreachable!("{NEVER}")
        }

        pub(crate) fn stack_room(&mut self) -> Result<(), Box<VmError>> {
            Ok(())
        }
    }
}
