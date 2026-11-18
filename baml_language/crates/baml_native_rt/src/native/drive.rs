//! Natives that stop and go on later: an `async fn`, and the loop that
//! drives it.
//!
//! Only the compiler's transform of an `async fn` into a state machine is
//! used. The state is one block on the Rust heap, made by the native's
//! symbol and polled by [`run`] with a waker that does nothing. A native
//! stops in one of two ways, both methods of its [`Scope`]:
//!
//! - [`Scope::call_baml`] runs a compiled function. The driver calls it as
//!   an ordinary call on the task's stack, between two polls, so no frame
//!   of the native is on the stack while compiled code runs: the call may
//!   park the task or throw like any other. A throw comes back to the
//!   native as an error.
//! - [`Scope::io`] runs async Rust on the I/O runtime's own threads, and
//!   parks the task until it is done.
//!
//! A poll that returns pending has left one of those two requests in the
//! block. One that has not is a native awaiting something else, which
//! nothing here would ever wake: the program ends saying so.
//!
//! Both methods take the scope exclusively, so no view of a heap object is
//! alive across a stop, and the future must be `Send`, so nothing bound to
//! a thread is either. What a native holds across a stop it holds in roots.
//! Those are in the block, which the collector reaches through the task
//! for as long as the native has not returned.

use std::{
    cell::Cell,
    future::Future,
    io::Write,
    marker::PhantomData,
    mem::{ManuallyDrop, MaybeUninit},
    pin::Pin,
    ptr::NonNull,
    sync::{Arc, Mutex, Once, OnceLock},
    task::{Context, Poll, Waker},
};

use super::{Elem, RootSlots, RtResult, Scope, Thrown, TypeId};
use crate::{
    heap::Mutator,
    sched::{Job, Task},
};

/// The most arguments a native passes a compiled function.
const CALL_ARGS: usize = 4;

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
    /// Run the job on the I/O runtime, and go on when it has ended.
    Io(Job),
}

/// Where a native and its driver leave each other what a stop is for and
/// how it ended. It is outside the scope, which the native's future holds
/// exclusively for as long as it lives.
#[derive(Default)]
pub(crate) struct Exchange {
    request: Option<Request>,
    /// What the compiled function returned, or what it threw.
    word: usize,
    threw: bool,
    /// The trace it threw that with.
    context: usize,
}

/// The state of one `async` native: what the driver and the collector read
/// of it. The future follows, in the same block.
pub(crate) struct Head {
    /// The native the task was inside already, which called the compiled
    /// code that called this one.
    outer: *mut Head,
    poll: unsafe fn(*mut Head) -> Poll<RtResult>,
    drop: unsafe fn(*mut Head),
    /// The builtin's path, for a message about it.
    name: &'static str,
    exchange: Exchange,
    slots: RootSlots,
    /// Borrowed by the future from when it is made.
    scope: MaybeUninit<Scope<'static>>,
}

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
    // SAFETY: `head` starts a `Block<F>` made by `drive`, which nothing
    // refers to any more. The future goes first: it borrows the head.
    unsafe {
        let block = head.cast::<Block<F>>();
        ManuallyDrop::drop(&mut (*block).future);
        drop(Box::from_raw(block));
    }
}

/// Run an `async` native to its end: `start` makes its future from the
/// scope it is to use, having rooted its arguments there and allocated
/// nothing.
///
/// # Safety
///
/// Called on a task, from the symbol compiled code called, after the
/// mutator was told that function's frame ([`Mutator::enter`]). The roots
/// `start` makes are of live objects.
pub(crate) unsafe fn drive<F, S>(mutator: &mut Mutator, name: &'static str, start: S) -> RtResult
where
    // `Send`: a native may hold nothing bound to a thread across a stop.
    F: Future<Output = RtResult> + Send,
    S: FnOnce(&'static mut Scope<'static>) -> F,
{
    let worker = std::ptr::from_mut(mutator);
    let block = Box::into_raw(Box::<Block<F>>::new_uninit()).cast::<Block<F>>();
    // SAFETY: the block is this function's until it is on the task's list,
    // and each part is written before it is read. The scope and the future
    // refer into the block, which stays where it is until it is dropped.
    unsafe {
        let head = &raw mut (*block).head;
        head.write(Head {
            outer: std::ptr::null_mut(),
            poll: poll_block::<F>,
            drop: drop_block::<F>,
            name,
            exchange: Exchange::default(),
            slots: RootSlots::new(),
            scope: MaybeUninit::uninit(),
        });
        let scope = (*head).scope.write(Scope {
            mutator: NonNull::new_unchecked(worker),
            slots: &(*head).slots,
            exchange: &raw mut (*head).exchange,
            life: PhantomData,
        });
        (&raw mut (*block).future).write(ManuallyDrop::new(start(scope)));
        run(worker, head)
    }
}

thread_local! {
    /// The native this thread is polling, for a panic inside it to name.
    static POLLING: Cell<Option<&'static str>> = const { Cell::new(None) };
}

/// What a native may await, for the two messages that say it awaited
/// something else.
const MAY_AWAIT: &str = "A native may await only `scope.call_baml` and `scope.io`. Async Rust \
                         of any other kind (a timer, a socket, a channel) has no runtime to run \
                         on here: put it inside the job given to `scope.io`.";

/// Say which native it was when a panic comes out of a poll. A Tokio type
/// used outside the job of `scope.io` panics for want of a runtime, and
/// that message alone does not say where to look.
fn name_native_in_panics() {
    static HOOKED: Once = Once::new();
    HOOKED.call_once(|| {
        let earlier = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |panic| {
            if let Some(name) = POLLING.get() {
                let _ = writeln!(
                    std::io::stderr().lock(),
                    "fatal: the native `{name}` panicked while it was polled. {MAY_AWAIT}"
                );
            }
            earlier(panic);
        }));
    });
}

/// Poll the native at `head` until it is done, doing between polls what
/// it stopped for.
///
/// # Safety
///
/// As [`drive`], with a block it made.
unsafe fn run(mut worker: *mut Mutator, head: *mut Head) -> RtResult {
    name_native_in_panics();
    // SAFETY: the caller's contract. The worker is reached through its
    // address: compiled code this calls reaches it through its register.
    unsafe {
        let task = (*worker).cur_task;
        // The frame of the compiled function that called the native: what a
        // collection during a poll walks from. Compiled code the native
        // calls records frames of its own.
        let caller = (*worker).anchor();
        (*head).outer = (*task).natives;
        (*task).natives = head;
        let result = loop {
            POLLING.set(Some((*head).name));
            let polled = ((*head).poll)(head);
            POLLING.set(None);
            if let Poll::Ready(result) = polled {
                break result;
            }
            match (*head).exchange.request.take() {
                Some(Request::Call { code, args }) => {
                    let (word, threw);
                    (word, threw, worker) = call_compiled(worker, code, args);
                    (*head).exchange.word = word;
                    (*head).exchange.threw = threw != 0;
                    // The function's entry leaves the trace of a throw in
                    // the scratch word.
                    (*head).exchange.context = (*worker).scratch();
                }
                Some(Request::Io(job)) => worker = wait_for_io(worker, job),
                None => {
                    let _ = writeln!(
                        std::io::stderr().lock(),
                        "fatal: the native `{}` stopped without asking for anything. {MAY_AWAIT}",
                        (*head).name
                    );
                    std::process::abort();
                }
            }
            (*worker).set_anchor(caller);
        };
        (*task).natives = (*head).outer;
        ((*head).drop)(head);
        result
    }
}

/// Call the compiled entry at `code` as compiled code calls a function:
/// with the worker in the pinned register. Returns its two results and the
/// worker the task went on under.
///
/// # Safety
///
/// Called on a task of `worker`, with an entry the compiler made for
/// natives to call and arguments of its types.
#[cfg(target_arch = "aarch64")]
unsafe fn call_compiled(
    worker: *mut Mutator,
    code: usize,
    args: [usize; CALL_ARGS],
) -> (usize, usize, *mut Mutator) {
    let (word, threw, resumed);
    // SAFETY: the caller's contract. The entry keeps every register the C
    // convention has a callee keep, but the pinned one.
    unsafe {
        std::arch::asm!(
            "blr {code}",
            code = in(reg) code,
            inout("x0") args[0] => word,
            inout("x1") args[1] => threw,
            inout("x2") args[2] => _,
            inout("x3") args[3] => _,
            inout("x21") worker => resumed,
            clobber_abi("C"),
        );
    }
    (word, threw, resumed)
}

/// Park the task until `job` has ended on the I/O runtime. Returns the
/// worker the task went on under.
///
/// # Safety
///
/// Called on a task of `worker`.
#[cfg(target_arch = "aarch64")]
unsafe fn wait_for_io(worker: *mut Mutator, job: Job) -> *mut Mutator {
    let job = Box::into_raw(Box::new(job));
    let resumed;
    // SAFETY: the stub keeps every register the C convention has a callee
    // keep, but the pinned one, and takes the job.
    unsafe {
        std::arch::asm!(
            "bl {wait}",
            wait = sym crate::sched::switch::baml_rt_io_wait,
            inout("x0") job => _,
            inout("x21") worker => resumed,
            clobber_abi("C"),
        );
    }
    resumed
}

/// The runtime that runs the jobs of [`Scope::io`], on a thread of its own.
/// Made when the first job is.
pub(crate) fn io_runtime() -> &'static tokio::runtime::Runtime {
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .thread_name("baml-io")
            .enable_all()
            .build()
            .expect("the I/O runtime starts")
    })
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
            (*head).slots.roots().for_each(&mut each);
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
    // SAFETY: the caller's contract: no driver will poll these again.
    unsafe {
        while !(*task).natives.is_null() {
            let head = (*task).natives;
            (*task).natives = (*head).outer;
            ((*head).drop)(head);
        }
    }
}

/// A call of compiled code in flight: see [`Scope::call_baml`].
pub struct CallBaml<'a, 's> {
    scope: &'a mut Scope<'s>,
    /// The call, until the driver has been asked for it.
    ask: Option<(usize, [usize; CALL_ARGS])>,
}

impl<'s> Future for CallBaml<'_, 's> {
    type Output = Result<Elem<'s>, Thrown<'s>>;

    fn poll(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        let exchange = self.scope.exchange;
        // SAFETY: the scope is an `async` native's, so the exchange is its
        // block's, and the driver answers a call before it polls again:
        // with what compiled code returned or threw, and its trace.
        unsafe {
            if let Some((code, args)) = self.ask.take() {
                (*exchange).request = Some(Request::Call { code, args });
                return Poll::Pending;
            }
            let value = self.scope.elem_raw((*exchange).word, TypeId(0));
            Poll::Ready(if (*exchange).threw {
                let context = self.scope.elem_raw((*exchange).context, TypeId(0));
                Err(Thrown {
                    error: value,
                    context: Some(context),
                })
            } else {
                Ok(value)
            })
        }
    }
}

/// A job on the I/O runtime in flight: see [`Scope::io`].
pub struct Io<'a, 's, T> {
    scope: &'a mut Scope<'s>,
    /// The job, until the driver has been asked to run it.
    job: Option<Job>,
    /// Where the job leaves what it made.
    made: Arc<Mutex<Option<T>>>,
}

impl<T> Future for Io<'_, '_, T> {
    type Output = T;

    fn poll(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<T> {
        if let Some(job) = self.job.take() {
            // SAFETY: the scope is an `async` native's, so the exchange is
            // its block's.
            unsafe { (*self.scope.exchange).request = Some(Request::Io(job)) };
            return Poll::Pending;
        }
        let made = self.made.lock().expect("the job did not panic").take();
        Poll::Ready(made.expect("the driver polls again once the job has ended"))
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
    pub fn call_baml<'a>(&'a mut self, function: BamlFn, args: &[Elem<'s>]) -> CallBaml<'a, 's> {
        self.stops("scope.call_baml");
        assert!(
            args.len() <= CALL_ARGS,
            "a native passes a compiled function at most {CALL_ARGS} arguments"
        );
        let mut words = [0; CALL_ARGS];
        for (word, arg) in words.iter_mut().zip(args) {
            *word = arg.get(self);
        }
        CallBaml {
            scope: self,
            ask: Some((function.0, words)),
        }
    }

    /// Run `job` on the I/O runtime, which is Tokio on threads of its own,
    /// and return what it returns. The task is parked meanwhile and other
    /// tasks run.
    ///
    /// The job is `'static` and `Send`: it can hold no root and no view,
    /// so it never touches the BAML heap. What it returns is plain Rust,
    /// which the native makes objects of once it is running again.
    pub fn io<'a, T, J>(&'a mut self, job: J) -> Io<'a, 's, T>
    where
        T: Send + 'static,
        J: Future<Output = T> + Send + 'static,
    {
        self.stops("scope.io");
        let made = Arc::new(Mutex::new(None));
        let slot = Arc::clone(&made);
        Io {
            scope: self,
            job: Some(Box::pin(async move {
                let value = job.await;
                *slot.lock().expect("no thread panics holding it") = Some(value);
            })),
            made,
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
