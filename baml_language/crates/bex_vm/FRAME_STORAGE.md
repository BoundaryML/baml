# VM call-frame storage budget (B-1842)

This is the primary **10 MiB frame-storage budget** solution for [B-1842](https://linear.app/boundaryml2/issue/B-1842/fixed-256-frame-vm-call-stack-max-frames-catch-alls-arm-doesnt-catch), independent of the emergency proposal to set `MAX_FRAMES = 10_000`. Neither proposal should be merged automatically.

## Baseline and design

The baseline is canary `a09a7199a57d491a4ea4239624d4f335af77da84`, refreshed from `origin/canary` before implementation. Its `vm.rs` still defines `MAX_FRAMES = 256`, checked separately by the general call dispatcher and the compact `CallExactArgs` path. Entry frames and native continuation pushes use the same vector but bypass those checks. Each `BexVm` owns its frame storage; spawned tasks get separate VMs. `EvalStack` stores arguments, locals, and operands separately, and `BexHeap` is shared across VMs.

The design recorded before implementation is to put the limit at the common frame-storage allocation boundary. Lazily grow the backing array geometrically, capped by the constant `10 * 1024 * 1024` bytes. The usable slot capacity is the budget divided by the running build's `size_of::<Frame>()`, rounded down; the count is a consequence of the layout, not a separately selected recursion ceiling. Both bytecode frames and native continuation frames consume one full `Frame` slot. Reserve before call-setup side effects, and prevent other operations from growing the backing storage.

Accounting covers **allocated backing capacity**, including unused slots, inline fields, enum layout, and padding. It excludes allocator bookkeeping/size-class rounding and transient allocator-internal reallocation overhead, the storage owner's inline header, metadata/type/context/continuation pointees, the evaluation/value stack, shared heap objects, telemetry and traceback allocations, and the host/native stack. This is a per-VM frame-storage budget, **not a general VM heap budget** or a process RSS limit. Generic calls and native callbacks can retain additional memory outside this budget. Multiple spawned tasks each have a budget; this is not a process-wide limit.

The repository already uses `Vec::try_reserve` / `try_reserve_exact` to report allocation failures in native byte-array operations and separately budgets profiler storage. Heap allocation-debt/GC pressure accounting tracks heap spending, not this frame vector; it is not a call-frame limit. The frame-storage owner uses checked `Layout::array::<Frame>` layouts and fallible global `alloc`/`realloc`, handing successful allocations back to `Vec` for normal element destruction/deallocation. This controls the exact usable backing capacity without delegating to `Vec::push`'s infallible geometric growth or assuming `try_reserve_exact`'s at-least capacity contract is a hard upper bound. A refused reallocation leaves existing frames intact. Raw allocation is confined to that boundary and tested with injected allocator refusal.

Budget exhaustion uses the existing `baml.panics.StackOverflow` controlled failure path. Allocator refusal uses the existing `baml.panics.AllocFailure` path. These remain BAML panics: an ordinary `catch_all` wildcard deliberately passes them through; a named panic arm or `catch_all_panics` may handle them. No handler-matching, assertion, or error-catching policy changes belong to this fix. The failure path can still need memory for the panic object, context, and traceback; it cannot guarantee recovery from global process OOM or a host native-stack overflow.

## Comparison with other runtimes

| Runtime | Rule and configuration | Relationship to this change |
| --- | --- | --- |
| CPython | `sys.getrecursionlimit()` exposes an interpreter recursion-depth count; `sys.setrecursionlimit()` changes it. The safe maximum is platform dependent and an excessive setting can crash the process. CPython 3.9's recursive-call check compares a thread's recursion depth with the interpreter's limit; this is an implementation detail, not a universal Python-language frame count. | Similar controlled recursion failure, but a count rather than this BAML backing-byte budget. Python version, extension calls, and native-stack behavior matter. |
| Node.js / V8 | V8's `stack_size` flag limits the native stack region it may use, in KiB. `StackGuard::ThreadLocal::Initialize` subtracts that byte size from the current native stack position. Node accepts V8 flags and exposes them through `--v8-options`; their availability/defaults are version dependent. The referenced Node 22 V8 sources use 864 KiB on ARM/ARM64 and IA32, 960 KiB for the other ASan case, and 984 KiB otherwise. Ordinary exhaustion constructs a `RangeError`. | A byte budget is analogous, but V8 guards native stack space; BAML guards a heap-allocated array of VM frame records. Node recursion is **not bounded solely by process out-of-memory**. Frame size, optimization, architecture, embedding, and available host stack affect depth. Increasing a V8 flag does not enlarge the OS thread stack. |
| Go | `runtime/debug.SetMaxStack(bytes)` controls a single goroutine's stack growth limit. Initial limits are 1,000,000,000 bytes on 64-bit and 250,000,000 bytes on 32-bit. The setting only affects future growth; system limits may also apply. Runtime stack growth doubles the backing stack and checks the proposed size against its limits. Exceeding them calls the fatal runtime throw path, rather than a recoverable ordinary panic. | Closest analogy is a per-execution-context growing backing store with a byte ceiling. Go's stack includes native Go call state/locals, unlike the BAML frame-record array. BAML reports a catchable BAML panic and does not claim to protect the host stack. |

Primary sources (read during this investigation):

- [Python 3.9 `sys.getrecursionlimit` and `sys.setrecursionlimit`](https://docs.python.org/3.9/library/sys.html#sys.setrecursionlimit), [CPython 3.9.6 recursive-call implementation](https://github.com/python/cpython/blob/v3.9.6/Include/internal/pycore_ceval.h#L54-L101).
- [Node CLI V8 flag documentation](https://nodejs.org/api/cli.html#v8-options), [Node 22.15.0 V8 stack-size flag](https://github.com/nodejs/node/blob/v22.15.0/deps/v8/src/flags/flag-definitions.h#L2070-L2072), [architecture-dependent defaults](https://github.com/nodejs/node/blob/v22.15.0/deps/v8/src/common/globals.h#L139-L165), [native stack guard initialization](https://github.com/nodejs/node/blob/v22.15.0/deps/v8/src/execution/stack-guard.cc#L235-L243), [RangeError construction on overflow](https://github.com/nodejs/node/blob/v22.15.0/deps/v8/src/execution/isolate.cc#L1644-L1680). The versioned source is Node 22.15.0; the installed binary measured below is a later Node 22 build, whose actual flag default was queried directly.
- [Go `SetMaxStack` documentation](https://pkg.go.dev/runtime/debug#SetMaxStack), [Go runtime default limits](https://go.dev/src/runtime/proc.go), [Go stack growth and fatal overflow checks](https://go.dev/src/runtime/stack.go). The same rules were checked in the installed Go 1.26.10 source under `src/runtime/{proc,stack}.go` and `src/runtime/debug/garbage.go`.

## Controlled comparison runs

Measured on macOS 26.6.2 (25G83), Darwin 25.6.0, ARM64. These small programs exercise `depth(n) = n == 0 ? 0 : 1 + depth(n - 1)`; depths are inputs to that function, not universal runtime frame capacities. Driver frames consume stack too. These are local observations, not promised portable thresholds.

| Runtime/build | Observed result |
| --- | --- |
| System CPython 3.9.6, Clang 21.0.0, arm64 | `sys.getrecursionlimit()` was 1000. A top-level call succeeded at input 997; 998, 999, and 1000 raised `RecursionError: maximum recursion depth exceeded in comparison`. After `sys.setrecursionlimit(2000)`, input 1500 returned 1500. |
| Node v22.23.4, V8 12.4.254.21-node.57, arm64, `--jitless` | `node --v8-options` reported `--stack-size=864`. Binary search in a single fresh `node -e` process found input 7846 successful and 7847 failing with `RangeError: Maximum call stack size exceeded`. JIT was disabled to avoid tiering changing frame layout during the search. |
| Same Node, `--jitless --stack-size=432` | The same search found 3896 successful and 3897 failing with the same `RangeError`. The reduction demonstrates the stack-byte flag affects recursion without exhausting process heap memory. |
| Go go1.26.10, darwin/arm64, normal `go build`, `//go:noinline` recursive function | `debug.SetMaxStack(64 * 1024)` returned the previous setting 1000000000. Input 1000 returned 1000; input 10000 in a separate process exited with status 2 and `runtime: goroutine stack exceeds 65536-byte limit` / `fatal error: stack overflow`. A deferred `recover()` did not run. This deliberately small limit avoids attempting a default-limit crash with a gigabyte stack. No maximum successful Go depth was measured. |

## BAML measurement method

Build from `baml_language/` with the pinned Rust 1.98.0 toolchain (`88d9e12ae`, LLVM 22.1.8), target `aarch64-apple-darwin`, default features, and the unmodified `release` profile (`opt-level=3`, fat LTO, one codegen unit). The small `frame_budget` Rust probe compiles a parameterized BAML function and observes VM storage after every opcode yield. It measures simultaneous live frames and backing capacity, rather than inferring depth from a traceback or a cross-platform frame-size assumption. Every-op yields add measurement overhead but no BAML frames. CLI runs separately check the full engine/host path without that instrumentation.

The recursive function is deliberately non-tail-recursive: `function depth(n: int) -> int { if (n == 0) { 0 } else { 1 + depth(n - 1) } }`, entered through `function main(n: int) -> int { depth(n) }`. Thus input `n` needs `n + 2` simultaneous frames: `main`, and `depth(n)` through `depth(0)`. A different wrapper, callback, trace hook, or program can consume additional frames. The bytecode interpreter iterates over these calls; it does not recurse once on the host Rust stack per BAML frame.

Run the manual probe with `BAML_PROBE_DEPTH=<n> cargo test --release -p bex_vm --test frame_budget measure_frame_capacity -- --ignored --nocapture`. For the full-engine repro, put the two functions above in a minimal project's `baml_src/main.baml`, then run `target/release/baml-cli run --from <project> --function main -- main --n <n>`. The ticket's named, ordinary wildcard, and panic wildcard wrappers provide the matching positive and negative controls. Keep each failed run's diagnostic and exit status; a controlled BAML `StackOverflow` is distinct from a fatal host `stack overflow` abort.


## Baseline measurements

The saved baseline CLI and release probe execute the pinned baseline. The probe links the unchanged library artifacts from that build; its binary was copied before the fixed release rebuild. The CLI reproduction above completed before implementation began.

| Baseline observation | Result |
| --- | --- |
| `size_of::<Frame>()` / alignment | 112 / 8 bytes |
| `size_of::<BytecodeFrame>()` / `size_of::<NativeFrame>()` | 112 / 32 bytes; each stored enum slot still occupies 112 bytes |
| Input 254, VM probe and full CLI | Returned 254; peak 256 frames, capacity 256 slots, 28,672 backing bytes |
| Input 255, VM probe and full CLI | Controlled `baml.panics.StackOverflow {message: "stack overflow"}`; peak/capacity 256 slots; probe traceback had 256 frames; CLI exited 1 |
| Input 1000 | Same controlled overflow at 256 live frames; no process abort |
| Named overflow arm at input 1000 | Returned -1, CLI exited 0 |
| Ordinary `catch_all` wildcard at input 1000 | Panic propagated; same diagnostic, CLI exited 1 |
| `catch_all_panics` wildcard at input 1000 | Returned -3, CLI exited 0 |

These reproduce the ticket's adjacent successful/failing inputs on the newer pinned canary, rather than relying on the ticket's older `af4bf2dba2` observation. The local CLI printed 261 total output lines for the uncaught case, including warnings; the useful invariant is the 256-frame peak, not an exact presentation-line count.

## Fixed release measurements

The fixed release probe has the same 112-byte `Frame` layout and 8-byte alignment as the baseline. The rule is `capacity * 112 <= 10,485,760`, so the last usable capacity is 93,622 slots / 10,485,664 bytes, with 96 bytes left over. This is a layout-derived capacity, not a new fixed recursion constant. The last growth is clamped to that capacity rather than doubling past the budget.

| Fixed release VM probe | Measured result |
| --- | --- |
| Input 1000 | Returned 1000; peak 1002 live frames, allocated capacity 1024 slots / 114,688 bytes |
| Input 93,620 | Returned 93,620; peak 93,622 live frames, capacity 93,622 slots / 10,485,664 bytes |
| Input 93,621 | Controlled `panics.StackOverflow`; peak and capacity remained 93,622 slots / 10,485,664 bytes |
| Exact and generic recursive dispatch | Both reached the layout-derived maximum successfully; requesting the next frame on the same reused VM failed through `StackOverflow` |
| Catching controls at overflow | The named arm and `catch_all_panics` recovered and called another function; ordinary `catch_all` propagated `StackOverflow` |
| Indirect and native callback recursion | Positive cases beyond 256 live frames succeeded; over-budget cases produced `StackOverflow` with backing capacity still bounded |

The maximum successful input above is measured for this two-function program and default-feature build, not promised for every BAML program or target. Native continuations consume full enum slots; callbacks and wrappers reduce source-level nesting for the same byte budget. Generic metadata, evaluation values, heap objects, and exception traces are additional allocations. Release optimization, feature flags, target pointer width, and future field changes can change layout/capacity; regression tests derive their limits from the executing build. The CLI and VM probe are separate controls: the probe can inspect allocation capacity; the CLI verifies the actual user-facing panic diagnostic and host integration.


The full-engine release CLI independently confirmed input 93,620 returning 93,620 (exit 0) and inputs 93,621 and 100,000 failing with `uncaught throw: baml.panics.StackOverflow {message: "stack overflow"}` (exit 1). At input 93,621 the named wrapper returned -1, the ordinary `catch_all` wildcard propagated the same panic (exit 1), and `catch_all_panics` returned -3. At input 1000 all three wrappers returned 1000 normally; their overflow arms were not taken. The uncaught boundary run printed 93,627 lines including CLI warnings and traceback; traceback construction/formatting is additional memory and work outside the frame-storage budget.
