# Response to REVIEW.md — fixes and contract tests

Cross-references use REVIEW.md's section numbers (§) and test numbers (T#).
All T-numbers below refer to the review's numbering, regardless of the order
items were fixed in.

## Status summary

| Review item | Status | Fix | Tests |
|---|---|---|---|
| §2.1 caught-exception desync (CRITICAL) | **Fixed** | VM `Unwound` notification + engine depth-truncation + Complete-arm defense | T1 (×3 variants), T2, T3 |
| §2.2 `$id` override lost after nested call | **Fixed** | `EngineSpan.id_override`, written back at the `SetId` drain | T4, T5 (lifted from repro), T6, T7 |
| §2.3 watch filters abort the program | **Fixed** | `interrupt()` loops, swallowing notify states | T8, T9 (VM-level — see note) |
| §2.4 orphan `EndFunction` / `call_callable` | **Fixed** | never-skip `CallFunction` + unknown-function sentinel row + real identity for `call_callable` | T10, T12a/b |
| §2.5 wall-clock timestamps | **Fixed** | `bex_events::clock` (process `OnceLock<Instant>` anchor); header anchor = process anchor | T13, T14, T15 |
| §2.6 root cancel reads as Error | **Fixed** | root epilogue branches on `is_cancelled_engine_error`; `exit(0)` → Ok/Completed, `exit(n≠0)` → Error/Error | T16, T17, T18a/b, T19, T20 |
| §2.7 child engine-error drain | **Fixed** | both abnormal spawn-task arms drain before `EndThread` | (T21 — see note) |
| §2.8 dropped-future truncation | **Decided: documented policy** | hosts must cancel + await; pinned in test form | T22 |
| §3.1 `$id += x` silent no-op | **Fixed** | TIR error `RuntimeIdCompoundAssignment`; MIR `lower_lvalue` fallback now fails loudly | T23, T24 |
| §3.2 `$id = <non-string>` | **Fixed** | TIR types the RHS against `string` | T24 |
| §3.3 `throws never` lie on `baml.id.set` | **Fixed** | `throws root.errors.InvalidArgument`; `current()` downgraded to `//baml:vm` | T25a/b |
| §3.4 `$id` not reserved | **Fixed** | reserved in let/pattern bindings and parameters (AST diagnostic) | T24 (snapshot) |
| §3.5 misleading `$id.foo` diagnostic | **Fixed** | targeted `RuntimeIdMemberAccess` message | T26 (snapshot) |
| §3.6 call-site `foo($id = …)` | **Interim: targeted diagnostic** | `RuntimeIdCallSiteArgument` ("assign `$id` inside the function body") — the call-site *form* remains a product decision with Antonio | T24 (snapshot) |
| §3.7 dead AST `$id` rewrite | **Fixed** | deleted; TIR/MIR special cases cross-reference each other in comments | T23 pins the lowering |
| §4.1 per-call yield vs perf contract | **Partially addressed** — see "Deliberately not done" | per-call name clones and table scans deleted (§4.3); the yield gating and VM-owns-call_id seam remain the Antonio coordination item | T28 protects `$id` across the future gating refactor |
| §4.2 eager CallRef minting | **Not done** (see below) | — | T30 deferred with it |
| §4.3 per-call name scan + misattribution | **Fixed** | notifications carry the resolved `Object::Function` `HeapPtr`; engine resolves via a prebuilt addr→`FunctionId` map; display-name fallback deleted; `FunctionExit` carries only `frame_depth` | T29 (covered by T1's exact-fqn call sequence + T12b) |
| §5.1 multi-engine JSONL ambiguity | **Fixed** | `EventSink::send_disk_event` carries `EngineId`; per-line `engine_id` on interim JSONL; one `write_all` per line | T31, T32 |
| §5.2 droppable header | **Fixed** | headers retry hard (bounded ~2s) before a loud drop — never silently lost, never an unbounded hang | T34 |
| §5.3 lossy event channel | **Documented** | lossiness is now stated on `NativeEventSink` (drop counter retained) | — |
| §6 status naming (`Error` vs `Errored`) | **Unchanged — flag to Antonio** | wire strings now pinned by T33; rename is a coordinated change if wanted | T33 |
| §6 `RuntimeEventIdentity` redundancy | **Unchanged** | identity constructed at a single site (`runtime_event_identity`), invariant holds by construction | — |
| §6 `current()` returns `""` w/o identity | **Documented sentinel** | doc comment on `baml.id.current` | T25c (VM-level) |
| §6 spawn-test determinism | **Addressed in new tests** | new spawn assertions partition by thread id rather than index | — |
| §7 entropy | **Mostly done** | see below | — |
| §9 Tier A | **Done** | `assert_balanced` / `assert_threads_closed` helpers, retrofitted into the four pre-existing loose tests and used by every new disk-event test | T11 |
| §9 two-engine scoping row | **Done** | distinct engine ids ⇒ distinct encoded CallRefs | `two_engines_mint_distinct_call_refs` |
| §9 ids.rs decode edges | **Done** | ThreadRef malformed/version, truncated payloads, cross-type prefixes | unit tests in `ids.rs` |

## Design choices worth a reviewer's eye

- **§2.1 fix shape: both options.** The VM yields
  `RuntimeCallNotification::Unwound { frames_remaining }` from every in-loop
  unwind site after `try_unwind_exception` reports popped *notified* frames
  (traced or runtime-call), so the engine closes unwound spans **timely and
  in-order** (before the catch handler runs, after any pre-throw `SetId`).
  Independently, `EngineSpan.frame_depth` + depth checks on every
  enter/exit make the engine **self-healing** against any future silent-pop
  path, and the `Complete` arm closes stray non-root spans loudly instead of
  blind-popping. External exception injection (`try_handle_external_exception`,
  which cannot yield) returns the popped count; `inject_sysop_throw` resyncs
  via `vm.frame_count()`.
- **Unwound spans get `FunctionEndStatus::Error`** with error
  `"unwound by exception"`. The frame terminated because an exception passed
  through it; `Ok` would be a lie and a new status variant is a wire-contract
  change Antonio would have to absorb.
- **§2.3 scoping decision:** calls inside watch-filter functions mint **no**
  identity and emit **no** spans (the `interrupt()` mini-runner swallows
  their notifications). The engine's stack stays balanced because it never
  sees the enters. Pinned by T9.
- **§2.4 sentinel:** `baml.<unknown-function>` row at pool-index+1 (next to
  the spawn-closure row). `emit_disk_call_function` maps unresolved callees
  to it and never skips. `call_callable` now resolves the real callee
  (`func_ptr` → pool row); its legacy span label stays `"<callable>"`
  (host-facing name unchanged; identity is the disk-stream contract).
- **§2.5:** `timestamp_ns` = nanos since the **process** clock anchor;
  `started_at_epoch_ns` in every header is the matching wall anchor captured
  in the same `OnceLock` init, so `wall = started_at + ts` composes across
  all engines in a process.
- **§2.6 `baml.sys.exit`:** exit(0) → `EndFunction(Ok)` + `EndThread(Completed)`;
  non-zero → `Error`/`Error`. Pinned by T19 as the documented decision.
- **§4.3:** `SpanNotification::FunctionEnter` also carries the function
  pointer (kills the traced path's name scan); `FunctionExit` on both paths
  carries `frame_depth` (the §2.1 watermark needs it — REVIEW §7.3's "decide
  which" is decided: it has a job now).
- **§5.1 option chosen:** per-line `engine_id` on the interim JSONL (the
  review's first option), via an `EngineId` parameter on
  `EventSink::send_disk_event`. Chosen over process-global thread ids because
  it keeps per-engine thread numbering (which existing tests and the header
  scoping model assume) and is trivially deletable when per-engine transport
  lands.

## Deliberately not done (and why)

- **§4.1 gating the per-call yield / §4.2 lazy CallRef minting.** These are
  the explicit Antonio-coordination items (the VM-owns-call_id seam decides
  both). What *was* done now: the per-call costs that didn't need the seam —
  both name clones, the O(F) table scans, and the per-iteration identity
  string re-encode on call transitions (override or previous-string reuse) —
  are gone. The remaining per-call cost is the two dispatch-loop exits and
  the `SpanId`/UUID mint, which is what his ring replaces. T28
  (`sinkless_engine_still_mints_correct_ids`) is in place as the guard for
  the gating refactor.
- **T27 call-heavy bench.** Not added in this pass; the repo's bench harness
  is divan under `crates/baml_tests` (generated from
  `tools/speedtest/workloads/`). Adding the call-heavy workload there is a
  small follow-up; flagging per §4.4 that until Antonio's master switch
  exists, nothing can turn the per-call yield off.
- **§7.4 serde-derived serializers.** T33 now pins every variant's exact key
  set, type string, and status strings, which makes this migration safe to do
  mechanically in a follow-up without behavior risk. Left hand-rolled here to
  keep this diff reviewable.
- **T21 (child engine-error drain test).** The fix is in (both abnormal
  spawn-task arms drain open spans before `EndThread`), but no organic
  trigger for `run_thread_event_loop` returning `Err` on a child exists with
  the native SysOps — the realistic child failures (throw, cancel, internal
  error) all settle the future and are covered by T17/T18b. Forcing the path
  needs a fault-injecting SysOps mock; noted as follow-up.
- **§3.6 call-site override form.** Both design docs show
  `foo($id = baml.id.new())`; the implementation is callee-side assignment.
  This is a semantic product decision (caller-names-callee vs
  callee-names-itself) that changes `SetId` adjacency — needs the sync with
  Antonio, not a unilateral fix. The diagnostic now says exactly what to do
  instead, and T24 pins it.
- **§6 `Errored` rename.** Wire strings are frozen by T33; renaming is a
  one-line change on both sides once Antonio confirms which way the .proto
  goes.

## Known pre-existing issues surfaced (not introduced, not fixed here)

- The `$watch.options(...)` surface fails member resolution for **all** types
  ("type `T` has no member `$watch`", E0007), so `WatchFilter::Function` is
  currently unreachable from BAML source. T8/T9 therefore install the filter
  directly on the VM's watch state (the exact state that surface produces).
  The §2.3 fix is what makes the surface *work* once the resolution bug is
  fixed.
- An exception escaping a watch-filter (`interrupt()`) frame into the outer
  program remains pathological (pre-existing); the engine now self-heals its
  span stack via exit-depth checks if that occurs.

## Cross-team notes for Antonio (per §8)

- `timestamp_ns` is now monotonic-since-process-start; rebase via the
  header's `started_at_epoch_ns` works as specified (T14 pins the formula).
- Every entered frame now emits exactly one `EndFunction`, **including frames
  popped during exception unwinding** (T1/T3) and `call_callable` roots (T10).
- Statuses: root cancellations now read `Cancelled`/`Cancelled` (T16);
  `FunctionEndStatus::Cancelled` and the `"error"`/`"errored"` naming remain
  open items for the .proto sync; the JSONL status strings are pinned (T33).
- Interim JSONL lines now carry `engine_id` (delete with the ring); the
  artifact may still drop non-header events under backpressure (documented).
- The unknown-function sentinel row (`baml.<unknown-function>`, id =
  pool+1) is new in every header; `expect("CallFunction precedes
  EndFunction")` is now safe (T10/T11/T31).

## Second-pass adversarial review of these fixes

After the fixes above were green, an adversarial multi-agent review was run
over the new diff itself (5 dimensions, every finding independently
verified, 2 findings refuted as pre-existing). Confirmed findings, all
addressed:

| Finding | Resolution |
|---|---|
| `interrupt()` swallowed `Unwound` when a filter exception escaped the interrupt boundary into announced program frames (silent span desync; the program's own completion could be consumed as the filter verdict) | Swallowing is now gated on `interrupt_frame` still being alive; an escape propagates the state and fails loudly (`ExpectedCompletion`), and the engine's error drain closes spans. Pinned by `watch_filter_exception_escaping_interrupt_fails_loudly`. |
| `$id = e` bypassed `baml.id.set`'s new throws clause (`throws never` functions could throw catchable `InvalidArgument`; caller catch arms flagged unreachable) | Both throws-fact walkers now register the implicit `id.set` summary for `$id` assignment targets; `throws never { $id = s; }` is now a precise compile error ("may also throw `baml.errors.InvalidArgument`"), pinned in the `runtime_id_misuse` snapshot. |
| Child-thread `Complete` blind-popped the top span (no defense-in-depth parity with the root arm) | Same stale-span drain added before `emit_completed_top_function_end`. |
| Blocking header send could hang `BexEngine::new` (and LSP document sync) forever on a stalled publisher (undrained stderr pipe, FIFO/NFS trace target) | Header send is now a bounded retry (~2s) that drops with a loud error instead of wedging the engine constructor. |
| T34 was tautological (verified: passed with the header-drop regression reintroduced) — channel pressure had drained before the header send | Hammer threads now run until `BexEngine::new` returns (stop flag), with compilation hoisted out of the pressure window. |
| Reserved-`$id` check missed destructure field shorthand `{ $id }`, and `function $id()` declarations | Both sites now reject with the reserved-name diagnostic; pinned in the snapshot. |
| `$id?.member` fell through to generic machinery whose suggested rewrite (`$id.member`) is itself rejected | `infer_optional_member_access_expr` now emits the targeted `RuntimeIdMemberAccess`; pinned in the snapshot. |
| T22 couldn't detect the truncation policy breaking in the detached-execution direction (10s sleep never landed in the observation window) | Program sleep shortened to 400ms and the assertion window extended past natural completion (800ms). |
| T14 / clock test could flake on laptop suspend or NTP step (Instant doesn't advance across suspend; window came from raw `SystemTime`) | T14's window is now derived from the same anchor+monotonic composition consumers use — suspend-immune, still catches the absolute-epoch bug class. |
| `interrupt()` swallowing `EarlyYield` delays the engine-wide GC stop-the-world for the filter body's duration | Documented as a known limitation in the `interrupt()` comment (filter bodies are expected to be tiny; a real fix needs parking support in the mini-runner). |
