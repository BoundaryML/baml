# Natives

How compiled code calls the standard library's Rust functions, how a
native calls back into BAML, and what a native that does I/O costs. The
flags and their promises are in `natives/README.md`; this document is the
mechanism and its cost. Numbers are instructions retired, measured unless
marked estimated; method in `README.md`.

## Three kinds of call

| flag | symbol compiled code calls | worker passed | may collect | may stop |
|---|---|---|---|---|
| none | the Rust function itself | no | no | no |
| `allocates` | a 4-instruction anchor stub | yes, first argument | yes | no |
| `async` | a compiled driver stub, per native | through `x21` | yes | yes |

`build.rs` generates the `extern "C"` symbol of each native from the lists
in `natives/*.txt` and the standard library's declarations
(`natives_generated.rs` in the build's `OUT_DIR`).

## Plain native

A C call. No stub, no worker, no anchor, nothing treated as moved.

```baml
s = s + t.byte_length();
```

```asm
  mov  x0, x19                      ; the string
  bl   _baml_rt_baml_string_byte_length
  ldr  w1, [x21, #24]               ; loop poll
  cbnz x1, poll_slow

_baml_rt_baml_string_byte_length:
  stp  x29, x30, [sp, #-16]!
  mov  x29, sp
  ldr  w0, [x0, #8]
  ldp  x29, x30, [sp], #16
  ret
```

16.00 instructions per iteration (N 10 M: 174,540,914; 60 M:
975,003,823): 2 at the call site, 5 in the callee, 9 for the loop and its
poll. Arguments move from the BAML convention (`x2`...) to the C one
(`x0`...). Go has no equivalent at this price: a cgo call is on the order
of 100 times more (not measured here, cgo did not build in this
environment), and a Go assembly function costs a plain call.

Many small natives never reach this path: `count_ones`, `abs`,
`leading_zeros` and the like are emitted as instructions.

## `allocates` native and the anchor stub

A collection walks the stack from the compiled frame that called the
runtime. That frame's stack pointer, return address and frame pointer are
the worker's anchor. The stub records them before any Rust runs, so the
walk does not depend on how rustc lays out the native's frame.

`src/stubs.rs`:

```rust
macro_rules! anchored {
    ($name:ident => $function:path) => {
        #[unsafe(naked)] #[unsafe(no_mangle)]
        unsafe extern "C" fn $name() {
            core::arch::naked_asm!(
                "mov x16, sp",
                "stp x16, x30, [x21, #{anchor}]",     // anchor.sp, anchor.pc
                "str x29, [x21, #{anchor} + 16]",     // anchor.fp
                "b {function}",
                anchor = const MUTATOR_ANCHOR, function = sym $function,
            )
        }
    };
}
```

The generated wrapper builds a `Scope` over 16 inline root slots, roots the
reference arguments, calls the trait method, and returns the value beside
a thrown flag:

```rust
unsafe extern "C" fn baml_int_isqrt(mutator: &mut Mutator, int: i64) -> RtResult {
    let slots = RootSlots::new();
    let mut scope = unsafe { Scope::new(mutator, &slots) };
    match <Natives as NativeBamlInt>::isqrt(&mut scope, int) {
        Ok(value) => RtResult::ok(value.cast_unsigned()),
        Err(thrown) => RtResult::thrown(&thrown),
    }
}
```

```baml
s = s + (i + 4).isqrt();
```

Single-step trace of one call: 48 instructions, 4 in the stub and 44 in
the wrapper with `isqrt` inlined (small argument). The caller then tests
the thrown flag and treats every reference it holds as moved: it reloads
them from their stack-map slots. The loop total is 96.02 per iteration
(973,901,214 at 10 M; 5,775,261,298 at 60 M); most of it is `isqrt` itself
at large arguments.

The fixed cost is the 4-instruction stub plus the reloads after the call.
`RootSlots::new()` for 16 slots is not free in a native that roots
nothing; it shows up as part of the 178 instructions of `baml.Map.set`
(`memory.md`).

## `async` native: the driver is compiled code

An `async` native is a Rust `async fn`. Its state lives in one block on the
Rust heap (`Head` plus the future, `src/native/drive.rs`). The compiler
emits one driver stub per native. The stub calls the native's symbol, which
allocates the block and returns it without running the body, then loops on
`baml_rt_native_poll`:

```asm
_baml_async_baml_rt_baml_ops_equals_equals:
  stp  x29, x30, [sp, #-16]!
  mov  x29, sp
  str  x26, [sp, #-16]!
  mov  x0, x21                 ; worker
  mov  x1, x2 ... mov x4, x5   ; BAML args -> C args
  bl   _baml_rt_baml_ops_equals_equals   ; anchor stub -> start(): Box the block
  mov  x26, x0                 ; the block
  mov  x2, #0 ; mov x3, #0     ; word, threw
poll:
  mov  x0, x21 ; mov x1, x26
  bl   _baml_rt_native_poll    ; anchor stub -> native_poll -> Future::poll
  cbnz x0, not_done
  mov  x2, x1                  ; STEP_DONE: the value
  ret
not_done:
  cmp  x0, #1 ; b.eq threw     ; STEP_THREW: baml_rt_rethrow(word, worker.scratch)
  cmp  x0, #2 ; b.eq call      ; STEP_CALL
  mov  x0, x1
  bl   _baml_rt_io_wait        ; STEP_WAIT: park until the job is done
  b    poll
call:
  ldr  x0, [x26] ... ldr x3, [x26, #24]   ; four argument words from the block
  blr  x1-as-code              ; the compiled entry the native asked for
  mov  x2, x0 ; mov x3, x1     ; what it returned / whether it threw
  b    poll
```

Every Rust function returns before the task can stop or run BAML. So a
stopped task has no Rust frame on its stack, and it can resume on another
thread under another worker: the stub rereads `x21` for each call and
`native_poll` writes the worker into the native's scope.

```rust
pub(crate) unsafe extern "C" fn native_poll(worker: *mut Mutator, head: *mut Head,
                                            word: usize, threw: usize) -> Step {
    let exchange = &raw mut (*head).exchange;
    (*exchange).word = word;
    (*exchange).threw = threw != 0;
    (*exchange).context = (*worker).scratch();
    (*exchange).worker = worker;
    (*worker).polling = Some((*head).name);
    let polled = ((*head).poll)(head);
    (*worker).polling = None;
    if let Poll::Ready(result) = polled {
        (*task).natives = (*head).outer;
        ((*head).drop)(head);
        return Step { kind: result.thrown, word: result.value as usize };
    }
    match (*exchange).request.take() {
        Some(Request::Call { code, args }) => { (*head).args = args; Step { kind: STEP_CALL, word: code } }
        Some(Request::Io(wait)) => { *waits_for = Some(wait); Step { kind: STEP_WAIT, word: waits_for as usize } }
        None => abort(),   // awaited something else
    }
}
```

### An `async` native that completes without stopping

Fixed cost of the machinery, from a single-step trace of `==` on two
instances (`baml.ops.equals_equals`), counting only the driver's own
functions:

| part | instructions |
|---|---|
| driver stub | 21 |
| anchor stub of the native's symbol | 4 |
| generated symbol: build `Head`, `Scope`, the future | 72 |
| anchor stub of the poll | 4 |
| `native_poll` | 43 |
| `poll_block` (enters the future) | 60, part of it the native's own prologue |
| `drop_block`, drop of `Head` | 32 |
| total | 236 |
| plus one `malloc` and one `free` of the block | about 150 to 250, estimated from the whole-program numbers below |

The whole operation is much more, because of what this native does:

```baml
class P { x int }
class H { item P  count int }
if (hs[i & 1] == b) { s += 1; }
```

3,267.67 instructions per `==` (N 1 M: 3,282,369,033; 6 M:
19,620,724,016). The trace shows where: five `malloc`/`free` pairs per
comparison (the block, a `HashMap<(usize, usize), ()>` of visited pairs
with SipHash, its table, a `Vec` of pairs, a regrow of it) and 334
instructions of SipHash for two insertions. The structural walk itself is
about 400.

This is the worst ratio on the map. Go compares two such structs with a
few loads and compares. A class with only scalar and acyclic fields needs
no visited set and no heap block. The known alternatives: emit field-wise
comparison in compiled code for classes whose shape is closed, or keep the
visited set and the pair stack inline until a cycle is possible.

### `scope.call_baml`: native to BAML and back

The native builds a request and returns `Pending`. `native_poll` hands the
stub `STEP_CALL` with a code address. The stub loads four argument words
from the block and calls a hook: a compiled entry that takes and returns
values as a union holds them, and calls the real function. The stub polls
again with the result.

```rust
pub fn call_baml<'a>(&'a mut self, function: BamlFn, args: &[Elem<'s>])
    -> impl Future<Output = Result<Elem<'s>, Thrown<'s>>> + Send + use<'a, 's> {
    let mut words = [0; CALL_ARGS];
    for (word, arg) in words.iter_mut().zip(args) { *word = arg.get(self); }
    let stop = Stop { scope: Some(self), request: Some(Request::Call { code: function.0, args: words }) };
    async move {
        let scope = stop.await;          // Pending once; the driver runs the call
        let value = scope.elem_raw((*scope.exchange).word, TypeId(0));
        if (*scope.exchange).threw { Err(...) } else { Ok(value) }
    }
}
```

```baml
class P {
  x int
  implements baml.ops.Equals {
    function eq(self, other: Self) -> bool throws never { self.x == other.x }
  }
}
class H { item P  count int }
if (hs[i & 1] == b) { s += 1; }       // the walk meets P and calls its eq
```

One round trip, single-step trace, in order:

| part | instructions |
|---|---|
| `call_baml`: copy args, make the request, return `Pending` | 51 |
| back out through the native's state machine and `poll_block` | 30 |
| `native_poll`: take the request, return `STEP_CALL` | 20 |
| driver stub: test the step, load 4 args, `blr` | 12 |
| hook, in | 14 |
| `P.eq` (the callee) | 8 |
| hook, out | 13 |
| driver stub: move results, loop | 7 |
| anchor stub | 4 |
| `native_poll`: store the answer, poll | 23 |
| `poll_block` and the state machine back in | 36 |
| `call_baml` resumes: wrap the word as an `Elem`, root it | 59 |
| total | 277 |
| without the callee | 269 |

An earlier count of this path was 328; at this commit it is 269. The whole `==` with a
user `eq` is 5,004.07 instructions (N 1 M: 5,023,483,784; 6 M:
30,043,842,776), 1,736 more than without, because the walk runs twice and
builds an array of pairs before calling.

What dominates: two full polls of the Rust future (state-machine dispatch
in and out twice, about 110) and the two ends of `call_baml` (110). The
call itself is 8 + 27. Go calls a method value in about 5 instructions;
there is nothing to compare the suspension with.

Not built: a direct path for a native that only needs to call BAML and
cannot be parked across (it would need the native's frame to stay on the
stack, which the migration rule forbids).

## The chain to Tokio: `scope.io`

`scope.io(job)` boxes the job, returns `Pending` with a `Request::Io`, and
the stub calls `baml_rt_io_wait`. That parks the task and starts the job
on the I/O runtime: a Tokio multi-thread runtime with one worker thread
named `baml-io`, created at the first job (`drive.rs`, `start_on_tokio`).
When the job ends, its wrapper calls `Wake::wake`, which pushes the task
into the worker's inbox under a mutex and signals a condvar. The worker,
with nothing to run, is asleep on that condvar.

```rust
pub(crate) unsafe extern "C" fn io_park(wait: usize, worker: &mut Mutator, frame: usize) -> usize {
    worker.sched.settle_last_switch();
    (*task).sp = frame;
    (*task).state.store(PARKING, Ordering::Release);
    let IoWait { job, start } = (*(wait as *mut Option<IoWait>)).take().expect(...);
    sched.on_io += 1;
    start(job, Wake { inbox: Arc::clone(&sched.inbox), task: task as usize });
    1                                   // the stub switches to the worker's stack
}

fn wake(&self, task: usize) {           // Inbox, any thread
    let mut woken = self.woken.lock().expect(...);
    woken.push(task);
    self.filled.store(true, Ordering::Release);
    drop(woken);
    self.arrived.notify_one();
}
```

The cheapest native that does this is `baml.env.get`, whose job is a
`getenv`:

```baml
let v = baml.env.get("BAML_COSTMAP_UNSET_VARIABLE");
```

Chain: BAML `main` -> driver stub -> native (Rust) -> `Pending` -> stub ->
`baml_rt_io_wait` -> `io_park` -> `tokio::spawn` (remote queue push,
`mio::Waker::wake`, a `kevent` syscall) -> switch to the worker's stack ->
`serve` -> `Sched::sleep` -> `pthread_cond_wait`. On `baml-io`: wake from
`kevent`, run the job, `Inbox::wake` (mutex, `pthread_cond_signal`), park
again. Worker: wake, `take_woken`, `baml_rt_resume`, stub, `native_poll`,
native finishes, returns.

Whole process, both threads, kernel included:

| | N lo | N hi | lo | hi | per round trip |
|---|---|---|---|---|---|
| instructions | 20,000 | 120,000 | 815,330,829 | 4,800,578,459 | 39,852 |
| wall time | 20,000 | 120,000 | 0.12 s | 0.74 s | 6.2 microseconds |
| involuntary context switches | 20,000 | 120,000 | 40,011 | 240,127 | 2.0 |

Worker thread, user mode only (single-step trace in two halves, before the
sleep and after the wake): 5,565 instructions, of which

| part | instructions |
|---|---|
| driver stub, anchor stubs, `native_poll` (2 polls), `poll_block` | 384 |
| generated symbol and block setup | 58 |
| `baml_rt_io_wait`, `io_park` | 61 |
| `start_on_tokio` and Tokio's spawn, remote push, unpark | 348 |
| `serve`, `take_woken` (twice), `fire_timers` (twice), `baml_rt_resume` | 224 |
| drops of the block, the job's result slot, the `Arc`s | 71 |
| mutex and condvar calls into libpthread | 751 |
| `malloc`/`free`/`memcpy`: 10 allocations per round trip | 3,597 under the debugger, estimated 1,500 to 2,000 outside it |

So about 1,150 instructions are the runtime and Tokio's submit path, and
about 34,000 of the 39,852 are the kernel and the `baml-io` thread: two
thread wake-ups, each through a syscall, and Tokio's park and unpark
around one tiny task.

Go for the same user-visible operation reads the environment in place:
271.50 instructions (`os.LookupEnv`, N 200,000: 73,401,569; 1,200,000:
344,908,852). That is 147 times less. The honest Go comparison for the
mechanism is a request handed to another goroutine and the answer received
back on a channel: 3,711.16 instructions per round trip (756,872,544;
4,468,033,546), 2,957.50 with `GOMAXPROCS=1`. Node, a promise settled by
`setImmediate`: 18,936.10.

```go
req, resp := make(chan int), make(chan int)
go func() { for v := range req { resp <- v + 1 } }()
for i := 0; i < n; i++ { req <- i; s += <-resp & 1 }
```

Weak points, in order of cost:

1. Every `scope.io` is two OS thread hand-offs. A job that does not block
   (`env.get`, most of `time`) should not be a job. The known alternative
   is to run such work inline in the native; `async` exists on these only
   to reach `scope.io`.
2. The worker sleeps in `pthread_cond_wait` whenever its queue is empty,
   even when a job is one syscall from done. No spinning before the sleep.
3. Ten Rust heap allocations per round trip: the block, the boxed job, two
   `Arc`s, the Tokio task, the inbox `Vec` growing from empty each time
   (`take_woken` takes the vector with `mem::take`, so the next `wake`
   allocates again).
4. One `baml-io` thread runs every job. Blocking work goes to Tokio's
   blocking pool, which adds a third thread and two more hand-offs.

## Not built

- Completion without a thread hop for work that is ready immediately.
- An I/O poller owned by the worker (kqueue or io_uring on the worker
  thread) in place of a second runtime.
- More than one I/O thread.
