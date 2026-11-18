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
//! # The collector
//!
//! Every task is on one list, which the collector walks: a task's future
//! and what it is to run are roots, and so is the stack of every task that
//! has one in use. The task running is walked from where it called the
//! runtime, as before. Any other is walked from its switch frame, whose
//! frame record is the top of its chain of compiled frames and whose result
//! word is a root.

pub mod switch;

use std::{
    cmp::Reverse,
    collections::{BinaryHeap, VecDeque},
    io::Write,
    sync::atomic::{AtomicU32, AtomicUsize, Ordering},
    time::Instant,
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
    /// Reserved: what a waker hands it, and what kind of thing that is.
    wait_value: usize,
    wait_tag: usize,
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
        }
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
                wait_tag: 0,
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

    /// Wake every task whose timer has fired, the soonest to run first.
    fn fire_timers(&mut self) {
        let now = self.now();
        let mut fired = Vec::new();
        while let Some(Reverse((when, _, task))) = self.timers.peek().copied() {
            if when > now {
                break;
            }
            self.timers.pop();
            fired.push(task as *mut Task);
        }
        // Each goes behind the one before it, and all of them behind what
        // was already queued.
        for task in fired {
            // SAFETY: a task with a timer parked on it.
            unsafe { self.wake(task, false) };
        }
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
        match sched.queue.pop_back() {
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
    let sched = &mut worker.sched;
    sched.settle_last_switch();
    if nanos <= 0 {
        return 0;
    }
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

/// A `spawn` whose plan asks for what tasks cannot do yet.
#[unsafe(no_mangle)]
pub extern "C" fn baml_rt_spawn_unsupported() -> ! {
    fatal("a spawn with semaphores or cancel tokens is not supported by native code yet")
}

/// Run the program: `entry` on a task of its own, and every task it spawns,
/// until none is left that can run. Returns the exit status of the process.
///
/// # Safety
///
/// Called once, on the thread `worker` belongs to, with the address of the
/// compiled program's entry function.
pub unsafe fn run(worker: *mut Mutator, entry: usize) -> i32 {
    // SAFETY: the caller's contract. The worker is reached through its
    // address here because tasks reach it through their pinned register in
    // between.
    unsafe {
        let main = (*worker).sched.new_task(MAIN_STACK, entry);
        (*main).flags = TASK_MAIN;
        (*worker).sched.queue.push_back(main);
        loop {
            let sched = &mut (*worker).sched;
            sched.settle_last_switch();
            sched.fire_timers();
            if let Some(task) = sched.queue.pop_back() {
                (*task).state.store(RUNNING, Ordering::Release);
                baml_rt_resume(worker, task);
                // Back on the worker's own stack: the task that left it is
                // off its stack now.
                let left = std::mem::replace(&mut (*worker).cur_task, std::ptr::null_mut());
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
            let Some(Reverse((when, ..))) = sched.timers.peek().copied() else {
                break;
            };
            let now = sched.now();
            if when > now {
                std::thread::sleep(std::time::Duration::from_nanos(when - now));
            }
        }
        if !(*worker).sched.main_done {
            fatal("deadlock: every task waits for another");
        }
        // Every task has ended or waits for good. An error no `await` took
        // is reported now, as `baml run` does after the program's result.
        i32::from(report_unhandled(&*worker))
    }
}

#[cfg(all(test, target_arch = "aarch64"))]
mod tests {
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
            run(&raw mut worker, returns_at_once as *const () as usize)
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
