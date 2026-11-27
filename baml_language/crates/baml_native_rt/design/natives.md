# Natives

How compiled code calls the standard library's Rust functions, how a native calls back into BAML, and what a native that does I/O costs. The flags and their promises are in `natives/README.md`; this document is the mechanism and its cost. Numbers are instructions retired; method in `README.md`. Native totals are counted at `881e4c1dfc` unless they say otherwise. "Before" is the layout before two-word values, at `39d3ad4c25`. The single-step traces, the I/O chain and every Go and Node number are from then and were not run again; each says so.

## Three kinds of call

| flag | symbol compiled code calls | worker passed | may collect | may stop |
|---|---|---|---|---|
| none | the Rust function itself | no | no | no |
| `allocates` | a 4-instruction anchor stub | yes, first argument | yes | no |
| `async` | a compiled driver stub, per native | through `x21` | yes | yes |

`build.rs` generates the `extern "C"` symbol of each native from the lists in `natives/*.txt` and the standard library's declarations (`natives_generated.rs` in the build's `OUT_DIR`; the rules are in `baml_builtins2_codegen/src/native_abi.rs`).

## What crosses the boundary

Parameters, in order: the worker when the native `allocates`; then, for seven natives only, the address of a type object per type parameter; then each declared parameter.

| declared | passed as |
|---|---|
| `int` | `i64` |
| `float` | `f64`, in a `d` register |
| `bool` | `u8` |
| `string`, `bigint`, `uint8array`, an array, a map, a class, Rust state | the object's address, never null |
| anything else: a type parameter's value (`Elem`), a union (`Any`), any optional, a `json` | two `u64`, tag then payload |

A value arrives as it is held everywhere else: an `int` in a union is `(INT, bits)`, an enum variant `(ENUM_VARIANT, address)`, an absent optional `(NULL, 0)`. The wrapper builds a `Value` of the two words and roots it. There are no boxes, no `OptionalScalar` and no per-argument "is a reference" flag: a root's tag says whether the collector follows it. `Elem` is a value in a root slot.

Results. A native whose declaration throws nothing returns an exact result in one register and any other as a `Value` in `x0` (tag), `x1` (payload). A native that can throw always returns a `Value`; bit 59 of the tag (`THROWN`) set means the value is what it threw, and otherwise it is the result, an exact one too (`(INT, bits)` for an `int`). Compiled code tests the bit with one instruction and reads the payload. `RtResult` is gone.

The seven natives passed types (`TAKES_TYPE_IDS`): `baml.Array._results`, `baml._rust_sort`, `baml._is_primitive_array`, `baml._to_json_default`, `baml._from_json_structural_default`, `baml.json._decode`, `baml.json._to`. Every other generic native reads what it needs from the tags of the values it is given. A parameter the `baml.json` and equality natives are told the static type of is preceded by one word, the address of that type's object or zero.

An argument left to its default cannot be passed to a native: the call is refused when checked.

## Plain native

A C call. No stub, no worker, no anchor, nothing treated as moved.

```baml
s = s + t.byte_length();
```

```asm
  stur x0, [sp]                     ; the string, kept in a frame slot across the call
  bl   _baml_rt_baml_string_byte_length
  mov  x1, x0
  ldr  w0, [x21, #0x18]             ; loop poll
  cbnz x0, poll_slow
  ldur x0, [sp]

_baml_rt_baml_string_byte_length:
  stp  x29, x30, [sp, #-0x10]!
  mov  x29, sp
  ldr  w0, [x0, #0x8]
  ldp  x29, x30, [sp], #0x10
  ret
```

18.00 instructions per iteration (N 10 M: 195,727,216; 60 M: 1,095,754,661), 16.00 before. The call and the callee did not change: a `string` parameter is still one word and an `int` result one register. The probe was written again and the two more are in its loop (the string is spilled and reloaded around the call). Go has no equivalent at this price: a cgo call is on the order of 100 times more (not measured, cgo did not build in the environment of `39d3ad4c25`), and a Go assembly function costs a plain call.

Many small natives never reach this path: `count_ones`, `abs`, `leading_zeros` and the like are emitted as instructions.

## `allocates` native and the anchor stub

A collection walks the stack from the compiled frame that called the runtime. That frame's stack pointer, return address and frame pointer are the worker's anchor. The stub records them before any Rust runs, so the walk does not depend on how rustc lays out the native's frame.

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

The generated wrapper builds a `Scope` over 16 inline root slots, each a value, roots the arguments that are objects or values, calls the trait method, and returns a value whose tag says whether it was thrown:

```rust
unsafe extern "C" fn baml_int_isqrt(mutator: &mut Mutator, int: i64) -> Value {
    let slots = RootSlots::new();
    let mut scope = unsafe { Scope::new(mutator, &slots) };
    match <Natives as NativeBamlInt>::isqrt(&mut scope, int) {
        Ok(value) => Value::int(value),
        Err(thrown) => thrown.returned(&scope),     // the thrown value, with THROWN in its tag
    }
}
```

```baml
s = s + (i + 4).isqrt();
```

```asm
  mov  x0, x21                      ; worker
  add  x1, x19, #0x4
  bl   _baml_rt_baml_int_isqrt      ; the anchor stub, then the wrapper
  tbnz x0, #0x3b, raise             ; bit 59 of the tag: thrown
  ...                               ; x1 = the result's payload
```

The loop total is 104.00 per iteration (1,054,819,557 at 10 M; 6,255,009,541 at 60 M), 96.02 before; most of it is `isqrt` itself at large arguments. A single-step trace of one call on the old layout was 48 instructions, 4 in the stub and 44 in the wrapper with `isqrt` inlined; not traced again, so where the 8 more are is not known (the root slots are 16 bytes each where they were 8, and the result is two words). After the call the caller treats every reference it holds as moved: it reloads them from their stack-map slots.

The fixed cost is the 4-instruction stub plus the reloads after the call. `RootSlots::new()` for 16 slots is not free in a native that roots nothing; it shows up in `baml.Map.set` (`memory.md`).

## `async` native: the driver is compiled code

An `async` native is a Rust `async fn`. Its state lives in one block on the Rust heap (`Head` plus the future, `src/native/drive.rs`). The compiler emits one driver stub per native. The stub calls the native's symbol, which allocates the block and returns it without running the body, then loops on `baml_rt_native_poll`:

```asm
_baml_async_baml_rt_baml_ops_equals_equals:    ; entered with the BAML convention
  stp  x29, x30, [sp, #-0x10]!
  mov  x29, sp
  str  x19, [sp, #-0x10]!
  mov  x0, x21                 ; worker
  mov  x1, x2 ... mov x6, x7   ; BAML words -> C arguments
  bl   _baml_rt_baml_ops_equals_equals   ; anchor stub -> start(): Box the block
  mov  x19, x0                 ; the block
  mov  x2, #0 ; mov x3, #0     ; nothing to hand the first poll
poll:
  mov  x0, x21 ; mov x1, x19
  bl   _baml_rt_native_poll    ; anchor stub -> native_poll -> Future::poll
  cbnz x0, not_done
  ldr  x2, [x19]               ; STEP_DONE: the result's tag and payload are the block's first two words
  ldr  x3, [x19, #0x8]
  ret
not_done:
  cmp  x0, #1 ; b.eq threw
  cmp  x0, #2 ; b.eq call
  mov  x0, x1
  bl   _baml_rt_io_wait        ; STEP_WAIT: park until the job is done
  b    poll
call:
  ldr  x0, [x19] ... ldr x3, [x19, #0x18]   ; two values from the block
  blr  x4                      ; the compiled entry the native asked for
  mov  x3, x1 ; mov x2, x0     ; the value it returned, THROWN in the tag if it threw
  b    poll
threw:
  ldr  x0, [x19] ; ldr x1, [x19, #0x8]      ; STEP_THREW: the thrown value
  ldr  x2, [x21, #0x20]        ; its trace, from worker.scratch
  bl   _baml_rt_rethrow
```

The stub takes the two-word convention itself, so a method-table row can hold it. A done native's block outlives the poll that ended it, since the stub reads the value from it: the worker keeps it (`spent_native`) and frees it when the next native ends.

Every Rust function returns before the task can stop or run BAML. So a stopped task has no Rust frame on its stack, and it can resume on another thread under another worker: the stub rereads `x21` for each call and `native_poll` writes the worker into the native's scope.

```rust
pub(crate) unsafe extern "C" fn native_poll(worker: *mut Mutator, head: *mut Head,
                                            tag: usize, payload: usize) -> Step {
    let exchange = &raw mut (*head).exchange;
    (*exchange).value = Value { tag: tag & !THROWN, payload };   // what the last entry returned
    (*exchange).threw = tag & THROWN != 0;
    (*exchange).context = (*worker).scratch();
    (*exchange).worker = worker;
    (*worker).polling = Some((*head).name);
    let polled = ((*head).poll)(head);
    (*worker).polling = None;
    if let Poll::Ready(result) = polled {
        (*task).natives = (*head).outer;
        retire(worker, head);
        (*head).args[0] = result.tag & !THROWN;                  // the stub loads these two
        (*head).args[1] = result.payload;
        return Step { kind: if result.tag & THROWN == 0 { STEP_DONE } else { STEP_THREW }, word: 0 };
    }
    match (*exchange).request.take() {
        Some(Request::Call { code, args }) => { (*head).args = args; Step { kind: STEP_CALL, word: code } }
        Some(Request::Io(wait)) => { *waits_for = Some(wait); Step { kind: STEP_WAIT, word: waits_for as usize } }
        None => abort(),   // awaited something else
    }
}
```

### An `async` native that completes without stopping

Fixed cost of the machinery, from a single-step trace of `==` on two instances (`baml.ops.equals_equals`) on the old layout at `39d3ad4c25`, counting only the driver's own functions. Not traced again; the stub is 24 instructions on this path now (counted in the listing above), where it was 21:

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
class P { x: int }
class H { item: P  count: int }
if (hs[i & 1] == b) { s += 1; }
```

4,531.83 instructions per `==` (N 1 M: 4,552,552,429; 6 M: 27,211,708,948), 3,267.67 before. That is 1,264 more, and it was not traced, so where they go is not known: the walk reads every slot under its lock and reads tags where it read a type table, and the instances are twice the bytes. The trace of the old layout showed where its 3,268 went: five `malloc`/`free` pairs per comparison (the block, a `HashMap<(usize, usize), ()>` of visited pairs with SipHash, its table, a `Vec` of pairs, a regrow of it) and 334 instructions of SipHash for two insertions. The structural walk itself is about 400.

This is the worst ratio on the map. Go compares two such structs with a few loads and compares. A class with only scalar and acyclic fields needs no visited set and no heap block. The known alternatives: emit field-wise comparison in compiled code for classes whose shape is closed, or keep the visited set and the pair stack inline until a cycle is possible.

### `scope.call_baml`: native to BAML and back

The native builds a request and returns `Pending`. `native_poll` hands the stub `STEP_CALL` with a code address. The stub loads four argument words from the block, which are two values, and calls a hook: a compiled entry with the C convention that takes two values and returns one in `x0`, `x1`, with `THROWN` in its tag when the function threw (the trace is then in `worker.scratch`), and calls the real function. The stub polls again with the result.

```rust
pub fn call_baml<'a>(&'a mut self, function: BamlFn, args: &[Elem<'s>])
    -> impl Future<Output = Result<Elem<'s>, Thrown<'s>>> + Send + use<'a, 's> {
    let mut words = [0; NATIVE_CALL_ARGS];                 // at most two values
    for (index, arg) in args.iter().enumerate() {
        let value = arg.value(self);
        words[2 * index] = value.tag;
        words[2 * index + 1] = value.payload;
    }
    let stop = Stop { scope: Some(self), request: Some(Request::Call { code: function.0, args: words }) };
    async move {
        let scope = stop.await;          // Pending once; the driver runs the call
        // the value the poll was handed is the result, or what was thrown
    }
}
```

```baml
class P {
  x: int
  implements baml.ops.Equals {
    function eq(self, other: Self) -> bool throws never { self.x == other.x }
  }
}
class H { item: P  count: int }
if (hs[i & 1] == b) { s += 1; }       // the walk meets P and calls its eq
```

One round trip, single-step trace on the old layout at `39d3ad4c25`, in order (not traced again):

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

The whole `==` with a user `eq` is 5,904.55 instructions (N 1 M: 5,970,093,042; 6 M: 35,492,887,302), 5,004.07 before; 1,373 more than without (1,736 before), because the walk runs twice and builds an array of pairs before calling.

What dominates: two full polls of the Rust future (state-machine dispatch in and out twice, about 110) and the two ends of `call_baml` (110). The call itself is 8 + 27. Go calls a method value in about 5 instructions; there is nothing to compare the suspension with.

Not built: a direct path for a native that only needs to call BAML and cannot be parked across (it would need the native's frame to stay on the stack, which the migration rule forbids).

## The chain to Tokio: `scope.io`

`scope.io(job)` boxes the job, returns `Pending` with a `Request::Io`, and the stub calls `baml_rt_io_wait`. That parks the task and starts the job on the I/O runtime: a Tokio multi-thread runtime with one worker thread named `baml-io`, created at the first job (`drive.rs`, `start_on_tokio`). When the job ends, its wrapper calls `Wake::wake`, which pushes the task into the worker's inbox under a mutex and signals a condvar. The worker, with nothing to run, is asleep on that condvar.

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

The cheapest native that does this is `baml.env.get`, whose job is a `getenv`:

```baml
let v = baml.env.get("BAML_COSTMAP_UNSET_VARIABLE");
```

Chain: BAML `main` -> driver stub -> native (Rust) -> `Pending` -> stub -> `baml_rt_io_wait` -> `io_park` -> `tokio::spawn` (remote queue push, `mio::Waker::wake`, a `kevent` syscall) -> switch to the worker's stack -> `serve` -> `Sched::sleep` -> `pthread_cond_wait`. On `baml-io`: wake from `kevent`, run the job, `Inbox::wake` (mutex, `pthread_cond_signal`), park again. Worker: wake, `take_woken`, `baml_rt_resume`, stub, `native_poll`, native finishes, returns.

Nothing in this chain changed with two-word values but the stub and the poll above. The numbers below are from `39d3ad4c25` and were not measured again.

Whole process, both threads, kernel included:

| | N lo | N hi | lo | hi | per round trip |
|---|---|---|---|---|---|
| instructions | 20,000 | 120,000 | 815,330,829 | 4,800,578,459 | 39,852 |
| wall time | 20,000 | 120,000 | 0.12 s | 0.74 s | 6.2 microseconds |
| involuntary context switches | 20,000 | 120,000 | 40,011 | 240,127 | 2.0 |

Worker thread, user mode only (single-step trace in two halves, before the sleep and after the wake): 5,565 instructions, of which

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

So about 1,150 instructions are the runtime and Tokio's submit path, and about 34,000 of the 39,852 are the kernel and the `baml-io` thread: two thread wake-ups, each through a syscall, and Tokio's park and unpark around one tiny task.

Go for the same user-visible operation reads the environment in place: 271.50 instructions (`os.LookupEnv`, N 200,000: 73,401,569; 1,200,000: 344,908,852). That is 147 times less. The honest Go comparison for the mechanism is a request handed to another goroutine and the answer received back on a channel: 3,711.16 instructions per round trip (756,872,544; 4,468,033,546), 2,957.50 with `GOMAXPROCS=1`. Node, a promise settled by `setImmediate`: 18,936.10.

```go
req, resp := make(chan int), make(chan int)
go func() { for v := range req { resp <- v + 1 } }()
for i := 0; i < n; i++ { req <- i; s += <-resp & 1 }
```

Weak points, in order of cost:

1. Every `scope.io` is two OS thread hand-offs. A job that does not block (`env.get`, most of `time`) should not be a job. The known alternative is to run such work inline in the native; `async` exists on these only to reach `scope.io`.
2. The worker sleeps in `pthread_cond_wait` whenever its queue is empty, even when a job is one syscall from done. No spinning before the sleep.
3. Ten Rust heap allocations per round trip: the block, the boxed job, two `Arc`s, the Tokio task, the inbox `Vec` growing from empty each time (`take_woken` takes the vector with `mem::take`, so the next `wake` allocates again).
4. One `baml-io` thread runs every job. Blocking work goes to Tokio's blocking pool, which adds a third thread and two more hand-offs.

## Not built

- Completion without a thread hop for work that is ready immediately.
- An I/O poller owned by the worker (kqueue or io_uring on the worker thread) in place of a second runtime.
- More than one I/O thread.
