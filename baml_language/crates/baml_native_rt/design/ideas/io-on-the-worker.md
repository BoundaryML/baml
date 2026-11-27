# I/O without a thread hop

Status: proposed. Nothing here is built.

Numbers marked measured come from `../natives.md` and `../tasks.md` (commit `39d3ad4c25`, arm64 macOS, instructions retired, kernel and every thread included unless it says otherwise). Everything else is an estimate or a prediction and says so. Statements about Tokio's scheduler, napi-rs and pyo3 are from knowledge of them, not from a test here.

## The problem

Every `scope.io` crosses OS threads twice. The cheapest native that uses it is `baml.env.get`, whose job is one `getenv`:

```baml
let v = baml.env.get("SOME_VARIABLE");
```

Measured per call: 39,852 instructions, 6.2 microseconds, 2.0 involuntary context switches.

What the same thing costs elsewhere (measured):

- Go, `os.LookupEnv` in place: 272 instructions. 147 times less.
- Go, a request handed to another goroutine over unbuffered channels and the answer received back: 3,711 instructions, 2,958 with `GOMAXPROCS=1`, about 260 ns. This is the fair comparison for the mechanism. We are 10.7 times the instructions and about 24 times the wall time.
- Node, a promise settled by `setImmediate`: 18,936 (approximate).

```go
req, resp := make(chan int), make(chan int)
go func() { for v := range req { resp <- v + 1 } }()
for i := 0; i < n; i++ { req <- i; s += <-resp & 1 }
```

### The two are not doing the same thing

Go's hand-off stays in user space on one OS thread most of the time. Ours goes through the kernel in both directions:

1. The worker parks the task and submits the job to Tokio: remote queue push, `mio::Waker::wake`, a `kevent` syscall to wake `baml-io`.
2. The worker has nothing to run, so it sleeps in `pthread_cond_wait` (`__psynch_cvwait`). There is no spin phase and no futex-style fast path.
3. `baml-io` wakes from `kevent`, runs the job, locks the inbox mutex, pushes the task, calls `pthread_cond_signal` (`__psynch_cvsignal`), and parks again.
4. The worker wakes, takes the task from the inbox, and resumes it.

That is at least four kernel entries and two context switches per job, whatever the job does. A system call that does nothing is about 1,090 instructions on this machine. The syscalls are the smaller part; the rest is the kernel scheduling two threads on and off a core.

### Where the 39,852 goes

About 34,000 are the kernel and the `baml-io` thread. The worker thread in user mode is 5,565 (single-step trace), made of:

- driver stub, anchor stubs, `native_poll` twice, `poll_block`: 384
- the generated symbol and block setup: 58
- `baml_rt_io_wait` and `io_park`: 61
- `start_on_tokio`, Tokio's spawn, remote push, unpark: 348
- `serve`, `take_woken` twice, `fire_timers` twice, `baml_rt_resume`: 224
- drops of the block, the job's result slot, the `Arc`s: 71
- mutex and condvar calls into libpthread: 751
- `malloc`, `free`, `memcpy` for ten allocations: 3,597 under the debugger, an estimated 1,500 to 2,000 outside it

So the runtime and Tokio's submit path are about 1,150, and the scheduler's own share is 224. The ten allocations are the native's block, the boxed job, two `Arc`s, the Tokio task, and the inbox `Vec` growing from empty each time (`take_woken` takes it with `mem::take`, so the next `wake` allocates again).

The `baml-io` thread's own user-mode share was not measured; only the whole-process total and the worker's trace exist.

### The task switch is not the cost

A park and resume through the worker is 60 instructions of assembly. `spawn` plus `await` is 466 against Go's 4,645. Parking a task is already a user-space operation and cheaper than Go's. The kernel entries come only from the reactor living on a different thread than the scheduler.

### How Go avoids it

Parking a goroutine does not involve the kernel. The thread keeps running other goroutines. When a goroutine waits on a socket, the same thread that runs goroutines calls `netpoll` from its scheduler loop, and only when it has nothing left to run does it enter the kernel, with one `kevent` that waits on every socket at once. Under load that is about one syscall per batch of ready sockets, plus the `read` and `write` calls nobody can avoid. A monitor thread polls the network periodically and preempts goroutines that run too long, so compute does not starve I/O.

### Same shape as the language bridges

This is how napi-rs and pyo3 bridge async Rust: the future runs on a Tokio thread and the completion is posted to the one thread that owns the language. napi-rs posts with a threadsafe function (`uv_async_send`, a write to a pipe or eventfd that wakes libuv). pyo3's async bridge uses `loop.call_soon_threadsafe`, a write to asyncio's self-pipe, with the GIL hand-off on top. Expect the same few microseconds per call or worse. Neither was measured here. So today's design is at par with the bridges and far behind Go.

### What the number overstates

39,852 is the worst case for the design in two ways.

- The job takes almost no time. For a real network read the job dominates and the hop matters less, though two context switches per request is still a real cost against Go's poller.
- It is idle ping-pong: one task, one job, both threads asleep between each. Under load the wakes should coalesce, because a worker with runnable tasks never sleeps and an I/O thread with queued jobs never parks. The per-operation cost with 100 concurrent requests should be several times lower. That is a prediction; only the single-task case has been measured, and networking under concurrency is the case that matters.

### Other weak points of today's path

- One `baml-io` thread runs every job.
- Blocking work (`tokio::fs`, `spawn_blocking`, DNS) goes to Tokio's blocking pool, which adds a third thread and two more hand-offs.

## Part 1: work that does not need to stop should not stop

`baml.env.get` and most of `time` are `async` only to reach `scope.io`. The first idea was to flip their flag in `natives/*.txt` from `async` to `allocates` or none and call libc directly. That is rejected, for two reasons.

### The environment belongs to the host

BAML will be bridged into Python, Node and Rust programs as well as run standalone, and each has different rules. The bridge side is not being designed now; it is a constraint known to be coming. What it fixes today is that reads and writes must bind to the same function, whatever the host makes that function.

- Node's `process.env` can only be read on the JS thread.
- Python's `os.environ` needs the GIL.

So under those bridges a read may really have to park the task. Only standalone can answer on the calling thread. The native therefore cannot be declared non-stopping and cannot call `std::env` directly.

A future `baml.env.set` (not in the standard library yet) must be seen by the host's own code when it reads the same variable. That rules out hard-coding `std::env::set_var` as much as it rules out a BAML-only copy.

### `env.set` and the `setenv` race

Separately from the bridges: in a standalone process there is one environment. `std::env::set_var` and `std::env::var_os` take an internal lock, so a get on the worker and a set on another thread are serialized, and any Rust code sees the new value whichever thread ran either. Where the get runs does not change that.

The hazard is C code that calls `getenv` without Rust's lock. The usual one is `getaddrinfo` during DNS, on Tokio's blocking pool. If `setenv` reallocates the environment block while that is reading it, it is a use-after-free. This is why `set_var` became `unsafe` in the 2024 edition. Routing calls through `baml-io` does not fix it: the set would be on `baml-io` and the lookup on a pool thread.

Three shapes `env.set` could take in standalone:

- A real `setenv`, with the C-reader race documented. What Go and Python do; Go locks its own reads and writes and says C code is on its own.
- A BAML-side overlay only: `set` writes a map the runtime owns and `get` checks it first. No race, but Rust and C code do not see it, and child processes do not unless the overlay is passed at spawn.
- An overlay plus a real `setenv` done only while nothing else runs. Safe only if every other thread can be stopped, which Tokio's pool cannot.

No choice is made here. Under the host interface below it is the standalone host's decision, and the first shape is the likely one.

To check when this is built: whether today's job calls `std::env::var_os` or `libc::getenv`. If it is the raw call, the standalone host should use `std` so the lock covers it.

### The proposal: ask the host, stop only if it says so

One binding point that every environment read (and later write) goes through. The host says whether the answer is available on this thread:

```rust
// one per process, installed at startup by whoever embeds the runtime
pub trait Host: Send + Sync {
    fn env_get(&self, name: &str) -> HostCall<Option<String>>;
    // env_set and the rest, when the bridge work needs them
}

pub enum HostCall<T> {
    Ready(T),                       // answered on the calling thread
    Later(BoxFuture<'static, T>),   // needs the host's thread
}
```

```rust
// native/env.rs
async fn get(scope: &mut Scope<'_>, name: Str<'_>) -> Result<...> {
    match host().env_get(name.text(scope)) {
        HostCall::Ready(value) => value,                 // finishes on its first poll
        HostCall::Later(job)   => scope.io(job).await,   // parks, as today
    }
}
```

The standalone host returns `Ready` from `std::env::var_os`. A Node host returns `Later` and goes to the JS thread. The BAML code and the native are the same in both. `scope.io` stays as the slow path.

The fast path is not inside `scope.io`. It is the native returning without awaiting anything that stops: the compiled driver stub calls the native, polls once, gets `STEP_DONE`, and returns. No park, no Tokio, no second thread.

Cost: an `async` native that completes without stopping is 236 instructions of machinery (measured: driver stub 21, two anchor stubs 8, the generated symbol building `Head`, `Scope` and the future 72, `native_poll` 43, `poll_block` 60, drop 32) plus one `malloc` and `free`. So about 400 to 500 in all, against 39,852 today and Go's 272. About 99% of the gap, with nothing committed about the bridges.

The remaining 200 or so over Go is the heap block every `async` native allocates. That applies to every `async` native that finishes at once, not only this one, and is one of the open poll-path questions below.

### Why the check is not inside `scope.io`

`scope.io` could poll the job once on the worker with a no-op waker and skip the request when it is already `Ready`. Every caller would get the fast path for free, and it would help anything that happens to be ready (a socket read with data buffered, a file already open). Rejected:

- it runs arbitrary job code on the worker thread;
- a job that blocks instead of returning `Pending` stalls every task;
- Tokio types expect to be polled inside a runtime context, and some would panic.

So the decision is explicit at the call site, per host answer, and `scope.io` keeps meaning "this goes to the reactor".

### Scope

Only `env_get` is proposed now. Arguments, the clock (if hosts want to mock time) and standard output are the same kind of host-owned thing and can join the trait when the bridge work starts; guessing them today is the design work that was deferred. The `time` natives stay as they are until it is decided whether the clock is the host's.

Size: about half a day. Around 40 lines for the trait and the standalone host, one caller. Low risk; the conformance suite covers it.

## Part 2: put the reactor where the scheduler is

For work that does wait (sockets, HTTP, SSE), the kernel entries go away only if parking a task does not involve a second thread. Four options were considered. The last is the proposal.

### Option A: polish the two-thread design

Keeps `baml-io`. Three independent pieces.

- Spin before sleeping. In `Sched::sleep`, check the inbox's `filled` atomic in a short loop before taking the mutex and waiting. About 15 lines, a few hours. A fast job then comes back to a worker that never slept, and `pthread_cond_signal` with no waiter stays in user space. It removes only the worker's wake-up, roughly half the kernel cost, and only for jobs that finish within the spin. The spin count needs tuning: too long burns CPU in an idle program, too short does nothing. Measure before keeping it.
- A cheaper park. Replace the mutex and condvar with one atomic and `__ulock_wait` or a futex. Perhaps 10 to 20% of the worker's side.
- Fewer allocations. Swap the inbox `Vec` with a reused buffer instead of `mem::take` (ten lines). The same for the scratch `Vec` that `fire_timers` allocates on every call that fires anything. Put the boxed job and its result slot in the native's existing block and drop one `Arc`; that touches `Scope::io` and `start_on_tokio` and needs some unsafe lifetime work, about a day. Saves an estimated 1,500 of 39,852, about 4%.

All of it is cheap and all of it is thrown away if the reactor moves. Skip it until the reactor's place is decided.

### Option B: a current-thread Tokio runtime owned by the worker

```rust
// Sched, created at the first scope.io as today
io: tokio::runtime::Builder::new_current_thread().enable_all().build()
```

- `scope.io` spawns onto that runtime's local queue from the worker: no remote push, no `mio` wake, no `kevent`. The job's completion wrapper pushes the task straight onto the run queue, with no inbox mutex or condvar.
- `Sched::sleep` becomes "drive the reactor". Where the worker blocks in `pthread_cond_wait` it blocks inside Tokio instead, which is the one `kevent` over every socket:

```rust
// nothing runnable
self.io.block_on(async {
    tokio::select! {
        _ = self.woken.notified() => {}           // some job finished
        _ = sleep_until(next_baml_timer) => {}
    }
});
```

- The reactor also gets a non-blocking turn every N task switches while tasks are runnable, or a busy run queue starves I/O. Go's scheduler does the same with `netpoll`.
- This runs on the worker's own stack, never a task's, so no Rust frame is left on a stopped task.
- The inbox stays for threads we do not own: file I/O and DNS still go to the blocking pool and wake the runtime through Tokio's waker.
- BAML timers already live in the scheduler and become the poll's timeout.

About three days. Unknown without trying: Tokio has no "turn the reactor once" call, so every drive is a `block_on` entry and exit with some fixed cost.

What it gives up:

- The free second core. Today a TLS handshake, decompression or a large body decode runs on `baml-io` while BAML code keeps running. On the worker, a job that computes for 1 ms stalls every task for 1 ms.
- I/O latency under compute. With no preemption, a task that loops for 50 ms means no socket is serviced for 50 ms. Today `baml-io` keeps reading and only the hand-back waits.
- Several workers. A current-thread runtime is tied to its thread. N workers means N reactors, and an idle worker cannot poll a busy worker's sockets.

### Option C: our own kqueue and epoll poller

Go's shape exactly: one poller, polled by any idle worker in `serve`. It needs:

- a kqueue instance (epoll on Linux) polled where the worker now sleeps;
- our own non-blocking read, write, accept and connect natives;
- timers folded into the poll timeout, which is easy since they are already the scheduler's;
- a way for another thread to wake the poll (a pipe or `EVFILT_USER`), since DNS and file I/O still need a thread.

One to two weeks for that. The expensive part is what sits on Tokio today. `baml.http` uses reqwest and rustls, which are bound to Tokio. Either HTTP keeps the thread hop, and the win is raw TCP and SSE only, or an HTTP client is written on our sockets, which is a much larger project and runs into the external-crypto and FIPS work. It becomes a real alternative if HTTP ends up provided by the host, which the vendoring work already wants.

### Option D, proposed: BAML workers inside a multi-thread Tokio runtime

A multi-thread runtime beside the workers is what exists today with more I/O threads; every `scope.io` still hops. The condition for it to work is that the BAML workers run inside Tokio's worker threads. Each worker's `serve` loop is itself a Tokio task:

```rust
// one per BAML worker, spawned on the multi-thread runtime
async fn serve(worker: &mut Mutator) {
    loop {
        run_baml_tasks_until_idle_or_budget(worker);   // switches to task stacks and back
        if worker.sched.idle() {
            worker.sched.woken.notified().await;       // hand the thread to Tokio
        } else {
            tokio::task::yield_now().await;            // let jobs and the reactor run
        }
    }
}
```

What follows:

- `scope.io` calls `tokio::spawn` from a Tokio worker thread, so the job lands on that thread's local queue. No remote push and no wake.
- When `serve` yields, the same thread polls the job. With nothing left, Tokio parks the thread on the shared reactor: the one `kevent`.
- The reactor is shared, which removes Option B's objection. Any idle Tokio thread can steal a queued job, so while one thread runs BAML code another can take a TLS handshake. The second core is not lost.
- reqwest and rustls keep working unchanged.
- No Rust frame is left on a stopped task: `serve` switches back to its own stack before it returns to Tokio.
- File I/O and DNS still hop to the blocking pool. No change there.

What does not come for free:

- `serve` must yield on a budget. While it runs BAML tasks its thread polls nothing, and jobs on its local queue wait for a steal or the yield. Tokio's newest-task slot cannot be stolen, so a job spawned just before a long stretch of compute can sit until the yield.
- A task that computes without parking delays I/O on its thread. The loop poll is the place to ask a running task to yield, and nothing sets the poll word yet (`../control.md`). Go covers this with its monitor thread and preemption.
- Tokio may wake a parked thread on every spawn so it can look for work. That is a kernel entry per `scope.io` when threads are idle, off the critical path but counted. With `worker_threads(1)` it does not happen, and neither does stealing.
- A wake between threads is still a system call. If one thread finishes a stolen job whose BAML task belongs to a parked thread, it wakes that thread through the kernel. Go pays the same.
- Tokio steals jobs, not BAML tasks. Balancing BAML tasks across workers (a shared run queue or our own stealing) is still ours to build.
- The main thread cannot be the worker. A thread inside `block_on` is not a Tokio worker and its spawns go through the remote queue, which is the hop again. The main worker has to be a spawned task while the main thread waits.
- A program that does no I/O should not pay for Tokio. Linking it costs every start about 0.8 M instructions and a second thread's symbols about 0.6 M (`natives/README.md`), which is why it is linked only when a native that uses `scope.io` is called. The worker starts plain and moves onto Tokio at the first `scope.io`. Moving a worker between threads is something the design already supports and stress-tests (`BAML_MIGRATE_STRESS`).
- Under a bridge none of this applies. Node and Python own their event loops. This is the standalone host's implementation and sits behind the same `Host` interface as `env_get`.

### An escape for work that should leave the worker

If stealing proves unreliable for jobs that compute, or under Option B where there is no stealing, the native says which it wants:

```rust
scope.io(job).await        // on this thread's queue, no hop
scope.offload(job).await   // always another thread, for work that computes or blocks
```

## Order of work

1. Part 1: the `Host` trait with `env_get`. Half a day, independent of everything else, and it fixes the measured case entirely.
2. Measure today's design with 100 concurrent requests, to learn how much the idle-case number overstates. About an hour.
3. Prototype Option D with `worker_threads(1)`: the single-worker case with no hops. About three days. Measure the single-task case and the concurrent one against step 2.
4. Raise the thread count and measure what stealing costs (the wake on spawn) and buys (jobs running beside BAML code).

Option A is skipped unless step 3 fails. The choice between Tokio and our own poller (Option C) is left to the several-workers design, since each shapes the other.

## Open questions

About this proposal:

- The fixed cost of one `yield_now` round trip through Tokio, per turn of `serve`.
- Whether Tokio's wake on spawn can be avoided or is cheap enough.
- The budget: how many task switches before `serve` yields.
- Whether HTTP ends up provided by the host. If so, Tokio on the standalone path is justified by raw sockets and SSE alone.
- What `baml.env.set` means in standalone, and whether the clock, arguments and standard output belong to the host.
- How this meets several workers (`../threads.md`): one `Mutator` per Tokio worker thread, how a collection stops them, and item 23 there (one Tokio runtime with one thread serving every worker).

About the poll path every `async` native shares, asked earlier and not yet answered:

- What the instruction budget for an `async` native is.
- Which callers matter most.
- Whether "no Rust frame on a task's stack while compiled code runs or the task is stopped" must hold for every native, or whether a second native form that calls BAML but cannot be parked across is allowed. It would keep its frame on the stack, which the migration rule forbids today.
- Whether the `worker.polling` store on every poll stays in release builds.
- Whether the block can avoid a `malloc` per call. This is the 200 or so instructions `env.get` would still be over Go after Part 1.

For reference, the measured poll-path costs those questions are about: a `call_baml` round trip is 269 instructions without the callee, of which about 110 are two full polls of the Rust future and 110 the two ends of `call_baml`; `==` on two small class instances is 3,268 because of five `malloc`/`free` pairs and a SipHash visited set.

## Not measured

- Today's path under concurrency.
- The `baml-io` thread's user-mode share of a round trip.
- napi-rs and pyo3.
- Any figure for Options A to D. Every cost given for them is an estimate.
