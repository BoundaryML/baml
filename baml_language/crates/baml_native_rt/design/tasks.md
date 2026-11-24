# Tasks

`spawn`, `await`, the stack switch, parking and waking. The OS threads behind this are in `threads.md`; the I/O chain is measured in `natives.md`. Numbers are instructions retired, measured unless marked estimated; method in `README.md`.

## What a task is

A task is a 128-byte record that is never freed or moved, and a stack of its own (`src/sched.rs`):

```rust
#[repr(C, align(128))]
pub struct Task {
    sp: usize,            // its switch frame while it is not running
    stack_limit: usize,   // copied into the worker on every switch in
    stack_lo: usize, stack_hi: usize,
    state: AtomicU32,     // FREE RUNNABLE RUNNING PARKING PARKED NOTIFIED DEAD
    flags: u32,
    link: *mut Task,      // free list, or the waiters of one future
    future: usize,        // root: the future it settles
    entry: usize,         // root: the closure it runs, until it starts
    natives: *mut drive::Head,   // async natives it is inside, innermost first
    all_prev: *mut Task, all_next: *mut Task,
    id: u64, wake_at: u64,
    wait_value: usize, cancel: usize, tele_node: usize,   // reserved
}
```

A task that is not running keeps its registers in a 160-byte switch frame at its saved `sp`: the frame record first, then `x19`, `x20`, `x22`-`x28`, `d8`-`d15`, and one result word. `x21` is not saved: whoever resumes the task supplies its own worker. A resume ends in `br`, not `ret`, so the return predictor is not fed an address no call pushed (`src/sched/switch.rs`):

```asm
; switch_in: go on as the task in x0, on the worker in x21
  str  x0, [x21, #48]         ; worker.cur_task
  ldr  x9, [x0, #8]           ; task.stack_limit
  str  x9, [x21, #16]         ; worker.stack_limit
  ldr  x9, [x0, #0]           ; task.sp
  mov  sp, x9
  ldp  x19, x20, [sp, #16]    ; 4 ldp + 1 ldr integer, 4 ldp float
  ...
  ldr  x0, [sp, #88]          ; the result word (zero or a reference)
  ldp  x29, x16, [sp], #160
  br   x16
```

`switch_in` is 17 instructions. The saving half is 10 (`save!`). A full park to the worker's stack and resume from it:

| part | instructions (single-step trace) |
|---|---|
| parking stub: save, call the Rust half, test | 15 |
| `switch_to_worker`: load `g0_sp`, restore, `ret` | 13 |
| `baml_rt_resume`: save the worker's registers, `switch_in` | 32 |

60 instructions of assembly for a round trip through the worker, plus the Rust half that decides (16 to 53, below).

## `spawn` runs the child first

`spawn` calls `baml_rt_spawn` with the closure, a task shim (the compiled function a task starts in; one per machine result type) and flags. The stub builds a switch frame for the parent, calls `spawn_prepare`, and switches to the child at once. The parent is left runnable. The child's first act (`baml_rt_task_start`) puts the parent on the run queue. A task that ends switches straight to the task queued last (`exit_pick`), which is the parent, with the future as the stub's result.

```rust
pub(crate) unsafe extern "C" fn spawn_prepare(mut closure: usize, shim: usize, flags: usize,
                                             worker: &mut Mutator, frame: usize) -> *mut Task {
    worker.sched.settle_last_switch();          // free the task that ended last
    worker.enter_switch_frame(frame);           // anchor, for the allocation below
    let child = worker.sched.new_task(TASK_STACK, shim);
    worker.root(&mut closure);
    let future = worker.allocate_object(TYPE_FUTURE, FUTURE_SIZE);
    worker.unroot(1);
    // future fields, child.future, child.entry
    store(frame + SWITCH_FRAME_RESULT, future); // what the parent resumes with
    (*parent).sp = frame;
    (*parent).state.store(RUNNABLE, Ordering::Release);
    worker.sched.pending = parent;              // queued by the child, once off this stack
    (*child).state.store(RUNNING, Ordering::Release);
    child
}
```

`await` is inline when the future has settled: an acquire load of the state and a load of the value. Otherwise it calls `baml_rt_await`, which links the task into the future's waiter list with a compare-and-swap and switches to the worker's stack.

```baml
for (let i = 0; i < N; i += 1) { s += await spawn { 1 }; };
```

```asm
  bl   _baml_rt_array_new     ; the spawn plan's two lists (semaphores,
  bl   _baml_rt_array_new     ; cancel tokens), both empty
  ...                         ; allocate the 48-byte plan inline, fill it
  ldr  x1, [x0, #24] ; ldr x1, [x1, #8]     ; length of one list
  ldr  x2, [x0, #32] ; ldr x2, [x2, #8]     ; length of the other
  orr  x1, x1, x2
  cbnz x1, unsupported        ; baml_rt_spawn_unsupported
  ldr  x0, [x0, #8]           ; plan.body: the closure
  mov  x2, #0                 ; flags
  mov  x1, x20                ; the shim
  bl   _baml_rt_spawn         ; returns after the child has run
  add  x0, x0, #8
  ldar x0, [x0]               ; future.state, acquire
  cmp  x0, #1                 ; FUTURE_READY
  b.ne slow                   ; baml_rt_await, or an error to rethrow
  ldr  ...                    ; future.value
```

466.42 instructions per `spawn` plus `await` (N 200,000: 114,693,203; 1,200,000: 581,117,318), 33 ns wall. Single-step trace of one iteration, 464:

| part | instructions |
|---|---|
| `main`: loop, plan object, closure, checks, inline `await` | 66 |
| two `baml_rt_array_new` calls for the empty plan lists | 58 |
| `baml_rt_spawn`: save 15, `switch_in` 17 | 32 |
| `spawn_prepare`, with the inline allocation of the future | 88 |
| `Sched::new_task`: pop the free list, zero and fill the first switch frame, link | 48 |
| `baml_rt_task_boot` | 4 |
| task shim | 29 |
| `baml_rt_task_start`: queue the parent, take the closure | 27 |
| the closure body `{ 1 }` | 5 |
| `baml_rt_task_finish`: settle the future, wake waiters (none) | 21 |
| `exit_pick`: mark dead, look at inbox and timers, pop the parent | 35 |
| `take_woken`, `fire_timers` (both empty) | 33 |
| `switch_in` to the parent | 18 |

| | N lo | N hi | instr lo | instr hi | per spawn+await |
|---|---|---|---|---|---|
| native | 200,000 | 1,200,000 | 114,693,203 | 581,117,318 | 466 |
| Go, goroutine + buffered channel | 200,000 | 1,200,000 | 961,764,490 | 5,606,675,205 | 4,645 |
| Go, same, `GOMAXPROCS=1` | 200,000 | 1,200,000 | 748,347,793 | 4,338,294,216 | 3,590 |
| Go, goroutine + `WaitGroup` | 200,000 | 1,200,000 | 1,031,340,006 | 6,017,841,657 | 4,987 |
| Node, `await` of an async function | 2 M | 12 M | 1,882,130,488 | 9,176,419,134 | 729 |

```go
for i := 0; i < n; i++ {
	ch := make(chan int, 1)
	go func() { ch <- 1 }()
	s += <-ch
}
```

Native is 8 to 10 times cheaper than Go here. The reasons are structural: no channel, no lock, the child runs on the parent's worker immediately and hands straight back, and the stack is reused from a free list.

Weak points:

- 124 of the 464 instructions build a spawn plan whose two lists are always empty, since a plan with semaphores or cancel tokens is rejected (`baml_rt_spawn_unsupported`). Two runtime calls and a 48-byte object per spawn, plus three garbage objects.
- `exit_pick` looks at the inbox and the timers on every task end (33 instructions when both are empty).

## A task that needs a new stack

The free list is empty when tasks are alive at once. Then `new_task` maps a stack: `mmap` of 80 KB and `mprotect` of the guard, and a `Box` for the record.

```baml
for (let i = 0; i < N; i += 1) { fs.push(spawn { sleep(1 ms); 1 }); };
for (let f in fs) { s += await f; };
```

| | N lo | N hi | instr lo | instr hi | per task |
|---|---|---|---|---|---|
| native | 2,000 | 12,000 | 45,317,582 | 113,339,524 | 6,802 |
| native | 20,000 | 40,000 | 158,932,058 | 276,713,440 | 5,889 |
| Go, goroutines blocked on a gate | 20,000 | 40,000 | 283,373,782 | 614,368,649 | 16,550 |

Kernel instructions are counted, and they are most of this: two syscalls and the first-touch fault of the stack's top page. Tasks whose timers fire while the loop is still spawning are reused, so this understates a true fresh stack (not isolated). Stacks are never unmapped: a burst of N live tasks keeps N x 80 KB of address space and N touched pages for the life of the process.

## Parking on a timer

`baml.sys.sleep` is compiled to `baml_rt_sleep`. `sleep_park` reads the clock, pushes `(when, id, task)` on a binary heap and parks. The worker's loop pops tasks whose time has come (`fire_timers`) and wakes them to run next.

```baml
function nap() -> int { baml.sys.sleep(Duration.from_nanoseconds(1n)) catch (e) { let e => 0 }; 1 }
s += await spawn { nap() };
```

1,816.23 instructions per iteration (N 100,000: 203,012,542; 600,000: 1,111,130,177), 1,350 more than a spawn that does not sleep. From the trace: two clock reads at about 170 each (`Instant::elapsed` to `clock_gettime` to `mach_timebase_info`), `sleep_park` 53, the parking stub and switch 28, the worker loop 64, `fire_timers` 98 plus a `Vec` it allocates and frees on every call that fires anything, `baml_rt_resume`
32.

Weak points: `fire_timers` collects fired tasks into a fresh `Vec` (one `malloc`/`free` per wake). `Instant::elapsed` goes through `clock_gettime`; `mach_absolute_time` with a cached timebase would be about a tenth of the instructions (estimated).

## Parking on a future

`baml_rt_await` on an unsettled future: `await_park` sets the task `PARKING`, links it at the head of the future's waiter list by compare-and-swap on the state word, and the stub switches to the worker. When the task that owns the future ends, `settle` swaps the state to `READY` or `ERROR` and wakes each waiter (`PARKED` to `RUNNABLE`, pushed to run next). The value is written before the state, and readers load the state first with acquire ordering.

```rust
fn settle(worker: &mut Mutator, value: usize, trace: usize, state: usize) {
    store(future + FUTURE_VALUE, value);
    store(future + FUTURE_TRACE, trace);
    let before = future_state(future).swap(state, Ordering::AcqRel);
    let mut waiter = (before & !FUTURE_SETTLED) as *mut Task;
    while !waiter.is_null() { let next = (*waiter).link; worker.sched.wake(waiter, true); waiter = next; }
}
```

Not measured in isolation: with one worker and child-first spawn, an `await` meets an unsettled future only when the child itself parked. Its cost is the 60-instruction round trip above plus `await_park` and `wake` (estimated 60 to 80 more).

## Parking on I/O, and the wake from another thread

A task waiting for I/O parks with `baml_rt_io_wait`. The job runs on the `baml-io` thread. When it ends, that thread calls `Inbox::wake`: lock a mutex, push the task, set an atomic flag, `notify_one` on a condvar.

The worker with nothing runnable blocks in `Sched::sleep`:

```rust
fn sleep(&mut self) -> bool {
    let until = self.timers.peek().map(|Reverse((when, ..))| *when);
    if until.is_none() && self.on_io == 0 { return false; }     // deadlock or done
    let woken = self.inbox.woken.lock().expect("no thread panics holding it");
    if !woken.is_empty() { return true; }
    match until {
        Some(when) => drop(self.inbox.arrived.wait_timeout(woken, Duration::from_nanos(...))),
        None => drop(self.inbox.arrived.wait(woken)),
    }
    true
}
```

That is `pthread_cond_wait` (or `pthread_cond_timedwait_relative_np`), which is the `__psynch_cvwait` syscall on macOS. There is no spin phase and no futex-style fast path. The wake is `pthread_cond_signal` (`__psynch_cvsignal`) from the I/O thread.

Measured on the cheapest I/O native (`natives.md`): 39,852 instructions, 6.2 microseconds and 2.0 context switches per round trip, both threads and the kernel included. The worker's own share in user mode is about 5,500 (traced), of which the scheduler is 224.

Go for a comparable hand-off between two goroutines over unbuffered channels: 3,711 instructions per round trip, 2,958 with one P, 260 ns wall. Go parks goroutines in user space and only parks threads when a P goes idle.

## Not built

- Cancellation (`Task.cancel` is reserved; a spawn plan with cancel tokens or semaphores ends the program).
- Work stealing, or any second worker (`threads.md`).
- Stack growth or release.
- Preemption: a task runs until it parks or ends.
