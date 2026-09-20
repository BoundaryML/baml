# Durable POC: component contracts

This file is the API agreement between the four components of the durable
functions proof of concept. The design rationale is in
`documents/durable-functions-design.md`. When this file and the design document
disagree, this file wins. A builder who needs to deviate from a contract must
keep the deviation backward compatible and must report it.

Components and owners:

| Id | Component | Location | Isolation |
|---|---|---|---|
| A | Snapshot core | `baml_language/crates/bex_snapshot/` (new), plus the support files listed in section 5 | own git worktree |
| B | Engine hooks and the `worker` subcommand | files listed in section 5 | own git worktree |
| C | Site server (BAML), demo program, mock worker | `baml_language/demo_durable/server/`, `program/`, `scripts/` | main worktree |
| D | React app | `baml_language/demo_durable/web/` | main worktree |

## 1. Common conventions

- All JSON messages are objects with a string field `type`.
- `ts` is Unix time in milliseconds as a JSON number. Every worker event and
  every SSE event carries `ts`.
- A run id is a string that matches `[a-z0-9-]+`, for example `r-7k2m9x`.
- A segment is one worker process in the life of a run. Segment numbers start
  at 1 and increase by 1 at every resume.
- A thread id is a JSON number. It identifies a BAML thread inside one
  segment. The root thread of a segment has `parent_thread: null`.
- Sites are named `local` and `cloud`. Ports are 8787 for `local` and 8788 for
  `cloud`.
- Unknown fields must be ignored by every reader. Unknown event types must be
  passed through by the site server and ignored by the app.

## 2. Worker process contract (B implements, C consumes, mock in C)

### 2.1 Command line

```
<worker-cmd> --project <dir> --run <run-id> --segment <n> --snapshot-dir <dir>
             ( --start <function> --json-args '<json object keyed by parameter name>'
             | --resume <snapshot-path> )
             [--remote-result '<json: {"call_id": "...", "value": <json>} or {"call_id": "...", "error": "..."}>']...
```

- The real worker command is `baml-cli worker`. It is a hidden clap
  subcommand and is not gated by `BAML_INTERNAL`.
- The mock worker command is `node scripts/mock-worker.mjs`. It accepts the
  same arguments.
- `--remote-result` may repeat. It delivers results of remote calls that
  completed while the run had no process. It is only meaningful with
  `--resume`.
- `--project` is a BAML project directory (the directory that contains
  `baml_src/` or the `.baml` files). The worker compiles it at startup.

### 2.2 Output and exit codes

- stdout carries exactly one JSON object per line and nothing else. Output of
  the BAML program (`baml.io.print*`) never reaches stdout directly. It is
  reported as `log` events.
- stderr is free-form diagnostics. The site server forwards stderr lines as
  `log` events with `stream: "worker_stderr"`.
- Exit codes: `0` completed, `1` failed, `75` paused (snapshot written),
  `130` cancelled. Any other exit, including death by signal, means the
  process was lost.

### 2.3 Events (worker stdout)

Every event has `v: 1`, `type`, `ts`, `run`, `segment`, and `pid`.

| type | Additional fields |
|---|---|
| `hello` | `mode: "start" \| "resume"`, `function: string`, `durable: bool` |
| `log` | `stream: "stdout" \| "stderr"`, `text: string` (one line, no trailing newline), `thread: number \| null` |
| `position` | `thread`, `function: string`, `file: string` (path relative to the project directory when possible), `line: number` (1-based), `reason: "sysop" \| "await" \| "early_yield" \| "remote_call"`, `op: string \| null` (sys-op path such as `baml.sys.sleep`) |
| `thread_started` | `thread`, `parent_thread: number \| null` |
| `thread_ended` | `thread` |
| `remote_call` | `call_id: string`, `thread`, `function: string`, `args: object` (JSON keyed by parameter name) |
| `remote_result_received` | `call_id`, `thread` |
| `pausing` | `waiting_on: string[]` (descriptions of operations in flight) |
| `blocked` | `reason: string`, `path: string[]` (root-to-value path, outermost first) |
| `paused` | `snapshot_path: string`, `state_path: string`, `stats: PauseStats` |
| `resumed` | `stats: ResumeStats` |
| `completed` | `value: json` |
| `failed` | `error: string`, `stack: {function, file, line}[]` |

`position` is emitted when the `(file, line)` of a thread's top user frame
differs from the last `position` emitted for that thread. It is not emitted
for frames inside the stdlib (`<builtin>/...`).

`call_id` is unique within a run. Format: `<run-id>-c<counter>`. The counter
is part of the run state and survives a resume.

```
PauseStats  = { pause_latency_ms, walk_ms, encode_ms, compress_ms, write_ms,
                objects, raw_bytes, compressed_bytes, program_bytes, blocked_attempts }
ResumeStats = { process_start_ms, program_load_ms, decode_ms, first_exec_ms }
```

All stats fields are numbers. A field that a component cannot measure is `null`.

### 2.4 Commands (worker stdin)

One JSON object per line.

| type | Fields | Behavior |
|---|---|---|
| `pause` | none | Write a snapshot at the next clean point, emit `paused`, exit 75. Emits `pausing` or `blocked` while it cannot. |
| `cancel` | none | Cancel the run and exit 130. |
| `remote_result` | `call_id`, and either `value: json` or `error: string` | Resume the thread that waits on `call_id`. |

Until the snapshot core is integrated, the real worker answers `pause` with a
`blocked` event whose reason is `"snapshot support not integrated"` and keeps
running. The mock worker implements `pause` and `--resume` with a fake
snapshot file so that C and D can be developed against the full protocol.

### 2.5 State dump file (`state_path`)

A JSON file written next to the snapshot.

```
StateDump = {
  run, segment, created_ts,
  threads: [{
    thread, name, parked: { kind: string, detail: string },
    frames: [{ function, file, line, locals: [{ name, type: string|null, value: DumpValue }] }]
  }],
  heap: { objects: number, bytes: number, by_kind: { [kind: string]: { count, bytes } } }
}
DumpValue = { kind: "null"|"bool"|"int"|"float"|"string"|"bigint"|"array"|"map"|"instance"|"closure"|"opaque"|"omitted",
              preview: string,             // short single-line rendering
              class?: string,              // for instance
              children?: [{ key: string, value: DumpValue }] }   // bounded depth and count
```

Frames are ordered innermost first. `omitted` marks a cycle or a depth limit.

## 3. Site server contract (C implements, D consumes)

### 3.1 Configuration (environment variables)

| Variable | Meaning | Default |
|---|---|---|
| `SITE` | `local` or `cloud` | `local` |
| `PORT` | listen port | `8787` |
| `PEER_URL` | base URL of the other site server | `http://127.0.0.1:8788` for `local`, `http://127.0.0.1:8787` for `cloud` |
| `WORKER_CMD` | JSON array: the worker command prefix | `["node","scripts/mock-worker.mjs"]` |
| `PROGRAM_DIR` | BAML project that workers run | `../program` |
| `RUNS_DIR` | run store root | `.baml/runs` |

### 3.2 Run record

```
Run = {
  id, site, function, args: object, durable: bool,
  status: "starting"|"running"|"pausing"|"paused"|"completed"|"failed"|"lost"|"cancelled"|"migrated",
  parent: { site, run, call_id } | null,          // set for a remote child run
  origin: { site, run } | null,                   // set for a run imported by migration
  segment: number, pid: number | null,
  position: { thread, function, file, line } | null,
  waiting_on: [{ call_id, child_site, child_run, function }],
  snapshots: [{ n, snapshot_path, state_path, bytes, ts, stats }],
  result: json | null, error: string | null,
  created_ts, updated_ts
}
```

`durable` is true when the function name contains `durable`.

Run store layout: `<RUNS_DIR>/<run-id>/meta.json` (the run record),
`events.jsonl`, `snap-<n>.bamlsnap`, `snap-<n>.json`.

### 3.3 HTTP API

All responses are JSON with `access-control-allow-origin: *`. Errors are
`{ "error": string }` with a 4xx or 5xx status. `OPTIONS` returns 204.

| Request | Response | Notes |
|---|---|---|
| `GET /api/info` | `{ site, peer_url, program_dir, functions: [{name, params:[{name,type}], durable, remote}] }` | `functions` may be `[]` if reflection is impractical. |
| `GET /api/runs` | `Run[]` | newest first |
| `GET /api/runs/:id` | `Run` | |
| `POST /api/runs` `{function, args}` | `Run` | starts segment 1 |
| `POST /api/runs/:id/pause` | `Run` with `status: "pausing"` | sends `pause` to the worker |
| `POST /api/runs/:id/resume` `{site?}` | `Run` | omitted or own site: start the next segment here from the latest snapshot. Other site: send `POST /api/runs/import` to the peer, mark this run `migrated`. |
| `POST /api/runs/:id/kill` | `Run` | `Process.kill`. A durable run with a snapshot becomes `paused`. Any other run becomes `lost`. |
| `POST /api/runs/:id/cancel` | `Run` | sends `cancel` |
| `POST /api/runs/:id/fork` `{n?}` | new `Run` with status `paused` | copies snapshot `n` (default latest) into a new run |
| `GET /api/runs/:id/snapshots/:n/state` | `StateDump` | |
| `GET /api/runs/:id/events` | event[] | contents of `events.jsonl` |
| `GET /api/source?file=<path>` | `{ file, text }` | path relative to `PROGRAM_DIR`. Reject paths that escape it. |
| `GET /api/events` | SSE stream | section 3.4 |
| `POST /api/remote/runs` `{function, args, parent: {site, run, call_id}}` | `Run` | peer to peer. Starts a child run. |
| `POST /api/runs/:id/remote_result` `{call_id, value}` or `{call_id, error}` | `{ok: true}` | peer to peer. Delivered to the worker if it is running. Stored in `meta.json` and passed as `--remote-result` at the next resume otherwise. |
| `POST /api/runs/import` `{run: Run, snapshot_base64, state: StateDump}` | `Run` | peer to peer. Creates the run with the same id and starts the next segment. |

Remote call flow. On a worker `remote_call` event the site server posts
`/api/remote/runs` to `PEER_URL`, records the entry in `waiting_on`, and emits
the `remote_dispatched` event. When a child run that has `parent` set reaches
`completed` or `failed`, its site server posts `remote_result` to the parent's
site.

### 3.4 SSE stream

Each message is `data: <json>\n\n`. A comment line `: keepalive\n\n` is sent
at least every 15 seconds. Every event has `type`, `ts`, and `site`.

| type | Fields | When |
|---|---|---|
| `init` | `runs: Run[]` | first message on every connection |
| `run` | `run: Run` | after every change to a run record |
| `remote_dispatched` | `run`, `call_id`, `child_site`, `child_run`, `function` | the parent's site dispatched a remote call |
| `remote_returned` | `run`, `call_id`, `child_site`, `child_run`, `ok: bool` | the parent's site received the result |
| `migrated_out` | `run`, `to_site` | |
| `migrated_in` | `run`, `from_site` | |
| any worker event | worker fields plus `site` | forwarded verbatim |

### 3.5 Web app (D)

- The app is written in TypeScript: Vite, React, and `strict` mode. All
  protocol types from sections 2 and 3 are declared once in
  `web/src/protocol.ts`. `pnpm build` must pass `tsc` with no errors. Use pnpm.
- Vite dev server on 5173. Proxy `/local/*` to `http://127.0.0.1:8787/*` and
  `/cloud/*` to `http://127.0.0.1:8788/*` with the prefix removed. SSE must
  not be buffered by the proxy.
- The app opens two `EventSource` connections, `/local/api/events` and
  `/cloud/api/events`, and sends commands with `fetch`.
- The app must work against two site servers that use the mock worker.

## 4. Demo program (C owns `demo_durable/program/`)

`demo_durable/program/baml_src/trip.baml` contains at least:

- `durable_plan_trip(city: string) -> TripPlan`: a `while` loop with
  `println` and `sleep`, then a direct call to `remote_fetch_weather`.
- `plan_trip(city: string) -> TripPlan`: the same body without the marker.
- `durable_plan_trip_parallel(city: string) -> TripPlan`: starts
  `remote_fetch_weather` inside `spawn { }`, continues its loop, then awaits
  the result.
- `remote_fetch_weather(city: string) -> string`: `println`, a 3 second
  `sleep`, and a string result.

The program must compile and run to completion under a plain `baml run`
(where the `remote_` prefix has no effect). Use `for` and `while` loops, not
`.map` with callbacks that sleep.

## 5. Rust file ownership

Two builders edit Rust in separate worktrees, and the results are merged.
Ownership keeps the merge mechanical.

**A owns:**

- `crates/bex_snapshot/**` (new crate; add it to the workspace `members` and
  `[workspace.dependencies]` in `baml_language/Cargo.toml`)
- `crates/bex_vm_types/src/heap_ptr.rs` and a new
  `crates/bex_vm_types/src/snapshot_ctx.rs`
- new files `crates/bex_vm/src/snapshot.rs` and
  `crates/bex_heap/src/snapshot.rs`, plus the one-line `mod` declarations for
  them
- `Borsh` impl changes inside `crates/bex_vm_types/src/types/**` when needed

**B owns:**

- `crates/bex_vm/src/vm.rs` (the `VmExecState::RemoteCall` variant, the call
  intercept, visibility changes)
- `crates/bex_engine/src/lib.rs`, `thread.rs`, `conversion.rs`, and a new
  `crates/bex_engine/src/durable.rs`
- `crates/baml_cli/**` (the `worker` subcommand in a new
  `worker_command.rs`)
- exhaustive-match fixes for the new `VmExecState` variant anywhere in the
  workspace

A must not edit `vm.rs` or `bex_engine`. If A needs a private item from
`vm.rs`, A accesses it from `crates/bex_vm/src/snapshot.rs`, which is inside
the crate and can see `pub(crate)` items. If that is not enough, A reports the
needed visibility change instead of making it.

## 6. Snapshot core API (A implements; integration consumes it in the next phase)

The crate `bex_snapshot` depends on `bex_vm_types`, `bex_heap`, and `bex_vm`.
It does not depend on `bex_engine`.

```rust
pub const FORMAT_VERSION: u32 = 1;

pub struct SnapshotHeader {
    pub format_version: u32,
    pub runtime_build: String,      // env!("CARGO_PKG_VERSION") plus git sha when available
    pub program_hash: [u8; 32],     // SHA-256 of the Borsh-encoded Program
    pub run_id: String,
    pub segment: u32,
    pub seq: u64,
    pub created_unix_ms: u64,
}

/// How a thread resumes. The snapshot crate stores it and does not interpret it.
pub struct ParkedAt { pub kind: String, pub payload: Vec<u8> }   // kind "runnable" means: call exec()

pub struct ThreadInput<'a> { pub thread_id: u64, pub parent_thread: Option<u64>, pub name: String,
                             pub vm: &'a BexVm, pub parked: ParkedAt,
                             pub extra_roots: Vec<Value> }        // values held outside the VM at the yield

pub struct WriteOptions { pub header: SnapshotHeader, pub embed_program: Option<Vec<u8>>, pub compress: bool,
                          pub run_state: Vec<u8> }                // opaque engine state, e.g. the call_id counter

pub struct WriteStats { pub walk_ms: f64, pub encode_ms: f64, pub compress_ms: f64,
                        pub objects: u64, pub raw_bytes: u64, pub compressed_bytes: u64 }

pub enum SnapshotError { Blocked { reason: String, path: Vec<String> }, Io(..), Format(String), Mismatch(String) }

/// Caller guarantees that no thread of the run is executing and that the GC cannot move objects.
pub fn write_snapshot(heap: &BexHeap, threads: &[ThreadInput], opts: WriteOptions)
    -> Result<(Vec<u8>, WriteStats), SnapshotError>;

pub fn read_header(bytes: &[u8]) -> Result<SnapshotHeader, SnapshotError>;

pub struct RestoredThread { pub thread_id: u64, pub parent_thread: Option<u64>, pub name: String,
                            pub state: VmThreadState, pub parked: ParkedAt, pub extra_roots: Vec<Value> }

pub struct Restored { pub header: SnapshotHeader, pub threads: Vec<RestoredThread>,
                      pub run_state: Vec<u8>, pub embedded_program: Option<Vec<u8>> }

/// Allocates every snapshot object in `heap` and returns thread states whose values point into `heap`.
/// `program_hash` is checked against the header.
pub fn restore_into(heap: &BexHeap, bytes: &[u8], program_hash: [u8; 32]) -> Result<Restored, SnapshotError>;

/// StateDump JSON from section 2.5, produced from live VMs (no snapshot bytes needed).
pub fn state_dump(heap: &BexHeap, threads: &[ThreadInput], run_id: &str, segment: u32) -> serde_json::Value;

/// StateDump JSON produced from snapshot bytes plus the program (for `baml snapshot inspect`).
pub fn state_dump_from_bytes(bytes: &[u8], program: &Program) -> Result<serde_json::Value, SnapshotError>;
```

In `bex_vm` (file `crates/bex_vm/src/snapshot.rs`):

```rust
/// Pointer-bearing state of one VM: frames, operand stack, exception bookkeeping.
pub struct VmThreadState { /* frames, stack, exception vectors, call id counters */ }

impl BexVm {
    pub fn export_thread_state(&self) -> Result<VmThreadState, String>;   // Err when a Frame::Native is present
    pub fn import_thread_state(&mut self, state: VmThreadState) -> Result<(), String>;
    pub fn snapshot_roots(&self) -> Vec<Value>;
}
```

Compile-time objects are referenced by their index in the program's object
pool and are never copied into a snapshot. A restore requires an engine or VM
that was built from the identical program.

A's acceptance test works at the VM level without `bex_engine`: run a program
to a yield, write a snapshot, build a second heap and VM from the same
program, restore, continue to completion, and compare the result with an
uninterrupted run.

## 7. Accepted extensions (phase 1)

Phase 1 added the items in this section. They are part of the agreement now,
and every component may rely on them. Section 1 still applies: a reader ignores
fields and event types that it does not know. When this section and sections 2
or 3 disagree, this section wins. Items marked "phase 2" were added while the
phase 1 gaps were closed.

### 7.1 Worker events and worker rules (extends sections 2.2 to 2.4)

Two event types join the table in section 2.3. Both carry the common fields
`v`, `type`, `ts`, `run`, `segment`, and `pid`.

| type | Additional fields | Meaning |
|---|---|---|
| `cancelled` | none | The last event of a worker that exits with code 130. The exit code remains authoritative, so a reader must not require the event. |
| `snapshot` | `snapshot_path`, `state_path`, `stats: PauseStats`, `automatic: true` | The worker wrote a snapshot that no `pause` command requested. The fields are those of `paused` plus `automatic`. The process keeps running and does not exit. `stats.pause_latency_ms` is `null`. |

A worker that never writes automatic snapshots never emits `snapshot`. The
mock worker emits one after each loop iteration of a durable function.
`MOCK_AUTO_SNAPSHOT=0` turns that off.

Rules for every worker, real and mock:

- Snapshot files are named `snap-<n>.bamlsnap` and `snap-<n>.json` and are
  written into `--snapshot-dir`. `n` is the highest `n` of the
  `snap-<n>.bamlsnap` files that exist in the directory plus 1, and 1 when
  there is none. The rule covers `paused` and `snapshot`. The site server puts
  the snapshot of a fork or of an imported run into the directory under its
  number before the resume, so the numbering of a run continues across forks
  and migrations.
- One `pause` command may be answered with any number of `pausing` and
  `blocked` events before the `paused` event. The worker retries at later
  yields. No event ends a pause request except `paused`. The end of the
  process ends it too.
- End of input on stdin means `cancel`. The worker emits `cancelled` when
  stdout is still open and exits with code 130. A site server that stops
  therefore leaves no worker process behind.
- A worker ignores a `remote_result` command or a `--remote-result` argument
  whose `call_id` is unknown to it, or for which it already has a result. It
  reports no error and emits no event for it. Only the first delivery of a
  result that a thread waits on produces `remote_result_received`. The site
  server offers stored results again after a recovery from an older snapshot
  and to forks, because it cannot know what a snapshot contains.
- A resumed worker that reaches a remote call again emits `remote_call` with
  the same `call_id`. The site server does not dispatch a `call_id` twice. It
  answers from the stored result, or it keeps waiting on the child that
  already runs.

### 7.2 SSE events (extends section 3.4)

Two server-originated event types join the table in section 3.4. Both carry
`type`, `ts`, and `site`.

| type | Fields | When |
|---|---|---|
| `worker_exit` | `run`, `segment`, `pid: number \| null`, `exit_code: number`, `signal: string \| null`, `status: RunStatus` | After every exit of a worker process, once stdout and stderr of the process reached end of file. |
| `forked` | `run` (the id of the new run), `from_run`, `n` | After `POST /api/runs/:id/fork` created the new run. The event belongs to the history of the new run. |

- `worker_exit.ts` is the end of the segment. `exit_code` is `-1` when a
  signal ended the process, and `signal` is the platform's name for that
  signal (for example `"9"`). `status` is the run status that results from
  the exit: the mapping of section 2.2, and `paused` instead of `lost` for a
  durable run that has a snapshot.
- Every worker event of a segment precedes the `worker_exit` of that segment.
  The statuses `paused`, `completed`, `failed`, `cancelled`, and `lost` are
  set at the exit of the process, not when the matching worker event arrives.
  The `run` event with the new status follows `worker_exit`, and `pid` is
  `null` from then on.
- A `log` event that the site server builds itself has `thread: null` and the
  stream `worker_stderr` for a stderr line, or `worker_stdout` for a stdout
  line that is not a JSON object.

### 7.3 Run record (extends section 3.2)

```
Run += {
  remote_results: [{ call_id, value: json, error: string | null, acked: bool, ts }],
  forked_from: { run, n } | null,
  result_delivered: bool,
  blocked: { reason, path: string[], ts, attempts: number } | null,     // phase 2
}
snapshots[] += { segment: number, automatic: bool }
```

- `remote_results` holds every remote result that reached the run's site.
  `acked` becomes true when a worker reports `remote_result_received` for the
  `call_id`. Results with `acked: false` are passed as `--remote-result` at
  the next resume. A recovery from an older snapshot sets every `acked` back
  to false.
- `forked_from` is set on a run that `POST fork` created. `run` is the source
  run on the same site, and `n` is the copied snapshot. A fork starts with
  status `paused`, with `segment` equal to the segment that wrote the
  snapshot, and with its own copy of the snapshot under the same `n`. It
  copies `remote_results` with `acked: false`. A fork of the latest snapshot
  also copies `waiting_on`. A run that is imported by migration keeps the
  `forked_from` of its origin.
- `result_delivered` is true on a child run once the parent's site accepted
  its result.
- `snapshots[].segment` is the segment that wrote the snapshot.
  `snapshots[].automatic` is true for a snapshot from a `snapshot` event and
  false for one from a `paused` event. `snapshots[].n` is the number in the
  worker's file name when that number is higher than the previous `n`, and the
  previous `n` plus 1 otherwise.
- `blocked` (phase 2) is the latest `blocked` event of a pause request that
  has not succeeded yet. `attempts` counts the `blocked` events since the
  request. The status stays `pausing` while the worker answers only with
  `blocked`. The field is `null` in every other status, and it becomes `null`
  at `paused` and at the exit of the process. The web app shows `reason` next
  to the status.
- A run that is imported by migration lists only the transferred snapshot,
  under the `n` it had on the origin. `GET /api/runs/:id/snapshots/:n/state`
  on the destination serves the imported StateDump under that `n`.
  `events.jsonl` starts empty on the destination, and the earlier segments
  stay in the origin's file under the same run id.

### 7.4 HTTP responses (extends section 3.3)

A command that does not fit the state of the run returns `409 { "error" }`:

| Request | 409 when |
|---|---|
| `POST /api/runs/:id/pause` | the run is not durable, or its status is not `starting` or `running`, or it has no worker process |
| `POST /api/runs/:id/resume` | the status is not `paused`, or the run has no snapshot |
| `POST /api/runs/:id/kill`, `POST /api/runs/:id/cancel` | the run has no worker process |
| `POST /api/runs/:id/fork` | the run has no snapshot (`404` when it has snapshots but not `n`) |
| `POST /api/runs/import` | a run with that id exists here and its status is not `migrated` |

- `POST pause` on a non-durable run is refused, because such a run could never
  leave `pausing`. A second `POST pause` while the status is `pausing` is
  refused too.
- `POST kill` waits up to 3 seconds for the exit of the process, so the
  response already carries `paused` or `lost`.
- `POST /api/runs/import` returns 400 when the run id does not match
  `[a-z0-9-]+`.
- `POST /api/runs/:id/remote_result` for a run whose status is `migrated`
  forwards the result to the peer and returns `{ ok: true, forwarded_to }`,
  or 502 when the peer did not accept it. Forks that wait on the same
  `call_id` receive the result too, on whichever site they are.
- A child run that ends as `lost` or `cancelled` also posts `remote_result`
  with an `error`, so that a parent never waits forever.
- `GET /api/source?file=` accepts the `file` value of a `position` event as it
  is: a path relative to `PROGRAM_DIR`, or an absolute path inside
  `PROGRAM_DIR` (phase 2; a worker reports an absolute path when it cannot
  make it relative). The response repeats the value in `file`. Every other
  path returns 400.

### 7.5 Contents of `events.jsonl`

`events.jsonl` of a run contains, in the order of arrival:

- every worker event of the run as it was forwarded, with `site` added;
- the `log` events that the site server builds from stderr lines and from
  stdout lines that are not JSON;
- the server-originated events about the run: `remote_dispatched`,
  `remote_returned`, `migrated_out`, `migrated_in`, `worker_exit`, and
  `forked`.

It never contains `run` or `init` messages. A reader that rebuilds the history
of a run from this file takes the end of a segment from `worker_exit`, and the
time of a pause request from `pausing`, from the first `blocked`, or from
`paused.ts - stats.pause_latency_ms`.

### 7.6 Configuration and `/api/info` (extends sections 3.1 and 3.3)

| Variable | Meaning | Default |
|---|---|---|
| `WORKER_CWD` | working directory of worker processes, relative to the server's working directory | `..` (that is `demo_durable/`, so that the default `WORKER_CMD` resolves while the server runs from `server/`) |
| `PORT` | listen port | `8787` for `SITE=local`, `8788` for `SITE=cloud` |

- The site server passes absolute paths for `--project`, `--snapshot-dir`, and
  `--resume`.
- A worker process gets the environment of its site server plus
  `BAML_CLI_ALLOW_DIRECT=1` (phase 2). Without that variable `baml-cli worker`
  prints a warning on stderr at every start.
- `GET /api/info` also returns `peer_site: "local" | "cloud"`,
  `worker_cmd: string[]`, and `runs_dir: string` (absolute).

### 7.7 Web app (extends section 3.5)

- `web/src/protocol.ts` declares every type of this section.
- A segment bar ends at its `worker_exit`. The end kind without a terminal
  worker event comes from `worker_exit.status`.
- An automatic snapshot is drawn as a hollow marker, from the `snapshot` event
  or, when the event list is not loaded, from `snapshots[].automatic`.
- A fork appears in the run tree of its source and the source in the tree of
  the fork. The timeline draws a fork marker at `forked.ts`, a gap without a
  process until the first segment of the fork, and a connector from the copied
  snapshot on the source's row. The runs list links a fork to its source.
- `cancelled` ends a segment with the end kind `cancelled`.

## 8. Named sites and the remote pool (phase 3)

This section replaces the two-site assumption of sections 1, 3.1, and 3.3. The
worker contract of section 2 does not change. A worker does not know which
site hosts it.

### 8.1 Sites

- The demo runs three site servers: `local` on port 8787, `cloud` on 8788, and
  `cloud2` on 8789. A site name matches `[a-z0-9]+`.
- Every site server receives the same registry in the environment variable
  `SITES`, a JSON object that maps each site name to its base URL. Default:
  `{"local":"http://127.0.0.1:8787","cloud":"http://127.0.0.1:8788","cloud2":"http://127.0.0.1:8789"}`.
- `PORT` defaults to the port of the site's own entry in `SITES`.
- `PEER_URL` is no longer read.
- `REMOTE_POOL` is a JSON array of site names that may host `remote_` calls.
  Default: `["cloud","cloud2"]`. `local` is not in the pool, so a remote call
  never runs on the local site.

### 8.2 Placement of a remote call

A `remote_` call always runs as its own run in its own worker process, on a
site of the remote pool that is not the caller's site. The site server of the
caller picks the target:

1. Take `REMOTE_POOL` in order and remove the caller's own site.
2. Pick the first remaining site. If the pool has no other site, the call
   fails with the error `no remote site available`, delivered to the worker as
   a `remote_result` with `error`.

The consequences for the demo are: a parent on `local` calls into `cloud`, a
parent on `cloud` calls into `cloud2`, and a parent on `cloud2` calls into
`cloud`.

### 8.3 API changes

- `GET /api/info` adds `sites: [{ name, url }]` in registry order and
  `remote_pool: string[]`. `peer_url` and `peer_site` are removed.
- `POST /api/runs/:id/resume { site }` accepts any site name of the registry.
  An unknown name returns 400. A run that moves records
  `migrated_to: <site>` on the record that stays behind.
- A `remote_result` for a run whose record is `migrated` is forwarded to the
  site in `migrated_to`. The chain is followed until a site holds a record
  that is not `migrated`.
- `parent.site` and `origin.site` name the site to call back. The callback URL
  comes from `SITES`.
- The SSE event fields `child_site`, `to_site`, and `from_site` carry any site
  name.

### 8.4 Web app

- The app reads the site list from `GET /local/api/info` and falls back to
  `["local","cloud","cloud2"]` when that request fails. It opens one
  `EventSource` per site and draws one timeline lane and one log lane per
  site, in registry order.
- The Vite dev server proxies `/<site>/*` for each of the three default sites.
  `LOCAL_SITE_URL`, `CLOUD_SITE_URL`, and `CLOUD2_SITE_URL` override the proxy
  targets, and `WEB_PORT` overrides the dev server port.
- The controls offer one "Resume on <site>" action for every site other than
  the one that holds the paused run.
- Each site has its own color. The colors are used consistently in the runs
  list, the timeline, and the log lanes.

### 8.5 Scripts

- `scripts/dev.sh` starts all three site servers. `PORT_BASE` (default 8787)
  sets the first port, and the three sites use `PORT_BASE`, `PORT_BASE + 1`,
  and `PORT_BASE + 2`. `RUNS_TAG` (default empty) is added to the run store
  directory names (`.baml/runs<RUNS_TAG>-<site>`), so that a second instance
  of the demo can run next to the first one. The cleanup of an instance ends
  only the workers of its own run stores and only the listeners on its own
  ports.
