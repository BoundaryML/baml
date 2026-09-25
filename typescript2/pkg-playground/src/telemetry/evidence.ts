/**
 * Catalog rows to the shape the Telemetry views render.
 *
 * ## Vocabulary (these words are load-bearing; keep them exact)
 *
 * - **Execution**: one root thread. It has an entry point, and it is what a
 *   row in the executions table stands for.
 * - **Span**: one individually retained call, with exact timestamps. A span
 *   is evidence.
 * - **Context**: one calling context, with complete counts and timing for
 *   every call that ever took that path. A context is a summary: it has no
 *   per-instance ordering and no position in time.
 * - **Exemplar**: a span joined to its context, so an aggregate can show
 *   real instances.
 * - **Gap**: calls the counts know happened but no span shows.
 *
 * ## The rules this module exists to keep
 *
 * 1. An aggregate never gets a position on a time axis.
 * 2. A count never expands into rows unless every instance was retained.
 * 3. Nothing is invented. A missing number stays null and the view says so,
 *    rather than being filled in with a plausible one.
 *
 * The span-to-context join is exact: `calls_v1.call_path_id` names the
 * aggregate directly, so no part of this file guesses at attribution by
 * matching function names.
 */

import type {
  ExecutionTelemetry,
  SourceVerification,
  TelemetryCall,
  TelemetryCallPath,
  TelemetryErrorCapture,
  TelemetryErrorStep,
  TelemetryExecution,
  TelemetryThread,
} from '../worker-protocol';

export type { SourceVerification } from '../worker-protocol';

/**
 * Status vocabulary shared by executions, spans, and threads.
 *
 * `incomplete` means no end is recorded. Unlike `running`, it does not claim
 * the work is still in flight: the recording may simply stop before the end
 * was written, and nothing says which.
 */
export type TelemetryStatus =
  | 'succeeded'
  | 'failed'
  | 'running'
  | 'cancelled'
  | 'incomplete';

/** What kind of call a row stands for; drives glyph and colour. */
export type CallKind = 'baml' | 'llm' | 'host' | 'spawn';

/**
 * How an execution was started.
 *
 * `unknown` is the common case today: the store records a source-snapshot
 * hash rather than the command that ran, so origin usually cannot be told.
 */
export type SourceKind = 'cli' | 'playground' | 'test' | 'sdk' | 'unknown';

/** One row in the executions table. */
export interface ExecutionRow {
  id: string;
  /** Root function, or the execution id when the root was not retained. */
  target: string;
  /**
   * What the store says produced this run. Usually a source-snapshot
   * identity rather than a command, so it is shortened for display and
   * `sourceLabelIsIdentity` says which one it is.
   */
  entryPoint: string;
  /** True when `entryPoint` is an opaque identity, not a readable command. */
  entryPointIsIdentity: boolean;
  sourceKind: SourceKind;
  status: TelemetryStatus;
  startedMs: number | null;
  durationMs: number | null;
  /** Every call that ran. Null when the execution never sealed. */
  calls: number | null;
  errors: number | null;
  /** Calls kept as spans. Never more than `calls`. */
  spanCount: number | null;
  revision: string | null;
  /**
   * True once the execution has sealed and its evidence is published.
   * A running execution is not complete and has no evidence yet, which is
   * a different thing from an execution whose records were lost.
   */
  indexComplete: boolean;
  /**
   * True only when records were actually lost. `no_root_ended` means the
   * root has not returned: normal for a run in flight, and a sign of a
   * writer that died only once the execution is no longer running. Reading
   * every non-complete state as loss told people their data was damaged
   * while they were watching it being produced.
   */
  recordsLost: boolean;
  /** `complete` | `partial` | `none`: whether captured values survived. */
  valueState: string | null;
}

/** One calling context: complete counts, no per-instance time. */
export interface ContextNode {
  id: string;
  parentId: string | null;
  fn: string;
  /** Fully qualified name: `user.` marks the reader's own code. */
  fqn: string | null;
  kind: CallKind;
  /**
   * Population entries: the denominator for any mean or rate. Null when the
   * recording has no count (never shown as zero).
   */
  enters: number | null;
  /** Errored calls in the population; null when outcomes are not recorded. */
  errors: number | null;
  ok: number | null;
  cancelled: number | null;
  outcomeState: TelemetryCallPath['outcomeState'] | null;
  totalMs: number | null;
  /** Null when self time is not supported by the evidence. */
  selfMs: number | null;
  awaitMs: number | null;
  /**
   * Waiting anywhere in this path's subtree, not just its own frame.
   *
   * The profiler records a wait on the frame that actually suspended, which
   * is always deep runtime plumbing: `baml.http._send`, never the model call
   * that led there. So a frame's own `awaitMs` is zero for essentially all
   * user code, and only the subtree total can answer "how much of this was
   * spent waiting". Null when any wait in the subtree is unknown.
   */
  subtreeAwaitMs: number | null;
  /** Reached through a spawn edge, so it overlaps its parent in time. */
  spawn: boolean;
  /** A synthetic row standing in for paths the tree could not keep apart. */
  folded: boolean;
  /**
   * False when a counter saturated or self time underflowed. A view must
   * not derive a percentage from a row that says this.
   */
  timingComplete: boolean;
  source: SourceRef | null;
}

/**
 * A recorded source location. It points into the program that ran, which
 * `verification` compares with the program built now: a `stale` or
 * `unverified` line may no longer be where the code is.
 */
export interface SourceRef {
  file: string;
  line: number | null;
  start: number | null;
  end: number | null;
  verification: SourceVerification;
}

/** One retained call: exact evidence. */
export interface SpanNode {
  id: string;
  /** Nearest retained ancestor; the real parent may be summary-only. */
  parentId: string | null;
  /** Exact join to the aggregate, straight from the store. */
  contextId: string | null;
  fn: string;
  kind: CallKind;
  threadId: string | null;
  threadName: string;
  /** Offset from the execution's start. */
  startMs: number;
  durationMs: number | null;
  status: TelemetryStatus;
  /** Why this call was kept: root, llm, manual. */
  reasons: string[];
  /** Per-role captured values, each with why it is or is not here. */
  values: {
    args: CapturedValue;
    output: CapturedValue;
    error: CapturedValue;
  };
  source: SourceRef | null;
  errorId: string | null;
}

/**
 * Why a value is or is not here. `notCaptured` and `lost` are different
 * facts with different remedies, and the UI must never merge them:
 * the first means widen the policy, the second means records were dropped.
 */
export type ValueAvailability =
  | { state: 'available'; cid: string | null }
  | { state: 'notCaptured' }
  | { state: 'lost'; reason: string }
  | { state: 'notApplicable' };

/**
 * One captured value.
 *
 * `body` is the parsed value when one was hydrated. Media inside it is a
 * descriptor rather than bytes; `mediaCid` is how those bytes are fetched,
 * on demand, for the roles that carry any.
 */
export interface CapturedValue {
  availability: ValueAvailability;
  body: unknown;
  /** Content id for fetching media bytes, when the value holds media. */
  mediaCid: string | null;
}

/** Calls the counts know about that no span shows. Never drawn as a bar. */
export interface GapInfo {
  id: string;
  contextId: string;
  fn: string;
  /** `enters - retained` on this context. */
  calls: number;
}

/** One logical thread. */
export interface ThreadLane {
  id: string;
  name: string;
  /** Offsets from the execution's start. */
  firstMs: number;
  lastMs: number | null;
  status: TelemetryStatus | null;
  spawnedBy: { atMs: number | null; fn: string | null; thread: string } | null;
  /** The spawn expression in the spawning function, when recorded. */
  spawnSite: SourceRef | null;
  /**
   * Time this lane spent running vs waiting. The store attributes both per
   * call path, not per thread, so these stay null and the lane renders
   * without a busy segment rather than with an invented one.
   */
  busyMs: null;
  awaitMs: null;
}

/**
 * One captured error.
 *
 * An error capture is not the same as an errored call: a single throw
 * unwinds through every frame above it, so one capture commonly marks many
 * calls as errored. Conflating the two is how a UI reports "8 errors" for
 * one bug.
 *
 * `grain` says which one this is. `throw` is a recorded throw occurrence
 * with its origin and stack. `erroredCall` is only an error value observed
 * on one retained call that ended in an error: where it was thrown, and
 * whether several such rows come from one throw, is not recorded, so these
 * rows are never counted as distinct errors or merged by equal values.
 */
export type ErrorGrain = 'throw' | 'erroredCall';

export interface ErrorCapture {
  id: string;
  grain: ErrorGrain;
  /**
   * For a throw, the function that raised it. For an errored call, the
   * function of the call that ended with it (not necessarily the thrower).
   */
  fn: string | null;
  /**
   * For a throw, the nearest retained span that raised it; for an errored
   * call, that call. Either way, where Trace pivots to.
   */
  callId: string | null;
  threadId: string | null;
  /** `fresh` for a new throw, `rethrow` for one passed along. */
  kind: string | null;
  /**
   * `fresh` is a distinct error. `ambiguous` and `unresolved` raises passed
   * an error along without a proven origin: shown, never counted as errors.
   * Null from backends that do not say.
   */
  originState: 'fresh' | 'proven' | 'ambiguous' | 'unresolved' | null;
  /** How many different origins matched an ambiguous raise. */
  originCandidates: number | null;
  unresolvedReason: string | null;
  /** What raised it, e.g. `throw`, `runtime`, `native_boundary`. */
  raiseKind: string | null;
  /**
   * The site is the BAML call that entered native or host code, not a
   * location inside that code.
   */
  boundary: boolean;
  /** Raises that passed this error along, in order. */
  propagation: ErrorStep[];
  /** `caught`, `unhandled`, ... for the raise that started it. */
  unwindResult: string | null;
  handlerFn: string | null;
  /** Retained calls this error failed. */
  failedCallIds: string[];
  /** A trace the VM carried from an earlier throw: names only. */
  inheritedStack: string[];
  /** Where it came from: bytecode, native_call, engine_call, future_resume. */
  source: string | null;
  source_location: SourceRef | null;
  /** Function names, root to throw: the traceback. */
  stack: string[];
  /** False when the stack has gaps, so it is not the whole path. */
  stackComplete: boolean;
  /** The captured error, parsed when it was valid JSON. */
  value: unknown;
  /** Set when no value could be shown, saying why. */
  valueUnavailable: ValueAvailability | null;
}

/** One rethrow or await that passed an error along. */
export interface ErrorStep {
  raiseId: string;
  kind: string | null;
  fn: string | null;
  threadId: string | null;
  source: SourceRef | null;
  result: string | null;
  handlerFn: string | null;
}

export interface Evidence {
  contexts: ContextNode[];
  spans: SpanNode[];
  gaps: GapInfo[];
  threads: ThreadLane[];
  /** Captured errors, root cause first. Both grains; see `ErrorCapture`. */
  errors: ErrorCapture[];
  /** Execution wall time, from the root span or the thread lanes. */
  durationMs: number | null;
  /** Sum of self time: attributed CPU, with no double counting. */
  cpuMs: number | null;
  /** Sum of await time: attributed waiting. */
  awaitMs: number | null;
  /** Total calls the counts cover, including folded rows; null if unknown. */
  totalCalls: number | null;
  /** Calls retained as spans. */
  retainedCalls: number;
  /** True once the execution sealed and published its evidence. */
  indexComplete: boolean;
  /** True only when records were actually lost, never merely in flight. */
  recordsLost: boolean;
  /** False when any context reported saturated or underflowed timing. */
  timingComplete: boolean;
  /**
   * Whether value bodies can be read over this connection. The store keeps
   * content ids; serving the bodies is separate work, so this is false and
   * the inspectors say the body exists but is not readable here.
   */
  valuesReadable: boolean;
}

const NS_PER_MS = 1_000_000;

/**
 * Fold rather than spread.
 *
 * These arrays hold one entry per retained span, bounded only by the
 * query's row limit. Spreading a hundred thousand arguments into
 * `Math.min` overflows the call stack and throws, so the panel would fail
 * on exactly the large executions it exists to explain.
 */
export function minOf(values: number[]): number {
  let smallest = Number.POSITIVE_INFINITY;
  for (const value of values) if (value < smallest) smallest = value;
  return Number.isFinite(smallest) ? smallest : 0;
}

export function maxOf(values: number[]): number {
  let largest = Number.NEGATIVE_INFINITY;
  for (const value of values) if (value > largest) largest = value;
  return Number.isFinite(largest) ? largest : 0;
}

function ms(ns: number | null | undefined): number | null {
  return ns == null ? null : ns / NS_PER_MS;
}

/** Short name for display: the last segment of a dotted fully qualified name. */
export function shortFunctionName(fqn: string): string {
  const cut = fqn.lastIndexOf('.');
  return cut >= 0 ? fqn.slice(cut + 1) : fqn;
}

function statusOf(status: string | null | undefined): TelemetryStatus {
  switch (status) {
    case 'failed':
    case 'panicked':
    case 'errored':
      return 'failed';
    case 'cancelled':
      return 'cancelled';
    case 'running':
      return 'running';
    case 'incomplete':
      return 'incomplete';
    // An abandoned execution never sealed: its writer died or the records
    // were lost. That is closer to a failure than to a success, and calling
    // it succeeded would be a lie.
    case 'abandoned':
      return 'failed';
    case 'succeeded':
    case 'ok':
      return 'succeeded';
    case 'exited':
      // An explicit terminal exit is not a failure.
      return 'succeeded';
    default:
      // No status means no end fact: `calls_v1.status` is NULL for a call
      // that never returned. Defaulting to success painted an unfinished
      // call green, which is the opposite of what happened.
      return 'running';
  }
}

function threadStatusOf(
  status: string | null | undefined,
): TelemetryStatus | null {
  switch (status) {
    case 'completed':
      return 'succeeded';
    case 'cancelled':
      return 'cancelled';
    case 'errored':
      return 'failed';
    default:
      return null;
  }
}

function availability(
  state: string | null,
  cid: string | null,
): ValueAvailability {
  if (state == null || state === 'not_applicable') {
    return { state: 'notApplicable' };
  }
  if (state === 'available') return { cid, state: 'available' };
  if (state === 'not_captured') return { state: 'notCaptured' };
  if (state.startsWith('lost:')) {
    return { reason: state.slice('lost:'.length), state: 'lost' };
  }
  return { state: 'notApplicable' };
}

/**
 * Whether a source label is an opaque identity rather than a readable name.
 *
 * The engine passes the source snapshot's content hash here, so most labels
 * are bare hex. Showing 64 hex characters where a command belongs is worse
 * than showing nothing, and reading an origin out of a hash is impossible.
 */
export function isOpaqueSourceLabel(label: string | null): boolean {
  return label != null && /^[0-9a-f]{16,}$/i.test(label);
}

/**
 * Which entry point started this execution.
 *
 * Origin is not recorded: the store keeps a source-snapshot hash, not the
 * command that ran. So a label that carries no origin returns `unknown`
 * rather than a specific-looking guess, and the list hides source filters
 * that nothing can match.
 */
export function sourceKindOf(label: string | null): SourceKind {
  if (label == null || label === '' || isOpaqueSourceLabel(label)) {
    return 'unknown';
  }
  const text = label.toLowerCase();
  if (text.includes('test')) return 'test';
  if (text.includes('playground')) return 'playground';
  if (text.includes('cli') || text.startsWith('baml ')) return 'cli';
  return 'sdk';
}

/**
 * Whether an execution's index says records were lost.
 *
 * `root_started_lost` and `index_corrupt` are always damage. `no_root_ended`
 * only is once the execution has stopped running: while it runs, it simply
 * means the root has not returned yet.
 */
function recordsWereLost(
  indexState: string | null,
  status: string | null,
): boolean {
  if (indexState == null || indexState === 'complete') return false;
  // No end recorded, and nothing says whether the run is still going or its
  // writer stopped: that is missing evidence, not proven loss.
  if (indexState === 'no_root_ended') {
    return status !== 'running' && status !== 'incomplete';
  }
  return true;
}

/** Short display form for an opaque identity. */
function shortIdentity(label: string): string {
  return label.slice(0, 12);
}

/**
 * A readable stand-in for an execution with no entry function yet.
 *
 * Ids are `baml_thread_1_<base64url>`; the prefix is the same on every row,
 * so only the tail distinguishes them.
 */
export function shortExecutionId(id: string): string {
  const payload = id.replace(/^baml_thread_1_/, '');
  return `execution ${payload.slice(0, 10)}`;
}

/**
 * Call kind for colour and glyph.
 *
 * LLM-ness is not a store column: the profiler classifies by execution kind
 * (bytecode, native, and so on), while whether a BAML function calls a model
 * is a property of the program. The caller passes the set the project
 * reports, and names are matched fully qualified or short so a namespaced
 * fqn still resolves.
 */
function callKind(
  fqn: string | null,
  edgeKind: string | null,
  llmFunctions: ReadonlySet<string>,
): CallKind {
  if (
    fqn &&
    (llmFunctions.has(fqn) || llmFunctions.has(shortFunctionName(fqn)))
  ) {
    return 'llm';
  }
  if (edgeKind === 'spawn') return 'spawn';
  return 'baml';
}

export interface EvidenceOptions {
  /** Functions the project reports as LLM calls, by name or short name. */
  llmFunctions?: ReadonlySet<string>;
}

/** One execution's row in the executions table. */
export function toExecutionRow(execution: TelemetryExecution): ExecutionRow {
  const entry = execution.entryFqn;
  const label = execution.sourceLabel;
  const opaque = isOpaqueSourceLabel(label);
  return {
    calls: execution.totalCalls,
    durationMs: ms(execution.durationNs),
    entryPoint: opaque
      ? `source ${shortIdentity(label as string)}`
      : (label ?? 'local runtime'),
    entryPointIsIdentity: opaque,
    errors: execution.totalErrors,
    id: execution.executionId,
    // Without a retained root there is no name to show, which is the normal
    // state of a run still in flight: the entry function is only known once
    // the root returns. A trimmed id stands in, because the full wire form
    // is 56 characters and would be the widest thing on screen. Naming it
    // after some other call it happened to retain would name the wrong
    // function.
    indexComplete: execution.indexState === 'complete',
    recordsLost: recordsWereLost(execution.indexState, execution.status),
    revision: execution.revisionId,
    sourceKind: sourceKindOf(execution.sourceLabel),
    spanCount: execution.callsRetained,
    startedMs: execution.startedAtMs,
    status: statusOf(execution.status),
    target: entry
      ? shortFunctionName(entry)
      : shortExecutionId(execution.executionId),
    valueState: execution.valueState,
  };
}

/**
 * Build the evidence one detail view renders.
 *
 * Timings are process-relative in the store, so everything here is rebased
 * onto the execution's own clock: offsets are from the first thread start.
 */
export function buildEvidence(
  telemetry: ExecutionTelemetry,
  options: EvidenceOptions = {},
): Evidence {
  const llmFunctions = options.llmFunctions ?? new Set<string>();

  const starts = [
    ...telemetry.threads.map((thread) => thread.startedNs),
    ...telemetry.calls.map((call) => call.startedNs),
  ].filter((value): value is number => value != null);
  const zeroNs = starts.length > 0 ? minOf(starts) : 0;
  const offsetMs = (ns: number | null): number =>
    ns == null ? 0 : (ns - zeroNs) / NS_PER_MS;

  const verification: SourceVerification =
    telemetry.execution?.sourceState ?? 'unverified';
  const contexts = telemetry.callPaths.map((path) =>
    toContext(path, llmFunctions, verification),
  );
  assignSubtreeAwait(contexts);
  const threadNames = new Map(
    telemetry.threads.map((thread) => [
      thread.threadId,
      threadLabel(thread, telemetry.threads),
    ]),
  );
  const spans = telemetry.calls.map((call) =>
    toSpan(call, offsetMs, threadNames, llmFunctions, verification),
  );
  reparentToRetainedAncestors(spans, telemetry.threads);

  const gaps = buildGaps(contexts, spans);
  const threads = telemetry.threads.map((thread) =>
    toThread(thread, offsetMs, telemetry.threads, verification),
  );

  // Self and await are disjoint parts of inclusive time per context, so
  // these sums attribute every nanosecond once. A total over contexts whose
  // share is unknown would claim a measurement that does not exist.
  const cpuNs = sumOf(telemetry.callPaths, (path) => path.selfNs);
  const awaitNs = sumOf(telemetry.callPaths, (path) => path.awaitNs);

  const rootPath = telemetry.callPaths.find(
    (path) => path.parentCallPathId == null && path.edgeKind === 'root',
  );
  const laneEnd = Math.max(
    maxOf(threads.map((lane) => lane.lastMs ?? lane.firstMs)),
    maxOf(spans.map((span) => span.startMs + (span.durationMs ?? 0))),
    0,
  );
  const durationMs =
    ms(telemetry.execution?.durationNs) ??
    ms(rootPath?.inclusiveNs) ??
    (laneEnd > 0 ? laneEnd : null);

  return {
    awaitMs: awaitNs == null ? null : awaitNs / NS_PER_MS,
    contexts,
    cpuMs: cpuNs == null ? null : cpuNs / NS_PER_MS,
    durationMs,
    errors: telemetry.errors.map((capture) =>
      toErrorCapture(capture, verification),
    ),
    gaps,
    indexComplete: telemetry.execution?.indexState === 'complete',
    recordsLost: recordsWereLost(
      telemetry.execution?.indexState ?? null,
      telemetry.execution?.status ?? null,
    ),
    retainedCalls: spans.length,
    spans,
    threads,
    timingComplete: contexts.every((context) => context.timingComplete),
    totalCalls:
      sumOf(contexts, (context) => context.enters) ??
      (contexts.length === 0 ? 0 : null),
    // Content ids are known; serving bodies over this connection is not
    // wired yet, and the inspectors distinguish that from "never captured".
    valuesReadable: false,
  };
}

/**
 * A total, or null when there is nothing to total or any part is unknown.
 * Skipping unknown parts would present a partial sum as the whole.
 */
export function sumOf<T>(
  rows: T[],
  pick: (row: T) => number | null,
): number | null {
  let total = 0;
  for (const row of rows) {
    const value = pick(row);
    if (value == null) return null;
    total += value;
  }
  return rows.length > 0 ? total : null;
}

/**
 * Parse a captured error for display.
 *
 * Providers commonly nest a raw response body as a JSON *string* inside the
 * error, so a single parse leaves the useful part as one escaped line. One
 * extra level is unwrapped where a string field is itself valid JSON, which
 * is what turns an unreadable blob into the provider's actual message.
 */
function parseErrorValue(raw: string | null): unknown {
  if (raw == null) return null;
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    // Not JSON: the rendered form is the best available answer.
    return raw;
  }
  if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) {
    const out: Record<string, unknown> = {};
    for (const [key, value] of Object.entries(parsed)) {
      if (typeof value === 'string') {
        const trimmed = value.trim();
        if (trimmed.startsWith('{') || trimmed.startsWith('[')) {
          try {
            out[key] = JSON.parse(trimmed);
            continue;
          } catch {
            // Looked like JSON and was not; keep the original string.
          }
        }
      }
      out[key] = value;
    }
    return out;
  }
  return parsed;
}

function toErrorStep(
  step: TelemetryErrorStep,
  verification: SourceVerification,
): ErrorStep {
  return {
    fn: step.fqn ? shortFunctionName(step.fqn) : null,
    handlerFn: step.handlerFqn ? shortFunctionName(step.handlerFqn) : null,
    kind: step.kind,
    raiseId: step.raiseId,
    result: step.result,
    source: sourceRef(
      step.site.file,
      step.site.line,
      step.site.start,
      step.site.end,
      verification,
    ),
    threadId: step.threadId,
  };
}

function toErrorCapture(
  capture: TelemetryErrorCapture,
  verification: SourceVerification,
): ErrorCapture {
  const unavailable = availability(capture.valueState, capture.valueCid);
  // Backends that predate `grain` only ever sent throw captures.
  const erroredCall = capture.grain === 'errored_call';
  const fqn = erroredCall ? (capture.callFqn ?? null) : capture.throwFqn;
  const raiseKind = capture.raiseKind ?? null;
  return {
    boundary: raiseKind === 'native_boundary' || raiseKind === 'host_boundary',
    callId: erroredCall ? (capture.callId ?? null) : capture.throwCallId,
    failedCallIds: capture.failedCallIds ?? [],
    fn: fqn ? shortFunctionName(fqn) : null,
    grain: erroredCall ? 'erroredCall' : 'throw',
    handlerFn: capture.handlerFqn
      ? shortFunctionName(capture.handlerFqn)
      : null,
    id: capture.errorId,
    inheritedStack: capture.inheritedStack ?? [],
    kind: capture.kind,
    originCandidates: capture.originCandidates ?? null,
    originState: capture.originState ?? null,
    propagation: (capture.propagation ?? []).map((step) =>
      toErrorStep(step, verification),
    ),
    raiseKind,
    source: capture.source,
    source_location: sourceRef(
      capture.throwSiteFile,
      capture.throwSiteLine,
      capture.throwSiteStart ?? null,
      capture.throwSiteEnd ?? null,
      verification,
    ),
    stack: capture.stack,
    stackComplete: capture.stackComplete ?? false,
    threadId: erroredCall
      ? (capture.callThreadId ?? null)
      : capture.throwThreadId,
    unresolvedReason: capture.unresolvedReason ?? null,
    unwindResult: capture.unwindResult ?? null,
    value: parseErrorValue(capture.value),
    valueUnavailable: capture.value == null ? unavailable : null,
  };
}

function toContext(
  path: TelemetryCallPath,
  llmFunctions: ReadonlySet<string>,
  verification: SourceVerification,
): ContextNode {
  const folded = path.overflowReason != null;
  return {
    awaitMs: ms(path.awaitNs),
    cancelled: path.completedCancelled,
    // BTEL recordings carry completed counts, not starts. Neither is null
    // turned into zero: an unknown count is not "no calls".
    enters: path.callsStarted ?? path.completedCalls ?? null,
    errors: path.completedError,
    fn: path.fqn
      ? shortFunctionName(path.fqn)
      : folded
        ? (path.overflowReason ?? 'folded contexts')
        : `function#${path.callPathId}`,
    folded,
    fqn: path.fqn,
    id: path.callPathId,
    kind: callKind(path.fqn, path.edgeKind, llmFunctions),
    ok: path.completedOk,
    outcomeState: path.outcomeState ?? null,
    parentId: path.parentCallPathId,
    selfMs: ms(path.selfNs),
    source: sourceRef(
      path.callSiteFile,
      path.callSiteLine,
      path.callSiteStart,
      path.callSiteEnd,
      verification,
    ),
    spawn: path.edgeKind === 'spawn',
    // Filled in once the whole tree is known.
    subtreeAwaitMs: null,
    timingComplete: path.timingComplete ?? false,
    totalMs: ms(path.inclusiveNs),
  };
}

/**
 * Roll each path's waiting up to its ancestors.
 *
 * Walking children upward is what makes waiting attributable at all: the
 * frame that suspends is a transport leaf, so without this every question
 * about time spent waiting has the answer zero for every function anyone
 * actually wrote.
 */
function assignSubtreeAwait(contexts: ContextNode[]): void {
  const childrenOf = new Map<string, ContextNode[]>();
  for (const context of contexts) {
    if (context.parentId == null) continue;
    const list = childrenOf.get(context.parentId);
    if (list) list.push(context);
    else childrenOf.set(context.parentId, [context]);
  }

  // Computed totals, so a node reached twice returns its real total rather
  // than zero. An earlier version used one `Set` for both the cycle guard
  // and the visited marker: a child processed before its parent by the
  // outer loop then contributed 0 upward, silently dropping that whole
  // subtree's waiting. `CALL_PATHS_SQL` has no ORDER BY, so which rows hit
  // that path was down to whatever order the query returned.
  const computed = new Map<string, number | null>();
  const inProgress = new Set<string>();

  const total = (context: ContextNode): number | null => {
    const done = computed.get(context.id);
    if (done !== undefined) return done;
    // A cycle would be a malformed tree; contributing zero for the back
    // edge keeps the walk finite without hanging the panel.
    if (inProgress.has(context.id)) return 0;
    inProgress.add(context.id);

    // Any unknown wait below makes the subtree's total unknown.
    let sum: number | null = context.awaitMs;
    for (const child of childrenOf.get(context.id) ?? []) {
      const childSum = total(child);
      sum = sum == null || childSum == null ? null : sum + childSum;
    }

    inProgress.delete(context.id);
    computed.set(context.id, sum);
    context.subtreeAwaitMs = sum;
    return sum;
  };

  for (const context of contexts) total(context);
}

function sourceRef(
  file: string | null,
  line: number | null,
  start: number | null,
  end: number | null,
  verification: SourceVerification,
): SourceRef | null {
  return file ? { end, file, line, start, verification } : null;
}

function toSpan(
  call: TelemetryCall,
  offsetMs: (ns: number | null) => number,
  threadNames: Map<string, string>,
  llmFunctions: ReadonlySet<string>,
  verification: SourceVerification,
): SpanNode {
  return {
    contextId: call.callPathId,
    durationMs: ms(call.durationNs),
    errorId: call.errorId,
    fn: call.fqn ? shortFunctionName(call.fqn) : `function#${call.callId}`,
    id: call.callId,
    kind: callKind(call.fqn, call.edgeKind, llmFunctions),
    parentId: call.parentCallId,
    reasons: call.selectionReasons,
    // Null for a direct recursive call: its own site is not recorded, and
    // the enclosing call's site would be the wrong expression.
    source: sourceRef(
      call.callSiteFile,
      call.callSiteLine,
      null,
      null,
      verification,
    ),
    startMs: offsetMs(call.startedNs),
    status: statusOf(call.status),
    threadId: call.threadId,
    threadName: call.threadId
      ? (threadNames.get(call.threadId) ?? call.threadId)
      : '',
    values: {
      args: capturedValue(call.args, call.argsState, call.argsCid),
      error: capturedValue(call.error, call.errorState, call.errorCid),
      output: capturedValue(call.output, call.outputState, call.outputCid),
    },
  };
}

/** True when a hydrated value carries media anywhere inside it. */
export function holdsMedia(body: unknown): boolean {
  if (body == null || typeof body !== 'object') return false;
  if (Array.isArray(body)) return body.some(holdsMedia);
  const record = body as Record<string, unknown>;
  if (typeof record.$media === 'string') return true;
  return Object.values(record).some(holdsMedia);
}

function capturedValue(
  raw: string | null,
  state: string | null,
  cid: string | null,
): CapturedValue {
  const body = parseErrorValue(raw);
  return {
    availability: availability(state, cid),
    body,
    // Media bytes live behind the value's own content id: without one there
    // is nothing to fetch, however the descriptor renders.
    mediaCid: cid != null && holdsMedia(body) ? cid : null,
  };
}

/**
 * Point each span at its nearest *retained* ancestor.
 *
 * Capture policy keeps calls, not subtrees, so a span's recorded parent is
 * often summary-only and absent from the retained set. Its ancestors are
 * absent too, so the chain cannot simply be walked upward: under a policy
 * that keeps roots and LLM calls, a nested model call would otherwise
 * render as a sibling of the root and the trace would read as a flat
 * forest of unrelated stacks.
 *
 * Containment recovers the nesting instead. A logical thread's calls form a
 * proper stack -- awaiting suspends a frame but never pops it -- so the
 * innermost retained span whose interval encloses this one is its nearest
 * retained ancestor. A span on a spawned thread has no enclosing span on
 * its own lane, so it falls back to the call that spawned the thread.
 *
 * What gets skipped over is not hidden: it is exactly what the gap counts
 * report.
 */
function reparentToRetainedAncestors(
  spans: SpanNode[],
  threads: TelemetryThread[],
): void {
  const retained = new Set(spans.map((span) => span.id));
  const spawnCallByThread = new Map(
    threads
      .filter((thread) => thread.spawnCallId != null)
      .map((thread) => [thread.threadId, thread.spawnCallId as string]),
  );

  const endOf = (span: SpanNode): number =>
    // A running span has no end yet, so it encloses everything after it
    // started rather than nothing.
    span.durationMs == null
      ? Number.POSITIVE_INFINITY
      : span.startMs + span.durationMs;

  const byThread = new Map<string, SpanNode[]>();
  for (const span of spans) {
    const key = span.threadId ?? '';
    const list = byThread.get(key);
    if (list) list.push(span);
    else byThread.set(key, [span]);
  }

  // Sweep each lane with a stack rather than scanning every other span on
  // it. Comparing each span against all its lane-mates is quadratic, which
  // is fine for the handful of spans a root-only policy keeps and hangs the
  // panel outright once a run retains thousands.
  const containerOf = new Map<string, string | null>();
  for (const lane of byThread.values()) {
    const ordered = [...lane].sort(
      (left, right) =>
        left.startMs - right.startMs || endOf(right) - endOf(left),
    );
    const open: SpanNode[] = [];
    for (const span of ordered) {
      // Anything ending before this span does cannot contain it. Ordering
      // by start means the survivor on top is the innermost container.
      let top = open.at(-1);
      while (top !== undefined && endOf(top) < endOf(span)) {
        open.pop();
        top = open.at(-1);
      }
      containerOf.set(span.id, top?.id ?? null);
      open.push(span);
    }
  }

  for (const span of spans) {
    if (span.parentId != null && retained.has(span.parentId)) continue;

    const container = containerOf.get(span.id) ?? null;
    if (container != null && container !== span.id) {
      span.parentId = container;
      continue;
    }

    const spawnCallId = span.threadId
      ? spawnCallByThread.get(span.threadId)
      : undefined;
    span.parentId =
      spawnCallId != null &&
      spawnCallId !== span.id &&
      retained.has(spawnCallId)
        ? spawnCallId
        : null;
  }
}

/**
 * Work the counts prove happened that no span shows.
 *
 * This is a subtraction, never a stored number: a context that ran 41 times
 * with 2 retained spans has a gap of 39. Folded rows are skipped because
 * they already stand for calls no context kept separately, so counting them
 * again would double-report the same work.
 */
function buildGaps(contexts: ContextNode[], spans: SpanNode[]): GapInfo[] {
  const retainedByContext = new Map<string, number>();
  for (const span of spans) {
    if (!span.contextId) continue;
    retainedByContext.set(
      span.contextId,
      (retainedByContext.get(span.contextId) ?? 0) + 1,
    );
  }
  const gaps: GapInfo[] = [];
  for (const context of contexts) {
    // Without a count there is no subtraction to make.
    if (context.folded || context.enters == null) continue;
    const retained = retainedByContext.get(context.id) ?? 0;
    const missing = context.enters - retained;
    if (missing > 0) {
      gaps.push({
        calls: missing,
        contextId: context.id,
        fn: context.fn,
        id: `gap:${context.id}`,
      });
    }
  }
  return gaps;
}

function threadLabel(
  thread: TelemetryThread,
  threads: TelemetryThread[],
): string {
  if (thread.name) return thread.name;
  if (thread.kind === 'root') return 'main';
  // Unnamed spawns get a stable ordinal rather than their raw id, which is
  // a long opaque token that would swamp the column.
  const spawns = threads
    .filter((other) => other.kind !== 'root')
    .sort((left, right) => (left.startedNs ?? 0) - (right.startedNs ?? 0));
  const index = spawns.findIndex((other) => other.threadId === thread.threadId);
  return index >= 0 ? `thread-${index + 1}` : thread.threadId;
}

function toThread(
  thread: TelemetryThread,
  offsetMs: (ns: number | null) => number,
  threads: TelemetryThread[],
  verification: SourceVerification,
): ThreadLane {
  const parent = thread.parentThreadId
    ? threads.find((other) => other.threadId === thread.parentThreadId)
    : undefined;
  return {
    awaitMs: null,
    busyMs: null,
    firstMs: offsetMs(thread.startedNs),
    id: thread.threadId,
    lastMs: thread.endedNs == null ? null : offsetMs(thread.endedNs),
    name: threadLabel(thread, threads),
    spawnedBy: thread.parentThreadId
      ? {
          atMs: thread.startedNs == null ? null : offsetMs(thread.startedNs),
          fn: thread.spawnFqn ? shortFunctionName(thread.spawnFqn) : null,
          thread: parent ? threadLabel(parent, threads) : thread.parentThreadId,
        }
      : null,
    spawnSite: sourceRef(
      thread.spawnSiteFile,
      thread.spawnSiteLine,
      null,
      null,
      verification,
    ),
    status: threadStatusOf(thread.endStatus),
  };
}
