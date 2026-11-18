//! Natives that stop and go on later: an `async fn`, and what drives it.
//!
//! Only the compiler's transform of an `async fn` into a state machine is
//! used. The state is one block on the Rust heap, made by the native's
//! symbol ([`start`]) and polled with a waker that does nothing
//! ([`baml_rt_native_poll`]). A native stops in one of two ways, both
//! methods of its [`Scope`]:
//!
//! - [`Scope::call_baml`] runs a compiled function, which may park the
//!   task or throw like any other. A throw comes back to the native as an
//!   error.
//! - [`Scope::io`] runs async Rust on the I/O runtime's own threads, and
//!   parks the task until it is done.
//!
//! # The driver is compiled code
//!
//! Compiled code calls an `async` native through a stub the compiler makes
//! for it. The stub calls the native's symbol, which makes the state and
//! returns it. Then it loops: it calls the poll, which polls the native
//! once and says what to do next, and does that itself, in compiled code:
//! return, throw, call the compiled function the native asked for, or wait
//! for I/O. Every Rust function returns before anything that can stop the
//! task.
//!
//! So no Rust frame is on a task's stack while the task is stopped or while
//! compiled code runs: nothing the Rust compiler keeps in a register or a
//! stack slot is carried to the thread and the worker the task goes on
//! under. The stub reads the worker from its pinned register for every
//! call, and the poll tells the native's scope. A stop checks the stack
//! for this in a build with checks.
//!
//! A poll that returns pending has left one of the two requests in the
//! block. One that has not is a native awaiting something else, which
//! nothing here would ever wake: the program ends saying so.
//!
//! Both methods take the scope exclusively, so no view of a heap object is
//! alive across a stop, and the future must be `Send`, so nothing bound to
//! a thread is either. What a native holds across a stop it holds in roots.
//! Those are in the block, which the collector reaches through the task
//! for as long as the native has not returned.

use std::{
    future::Future,
    io::Write,
    marker::PhantomData,
    mem::{ManuallyDrop, MaybeUninit},
    pin::Pin,
    ptr::NonNull,
    sync::{Arc, Mutex, Once, OnceLock, atomic::Ordering},
    task::{Context, Poll, Waker},
};

use super::{Elem, RootSlots, RtResult, Scope, Thrown, TypeId};
use crate::{
    heap::{Mutator, current_frame},
    layout::{NATIVE_CALL_ARGS, NATIVE_STATE_ARGS, STEP_CALL, STEP_DONE, STEP_THREW, STEP_WAIT},
    sched::{IoWait, Job, Task, Wake},
};

/// The most arguments a native passes a compiled function.
const CALL_ARGS: usize = NATIVE_CALL_ARGS;

/// A compiled function a native can call: an entry the compiler makes for
/// the purpose, which takes and returns every value as a union holds it.
#[derive(Clone, Copy)]
pub struct BamlFn(usize);

impl BamlFn {
    /// The function whose entry is at `code`, when there is one.
    pub(crate) fn at(code: usize) -> Option<Self> {
        (code != 0).then_some(Self(code))
    }
}

/// What a native stopped for.
enum Request {
    /// Call the compiled function at `code` with `args`.
    Call {
        code: usize,
        args: [usize; CALL_ARGS],
    },
    /// Start the job where it says, and go on when it has ended.
    Io(IoWait),
}

/// Where a native and its driver leave each other what a stop is for and
/// how it ended. It is outside the scope, which the native's future holds
/// exclusively for as long as it lives.
pub(crate) struct Exchange {
    request: Option<Request>,
    /// What the compiled function returned, or what it threw.
    word: usize,
    threw: bool,
    /// The trace it threw that with.
    context: usize,
    /// The worker the native is polled under: after a stop, the one the
    /// task went on under.
    worker: *mut Mutator,
}

/// The state of one `async` native: what its stub, the poll and the
/// collector read of it. The future follows, in the same block.
#[repr(C)]
pub struct Head {
    /// What the stub passes the compiled function the native asked for.
    /// First, where the stub reads them.
    args: [usize; CALL_ARGS],
    /// The native the task was inside already, which called the compiled
    /// code that called this one.
    outer: *mut Head,
    poll: unsafe fn(*mut Head) -> Poll<RtResult>,
    drop: unsafe fn(*mut Head),
    /// The builtin's path, for a message about it.
    name: &'static str,
    exchange: Exchange,
    /// The I/O the native asked to wait for, until the stub's wait has
    /// taken it.
    waits_for: Option<IoWait>,
    slots: RootSlots,
    /// Borrowed by the future from when it is made.
    scope: MaybeUninit<Scope<'static>>,
}

const _: () = assert!(std::mem::offset_of!(Head, args) == NATIVE_STATE_ARGS);

#[repr(C)]
struct Block<F> {
    head: Head,
    future: ManuallyDrop<F>,
}

unsafe fn poll_block<F: Future<Output = RtResult>>(head: *mut Head) -> Poll<RtResult> {
    // SAFETY: `head` starts a `Block<F>` that never moves, and nothing
    // else refers to its future.
    unsafe {
        let future = &raw mut (*head.cast::<Block<F>>()).future;
        Pin::new_unchecked(&mut **future).poll(&mut Context::from_waker(Waker::noop()))
    }
}

unsafe fn drop_block<F>(head: *mut Head) {
    // SAFETY: `head` starts a `Block<F>` made by `start`, which nothing
    // refers to any more. The future goes first: it borrows the head.
    unsafe {
        let block = head.cast::<Block<F>>();
        ManuallyDrop::drop(&mut (*block).future);
        drop(Box::from_raw(block));
    }
}

/// Make the state of an `async` native compiled code has just called, and
/// put it first among those its task is inside: `make` makes the native's
/// future from the scope it is to use, having rooted its arguments there
/// and allocated nothing. The native has not run yet. Its stub polls it
/// next.
///
/// # Safety
///
/// Called on a task of `mutator`, from the symbol the native's stub
/// called. The roots `make` makes are of live objects.
pub(crate) unsafe fn start<F, S>(mutator: &mut Mutator, name: &'static str, make: S) -> *mut Head
where
    // `Send`: a native may hold nothing bound to a thread across a stop.
    F: Future<Output = RtResult> + Send,
    S: FnOnce(&'static mut Scope<'static>) -> F,
{
    name_native_in_panics();
    let worker = std::ptr::from_mut(mutator);
    let block = Box::into_raw(Box::<Block<F>>::new_uninit()).cast::<Block<F>>();
    // SAFETY: the block is this function's until it is on the task's list,
    // and each part is written before it is read. The scope and the future
    // refer into the block, which stays where it is until it is dropped.
    unsafe {
        let task = (*worker).cur_task;
        let head = &raw mut (*block).head;
        head.write(Head {
            args: [0; CALL_ARGS],
            outer: (*task).natives,
            poll: poll_block::<F>,
            drop: drop_block::<F>,
            name,
            exchange: Exchange {
                request: None,
                word: 0,
                threw: false,
                context: 0,
                worker,
            },
            waits_for: None,
            slots: RootSlots::new(),
            scope: MaybeUninit::uninit(),
        });
        let scope = (*head).scope.write(Scope {
            mutator: NonNull::new_unchecked(worker),
            slots: &(*head).slots,
            exchange: &raw mut (*head).exchange,
            life: PhantomData,
        });
        (&raw mut (*block).future).write(ManuallyDrop::new(make(scope)));
        (*task).natives = head;
        head
    }
}

/// What a native may await, for the two messages that say it awaited
/// something else.
const MAY_AWAIT: &str = "A native may await only `scope.call_baml` and `scope.io`. Async Rust \
                         of any other kind (a timer, a socket, a channel) has no runtime to run \
                         on here: put it inside the job given to `scope.io`.";

/// Say which native it was when a panic comes out of a poll. A Tokio type
/// used outside the job of `scope.io` panics for want of a runtime, and
/// that message alone does not say where to look.
///
/// The name is in the worker, not in a thread-local: the hook finds the
/// worker through its one process-wide address.
fn name_native_in_panics() {
    static HOOKED: Once = Once::new();
    HOOKED.call_once(|| {
        let earlier = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |panic| {
            let worker = crate::sched::WORKER.load(Ordering::Acquire);
            // SAFETY: null or the worker, which is polling on this thread
            // when a native panics.
            let polling = unsafe { worker.as_ref() }.and_then(|worker| worker.polling);
            if let Some(name) = polling {
                let _ = writeln!(
                    std::io::stderr().lock(),
                    "fatal: the native `{name}` panicked while it was polled. {MAY_AWAIT}"
                );
            }
            earlier(panic);
        }));
    });
}

/// What the stub that drives a native does next: one of the `STEP_*` of
/// [`crate::layout`], and the word that goes with it.
#[repr(C)]
pub(crate) struct Step {
    kind: usize,
    word: usize,
}

// The two ways a native ends are what its result says of a throw.
const _: () = assert!(STEP_DONE == 0 && STEP_THREW == 1);

/// Poll the native at `head` once, and say what the stub that drives it
/// does next. `word` and `threw` are what the compiled function the native
/// asked for last returned, and whether that was thrown, with its trace in
/// the worker's scratch word. Both are zero for the first poll and after a
/// wait.
///
/// `worker` is the stub's pinned register as it calls: after a stop, the
/// worker the task went on under. The native's scope is told here.
///
/// This returns before the task can stop. What the native stopped for is
/// done by the stub, in compiled code, between two calls.
///
/// # Safety
///
/// Called by the stub of an `async` native, on its task, with the worker
/// running the task and the state the native's symbol returned.
#[unsafe(no_mangle)]
pub(crate) unsafe extern "C" fn baml_rt_native_poll(
    worker: *mut Mutator,
    head: *mut Head,
    word: usize,
    threw: usize,
) -> Step {
    // SAFETY: the caller's contract. The worker is reached through its
    // address: the native reaches it through its scope meanwhile.
    unsafe {
        // What a collection during the poll walks from: the stub's frame.
        (*worker).enter(current_frame());
        let exchange = &raw mut (*head).exchange;
        (*exchange).word = word;
        (*exchange).threw = threw != 0;
        (*exchange).context = (*worker).scratch();
        (*exchange).worker = worker;
        (*worker).polling = Some((*head).name);
        let polled = ((*head).poll)(head);
        (*worker).polling = None;
        if let Poll::Ready(result) = polled {
            let task = (*worker).cur_task;
            debug_assert!(
                std::ptr::eq((*task).natives, head),
                "natives end innermost first"
            );
            (*task).natives = (*head).outer;
            ((*head).drop)(head);
            return Step {
                kind: result.thrown,
                word: result.value as usize,
            };
        }
        match (*exchange).request.take() {
            Some(Request::Call { code, args }) => {
                (*head).args = args;
                Step {
                    kind: STEP_CALL,
                    word: code,
                }
            }
            Some(Request::Io(wait)) => {
                let waits_for = &raw mut (*head).waits_for;
                *waits_for = Some(wait);
                Step {
                    kind: STEP_WAIT,
                    word: waits_for as usize,
                }
            }
            None => {
                let _ = writeln!(
                    std::io::stderr().lock(),
                    "fatal: the native `{}` stopped without asking for anything. {MAY_AWAIT}",
                    (*head).name
                );
                std::process::abort();
            }
        }
    }
}

/// Start `job` on the runtime that runs the jobs of [`Scope::io`]: Tokio,
/// on a thread of its own, made when the first job is. `wake` is used when
/// the job has ended.
///
/// Only [`Scope::io`] names this function, and the wait calls it through
/// the request, so a program none of whose natives calls `scope.io` links
/// no Tokio.
fn start_on_tokio(job: Job, wake: Wake) {
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    let runtime = RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .thread_name("baml-io")
            .enable_all()
            .build()
            .expect("the I/O runtime starts")
    });
    runtime.spawn(async move {
        job.await;
        wake.wake();
    });
}

/// Hand `each` the address of every root the `async` natives `task` is
/// inside hold.
///
/// # Safety
///
/// `task` is a task in use.
pub(crate) unsafe fn each_root(task: *mut Task, mut each: impl FnMut(usize)) {
    // SAFETY: the caller's contract: the list is of live blocks.
    unsafe {
        let mut head = (*task).natives;
        while !head.is_null() {
            (*head).slots.each_root(&mut each);
            head = (*head).outer;
        }
    }
}

/// Drop the state of every `async` native `task` is inside, innermost
/// first: their destructors run, and their roots are roots no more.
///
/// # Safety
///
/// `task` is a task whose stack nothing will run on again.
pub(crate) unsafe fn drop_all(task: *mut Task) {
    // SAFETY: the caller's contract: no stub will poll these again.
    unsafe {
        while !(*task).natives.is_null() {
            let head = (*task).natives;
            (*task).natives = (*head).outer;
            ((*head).drop)(head);
        }
    }
}

/// One stop of a native: the driver is asked for something, and then the
/// native has its scope again, under the worker its task went on under.
///
/// Every way a native stops is made of this one future, so no stop leaves
/// a scope with the worker the task stopped on.
struct Stop<'a, 's> {
    /// The scope, until the native has gone on.
    scope: Option<&'a mut Scope<'s>>,
    /// What the driver is asked for, until it has been.
    request: Option<Request>,
}

impl<'a, 's> Future for Stop<'a, 's> {
    type Output = &'a mut Scope<'s>;

    fn poll(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        let scope = self.scope.take().expect("a stop is over once it is ready");
        // SAFETY: the scope is an `async` native's, so the exchange is its
        // block's, and the driver answers a request before it polls again,
        // having left there the worker the task went on under.
        unsafe {
            if let Some(request) = self.request.take() {
                (*scope.exchange).request = Some(request);
                self.scope = Some(scope);
                return Poll::Pending;
            }
            scope.mutator = NonNull::new_unchecked((*scope.exchange).worker);
        }
        Poll::Ready(scope)
    }
}

impl<'s> Scope<'s> {
    fn stops(&self, how: &str) {
        assert!(
            !self.exchange.is_null(),
            "`{how}` is for a native listed as `async`"
        );
    }

    /// Call the compiled function `function` with `args`, each a value as
    /// a union holds it, and return what it returns, held the same way.
    /// What it throws is the error, to hand on with `?`.
    ///
    /// The call runs between two polls of the native: it may run for as
    /// long as any BAML, park, and collect.
    pub fn call_baml<'a>(
        &'a mut self,
        function: BamlFn,
        args: &[Elem<'s>],
    ) -> impl Future<Output = Result<Elem<'s>, Thrown<'s>>> + Send + use<'a, 's> {
        self.stops("scope.call_baml");
        assert!(
            args.len() <= CALL_ARGS,
            "a native passes a compiled function at most {CALL_ARGS} arguments"
        );
        let mut words = [0; CALL_ARGS];
        for (word, arg) in words.iter_mut().zip(args) {
            *word = arg.get(self);
        }
        let stop = Stop {
            scope: Some(self),
            request: Some(Request::Call {
                code: function.0,
                args: words,
            }),
        };
        async move {
            let scope = stop.await;
            // SAFETY: the driver answered the call before it polled again:
            // with what compiled code returned or threw, and its trace.
            unsafe {
                let exchange = scope.exchange;
                let value = scope.elem_raw((*exchange).word, TypeId(0));
                if (*exchange).threw {
                    let context = scope.elem_raw((*exchange).context, TypeId(0));
                    Err(Thrown {
                        error: value,
                        context: Some(context),
                    })
                } else {
                    Ok(value)
                }
            }
        }
    }

    /// Run `job` on the I/O runtime, which is Tokio on threads of its own,
    /// and return what it returns. The task is parked meanwhile and other
    /// tasks run.
    ///
    /// The job is `'static` and `Send`: it can hold no root and no view,
    /// so it never touches the BAML heap. What it returns is plain Rust,
    /// which the native makes objects of once it is running again.
    pub fn io<'a, T, J>(&'a mut self, job: J) -> impl Future<Output = T> + Send + use<'a, 's, T, J>
    where
        T: Send + 'static,
        J: Future<Output = T> + Send + 'static,
    {
        self.stops("scope.io");
        // Where the job leaves what it made.
        let made = Arc::new(Mutex::new(None));
        let slot = Arc::clone(&made);
        let stop = Stop {
            scope: Some(self),
            request: Some(Request::Io(IoWait {
                job: Box::pin(async move {
                    let value = job.await;
                    *slot.lock().expect("no thread panics holding it") = Some(value);
                }),
                start: start_on_tokio,
            })),
        };
        async move {
            stop.await;
            let made = made.lock().expect("the job did not panic").take();
            made.expect("the driver polls again once the job has ended")
        }
    }

    /// Run `work`, which is long and touches no heap object, and return
    /// what it returns. It runs here, on the same thread.
    ///
    /// With one worker this is a plain call. With several, a collection
    /// needs every worker stopped, and this is where a worker says it
    /// already is: the others then collect without waiting for the work to
    /// end. That is why a native needs no flag to say it runs long. The
    /// closure is `'static` and `Send` so that it can hold no root and no
    /// view, which a collection in the meantime would leave stale.
    pub fn off_heap<T>(&mut self, work: impl FnOnce() -> T + Send + 'static) -> T {
        work()
    }

    /// What the symbol of an `async` native returns for an error: the
    /// error itself with a flag, having left the trace it was caught with
    /// in the scratch word for the caller to throw it again with.
    pub fn throw_again(&mut self, thrown: &Thrown<'_>) -> RtResult {
        let context = thrown.context.map_or(0, |context| context.slot.get());
        self.mutator().set_scratch(context);
        RtResult {
            value: thrown.error.slot.get() as u64,
            thrown: 1,
        }
    }
}

#[cfg(all(test, target_arch = "aarch64"))]
mod tests {
    use std::{
        process::Command,
        sync::atomic::{AtomicBool, Ordering},
        time::Duration,
    };

    use super::*;
    use crate::{
        heap::{Program, load},
        layout::RUNTIME_TYPES,
    };

    /// A stack map table with no functions.
    static NO_STACK_MAPS: usize = 0;

    /// A worker of a program with no compiled code. With `stress` every
    /// allocation collects.
    fn worker(stress: bool) -> Box<Mutator> {
        let program = Program {
            stack_maps: std::ptr::from_ref(&NO_STACK_MAPS) as usize,
            types: RUNTIME_TYPES.to_vec().leak().as_ptr() as usize,
            unwind: 0,
            functions: 0,
        };
        // SAFETY: both tables are in the compiler's formats.
        let mut worker = Box::new(unsafe { Mutator::new(program, stress) });
        // For a test that drives a native without running the scheduler.
        crate::sched::WORKER.store(&raw mut *worker, Ordering::Release);
        worker
    }

    /// A worker running a task: where a native runs.
    fn worker_on_a_task(stress: bool) -> (Box<Mutator>, *mut Task) {
        let mut worker = worker(stress);
        let task = crate::sched::task_for_tests(&mut worker);
        (worker, task)
    }

    /// What the compiled stub of a native does, for a native that asks
    /// only for calls: poll, call what was asked for with the worker in
    /// the pinned register, and poll again under the worker the task went
    /// on under. Returns that worker and how the native ended.
    unsafe fn drive<F, S>(
        mut worker: *mut Mutator,
        name: &'static str,
        make: S,
    ) -> (*mut Mutator, Step)
    where
        F: Future<Output = RtResult> + Send,
        S: FnOnce(&'static mut Scope<'static>) -> F,
    {
        // SAFETY: the caller is on a task of `worker`, and what the native
        // calls keeps every register the C convention has a callee keep,
        // but the pinned one.
        unsafe {
            let head = start(&mut *worker, name, make);
            let (mut word, mut threw) = (0, 0);
            loop {
                let step = baml_rt_native_poll(worker, head, word, threw);
                if step.kind != STEP_CALL {
                    return (worker, step);
                }
                let args = (*head).args;
                std::arch::asm!(
                    "blr {code}",
                    code = in(reg) step.word,
                    inout("x0") args[0] => word,
                    inout("x1") args[1] => threw,
                    inout("x2") args[2] => _,
                    inout("x3") args[3] => _,
                    inout("x21") worker,
                    clobber_abi("C"),
                );
            }
        }
    }

    struct SetWhenDropped(&'static AtomicBool);

    impl Drop for SetWhenDropped {
        fn drop(&mut self) {
            self.0.store(true, Ordering::Release);
        }
    }

    #[test]
    fn a_stopped_native_keeps_its_roots_current_and_is_dropped_whole() {
        static DROPPED: AtomicBool = AtomicBool::new(false);
        const TEXT: &str = "held across the stop";
        let (mut worker, task) = worker_on_a_task(true);
        // SAFETY: the native is polled and dropped on the worker it was
        // made for, and its one root is a live string.
        unsafe {
            let head = start(&mut worker, "test.stops", |scope| async move {
                let held = SetWhenDropped(&DROPPED);
                let text = scope.alloc_str(TEXT);
                scope.io(std::future::pending::<()>()).await;
                drop(held);
                RtResult::ok(text.raw(scope) as u64)
            });
            assert!(((*head).poll)(head).is_pending());

            let mut roots = Vec::new();
            each_root(task, |slot| roots.push(slot));
            let [root] = roots[..] else {
                panic!("the native holds one root, not {}", roots.len());
            };
            let before = load(root);
            worker.allocate(crate::layout::WORD);
            assert_ne!(load(root), before, "a collection moves the string");
            assert_eq!(crate::string::text(load(root)), TEXT);

            drop_all(task);
            assert!(DROPPED.load(Ordering::Acquire), "its destructors ran");
            each_root(task, |_| panic!("a dropped native holds no root"));
        }
    }

    #[test]
    fn rust_state_moves_with_its_object_and_an_owned_value_is_dropped_once() {
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };

        static DROPS: AtomicUsize = AtomicUsize::new(0);
        struct Counted(u64);
        impl Drop for Counted {
            fn drop(&mut self) {
                DROPS.fetch_add(1, Ordering::AcqRel);
            }
        }

        let (mut worker, task) = worker_on_a_task(true);
        // SAFETY: the native is polled and dropped on the worker it was
        // made for.
        unsafe {
            let head = start(&mut worker, "test.owns", |scope| async move {
                let owned = scope.alloc_owned(Arc::new(Counted(7)));
                let plain = scope.alloc_plain([1_u64, 2, 3]);
                // Its root is given back, so nothing holds the object.
                scope.scoped(|scope, _| {
                    let dead = scope.alloc_owned(Arc::new(Counted(9)));
                    assert_eq!(dead.get(scope).owned::<Counted>().0, 9);
                });
                // Every allocation collects: the value of the dead object
                // goes, and the others are found where their objects are.
                scope.alloc_str("moves everything");
                assert_eq!(DROPS.load(Ordering::Acquire), 1);
                plain.get(scope).plain::<[u64; 3]>().set([4, 5, 6]);
                let shared = owned.get(scope).share::<Counted>();
                scope.alloc_str("again");
                assert_eq!(plain.get(scope).plain::<[u64; 3]>().get(), [4, 5, 6]);
                assert_eq!(owned.get(scope).owned::<Counted>().0, 7);
                assert_eq!(Arc::strong_count(&shared), 2);
                drop(shared);
                assert_eq!(DROPS.load(Ordering::Acquire), 1);
                RtResult::ok(0)
            });
            assert!(((*head).poll)(head).is_ready());
            drop_all(task);
            // Nothing holds the object now.
            worker.allocate(crate::layout::WORD);
            assert_eq!(DROPS.load(Ordering::Acquire), 2);
            worker.drop_owned();
            assert_eq!(DROPS.load(Ordering::Acquire), 2);
        }
    }

    /// What compiled code a native calls would be: it sleeps, and returns
    /// nothing thrown.
    #[unsafe(naked)]
    unsafe extern "C" fn sleeps() {
        core::arch::naked_asm!(
            "stp x29, x30, [sp, #-16]!",
            "mov x29, sp",
            "mov x0, #16960",
            "movk x0, #15, lsl #16",
            "bl {sleep}",
            "mov x0, #0",
            "mov x1, #0",
            "ldp x29, x30, [sp], #16",
            "ret",
            sleep = sym crate::sched::switch::baml_rt_sleep,
        )
    }

    /// Whether the native below found itself on the thread it started on.
    static SAME_THREAD: AtomicBool = AtomicBool::new(false);

    /// Runs a native that does what no native may: it keeps something of
    /// its thread, here the thread's id, across `scope.call_baml`.
    unsafe extern "C" fn keeps_its_thread(worker: *mut Mutator) -> *mut Mutator {
        // SAFETY: on a task of `worker`.
        let (worker, _) = unsafe {
            drive(worker, "test.keeps_its_thread", |scope| async move {
                let thread = std::thread::current().id();
                let called = scope.call_baml(BamlFn(sleeps as *const () as usize), &[]);
                let slept = called.await.is_ok();
                SAME_THREAD.store(
                    slept && std::thread::current().id() == thread,
                    Ordering::Release,
                );
                RtResult::ok(0)
            })
        };
        worker
    }
    crate::sched::tests::task_function!(keeps_its_thread_task => keeps_its_thread);

    /// Migration stress finds a native that holds on to its thread across
    /// a stop: without it the native never notices.
    #[test]
    fn migration_stress_catches_a_native_that_keeps_its_thread() {
        for migrate in [false, true] {
            let mut worker = worker(false);
            let entry = keeps_its_thread_task as *const () as usize;
            // SAFETY: the worker is this test's, and the entry a task
            // function.
            let status = unsafe { crate::sched::run(&raw mut *worker, entry, migrate) };
            assert_eq!(status, 0);
            assert_eq!(SAME_THREAD.load(Ordering::Acquire), !migrate);
        }
    }

    /// Whether the native below found what it held and what it made.
    static INTACT: AtomicBool = AtomicBool::new(false);

    /// Runs a native that allocates once it has stopped and gone on.
    unsafe extern "C" fn allocates_after_a_stop(worker: *mut Mutator) -> *mut Mutator {
        const HELD: &str = "held across the stop";
        const MADE: &str = "made after it";
        // SAFETY: on a task of `worker`.
        let (worker, _) = unsafe {
            drive(worker, "test.allocates_after_a_stop", |scope| async move {
                let held = scope.alloc_str(HELD);
                let called = scope.call_baml(BamlFn(sleeps as *const () as usize), &[]);
                let slept = called.await.is_ok();
                let made = scope.alloc_str(MADE);
                INTACT.store(
                    slept && held.get(scope).as_str() == HELD && made.get(scope).as_str() == MADE,
                    Ordering::Release,
                );
                RtResult::ok(0)
            })
        };
        worker
    }
    crate::sched::tests::task_function!(allocates_after_a_stop_task => allocates_after_a_stop);

    /// A native allocates with the worker its task went on under, which
    /// under migration stress is at another address than the one it
    /// stopped on. Every allocation collects.
    #[test]
    fn a_native_allocates_after_it_stopped_and_moved() {
        for migrate in [false, true] {
            let mut worker = worker(true);
            let entry = allocates_after_a_stop_task as *const () as usize;
            // SAFETY: the worker is this test's, and the entry a task
            // function.
            let status = unsafe { crate::sched::run(&raw mut *worker, entry, migrate) };
            assert_eq!(status, 0);
            assert!(INTACT.swap(false, Ordering::AcqRel), "migrating: {migrate}");
        }
    }

    /// Whether the native below found every string it held.
    static ALL_HELD: AtomicBool = AtomicBool::new(false);

    /// Runs a native that holds forty roots across a stop.
    unsafe extern "C" fn holds_forty_roots(worker: *mut Mutator) -> *mut Mutator {
        // SAFETY: on a task of `worker`.
        let (worker, _) = unsafe {
            drive(worker, "test.holds_forty_roots", |scope| async move {
                let held: Vec<_> = (0..40)
                    .map(|index| scope.alloc_str(&format!("root {index}")))
                    .collect();
                let called = scope.call_baml(BamlFn(sleeps as *const () as usize), &[]);
                let slept = called.await.is_ok();
                // One more allocation, which moves all forty.
                let last = scope.alloc_str("and one more");
                let intact = held
                    .iter()
                    .enumerate()
                    .all(|(index, text)| text.get(scope).as_str() == format!("root {index}"));
                ALL_HELD.store(
                    slept && intact && last.get(scope).as_str() == "and one more",
                    Ordering::Release,
                );
                RtResult::ok(0)
            })
        };
        worker
    }
    crate::sched::tests::task_function!(holds_forty_roots_task => holds_forty_roots);

    /// A native holds as many roots as it makes, and the collector keeps
    /// every one current across a stop, with and without a move.
    #[test]
    fn a_native_holds_forty_roots_across_a_stop() {
        for (stress, migrate) in [(false, false), (true, false), (false, true), (true, true)] {
            let mut worker = worker(stress);
            let entry = holds_forty_roots_task as *const () as usize;
            // SAFETY: the worker is this test's, and the entry a task
            // function.
            let status = unsafe { crate::sched::run(&raw mut *worker, entry, migrate) };
            assert_eq!(status, 0);
            assert!(
                ALL_HELD.swap(false, Ordering::AcqRel),
                "collecting always: {stress}, migrating: {migrate}"
            );
        }
    }

    /// Runs a native that panics once it has stopped and gone on.
    unsafe extern "C" fn panics_after_a_stop(worker: *mut Mutator) -> *mut Mutator {
        // SAFETY: on a task of `worker`.
        let (worker, _) = unsafe {
            drive(worker, "test.panics_after_a_stop", |scope| async move {
                let called = scope.call_baml(BamlFn(sleeps as *const () as usize), &[]);
                let _ = called.await;
                panic!("after the stop")
            })
        };
        worker
    }
    crate::sched::tests::task_function!(panics_after_a_stop_task => panics_after_a_stop);

    #[test]
    #[ignore = "ends its process: run by the test below"]
    fn panics_after_it_moved() {
        let mut worker = worker(false);
        let entry = panics_after_a_stop_task as *const () as usize;
        // SAFETY: the worker is this test's, and the entry a task function.
        unsafe { crate::sched::run(&raw mut *worker, entry, true) };
    }

    /// The panic of a native is named whichever thread it is polled on: it
    /// stopped on one and went on on another.
    #[test]
    fn a_native_that_panics_after_it_moved_is_named() {
        let stderr = in_a_process_of_its_own("panics_after_it_moved");
        assert!(
            stderr.contains("the native `test.panics_after_a_stop` panicked while it was polled"),
            "{stderr}"
        );
    }

    /// Run the ignored test `name` of this module in a process of its own,
    /// which it does not survive, and return what it wrote to stderr.
    fn in_a_process_of_its_own(name: &str) -> String {
        let output = Command::new(std::env::current_exe().expect("the test binary"))
            .args(["--exact", &format!("native::drive::tests::{name}")])
            .args(["--ignored", "--nocapture"])
            .output()
            .expect("the test binary starts");
        assert!(!output.status.success(), "`{name}` ends its process");
        String::from_utf8_lossy(&output.stderr).into_owned()
    }

    #[test]
    fn a_native_that_awaits_anything_else_is_named_when_the_program_ends() {
        let stderr = in_a_process_of_its_own("stops_for_nothing");
        assert!(
            stderr.contains("the native `test.stops_for_nothing` stopped without asking"),
            "{stderr}"
        );
        let stderr = in_a_process_of_its_own("awaits_a_tokio_timer");
        assert!(
            stderr.contains("the native `test.awaits_a_tokio_timer` panicked while it was polled"),
            "{stderr}"
        );
    }

    #[test]
    #[ignore = "ends its process: run by the test above"]
    fn stops_for_nothing() {
        let (mut worker, _) = worker_on_a_task(false);
        // SAFETY: on a worker that runs a task.
        unsafe {
            drive(&raw mut *worker, "test.stops_for_nothing", |_| async {
                std::future::pending::<()>().await;
                RtResult::ok(0)
            });
        }
    }

    #[test]
    #[ignore = "ends its process: run by the test above"]
    fn awaits_a_tokio_timer() {
        let (mut worker, _) = worker_on_a_task(false);
        // SAFETY: as above.
        unsafe {
            drive(&raw mut *worker, "test.awaits_a_tokio_timer", |_| async {
                tokio::time::sleep(Duration::from_millis(1)).await;
                RtResult::ok(0)
            });
        }
    }
}
