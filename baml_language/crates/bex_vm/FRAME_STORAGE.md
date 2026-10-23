# VM stack storage budget (B-1842)

This document describes the **10 MiB shared frame/value-stack storage budget** for [B-1842](https://linear.app/boundaryml2/issue/B-1842/fixed-256-frame-vm-call-stack-max-frames-catch-alls-arm-doesnt-catch).

## Baseline and design

The baseline is canary `a09a7199a57d491a4ea4239624d4f335af77da84`, refreshed from `origin/canary` before implementation. Its `vm.rs` still defines `MAX_FRAMES = 256`, checked separately by the general call dispatcher and the compact `CallExactArgs` path. Entry frames and native continuation pushes use the same vector but bypass those checks. Each `BexVm` owns its frame storage; spawned tasks get separate VMs. `EvalStack` stores arguments, locals, and operands separately, and `BexHeap` is shared across VMs.

The limit belongs at the allocation boundary shared by frame records and the separate argument/local/operand slots in `EvalStack`. Each VM lazily grows two backing arrays under one constant `10 * 1024 * 1024` byte budget. The invariant is `frames.capacity() * size_of::<Frame>() + stack.capacity() * size_of::<Value>() <= STACK_STORAGE_BUDGET_BYTES`. Bytecode frames and native continuations each occupy a full `Frame` enum slot. A function's argument, real-local, and temporary operand slots occupy `Value` slots in the separate value array, so functions with more live slots consume more budget and reach overflow at smaller depths. There is no separately chosen maximum frame count.

Accounting covers **allocated backing capacity** of both arrays, including unused reserved slots, inline fields, enum layout, and padding. It excludes allocator bookkeeping/size-class rounding, the storage owners' inline headers and shared counter, metadata/type/context/continuation pointees, temporary argument-remapping and native callback vectors, shared heap objects referenced by values, telemetry and traceback allocations, and the host/native stack. This is a per-VM stack-storage budget, **not a general VM heap budget** or a process RSS limit. Each spawned task has its own budget, so the total across tasks can exceed 10 MiB.

`StackStorage<T>` owns each array and shares a byte counter between them. All growth uses safe, fallible `Vec::try_reserve_exact`; the allocation module forbids unsafe code. Growth is geometric and clamped to the bytes available after accounting for the other array. Because `try_reserve_exact` promises at least the requested capacity, growth allocates a replacement, checks and charges its observed capacity before installation, then moves the existing initialized elements using safe `Vec::append`. Allocation or budget refusal preserves the original array and its contents. The old and new buffers coexist briefly during copying; this bounded transient replacement memory is not counted twice in the steady-state 10 MiB budget. No raw allocation/reallocation or `Vec::from_raw_parts` is needed.

Spare capacity remains charged after a return or unwind and can be reused without allocating. Consequently practical maximum depth depends on both array growth histories as well as program slots and record layout; it is not `10 MiB / size_of::<Frame>()`. The regression tests measure a fresh VM's actual success/failure boundary and check reuse separately. The budget also bounds value storage in shallow functions with many operands or locals, rather than only guarding recursive call setup.

All frame reservations and value insertions, extensions, receiver insertions, and local resizes use this boundary. Callee locals are reserved before publishing a new frame so a setup failure unwinds from the caller. Expanding callback arguments is included in normal unwinding. The existing void entry API and host-result push API retain a raw allocation/budget panic on failure and deliver it through the normal unwinder on the next `exec`; they do not silently complete, construct an unrooted pending heap exception, or change matching policy. The repository's heap allocation-debt/GC-pressure accounting still tracks separate shared-heap spending and is not used as this stack limit.

Budget exhaustion uses the existing `baml.panics.StackOverflow` controlled failure path. Allocator refusal uses the existing `baml.panics.AllocFailure` path. These remain BAML panics: an ordinary `catch_all` wildcard deliberately passes them through; a named panic arm or `catch_all_panics` may handle them. No handler-matching, assertion, or error-catching policy changes belong to this fix. The failure path can still need memory for the panic object, context, and traceback; it cannot guarantee recovery from global process OOM or a host native-stack overflow.

## Comparison with other runtimes

| Runtime | Rule and configuration | Relationship to this change |
| --- | --- | --- |
| CPython | CPython defaults to 1,000 interpreter recursion levels; `sys.getrecursionlimit()` exposes the current count limit; `sys.setrecursionlimit()` changes it. The safe maximum is platform dependent and an excessive setting can crash the process. CPython 3.9's recursive-call check compares a thread's recursion depth with the interpreter's limit; this is an implementation detail, not a universal Python-language frame count. | Similar controlled recursion failure, but a count rather than this BAML backing-byte budget. Python version, extension calls, and native-stack behavior matter. |
| Node.js / V8 | V8's `stack_size` flag limits the native stack region it may use, in KiB. `StackGuard::ThreadLocal::Initialize` subtracts that byte size from the current native stack position. Node accepts V8 flags and exposes them through `--v8-options`; their availability/defaults are version dependent. The referenced Node 22 V8 sources use 864 KiB on ARM/ARM64 and IA32, 960 KiB for the other ASan case, and 984 KiB otherwise. Ordinary exhaustion constructs a `RangeError`. | A byte budget is analogous, but V8 guards native stack space; BAML guards heap-allocated frame records plus their separate argument/local/operand slots. Referenced heap allocations are outside both stack budgets. Node recursion is **not bounded solely by process out-of-memory**. Frame size, optimization, architecture, embedding, and available host stack affect depth. Increasing a V8 flag does not enlarge the OS thread stack. |
| Go | `runtime/debug.SetMaxStack(bytes)` controls a single goroutine's stack growth limit. Initial limits are 1,000,000,000 bytes on 64-bit and 250,000,000 bytes on 32-bit. The setting only affects future growth; system limits may also apply. Runtime stack growth doubles the backing stack and checks the proposed size against its limits. Exceeding them calls the fatal runtime throw path, rather than a recoverable ordinary panic. | Closest analogy is a per-execution-context growing backing store with a byte ceiling. Go's stack includes native Go call state and locals; BAML accounts for its interpreter frame records and value slots in two heap-allocated arrays. BAML reports a catchable BAML panic and does not claim to protect the host stack. |

Primary sources (read during this investigation):

- [CPython 3.13.5 default recursion limit](https://github.com/python/cpython/blob/v3.13.5/Include/internal/pycore_ceval.h#L39-L41), [Python 3.9 `sys.getrecursionlimit` and `sys.setrecursionlimit`](https://docs.python.org/3.9/library/sys.html#sys.setrecursionlimit), [CPython 3.9.6 recursive-call implementation](https://github.com/python/cpython/blob/v3.9.6/Include/internal/pycore_ceval.h#L54-L101).
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

Build from `baml_language/` with the pinned Rust 1.98.0 toolchain (`88d9e12ae`, LLVM 22.1.8), target `aarch64-apple-darwin`, default features, and the unmodified `release` profile (`opt-level=3`, fat LTO, one codegen unit). The small `frame_budget` Rust probe compiles a parameterized BAML function and observes VM storage after every opcode yield. It measures simultaneous live frames, live value slots, and both backing capacities, rather than inferring depth from a traceback or a cross-platform frame-size assumption. Every-op yields add measurement overhead but no BAML frames. CLI runs separately check the full engine/host path without that instrumentation.

The recursive function is deliberately non-tail-recursive: `function depth(n: int) -> int { if (n == 0) { 0 } else { 1 + depth(n - 1) } }`, entered through `function main(n: int) -> int { depth(n) }`. Thus input `n` needs `n + 2` simultaneous frames: `main`, and `depth(n)` through `depth(0)`. A different wrapper, callback, trace hook, or program can consume additional frames. The bytecode interpreter iterates over these calls; it does not recurse once on the host Rust stack per BAML frame.

Run the manual probe with `BAML_PROBE_DEPTH=<n> cargo test --release -p bex_vm --test frame_budget measure_frame_capacity -- --ignored --nocapture`. Use `BAML_PROBE_DEPTH=search` for a fresh-VM boundary search; `BAML_PROBE_LOCALS=64` adds 64 live locals per recursive call and selects the generated fixture by default; `BAML_PROBE_ENTRY` can override the entry. For the full-engine repro, put the two functions above in a minimal project's `baml_src/main.baml`, then run `target/release/baml-cli run --from <project> --function main -- main --n <n>`. The ticket's named, ordinary wildcard, and panic wildcard wrappers provide the matching positive and negative controls. Keep each failed run's diagnostic and exit status; a controlled BAML `StackOverflow` is distinct from a fatal host `stack overflow` abort.


## Baseline measurements

The saved baseline CLI and release probe execute the pinned baseline. The probe links the unchanged library artifacts from that build; its binary was copied before the fixed release rebuild. The CLI reproduction above completed before implementation began.

| Baseline observation | Result |
| --- | --- |
| `size_of::<Frame>()` / alignment | 112 / 8 bytes |
| `size_of::<BytecodeFrame>()` / `size_of::<NativeFrame>()` | 112 / 32 bytes; each stored enum slot still occupies 112 bytes |
| Input 254, VM probe and full CLI | Returned 254; peak 256 frames, capacity 256 slots, 28,672 frame backing bytes |
| Input 255, VM probe and full CLI | Controlled `baml.panics.StackOverflow {message: "stack overflow"}`; peak/capacity 256 slots; probe traceback had 256 frames; CLI exited 1 |
| Input 1000 | Same controlled overflow at 256 live frames; no process abort |
| Named overflow arm at input 1000 | Returned -1, CLI exited 0 |
| Ordinary `catch_all` wildcard at input 1000 | Panic propagated; same diagnostic, CLI exited 1 |
| `catch_all_panics` wildcard at input 1000 | Returned -3, CLI exited 0 |

These reproduce the ticket's adjacent successful/failing inputs on the newer pinned canary, rather than relying on the ticket's older `af4bf2dba2` observation. The local CLI printed 261 total output lines for the uncaught case, including warnings; the useful invariant is the 256-frame peak, not an exact presentation-line count.

## Fixed release measurements

The shared-budget release probe measured `size_of::<Frame>() = 112`, alignment 8, and `size_of::<Value>() = 8`. `BytecodeFrame` is 112 bytes and `NativeFrame` is 32 bytes; both still occupy a full 112-byte enum slot in frame storage. The rule is `frame_capacity * 112 + value_capacity * 8 <= 10,485,760`, rather than a frame-only slot maximum.

| Release VM probe | Peak live frames / allocated frame slots | Peak live values / allocated value slots | Combined backing bytes | Result |
| --- | --- | --- | --- | --- |
| Ordinary recursion, input 1000 | 1002 / 1024 | 2002 / 2048 | 131,072 | Returned 1000 |
| Ordinary recursion, input 74,896 | 74,898 / 74,898 | 149,794 / 262,144 | 10,485,728 | Returned 74,896 |
| Ordinary recursion, input 74,897 | 74,898 / 74,898 | 149,794 / 262,144 | 10,485,728 | Controlled `panics.StackOverflow` |
| Ordinary recursion, input 100,000 | 74,898 / 74,898 | 149,794 / 262,144 | 10,485,728 | Controlled `panics.StackOverflow` |
| Generic recursion, input 74,896 | 74,898 / 74,898 | 224,691 / 262,144 | 10,485,728 | Returned 74,896 |
| Recursion with 64 extra live locals, input 16,382 | 16,384 / 16,384 | 1,081,278 / 1,081,344 | 10,485,760 | Returned normally |
| Same 64-local program, input 16,383 | 16,384 / 16,384 | 1,081,278 / 1,081,344 | 10,485,760 | Controlled `panics.StackOverflow` |

For ordinary recursion the final frame backing array occupies 8,388,576 bytes and the value array occupies 2,097,152 bytes, leaving 32 budget bytes unused. For the 64-local program the arrays occupy 1,835,008 and 8,650,752 bytes respectively, consuming the full budget. Its locals are initialized from `n` and used after the recursive call, so they remain live across recursion; its return value includes that additional arithmetic rather than simply returning the input depth.

A search with fresh VMs found each last successful input, and separate runs confirmed the adjacent failing input. The exact/general call tests independently check both boundaries and reuse retained capacity after a successful run. The named panic and `catch_all_panics` controls recover, call another function, and successfully reuse the VM at depth 1000; ordinary `catch_all` propagates `StackOverflow`. Indirect and native callback recursion has positive cases beyond 256 frames and controlled over-budget failures. Unit tests cover shared-capacity accounting, preservation of live contents/charges on injected allocation refusal, budget rejection before allocation, and delivery of entry/value/host-result allocation failures through execution.

These are measured depths for these programs, this layout, and fresh VMs, not portable recursion constants. Geometric growth charges spare capacity, and retained capacity means the growth history of a reused VM can affect how much space remains for the other array. Generic metadata, native continuation pointees, referenced heap objects, and exception traces still allocate outside the budget. Release optimization, feature flags, target pointer width, and future field changes can change layout and capacity; regression tests use the executing build and observe both arrays. An uncaught-error VM retains its failing stack for diagnostics; the reuse controls exercise completed runs and caught overflow, without changing that existing behavior. The interpreter handles BAML recursion iteratively and this limit does not bound or protect the host/native stack.
