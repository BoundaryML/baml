# BAML LSP latency and mutex regression — implementation-qualified design

> **Status:** v4 is the implementation contract for Phase 0 and Phase 1. The root-cause
> diagnosis and B1 memoization are ready to execute; the remaining active work has explicit merge
> gates below. All product/behavior decisions are resolved, so the active design is ready for
> phased implementation. Shared Salsa snapshots, cancel-on-write, and the request worker pool are
> deferred.
>
> **Provenance:** v1 was produced 2026-07-08 from a multi-agent investigation against
> `canary` and installed 0.14.0/0.14.1 binaries. v2 incorporated a first independent
> review on 2026-07-09. v3 incorporates a second review by independent engine/runtime,
> protocol/encoding, and client/ingress reviewers against local `canary` at
> `d7bb6f9555405f33200cff4822e79962f7812477`. The local `origin/canary`
> reference was three commits ahead, but those commits did not touch the reviewed paths.
> v4 records the selected D1-D8 behavior, treats `$init` locality as an
> engineering fact rather than a product decision, and defers the unwind-dependent Phase 2.
> The external latency harness was not rerun for v3 or v4.
>
> **Measurement harness:** `/tmp/baml-inlay-repro` (`drive_lsp.py`,
> `measure.py`). Measurements below are from 0.14.1 on a shared machine and carry
> the original ±15% qualification.

## 0. Implementation status and phase gates

| Area | Status | Gate |
|---|---|---|
| Root-cause diagnosis | Qualified | Preserve the measured mechanism and rerun the harness after each latency phase. |
| B1 Salsa memoization | Ready | Warm-hit and RSS tests must pass. |
| Phase 0 containment | Engineering gates open | Source revision, conditional commit, coherent runtime state, durable diagnostics, and profiling cleanup must land before the bounded-wait hotfix is complete. |
| Client ownership (A1/A2) | D1 resolved | One outer owner multiplexes nested semantic projects; initialization, selector, watcher, command routing, and server discovery share that boundary. |
| Minimal ingress (B2) | D2-D3 resolved | One active browser session, shared stdio/browser policy, fail-fast excess reads, exactly-once cancellation, and lifecycle/mutation barriers. |
| Debounced tail (B3/B4) | Depends on B2 | Internal timer work must have reserved admission and source-revision fencing. |
| Position correctness (C1/C2) | D4 resolved | Negotiate UTF-8 when offered, otherwise UTF-16; keep a separate fixed-UTF-16 playground contract. |
| Demand-gated runtime (B5) | D5-D8 resolved | Warm the selected project, render preparation immediately, require current source for new Run/Test, and let already-started runs finish pinned. |
| Phase 2 snapshots/workers | Deferred | The abort-profile LSP may not race shared Salsa clones with writes or use Salsa cancellation. Reconsider only through a separate value-qualified proposal. |

If a compatibility release cannot fit Phase 0's revision and commit invariants, restore the
serialized pre-0.14 rebuild behavior. An unfenced background installer is not an acceptable
hotfix.

## 1. Root cause and evidence

### 1.1 Regression mechanism

PR #3938 (`f2bdbdfc2`, shipped in 0.14.0) moved engine rebuilds off the single
LSP dispatch thread into a 300ms-debounced `spawn_blocking` task. That introduced a
second thread holding `BexProject`'s `std::sync::Mutex<ProjectDatabase>`
during the diagnostics gate and full non-incremental bytecode generation.

Request handlers acquire that mutex through `try_lock_db`
(`bex_project/src/project.rs:87-95`). Any failure—including ordinary contention and
poisoning—is currently serialized as JSON-RPC `-32001`
(`multi_project/mod.rs:80-84`). Before 0.14, rebuilds ran on the one dispatch thread,
so requests queued slowly but never raced the database mutex. That is why the regression
bisects to #3938.

Two correctness bugs share the mechanism:

- `get_bytecode` uses `try_lock` and can silently abandon a rebuild
  (`project.rs:235-242`, `multi_project/mod.rs:664-669`);
- `diagnostics_by_file` maps contention or poison to an empty map
  (`diagnostics.rs:166-173`), which can clear every published diagnostic.

The v3 review found a more serious engine race: `rebuild_epoch` is only a debounce
ticket. A stale task can install its engine before the scheduler's post-build epoch check.
Two tasks can finish out of order, allowing an older engine to permanently overwrite a newer
one. Source-revision-conditional commit is therefore a Phase 0 blocker, not Phase 2 polish.

### 1.2 Measured symptoms

| Metric | @400 functions | @1500 functions |
|---|---:|---:|
| inlayHint p50 while typing | 234-256ms | 1145ms |
| didChange → publishDiagnostics p50 | 45ms | 459ms |
| Post-pause rebuild DB hold | 156-166ms | ~302ms observed; ≲1s bound |
| `-32001` failures per pause | 42-48 | 115 |
| Five idle hints | 68/113/159/209/251ms | — |
| Hint p50 under 10ms spam | 2.1-2.4s; max 4.8s | — |
| Unanswered requests at shutdown | 114-146 | 198 |
| didChanges delivered in 15s | 100/100 | 16/~100 |

Other verified behavior:

- `$/cancelRequest` is rejected and the target request still completes;
- per-keystroke diagnostics and playground payload construction run synchronously;
- `build_project_update` takes a blocking DB lock;
- stdio dispatch is serial;
- the browser LSP bridge has a separate unbounded serial dispatcher;
- several playground paths hold the projects-registry lock across database work.

## 2. Cross-phase correctness invariants

These are engineering invariants, not product choices. Every phase must preserve them.

### I1. `SourceRevision` is authoritative

`BexProject` owns a monotonic source revision. It advances once for every accepted
source-mutation batch: open/change/close, watched-file refresh, full replacement, file
add/remove, and playground edit. Database mutation, revision advance, client URI, open-document
version, and text identity are one transaction. A captured `SourceSnapshot` contains those
values together; diagnostics may not combine database contents from one revision with a version
map captured under another lock.

The revision and database contents are read or changed under one mutation gate. Lock-free
revision observation may be added for diagnostics, but it cannot authorize commit.
`rebuild_epoch` is renamed or documented as a debounce ticket only: it may suppress
work before it begins, but it may never authorize installation, publication, test collection,
or run launch.

### I2. Background work produces owned, revision-tagged candidates

Database reads produce an owned, revision-tagged outcome. Invalid source is a first-class
outcome and does not require a `Program` or engine commit:

```rust
struct DiagnosticCandidate {
    source_revision: SourceRevision,
    documents: OwnedVersionedDiagnostics,
}

struct CompiledCandidate {
    source_revision: SourceRevision,
    program: bex_vm_types::Program,
    diagnostics: DiagnosticCandidate,
}

struct EngineCandidate {
    source_revision: SourceRevision,
    engine: BexEngine, // unpublished; profiling lifecycle is inactive
    diagnostics: DiagnosticCandidate,
}

enum CompilationOutcome {
    Ready(CompiledCandidate),
    BlockedByDiagnostics(DiagnosticCandidate),
}
```

`DiagnosticCandidate` publishes through its own source-revision/document-version fence.
Only engine currentness, ready `UpdateProject` state, CFG/test work, and other
engine-derived output require an engine `CommitReceipt`. Only a winning commit activates
profiling and wraps the candidate engine in the installed `Arc<BexEngine>`.

No DB guard, Salsa snapshot, borrowed query result, projects-registry guard, runtime-state
guard, or dedupe-cache guard may survive candidate creation or cross an `await`.

### I3. Engine installation is one atomic conditional commit

Source mutation and `commit_engine(candidate)` use the same serialization gate and a
documented lock order. Commit compares the candidate revision with the authoritative current
revision and swaps all identity-bearing runtime fields in one critical section.

After the revision comparison wins—but before the engine becomes reachable—commit activates the
candidate's profiling lifecycle, registers metadata, wraps it in `Arc`, and installs it. This
activation is non-awaiting and part of the same commit outcome.

The only successful result is a `CommitReceipt` containing the source revision,
engine generation, and engine handle. A mismatched candidate returns `Superseded`.
A superseded candidate must not:

- install or mark an engine current;
- increment generation;
- cancel current runtime work;
- clear or install a test registry;
- populate a CFG cache;
- publish diagnostics through the engine path or publish `UpdateProject` (the separately fenced
  I2 `DiagnosticCandidate` remains authoritative);
- register profiling metadata, emit `engine_closed`, or create a closed-engine tombstone;
- start test collection.

A separate atomic revision load followed by a later engine swap is still racy and does not
satisfy this invariant.

### I4. Runtime identity is coherent

Replace the split `current_bex`, `TestState`, and mutable currentness flag
with one runtime state:

```rust
struct RuntimeState {
    installed: Option<InstalledEngine>, // source revision + generation + Arc<BexEngine>
    derived_epoch: u64,
    derived_cancel: CancellationToken,
    collection_epoch: u64,
    registry: Option<InstalledRegistry>, // revision + generation + collection epoch + handle
}
```

Currentness is derived from
`installed.source_revision == current_source_revision`. A source mutation immediately
makes the installed engine non-current. In the same mutation transaction it increments
`derived_epoch` and `collection_epoch`, cancels the derived token, and marks the installed
registry stale and non-launchable. This invalidates collection/expansion immediately; it does
not cancel the separately owned function/test-run tokens retained by Decision D8.

A winning engine commit allocates one generation, replaces the engine, installs a fresh derived
token, and clears or replaces the registry atomically.

### I5. Run and test entry use coherent snapshots

Replace `project_generation → CFG pin → get_bex_for_project` with one
`prepare_and_register_run` transaction:

```rust
struct RunLease {
    run_id: BoundaryId,
    source_revision: SourceRevision,
    generation: EngineGeneration,
    engine: Arc<BexEngine>,
    graph: Option<Arc<ControlFlowGraph>>,
    test_registry: Option<Handle>,
    cancellation: RunCancellationHandle,
}
```

Run preparation is ordered as follows:

1. If the target needs a graph, obtain or build an owned
   `CfgCandidate { source_revision, generation, graph: Arc<ControlFlowGraph> }` while holding
   at most the serialized live-database lane—never runtime, active-run, or cache locks—and then
   drop the database guard.
2. Enter the source/runtime registration transaction. Revalidate the CFG candidate, confirm that
   the installed engine still matches current source, capture the engine and—for a test run—the
   registry handle, then insert the `RunLease` before releasing the gates.
3. Populate any generation-keyed CFG cache after those guards are released. Active-run overlays
   resolve directly from the graph retained by the lease; they never rebuild from the live
   database.

This transaction linearizes edit versus run start:

- if the edit commits first, preparation returns `NeedsCurrentBuild(new_revision)` and D7
  rebuilds/retries against current source;
- if run registration commits first, the immutable lease owns the old engine state and D8 lets it
  continue after the edit.

Explicit Cancel resolves the lease by `run_id` and targets its retained engine/cancellation
handle, never the project's current engine. One terminal outcome removes the lease; natural
completion, explicit cancellation, and transport/session changes race through the run's own
exactly-once terminal state.

Test-tree collection and expansion are project-derived work: they atomically capture matching
engine, generation, derived token, registry, and operation epoch, then publish through I9. Their
stale success, empty, cancellation, and error paths emit nothing.

Function and test execution are run-owned work. Their events and terminal result publish by
`{ run_id, monotonically increasing run_sequence }`, not by project currentness, and remain valid
after the source revision advances.

### I6. Diagnostics never confuse empty, busy, and broken

Diagnostics use:

```rust
enum DiagnosticRead<T> {
    Ready(T),
    Busy,    // only TryLockError::WouldBlock
    Poisoned,
}
```

An owned result carries its source revision and the exact URI/document version for each open
file. `Busy` preserves the last publication and schedules a trailing retry.
`Poisoned` is an internal failure and never becomes an empty diagnostic set.

### I7. LSP errors are typed at one serialization boundary

Each connection owns an `LspSession`: a fresh session ID/epoch, lifecycle state, immutable
negotiated capabilities after initialize, outstanding-request registry, open-document ownership,
and outbound sink. Project/workspace state is shared separately. Dispatch carries an opaque
`ResponseToken { session_id, request_id }`; a bare request ID is never sufficient.

Expand `LspClientSenderTrait` (or replace it with `LspSessionSender`) so
`send_response(ResponseToken, TypedResult)` is the only request error-code mapping and
response-routing boundary for both transports. It also owns transport-level protocol errors with
an optional/null ID.

| Condition | Code |
|---|---:|
| Invalid JSON/framing | `ParseError` (`-32700`) |
| Invalid JSON-RPC request/envelope | `InvalidRequest` (`-32600`) |
| Request before initialize | `ServerNotInitialized` (`-32002`) |
| Unsupported method | `MethodNotFound` (`-32601`) |
| Malformed params/position/range | `InvalidParams` (`-32602`) |
| Unwind-enabled panic, poison, violated invariant | `InternalError` (`-32603`) |
| Explicit client cancellation wins response ownership | `RequestCanceled` (`-32800`) |
| External request invalidated by an applied source revision | `ContentModified` (`-32801`) |
| Same-revision busy timeout, overload, or unavailable valid request | `RequestFailed` (`-32803`) |

`UnknownErrorCode` (`-32001`) is not a fallback.
`ServerCancelled` (`-32802`) remains unused unless a particular method and
client capability explicitly support it.

This table does not imply that an abort-profile panic can be serialized. If a poisoned project is
observable in an unwind-enabled development/test build, it enters terminal `ProjectBroken`:
the current request receives
`InternalError`, later project requests are rejected, and the LSP schedules controlled
exit/restart. It never clears poison and continues. A true release panic aborts by current product
policy; v4 does not add in-process panic recovery.

A later `didChange` waiting in ingress is not proof that an earlier request is
invalid. Queue lookahead may not synthesize `ContentModified`.

### I8. Lock and publication discipline

The canonical order is:

1. clone project handles out of the projects registry;
2. acquire the source/database gate;
3. acquire runtime state only for brief capture or commit;
4. acquire the active-run registry only after runtime state when atomically registering a run;
5. touch the reserved publication mailbox only through a nonblocking enqueue while the source
   gate is held; it may never wait for or call the transport;
6. acquire caches only after source/runtime guards are released;
7. serialize and send with no shared lock held.

No code catches a panic while retaining a shared mutable guard and then continues as though
the state were repaired. Clearing mutex poison is not recovery from a partially applied write.

### I9. Project-derived publication is a conditional transaction

Every asynchronous project-derived publication—diagnostics, catalog/runtime state, CFG, test-tree
collection, or expansion—carries `session_id`, `project_id` plus project incarnation,
`source_revision`, and, when applicable, engine generation plus derived/collection epoch. Its
final currentness comparison and insertion into a reserved, nonblocking per-project outbound
mailbox happen while holding the same mutation gate used by source changes. Sending/serialization
happens later with no shared lock held.

The source-mutation transaction advances that mailbox's invalidation watermark. The sequencer
drops an envelope below the watermark even if it was queued before the mutation, and the
playground frontend independently rejects payloads older than its latest project revision/epoch.
LSP diagnostics additionally carry their exact document version. This closes the check-then-send
race without holding a project lock across transport I/O.

Run-owned function/test events are explicitly outside this watermark. They publish through the
run lease's immutable `run_id`, ordered sequence, and exactly-once terminal state, so a pinned run
can finish after its project source changes without allowing stale project state to reappear.

## 3. Phase 0 — correctness and compatibility containment

### Phase 0A. Source revision, candidates, and coherent runtime state

This lands before bounded request waiting.

1. Add authoritative `SourceRevision` to the database mutation gate.
2. Introduce the coherent `RuntimeState` and remove independently mutable
   currentness.
3. Split rebuild into:

   ```text
   capture revision R + compile owned CompilationOutcome
       → drop database guard
       → publish DiagnosticCandidate(R) through the diagnostics fence
       → if Ready, construct EngineCandidate(R)
       → commit_engine_if_current(R)
       → only CommitReceipt may publish ready runtime state or collect tests
   ```

4. Treat the debounce ticket only as a pre-work optimization.
5. Make `ensure_engine_current` single-flight by project and revision and return a
   coherent receipt/snapshot, never `()` followed by unrelated engine reads.
6. Replace run launch with I5's atomic `prepare_and_register_run`.
7. Capture engine/registry/test identity atomically and add a collection epoch so two
   collections on one engine generation cannot complete out of order.
8. Serialize expansion mutations per installed registry; the registry handle has one mutation
   owner.
9. Keep candidate profiling inactive throughout construction. Only the winning conditional commit
   registers metadata and activates the engine's profiling lifecycle immediately before install.
   Failed or superseded candidates drop quietly: no `engine_closed` observer notification and no
   permanent closed-engine tombstone.
10. Run synchronous `$init` as ordinary candidate construction outside the source mutation gate.
    It evaluates top-level bindings into that candidate's owned globals; events are dropped and
    async/sys-op yields fail initialization. It is deterministic and candidate-local, so a stale
    candidate may complete `$init` and then be rejected by conditional commit.

**Phase 0A merge gate**

- forced A/R1 and B/R2 builds finish in both orders; only R2 may remain installed;
- final invalid R2 while A/R1 is constructing may not install R1;
- invalid R2 publishes its versioned diagnostics without requiring an engine commit;
- a rejected candidate changes no generation/cancel/registry/CFG/publication state;
- run launch returns one engine/generation/CFG revision;
- collection/expansion stale success and error paths emit nothing;
- failed and discarded candidates leave no profiling metadata, file, ring, or observer state.

### Phase 0B. Durable, versioned diagnostics

Store open editor documents as `{ client_uri, version, text }`; do not discard LSP
versions.

Each project owns a latest-revision diagnostics state machine:

```text
source mutation           → mark latest revision dirty; schedule trailing attempt
Ready(current revision)   → conditionally publish; clear dirty
Ready(stale revision)     → discard; retain latest dirty
Busy                      → publish nothing; retry with bounded backoff
Poisoned                  → publish nothing; surface internal failure
```

“Clear dirty” is compare-and-clear: it succeeds only when
`dirty_revision == result.source_revision`. If a newer mutation wins between computation and
publication, retain/re-arm the newest dirty revision. The final publication uses I9's conditional
outbound transaction rather than a separate check followed by send.

The trailing attempt is independent of engine success, `$init`, test collection,
playground demand, and further typing. A final invalid edit must eventually publish.

For open files, `publishDiagnostics.version` is the exact checked document version.
Closed/disk-only files use `None`. A busy full refresh does not update
`last_published_files` and does not clear deleted-file state.

Phase 0 may reuse the existing background scheduler for the rare busy retry, but the retry is
tagged with the source revision, retries until success or supersession, and always runs even
when engine construction returns invalid source or another terminal error. B3 later moves this
work onto the shared ingress tail.

**Phase 0B merge gate**

- hold the DB through the final syntax-error edit, release it, send no further edit, and observe
  the latest versioned diagnostics;
- complete v7 diagnostics after v8 arrives and observe no v7 publication;
- mutate to v8 between v7's ready check and dirty clear; v8 remains scheduled and publishes;
- deterministically distinguish ready, busy, and poison;
- engine or `$init` failure cannot suppress diagnostics.

### Phase 0C. Bounded request wait

Keep the native 1s deadline, 2ms retry interval, and 50ms wait-log threshold as disposable
compatibility scaffolding:

- request handlers use `try_lock_db_wait_for_request`;
- workspace-wide loops use `try_lock_db_nowait` and skip only `WouldBlock`;
- poison maps once to `InternalError`, transitions the project to `ProjectBroken`, and schedules
  controlled restart; it is never treated as recoverable contention;
- timeout maps to `ContentModified` only when the captured source revision actually
  changed; otherwise it maps to `RequestFailed`;
- WASM retains the non-waiting single-thread path;
- formatting and other requests receive typed failures, never silent success/no-op;
- `$/cancelRequest` is accepted without the current error log, though real cancellation
  arrives with B2.

Change background bytecode acquisition from silent `try_lock` failure to the owned
candidate flow in Phase 0A. Blocking acquisition alone is not a publication fence.

Expected containment effect: ordinary 156-302ms holds fall below the deadline, eliminating the
observed `-32001` burst while preserving a finite escape for pathological multi-second
holds. It does not fix per-keystroke diagnostics cost or request spam backlog.

### Phase 0D. Compatibility verification

- rerun harness exp3/exp5: zero `-32001`, hints succeed after pauses;
- rerun exp6: no cancel error log;
- assert same-revision timeout is `-32803`, actual revision invalidation is
  `-32801`, explicit cancellation is `-32800`, and poison is `-32603`
  followed by terminal restart in unwind-enabled test builds;
- record longest wait and rebuild phase timings in the reporting workspace;
- run the out-of-order engine, final invalid edit, coherent run, collection ABA, and document
  version tests above, including invalidation at the final outbound-enqueue barrier without an
  engine-generation change.

## 4. Phase 1 — bounded latency and ownership

Tracks A and C can begin in parallel after their decision gates. Track B remains ordered.

**Abort-profile invariant.** The shipped CLI/LSP remains `panic = "abort"`. Therefore no
`ProjectDatabase::clone()` may outlive the source/database gate, no shipped request path may
invoke Salsa local cancellation or `Cancelled::catch`, and no shared Salsa storage clone may
race a write. Live-database work stays on one mutex-serialized lane. Background work may run
concurrently only after it has produced fully owned, Salsa-detached inputs. Explicit request
cancellation may claim/suppress the response but may not unwind an in-flight Salsa query.

### Track A. One client ownership domain per document

Distinguish:

- **ownership root:** a non-overlapping LanguageClient process/event boundary;
- **semantic project root:** the closest ancestor marked by `baml.toml` or
  `baml_src/`, used to select a `LiveProject`.

Every eligible URI matches exactly one ownership root. Ownership is independent of document
open order. Nested semantic projects may share one non-overlapping owner if Decision D1 selects
the recommended topology.

The extension must expose two distinct resolvers:

```text
resolveSemanticProjectRoot(uri) = nearest ancestor containing baml.toml or baml_src/
resolveOwnershipRoot(uri)       = outermost marked ancestor in that same ancestor chain
```

The second rule is the recommended D1 topology: sibling top-level projects retain separate
clients while nested projects share their outer owner. Both resolvers use one canonical path
identity helper: URI decode, absolute/component normalization, resolution of symlinks through the
nearest existing ancestor, and filesystem-aware case normalization. Preserve the client URI
separately for publication. The walk ends at the filesystem/volume root; if it finds no marker,
the file has no owner under the current standalone-file contract.

Overlapping VS Code workspace folders do not create competing owners; the canonical marker chain
does. Adding or removing a marker invalidates both resolutions. One extension-level ownership
coordinator serializes migration: detach routing from the old owner, stop it if unused, then attach
or start the new owner. No document or command may be routed to two clients during migration.

#### A1. Complete VS Code ownership scoping

For each ownership root:

- construct a synthetic `vscode.WorkspaceFolder`;
- set `LanguageClientOptions.workspaceFolder`;
- use one `vscode.RelativePattern(ownerFolder, '**/*.baml')` for both
  `documentSelector` and `createFileSystemWatcher`;
- never use an interpolated absolute glob.

Marker topology is not owned by individual language clients. The extension-level ownership
coordinator installs ref-counted exact ancestor-directory watchers for `baml.toml` and
`baml_src` along each active document's canonical chain (plus one scoped topology watcher per
VS Code workspace). Marker create/delete/rename events recompute and serialize the migration
described above; they do not broadcast the same file event through every client.

Setting only selector and watcher is insufficient: omitted `workspaceFolder` makes
vscode-languageclient populate initialize with every VS Code workspace folder and register
global workspace-folder handling.

Key the client map by `resolveOwnershipRoot`, not by the nearest semantic root. Update
`startForUri`, `activeClient`, `projectKeyForPath`, restart/log commands, and playground
command routing to use that owner key. Semantic project lookup remains server-side and nearest
marker wins.

Opening a file outside VS Code workspaces does not attach it to an arbitrary existing client.
If it belongs to a marked BAML project, activation creates one scoped owner for that project.
Unmarked arbitrary standalone files retain today's unsupported server behavior; adding general
standalone support is a separate feature.

#### A2. Enforce ownership on the server

Initialization roots are authoritative boundaries:

- discover semantic projects only inside the declared ownership roots;
- route nested files to the nearest semantic project;
- validate every URI-bearing request, notification, workspace event, and playground command
  before project lookup or filesystem access, using component-aware canonical containment;
- ignore and log out-of-bound notifications; return typed `RequestFailed` for out-of-bound
  requests or commands;
- preserve multi-root discovery only for browser/CLI callers that explicitly own multiple
  disjoint roots;
- do not broaden ownership when a project is added or removed.

A1 already removes the global startup matrix by narrowing initialize roots. A2 is defense in
depth and covers non-VS-Code entry points; it is not a separate claim that only server work
fixes startup.

**Ownership gate**

- nested inner-first and outer-first opens produce the same topology;
- each change, hover, hint, and watcher event reaches one dispatcher;
- N semantic projects refresh N times total, not N²;
- Windows separators and paths with spaces work;
- symlink and case aliases resolve to one owner while publications retain the client URI;
- overlapping workspace folders and marker add/remove migrations never create two owners;
- restart, log, and playground commands route to the same owner as document requests;
- an external marked project gets one dedicated owner;
- an external unmarked file neither broadens a workspace owner nor attaches to all clients.

### Track B. Server latency work

#### B1. Memoize annotations and semantic tokens

Make `annotations()` and `semantic_tokens()`
`#[salsa::tracked(returns(ref))]`, with required `Eq`/`salsa::Update`
derives and handler changes. The existing `file_outline` query is the precedent.

Gate:

- first call remains roughly the current cold cost;
- repeated unchanged calls are <2ms;
- the request-spam harness leaves approximately no unanswered requests;
- measure RSS for small, 400-function, 1500-function, and many-file projects.

#### B2. Minimal shared bounded ingress

Stdio and `/api/lsp` use the same process-scoped ingress/scheduler implementation.
Transport adapters only parse/frame and route output.

Each connection constructs the I7 `LspSession` and follows:

```text
PreInitialize → InitializeResponding → InitializeResponded → Initialized
    → ShutdownResponding → Shutdown → Exited
```

Only one `initialize` request is accepted. The state advances to
`InitializeResponded` only after that response is ordered onto the owning session's
outbound stream; only then is `initialized` accepted. Duplicate `initialize` receives
`InvalidRequest`; `initialized` in any other state is ignored and logged. Before initialize,
other requests receive `ServerNotInitialized` and notifications are dropped except
`exit`. Between the initialize response and valid `initialized`, other requests likewise
receive `ServerNotInitialized` and notifications are dropped.

Only one `shutdown` request is accepted in `Initialized`; the state becomes
`Shutdown` only after its response is ordered. Afterward, requests receive
`InvalidRequest`, notifications are ignored except `exit`, and `exit` transitions to
`Exited`. `exit` before a completed shutdown terminates that session abnormally (nonzero for
stdio); after shutdown it is normal. Remove transport-local shutdown/exit shortcuts so both
transports use this state machine. Explicit CLI roots may be recorded before the handshake but
may not trigger position-bearing LSP publication.

Every admitted non-cancel message receives a sequence number. Phase 1 starts them in FIFO order:
neither a later request nor a later mutation crosses any earlier admitted message. Lifecycle,
source mutation, workspace mutation, and side-effecting requests remain barriers for any future
concurrency proposal.

The first B2 delivery contains only:

1. one stdio/browser ingress policy;
2. lifecycle and mutation barriers;
3. exactly-once explicit cancellation;
4. adjacent same-URI FULL-sync `didChange` coalescing;
5. bounded admission with the Decision D3 overload policy.

`$/cancelRequest` and transport-close control use a separate bounded control path and do
not wait behind normal work. Lifecycle/mutation admission has reserved item and byte capacity.
Read/formatting requests have a smaller explicit budget; overload rejects the newly arriving
request and never evicts an already admitted one. If only non-droppable mutation/command work
remains and its reserve is full, the transport stops reading to apply backpressure. Adjacent FULL
coalescing replaces the old body and releases its byte charge.

Adjacent coalescing retains the newest full text and document version and never crosses another
URI, request, open, close, save, watched-file event, workspace event, or source mutation.

Outstanding requests are keyed by `(session_id, request_id)` and have one response
owner:

```text
Queued → Running → Responded
   └──────────────→ Responded(RequestCanceled)
```

Cancellable work receives an operation-owned token. Cancellation and the normal result race to
claim the one response slot: queued cancellation removes the work and responds; running
cancellation claims the response and signals the token; a later handler result is discarded.
Repeated/unknown cancellation is idempotent. Under the abort-profile invariant, the token is
checked only at safe handler boundaries; it never triggers Salsa's unwind-based local
cancellation inside a live query.
For a running side-effecting request, cancellation owns only the response; it does not imply that
already-applied effects were rolled back. Each side-effecting method must define whether it is
cancelable before its commit point or non-cancelable once running. After a non-cancelable commit
point, a cancel notification is a no-op and the normal result retains response ownership.

Do not include queued-change `ContentModified`, interactive priority, arbitrary
latest-wins request cancellation, or request dedupe in B2. Add them only after measurement and
only if they preserve barriers and protocol semantics.

The browser deletes its per-WebSocket unbounded dispatcher. Responses route only to their owning
session; a lossy global broadcast is not a response transport. Inbound and outbound queues both
have item and serialized-byte budgets. Each admitted request reserves one outbound response slot;
responses are never dropped. Revisioned notifications use reserved/coalescing project mailboxes.
A browser session that cannot drain by a configured write deadline is deterministically closed;
stdio request admission is coupled to writer capacity. No transport may hide an unbounded writer
queue.

**B2 gate**

- identical recorded traces through stdio/browser produce identical ordering/outcomes;
- queued/running/repeated cancel races produce exactly one response;
- only adjacent same-URI FULL changes coalesce;
- final source equals the newest admitted change;
- inbound and outbound memory remain bounded under request, mutation, and slow-reader spam;
- lifecycle/control remains admissible under saturation;
- malformed browser/stdio JSON receives null-ID `ParseError` or `InvalidRequest` as
  applicable;
- duplicate initialize, wrong-state initialized, shutdown, post-shutdown request, and early/late
  exit traces are identical through both transports;
- browser takeover during a request or edit closes old overlays, rejects late old-session output,
  tolerates reused request IDs, and negotiates fresh capabilities;
- an unprocessed queued later edit alone never turns an earlier request into
  `ContentModified`.

#### B3. Debounced diagnostics plus the minimum B4 tail move

After source apply/version bookkeeping, `didChange` only:

- advances source/diagnostics revision;
- marks runtime non-current;
- resets the engine timer;
- resets a 150ms diagnostics/project-update timer.

The timer never computes work itself. It admits at most one reserved internal
`RunProjectTail { project, epoch, source_revision }` item. A newer epoch replaces the
older item; normal ingress saturation cannot drop it.

At dequeue:

1. discard stale epoch/revision;
2. compute diagnostics;
3. on `Busy`, retain and re-arm with bounded backoff;
4. on poison/panic, surface an internal failure and stop retrying broken state;
5. conditionally enqueue versioned diagnostics through I9's atomic publication transaction;
6. if runtime subscribers exist and this project is demanded, build one owned complete
   `ProjectUpdate` candidate;
7. conditionally enqueue that candidate through I9; stale or removed project incarnations emit
   nothing.

This minimum payload move is part of B3. Without it, `didChange` is not apply-only.

#### B4. Separate editor catalog from runtime snapshots

**Catalog — `ListProjects`.** It feeds the VS Code extension and runtime clients.
Never receiver-gate it. Sort and change-dedupe routine sends; force a catalog after
initialization and every explicit `requestState`. `requestState` does not by itself
create runtime demand for every catalog entry.

**Runtime detail — `UpdateProject`.** Rename the trait signal to
`has_runtime_subscribers`. Check it before flattening playground diagnostics, listing
functions/types, serializing, or locking the project database. With zero subscribers, skip all
runtime candidate work while editor diagnostics and catalog continue.

That global bit answers only whether delivery is possible. B5's per-project demand lease is a
second required predicate for project-specific runtime construction.

When subscribers exist, compare the complete owned payload: runtime state/currentness, functions,
origins, capabilities, client names, params, types, and diagnostics. A hash may accelerate but
cannot replace equality. New subscribers and explicit state requests force full snapshots and
bypass global dedupe for currently demanded projects.

`request_playground_state` clones `(root, Arc<LiveProject>)` under the registry
lock, drops it, builds owned candidates with short project reads, drops all guards, then sends.

Install a WebSocket subscription before requesting the forced catalog/snapshots so the response
cannot race the listener. Catalog removal evicts the server's cached payload and increments a
project incarnation; the frontend purges all payload, test, runtime, and selection state for that
incarnation. Remove→re-add may never be suppressed by a warm dedupe cache.

#### B5. Demand-gated runtime with asynchronous first demand

`has_runtime_subscribers` is only a transport/payload-delivery gate. Runtime work is demanded per
project. Each browser session owns at most one selected-project lease, changed through an explicit
`EnsureProjectRuntime { project_id, incarnation }` message (and release on selection change or
disconnect). Run and test requests independently acquire demand for their target project.

With zero demand leases for a project, its edits still update source, editor diagnostics, and
catalog, but do not run bytecode generation, `$init`, engine installation, test collection,
or `UpdateProject` construction. A global `requestState` alone never warms every project.

Under recommended Decision D5, first playground open chooses the active-editor/previously selected
project after catalog delivery and demands only that project; selection changes move the lease.
The warm-all alternative remains an explicit product choice.

`ensure_engine_current(project, requested_revision)` is asynchronous, single-flight,
and revision-conditional:

- current revision returns immediately;
- concurrent callers join one build;
- newer source supersedes the old candidate and waiters follow the new revision;
- invalid source returns `BlockedByDiagnostics`;
- engine/`$init` failure returns a typed terminal state;
- no projects-registry lock is held during compile or await.

Under the recommended Decision D6 behavior, first subscriber state streams immediately, exposes
`preparing current build`, and warms in the background. Run/test await that same
latest-revision flight before capturing runtime identity.

Replace the ambiguous boolean with a backward-compatible runtime state carrying at least:

- `idleStale`;
- `building`;
- `ready`;
- `blockedByDiagnostics`;
- `failed`;
- requested and installed revisions;
- whether a last-known-good engine exists.

Retain `isBexCurrent` only as a derived compatibility field.

Once a demanded build begins, a transient subscriber disconnect does not cancel it. Future
zero-demand edits remain gated. Source supersession—not receiver churn—owns cancellation.

Terminal build failure is keyed by source revision plus a revision of enumerated runtime inputs
(configuration/environment/toolchain inputs used by `$init`). A relevant input change permits
one new single-flight attempt. Otherwise the error remains visible and an explicit
`Retry build` action advances an attempt nonce; repeated state requests never form a retry
loop. Invalid source remains `BlockedByDiagnostics` and retries only after source change.

## 5. Track C — complete position correctness

### C1. Negotiated LSP position boundary

Initialize selects UTF-8 when offered; otherwise UTF-16, and explicitly advertises the selected
`ServerCapabilities.position_encoding`. UTF-32 is not negotiated unless Decision D4
requires it.

Negotiated state belongs to the I7 `LspSession`, not the shared project/workspace.
Use a fresh `Arc<OnceLock<SessionConfig>>` or equivalent across only the dispatcher clones
bound to that connection. Reading before initialize is an error; never
`get_or_init(UTF16)`. A replacement browser connection gets a new session/config even
though it reuses the shared project registry. Project seeding may happen early, but outbound LSP
payloads wait until initialize/initialized.

Create one `LspPositionCodec` at the LSP boundary. Compiler APIs remain byte-based.
The codec owns text plus line starts/ends and converts:

- LSP position/range ↔ byte offset/range;
- byte span → LSP range;
- same-line byte span → encoded semantic-token length;
- document end → formatting range.

It recognizes LF, CRLF, and bare CR. Overlong characters clamp to that line's end; nonexistent
lines, malformed ranges, and positions inside an encoding unit return `InvalidParams`.

Audit all current paths:

- incoming completion, hover, definition, references, inlay ranges, and code-action ranges;
- outgoing primary/related diagnostics, CodeLens, inlay hints, definition/reference locations,
  workspace/document symbols, formatting edits, and semantic tokens.

No LSP handler may directly construct a nontrivial position/range or call the current raw-byte
`lsp_position_to_offset`/`offset_to_lsp_position` helpers.

Semantic tokens encode both `deltaStart` and `length` in the negotiated
encoding. Always split multiline spans into same-line segments; this is valid for all clients
and required for VS Code's no-multiline capability. Discard zero-length segments, exclude newline
units, keep sorted order, and assert unsupported overlaps are absent.

### C2. Fixed playground editor coordinates

The playground protocol does not inherit negotiated LSP encoding:

- wire `line/column/endLine/endColumn` are zero-based UTF-16 code units, matching
  VS Code positions; the Monaco adapter converts to/from Monaco's one-based `IPosition`;
- `startOffset/endOffset/cursorOffset` are UTF-8 byte offsets;
- incoming cursor coordinates convert fixed UTF-16 → byte offset;
- outgoing graph/source spans convert byte range → fixed UTF-16;
- LSP UTF-8 negotiation cannot change playground coordinates.

Advertise a playground protocol capability such as
`sourcePositionEncoding: "utf16-zero-based-v1"`. The frontend uses the new adapter only when
advertised; otherwise it preserves legacy conversion. This capability may be waived only if the
release process proves all server/frontend consumers update atomically. Remove the current graph
navigation subtraction only behind that coordinated gate.

**C1/C2 gate**

- ASCII, accent, CJK, and emoji fixtures cover every handler under UTF-8 and UTF-16;
- LF, CRLF, and bare CR are covered;
- semantic-token starts/lengths around emoji decode correctly and multiline spans split;
- formatting uses encoded document end;
- pre-initialize access errors without initializing or freezing negotiation;
- a clone created before initialize observes the later shared config;
- browser session replacement starts with fresh, uninitialized capabilities;
- playground cursor and graph navigation remain UTF-16 under either LSP encoding;
- old/new playground protocol pairings preserve navigation or negotiate the new wire contract.

## 6. Deferred work — shared Salsa snapshots and request workers

This is not part of the v4 implementation plan.

`ProjectDatabase::clone()` shares Salsa storage while cloning the database's metadata maps.
Salsa cancels a cloned reader for a pending write with `panic::resume_unwind`. The shipped
CLI/LSP `release` and `dist` profiles use `panic = "abort"`, so one edit racing a cloned
reader can abort the entire server; `Cancelled::catch`, Tokio task isolation, or worker
replacement cannot contain it.

Consequently, v4 does not ship:

- a shared-storage database snapshot that outlives the source gate;
- same-project parallel Salsa queries;
- cancel-on-write or Salsa local request cancellation;
- a database request worker pool.

A pool operating only on fully owned, Salsa-detached inputs remains allowed, but it does not
provide the deferred same-project query parallelism. Blocking writes until all snapshots finish
is also rejected: it restores edit stalls and one missed gate becomes a release abort.

Reconsider this only in a separate proposal after Phase 1 measurements show a material remaining
latency problem. That proposal must first quantify the user-visible value and target SLO, then
choose an isolation model—an unwind-enabled artifact, a subprocess, or a different database
architecture—before specifying workers. Snapshot clone-cost experiments may continue offline
under an unwind build, but they are not evidence that the release artifact is shippable.

## 7. Product and behavior decision record

D1-D8 adopt the recommended behavior. There are no remaining product choices in the active plan.
Speculative `$init` is resolved as an engineering fact, and unwind-dependent snapshot
parallelism is deferred rather than selected.

### D1. Nested project process/toolchain isolation

One parent `RelativePattern('**/*.baml')` also matches nested projects. Static parent
and child clients therefore cannot both be exact owners.

- **Recommended:** one non-overlapping outer owner process multiplexes nested semantic projects;
  the server routes each file to its nearest project marker.
- **Alternative:** each nested project gets a separate process/toolchain. This requires a central
  exact-owner router or dynamic exclusion/restart design in the extension.

**Decision D1:** use one non-overlapping outer owner process and route nested files to their
nearest semantic project.

### D2. Browser LSP sessions

Today multiple `/api/lsp` sockets share one Bex state and globally broadcast output.
Request IDs, document versions, and negotiated capabilities can collide.

- **Recommended:** one active browser LSP session per process; the newest connection atomically
  supersedes and closes the prior one, which handles reload cleanly.
- **Alternative:** true concurrent editable tabs, requiring per-session response routing,
  capabilities, and document-writer ownership/conflict behavior.

Under the recommended policy, takeover is one ingress barrier. It tombstones the old session and
its queued/running response tokens, revokes its sink, invalidates its diagnostic-tail items, and
closes its owned document overlays as one source mutation before the replacement initializes.
Late old-session work is rejected by session epoch and source revision even when request IDs are
reused. The replacement negotiates fresh capabilities and then reopens current buffers through
normal `didOpen` messages; no old overlay is silently transferred.

**Decision D2:** support one active browser LSP session; the newest connection supersedes the
previous session.

### D3. Ingress overload behavior

Mutations cannot be dropped. A bounded request queue must choose between visible feature-request
failure and a stale multi-second backlog.

- **Recommended:** preserve lifecycle, mutations, and side-effecting commands; reject excess
  read/formatting requests promptly with `RequestFailed`; backpressure only when the
  queue contains nothing safely rejectable.
- **Alternative:** protect selected explicit formatting/command requests with reserved admission,
  accepting more backpressure.

**Decision D3:** under extreme overload, fail newly arriving hover/hint/formatting requests;
never evict admitted work, and preserve mutation/lifecycle/command capacity.

### D4. UTF-32 negotiation

UTF-16 is mandatory in LSP and UTF-8 is BAML's internal fast path. UTF-32 adds another complete
handler and token test matrix without a known client requirement.

- **Recommended:** negotiate UTF-8 when offered, otherwise UTF-16; do not select UTF-32.

**Decision D4:** support UTF-8 when offered and UTF-16 otherwise; do not add UTF-32.

### D5. First runtime-demand breadth

The current global `requestState` walks every project. Treating one WebSocket subscriber as
runtime demand would compile and run `$init` for all projects on first playground open—44
engines in the reporting workspace.

- **Recommended:** catalog all projects, but warm only the active-editor or explicitly selected
  project. Moving selection moves a per-session lease; Run/Test demand their own target.
- **Alternative:** eagerly warm every project so switching is instant, accepting the startup CPU,
  memory, and `$init` cost.

**Decision D5:** first playground open warms only the active or selected project.

### D6. First playground presentation

After editor-only demand gating, first open may not have a current engine.

- **Recommended:** stream catalog/source/diagnostics immediately, show “Preparing current
  build…”, warm asynchronously, and disable runtime-dependent controls until ready.
- **Alternative:** block the playground presentation until compile/`$init` completes.

**Decision D6:** render immediately with a preparation state and disable runtime-dependent
controls until the selected project is ready.

### D7. Run/test against stale source

When source is newer than the last successful engine:

- **Recommended:** wait for the latest single-flight build; if current source is invalid, fail
  explicitly. Never silently run last-known-good.
- **Alternative:** expose an explicit, prominently labeled `Run last successful build`
  action. It is never selected implicitly.

For invalid source, the recommended test-tree UX retains the prior tree, marks it stale, and
disables launches. Collection failure retains the tree and shows an error; an empty tree means
zero tests, not failure.

**Decision D7:** normal Run/Test always uses current source; do not add a stale-run action in this
plan.

### D8. Runs already active when source changes

Here “collection” means rebuilding and serializing the test-discovery tree for an engine;
“expansion” means enumerating child cases for a test set in that tree. Neither operation executes
a user test. They are stale tree-maintenance work once source changes, so their old results must
be canceled or discarded.

- **Recommended:** allow an already-started run to continue on its pinned engine/CFG revision for
  reproducibility. Cancel only stale test-tree discovery/serialization/expansion. Keep a
  run-scoped `{ engine, graph, cancellation }` handle until terminal completion so explicit
  cancel still targets the old function or test run after a new engine commits. Old engines remain
  in memory while pinned runs finish.
- **Alternative:** cancel user runs on every edit. Source mutation claims one terminal
  `canceledBySourceChange` outcome; a concurrent explicit cancel or natural completion loses the
  exactly-once terminal race.

**Decision D8:** already-started function and test runs continue on their pinned engine. New runs
follow D7 and require current source. Each active run retains its own engine/cancellation handle so
explicit Cancel continues to work; the old engine remains in memory only until its pinned runs
reach terminal completion. Run events/results are ordered by run identity and remain publishable
after a project-revision change; the stale-project watermark applies only to project-derived
state.

### Resolved engineering fact — discarded-candidate `$init`

`$init` is synchronous, deterministic candidate initialization. It populates the candidate's
owned globals; notifications are ignored, events are dropped, and an async/sys-op yield fails
initialization. A stale candidate may therefore finish `$init` and be discarded normally.

The separate issue is constructor profiling bookkeeping: `BexEngine::new` currently registers
metadata before `$init`, while cleanup normally occurs from `BexEngine::drop`. If
`$init` returns an error, no engine reaches `Drop`; if a successfully constructed candidate
is superseded, normal `Drop` emits observer-close bookkeeping and leaves a closed-engine
tombstone. Phase 0 therefore keeps profiling inactive until conditional commit succeeds. Failed
or superseded candidates never register and drop quietly. These bookkeeping issues do not make
`$init` externally observable.

### Deferred rather than decided — unwind and snapshots

No unwind artifact or packaging decision is part of v4. Section 6 defers the Salsa snapshot and
worker design that required it. Revisit only if post-Phase-1 measurements establish enough value
for a separate proposal.

## 8. Acceptance matrix

### Engine/runtime

- old/new candidates finish in both orders; only the newest revision installs;
- edit wins exactly around commit without a false-current state;
- final invalid edit publishes diagnostics without an engine commit and never permits an older
  candidate to install;
- run registration returns same-revision engine/generation/CFG and, for tests, registry;
- an edit winning before registration forces the new run through D7; registration winning first
  creates a D8 pinned lease;
- collection ABA and stale expansion results emit nothing;
- source mutation immediately invalidates derived work while preserving the D8 user-run policy;
- outbound/backend and frontend reject deliberately injected older-revision project payloads even
  when engine generation is unchanged, while pinned run streams continue by run identity;
- after edit and new-engine commit, explicit Cancel and terminal completion still target old pinned
  function and test runs exactly once;
- `$init` failure and discarded candidates leave no profiling metadata or resources.

### Diagnostics

- final invalid edit eventually publishes without another keystroke;
- publications carry document versions captured atomically with source revision/text;
- busy preserves prior diagnostics and retries; poison is explicit;
- mutation between ready-check and compare-and-clear cannot lose the newer dirty revision;
- saturated ingress cannot lose the one pending project-tail item;
- projects debounce/retry independently.

### Ownership/catalog/runtime delivery

- every URI/event has exactly one owner independent of open order;
- ownership and semantic resolvers produce stable outer/nearest roots, including commands,
  symlink/case aliases, overlapping folders, and marker migration;
- initialize contains only owner roots;
- startup work is linear;
- `ListProjects` reaches the extension with zero runtime subscribers;
- zero-demand project edits perform no `UpdateProject` construction or engine work;
- a global state request never warms undemanded projects; selection/run/test demand only their
  specified project;
- a new subscriber receives a forced full snapshot for demanded projects even with warm dedupe
  caches;
- remove→re-add purges server/frontend caches and sends the new project incarnation;
- failed same-revision builds do not retry-loop; runtime-input change or explicit Retry starts one
  joined attempt;
- changing any complete payload field emits an update;
- no registry/DB/cache lock is held while sending.

### Ingress/protocol

- no path emits `-32001`;
- every typed condition maps to the table in I7;
- response tokens route reused request IDs to the owning session and cancellation races emit one
  response;
- stdio/browser traces are equivalent;
- inbound/outbound queue items and bytes are bounded; mutations retain FIFO order and slow readers
  cannot create an unbounded writer queue;
- an unprocessed queued edit alone never causes server-originated `ContentModified`;
- initialize/shutdown responses precede their state transitions and all lifecycle error traces are
  transport-identical;
- browser takeover revokes old overlays, sinks, requests, tails, and capabilities.

### Encoding

- ASCII, accent, CJK, and emoji round-trip under UTF-8/UTF-16;
- LF/CRLF/bare-CR line ends are correct;
- every current handler has integration coverage;
- semantic-token starts and lengths decode correctly and multiline spans split;
- pre-initialize access errors and cannot freeze negotiation;
- playground coordinates remain fixed UTF-16 under either LSP encoding;
- Monaco's one-based adapter and old/new playground protocol pairings pass the capability gate.

### Abort-profile boundary

- release-path tests assert no shared `ProjectDatabase` clone outlives the source gate;
- shipped request handlers do not invoke Salsa local cancellation or
  `Cancelled::catch`;
- canceling a running request claims its response and discards its later result without unwinding
  the in-flight Salsa query;
- background tasks accept only fully owned, Salsa-detached inputs.

## 9. Rejected approaches

- **Generic `ContentModified` for busy or queued edits:** wrong semantics and often
  silently drops editor features.
- **Plain blocking DB lock everywhere:** an open playground/CFG build can freeze all dispatch.
- **Diagnostics `Option` plus “heal next keystroke”:** loses the final invalid edit and
  conflates poison.
- **Debounce before a durable internal queue:** risks dropped tail work or background DB holds
  racing source mutation.
- **Static parent and child client selectors:** parent globs include nested roots.
- **Receiver-gating `ListProjects`:** breaks the VS Code catalog.
- **Partial `ProjectUpdate` hashes:** suppress valid function/schema/type changes.
- **A global negotiated encoding read with `get_or_init` fallback:** early refresh can
  permanently choose the wrong encoding; clones can diverge.
- **Incremental sync before C1:** raw-byte incoming positions would corrupt edits.
- **Snapshot retained through `$init` or commit:** blocks writers and violates owned
  candidate boundaries.
- **Per-request `spawn_blocking` snapshots:** unbounded clones make writer cancellation
  scale with request spam.
- **Shared Salsa snapshots under `panic=abort`:** a pending write unwinds and therefore
  aborts the shipped LSP process.
- **Clearing a poisoned mutation lock and continuing:** partially updated state is not repaired.
- **Priority/dedup before barriers and measurements:** risks reading pre-mutation state and
  inventing cancellation semantics.

## 10. Remaining measurement and engineering questions

These do not require a UX decision before their spike or instrumentation:

1. Actual DB-hold distribution in the reporting multi-project workspace.
2. Exact 1500-function split between diagnostics, emit, and `$init`.
3. Runtime-subscriber duty cycle while editing.
4. Non-VS-Code client behavior for typed errors.
5. Symlink URI identity (`/tmp` vs `/private/tmp`).
6. Queue budgets derived from maximum mutation admission-to-apply latency.

Every measurement gate records the command, fixture, raw output, build profile, and commit SHA.
