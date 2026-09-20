/**
 * Protocol types for the durable functions proof of concept.
 *
 * These declarations mirror `documents/durable-poc-contracts.md`, sections
 * 2.3 (worker events), 2.5 (state dump), 3.2 (run record), 3.3 (HTTP API),
 * 3.4 (SSE stream), 7 (accepted extensions), 8 (named sites), and 9 (threads
 * and futures, durable sleep, remote cancellation, program store). Readers ignore unknown
 * fields, and the app ignores unknown event types, so every parser in this
 * file is tolerant.
 */

// ---------------------------------------------------------------------------
// Section 1: common conventions
// ---------------------------------------------------------------------------

/**
 * A site name (section 8.1). It matches `[a-z0-9]+`. The set of sites is not
 * fixed: the app reads it from `GET /api/info` at startup.
 */
export type Site = string;

/** One entry of the site registry, in the form that `GET /api/info` returns it. */
export interface SiteEntry {
  name: Site;
  url: string;
}

/** The registry that the app assumes when `GET /local/api/info` fails (section 8.4). */
export const DEFAULT_SITES: readonly SiteEntry[] = [
  { name: "local", url: "http://127.0.0.1:8787" },
  { name: "cloud", url: "http://127.0.0.1:8788" },
  { name: "cloud2", url: "http://127.0.0.1:8789" },
];

/** The site whose `GET /api/info` names the registry. */
export const REGISTRY_SITE: Site = "local";

export function isSite(value: unknown): value is Site {
  return typeof value === "string" && /^[a-z0-9]+$/.test(value);
}

/** The port of a site's base URL, for the lane label. `null` when the URL has none. */
export function sitePort(url: string): string | null {
  const match = /^[a-z]+:\/\/[^/:]+:(\d+)/i.exec(url);
  return match?.[1] ?? null;
}

/**
 * The site registry that a `GET /api/info` response names, or `null` when the
 * response names none.
 *
 * A server of section 8.3 lists `sites`. A server that predates it names
 * itself and one peer, which is a registry of two sites.
 */
export function sitesFromInfo(info: unknown): SiteEntry[] | null {
  if (!isRecord(info)) return null;
  const listed = info["sites"];
  if (Array.isArray(listed)) {
    const sites: SiteEntry[] = [];
    for (const entry of listed) {
      if (!isRecord(entry) || !isSite(entry["name"])) continue;
      if (sites.some((site) => site.name === entry["name"])) continue;
      sites.push({ name: entry["name"], url: typeof entry["url"] === "string" ? entry["url"] : "" });
    }
    if (sites.length > 0) return sites;
  }
  const self = info["site"];
  const peer = info["peer_site"];
  if (isSite(self) && isSite(peer) && self !== peer) {
    const peerUrl = typeof info["peer_url"] === "string" ? info["peer_url"] : "";
    const pair: SiteEntry[] = [
      { name: self, url: DEFAULT_SITES.find((site) => site.name === self)?.url ?? "" },
      { name: peer, url: peerUrl },
    ];
    // The registry site leads, so the lanes keep the order of the two-site demo.
    return pair.sort((a, b) => Number(b.name === REGISTRY_SITE) - Number(a.name === REGISTRY_SITE));
  }
  return null;
}

/** Any JSON value. */
export type Json = null | boolean | number | string | Json[] | { [key: string]: Json };

export type JsonObject = { [key: string]: Json };

// ---------------------------------------------------------------------------
// Section 2.3: worker events
// ---------------------------------------------------------------------------

/** A stats field that a component cannot measure is `null`. */
export type Stat = number | null;

export interface PauseStats {
  pause_latency_ms: Stat;
  walk_ms: Stat;
  encode_ms: Stat;
  compress_ms: Stat;
  write_ms: Stat;
  objects: Stat;
  raw_bytes: Stat;
  compressed_bytes: Stat;
  program_bytes: Stat;
  blocked_attempts: Stat;
}

export interface ResumeStats {
  process_start_ms: Stat;
  program_load_ms: Stat;
  decode_ms: Stat;
  first_exec_ms: Stat;
  /**
   * Section 9.2: where the resumed worker got the program from. `store` is the
   * content-addressed program store, and `compile` is a compilation of
   * `--project`. Absent on a worker that predates the program store.
   */
  program_source?: ProgramSource | null;
}

export type ProgramSource = "store" | "compile";

/**
 * Fields of every worker event. The site server adds `site` when it forwards
 * the event on the SSE stream. The app fills `site` in when it reads
 * `events.jsonl` from a site and the field is absent.
 */
export interface WorkerEventBase {
  v: 1;
  ts: number;
  run: string;
  segment: number;
  pid: number;
  site: Site;
}

export type PositionReason = "sysop" | "await" | "early_yield" | "remote_call";

export interface HelloEvent extends WorkerEventBase {
  type: "hello";
  mode: "start" | "resume";
  function: string;
  durable: boolean;
  /** Section 9.2: the hash of the program that the worker runs, in hex. */
  program_hash?: string;
}

export interface LogEvent extends WorkerEventBase {
  type: "log";
  /** `worker_stderr` marks a stderr line that the site server forwarded (section 2.2). */
  stream: "stdout" | "stderr" | "worker_stderr";
  text: string;
  thread: number | null;
}

export interface PositionEvent extends WorkerEventBase {
  type: "position";
  thread: number;
  function: string;
  file: string;
  line: number;
  reason: PositionReason;
  op: string | null;
}

export interface ThreadStartedEvent extends WorkerEventBase {
  type: "thread_started";
  thread: number;
  parent_thread: number | null;
  /** Section 10.1: the `spawn` site in the parent thread. Null for the root thread of a segment. */
  file?: string | null;
  line?: number | null;
}

export interface ThreadEndedEvent extends WorkerEventBase {
  type: "thread_ended";
  thread: number;
}

export interface RemoteCallEvent extends WorkerEventBase {
  type: "remote_call";
  call_id: string;
  thread: number;
  function: string;
  args: JsonObject;
  /**
   * Section 10.1: the call site in the calling thread, from the same frame that
   * the accompanying `position` event describes. `file` is relative to the
   * project directory. Both are null when the worker cannot attribute the call
   * to a user frame.
   */
  file?: string | null;
  line?: number | null;
  /**
   * Section 10.3: the function the call is written in, in the spelling of a
   * `position` event. It is the one piece of evidence for "the calling
   * function", which is not the run's entry function whenever the call is made
   * in a helper or in a `spawn` body. Null when the worker could not attribute
   * the call, and absent on a worker that predates the field.
   */
  caller?: string | null;
}

export interface RemoteResultReceivedEvent extends WorkerEventBase {
  type: "remote_result_received";
  call_id: string;
  thread: number;
}

export interface PausingEvent extends WorkerEventBase {
  type: "pausing";
  waiting_on: string[];
}

export interface BlockedEvent extends WorkerEventBase {
  type: "blocked";
  reason: string;
  path: string[];
}

/**
 * Section 9.2: the reason a run suspended itself. `remaining_ms` is measured
 * at the moment of the snapshot, and `at_ts` is the deadline of the sleep on
 * the worker's clock.
 */
export interface SleepWake {
  reason: "sleep" | (string & {});
  remaining_ms: number;
  at_ts: number;
}

export interface PausedEvent extends WorkerEventBase {
  type: "paused";
  snapshot_path: string;
  state_path: string;
  stats: PauseStats;
  /** Section 9.2: set when the run suspended itself for a sleep. Absent or `null` for a requested pause. */
  wake?: SleepWake | null;
}

/**
 * Section 7.1: a snapshot that no `pause` command requested. The run keeps
 * running. The fields are those of `paused` plus `automatic: true`.
 */
export interface SnapshotEvent extends WorkerEventBase {
  type: "snapshot";
  snapshot_path: string;
  state_path: string;
  stats: PauseStats;
  automatic: true;
}

export interface ResumedEvent extends WorkerEventBase {
  type: "resumed";
  stats: ResumeStats;
}

export interface CompletedEvent extends WorkerEventBase {
  type: "completed";
  value: Json;
}

export interface StackEntry {
  function: string;
  file: string;
  line: number;
}

export interface FailedEvent extends WorkerEventBase {
  type: "failed";
  error: string;
  stack: StackEntry[];
}

/** Section 7.1: the last event of a worker that exits with code 130. */
export interface CancelledEvent extends WorkerEventBase {
  type: "cancelled";
}

/**
 * Section 10.1: how the engine classified a cancellation.
 *
 * `future_cancel` covers `Future.cancel` and a `race` loser, `token` a user
 * `baml.spawn.CancelToken` linked to the thread (which covers `with_timeout`),
 * `parent` an ancestor thread's token, and `unknown` a cancellation that the
 * engine cannot attribute.
 */
export type CancelCause = "future_cancel" | "token" | "parent" | "unknown";

const CANCEL_CAUSES: ReadonlySet<string> = new Set<CancelCause>(["future_cancel", "token", "parent", "unknown"]);

/** The `cause` of a `remote_cancel` event, or `null` when it carries none this app knows. */
export function cancelCauseOf(value: unknown): CancelCause | null {
  return typeof value === "string" && CANCEL_CAUSES.has(value) ? (value as CancelCause) : null;
}

/**
 * Section 9.2: the thread that waited on a remote call was cancelled. The run
 * no longer waits on `call_id`.
 */
export interface RemoteCancelEvent extends WorkerEventBase {
  type: "remote_cancel";
  call_id: string;
  /** Section 9.7: null for a call that was outstanding at the end of the run. */
  thread: number | null;
  /** Section 10.1: the call site of the cancelled call, as recorded when the call was made. */
  file?: string | null;
  line?: number | null;
  /** Section 10.1: why the thread was cancelled. Absent on a worker that predates the field. */
  cause?: CancelCause | (string & {}) | null;
}

export type WorkerEvent =
  | HelloEvent
  | LogEvent
  | PositionEvent
  | ThreadStartedEvent
  | ThreadEndedEvent
  | RemoteCallEvent
  | RemoteResultReceivedEvent
  | PausingEvent
  | BlockedEvent
  | PausedEvent
  | SnapshotEvent
  | ResumedEvent
  | CompletedEvent
  | FailedEvent
  | CancelledEvent
  | RemoteCancelEvent;

export type WorkerEventType = WorkerEvent["type"];

// ---------------------------------------------------------------------------
// Section 2.5: state dump
// ---------------------------------------------------------------------------

export type DumpValueKind =
  | "null"
  | "bool"
  | "int"
  | "float"
  | "string"
  | "bigint"
  | "array"
  | "map"
  | "instance"
  | "closure"
  | "opaque"
  | "omitted"
  // Phase 3 additions of the snapshot core. A future has the preview
  // `#<id> (<state>)` and, once it settled, one child `value` or `error`.
  | "future"
  | "cancel_token"
  | "task_group";

export interface DumpValue {
  /** One of `DumpValueKind`. A reader must tolerate a kind that it does not know. */
  kind: DumpValueKind | (string & {});
  preview: string;
  class?: string;
  children?: DumpChild[];
}

export interface DumpChild {
  key: string;
  value: DumpValue;
}

export interface DumpLocal {
  name: string;
  type: string | null;
  value: DumpValue;
}

/** Frames are ordered innermost first. */
export interface DumpFrame {
  function: string;
  file: string;
  line: number;
  locals: DumpLocal[];
}

export interface DumpThread {
  thread: number;
  name: string;
  parked: { kind: string; detail: string };
  frames: DumpFrame[];
  // Phase 3 additions of the snapshot core. Absent in a dump of an earlier worker.
  parent_thread?: number | null;
  /** The id of the future that the thread settles when it ends. The preview of a future names the id as `#<id>`. */
  settles_future?: number | null;
  /** The thread was cancelled and ends at its next await point. */
  cancelled?: boolean;
}

export interface HeapKindTotals {
  count: number;
  bytes: number;
}

export interface StateDump {
  run: string;
  segment: number;
  created_ts: number;
  threads: DumpThread[];
  heap: { objects: number; bytes: number; by_kind: Record<string, HeapKindTotals> };
}

// ---------------------------------------------------------------------------
// Section 3.2: run record
// ---------------------------------------------------------------------------

export type RunStatus =
  | "starting"
  | "running"
  | "pausing"
  | "paused"
  | "completed"
  | "failed"
  | "lost"
  | "cancelled"
  | "migrated"
  /** Section 9.3: the run suspended itself for a sleep. No process exists, and a timer resumes it at `wake_at`. */
  | "sleeping";

export interface RunParent {
  site: Site;
  run: string;
  call_id: string;
}

export interface RunOrigin {
  site: Site;
  run: string;
}

export interface RunPosition {
  thread: number;
  function: string;
  file: string;
  line: number;
}

export interface WaitingOn {
  call_id: string;
  child_site: Site;
  child_run: string;
  function: string;
  /** Section 9.7: true on an entry that a fork copied, or that a run attached to a running child. */
  inherited?: boolean;
  /** Section 10.2: the call site that the worker reported with `remote_call`, so it survives a resume. */
  file?: string | null;
  line?: number | null;
  /** Section 10.3: the function that made the call, as `remote_call` reported it. */
  caller?: string | null;
}

/**
 * Section 9.7: one remote call that a worker of the run announced. Section 10.2
 * adds the call site, so the cause of a call survives a resume and a reload.
 */
export interface CallRecord {
  call_id: string;
  function: string;
  args: JsonObject;
  file?: string | null;
  line?: number | null;
  /** Section 10.3: the function that made the call, as `remote_call` reported it. */
  caller?: string | null;
}

export interface SnapshotRecord {
  n: number;
  snapshot_path: string;
  state_path: string;
  bytes: number;
  ts: number;
  stats: PauseStats | null;
  /** Section 7.3: the segment that wrote the snapshot. Absent on servers that predate it. */
  segment?: number;
  /** Section 7.3: true for a snapshot that no `pause` command requested. */
  automatic?: boolean;
}

/** Section 7.3: a remote result that reached the run's site. */
export interface StoredResult {
  call_id: string;
  value: Json;
  error: string | null;
  /** True once a worker reported `remote_result_received` for the result. */
  acked: boolean;
  ts: number;
}

/** Section 7.3: the snapshot that a forked run started from. */
export interface ForkRef {
  run: string;
  n: number;
}

/** Section 7.3: the latest `blocked` answer to a pause request that has not succeeded yet. */
export interface PauseBlocked {
  reason: string;
  path: string[];
  ts: number;
  attempts: number;
}

export interface Run {
  id: string;
  site: Site;
  function: string;
  args: JsonObject;
  durable: boolean;
  status: RunStatus;
  parent: RunParent | null;
  origin: RunOrigin | null;
  segment: number;
  pid: number | null;
  position: RunPosition | null;
  waiting_on: WaitingOn[];
  snapshots: SnapshotRecord[];
  result: Json;
  error: string | null;
  created_ts: number;
  updated_ts: number;
  // Section 7.3. `normalizeRun` fills them in for a server that omits them.
  remote_results: StoredResult[];
  forked_from: ForkRef | null;
  result_delivered: boolean;
  blocked: PauseBlocked | null;
  /** Section 8.3: the site that a `migrated` record moved to. Absent on a record that did not move. */
  migrated_to?: Site | null;
  /** Section 9.3: the time at which the site server resumes a `sleeping` run (Unix ms). */
  wake_at?: number | null;
  /** Section 9.3: the hash of the program that the run executes, in hex. */
  program_hash?: string | null;
  /** Section 9.7: the remote calls that workers of the run announced. A fork copies the list. */
  calls?: CallRecord[];
  /** Section 9.7: the remote calls that the run no longer waits on. */
  cancelled_calls?: string[];
}

// ---------------------------------------------------------------------------
// Section 3.3: HTTP API
// ---------------------------------------------------------------------------

export interface FunctionParam {
  name: string;
  type: string;
}

export interface FunctionInfo {
  name: string;
  params: FunctionParam[];
  durable: boolean;
  remote: boolean;
}

export interface SiteInfo {
  site: Site;
  program_dir: string;
  /** May be empty when reflection is impractical on the site server. */
  functions: FunctionInfo[];
  // Section 7.6.
  worker_cmd?: string[];
  runs_dir?: string;
  // Section 8.3. `sitesFromInfo` reads the registry and tolerates a server that omits it.
  /** The site registry, in registry order. */
  sites?: SiteEntry[];
  /** The sites that may host `remote_` calls. */
  remote_pool?: Site[];
  /** Sections 3.3 and 7.6, removed by section 8.3. A server that predates section 8 still sends them. */
  peer_url?: string;
  peer_site?: Site;
}

export interface ApiError {
  error: string;
}

export interface StartRunRequest {
  function: string;
  args: JsonObject;
}

export interface ResumeRequest {
  /** Any site of the registry (section 8.3). Omitted: resume on the site that holds the run. */
  site?: Site;
}

export interface ForkRequest {
  n?: number;
}

export interface SourceResponse {
  file: string;
  text: string;
}

/** Peer to peer. Declared for completeness. The app does not send it. */
export interface RemoteRunRequest {
  function: string;
  args: JsonObject;
  parent: RunParent;
  // Section 9.5.
  program_hash?: string;
  from_site?: Site;
}

/** Section 9.5: `GET /api/programs/:hash`. Peer to peer. Declared for completeness. */
export interface ProgramResponse {
  hash: string;
  runtime_build: string;
  program_base64: string;
}

/** Peer to peer. Declared for completeness. The app does not send it. */
export type RemoteResultRequest =
  | { call_id: string; value: Json }
  | { call_id: string; error: string };

/** Peer to peer. Declared for completeness. The app does not send it. */
export interface ImportRunRequest {
  run: Run;
  snapshot_base64: string;
  state: StateDump;
  // Section 9.5.
  program_hash?: string;
  from_site?: Site;
}

// ---------------------------------------------------------------------------
// Section 3.4: SSE stream
// ---------------------------------------------------------------------------

export interface InitEvent {
  type: "init";
  ts: number;
  site: Site;
  runs: Run[];
}

export interface RunEvent {
  type: "run";
  ts: number;
  site: Site;
  run: Run;
}

export interface RemoteDispatchedEvent {
  type: "remote_dispatched";
  ts: number;
  site: Site;
  run: string;
  call_id: string;
  child_site: Site;
  child_run: string;
  function: string;
}

export interface RemoteReturnedEvent {
  type: "remote_returned";
  ts: number;
  site: Site;
  run: string;
  call_id: string;
  child_site: Site;
  child_run: string;
  ok: boolean;
}

export interface MigratedOutEvent {
  type: "migrated_out";
  ts: number;
  site: Site;
  run: string;
  to_site: Site;
}

export interface MigratedInEvent {
  type: "migrated_in";
  ts: number;
  site: Site;
  run: string;
  from_site: Site;
}

/**
 * Section 7.2: a worker process ended. `ts` is the time at which the site
 * server saw the exit, after both pipes of the worker were drained. `status`
 * is the run status that results from the exit.
 */
export interface WorkerExitEvent {
  type: "worker_exit";
  ts: number;
  site: Site;
  run: string;
  segment: number;
  pid: number | null;
  exit_code: number;
  signal: string | null;
  status: RunStatus;
}

/** Section 7.2: `run` was created from snapshot `n` of `from_run` on the same site. */
export interface ForkedEvent {
  type: "forked";
  ts: number;
  site: Site;
  run: string;
  from_run: string;
  n: number;
}

/** Section 9.3: the run suspended itself, and the site server set a timer for `wake_at`. */
export interface SleepScheduledEvent {
  type: "sleep_scheduled";
  ts: number;
  site: Site;
  run: string;
  wake_at: number;
}

export type WakeReason = "timer" | "manual" | "restart";

/** Section 9.3: a `sleeping` run starts its next segment. */
export interface WokenEvent {
  type: "woken";
  ts: number;
  site: Site;
  run: string;
  reason: WakeReason | (string & {});
}

/** Section 9.3: the parent's site asked the child's site to cancel the child of a cancelled remote call. */
export interface RemoteCancelledEvent {
  type: "remote_cancelled";
  ts: number;
  site: Site;
  run: string;
  call_id: string;
  child_site: Site;
  child_run: string;
}

/** Events that the site server produces about one run. `run` is a run id. */
export type SiteRunEvent =
  | RemoteDispatchedEvent
  | RemoteReturnedEvent
  | MigratedOutEvent
  | MigratedInEvent
  | WorkerExitEvent
  | ForkedEvent
  | SleepScheduledEvent
  | WokenEvent
  | RemoteCancelledEvent;

/** Every event that belongs to the history of one run. */
export type RunHistoryEvent = WorkerEvent | SiteRunEvent;

/** Every SSE message that the app understands. */
export type SseEvent = InitEvent | RunEvent | RunHistoryEvent;

const WORKER_EVENT_TYPES: ReadonlySet<string> = new Set<WorkerEventType>([
  "hello",
  "log",
  "position",
  "thread_started",
  "thread_ended",
  "remote_call",
  "remote_result_received",
  "pausing",
  "blocked",
  "paused",
  "snapshot",
  "resumed",
  "completed",
  "failed",
  "cancelled",
  "remote_cancel",
]);

const SITE_RUN_EVENT_TYPES: ReadonlySet<string> = new Set<SiteRunEvent["type"]>([
  "remote_dispatched",
  "remote_returned",
  "migrated_out",
  "migrated_in",
  "worker_exit",
  "forked",
  "sleep_scheduled",
  "woken",
  "remote_cancelled",
]);

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

/**
 * Parses one SSE message or one line of `events.jsonl`.
 *
 * Returns `null` for a message that is not an object, has no usable `type`
 * and `ts`, or has a type that this app does not know. `fallbackSite` is the
 * site that the message was read from. It is used when the message has no
 * `site` field, which is the case for `events.jsonl` on some servers.
 *
 * The parser checks the discriminating fields only. It trusts the remaining
 * fields of a known event type, because the contract defines them.
 */
export function parseSseEvent(raw: unknown, fallbackSite: Site): SseEvent | null {
  if (!isRecord(raw)) return null;
  const type = raw["type"];
  const ts = raw["ts"];
  if (typeof type !== "string" || typeof ts !== "number") return null;
  const site = isSite(raw["site"]) ? raw["site"] : fallbackSite;
  if (type === "init") {
    const runs = raw["runs"];
    if (!Array.isArray(runs)) return null;
    return { type, ts, site, runs: runs.filter(isRunRecord) };
  }
  if (type === "run") {
    const run = raw["run"];
    if (!isRunRecord(run)) return null;
    return { type, ts, site, run };
  }
  if (WORKER_EVENT_TYPES.has(type) || SITE_RUN_EVENT_TYPES.has(type)) {
    if (typeof raw["run"] !== "string") return null;
    return { ...raw, site } as unknown as RunHistoryEvent;
  }
  return null;
}

/** Checks the fields of a run record that the app cannot work without. */
export function isRunRecord(value: unknown): value is Run {
  return (
    isRecord(value) &&
    typeof value["id"] === "string" &&
    isSite(value["site"]) &&
    typeof value["status"] === "string"
  );
}

/**
 * Fills in the optional collections of a run record, so that the rest of the
 * app can rely on the shape in section 3.2 even when a server omits a field.
 */
export function normalizeRun(run: Run): Run {
  const loose = run as Partial<Run> & Pick<Run, "id" | "site" | "status">;
  return {
    ...run,
    function: loose.function ?? "",
    args: loose.args ?? {},
    durable: loose.durable ?? false,
    parent: loose.parent ?? null,
    origin: loose.origin ?? null,
    segment: loose.segment ?? 1,
    pid: loose.pid ?? null,
    position: loose.position ?? null,
    waiting_on: loose.waiting_on ?? [],
    snapshots: loose.snapshots ?? [],
    result: loose.result ?? null,
    error: loose.error ?? null,
    created_ts: loose.created_ts ?? 0,
    updated_ts: loose.updated_ts ?? loose.created_ts ?? 0,
    remote_results: loose.remote_results ?? [],
    forked_from: loose.forked_from ?? null,
    result_delivered: loose.result_delivered ?? false,
    blocked: loose.blocked ?? null,
    wake_at: typeof loose.wake_at === "number" ? loose.wake_at : null,
    program_hash: typeof loose.program_hash === "string" ? loose.program_hash : null,
    calls: loose.calls ?? [],
    cancelled_calls: loose.cancelled_calls ?? [],
  };
}

/** Statuses in which a worker process exists or is about to exist. */
export function isLiveStatus(status: RunStatus): boolean {
  return status === "starting" || status === "running" || status === "pausing";
}

/**
 * Statuses in which the picture of a run changes with the clock: a process
 * exists, or a timer counts down to the wake time of a `sleeping` run.
 */
export function isTickingStatus(status: RunStatus): boolean {
  return isLiveStatus(status) || status === "sleeping";
}

/** Statuses in which a run has no process and continues from a snapshot: by a command, or by its wake timer. */
export function isSuspendedStatus(status: RunStatus): boolean {
  return status === "paused" || status === "sleeping";
}

/** A source location that a worker event or a run record carries (section 10.1). */
export interface SourceLocation {
  file: string;
  line: number;
}

/**
 * The `file` and `line` of a value that may carry them (a `remote_call`, a
 * `remote_cancel`, a `thread_started`, or a `waiting_on` entry). `null` when
 * either is missing, so the app never shows half a location.
 */
export function locationOf(value: { file?: string | null; line?: number | null } | null | undefined): SourceLocation | null {
  if (!value) return null;
  const { file, line } = value;
  if (typeof file !== "string" || file === "" || typeof line !== "number" || !Number.isFinite(line) || line < 1) return null;
  return { file, line };
}

/** The last path segment of a file, for a compact link label. */
export function baseName(file: string): string {
  const slash = Math.max(file.lastIndexOf("/"), file.lastIndexOf("\\"));
  return slash === -1 ? file : file.slice(slash + 1);
}

/** The first characters of a program hash, for a label. The full hash stays in the tooltip. */
export function shortHash(hash: string | null | undefined, length = 10): string | null {
  return typeof hash === "string" && hash !== "" ? hash.slice(0, length) : null;
}
