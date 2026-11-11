//! Compiled bodies run on a lazy, VM-owned fiber. Suspensions return control
//! to the VM driver. A parked fiber's stack, from the stack pointer `suspend`
//! records to the stack's base, is a conservative root range, so a body's
//! values stay in its Rust locals across a suspension.
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

/// Whether this target supports the fibers required by compiled bodies.
pub const STACKFUL_FIBERS_SUPPORTED: bool = !cfg!(any(
    target_arch = "wasm32",
    all(windows, target_arch = "aarch64")
));

#[cfg(not(any(target_arch = "wasm32", all(windows, target_arch = "aarch64"))))]
pub(crate) use stackful::{Fiber, YielderPtr};
#[cfg(any(target_arch = "wasm32", all(windows, target_arch = "aarch64")))]
pub(crate) use stackless::{Fiber, YielderPtr};

#[cfg(not(any(target_arch = "wasm32", all(windows, target_arch = "aarch64"))))]
mod stackful {
    use std::{ops::Range, ptr::NonNull};

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
        /// The highest address of the stack: frames grow down from it.
        base: usize,
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
            let base = stack.base().get();
            let coroutine = Coroutine::with_stack(stack, fiber_main);
            Ok(Self {
                coroutine,
                floor,
                base,
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

        /// The words a collection scans conservatively while the fiber is
        /// parked: from `sp`, recorded by the innermost `suspend`, to the
        /// stack's base. Not only while a compiled body is on the fiber: a
        /// fiber parked inside a native that re-entered BAML holds raw
        /// pointers in Rust locals with no body counted. `None` until a
        /// first park records an `sp`.
        pub(crate) fn parked_range(&self, sp: usize) -> Option<Range<*const usize>> {
            (sp != 0).then(|| (sp as *const usize)..(self.base as *const usize))
        }
    }

    /// Store the callee-saved registers to `regs` and return the stack
    /// pointer. A caller's value held only in a callee-saved register is then
    /// in this frame, above the returned pointer, when the fiber switches.
    #[cfg(target_arch = "aarch64")]
    #[allow(clippy::inline_always, unsafe_code)]
    #[inline(always)]
    fn spill_registers(regs: &mut [usize; SPILLED]) -> usize {
        let sp: usize;
        // SAFETY: stores into `regs`, which has room for 11 words, and reads
        // the stack pointer.
        unsafe {
            std::arch::asm!(
                "stp x19, x20, [{b}]",
                "stp x21, x22, [{b}, #16]",
                "stp x23, x24, [{b}, #32]",
                "stp x25, x26, [{b}, #48]",
                "stp x27, x28, [{b}, #64]",
                "str x29, [{b}, #80]",
                "mov {sp}, sp",
                b = in(reg) regs.as_mut_ptr(),
                sp = out(reg) sp,
                options(nostack, preserves_flags),
            );
        }
        sp
    }

    /// As on aarch64; `rdi` and `rsi` are callee-saved on Windows.
    #[cfg(target_arch = "x86_64")]
    #[allow(clippy::inline_always, unsafe_code)]
    #[inline(always)]
    fn spill_registers(regs: &mut [usize; SPILLED]) -> usize {
        let sp: usize;
        // SAFETY: stores into `regs`, which has room for 8 words, and reads
        // the stack pointer.
        unsafe {
            std::arch::asm!(
                "mov [{b}], rbx",
                "mov [{b} + 8], rbp",
                "mov [{b} + 16], r12",
                "mov [{b} + 24], r13",
                "mov [{b} + 32], r14",
                "mov [{b} + 40], r15",
                "mov [{b} + 48], rdi",
                "mov [{b} + 56], rsi",
                "mov {sp}, rsp",
                b = in(reg) regs.as_mut_ptr(),
                sp = out(reg) sp,
                options(nostack, preserves_flags),
            );
        }
        sp
    }

    /// Other targets do not name their registers here: scan the whole
    /// stack, which the switch itself spilled into.
    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
    fn spill_registers(_regs: &mut [usize; SPILLED]) -> usize {
        1
    }

    /// Words of callee-saved registers `spill_registers` stores.
    const SPILLED: usize = 11;

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

    /// Kept out of corosensei's root `catch_unwind`: inlined there, LLVM (Rust
    /// 1.98, x86_64-pc-windows-msvc, opt-level >= 1) emits a cleanup funclet
    /// that clobbers `rsi` without saving it, and the CRT's unwinder then
    /// faults on its own `FuncInfo` pointer whenever a panic or forced unwind
    /// leaves a body.
    #[inline(never)]
    fn fiber_main(yielder: &Yielder<Message, Request>, mut message: Message) {
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

    /// The scan start for a park at `sp`: `sp`, or the stack's lowest usable
    /// address where `spill_registers` does not know the registers.
    fn parked_sp(sp: usize, vm: &BexVm) -> usize {
        if cfg!(any(target_arch = "aarch64", target_arch = "x86_64")) {
            sp
        } else {
            vm.compiled.fiber_floor
        }
    }

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
                // Every caller's live value is now in this stack or `regs`, at
                // or above `sp`: a collection while parked scans from there.
                let mut regs = [0usize; SPILLED];
                let sp = spill_registers(&mut regs);
                // SAFETY: only the fiber touches the VM until it suspends.
                let parked = unsafe { vm.as_mut() };
                parked.compiled.on_fiber = false;
                let scan_from = parked_sp(sp, parked);
                parked.compiled.parked_sp = scan_from;
                // SAFETY: a yielder lives as long as its fiber, and this body is on it.
                let message = unsafe { yielder.0.as_ref() }.suspend(request);
                std::hint::black_box(&regs);
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
        pub fn stack_room(&mut self) -> Result<(), Box<VmError>> {
            if stack_address() >= self.compiled.fiber_floor {
                return Ok(());
            }
            Err(self.stack_overflow())
        }

        /// The `StackOverflow` throw, out of line: every compiled call checks
        /// `stack_room` and inlines its fast path.
        #[allow(
            clippy::unnecessary_box_returns,
            reason = "boxing out of line keeps the allocation off the inlined fast path"
        )]
        #[cold]
        #[inline(never)]
        fn stack_overflow(&mut self) -> Box<VmError> {
            Box::new(VmError::thrown_fresh(
                self.panic_to_exception_value(VmPanic::StackOverflow),
            ))
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

        pub(crate) fn parked_range(&self, _sp: usize) -> Option<std::ops::Range<*const usize>> {
            None
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

        pub fn stack_room(&mut self) -> Result<(), Box<VmError>> {
            Ok(())
        }
    }
}
