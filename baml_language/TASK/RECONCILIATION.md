# BEX Identity × Profiling Ring — Reconciliation Handoff

**Branch:** `paulo/bex-prof-ring-reconcile` → targets `canary`.
**What it contains:** everything from both BEX tracing workstreams, reconciled:
Paulo's event identity + metadata + review-fix suite (`paulo/bex-event-identity-metadata`,
PR #3730 + commit `3c1929dfd`) merged with Antonio's profiling ring stack
(`antonio/bex-prof-ring`, PR #3733, tip `27ec7c0b4`), plus a post-merge
adversarial review pass. Landing this branch lands both workstreams.

**Commit map:**

| Commit | What |
|---|---|
| `d318a6ceb` | Paulo M0/M1: identity, `$id`, metadata table, interim JSONL (PR #3730 — already inside Antonio's branch via his merge `a7f247f79`) |
| `3c1929dfd` | Paulo's review fixes + contract test suite (T1–T34, see `TASK/REVIEW.md` / `TASK/REVIEW-RESPONSE.md`) |
| `bb67b34a7..27ec7c0b4` | Antonio PR1–PR5: ring, producer, consumer, `.bamlprof`, teardown, CI |
| `128bb22bd` | **The reconciliation merge** (this document's subject; long commit message has the per-seam decisions) |
| `80a2c5da6` | Fixes from the post-merge adversarial review (one real `$id` regression + latent defects + doc truthfulness) |

---

## 1. What Paulo's branch did (M0/M1 + review hardening)

Design doc: `TASK/TICKET.md`. Post-impl notes: `TASK/POST-IMPLEMENTATION-NOTES.md`.

- **`ids.rs` (M0)** — the identity quad `ProcessEuid / EngineId / BexThreadId / BexCallId`
  plus `FunctionId`, `CallRef`/`ThreadRef`/`RuntimeId` with reversible string
  encodings (`decode(encode(x)) == x`), versioned base64url, hardened decode
  edges (malformed/truncated/cross-type all fail cleanly, unit-tested).
- **`$id` (M1)** — language primitive: read returns the current call's
  identity; `baml.id.new()` mints an override UUID; `baml.id.set(...)` /
  `$id = ...` installs it. Compiler-side misuse diagnostics
  (`RuntimeIdCompoundAssignment`, `RuntimeIdMemberAccess`,
  `RuntimeIdCallSiteArgument`, reserved-name checks, throws-fact
  registration for the implicit `id.set`), all snapshot-pinned.
- **Function metadata table** — per-program `FunctionMetadata` rows (fqn,
  display name, source span, kind, origin, owner type, lambda metadata,
  BEP-053 join fields), synthetic spawn-closure and unknown-function rows,
  carried in the artifact header.
- **Interim JSONL transport** — engine-emitted `DiskEventV1` records through
  `EventSink` (a stopgap until the ring landed; **deleted in this merge**).
- **Review-fix suite (`3c1929dfd`)** — 30+ fixes pinned by tests T1–T34:
  caught-exception span desync (`Unwound` notification + engine depth
  self-healing), `$id` override persistence across nested calls, watch-filter
  crash fix + scoping, never-skip CallFunction + sentinel attribution,
  monotonic process-anchored timestamps, cancel/exit status mapping,
  multi-engine scoping, header delivery robustness, wire-shape pins.

## 2. What Antonio's branch did (M2–M4 ring stack)

Design docs (private `BoundaryML/thoughts` repo): `antonio/bex-event-stream-design-v2.md`
(the spec) and `antonio/bex-event-stream-impl-plan.md` (the locked decision
ledger D1–D7 + codebase map). Implemented in full as PR1–PR5:

- **Segmented SPSC ring** per `(engine, os-thread)`: lock-free push (memcpy +
  one Release store), lossless-by-growth, free-list recycling, append-only
  registry, orphan→pool→claim lifecycle. **loom + miri gated**
  (`bex_events/src/prof/concurrency_tests.rs`, cfg `baml_loom`).
- **Producer integration**: VM mints `call_id` per call unconditionally
  (frames carry it), writes `CallFunction`/`EndFunction` raw records at
  call/return/unwind; engine emits `StartThread`/`EndThread` (+ sysop pairs)
  on cold paths; D5a ring-pointer refresh once per exec resume.
- **Background consumer**: heap-permit-free `std::thread`, drains rings,
  transcodes raw POD → protobuf `DiskEventV1`, writes per-engine
  `.bamlprof` (`prof/proto/bamlprof.proto`), heartbeats, durable flush ack,
  torn-tail-tolerant reader, engine teardown closes the file.
- **His merge `a7f247f79`** pulled PR #3730 into "one id universe": **he
  rewrote `$id` to be VM-sourced and lazy** — which is exactly the §4.1
  (VM-owns-call_id, no per-call engine yield) and §4.2 (lazy CallRef
  minting) items Paulo's review response had deferred to him.
- Measured: clock 8.5 ns/read; drain ~7.5M ev/s/core; call-pair ~63 ns on
  the pure-call microbench, ~0–4.4% realistic. Profiling ships
  **default-off** (`BAML_PROFILE=1` opt-in) per PR5's rollout note.

## 3. Where the branches diverged

Antonio's merge predated Paulo's review fixes, so `3c1929dfd` (written
against the engine-owned model) collided with his 12 ring commits on the
same seams — 54 conflict hunks across 7 files, plus semantic divergences no
textual conflict surfaced:

| Seam | Paulo (`3c1929fd`) | Antonio (`27ec7c0b4`) |
|---|---|---|
| Identity owner | Engine pushes `CurrentBexIdentity` into VM each step; engine mints call ids | **VM owns it**: `call_id_counter`, `current_call_id`, `bex_ref_seed`; lazy CallRef encode on read |
| `$id` override | Persisted on `EngineSpan.id_override`, written back at the SetId drain | VM `current_id_override`, keyed by call id |
| Per-call lifecycle | `RuntimeCallNotification` yield per call → engine emits | Ring records pushed in-VM, no yield |
| Disk transport | Interim JSONL via `EventSink::send_disk_event` (engine_id per line, header retry, T31–T34) | Ring → per-engine `.bamlprof`; JSONL deleted |
| Unwind contract | VM yields `Unwound{frames_remaining}`; engine closes spans by depth | VM emits `EndFunction{Error}` per unwound frame into the ring |
| Function ids | Pool-index scheme; engine ptr→id map; unknown sentinel attribution | 1-based sequential stamped on `Function` objects; ids snapshot-pinned |
| Status mapping | Cancel→`Cancelled`, exit(0)→Ok/Completed, exit(n)→Error (T16–T20) | `ThreadEndStatus{Completed,Cancelled,Errored}`; all exits → Completed; no function-level Cancelled |
| Naming | `BexCallId` (avoids `sys_types::CallId` clash) | kept `CallId`, aliased in engine |

## 4. What the reconciliation decided (merge `128bb22bd`)

**Principle: Antonio's architecture wins the seams he owns (it implements
the agreed deferral); Paulo's behavioral guarantees are ported onto it as
code where still applicable and as artifact-level tests everywhere.**

1. **VM-owned identity stands.** `CurrentBexIdentity`, `pending_disk_events`,
   the SetId drain, `EngineSpan.id_override` — deleted. `$id` reads VM state
   lazily; `bex_engine` re-exports our `BexCallId` naming and all
   Antonio-side `ids::CallId` references were renamed.
2. **JSONL deleted, pins ported.** New artifact-level tests in
   `bex_engine/tests/prof_gate.rs`: `caught_exception_keeps_ring_balance`,
   `call_callable_has_real_identity_and_balance`,
   `root_cancellation_ends_thread_cancelled`,
   `spawned_child_cancellation_ends_child_cancelled`,
   `spawned_child_error_ends_child_errored`, `sys_exit_status_mapping`,
   `sentinel_rows_present_in_header`,
   `same_display_name_functions_are_not_misattributed`.
   `bex_engine/tests/jsonl_artifact.rs` deleted (T31 coverage is
   prof_gate's header/balance checks).
3. **`Unwound` ported onto `SpanNotification`.** `RuntimeCallNotification`
   died with the per-call yield, but the traced (`@trace`) span stream still
   needs the §2.1 fix: the unwinder counts popped *traced* frames (plus one
   if it crosses the watch-filter interrupt boundary — that forced yield is
   what keeps an escaping filter exception loud instead of letting the
   program's completion be consumed as the filter verdict) and the engine
   closes spans by depth. `bex_vm/tests/call_notifications.rs` pins this on
   the SpanNotification stream.
4. **Function ids: sequential scheme kept, made coherent.** The naive merge
   had the engine reading sequential ids as pool indices (would have
   misattributed every function in the ptr→id map) and the unknown sentinel
   at a pool-derived id. Fixed: the ptr→id map is built from the same
   stamping walk; sentinel rows sit at max+1 (spawn-closure) and max+2
   (unknown); per-call resolution stays pointer-based (no name scans on any
   per-call path).
5. **Status mapping ported where the ring supports it.**
   `run_thread_event_loop`'s EndThread mapping now distinguishes
   `baml.sys.exit(0)` (Completed) from `exit(n≠0)` (Errored) per T19's
   pinned policy; root/child cancellation reads `Cancelled`. The
   *function-level* `Cancelled` status does not exist in the proto — see
   pending items.
6. **Spawn-arm drains ported (§2.7):** both abnormal spawn-task arms close
   open spans before the wrapper's EndThread.
7. **Tests as the contract:** `tracing.rs` keeps every `$id`/RuntimeEvent
   assertion (22 tests), dropping only dead JSONL assertions, each with a
   pointer comment to its prof_gate replacement.

## 5. Post-merge adversarial review (commit `80a2c5da6`)

A 30-agent verification pass (5 dimensions — lost fixes, merge junctions,
`$id` semantics, ring balance, doc consistency — every finding independently
re-verified) confirmed and fixed:

- **`$id` override nesting regression (the one real bug).** The merged
  single-slot `current_id_override` meant a callee calling `baml.id.set`
  destroyed its caller's override (empirically reproduced: caller's `$id`
  reverted to the default CallRef after the callee returned). Replaced with
  a per-call override **stack** on the VM (`id_overrides`), popped with the
  exiting frame in `prof_exit_call` (`>=` guard self-heals). Pinned by
  `id_override_survives_callee_override`.
- **Doubled error-drain emit** in the escaped-throw arm of
  `run_thread_event_loop_inner` (introduced by the ring branch's
  `ChildSettleKind` restructuring; currently a no-op only because the first
  drain empties the stack). Single emission restored.
- **Watch filter yielding a sys-op stranded the armed sysop CallFunction**
  (its only close site — the engine's SysOp arm — is unreachable from
  `interrupt()`). The mini-runner now closes the pending pair with
  `EndFunction{Error}` before failing loudly.
- ~10 comment/doc corrections so the tree tells the truth about the merged
  architecture (sentinel attribution, producer-less JSONL APIs, dangling
  cross-references, `SetId` vs `SetFunctionId`, stale M0 notes).

Refuted (checked, not real): prof-clock suspend-flake claim, dropped-header
claim, sysop early-return-leak claim.

## 6. Verification status

- `cargo test` green across `bex_vm`, `bex_events`, `bex_events_native`,
  `bex_engine` (incl. prof_gate 14, tracing 23, call_notifications 7),
  `bridge_ctypes`, `baml_cli` (210), `baml_tests` (1,603 snapshots), and the
  rest of the workspace — **439/439 in the affected crates** at tip.
- `cargo clippy --all-targets` 0 warnings; `cargo fmt --check` clean.
- One snapshot regenerated: `baml_cli` describe listing (line numbers
  shifted by `ns_id/id.baml` doc/throws changes — content-identical).
- Not run here (sandbox lacks toolchains; need CI confirmation):
  `bridge_python` (pyo3 build), `sdk_test_typescript_node` (tsc/vitest),
  loom/miri jobs (`--cfg baml_loom`), wasm32 builds.

## 7. Pending items / things to check before & after landing

### Decisions — RESOLVED 2026-06-11 (Antonio) and implemented on this branch

1. **Function-level `Cancelled` status** → DECIDED: added
   `FUNCTION_END_STATUS_CANCELLED = 2`. In-flight sysop pairs at cancel now
   close `Cancelled` (was `Error`), as does the queued-then-cancelled
   spawn's entry close.
2. **Cancellation balance gap** → DECIDED: drain at cancel. The engine
   closes every open call frame of the suspended VM innermost-first with
   `EndFunction{Cancelled}` (`prof_drain_open_calls`, called at the seven
   blocks that terminate a thread without unwinding the VM: six cancel
   blocks with `Cancelled`, plus the unobserved fire-and-forget child-error
   surfacing with `Errored` — found by the post-decision adversarial
   review; previously that path stranded every open parent frame).
   Cancelled threads never strand open calls; the KNOWN GAP tests now
   assert full balance and `assert_balance_allowing_unended` is deleted.
3. **exit(0) frame status** → DECIDED: exit is a recognized unwind class.
   Added `FUNCTION_END_STATUS_EXITED = 3`; the unwinder peeks the thrown
   value's panic class (`prof_unwind_status`: Exit→Exited,
   Cancelled→Cancelled, else Errored — frame fate, not program outcome) and
   `baml.sys.exit`'s own native pair closes `Exited`. Thread-level mapping
   unchanged (root threads: exit(0)→Completed, nonzero→Errored; a child
   terminated by an exit settles like any unhandled throw → Errored, so
   EXITED frames are the reliable exit signal off the root).
4. **`Errored` vs `Error` naming** → DECIDED: uniform "STATUS_ prefix +
   past tense": `FUNCTION_END_STATUS_ERROR` renamed to
   `FUNCTION_END_STATUS_ERRORED` (tag 1 unchanged); `OK` stays (Result
   convention). Thread enum already conformed.
5. **Unknown-function sentinel** → DECIDED: bless `function_id: 0` as the
   wire contract for unattributable calls; the `baml.<unknown-function>`
   row (max+2) stays as the display bucket. Documented in the proto header;
   no re-pointing (0 is also where forgot-to-stamp bugs land — a soft,
   honest failure mode).
6. **Watch-filter calls in the stream** → DECIDED: keep emission — "it's
   code that runs". Filter time is real and attaches under the interrupted
   call; program-only views hide it renderer-side. Documented in the proto
   header and at `interrupt()`.
7. **Root-thread record order** → DECIDED: moved the root `StartThread`
   emission into `run_entry_point`, immediately before `set_entry_point` —
   "every thread's first record is its StartThread" is now a wire invariant
   (pinned in `reconstruction_smoke`). BALANCE: no early return may be
   introduced between that emission and the `run_thread_event_loop` call
   (comments pin both ends).
8. **`$id` call-site form `foo($id = ...)`** → DECIDED: defer (product
   call). The targeted diagnostic stays; design docs need a
   "not yet implemented" marker. Purely additive later; `SetFunctionId`
   last-wins semantics already accommodate it.
9. **SetFunctionId dedup** → DECIDED: keep per-`set()` emission; a call's
   effective `$id` at a point in time is its most recent record, last-wins
   for a single label. Documented in the proto header.

### Follow-up work (either of us, post-land)

10. **Delete the producer-less interim JSONL plumbing end-to-end**:
    `EventSink::send_disk_event`/`send_event_file_header` (+ FanOut/native
    impls and `BufferedEvent::Disk`/`Header`), `serialize::disk_event_to_jsonl`
    + its shape-pin tests, `bex_events::clock`, `bex_events::DiskEventV1`
    (legacy Rust enum, distinct from the proto), and
    `BexEngine::event_file_header_v1` if no host needs it. All are dead but
    tested; docs now say so explicitly.
11. **Spawned-task drop leak** (abnormal host teardown only): a spawned task
    dropped at an await before its event loop runs leaks an unclosed
    StartThread/CallFunction. A small RAII closer in `spawn_thread_inner`'s
    task body would close it; sketch is in the review notes.
12. **Adopt the `ids.rs` newtypes in `RawRecord`** (fields are plain u64 —
    the remaining M0 unification step).
13. **Port the two JSONL-only tests**: early-yield-resume-identical-stream
    and dropped-future truncation policy (T22), against `.bamlprof`.
14. **T27 call-heavy bench** under `tools/speedtest/workloads/` (Antonio's
    microbench exists; the speedtest-integrated workload doesn't).
15. **Serde-derive serializers** (§7.4) — only relevant if any JSONL shape
    survives item 10; otherwise dies with it.
16. **Default-on flip** for `BAML_PROFILE` once PR4/PR5 gates have CI
    history (his PR5 note), in its own one-line PR.

### Things to double-check in review

- The 54-hunk resolution in `bex_engine/src/lib.rs` / `bex_vm/src/vm.rs`
  (the merge commit message lists every decision; the adversarial pass
  specifically hunted junction errors and found two, both fixed).
- `vm.rs` `try_unwind_exception`: the popped-traced-frame count + the
  interrupt-boundary crossing both force the `SpanNotification::Unwound`
  yield — confirm the crossing bump matches your mental model of the filter
  contract.
- The override stack pop in `prof_exit_call` runs **unconditionally**
  (identity semantics, not profiling) — placed before the ring gate.
- `build_program_metadata` + the stamping walk + `function_ids_by_ptr` must
  stay the *same walk* (comments now say so) — any future reorder breaks
  table↔record id agreement silently.
- CI must run loom/miri + wasm + pyo3/TS-SDK jobs that this sandbox could
  not.
