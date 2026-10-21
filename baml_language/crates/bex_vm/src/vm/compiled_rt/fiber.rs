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
//! Every VM that runs compiled code has its own fiber, mapped on first use.
//!
//! Compiled bodies are packed for native hosts only. Targets without stack
//! switching (wasm, Windows on aarch64) build the VM with a fiber that is never
//! entered.

use std::ptr::NonNull;

use bex_vm_types::Value;
use smallvec::SmallVec;

use super::{CompiledFn, Step};
use crate::{
    errors::{VmError, VmInternalError},
    vm::BexVm,
};

/// What the VM sends into the fiber. The VM pointer is passed on every message
/// because the fiber only touches the VM while the VM is driving it.
///
/// The VM must not move while a body is parked on its fiber: a parked body
/// still holds the `&mut BexVm` it was entered with, and [`BexVm::suspend`]
/// asserts that every later message names the same VM.
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
    /// Leave the fiber. Sent only with no body on it, so the coroutine returns.
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
    /// Drive the fiber with the message `message` builds until it suspends or
    /// its body returns.
    ///
    /// The fiber is moved out of `self` while it runs: the body on it holds
    /// the only `&mut BexVm`, so nothing on this side may stay borrowed from
    /// the VM across the switch.
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
        let request = fiber.resume(message(NonNull::from(&mut *self)));
        self.compiled.fiber = Some(fiber);
        self.compiled.on_fiber = false;
        request
    }

    pub(crate) fn enter_compiled(
        &mut self,
        body: CompiledFn,
        args: SmallVec<[Value; 4]>,
    ) -> Request {
        self.fiber_resume(|vm| Message::Enter { vm, body, args })
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

    /// Address space per fiber; the OS commits pages as they are touched. The
    /// VM's frame limit bounds how much is ever used.
    const STACK_SIZE: usize = 4 << 20;

    /// Fiber stack a compiled callee must still have below it: its builtins
    /// and unwinding.
    const STACK_RED_ZONE: usize = 256 << 10;

    pub(crate) struct Fiber {
        coroutine: Coroutine<Message, Request, ()>,
        /// Lowest address a compiled callee may start at; see [`BexVm::stack_room`].
        floor: usize,
        /// Bodies running or waiting on the fiber. Each [`Message::Enter`]
        /// starts one and each [`Request::Done`] ends one, so at zero the
        /// coroutine is parked in its top-level loop (or has never started)
        /// and the stack holds no frame of ours.
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

        /// Lowest address a compiled callee may start at; see [`BexVm::stack_room`].
        pub(crate) fn floor(&self) -> usize {
            self.floor
        }
    }

    impl Drop for Fiber {
        fn drop(&mut self) {
            // A waiting body keeps frames on the stack: dropping the coroutine
            // unwinds them. A panic in flight gets the same, with no stack
            // switch during unwinding. A coroutine that never ran holds nothing.
            if self.bodies != 0 || std::thread::panicking() || !self.coroutine.started() {
                return;
            }
            // The coroutine is parked in its top-level loop: `Stop` makes it
            // return at once, which is cheaper than the forced unwind that
            // dropping it suspended would take.
            match self.coroutine.resume(Message::Stop) {
                CoroutineResult::Return(()) => {}
                CoroutineResult::Yield(_) => unreachable!("a stopped fiber returns"),
            }
        }
    }

    /// Run `body` on the fiber with the VM driving. A body entered from
    /// bytecode nests below the bodies already waiting on the fiber, so it
    /// checks for stack room as a direct compiled call does.
    fn run(
        vm: &mut BexVm,
        yielder: &Yielder<Message, Request>,
        body: CompiledFn,
        args: &[Value],
    ) -> Result<Value, Box<VmError>> {
        vm.compiled.yielder = Some(YielderPtr(yielder.into()));
        vm.compiled.on_fiber = true;
        vm.stack_room()?;
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
                let vm = match &message {
                    Message::Resume { vm, .. } | Message::Enter { vm, .. } => *vm,
                    Message::Stop => unreachable!("a fiber is stopped only with no body on it"),
                };
                // A parked body's `&mut BexVm` must still name the VM driving it.
                assert_eq!(
                    vm.as_ptr(),
                    std::ptr::from_mut(self),
                    "a VM moved while a compiled body was parked on its fiber"
                );
                match message {
                    Message::Resume { outcome, .. } => {
                        self.compiled.on_fiber = true;
                        return outcome;
                    }
                    Message::Enter { body, args, .. } => {
                        // SAFETY: see above.
                        let yielder = unsafe { yielder.0.as_ref() };
                        request = Request::Done(run(self, yielder, body, &args));
                    }
                    Message::Stop => unreachable!("a fiber is stopped only with no body on it"),
                }
            }
        }

        /// Whether the fiber has room for a compiled callee, or the error to raise.
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
        /// See the native implementation.
        pub fn suspend(&mut self, _: Step) -> Result<Value, Box<VmError>> {
            unreachable!("{NEVER}")
        }

        pub(crate) fn stack_room(&mut self) -> Result<(), Box<VmError>> {
            Ok(())
        }
    }
}
