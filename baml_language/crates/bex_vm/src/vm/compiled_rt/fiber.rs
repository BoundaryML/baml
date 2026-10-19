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

use std::ptr::NonNull;

use bex_vm_types::Value;
#[cfg(not(target_arch = "wasm32"))]
use corosensei::{
    Coroutine, CoroutineResult, Yielder,
    stack::{DefaultStack, Stack as _},
};
use smallvec::SmallVec;

use super::{CompiledFn, Step};
use crate::{errors::VmError, vm::BexVm};

/// Address space per fiber; the OS commits pages as they are touched. The VM's
/// frame limit bounds how much is ever used.
#[cfg(not(target_arch = "wasm32"))]
const STACK_SIZE: usize = 4 << 20;

/// What the VM sends into the fiber. The VM pointer is passed on every message
/// because the fiber only touches the VM while the VM is driving it.
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
}

/// What the fiber sends back to the VM, always about the innermost body.
pub(crate) enum Request {
    /// Do this, then resume with its outcome.
    Step(Step),
    /// The body returned.
    Done(Result<Value, Box<VmError>>),
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) struct Fiber {
    coroutine: Coroutine<Message, Request, ()>,
    /// Lowest address a compiled callee may start at; see [`BexVm::stack_room`].
    pub(crate) floor: usize,
}

// SAFETY: while suspended, the fiber's stack holds only frames of compiled
// bodies, VM code and builtins, whose locals are `Value`s and other `Send`
// data; nothing thread-bound is held across `suspend`. The VM itself moves
// between threads between `exec` calls, and the fiber moves with it.
#[cfg(not(target_arch = "wasm32"))]
#[allow(unsafe_code)]
unsafe impl Send for Fiber {}

#[cfg(not(target_arch = "wasm32"))]
impl Fiber {
    pub(crate) fn new() -> Self {
        let stack = DefaultStack::new(STACK_SIZE).expect("map a fiber stack");
        let floor = stack.limit().get() + super::STACK_RED_ZONE;
        let coroutine = Coroutine::with_stack(stack, |yielder, mut message| {
            loop {
                let Message::Enter { vm, body, args } = message else {
                    unreachable!("a fiber with no body running is resumed with one to run")
                };
                message = yielder.suspend(Request::Done(run(yielder, vm, body, &args)));
            }
        });
        Self { coroutine, floor }
    }

    pub(crate) fn resume(&mut self, message: Message) -> Request {
        match self.coroutine.resume(message) {
            CoroutineResult::Yield(request) => request,
            CoroutineResult::Return(()) => unreachable!("a fiber never returns"),
        }
    }
}

/// Run `body` on the fiber with the VM driving.
#[cfg(not(target_arch = "wasm32"))]
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

/// The running fiber's yielder, reachable from the VM so [`BexVm::suspend`] can
/// switch out from any depth.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Copy)]
pub(crate) struct YielderPtr(NonNull<Yielder<Message, Request>>);

// SAFETY: the pointer is only dereferenced by code running on the fiber it
// belongs to, on whichever thread is driving that fiber.
#[cfg(not(target_arch = "wasm32"))]
#[allow(unsafe_code)]
unsafe impl Send for YielderPtr {}

/// Compiled bodies are packed for native targets only; a wasm VM never enters
/// a fiber.
#[cfg(target_arch = "wasm32")]
pub(crate) struct Fiber {
    pub(crate) floor: usize,
}

#[cfg(target_arch = "wasm32")]
impl Fiber {
    pub(crate) fn new() -> Self {
        unreachable!("compiled bodies do not run on wasm")
    }

    pub(crate) fn resume(&mut self, _: Message) -> Request {
        unreachable!("compiled bodies do not run on wasm")
    }
}

#[cfg(target_arch = "wasm32")]
#[derive(Clone, Copy)]
pub(crate) struct YielderPtr;

impl BexVm {
    /// Hand `step` to the VM and continue with its outcome once the VM resumes
    /// this body. Heap values held in Rust locals across this call are stale
    /// afterwards: generated code keeps them in frame slots and reloads them.
    #[cfg(target_arch = "wasm32")]
    pub fn suspend(&mut self, _: Step) -> Result<Value, Box<VmError>> {
        unreachable!("compiled bodies do not run on wasm")
    }

    /// Hand `step` to the VM and continue with its outcome once the VM resumes
    /// this body. Heap values held in Rust locals across this call are stale
    /// afterwards: generated code keeps them in frame slots and reloads them.
    #[cfg(not(target_arch = "wasm32"))]
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
            }
        }
    }

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

    /// Whether the fiber has room for a compiled callee, or the error to raise.
    #[inline]
    pub(crate) fn stack_room(&mut self) -> Result<(), Box<VmError>> {
        let floor = self.compiled.fiber.as_ref().map_or(0, |fiber| fiber.floor);
        if super::stack_address() >= floor {
            return Ok(());
        }
        Err(Box::new(VmError::thrown_fresh(
            self.panic_to_exception_value(crate::errors::VmPanic::StackOverflow),
        )))
    }
}
