//! Tasks: what `spawn` starts and `await` waits for.
//!
//! A task is a thread of BAML execution with a stack of its own, run by a
//! worker. The program's entry function runs as one. Compiled code keeps
//! the worker, not the task, in its pinned register: a task that parks may
//! go on under another worker.
//!
//! # One worker
//!
//! There is one worker so far, and it is the [`Mutator`]. What is written
//! for more than one is the layout of a task and of a future, and the
//! states a task moves through, which are read and written atomically:
//!
//! - `RUNNABLE` to `RUNNING` when a worker takes the task to run it.
//! - `RUNNING` to `PARKING` when the task, on its own stack, has put itself
//!   where a waker will find it (a future's waiters, the timers).
//! - `PARKING` to `PARKED` by the worker once it has left the task's stack.
//!   A waker that finds the task `PARKING` cannot resume it, since its
//!   stack is still in use: it makes the task `NOTIFIED`, and the worker
//!   that finds that in place of `PARKING` makes it runnable itself.
//! - `PARKED` to `RUNNABLE` by a waker.
//!
//! # Spawning runs the child first
//!
//! `spawn` switches to the new task at once and leaves the spawning task
//! runnable. The child's first act is to put its parent on the run queue:
//! only then is the parent's stack no longer in use, which is what a second
//! worker taking it would need. A task that ends switches straight to the
//! task queued last, which is its parent unless something else woke since.
//!
//! # Stacks
//!
//! Stacks have a fixed size and a guard below them, and Rust still runs on
//! them: a runtime function is called on the stack of the task that calls
//! it. The entry check of a compiled function keeps
//! [`RUNTIME_STACK`] bytes free for that, and a function entered with less
//! throws `baml.panics.StackOverflow` from its caller on, in that part of
//! the stack.
//!
//! # Errors
//!
//! What a task throws and does not catch arrives at the handler of the
//! function it started in, with its trace: the task's future settles with
//! both, and the collector follows both from the future. An `await` of the
//! future throws them again together, as a handler in the task would have,
//! so a handler around the `await` is found as for any throw and reads the
//! context of the task's throw, not of the `await`. An error no `await`
//! took is reported with that trace once the program is over.
//!
//! # Waiting for another thread
//!
//! A task that waits for I/O parks, and the thread that finishes the I/O
//! puts it in the worker's [`Inbox`]: the one thing here another thread
//! touches. The worker empties the inbox where it looks at its timers, and
//! sleeps on it when it has nothing to run. Whoever empties it wakes the
//! tasks, through the same states as any waker, so several workers can
//! share one.
//!
//! # The collector
//!
//! Every task is on one list, which the collector walks: a task's future
//! and what it is to run are roots, and so is the stack of every task that
//! has one in use, and what the `async` natives it is inside hold. The task running is walked from where it called the
//! runtime, as before. Any other is walked from its switch frame, whose
//! frame record is the top of its chain of compiled frames and whose result
//! word is a root.

pub mod switch;

use std::{
    cmp::Reverse,
    collections::{BinaryHeap, VecDeque},
    io::Write,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, AtomicPtr, AtomicU32, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use switch::{
    SWITCH_FRAME, SWITCH_FRAME_BOOT, SWITCH_FRAME_RECORD, SWITCH_FRAME_RESULT, baml_rt_resume,
    baml_rt_task_boot,
};

use crate::{
    heap::{Mutator, load, store},
    layout::{
        FUTURE_ERROR, FUTURE_FLAGS, FUTURE_OBSERVED, FUTURE_READY, FUTURE_SETTLED, FUTURE_SIZE,
        FUTURE_STATE, FUTURE_TASK, FUTURE_TRACE, FUTURE_VALUE, TYPE_FUTURE, WORD,
    },
};

/// The stack of a spawned task.
const TASK_STACK: usize = 64 << 10;
/// The stack of the task that runs the program's entry function.
const MAIN_STACK: usize = 8 << 20;
/// What is left unmapped below a stack. A multiple of every page size the
/// runtime is built for.
const STACK_GUARD: usize = 16 << 10;
/// The part of a stack compiled code leaves to the runtime functions it
/// calls and to the functions that do not check: the entry check of a
/// compiled function fails below this.
const RUNTIME_STACK: usize = 16 << 10;

/// The worker of this process, for what is handed no worker and cannot
/// find one through a register: a panic hook. Null until the program runs.
/// With several workers this becomes the list of them.
pub(crate) static WORKER: AtomicPtr<Mutator> = AtomicPtr::new(std::ptr::null_mut());

/// Not in use.
const FREE: u32 = 0;
const RUNNABLE: u32 = 1;
const RUNNING: u32 = 2;
const PARKING: u32 = 3;
const PARKED: u32 = 4;
/// Woken while `PARKING`.
const NOTIFIED: u32 = 5;
/// Ended. A task whose error nothing has taken stays so, with its future,
/// until an `await` takes the error or the program ends.
const DEAD: u32 = 6;

/// Set in a task's flags when it runs the program's entry function.
const TASK_MAIN: u32 = 1;
/// Set in a task's flags when it ended by throwing.
const TASK_FAILED: u32 = 2;
/// Where a task's flags say which thread it last stopped on, under
/// migration stress: zero for a task that has not run, and otherwise one
/// more than the thread.
const TASK_STOPPED_ON_SHIFT: u32 = 2;
const TASK_STOPPED_ON: u32 = 3 << TASK_STOPPED_ON_SHIFT;

/// A task. Never freed and never moved, so that a stale address of one is
/// still the address of a task: only reused.
#[repr(C, align(128))]
pub struct Task {
    /// The address of its switch frame while it is not running.
    sp: usize,
    /// The lowest address its stack pointer may have on entry to a compiled
    /// function.
    stack_limit: usize,
    /// The lowest address of its stack.
    stack_lo: usize,
    /// Where its chain of frames ends: just above its first frame.
    stack_hi: usize,
    state: AtomicU32,
    flags: u32,
    /// The next task of the list it is on: the free tasks, or the tasks
    /// waiting for one future.
    link: *mut Task,
    /// The future it settles. A root.
    future: usize,
    /// The closure it is to run, until it starts. A root.
    entry: usize,
    /// Reserved: what a waker hands it.
    wait_value: usize,
    /// The state of the `async` native it is inside, innermost first.
    pub(crate) natives: *mut crate::native::drive::Head,
    /// Reserved: its cancellation state.
    cancel: usize,
    /// Reserved: its telemetry node.
    tele_node: usize,
    /// The list of every task in use.
    all_prev: *mut Task,
    all_next: *mut Task,
    id: u64,
    /// When its timer fires, in nanoseconds since the worker started.
    wake_at: u64,
}

const _: () = assert!(size_of::<Task>() == 128);

/// The state of a future, as every task sees it.
///
/// # Safety
///
/// `future` is a live future.
unsafe fn future_state<'a>(future: usize) -> &'a AtomicUsize {
    // SAFETY: the caller's contract; the word is only ever read and
    // written atomically.
    unsafe { &*((future + FUTURE_STATE) as *const AtomicUsize) }
}

/// The tasks other threads have woken: each parked on I/O that is now
/// done.
pub(crate) struct Inbox {
    /// Whether `woken` holds a task, for a worker to read without the lock.
    filled: AtomicBool,
    woken: Mutex<Vec<usize>>,
    /// Signalled when a task arrives, for a worker asleep on it.
    arrived: Condvar,
}

impl Inbox {
    /// Hand `task` to a worker. Called from any thread, once for each time
    /// the task parked on I/O.
    fn wake(&self, task: usize) {
        let mut woken = self.woken.lock().expect("no thread panics holding it");
        woken.push(task);
        self.filled.store(true, Ordering::Release);
        drop(woken);
        self.arrived.notify_one();
    }
}

/// What one worker schedules with.
pub(crate) struct Sched {
    /// The tasks ready to run. The one queued last runs first.
    queue: VecDeque<*mut Task>,
    /// The sleeping tasks, soonest first: when, the task's id to order
    /// equal times, and the task.
    timers: BinaryHeap<Reverse<(u64, u64, usize)>>,
    /// Every task in use.
    all: *mut Task,
    free: *mut Task,
    /// The task that ended last, whose stack may still be in use by the
    /// code that is switching away from it.
    dead: *mut Task,
    /// The task that spawned the one now starting, to be queued once its
    /// stack is no longer in use.
    pending: *mut Task,
    next_id: u64,
    main_done: bool,
    started: Instant,
    /// Where other threads leave the tasks they wake.
    inbox: Arc<Inbox>,
    /// How many tasks are parked on I/O, each to arrive in the inbox.
    on_io: usize,
    /// Under migration stress, which of the two threads serves as the
    /// worker now: see `serve_in_turns`.
    migrate: Option<u32>,
}

impl Sched {
    pub(crate) fn new() -> Self {
        Self {
            queue: VecDeque::new(),
            timers: BinaryHeap::new(),
            all: std::ptr::null_mut(),
            free: std::ptr::null_mut(),
            dead: std::ptr::null_mut(),
            pending: std::ptr::null_mut(),
            next_id: 0,
            main_done: false,
            started: Instant::now(),
            inbox: Arc::new(Inbox {
                filled: AtomicBool::new(false),
                woken: Mutex::new(Vec::new()),
                arrived: Condvar::new(),
            }),
            on_io: 0,
            migrate: None,
        }
    }

    /// Under migration stress, note that `task` stops running on the
    /// thread serving now: it goes on on the other one.
    ///
    /// # Safety
    ///
    /// `task` is a task in use that this worker is running.
    unsafe fn stops_here(&self, task: *mut Task) {
        if let Some(thread) = self.migrate {
            // SAFETY: the caller's contract. Only the thread serving as
            // the worker reads or writes a task's flags.
            unsafe {
                (*task).flags =
                    ((*task).flags & !TASK_STOPPED_ON) | ((1 + thread) << TASK_STOPPED_ON_SHIFT);
            }
        }
    }

    /// Whether `task` last stopped on the thread serving now, which under
    /// migration stress may not be the one it goes on on.
    ///
    /// # Safety
    ///
    /// `task` is a task in use.
    unsafe fn stopped_here(&self, task: *mut Task) -> bool {
        // SAFETY: the caller's contract.
        let stopped_on = unsafe { (*task).flags & TASK_STOPPED_ON } >> TASK_STOPPED_ON_SHIFT;
        self.migrate.is_some_and(|thread| stopped_on == 1 + thread)
    }

    fn now(&self) -> u64 {
        u64::try_from(self.started.elapsed().as_nanos()).unwrap_or(u64::MAX)
    }

    /// A task in state `RUNNABLE` whose first switch frame starts `shim`.
    fn new_task(&mut self, stack: usize, shim: usize) -> *mut Task {
        let task = if self.free.is_null() || stack != TASK_STACK {
            let lo = map_stack(stack);
            // Two zero words end the chain of frame records for anything
            // that follows it past the task's first frame.
            let hi = lo + stack - 2 * WORD;
            Box::into_raw(Box::new(Task {
                sp: 0,
                stack_limit: lo + RUNTIME_STACK,
                stack_lo: lo,
                stack_hi: hi,
                state: AtomicU32::new(FREE),
                flags: 0,
                link: std::ptr::null_mut(),
                future: 0,
                entry: 0,
                wait_value: 0,
                natives: std::ptr::null_mut(),
                cancel: 0,
                tele_node: 0,
                all_prev: std::ptr::null_mut(),
                all_next: std::ptr::null_mut(),
                id: 0,
                wake_at: 0,
            }))
        } else {
            let task = self.free;
            // SAFETY: the free list holds tasks.
            self.free = unsafe { (*task).link };
            task
        };
        // SAFETY: the task is this function's alone, and its stack is
        // mapped up to two words above `stack_hi`.
        unsafe {
            let task = &mut *task;
            task.id = self.next_id;
            self.next_id += 1;
            task.flags = 0;
            task.link = std::ptr::null_mut();
            let frame = task.stack_hi - SWITCH_FRAME;
            std::ptr::write_bytes(frame as *mut u8, 0, SWITCH_FRAME + 2 * WORD);
            store(frame + SWITCH_FRAME_RECORD, task.stack_hi);
            store(
                frame + SWITCH_FRAME_RECORD + WORD,
                baml_rt_task_boot as *const () as usize,
            );
            store(frame + SWITCH_FRAME_BOOT, shim);
            task.sp = frame;
            task.all_prev = std::ptr::null_mut();
            task.all_next = self.all;
            if !self.all.is_null() {
                (*self.all).all_prev = task;
            }
            self.all = task;
            task.state.store(RUNNABLE, Ordering::Release);
        }
        task
    }

    /// Take `task` off the list of tasks in use and keep it for the next
    /// spawn.
    ///
    /// # Safety
    ///
    /// `task` is dead and nothing runs on its stack.
    unsafe fn release(&mut self, task: *mut Task) {
        // SAFETY: the caller's contract, and the list's links are tasks.
        unsafe {
            let (prev, next) = ((*task).all_prev, (*task).all_next);
            if prev.is_null() {
                self.all = next;
            } else {
                (*prev).all_next = next;
            }
            if !next.is_null() {
                (*next).all_prev = prev;
            }
            (*task).future = 0;
            (*task).entry = 0;
            // A task that ends runs its natives to their ends first. One
            // that is ended from outside has their state dropped here.
            crate::native::drive::drop_all(task);
            (*task).state.store(FREE, Ordering::Release);
            if (*task).stack_hi - (*task).stack_lo + 2 * WORD == TASK_STACK {
                (*task).link = self.free;
                self.free = task;
            }
        }
    }

    /// What every entry into the scheduler does first, now that the worker
    /// is off the stack it last left: the task that ended there goes back
    /// to the pool, unless it holds an error nothing has taken yet.
    fn settle_last_switch(&mut self) {
        let dead = std::mem::replace(&mut self.dead, std::ptr::null_mut());
        // SAFETY: `dead` is null or a task that has ended and been left.
        unsafe {
            if !dead.is_null() && (*dead).flags & TASK_FAILED == 0 {
                self.release(dead);
            }
        }
    }

    /// Make a parked task runnable, or tell the worker still parking it to.
    /// It is queued to run before everything already queued if `next`, and
    /// after all of it otherwise.
    ///
    /// # Safety
    ///
    /// `task` is a task that parked and has not been woken since.
    unsafe fn wake(&mut self, task: *mut Task, next: bool) {
        // SAFETY: the caller's contract.
        let state = unsafe { &(*task).state };
        loop {
            match state.compare_exchange(PARKED, RUNNABLE, Ordering::AcqRel, Ordering::Acquire) {
                Ok(_) => {
                    if next {
                        self.queue.push_back(task);
                    } else {
                        self.queue.push_front(task);
                    }
                    return;
                }
                Err(PARKING) => {
                    if state
                        .compare_exchange(PARKING, NOTIFIED, Ordering::AcqRel, Ordering::Acquire)
                        .is_ok()
                    {
                        return;
                    }
                }
                Err(other) => unreachable!("a woken task is parked, not in state {other}"),
            }
        }
    }

    /// Wake every task whose timer has fired: they run next, the soonest
    /// first, ahead of what was already queued. The queue is served from
    /// its back, where a task that spawns and one that ends keep handing
    /// over to each other, so a woken task put at its front would wait for
    /// as long as they do.
    fn fire_timers(&mut self) {
        // The clock is read only when a task waits on it.
        if self.timers.is_empty() {
            return;
        }
        let now = self.now();
        let mut fired = Vec::new();
        while let Some(Reverse((when, _, task))) = self.timers.peek().copied() {
            if when > now {
                break;
            }
            self.timers.pop();
            fired.push(task as *mut Task);
        }
        for task in fired.into_iter().rev() {
            // SAFETY: a task with a timer parked on it.
            unsafe { self.wake(task, true) };
        }
    }

    /// Wake every task another thread has put in the inbox. They run next,
    /// as the tasks a timer wakes do.
    fn take_woken(&mut self) {
        if !self.inbox.filled.load(Ordering::Acquire) {
            return;
        }
        let woken = {
            let mut woken = self
                .inbox
                .woken
                .lock()
                .expect("no thread panics holding it");
            self.inbox.filled.store(false, Ordering::Release);
            std::mem::take(&mut *woken)
        };
        self.on_io -= woken.len();
        for task in woken {
            // SAFETY: a task is put in the inbox once it has parked on
            // I/O, and once for each time.
            unsafe { self.wake(task as *mut Task, true) };
        }
    }

    /// With nothing to run: sleep until the next timer fires or a task
    /// arrives in the inbox, whichever is first. Returns false when
    /// neither can happen.
    fn sleep(&mut self) -> bool {
        let until = self.timers.peek().map(|Reverse((when, ..))| *when);
        if until.is_none() && self.on_io == 0 {
            return false;
        }
        let woken = self
            .inbox
            .woken
            .lock()
            .expect("no thread panics holding it");
        if !woken.is_empty() {
            return true;
        }
        // Woken early or for nothing, the worker's loop looks again.
        match until {
            Some(when) => {
                let wait = Duration::from_nanos(when.saturating_sub(self.now()));
                drop(self.inbox.arrived.wait_timeout(woken, wait));
            }
            None => drop(self.inbox.arrived.wait(woken)),
        }
        true
    }

    /// The tasks in use.
    pub(crate) fn tasks(&self) -> impl Iterator<Item = *mut Task> {
        let mut next = self.all;
        std::iter::from_fn(move || {
            let task = next;
            if task.is_null() {
                return None;
            }
            // SAFETY: the list's links are tasks.
            next = unsafe { (*task).all_next };
            Some(task)
        })
    }
}

/// A task `worker` is running, for a test of what runs on one. It has a
/// stack, which the test does not switch to.
#[cfg(test)]
pub(crate) fn task_for_tests(worker: &mut Mutator) -> *mut Task {
    let task = worker.sched.new_task(TASK_STACK, 0);
    worker.cur_task = task;
    task
}

/// What the collector reads of a task.
pub(crate) struct TaskRoots {
    /// The addresses of the task's own references.
    pub(crate) slots: [usize; 2],
    /// When the task's stack is in use and the task is not the one running:
    /// the address of the result word of its switch frame, which is a
    /// reference, the stack pointer, return address and frame pointer of
    /// its top compiled frame, and where its chain of frames ends.
    pub(crate) stack: Option<(usize, [usize; 3], usize)>,
}

/// # Safety
///
/// `task` is a task in use, and `running` the worker's current task.
pub(crate) unsafe fn task_roots(task: *mut Task, running: *mut Task) -> TaskRoots {
    // SAFETY: the caller's contract; a task that is not running and not
    // dead has a switch frame at its saved stack pointer.
    unsafe {
        let state = (*task).state.load(Ordering::Acquire);
        let parked = task != running && matches!(state, RUNNABLE | PARKING | PARKED | NOTIFIED);
        let sp = (*task).sp;
        TaskRoots {
            slots: [
                &raw mut (*task).future as usize,
                &raw mut (*task).entry as usize,
            ],
            stack: parked.then(|| {
                (
                    sp + SWITCH_FRAME_RESULT,
                    [
                        sp + SWITCH_FRAME,
                        load(sp + SWITCH_FRAME_RECORD + WORD),
                        load(sp + SWITCH_FRAME_RECORD),
                    ],
                    (*task).stack_hi,
                )
            }),
        }
    }
}

/// Where the chain of compiled frames of `task` ends.
///
/// # Safety
///
/// `task` is a task in use.
pub(crate) unsafe fn stack_base(task: *mut Task) -> usize {
    // SAFETY: the caller's contract.
    unsafe { (*task).stack_hi }
}

unsafe extern "C" {
    fn mmap(
        address: *mut u8,
        length: usize,
        protection: i32,
        flags: i32,
        file: i32,
        offset: i64,
    ) -> *mut u8;
    fn mprotect(address: *mut u8, length: usize, protection: i32) -> i32;
}

const PROT_NONE: i32 = 0;
const PROT_READ_WRITE: i32 = 3;
const MAP_PRIVATE: i32 = 2;
#[cfg(target_vendor = "apple")]
const MAP_ANONYMOUS: i32 = 0x1000;
#[cfg(not(target_vendor = "apple"))]
const MAP_ANONYMOUS: i32 = 0x20;

/// Map a stack of `size` bytes above a guard, and return its lowest
/// address.
fn map_stack(size: usize) -> usize {
    // SAFETY: a fresh anonymous mapping, and a protection change inside it.
    unsafe {
        let base = mmap(
            std::ptr::null_mut(),
            STACK_GUARD + size,
            PROT_READ_WRITE,
            MAP_PRIVATE | MAP_ANONYMOUS,
            -1,
            0,
        );
        assert!(
            base as isize != -1 && !base.is_null(),
            "out of memory: cannot map a {size} byte task stack"
        );
        assert!(
            mprotect(base, STACK_GUARD, PROT_NONE) == 0,
            "cannot protect the guard of a task stack"
        );
        base as usize + STACK_GUARD
    }
}

/// What a task may have a return address into when it stops, beside
/// compiled code: each stub by name, and the address in it.
#[cfg(checks)]
fn stubs_under_a_stopped_task() -> [(&'static str, usize); 1] {
    [(
        "baml_rt_task_boot",
        baml_rt_task_boot as *const () as usize + switch::TASK_BOOT_RETURN,
    )]
}

/// Check, where the task running on `worker` stops, that nothing but
/// compiled code is on its stack: every return address, from the one into
/// the function that called the stub whose switch frame is at `frame` to
/// the one the task's first function returns to, is in compiled code or is
/// one of [`stubs_under_a_stopped_task`].
///
/// A task that stops may go on on another thread and under another worker.
/// A Rust function still on its stack then goes on with whatever the
/// compiler chose to keep of the thread and the worker it stopped on, in a
/// register or a stack slot, which no reading of the source can rule out.
/// So no Rust frame is there at all: every function of the runtime returns
/// before its task can stop, and the stubs that switch stacks are the only
/// code that is not compiled BAML between a task's first frame and its
/// last.
///
/// # Safety
///
/// `frame` is a switch frame on the stack of the task `worker` is running.
#[cfg(checks)]
unsafe fn only_compiled_code_stops(worker: &Mutator, frame: usize) {
    use std::ffi::{CStr, c_char, c_void};

    #[repr(C)]
    struct Symbol {
        file: *const c_char,
        base: *mut c_void,
        name: *const c_char,
        address: *mut c_void,
    }
    unsafe extern "C" {
        fn dladdr(address: *const c_void, symbol: *mut Symbol) -> i32;
    }

    let stubs = stubs_under_a_stopped_task();
    // SAFETY: the caller's contract: frame records chain from the switch
    // frame to the task's first frame, each the frame pointer of the
    // caller and then the address the function returns to.
    unsafe {
        let base = stack_base(worker.cur_task);
        let mut record = frame + SWITCH_FRAME_RECORD;
        loop {
            let (caller, returns_to) = (load(record), load(record + WORD));
            // A worker of a program with no compiled code is a test's,
            // whose tasks are Rust functions.
            let Some(compiled) = worker.unwind.is_compiled(returns_to) else {
                return;
            };
            if !compiled && !stubs.iter().any(|(_, address)| *address == returns_to) {
                let mut symbol = std::mem::zeroed::<Symbol>();
                let name = if dladdr(returns_to as *const c_void, &raw mut symbol) != 0
                    && !symbol.name.is_null()
                {
                    CStr::from_ptr(symbol.name).to_string_lossy().into_owned()
                } else {
                    "a function with no name".to_owned()
                };
                let allowed: Vec<&str> = stubs.iter().map(|(name, _)| *name).collect();
                fatal(&format!(
                    "a task stops with a frame on its stack that is not compiled code: it \
                     returns to {returns_to:#x}, in `{name}`. A function of the runtime returns \
                     before its task can stop; the only stubs a stopped task returns into are: {}",
                    allowed.join(", ")
                ));
            }
            if caller >= base {
                return;
            }
            record = caller;
        }
    }
}

#[expect(clippy::exit, reason = "the scheduler cannot go on")]
fn fatal(message: &str) -> ! {
    let _ = std::io::stdout().flush();
    let _ = writeln!(std::io::stderr().lock(), "fatal: {message}");
    std::process::exit(70)
}

/// The Rust half of [`switch::baml_rt_spawn`], on the spawning task's
/// stack: make the future and the task that settles it, and leave the
/// spawning task ready to be resumed with the future. Returns the new task,
/// which the stub switches to.
///
/// `frame` is the switch frame the stub built; `shim` the compiled function
/// the new task starts in, which asks for `closure` back.
pub(crate) unsafe extern "C" fn spawn_prepare(
    mut closure: usize,
    shim: usize,
    flags: usize,
    worker: &mut Mutator,
    frame: usize,
) -> *mut Task {
    worker.sched.settle_last_switch();
    // SAFETY: the stub's frame, on the spawning task's stack.
    #[cfg(checks)]
    unsafe {
        only_compiled_code_stops(worker, frame);
    }
    worker.enter_switch_frame(frame);
    let child = worker.sched.new_task(TASK_STACK, shim);
    worker.root(&mut closure);
    let future = worker.allocate_object(TYPE_FUTURE, FUTURE_SIZE);
    worker.unroot(1);
    // SAFETY: the future is the object just allocated, the child is this
    // function's until it is switched to, and `frame` is the stub's.
    unsafe {
        store(future + FUTURE_STATE, 0);
        store(future + FUTURE_VALUE, 0);
        store(future + FUTURE_TASK, child as usize);
        store(future + FUTURE_FLAGS, flags);
        store(future + FUTURE_TRACE, 0);
        (*child).future = future;
        (*child).entry = closure;
        store(frame + SWITCH_FRAME_RESULT, future);
        let parent = worker.cur_task;
        (*parent).sp = frame;
        worker.sched.stops_here(parent);
        (*parent).state.store(RUNNABLE, Ordering::Release);
        worker.sched.pending = parent;
        (*child).state.store(RUNNING, Ordering::Release);
    }
    child
}

/// A new task's first call: its spawner's stack is no longer in use, so
/// the spawner can be queued. Returns the closure the task is to run.
#[unsafe(no_mangle)]
pub extern "C" fn baml_rt_task_start(worker: &mut Mutator) -> usize {
    worker.sched.settle_last_switch();
    let pending = std::mem::replace(&mut worker.sched.pending, std::ptr::null_mut());
    if !pending.is_null() {
        worker.sched.queue.push_back(pending);
    }
    // SAFETY: called on a task.
    unsafe { std::mem::take(&mut (*worker.cur_task).entry) }
}

/// Settle the future of the task running on `worker` as `state` with
/// `value` and, for an error, the `trace` it was thrown with, and wake what
/// waits for it.
fn settle(worker: &mut Mutator, value: usize, trace: usize, state: usize) {
    // SAFETY: called on a task, whose future is live.
    unsafe {
        let future = (*worker.cur_task).future;
        store(future + FUTURE_VALUE, value);
        store(future + FUTURE_TRACE, trace);
        if state == FUTURE_READY {
            // Nothing asks a task that returned for anything more.
            store(future + FUTURE_TASK, 0);
        }
        let before = future_state(future).swap(state, Ordering::AcqRel);
        let mut waiter = (before & !FUTURE_SETTLED) as *mut Task;
        while !waiter.is_null() {
            let next = (*waiter).link;
            worker.sched.wake(waiter, true);
            waiter = next;
        }
    }
}

/// The closure of the task running on `worker` returned `value`, the bits
/// of one slot.
#[unsafe(no_mangle)]
pub extern "C" fn baml_rt_task_finish(worker: &mut Mutator, value: usize) {
    settle(worker, value, 0, FUTURE_READY);
}

/// The closure of the task running on `worker` threw `error` and nothing
/// in the task caught it. `trace` is what the task's handler landed with:
/// the future keeps it beside the error, for an `await` to throw the two
/// again as a handler in the task would have, and for the report of an
/// error no `await` took.
#[unsafe(no_mangle)]
pub extern "C" fn baml_rt_task_fail(worker: &mut Mutator, error: usize, trace: usize) {
    // SAFETY: called on a task.
    unsafe { (*worker.cur_task).flags |= TASK_FAILED };
    settle(worker, error, trace, FUTURE_ERROR);
}

/// Run `run` on the worker's own stack, whose top free address is `stack`,
/// and return what it returns: for a runtime function that recurses past
/// what a task's stack keeps for the runtime ([`RUNTIME_STACK`]). `run`
/// calls no compiled code and switches to no task. A collection inside it
/// finds the task's frames as it does from any runtime function: from the
/// compiled frame that called the runtime.
///
/// # Safety
///
/// A task is running, and `stack` is where the worker left its own stack
/// for it.
pub(crate) unsafe fn on_stack<F: FnOnce() -> R, R>(stack: usize, run: F) -> R {
    struct Call<F, R> {
        run: Option<F>,
        result: Option<R>,
    }
    unsafe extern "C" fn enter<F: FnOnce() -> R, R>(data: *mut u8) {
        // SAFETY: `data` is the `Call` below, which nothing else reads
        // until this returns.
        let call = unsafe { &mut *data.cast::<Call<F, R>>() };
        if let Some(run) = call.run.take() {
            call.result = Some(run());
        }
    }
    let mut call = Call {
        run: Some(run),
        result: None,
    };
    // Below the frame the worker left, and aligned as a call needs.
    let stack = (stack - 256) & !15;
    // SAFETY: the caller's contract: the worker's stack is not in use
    // below the frame it left while one of its tasks runs.
    unsafe { switch::call_on(std::ptr::from_mut(&mut call).cast(), enter::<F, R>, stack) };
    call.result.expect("the call ran")
}

/// Whether a throw can be raised with `sp` the lowest address of the
/// running task's stack in use: whether most of the part of the stack kept
/// for the runtime is still below it.
pub(crate) fn has_room_to_throw(worker: &Mutator, sp: usize) -> bool {
    if worker.cur_task.is_null() {
        return true;
    }
    // SAFETY: the current task is a task in use.
    let lowest = unsafe { (*worker.cur_task).stack_lo };
    sp.saturating_sub(lowest) >= RUNTIME_STACK - RUNTIME_STACK / 4
}

/// Report every error no `await` took, as `baml run` does once the program
/// is over, in the order their tasks were spawned in, and say whether there
/// was one.
pub(crate) fn report_unhandled(worker: &Mutator) -> bool {
    // SAFETY: the tasks in use.
    let mut failed: Vec<*mut Task> = worker
        .sched
        .tasks()
        .filter(|task| unsafe { (**task).flags & TASK_FAILED != 0 })
        .collect();
    failed.sort_unstable_by_key(|task| unsafe { (**task).id });
    for task in &failed {
        // SAFETY: a task that failed keeps its future, which settled with
        // the error and its trace.
        unsafe {
            let future = (**task).future;
            crate::unwind::report(
                worker,
                "unhandled spawned task failed: ",
                load(future + FUTURE_VALUE),
                load(future + FUTURE_TRACE),
            );
        }
    }
    !failed.is_empty()
}

/// The task running on `worker` is over: called by its boot code, still on
/// its stack. Returns the task to switch to, or null to go back to the
/// worker's own stack.
pub(crate) unsafe extern "C" fn exit_pick(worker: &mut Mutator) -> *mut Task {
    let sched = &mut worker.sched;
    sched.settle_last_switch();
    let task = worker.cur_task;
    // SAFETY: called on a task; a queued task is runnable.
    unsafe {
        if (*task).flags & TASK_MAIN != 0 {
            sched.main_done = true;
        }
        (*task).state.store(DEAD, Ordering::Release);
        sched.dead = task;
        // A run of tasks that each end by handing over to the next never
        // returns to the worker's loop, so the timers are looked at here
        // too: a sleeper is not kept waiting until the run is over. So is
        // the inbox.
        sched.take_woken();
        sched.fire_timers();
        match sched.queue.pop_back() {
            // Under migration stress a task that stopped on this thread
            // is left for the worker's loop to hand to the other one.
            Some(next) if sched.stopped_here(next) => {
                sched.queue.push_back(next);
                std::ptr::null_mut()
            }
            Some(next) => {
                (*next).state.store(RUNNING, Ordering::Release);
                next
            }
            None => std::ptr::null_mut(),
        }
    }
}

/// The Rust half of [`switch::baml_rt_await`], on the awaiting task's
/// stack. Returns zero when the future has settled, and otherwise leaves
/// the task among the future's waiters, parking, for the stub to switch
/// away from.
pub(crate) unsafe extern "C" fn await_park(
    future: usize,
    worker: &mut Mutator,
    frame: usize,
) -> usize {
    worker.sched.settle_last_switch();
    let task = worker.cur_task;
    // SAFETY: called on a task with a live future and the stub's frame.
    unsafe {
        let state = future_state(future);
        let mut seen = state.load(Ordering::Acquire);
        if seen & FUTURE_SETTLED != 0 {
            return 0;
        }
        #[cfg(checks)]
        only_compiled_code_stops(worker, frame);
        (*task).sp = frame;
        (*task).state.store(PARKING, Ordering::Release);
        loop {
            (*task).link = seen as *mut Task;
            match state.compare_exchange(seen, task as usize, Ordering::AcqRel, Ordering::Acquire) {
                Ok(_) => return 1,
                Err(now) if now & FUTURE_SETTLED != 0 => {
                    (*task).state.store(RUNNING, Ordering::Release);
                    return 0;
                }
                Err(now) => seen = now,
            }
        }
    }
}

/// The error an awaited future settled with, for the `await` to throw.
/// The error now has someone to handle it, so the task that threw it is
/// done with.
///
/// # Safety
///
/// `future` is a future that settled with an error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_rt_await_error(worker: &mut Mutator, future: usize) -> usize {
    worker.sched.settle_last_switch();
    // SAFETY: the caller's contract; a failed future names its task until
    // its error is taken.
    unsafe {
        store(
            future + FUTURE_FLAGS,
            load(future + FUTURE_FLAGS) | FUTURE_OBSERVED,
        );
        let task = load(future + FUTURE_TASK) as *mut Task;
        if !task.is_null() {
            store(future + FUTURE_TASK, 0);
            worker.sched.release(task);
        }
        load(future + FUTURE_VALUE)
    }
}

/// The Rust half of [`switch::baml_rt_sleep`], on the sleeping task's
/// stack. Returns zero when there is nothing to wait for, and otherwise
/// leaves the task among the timers, parking.
pub(crate) unsafe extern "C" fn sleep_park(
    nanos: i64,
    worker: &mut Mutator,
    frame: usize,
) -> usize {
    worker.sched.settle_last_switch();
    if nanos <= 0 {
        return 0;
    }
    // SAFETY: the stub's frame, on the sleeping task's stack.
    #[cfg(checks)]
    unsafe {
        only_compiled_code_stops(worker, frame);
    }
    let sched = &mut worker.sched;
    let task = worker.cur_task;
    let when = sched.now().saturating_add(nanos.cast_unsigned());
    // SAFETY: called on a task with the stub's frame.
    unsafe {
        (*task).sp = frame;
        (*task).wake_at = when;
        (*task).state.store(PARKING, Ordering::Release);
        sched
            .timers
            .push(Reverse((when, (*task).id, task as usize)));
    }
    1
}

/// What a task parked on I/O waits for: it ends when the I/O has, having
/// left its result where the task will look.
pub(crate) type Job = std::pin::Pin<Box<dyn Future<Output = ()> + Send>>;

/// A job, and what starts it somewhere other than on a worker. The worker
/// names no runtime of its own for that: whoever makes the job says where
/// it runs.
pub(crate) struct IoWait {
    pub(crate) job: Job,
    pub(crate) start: fn(Job, Wake),
}

/// What hands a task parked on I/O back to a worker, once.
pub(crate) struct Wake {
    inbox: Arc<Inbox>,
    task: usize,
}

impl Wake {
    /// Called from any thread, when the job has ended.
    pub(crate) fn wake(self) {
        self.inbox.wake(self.task);
    }
}

/// The Rust half of [`switch::baml_rt_io_wait`], on the waiting task's
/// stack: start the job `wait` is the address of, taking it from there,
/// and leave the task parking until the job has ended.
///
/// The job starts only once the task is parking, so that whichever worker
/// finds it in the inbox finds it in a state a waker knows.
pub(crate) unsafe extern "C" fn io_park(wait: usize, worker: &mut Mutator, frame: usize) -> usize {
    worker.sched.settle_last_switch();
    // SAFETY: the stub's frame, on the waiting task's stack.
    #[cfg(checks)]
    unsafe {
        only_compiled_code_stops(worker, frame);
    }
    let sched = &mut worker.sched;
    let task = worker.cur_task;
    // SAFETY: called on a task with the stub's frame, and with the address
    // of the wait its native asked for.
    let waits_for = unsafe {
        (*task).sp = frame;
        (*task).state.store(PARKING, Ordering::Release);
        (*(wait as *mut Option<IoWait>)).take()
    };
    let IoWait { job, start } = waits_for.expect("a wait is for what a native asked for, once");
    sched.on_io += 1;
    start(
        job,
        Wake {
            inbox: Arc::clone(&sched.inbox),
            task: task as usize,
        },
    );
    1
}

/// A `spawn` whose plan asks for what tasks cannot do yet.
#[unsafe(no_mangle)]
pub extern "C" fn baml_rt_spawn_unsupported() -> ! {
    fatal("a spawn with semaphores or cancel tokens is not supported by native code yet")
}

/// Run the program: `entry` on a task of its own, and every task it spawns,
/// until none is left that can run. Returns the exit status of the process.
/// With `migrate`, every task that stops goes on on another thread and
/// under a worker at another address: see `serve_in_turns`. Only the
/// runtime a test build links can, since a second thread costs every start
/// of a program about 0.6 M instructions for the system symbols it imports,
/// whether or not it is ever started.
///
/// # Safety
///
/// Called once, on the thread `worker` belongs to, with the address of the
/// compiled program's entry function.
pub unsafe fn run(worker: *mut Mutator, entry: usize, migrate: bool) -> i32 {
    // SAFETY: the caller's contract. The worker is reached through its
    // address here because tasks reach it through their pinned register in
    // between.
    unsafe {
        WORKER.store(worker, Ordering::Release);
        let main = (*worker).sched.new_task(MAIN_STACK, entry);
        (*main).flags = TASK_MAIN;
        (*worker).sched.queue.push_back(main);
        if migrate {
            #[cfg(checks)]
            serve_in_turns(worker);
            #[cfg(not(checks))]
            fatal("migration stress is in the runtime a test build links, and this is not one");
        } else {
            serve(worker);
        }
        if !(*worker).sched.main_done {
            fatal("deadlock: every task waits for another");
        }
        // Every task has ended or waits for good. An error no `await` took
        // is reported now, as `baml run` does after the program's result.
        i32::from(report_unhandled(&*worker))
    }
}

/// Serve as `worker` on this thread: run every task that can run. Returns
/// false once none is left. Under migration stress it returns true when
/// the task to run next last stopped on this thread: the task is back on
/// the queue, for the other thread to take.
///
/// # Safety
///
/// Called on the stack of the thread that serves as `worker`.
unsafe fn serve(worker: *mut Mutator) -> bool {
    // SAFETY: the caller's contract.
    unsafe {
        loop {
            let sched = &mut (*worker).sched;
            sched.settle_last_switch();
            sched.take_woken();
            sched.fire_timers();
            if let Some(task) = sched.queue.pop_back() {
                if sched.stopped_here(task) {
                    sched.queue.push_back(task);
                    return true;
                }
                (*task).state.store(RUNNING, Ordering::Release);
                baml_rt_resume(worker, task);
                // Back on the worker's own stack: the task that left it is
                // off its stack now.
                let left = std::mem::replace(&mut (*worker).cur_task, std::ptr::null_mut());
                (*worker).sched.stops_here(left);
                if (*left).state.compare_exchange(
                    PARKING,
                    PARKED,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                ) == Err(NOTIFIED)
                {
                    (*left).state.store(RUNNABLE, Ordering::Release);
                    (*worker).sched.queue.push_back(left);
                }
                continue;
            }
            if !sched.sleep() {
                return false;
            }
        }
    }
}

/// What the two threads of migration stress hand each other.
#[cfg(checks)]
struct Baton {
    /// Which of the two serves as the worker.
    holder: u32,
    /// Where the worker is.
    worker: usize,
    /// No task is left that can run.
    over: bool,
}

/// Migration stress: serve as the worker at `home` on two threads in
/// turns, so that every task that stops goes on on the other thread than
/// the one it stopped on, and under a worker at another address.
///
/// A task may be resumed by any worker once there are several. Whatever a
/// stopped task still holds of the thread or the worker it stopped on is
/// then stale, and here it is stale every time: the thread is another, and
/// the worker has moved to the other of two places, the one it left
/// unreadable. Only one thread runs at a time, so this finds what is held
/// across a stop and nothing that needs two workers running at once.
///
/// # Safety
///
/// As [`serve`], with nothing holding the address of the worker but the
/// caller, which finds the worker at `home` again afterwards.
#[cfg(checks)]
unsafe fn serve_in_turns(home: *mut Mutator) {
    const PLACE: usize = STACK_GUARD;
    const _: () = assert!(size_of::<Mutator>() <= PLACE);
    /// Move the worker from `from` to `to`, which is readable.
    unsafe fn move_worker(from: *mut Mutator, to: *mut Mutator, thread: u32) {
        // SAFETY: both are places for a worker, and the one at `from` is
        // not used again until one is moved there.
        unsafe {
            std::ptr::copy_nonoverlapping(from, to, 1);
            (*to).sched.migrate = Some(thread);
        }
        WORKER.store(to, Ordering::Release);
    }
    let protect = |place: usize, protection: i32| {
        // SAFETY: a mapping of this function's own.
        let changed = unsafe { mprotect(place as *mut u8, PLACE, protection) };
        assert!(changed == 0, "cannot protect a place for the worker");
    };
    // `map_stack` leaves a guard below what it maps, which is not needed
    // here and costs nothing.
    let places = [map_stack(PLACE), map_stack(PLACE)];
    // SAFETY: the caller's contract.
    unsafe { move_worker(home, places[0] as *mut Mutator, 0) };
    protect(places[1], PROT_NONE);
    let baton = Mutex::new(Baton {
        holder: 0,
        worker: places[0],
        over: false,
    });
    let passed = Condvar::new();
    let take_turns = |me: u32| {
        loop {
            let worker = {
                let mut baton = baton.lock().expect("no thread panics holding it");
                while baton.holder != me && !baton.over {
                    baton = passed.wait(baton).expect("no thread panics holding it");
                }
                if baton.over {
                    return;
                }
                baton.worker as *mut Mutator
            };
            // SAFETY: this thread holds the baton, so it alone serves as
            // the worker, which is where the baton says.
            let more = unsafe { serve(worker) };
            let mut baton = baton.lock().expect("no thread panics holding it");
            if more {
                let other = 1 - me;
                let (from, to) = (places[me as usize], places[other as usize]);
                protect(to, PROT_READ_WRITE);
                // SAFETY: the worker runs nothing, and nothing but this
                // thread knows where it is.
                unsafe { move_worker(worker, to as *mut Mutator, other) };
                protect(from, PROT_NONE);
                *baton = Baton {
                    holder: other,
                    worker: to,
                    over: false,
                };
            } else {
                baton.over = true;
            }
            drop(baton);
            passed.notify_all();
        }
    };
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .name("baml-worker-2".to_owned())
            .stack_size(MAIN_STACK)
            .spawn_scoped(scope, || take_turns(1))
            .expect("a second thread starts");
        take_turns(0);
    });
    let worker = baton.lock().expect("no thread panics holding it").worker;
    // SAFETY: both threads are done, and the worker is where the baton
    // says.
    unsafe {
        std::ptr::copy_nonoverlapping(worker as *const Mutator, home, 1);
        (*home).sched.migrate = None;
    }
    WORKER.store(home, Ordering::Release);
}

#[cfg(all(test, target_arch = "aarch64"))]
pub(crate) mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::{heap::Program, layout::RUNTIME_TYPES};

    /// A stack map table with no functions.
    static NO_STACK_MAPS: usize = 0;
    /// What the tasks of the test did, in the order they did it.
    static EVENTS: Mutex<Vec<usize>> = Mutex::new(Vec::new());

    /// Call a parking stub as compiled code does: with the worker in the
    /// pinned register. Returns the worker the task went on under.
    unsafe fn park(
        stub: unsafe extern "C" fn(),
        argument: usize,
        worker: *mut Mutator,
    ) -> *mut Mutator {
        let resumed: *mut Mutator;
        // SAFETY: the stub keeps every register the C convention has a
        // callee keep, and the caller is a task of `worker`.
        unsafe {
            std::arch::asm!(
                "blr {stub}",
                stub = in(reg) stub,
                in("x0") argument,
                inout("x21") worker => resumed,
                clobber_abi("C"),
            );
        }
        resumed
    }

    /// Sleeps for as long as its task was told to, and then settles its
    /// task's future, if it has one, with that.
    unsafe extern "C" fn sleeper(worker: *mut Mutator) {
        // SAFETY: called by the boot code on a task of `worker`.
        unsafe {
            let task = (*worker).cur_task;
            let nanos = (*task).wait_value;
            let worker = park(switch::baml_rt_sleep, nanos, worker);
            EVENTS.lock().expect("no test panicked").push(nanos);
            if (*task).future != 0 {
                baml_rt_task_finish(&mut *worker, nanos);
            }
        }
    }

    /// Awaits the future its task was handed.
    unsafe extern "C" fn awaiter(worker: *mut Mutator) {
        // SAFETY: called by the boot code on a task of `worker`.
        unsafe {
            let future = (*(*worker).cur_task).wait_value;
            park(switch::baml_rt_await, future, worker);
            assert_eq!(
                future_state(future).load(Ordering::Acquire),
                FUTURE_READY,
                "an await returns once the future has settled"
            );
            EVENTS
                .lock()
                .expect("no test panicked")
                .push(1 + load(future + FUTURE_VALUE));
        }
    }

    unsafe extern "C" fn returns_at_once(_worker: *mut Mutator) {}

    /// A function a task starts in, named `$name`, that runs `$body` as
    /// compiled code would: `$body` takes the worker and returns the one
    /// its task went on under, which is in the pinned register when the
    /// function returns. A Rust function alone cannot be one under
    /// migration stress: it gives its caller back the register it was
    /// called with.
    macro_rules! task_function {
        ($name:ident => $body:path) => {
            #[unsafe(naked)]
            unsafe extern "C" fn $name() {
                core::arch::naked_asm!(
                    "stp x29, x30, [sp, #-16]!",
                    "mov x29, sp",
                    "mov x0, x21",
                    "bl {body}",
                    "mov x21, x0",
                    "ldp x29, x30, [sp], #16",
                    "ret",
                    body = sym $body,
                )
            }
        };
    }
    pub(crate) use task_function;

    /// Whether a task that slept went on on another thread, and under a
    /// worker at another address.
    static MOVED: Mutex<Vec<(bool, bool)>> = Mutex::new(Vec::new());

    unsafe extern "C" fn sleeps_and_looks(worker: *mut Mutator) -> *mut Mutator {
        let thread = std::thread::current().id();
        // SAFETY: called on a task of `worker`.
        let resumed = unsafe { park(switch::baml_rt_sleep, 1_000_000, worker) };
        MOVED
            .lock()
            .expect("no test panicked")
            .push((std::thread::current().id() != thread, resumed != worker));
        resumed
    }
    task_function!(sleeps_and_looks_task => sleeps_and_looks);

    #[test]
    fn under_migration_stress_a_task_goes_on_elsewhere() {
        for migrate in [false, true] {
            let program = Program {
                stack_maps: std::ptr::from_ref(&NO_STACK_MAPS) as usize,
                types: RUNTIME_TYPES.to_vec().leak().as_ptr() as usize,
                unwind: 0,
                functions: 0,
            };
            // SAFETY: both tables are in the compiler's formats.
            let status = unsafe {
                let mut worker = Mutator::new(program, false);
                let entry = sleeps_and_looks_task as *const () as usize;
                run(&raw mut worker, entry, migrate)
            };
            assert_eq!(status, 0);
            let moved = std::mem::take(&mut *MOVED.lock().expect("no test panicked"));
            assert_eq!(moved, [(migrate, migrate)]);
        }
    }

    #[test]
    fn tasks_park_and_wake() {
        const MILLISECOND: usize = 1_000_000;
        let program = Program {
            stack_maps: std::ptr::from_ref(&NO_STACK_MAPS) as usize,
            types: RUNTIME_TYPES.to_vec().leak().as_ptr() as usize,
            unwind: 0,
            functions: 0,
        };
        // SAFETY: both tables are in the compiler's formats, the tasks are
        // made before anything runs, and the future is a fresh object.
        let status = unsafe {
            let mut worker = Mutator::new(program, false);
            let future = worker.allocate_object(TYPE_FUTURE, FUTURE_SIZE);
            for field in [
                FUTURE_STATE,
                FUTURE_VALUE,
                FUTURE_TASK,
                FUTURE_FLAGS,
                FUTURE_TRACE,
            ] {
                store(future + field, 0);
            }
            // The shortest sleep starts first, so that no delay between
            // the starts can reorder the three.
            for milliseconds in [10, 20, 30] {
                let task = worker
                    .sched
                    .new_task(TASK_STACK, sleeper as *const () as usize);
                (*task).wait_value = milliseconds * MILLISECOND;
                if milliseconds == 20 {
                    (*task).future = future;
                }
                worker.sched.queue.push_front(task);
            }
            let waiting = worker
                .sched
                .new_task(TASK_STACK, awaiter as *const () as usize);
            (*waiting).wait_value = future;
            worker.sched.queue.push_back(waiting);
            // The entry task ends at once: the others still run to their
            // ends.
            run(
                &raw mut worker,
                returns_at_once as *const () as usize,
                false,
            )
        };
        assert_eq!(status, 0);
        assert_eq!(
            *EVENTS.lock().expect("no test panicked"),
            [
                10 * MILLISECOND,
                20 * MILLISECOND,
                20 * MILLISECOND + 1,
                30 * MILLISECOND
            ],
            "timers fire soonest first, and settling a future resumes its awaiter next"
        );
    }
}
