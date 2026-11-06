# Runtime v2: contract ledger

Every place where v2 could change behaviour a BAML user, SDK, or team member can observe, and how v2 keeps it. Status: **kept** (no observable change, verified by tests), **changed** (needs team sign-off before merge), **internal** (not observable).

Sources of contracts: BEPs (in the BEP app, not in this repo), `TYPE_SYSTEM.md`, `ARCHITECTURE.md`, `ENV_VARS.md`, and the test suites (`baml_tests`, `sdk_tests`, `rust_differential`). Merge gate for the v2 branch: every existing suite passes with the v2 backend, plus the differential suite with telemetry comparison.

## Needs team sign-off

| # | Contract | Today | v2 | Why |
|---|---|---|---|---|
| C1 | Order in which a spawned child starts | Usually after the parent's next await, but GC parking can let it run earlier (`bex_engine/src/lib.rs:6603` labels this a bug) | Always when the parent next yields; ready tasks FIFO | One loop needs a written order; this picks the one today's code says it intended |
| C2 | Object identity across runtime compilation (`Package.compile`, `Session.eval`) | Runtime-compiled code shares the heap: objects passed in and mutated are seen by the caller | Interpreter island on its own heap; values copied across, so mutations are not shared | Sharing would require moving the interpreter onto the v2 object model (decision 5) |
| C3 | When `cleanup` finalizers run | When a precise collector proves the object dead | Conservative stack scanning can keep a dead object alive a little longer, so cleanup can run later | Non-moving collector with conservative roots (decision 3) |
| C5 | GC maintenance telemetry records (collection counts, collector heap context) | Describe today's copying collector; today's compiled backend already has an accepted divergence from bytecode here | Describe the v2 collector truthfully, same record formats; exempt from the differential comparison like today's compiled backend | A different collector does different work; the records must stay truthful rather than identical |
| C4 | Environment variables `BAML_COMPILED_BODIES`, `BAML_COMPILED_FRAMES` | Select bytecode vs compiled bodies, VM frames per call | Meaningless for v2 packs (no VM frames, no per-function fallback) | No VM underneath compiled code |

## Kept (verified by the differential suite and existing tests)

| Contract | How |
|---|---|
| `int` range and overflow errors (i63) | Raw `i64` with the interpreter's range checks and messages |
| Error classes, messages, causes, stack traces | Same constructors; line side tables; "inlined at" metadata so inlined frames still appear |
| `spawn`, `await`, `await_any`, `future.all`, cancel semantics, shielding, deadlines | Ported as specified in INTERFACES.md section 5 |
| Telemetry levels, policies, hooks, `$trace`, records, published aggregates and spans | Same records; producer emitted by the compiler; telemetry compared by the differential harness |
| Default telemetry level (medium) | Unchanged |
| Reflection APIs, `call_any` argument coercion and omission rules, tool argument coercion | Generated tables and adapters with the same rules |
| Host invocation ABI (BEP-81): validate before execution, no side effects from abandoned preparation, callback ownership | Host entry ported with the same rules (INTERFACES.md 5.7) |
| JSON argument decoding via `baml.json.decode<T>` and user `from_json` | Same path, typed decoders per type |
| Map iteration with mutation checks | Epoch checks kept |
| Playground, REPL, wasm | Unchanged interpreter |

## Internal

- Object layout, tagged slot size (fixed 16 bytes), descriptors, heap and collector design.
- Producer-to-processor telemetry format (pre-combined aggregate deltas); published output unchanged.
- Builtin glue, continuation machinery, lock-free twins (deleted).
- Pack contents: v2 packs don't link the VM unless they use runtime compilation (binary size changes; not a behavioural contract).
