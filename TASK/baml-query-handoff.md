# Handoff: Implementing `baml query` and its Backend Prerequisite

You are inheriting a fully specified, decision-complete plan. Your job is
implementation, not design. If you find yourself inventing behavior, stop —
that is a spec gap; amend the spec first (Section 8 rule 1).

---

## 1. Environment

- Repo: `/Users/rossir/dev/baml`. Two Rust workspaces: **`baml_language/`**
  (all of this work lives here; toolchain 1.93.0, wasm32 target installed)
  and `engine/` (legacy; do not touch). Frontend in `typescript2/` (do not
  touch for this work).
- Branch stack: `canary` ← `paulo/re-profiling-backend` (`7a351097c`, the
  segmented profiling backend MVP) ← **`paulo/baml-query-scope`** (you are
  here; one docs commit). Implementation lands as new commits on this branch
  (or stacked branches per phase, base `paulo/re-profiling-backend`); PRs
  ultimately target `canary`.
- `TASK/*` is ignored via `.git/info/exclude`; doc changes need `git add -f`.
  Pre-commit hooks run fmt/clippy/etc. on staged files.
- Do not commit unless asked; when asked, prefer one commit per phase.
  End commit messages with the `Co-Authored-By` / `Claude-Session` trailer
  convention used on this branch (see `git log -1 --format=%B`).

## 2. The documents, in precedence order

1. **`TASK/profiling-backend-streams.md`** — P0: the storage rework. Byte
   layouts, writer/reader contracts, failure semantics, acceptance gates
   (§9), MVP amendment list (§11), resolved decisions (§12). *Reviewed by
   four adversarial passes; treat every sentence as load-bearing.*
2. **`TASK/baml-query-scope.md`** — the query layer: crates, catalog v1
   (§4), DataFusion design (§5), CLI contract (§6), prerequisites ledger
   (§7), phases and gates (§8), remaining open questions (§10), cct-1 reuse
   checklist (§11 appendix).
3. **`TASK/profiling-backend-mvp.md`** — the standing backend contract that
   P0 amends. Everything it pins that the streams spec does not amend is
   still binding (capture policy, CCT semantics, memory governor, disk
   guard, publication protocol, acceptance gates).
4. `TASK/baml-query-catalog.md` — reader-facing table guide (keep in sync
   with any catalog change).
5. `TASK/reference/**` — stale background only. Never treat it as evidence
   of what exists.

Where the streams spec and the MVP disagree, the streams spec wins; where
any doc and the code disagree about *current* behavior, the code wins — and
that discrepancy goes in the log (Section 9).

## 3. Code map (what exists today)

Backend (`baml_language/crates/bex_events/src/`):
- `prof/backend/store.rs` (1806) — publication protocol, locks, usage
  ledger, disk guard, CAS. `reader.rs` (576) — `DurableRunReader` (to be
  replaced). `decoder.rs` (2463) — live record decoder, thread→execution
  joins, epochs, evidence batches, finalization. `session.rs` (1590) —
  admission, consumer maintenance. `boundary.rs` (706) — slot registry.
  `cct.rs`/`cct_codec.rs`, `evidence.rs`/`evidence_codec.rs` — payload
  codecs (reused inside P0 groups). `domain.rs` — `ContextKey`, `ValueCid`,
  capture plan. `sizing.rs`/`memory.rs` — derived sizing, governor.
- `ids.rs` — all id types and wire encodings. `prof/{record,clock,consumer,
  registry,ring}.rs` — producer records, tick clock, consumer loop.
- Engine call sites: `bex_engine/src/lib.rs` — admission `:3560-3620`,
  `build_program_metadata` `:1587-1697`, `activate_profiling` `:2139-2154`,
  spawn/thread plumbing `:6059-6116`. `bex_engine/src/function_call_context.rs`
  — host token minting. `bex_vm/src/vm.rs` — record emission `:5661-6006`.
- CLI: `baml_language/crates/baml_cli/src/` — `run_command.rs`,
  `test_command.rs`, `clean_command.rs`, `commands.rs`, `output.rs`
  (house style: `--output-preset`, exit codes, insta help snapshots).
- Tests you will rewrite/extend: `bex_engine/tests/profiling_backend.rs`
  (4 tests), `bex_engine/tests/identity.rs` (**must pass unchanged**),
  `baml_tests/examples/profiling_e2e_verify.rs` + `baml_tests/profiling_e2e/
  run.py`, `baml_tests/benches/profiling_overhead.rs`,
  `bex_events` unit/golden tests listed in streams spec §9.

Porting sources (read with `git show`, never check out):
- `origin/paulo/cct-1` @ `32b9fe5fd` — `baml_language/crates/baml_query/`,
  `baml_query_local/`, `baml_cli/src/query_command.rs`,
  `baml_query/tests/q1_gates.rs`. The scope doc §11 appendix says
  file-by-file what to port vs rewrite.
- PR #4343 (`origin/codex/local-query-engine`) — **anti-pattern reference
  only** (SQLite, per-batch budgets, dead duplicate providers). Do not port.

Untouchable: the legacy history/playground plane (`bex_events/src/{run,
run_wire,history,value}/`, `baml_lsp_server/src/playground_*`, `bridge_wasm`)
except the one LSP ignore-pattern string (streams §8); `fern/`; `engine/`.

## 4. Implementation order

Work strictly in this order; each step has its own definition of done.

**Step 0 — B0, leaf crate** (optional first, recommended): move `ids.rs` +
`prof/backend/*` + `prof/{record,clock}.rs` into new crate `bex_prof_store`;
`bex_events` re-exports via `pub use` so no caller changes. Deps allowlist:
`sha2, hex, fs2, rustc-hash, smallvec, base64, uuid` (+ `web-time` wasm).
Done = workspace builds, all existing tests green, `cargo tree -p
bex_prof_store` shows no VM/tokio deps.

**Step 1 — P0** (streams spec, whole document): formats §4, store §5.1,
writer §5.2-5.4, admission §5.5, finalization §5.6, fork guard §5.8, reader
§6, engine/session changes §7, ProgramId hash §2.3. Apply the MVP amendments
(§11) in the same PR. Done = every gate in streams §9 passes.

**Step 2 — Phase 1, `baml_query` core** (scope §3, §4.1, §5): port cct-1
modules per the reuse checklist onto the neutral `Value` model; catalog
registry + `CatalogProfile` + `ViewDef` (views: `llm_calls`, `hot_contexts`
only — **no `executions` view**, decided); lazy `SchemaProvider` +
information_schema; batched `resolve_many`; budgets/outcome/errors. Done =
conformance corpus (ported q1 gates + Phase 1 additions in scope §8) green;
dependency-allowlist test green.

**Step 3 — Phase 2, `baml_query_profiles`** (scope §3.2, §5.3-5.6): bind
from meta planes, per-execution fold LRU, six public + three internal
relations, pushdown classes, CAS resolver + `BamlOutboundValue → Value`
decoder (B4). Done = e2e goldens, determinism gate, perf gates (scope §8
Phase 2).

**Step 4 — Phase 3, CLI** (scope §6): `baml query` verb, `--schema`,
formats, exit codes 0-5, describe topic (agent-native docs — never `fern/`),
help snapshots, size-gate measurement. Done = scope §8 Phase 3 gates +
`exit_code_e2e` additions.

Phase 4 (playground endpoint) is not in this handoff's scope; stop after
Phase 3 and report.

## 5. Decisions already made — do not relitigate

DataFusion is the engine (pinned `=` version, minimal features). Execution =
parentless thread; no `BoundaryId`/"run" in durable formats; `baml.id.*`
language surface unchanged. Catalog = `threads/contexts/calls/errors/
functions/health` + 2 views, `_v1` names + aliases. `publish_interval` = 1 s.
Meta-plane loss tolerated, never latches. `ProgramId` = conservative
source-content hash (streams §2.3 — any byte, comments included, splits;
fallback random; the later semantic hash must use a new domain string). No
`executions` view. Exit codes 0-5. Native only; wasm never links DataFusion.
Contexts column is `calls_selected` (the `spans_selected` counter).

## 6. Open questions — resolve with Paulo *before* the phase that needs them

| # | Question (scope §10) | Blocks | Default if unanswered |
|---|---|---|---|
| 1 | `_v1` + alias scheme, secondary aliases | Phase 1 catalog freeze | keep as spec'd |
| 2 | `context_id` form: hex vs `baml_ctx_1_` wire vs dense | Phase 1 catalog freeze | hex (spec'd), lean `baml_ctx_1_` |
| 3 | CID literal semantics / `bamlv_1_` prefix reuse | Phase 1 value layer | identity-only on `*_cid` columns |
| 4 | logs in v1 | nothing (answer is "wait") | wait |
| 5 | acceptable binary-size delta / `query` feature flag | Phase 3 ship | measure, report, ask |
| 6 | `BAML_INTERNAL` exposes internal relations? | Phase 3 | yes, reuse `BAML_INTERNAL` |
| 7 | playground endpoint timing | Phase 4 | out of scope |

Ask in the format that worked: question → plain meaning → interactions →
recommendation → trade-offs.

## 7. Pitfalls (each one was found the hard way in review — respect them)

P0:
1. **Publication cycle order is meta-pre → data → meta-post** with the
   `RootEnded` eligibility rule (streams §5.3). Any reordering breaks either
   the "RootStarted before its data" invariant or the finality of
   `RootEnded.data_*`.
2. **`Blocked` vs `Indeterminate{seq}`** are different store outcomes
   (batch not written vs written-at-sequence). Conflating them double-writes
   or drops batches; the MVP's `reserve_and_publish` has this latent bug
   (`store.rs:775-781`) — do not copy it.
3. **`apply_batch_outcome` must run on every data Committed/Lost** or
   `SpanState::Queued` never resolves and dangling `SpanEnd`s violate MVP
   §7.3. It is a no-op for released slots — that is correct, not a bug.
4. **Admission facts ride the registry slot** (`take_admitted`), never the
   lossy `DecoderCommand` lane and never a new "acknowledged lane" (none
   exists). `EngineStarted` rides the registry-side vector, drained *after*
   the slot scan, ordered before same-engine `RootStarted`s.
5. **`started_ns` = `admitted_ticks` sampled in `register_root`**, not the
   `StartThread` record (emitted later, can be lost).
6. **`Owner::Writer` has a 64 KiB minimum charge** — the writer takes one
   `meta_queue` reservation at session start, never per-record reservations.
7. **`StreamStarted` only when `high_water().meta == 0`** — re-opened
   streams must not re-emit it.
8. **Keep the indeterminate admission gate** (`is_indeterminate()` check in
   `register_root`) — it is what bounds `pending_meta_*`.
9. **Group prefix is 49 bytes with two record counts**; sub-decoders need
   them. Readers read whole files (trailing SHA-256) and skip foreign groups
   by slice arithmetic.
10. **`SCHEMA_VERSION = 2` for segments only; CAS keeps `CAS_FORMAT_VERSION
    = 1`** — split the constant or you invalidate every stored value.
11. **Goldens to regenerate** are enumerated in streams §9 (evidence.rs
    ×2, evidence_codec.rs; cct_codec fixture is a source edit, hash
    unchanged). Regenerate deliberately, never blindly.
12. **`identity.rs` must pass unchanged.** If it doesn't, you changed the
    language surface — revert.
13. **Fork guard**: `register_root` checks `std::process::id()`; children
    profile nothing. Don't "fix" the inherited-lock liveness false positive.
14. **Reader liveness**: same-process short-circuit via `OPEN_STREAMS`, not
    lock probing (NFS flock emulation).
15. `flush_and_join(5 s)` on **every** CLI exit path, including
    `ExitCode::Exit(code)` branches that call `process::exit` directly.

Query layer:
16. **No eager materialization** — providers are lazy, prune from the meta
    plane, fold per execution (cct-1's `MemTable`-everything is the smell
    you're removing, scope §2.3).
17. **Value predicates never push down; LIMIT pushes only below
    exact-only filters** — gate `final_limit_never_reaches_the_provider…`
    must survive the port.
18. **Never let handle bytes escape**: bare `SELECT args` renders text;
    keep the relation-qualifier fix (`32b9fe5fd`).
19. **Don't shadow DataFusion built-ins** (PR #4343 broke string
    `contains`); all internal UDFs are `__baml_*` and rejected in user SQL.
20. **Typed catalog↔provider coupling**: per-relation typed row builders,
    not `match relation.name` + `unreachable!`.
21. Dependency allowlists are tests, not conventions — `baml_query` never
    names `bex_events`; `baml_query_profiles` never names `bex_engine`;
    wasm gate `cargo tree -p bridge_wasm -i datafusion` empty.

## 8. Working rules

1. **Amend-first**: any implementation finding that changes capture policy,
   record shape, ordering, failure behavior, or a resource guarantee →
   amend the spec *and* its gates before the code lands (MVP §16 discipline;
   streams §0 restates it).
2. **Evidence before hypothesis** on any CI/test failure: reproduce, read
   the actual output, check the baseline (`.claude/skills/fix-ci` and the
   standing CI working agreement). Never weaken a gate to pass it; a gate
   change is a spec amendment (rule 1).
3. Loss is never silent: every bound you add must have a counter or a typed
   error, and every counter must be reachable by a test.
4. Keep the docs true: catalog changes update `baml-query-catalog.md`;
   layout changes update the streams spec; landing P0 applies the MVP §11
   amendments in the same PR.

## 9. Progress log

Create `TASK/baml-query-log.md` on first deviation or measurement, in the
style of `TASK/profiling-backend-mvp-log.md`: "Things that did not work",
deviations from the canonical plan (with why), measured numbers backing
each perf gate, and the verification-commands transcript for each phase.

## 10. Verification commands

~~~bash
# unit + backend
cargo test -p bex_events --lib
cargo test -p bex_engine --test profiling_backend -- --test-threads=1
cargo test -p bex_engine --test identity
# concurrency model (loom)
RUSTFLAGS="--cfg baml_loom" cargo test -p bex_events prof::concurrency_tests::loom_suite:: -- --nocapture
# query layer (once created)
cargo test -p baml_query --all-targets
cargo test -p baml_query_profiles --all-targets
# lint / format / hygiene
mise exec -- cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check && git diff --check
# wasm must stay clean
cargo check -p bex_events -p bex_engine -p bridge_wasm --target wasm32-unknown-unknown
cargo tree -p bridge_wasm -i datafusion   # must be empty / error "not found"
# e2e + perf (from baml_language/)
python3 crates/baml_tests/profiling_e2e/run.py
cargo bench -p baml_tests --bench profiling_overhead
~~~

(Run from `baml_language/`. The full gate list with expected numbers:
streams §9 and scope §8.)

## 11. Invariants that must hold at every commit

- BAML execution is never blocked, slowed unboundedly, or failed by the
  profiler; profiler-off (`BAML_PROFILE=0`) creates no profiler state at all.
- No unbounded memory: every pending set has a governor reservation or a
  structural bound; every overflow is a counted loss, never an abort.
- Crash safety: committed files are readable; a checksum failure is typed
  corruption; nothing is ever silently reinterpreted.
- Root admission does no I/O and takes no lock (p99 < 20 µs gate).
- `spans_selected/calls_selected ≥ resulting calls rows`, and the MVP §7.3
  terminal equations reconcile whenever `RootEnded` is durable.
- Language identity: `baml.id.*`, `boundary.id()`, `$id`, call refs —
  byte-for-byte today's behavior (`identity.rs` is the oracle).
- Byte-identical builds ⇒ identical `program_id`/`ContextKey`s; any source
  byte or compiler change ⇒ different (once P0 lands).
- Every SQL result stream ends with exactly one typed outcome; unavailable
  values are counted, never silently NULL; exit codes 0-5 mean what scope
  §6 says.
- Format versions: segments v2, CAS v1, catalog `_v1` — bump, never mutate.
