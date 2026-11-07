# Runtime v2 task loop

Single-owner stackful scheduler for `FnOnce(&mut TaskCx) -> Result<Value, Err>`.
Provides FIFO scheduling, pooled stacks, spawn/await/ordered await_any, timers,
owned Tokio I/O, cancellation, shielding, admission gates and host submission.
The Tokio runtime must be multithreaded; all managed execution stays on the owner.

Spawn queues work without switching. Awaiting a settled future does not switch.
Polls check cancellation and service arrivals; a switch needs another ready task
and an expired slice. Bodies must poll cooperatively. Task generations and I/O
epochs reject stale wakeups. Shielding delays cancellation, not token propagation.
Shutdown rejects submissions, allows a grace interval, then cancels and unwinds
remaining fibers. A body that never polls can exceed that interval.

`LoopHeap` and `RootSource` connect synchronous collection to the scheduler.
Root callbacks must not retain addresses. Running registers are spilled before
scanning the current stack. Before every suspension a non-inlined frame spills
callee-saved registers into a local buffer and records SP below it; the frame
remains live across the switch. Parked roots cover `[recorded_sp, stack_base)`.
The saved pointer is cleared on resume; never-started fibers have no saved range.
AArch64 captures x19–x30 and d8–d15; x86_64 captures rbx/rbp/r12–r15.
Other native targets fail compilation. No private coroutine layout is inspected.

Precise slots cover task/future values and integration root leases. Rust closure
captures, vectors and boxes are not implicit managed roots. Host submissions and
pending I/O carry owned values; hydration happens on the owner thread.
`IntegrationRoots` gives the runtime synchronous root access and stack bounds.

From `baml_language`, with `CARGO_TARGET_DIR` inside your worktree:

```sh
cargo test -p bex_v2_loop
cargo test -p bex_v2_loop --release
cargo run -p bex_v2_loop --release --example measure
cargo run -p bex_v2_loop --release --example profile
```

`instrument` enables scheduler counters for separate measurement runs.
See `architecture/runtime-v2/{INTERFACES,CONTRACTS,PHASE1-REPORT}.md`.
