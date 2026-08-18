# Telemetry MVP — the minimum data set behind the design-first UI

> **What this is.** We built the Telemetry UI first (branch `paulo/cct-1`,
> `pkg-playground/src/obs/TelemetryView.tsx`) and let two design reviews shape
> it against prototype data. This doc walks back from every surface that
> survived review to the *minimum* engine/wire data it needs. It is a scoping
> doc, not a design doc: 03-profiler.md and 06-studio-experience.md stay
> canonical for the full system; this is the subset that makes the UI land.
>
> **Provenance.** UI shapes: confirmed in review (2026-08-17/18 sessions).
> Wire status: verified against `obs_ws.rs` + `bqf1.ts` on this branch.
> Engine deltas: proposed here, not yet decided.

---

## 1. The whole UI reduces to four records

Everything on screen is one of these four, or a client-side join of them.
That is the elegance criterion this doc optimizes: **no new aggregate kinds,
no new query language, one new join column.**

### R1 — Run (one per parentless thread)

| field | status | notes |
|---|---|---|
| run_key, target, entry_point, source, status | **serves today** (`runs` frame) | |
| created_ms, completed_ms, revision | **serves today** | |
| enters_total, errors_total, span_count | **add: 3 columns** | engine has them at seal; feeds list expand + % traced without opening the run |
| failure (root error type + short message) | **add: 1 interned string** | set at seal from the root call's error; feeds the red line in the list and the Overview panel title |
| code_modified_ms | **add: 1 i64** | max source mtime (or commit ts) recorded at revision build; replaces the "rev 1" chip with "code as of 11:38" |
| epoch↔monotonic anchor | **add to `run_meta`: 1 pair** | makes Trace wall-clocks exact instead of min-startNs approximation |
| stdout_cid | **fast-follow** | bounded ring → CAS blob at seal; UI already has the honest empty state |

### R2 — ContextNode (one per CCT path; identity `(parent_node, function)` unchanged)

| field | status | notes |
|---|---|---|
| node_id, parent, function_id, depth | **serves today** (`left_heavy`) | |
| enters, errors, self_ns, total_ns, folds | **serves today** | |
| **await_ns** | **add: the one new aggregate** | one u64 per node in the fold + one wire column. Unlocks: wall\|busy flame toggle, Await card, honest CPU-vs-IO story. This is the single engine data-model change the reviews kept asking for |
| spawn-edge flag | **add: 1 bit** | reader knows it; needed for the "spawned" badge + flame normalization |
| histogram | **cut for MVP** | see §4 — LLM histograms are derivable from spans, exactly, for free |

### R3 — Span (retained call: recent ring, roots, LLM policy, `$id`, error promotion)

| field | status | notes |
|---|---|---|
| thread, call_id, parent_call_id, function_id | **serves today** (`recent_calls`, same-process only) | |
| start_ns, end_ns, status | **serves today** | |
| **cct_node_id** | **add: 1 u32 — the join column** | the single highest-leverage addition. Kills the labeled "joined by function identity" approximation and makes *derived*: exact span↔context pivots, per-node "% traced", per-row Σ gap badges (`enters − retained`), exemplar lists |
| error_type, error_msg | **add: 2 interned strings on errored spans** | feeds the unhandled-exception panel + caught-errors list without waiting for full value reads |
| args/result/error bodies | **fast-follow: CID read endpoint** | UI already distinguishes "not captured by policy" vs "not readable over this connection" |
| capture reason | **not needed** | the reviews deleted every UI use of it ("vibe coded") |

### R4 — Thread (already recorded on the wire; not yet served)

`StartThread (0x03)` already carries child, parent, spawning call, timestamp,
name; `EndThread (0x04)` carries status. **This is serving work, not
capture work** — the headline demo feature costs no new instrumentation.

| field | status | notes |
|---|---|---|
| thread, first_ts, last_ts, busy_ns, await_ns, errors | **serves today** (`timeline`) | |
| name | **add** | today the UI synthesizes `t3` |
| parent_thread, spawn_call_id, spawn_function_id | **add** | feeds "spawned by ProcessCheckout at +142ms" lineage + indentation |
| end_status | **add** | cancelled/failed markers on lanes |

Either extend the `timeline` frame or add a `threads` frame; extending is
fewer moving parts.

### CAS-backed extras (both fast-follow, both have honest empty states in the UI)

- **Value bodies**: `value_read(cid)` over `/api/obs`, policy/authz-gated.
- **Source snapshot**: `revision_files(revision)` → file list + contents
  (exact revision, never the working tree). Feeds the Code panel and the
  Timings `·src` sort for live runs. `run_meta` additionally needs
  `function → (file, line)` plus a `kind` column (llm/host/spawn) — kind also
  drives the purple/gray palette and the policy displays.

---

## 2. Per-UI-feature map

| UI feature (as built) | Needs | Status |
|---|---|---|
| Executions list, compressed rows | `runs` frame | **works today** |
| — expand: calls / caught errors / % traced | R1 totals columns | 3 columns |
| — expand: stdout | R1 stdout_cid | fast-follow |
| — red failure line, "N caught" badge | R1 failure + errors_total | 2 columns |
| — "code as of" chip | R1 code_modified_ms | 1 column |
| Overview: wall / CPU / IO cards | `timeline` busy/await | **works today** |
| Overview: unhandled-exception panel + call stack | R3 error_type/msg (chain itself is a client-side walk of failed spans) | 2 columns |
| Overview: caught-errors panel | same + CCT errors | same |
| Overview: where-time-went | `left_heavy` | **works today** |
| Overview: Threads panel (offsets + lineage) | R4 additions | serving work only |
| Trace: span rows, depth, timestamps | `recent_calls` + runs.created_ms | **works today** (approx clock → exact with anchor) |
| Trace: thread column | R4 names | with Threads work |
| Trace: Σ gap badges (`+43` on parent rows) | R3 cct_node_id (badge = `enters − retained`, derived) | 1 column |
| Trace: inspector values | value_read | fast-follow |
| Trace: search / filter | — | client-side, done |
| Timings: tree + flame, Counts/Errors/Total/Self/% | `left_heavy` | **works today** |
| Timings: wall\|busy toggle | R2 await_ns | 1 aggregate |
| Timings: % traced column | R3 cct_node_id | shared with badges |
| Timings: histogram sparkline | derive from LLM spans (§4) | free for LLM fns |
| Timings: ·src sort + line anchors | run_meta file/line | with source work |
| Code panel | revision_files | fast-follow |
| Purple = AI palette | run_meta kind column | 1 column |

---

## 3. The complete engine/wire delta, in one list

Must-have to demo on **real** data (ordered by leverage):

1. **`cct_node_id` on retained calls** (u32/row) — the exemplar pattern
   becomes a pure join; three UI features fall out derived.
2. **Thread lineage serving** (name, parent, spawn call+fn, end status) —
   already captured; the demo's headline view.
3. **`await_ns` per CCT node** — the one true aggregate addition.
4. **Run seal summary** (enters_total, errors_total, span_count, failure,
   code_modified_ms) — five cheap columns written once per run.
5. **Error identity on errored spans** (type + truncated message, interned).
6. **`run_meta` extensions** — fn kind, fn file/line, epoch↔monotonic anchor.

Fast-follow (UI ships with honest empty states meanwhile):

7. `value_read(cid)` endpoint (bodies in inspectors).
8. `revision_files` endpoint (Code panel + live ·src sort).
9. stdout capture → CAS.

Cost sanity: span row grows ~4B (+2 interned ids on the error minority);
CCT node grows 8B; thread rows are already bounded by the threads table.
Nothing becomes traffic-proportional that wasn't already.

---

## 4. Derived, not stored (deliberate)

- **Gap badges / "counted only"**: `node.enters − count(retained spans on node)`. Needs (1); never a stored number.
- **Unhandled vs caught split**: client walk of failed spans from the root.
- **% traced** (list and per-node): same join.
- **LLM histograms**: LLM functions are 100% retained under default policy,
  so their duration histogram is *exactly* reconstructible from spans —
  post-computed, zero runtime tax. Engine histograms (capture-policy column,
  configurable bins) stay a later opt-in for non-LLM functions.
- **CPU/IO cards**: sums over thread lanes.
- All sorting, filtering, search, pivots.

## 5. Explicitly cut from this landing

Wait-edge (thread-joined-on) visualization · logs in Trace · `@id capture`
write-back (hover hint ships, action doesn't) · cross-run comparison ·
spawn-instance-table UI · process-ID lanes · engine histograms · in-app SQL.
Also unresolved and parked: sibling order from source (contradicts the
standing "never infer" decision), ExecutionPanel tab restructure (fold Graph
into per-execution Code+Telemetry tabs).

## 6. Suggested order

1 → 2 → 3 land the demo on real data (each is independently shippable and
each unblocks visible UI already built). 4–6 are one sweep over seal +
run_meta. 7–9 as capacity allows. The UI needs no further changes to absorb
any of them — every consumer is already written against these shapes with
labeled approximations that simply disappear.
