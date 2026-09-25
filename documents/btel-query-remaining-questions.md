# Remaining query parity, by question

This is a question-oriented audit of the old tracer's nine relations and
`hot_call_paths`, using the [complete old inventory](btel-logical-schema.md)
and the current BTEL reader in PR #4986. It distinguishes missing evidence
from changed query spelling. It is not a promise to reproduce every
DataFusion SQL function or every old storage-internal field.

Review baseline: query commit `0735e3909`, after population outcome counts
and structured value equality, plus the subsequent
[recording-completion implementation](btel-query-recording-completion.md).
Normal shutdown now records terminal extent and final clocks. File gaps,
unsettled clocks and older recordings remain explicit. No other producer
features are implemented by this audit.

## Questions with missing or narrower answers

| Question | What works now; what remains missing | Work needed |
| --- | --- | --- |
| **Which expression called a function or spawned a task?** | Answered from format minor 2: `call_paths`, `threads` and `calls` resolve call and spawn expressions through recorded source maps ([record](btel-query-source-errors.md)). A direct recursive re-entry has no site of its own, and older recordings have definition spans only. | Recording each recursive invocation's own PC would change normal call-path production. |
| **What image/audio value was passed or returned?** | Ordinary captured bytes work. Media stored as host objects becomes an opaque value. The old path captured media kind, MIME and URL/base64/file descriptors; URL/file capture did not promise to fetch external bytes. | Capture/format support, coordinated with recursive CAS; then reader/UI support. |
| **Where did an error originate, was it a fresh throw or rethrow, and which calls failed from that occurrence?** | Mostly answered from format minor 2: `error_raises`, `error_occurrences`, `error_frames` and `error_call_links` record every raise, including throws caught in place, its site and stack, proven origins across rethrow and await, and the retained calls it failed. Nested handlers holding the same object make a rethrow `ambiguous`. An `UnknownError` conversion never establishes a proven link: when it carries an earlier throw's context it is `unresolved` (`source_not_recorded`) and keeps that trace only as an inherited trace; a conversion of a value never thrown before is a fresh raise. Native failures are placed at their BAML call site. Raise values are read from failed calls, not captured separately. | The "during handling of" cause relation, which binding a rethrow read, and which value an `UnknownError` conversion received need further evidence. |
| **How many calls started, remain unfinished, or were selected for capture?** | Population success/error/cancellation **completions** are counted, including timing-only calls. Retained calls provide their own starts, but not all invocation starts. Selected-before-loss counts and starts in old overflow buckets are absent. | New population evidence/policy accounting; cannot infer it from retained rows or completed totals. |
| **How many times did a function suspend on await?** | Await duration is available. Await count and the current population of suspended invocations are not. | Producer instrumentation. |
| **Which native/sysop functions consumed the time?** | Bytecode timing and waits work. The current telemetry policy hides native/sysop invocation coverage; metadata cannot create those missing calls. | Policy/coverage changes and measurements of the affected execution paths. |
| **Why was this call/value not captured, and how much telemetry was lost?** | `issues` reports reader-visible gaps, corrupt/missing files/blobs, unresolved references and invalid clocks. Captured truncation has explicit states. The old selection reasons (`root`, `llm`, `manual`), requested input/output/error roles, absent-capture reasons and full producer loss/overflow counters are unavailable. Zero issues is not proof of lossless capture. | Some background health counters can be published cheaply; exact capture/selection accounting needs additional facts. Pool allocation misses and cloud discarded-group counts must not be relabeled as lost invocations. |
| **Is an unfinished execution still running, abandoned, or panicked?** | A recorded root completion gives its outcome. An absent completion is incomplete evidence. A recording end marker can establish terminal recording extent, but is not a process liveness lease or a panic event. | A separate liveness/lifecycle contract. The old local `processes.alive` used a writer lock; it did not infer liveness from missing records. |
| **Which process/engine/revision produced these runs, and what were the threads named?** | Recording IDs scope identities; source fingerprints and recorded static function metadata supply some provenance. OS PID, durable process/engine identity, producer version/platform, revision/source labels and user thread names are missing. A recording ID is not the old process ID. | Mostly cold creation-time metadata, with identity scopes explicitly defined. |
| **Which functions existed but never ran, or were created dynamically?** | Referenced static functions have recorded names, definitions and parameter slots. The dictionary is not a complete compiled-function inventory, and runtime-created definitions can remain unavailable. General old `kind_detail` has only a narrower sysop-name counterpart. | Cold full-table publication and/or dynamic registration. Reader inference from the current source would not establish historical truth. |
| **What was the entry function if the root has ambiguous evidence?** | `executions` resolves it when the root thread has exactly one top-level call path. Otherwise it stays unknown. | A durable explicit entry reference if this ambiguity matters; no guessed first function. |
| **Which runs/calls ended through the old explicit-exit outcome?** | Current completion outcomes distinguish success, error and cancellation. There is no separately recorded `exited` outcome. | First decide whether the current engine still has a distinct exit behavior to expose; do not invent an SQL mapping. |
| **Show every CAS object, its codec, size and path, including unreferenced objects.** | Call rows expose recorded capture references and the reader resolves values lazily. The old internal `value_index` filesystem inventory is not exposed. `recording_files` inventories recording segments, not CAS blobs. | Reader/tooling only: an explicit CAS inventory operation, without making ordinary metadata queries scan all blobs. |
| **Which files contain this execution, and how many records are in each?** | `recording_files` exposes segment sequence, size, fingerprint and acceptance/rejection. It does not expose per-file event counts or the old root's first/last/expected-file manifest. Recording-wide gaps are visible; that is a different answer. | Reader/index provenance can expose observed execution/file associations and event counts. Expected-but-never-recorded execution membership cannot be recovered from observed files alone. |

Recording completion/final clocks are covered by the
[implementation and validation record](btel-query-recording-completion.md).
Older files without terminal/finality facts cannot be repaired merely by
upgrading the reader. A completed recording does not establish process
liveness or promise every optional capture was delivered.

## Value and SQL compatibility

- **Structured equality works:** supported captured lists/maps/classes/enums
  can be compared with `=`/`!=`, including
  `args['tags'] = baml_value_json('["urgent", "billing"]')`.
  Captured cycles, opaque values, truncated evidence and traversal limits
  still prevent some comparisons. These limitations are explicit; not every
  one is an old regression, since the old capture path also omitted values.
  See the [comparison contract](btel-query-value-comparisons.md).
- **Comparing against an old saved CID is not portable.** The old
  `baml_value_cid('bamlv_1_<hex64>')` constructor refers to the old format.
  Current CAS IDs have different identity/encoding semantics. Comparing
  `args_cas_id` is possible, but snapshot identity is not a substitute for
  equality of an arbitrary nested value.
- **Distinct values/grouping/set deduplication need a separate contract.**
  Semantic `=` does not make `DISTINCT`, `GROUP BY`, `COUNT(DISTINCT ...)`
  or `UNION` use the same logical-value equality. Some forms operate on
  handles or rendered scalars. Metadata grouping works. Do not treat
  `COUNT(DISTINCT output)` as a verified count of distinct logical outputs.
  The old implementation also mixed rendered output and internal handles;
  full canonical grouping is not established old behavior to copy blindly.
- **SQL spelling and types changed.** Old `_v1` aliases, `retained_calls`,
  `cct_population`, `errors`, `health`, `processes`, `store_files` and
  `value_index` are not a drop-in compatibility surface. Use the current
  catalog for replacements. SQLite scalar/window functions and the SQL
  lowering support a subset of DataFusion syntax; named windows, some
  clauses and engine-specific functions need query rewrites or extensions.
  Arrow lists/unsigned integers/timestamps are also not identical SQLite
  types. Brackets on captured BAML values are supported; old resident-list
  columns are a different facility. Raw ticks beyond the signed 64-bit range
  become NULL in the index with an issue, although BTEL retains them; the
  old unsigned numeric query behavior is not fully preserved.
- **Old token correlation was intentionally removed upstream.**
  `threads.runtime_id` and the override history in `calls.runtime_ids` cannot
  be reconstructed from BTEL IDs. Reintroducing them is a product/runtime
  decision, not a reader port.

## Changed meanings that are not missing tables

- `calls` counts **retained invocations**. Population totals belong in
  `call_path_stats`/`function_stats`; the new totals count completions, not
  starts. A valid late completion can form a retained row even when the
  old provider would have omitted an end-only call.
- Parent IDs describe recorded telemetry ancestry, which may skip
  timing-only invocations. Recursion-aware aggregate timing is supported;
  aggregate counters do not reconstruct every recursive invocation/frame.
- Recording-scoped IDs and clock epochs replace old process/program IDs
  and process-zero nanoseconds. Epoch conversions and uncertainty are
  explicit. Old IDs or raw timestamps cannot simply be joined to new ones.
- `recording_files` replaces old meta/data-plane inventory with BTEL
  segment sequences and decode evidence. It does not reproduce the old
  per-execution expected-file manifest. A file fingerprint calculated by
  the reader is not an independent producer checksum.
- Current captures expose reference/not-recorded/pending/conflicted states
  and lazy read failures. A whole-execution claim that all policy-requested
  values arrived requires policy/loss evidence, even after the recording
  has ended.

## Do not count these as old-schema regressions

- Population latency percentiles/min/max/histograms were not fields in
  the old `call_path_stats`. Retained invocation durations are available
  for queries over that subset.
- The old catalog did not ship an `llm_calls` view, and its function `kind`
  meant bytecode/native/sysop, not LLM. Historical LLM classification would
  be useful new cold metadata; the playground's current-project knowledge
  is not a historical recorded classification.
- Literal array syntax such as `args['tags'] = ['a', 'b']` remains a deferred
  convenience; the JSON constructor already answers that comparison.
- Boundary Studio/cloud query delivery is a required product path, but
  this audit compares the local query capabilities. It is not being
  declared complete by local SQLite work.

Call/spawn source navigation and error origins are implemented, following
the [source/error audit](btel-query-source-errors-audit.md). The
[implementation record](btel-query-source-errors.md) lists what is proven,
what stays ambiguous or unresolved, the layout and runtime changes, and the
measurements. Media remains separate.

The [origin lifetime review](btel-query-error-origin-review.md) fixes stale
handler attribution. Origins remain unresolved when an error reaches a
catch without a recorded landing (including some defer forwarding), or
when the handler can overwrite its error slot. Inherited diagnostic traces
are weaker evidence and do not prove an origin.
