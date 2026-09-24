/**
 * Contract section 10.3: why an object on the timeline happened.
 *
 * Every object that the timeline draws can name what made it happen, in one
 * plain sentence, and where in the program it happened. This module holds the
 * rules. It is pure: it takes the layout item, the run records, and the event
 * lists, and returns text and at most one source location. The React component
 * renders the result and turns the location into a link.
 *
 * Three sources of a location, in this order:
 *
 * 1. The worker's own field (`remote_call.file/line`, `remote_cancel.file/line`,
 *    `thread_started.file/line` of section 10.1). Exact.
 * 2. The run record, which keeps the call site of every entry in `waiting_on`
 *    and `calls` (section 10.2). Exact, and it survives a resume and a reload.
 * 3. The last `position` event of the thread before the moment in question.
 *    Marked approximate, because it is the last line the thread reported and
 *    not the line of the object itself.
 *
 * When none of the three answers, the panel says so. It never shows a location
 * that it cannot attribute.
 */

import {
  baseName,
  cancelCauseOf,
  locationOf,
  type CancelCause,
  type Json,
  type PositionEvent,
  type RemoteCallEvent,
  type RemoteCancelEvent,
  type Run,
  type Site,
  type SourceLocation,
  type StateDump,
  type ThreadStartedEvent,
} from "../protocol";
import type { TimelineEvent } from "../state";
import { formatClock, formatCountdown, formatMs } from "../format";
import type { Connector, Gap, Marker, SegmentBar, ThreadBar, WaitInterval } from "./layout";

/** The object that the detail panel describes. */
export type DetailTarget =
  | { kind: "marker"; item: Marker }
  | { kind: "segment"; item: SegmentBar }
  | { kind: "gap"; item: Gap }
  | { kind: "thread"; item: ThreadBar }
  | { kind: "wait"; item: WaitInterval }
  | { kind: "connector"; item: Connector };

/** A location that the panel offers as a link into the source view. */
export interface CauseLocation extends SourceLocation {
  /** The site whose `GET /api/source` serves the file. */
  site: Site;
  /**
   * True when the location is the last `position` of a thread rather than the
   * location of the object itself. The panel says so next to the link.
   */
  approximate: boolean;
  /** What the location stands for, for example `the call site` or `thread 1 last reported`. */
  what: string;
}

export interface Cause {
  /** The heading of the panel. */
  title: string;
  /** One sentence a person can read. */
  sentence: string;
  /** Rows under the sentence. */
  rows: [string, string][];
  /** The source location to link to, or `null`. */
  location: CauseLocation | null;
  /** Why there is no location. Set exactly when `location` is `null` and one was expected. */
  missing: string | null;
  /** Extra prose under the rows. */
  note?: string;
}

export interface CauseContext {
  /** The run records of the tree. */
  runs: readonly Run[];
  /** Event lists by run key, as the app state keeps them. */
  events: Readonly<Record<string, readonly TimelineEvent[] | undefined>>;
  /**
   * The state dump of the snapshot that is selected, when it is loaded. The
   * cause of a snapshot marker names its top user frame (section 10.3).
   */
  snapshot?: { site: Site; run: string; n: number; dump: StateDump } | null;
}

// ---------------------------------------------------------------------------
// Lookups
// ---------------------------------------------------------------------------

/**
 * Every event of a run id, over every site that held it, in time order. The
 * result is cached per event map, because the panel asks for it several times
 * while one object is selected and the map is replaced on every change.
 */
const mergedEvents = new WeakMap<object, Map<string, readonly TimelineEvent[]>>();

function runEvents(ctx: CauseContext, run: string): readonly TimelineEvent[] {
  let cache = mergedEvents.get(ctx.events);
  if (cache === undefined) {
    cache = new Map();
    mergedEvents.set(ctx.events, cache);
  }
  const cached = cache.get(run);
  if (cached !== undefined) return cached;
  const collected: TimelineEvent[] = [];
  for (const [key, list] of Object.entries(ctx.events)) {
    const slash = key.indexOf("/");
    if (slash <= 0 || key.slice(slash + 1) !== run || list === undefined) continue;
    collected.push(...list);
  }
  collected.sort((a, b) => a.ts - b.ts);
  cache.set(run, collected);
  return collected;
}

function recordOf(ctx: CauseContext, site: Site, run: string): Run | undefined {
  return ctx.runs.find((candidate) => candidate.site === site && candidate.id === run);
}

/** Every record of a run id: the run may have moved between sites. */
function recordsOf(ctx: CauseContext, run: string): Run[] {
  return ctx.runs.filter((candidate) => candidate.id === run);
}

function remoteCallEvent(ctx: CauseContext, run: string, callId: string): RemoteCallEvent | null {
  for (const event of runEvents(ctx, run)) {
    if (event.type === "remote_call" && event.call_id === callId) return event;
  }
  return null;
}

function remoteCancelEvent(ctx: CauseContext, run: string, callId: string): RemoteCancelEvent | null {
  for (const event of runEvents(ctx, run)) {
    if (event.type === "remote_cancel" && event.call_id === callId) return event;
  }
  return null;
}

function threadStartedEvent(ctx: CauseContext, run: string, segment: number, thread: number): ThreadStartedEvent | null {
  for (const event of runEvents(ctx, run)) {
    if (event.type === "thread_started" && event.segment === segment && event.thread === thread) return event;
  }
  return null;
}

/**
 * The call site that the run record remembers for `callId` (section 10.2). It
 * outlives the events of the segment that made the call, so it answers after a
 * resume and after a reload that did not backfill the history.
 */
function recordedCallSite(ctx: CauseContext, run: string, callId: string): SourceLocation | null {
  for (const record of recordsOf(ctx, run)) {
    const waiting = record.waiting_on.find((entry) => entry.call_id === callId);
    const located = locationOf(waiting);
    if (located) return located;
    const call = (record.calls ?? []).find((entry) => entry.call_id === callId);
    const fromCall = locationOf(call);
    if (fromCall) return fromCall;
  }
  return null;
}

/** The name of the callee that the run record remembers for `callId`. */
function recordedFunction(ctx: CauseContext, run: string, callId: string): string | null {
  for (const record of recordsOf(ctx, run)) {
    const waiting = record.waiting_on.find((entry) => entry.call_id === callId);
    if (waiting) return waiting.function;
    const call = (record.calls ?? []).find((entry) => entry.call_id === callId);
    if (call) return call.function;
  }
  return null;
}

/**
 * The calling function that the run record remembers for `callId` (section
 * 10.3). A resumed run emits `remote_wait` and no `remote_call` (section 9.7),
 * so for such a call the record is the only evidence left.
 */
function recordedCaller(ctx: CauseContext, run: string, callId: string): string | null {
  for (const record of recordsOf(ctx, run)) {
    const waiting = record.waiting_on.find((entry) => entry.call_id === callId);
    if (typeof waiting?.caller === "string" && waiting.caller !== "") return waiting.caller;
    const call = (record.calls ?? []).find((entry) => entry.call_id === callId);
    if (typeof call?.caller === "string" && call.caller !== "") return call.caller;
  }
  return null;
}

/**
 * The last `position` that `thread` reported at or before `ts`. `thread` is
 * `null` for "any thread of the run", which is what a run-wide object such as a
 * migration or a snapshot falls back to.
 */
export function lastPositionBefore(
  ctx: CauseContext,
  run: string,
  ts: number,
  thread: number | null,
  segment?: number,
): PositionEvent | null {
  let best: PositionEvent | null = null;
  for (const event of runEvents(ctx, run)) {
    if (event.type !== "position" || event.ts > ts) continue;
    if (thread !== null && event.thread !== thread) continue;
    if (segment !== undefined && event.segment !== segment) continue;
    best = event;
  }
  return best;
}

/**
 * The last `position` with the sys-op `baml.sys.sleep` before `ts`, in
 * `segment`. It is the last sleep that ANY thread of the segment reported, not
 * the sleep of one particular wait: a run whose threads sleep concurrently
 * reports several, and the worker emits none at all for a sleep whose line
 * equals the thread's previous one. The caller therefore treats the result as
 * approximate.
 */
function lastSleepPosition(ctx: CauseContext, run: string, ts: number, segment?: number): PositionEvent | null {
  let best: PositionEvent | null = null;
  for (const event of runEvents(ctx, run)) {
    if (event.type !== "position" || event.ts > ts) continue;
    if (segment !== undefined && event.segment !== segment) continue;
    if (event.op !== "baml.sys.sleep") continue;
    best = event;
  }
  return best;
}

/**
 * A frame of a state dump that belongs to the program, not to the stdlib, with
 * the thread it belongs to. A phase 3 snapshot holds every live thread, so the
 * frame is one thread's position and not the position of the whole run: the
 * panel names the thread.
 */
function topUserFrame(dump: StateDump): { thread: number; function: string; file: string; line: number } | null {
  for (const thread of dump.threads) {
    for (const frame of thread.frames) {
      if (!frame.file.startsWith("<builtin>")) return { thread: thread.thread, ...frame };
    }
  }
  return null;
}

// ---------------------------------------------------------------------------
// Locations
// ---------------------------------------------------------------------------

export function exactLocation(site: Site, location: SourceLocation, what: string): CauseLocation {
  return { ...location, site, approximate: false, what };
}

export function approximateLocation(site: Site, position: PositionEvent | null, what: string): CauseLocation | null {
  if (position === null) return null;
  return { site, file: position.file, line: position.line, approximate: true, what };
}

/** `quotes.baml:41`, the label of a location link. */
export function locationLabel(location: SourceLocation): string {
  return `${baseName(location.file)}:${location.line}`;
}

// ---------------------------------------------------------------------------
// Words
// ---------------------------------------------------------------------------

/** Section 10.1: one readable sentence per `cause` value of `remote_cancel`. */
export const CANCEL_CAUSE_TEXT: Record<CancelCause, string> = {
  future_cancel: "cancelled because the future was cancelled (a race loser, or Future.cancel)",
  token: "cancelled because a cancel token fired (the deadline of with_timeout, or a token of the program)",
  parent: "cancelled because an ancestor thread was cancelled, which cancels its children",
  // `unknown` is "I do not know", not "the run cancelled it": the worker also
  // writes it for every call its end-of-run sweep abandons, whatever ended the
  // run. The sentence says only what the worker said.
  unknown:
    "cancelled, and the worker could not say what fired: the run itself may have been cancelled, or the thread held no link the engine can read",
};

/** The short label of a `cause` value, for the rows under the sentence. */
export const CANCEL_CAUSE_LABEL: Record<CancelCause, string> = {
  future_cancel: "a cancelled future",
  token: "a cancel token",
  parent: "a cancelled parent",
  unknown: "not reported",
};

const SEGMENT_END_TEXT: Record<SegmentBar["endKind"], string> = {
  open: "the process is still running",
  paused: "it wrote a snapshot and exited",
  completed: "it ran the function to its end",
  failed: "the function threw",
  cancelled: "it was cancelled",
  lost: "the process was lost and left no snapshot",
  killed: "the process ended without a terminal event",
};

/**
 * The name of the function that a frame belongs to. A worker names the body of
 * a `spawn` after the function that contains it (`durable_fan_out.<spawn>`).
 * The sentence names the function, not the engine's frame.
 */
const callerName = (fn: string): string => fn.replace(/\.<[a-z_]+>$/, "");

const shortJson = (value: Json, limit = 70): string => {
  const text = JSON.stringify(value) ?? "null";
  return text.length <= limit ? text : `${text.slice(0, limit - 1)}…`;
};

// ---------------------------------------------------------------------------
// The cause of each kind of object
// ---------------------------------------------------------------------------

/** The cause of a remote call, shared by the call arrow and the wait band. */
/**
 * How long a remote call took, from the caller's side. `launch` is the part
 * the demo is about: the interval between the parent yielding the call and the
 * child's process announcing itself on the other machine, which covers the
 * dispatch between the sites, the worker start and the program load.
 */
function callTimings(
  ctx: CauseContext,
  parentRun: string,
  callId: string,
  childRun: string | null,
): [string, string][] {
  const call = remoteCallEvent(ctx, parentRun, callId);
  if (call === null) return [];
  const rows: [string, string][] = [];
  if (childRun !== null) {
    const child = runEvents(ctx, childRun);
    const hello = child.find((event) => event.type === "hello") ?? null;
    if (hello !== null) {
      rows.push(["launch", formatMs(hello.ts - call.ts)]);
      const done = child.find((event) => event.type === "completed" || event.type === "failed");
      if (done !== undefined) rows.push(["child ran for", formatMs(done.ts - hello.ts)]);
    }
  }
  // What the calling thread actually waited. It is not launch plus run time
  // when the parent had no process for part of the call: the result then waits
  // in the parent's run store until a new process takes it.
  const taken = runEvents(ctx, parentRun).find(
    (event) => event.type === "remote_result_received" && event.call_id === callId,
  );
  if (taken !== undefined) rows.push(["result taken after", formatMs(taken.ts - call.ts)]);
  return rows;
}

function callCause(ctx: CauseContext, site: Site, run: string, callId: string, childSite: Site | null, childRun: string | null): Cause {
  const event = remoteCallEvent(ctx, run, callId);
  const fn = event?.function ?? recordedFunction(ctx, run, callId) ?? "a remote function";
  const thread = event?.thread ?? null;
  const fromEvent = locationOf(event);
  const fromRecord = fromEvent === null ? recordedCallSite(ctx, run, callId) : null;
  const exact = fromEvent ?? fromRecord;
  const position = exact === null && event !== null ? lastPositionBefore(ctx, run, event.ts, thread, event.segment) : null;
  // The calling function is evidence, not a guess. The worker reports it, the
  // run record keeps it, and the `position` of the calling thread names the
  // frame the call was made in. The run's entry function is none of those: it
  // is a different function whenever the call is written in a helper.
  const reported =
    (typeof event?.caller === "string" && event.caller !== "" ? event.caller : null) ??
    recordedCaller(ctx, run, callId) ??
    (event === null ? null : lastPositionBefore(ctx, run, event.ts, thread, event.segment)?.function) ??
    null;
  const caller = reported === null ? null : callerName(reported);
  const location =
    exact !== null
      ? exactLocation(site, exact, fromEvent === null ? "the call site, from the run record" : "the call site")
      : approximateLocation(site, position, `the last line thread ${thread ?? "?"} reported before the call`);
  const where = location === null ? "" : ` at ${locationLabel(location)}`;
  const placed = childSite === null ? "" : `, placed on ${childSite}`;
  const rows: [string, string][] = [
    ["call", callId],
    ["callee", fn],
    ...(caller === null ? [] : ([["calling function", caller]] as [string, string][])),
    ...(thread === null ? [] : ([["calling thread", String(thread)]] as [string, string][])),
    ...(childRun === null ? [] : ([["child run", `${childSite}/${childRun}`]] as [string, string][])),
    ...callTimings(ctx, run, callId, childRun),
    ...(event === null ? [] : ([["arguments", shortJson(event.args)]] as [string, string][])),
  ];
  return {
    title: "remote call",
    sentence: caller === null ? `${fn} was called${where}${placed}.` : `${caller} called ${fn}${where}${placed}.`,
    rows,
    location,
    missing:
      location === null
        ? "The worker reported no call site, the run record keeps none, and the calling thread reported no position before the call."
        : null,
  };
}

function returnCause(ctx: CauseContext, connector: Connector): Cause {
  const callId = connector.callId ?? "";
  const parentRun = connector.to.run;
  const parentSite = connector.to.site;
  const fn = recordedFunction(ctx, parentRun, callId) ?? remoteCallEvent(ctx, parentRun, callId)?.function ?? "the remote call";
  const outcome = connector.ok === false ? "failed" : "succeeded";
  let received: { ts: number; thread: number; segment: number } | null = null;
  /** When the result reached the parent's site. */
  let arrived: number | null = null;
  /** The start of each segment of the parent, from its `hello` events. */
  const segmentStart = new Map<number, number>();
  for (const event of runEvents(ctx, parentRun)) {
    if (event.type === "remote_result_received" && event.call_id === callId) {
      received = { ts: event.ts, thread: event.thread, segment: event.segment };
    }
    if (event.type === "remote_returned" && event.call_id === callId) arrived = event.ts;
    if (event.type === "hello" && !segmentStart.has(event.segment)) segmentStart.set(event.segment, event.ts);
  }
  // The result reached the site before the process that took it existed: the
  // run was suspended, and the resume delivered the stored result.
  const takenAtResume =
    received !== null && arrived !== null && (segmentStart.get(received.segment) ?? -Infinity) > arrived;
  const rows: [string, string][] = [
    ...(callId === "" ? [] : ([["call", callId]] as [string, string][])),
    ["child", `${connector.from.site}/${connector.from.run}`],
    ["result", connector.ok === false ? "error" : "ok"],
    ["latency", formatMs(connector.to.t - connector.from.t)],
  ];
  if (connector.pending || received === null || takenAtResume) {
    return {
      title: "remote result",
      sentence: `${fn} ${outcome} on ${connector.from.site}. The result was delivered while the run had no process, taken at resume.`,
      rows: [
        ...rows,
        ["delivery", connector.pending ? "stored until the resume" : takenAtResume ? "stored, taken at the resume" : "not taken yet"],
        ...(received === null ? [] : ([["taken by", `thread ${received.thread} in segment ${received.segment}`]] as [string, string][])),
      ],
      location: null,
      missing:
        "The parent had no process when the result arrived, so no line of the program took it. The site server stored it and the resume delivered it before the threads ran.",
    };
  }
  const position = lastPositionBefore(ctx, parentRun, received.ts, received.thread, received.segment);
  const location = approximateLocation(parentSite, position, `the last line thread ${received.thread} reported before it took the result`);
  return {
    title: "remote result",
    sentence:
      `${fn} ${outcome} on ${connector.from.site}, and thread ${received.thread} of the parent took the result` +
      `${location === null ? "" : ` at ${locationLabel(location)}`}.`,
    rows: [...rows, ["taken by", `thread ${received.thread} in segment ${received.segment}`], ["delivery", "delivered to the process"]],
    location,
    missing: location === null ? "The thread that took the result reported no position in this segment." : null,
  };
}

function cancelCause(
  ctx: CauseContext,
  site: Site,
  run: string,
  callId: string,
  source: "worker" | "site",
  child: { site: Site | null; run: string | null },
): Cause {
  const event = remoteCancelEvent(ctx, run, callId);
  const cause = cancelCauseOf(event?.cause);
  const fn = recordedFunction(ctx, run, callId) ?? remoteCallEvent(ctx, run, callId)?.function ?? "a remote call";
  const fromEvent = locationOf(event);
  const fromRecord = fromEvent === null ? recordedCallSite(ctx, run, callId) : null;
  const exact = fromEvent ?? fromRecord;
  const thread = event?.thread ?? null;
  const position =
    exact === null && event !== null ? lastPositionBefore(ctx, run, event.ts, thread, event.segment) : null;
  const location =
    exact !== null
      ? exactLocation(site, exact, "the call site of the cancelled call")
      : approximateLocation(site, position, `the last line thread ${thread ?? "?"} reported`);
  const reason =
    source === "site" && event === null
      ? "The run ended, and its site cancelled the remote children that were still running."
      : cause === null
        ? "The worker did not say why the call was cancelled."
        : `${fn} was ${CANCEL_CAUSE_TEXT[cause]}.`;
  const where = location === null ? "" : ` The call was made at ${locationLabel(location)}.`;
  return {
    title: "remote cancel",
    sentence: `${reason}${where}`,
    rows: [
      ["call", callId],
      ["callee", fn],
      ["cause", cause === null ? (source === "site" ? "the run ended" : "not reported") : CANCEL_CAUSE_LABEL[cause]],
      ...(thread === null ? [] : ([["cancelled thread", String(thread)]] as [string, string][])),
      ...(child.run === null ? [] : ([["child run", `${child.site}/${child.run}`]] as [string, string][])),
    ],
    location,
    missing:
      location === null
        ? "Neither the worker nor the run record names the call site, and the thread reported no position."
        : null,
    note:
      source === "site" && event === null
        ? undefined
        : "The site server removes the entry from `waiting_on`, cancels the child on its machine, and discards a result that arrives later.",
  };
}

function migrationCause(ctx: CauseContext, connector: Connector): Cause {
  const run = connector.from.run;
  const from = connector.from.site;
  const to = connector.to.site;
  const position = lastPositionBefore(ctx, run, connector.from.t, null);
  const location = approximateLocation(from, position, `the last line ${run} reported on ${from}`);
  const record = recordOf(ctx, from, run);
  const snapshot = (record?.snapshots ?? []).filter((entry) => entry.ts <= connector.from.t + 1).at(-1) ?? null;
  return {
    title: "migration",
    sentence:
      `A "Resume on ${to}" command moved ${run} from ${from} to ${to}. Its snapshot travelled with it` +
      `${location === null ? "" : `, and it was taken at ${locationLabel(location)}`}.`,
    rows: [
      ["action", `resume on ${to}`],
      ["from", `${from} at ${formatClock(connector.from.t)}`],
      ["to", `${to} at ${formatClock(connector.to.t)}`],
      ["transfer", formatMs(connector.to.t - connector.from.t)],
      ...(snapshot === null ? [] : ([["snapshot", `#${snapshot.n}`]] as [string, string][])),
    ],
    location,
    missing: location === null ? `No position was recorded for ${run} on ${from}, so the frame cannot be named.` : null,
    note: `The record on ${from} stays behind with the status migrated, and a result that arrives there is forwarded to ${to}.`,
  };
}

function forkCause(ctx: CauseContext, site: Site, forkRun: string, sourceRun: string, n: number, at: number): Cause {
  const source = recordOf(ctx, site, sourceRun);
  const snapshot = (source?.snapshots ?? []).find((entry) => entry.n === n) ?? null;
  const takenAt = snapshot?.ts ?? at;
  const position = lastPositionBefore(ctx, sourceRun, takenAt, null);
  const location = approximateLocation(site, position, `the last line ${sourceRun} reported before the snapshot`);
  return {
    title: "fork",
    sentence:
      `${forkRun} was forked from snapshot #${n} of ${sourceRun}` +
      `${location === null ? "" : `, which was taken at ${locationLabel(location)}`}.`,
    rows: [
      ["forked from", `${site}/${sourceRun}`],
      ["snapshot", `#${n}`],
      ...(snapshot === null ? [] : ([["snapshot written", formatClock(snapshot.ts)]] as [string, string][])),
      ["fork created", formatClock(at)],
      ...(snapshot === null ? [] : ([["snapshot age at the fork", formatMs(at - snapshot.ts)]] as [string, string][])),
    ],
    location,
    missing: location === null ? `No position was recorded for ${sourceRun}, so the frame of the snapshot cannot be named.` : null,
    note: "The fork has its own copy of the snapshot. It waits on the same children as its source and never cancels them.",
  };
}

function sleepCause(ctx: CauseContext, gap: Gap, now: number): Cause {
  // Only the segment that wrote this gap's snapshot can have reported the
  // sleep the run suspended itself for. A later segment's sleep is a different
  // wait, and an earlier one was already over.
  const segment = (recordOf(ctx, gap.site, gap.run)?.snapshots ?? []).find((entry) => entry.n === gap.snapshotN)?.segment;
  const sleep = lastSleepPosition(ctx, gap.run, gap.start + 1, segment);
  const location =
    sleep === null
      ? approximateLocation(gap.site, lastPositionBefore(ctx, gap.run, gap.start + 1, null, segment), `the last line ${gap.run} reported`)
      : approximateLocation(gap.site, sleep, "the last sleep the run reported before it suspended");
  const wake = gap.wakeAt === null ? null : formatClock(gap.wakeAt);
  return {
    title: "sleeping, no process",
    sentence:
      `The run suspended itself for a sleep${location === null ? "" : ` at ${locationLabel(location)}`}` +
      `${wake === null ? "." : ` and wakes at ${wake}.`}`,
    rows: [
      ["state lives in", gap.snapshotN === null ? "a snapshot" : `snapshot #${gap.snapshotN}`],
      ["wake at", wake ?? "n/a"],
      gap.open
        ? ["wakes in", gap.wakeAt === null ? "n/a" : formatCountdown(gap.wakeAt - now)]
        : ["slept for", formatMs(gap.end - gap.start)],
      ...(gap.wakeReason ? ([["woken by", gap.wakeReason]] as [string, string][]) : []),
    ],
    location,
    missing: location === null ? "No sleep was reported for this segment, so the call site cannot be named." : null,
    note: "No process and no memory are held while the run sleeps. A timer of the site server resumes it.",
  };
}

function snapshotCause(ctx: CauseContext, marker: Extract<Marker, { kind: "snapshot" }>): Cause {
  const dump = ctx.snapshot && ctx.snapshot.site === marker.site && ctx.snapshot.run === marker.run && ctx.snapshot.n === marker.n
    ? ctx.snapshot.dump
    : null;
  const frame = dump === null ? null : topUserFrame(dump);
  const location =
    frame !== null
      ? exactLocation(
          marker.site,
          { file: frame.file, line: frame.line },
          `the top user frame of thread ${frame.thread} (${frame.function})`,
        )
      : approximateLocation(marker.site, lastPositionBefore(ctx, marker.run, marker.t, null, marker.segment), `the last line ${marker.run} reported`);
  const how = marker.wake !== null ? "self-suspend" : marker.automatic ? "automatic" : "requested";
  const why =
    marker.wake !== null
      ? "The run suspended itself: every thread was parked in a wait that can be issued again, and the earliest sleep was far enough away."
      : marker.automatic
        ? "No pause was requested. The worker writes this snapshot on its own, and the process keeps running."
        : "A Pause command asked for this snapshot. The worker wrote it at the next clean point and exited.";
  return {
    title: `snapshot #${marker.n}`,
    sentence:
      `${why}` +
      `${location === null ? "" : frame === null ? ` The run stood at ${locationLabel(location)}.` : ` Thread ${frame.thread} stood at ${locationLabel(location)}.`}`,
    rows: [
      ["snapshot", `#${marker.n}`],
      ["written", formatClock(marker.t)],
      ["kind", how],
      ...(marker.wake === null ? [] : ([["sleep remaining", formatMs(marker.wake.remainingMs)]] as [string, string][])),
    ],
    location,
    missing:
      location === null
        ? "The state dump of this snapshot is not loaded, and the run reported no position in this segment."
        : null,
  };
}

function segmentCause(ctx: CauseContext, bar: SegmentBar): Cause {
  const positions: PositionEvent[] = [];
  for (const event of runEvents(ctx, bar.run)) {
    if (event.type === "position" && event.segment === bar.segment && event.site === bar.site) positions.push(event);
  }
  const first = positions[0] ?? null;
  const last = positions[positions.length - 1] ?? null;
  const started = bar.mode === "resume" ? "restored a snapshot" : bar.mode === "start" ? "started the function" : "ran";
  const location = approximateLocation(bar.site, last, `the last line segment ${bar.segment} reported`);
  return {
    title: `segment ${bar.segment}`,
    sentence:
      `The process ${started} and ${SEGMENT_END_TEXT[bar.endKind]}` +
      `${first === null || last === null ? "." : `. It ran from ${baseName(first.file)}:${first.line} to ${baseName(last.file)}:${last.line}.`}`,
    rows: [
      ["pid", String(bar.pid ?? "?")],
      ["mode", bar.mode ?? "unknown"],
      ["first position", first === null ? "none reported" : `${baseName(first.file)}:${first.line} (${first.function})`],
      ["last position", last === null ? "none reported" : `${baseName(last.file)}:${last.line} (${last.function})`],
      ["duration", formatMs(bar.end - bar.start)],
      ...(bar.programHash ? ([["program", bar.programHash.slice(0, 12)]] as [string, string][]) : []),
    ],
    location,
    missing: location === null ? "This segment reported no position." : null,
  };
}

function threadCause(ctx: CauseContext, thread: ThreadBar): Cause {
  const started = threadStartedEvent(ctx, thread.run, thread.segment, thread.thread);
  const spawnSite = locationOf(started);
  const parent = thread.parentThread;
  const fallback =
    spawnSite === null && parent !== null
      ? lastPositionBefore(ctx, thread.run, thread.start + 1, parent, thread.segment)
      : null;
  const location =
    spawnSite !== null
      ? exactLocation(thread.site, spawnSite, "the spawn site in the parent thread")
      : approximateLocation(thread.site, fallback, `the last line thread ${parent ?? "?"} reported before the spawn`);
  const restored = thread.continued ? " It was restored from the snapshot of an earlier segment." : "";
  return {
    title: `thread ${thread.thread}`,
    sentence:
      `Thread ${thread.thread} was spawned by thread ${parent ?? "the root thread"}` +
      `${location === null ? "" : ` at ${locationLabel(location)}`}.${restored}`,
    rows: [
      ["parent thread", String(parent ?? "none")],
      ["segment", String(thread.segment)],
      [thread.continued || thread.continues ? "alive in this segment" : "lifetime", formatMs(thread.end - thread.start)],
      ...(thread.continues ? ([["continues", "in a later segment, from the snapshot"]] as [string, string][]) : []),
    ],
    location,
    missing:
      location === null
        ? "The worker reported no spawn site for this thread, and its parent reported no position before the spawn."
        : null,
  };
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/** The cause of one timeline object (contract section 10.3). */
export function causeOf(target: DetailTarget, ctx: CauseContext, now: number): Cause {
  switch (target.kind) {
    case "connector": {
      const connector = target.item;
      switch (connector.kind) {
        case "call":
          return callCause(ctx, connector.from.site, connector.from.run, connector.callId ?? "", connector.to.site, connector.to.run);
        case "return":
          return returnCause(ctx, connector);
        case "cancel":
          return cancelCause(ctx, connector.from.site, connector.from.run, connector.callId ?? "", "worker", {
            site: connector.to.site,
            run: connector.to.run,
          });
        case "migration":
          return migrationCause(ctx, connector);
        case "fork": {
          const record = recordOf(ctx, connector.to.site, connector.to.run);
          const n = record?.forked_from?.n ?? 0;
          return forkCause(ctx, connector.from.site, connector.to.run, connector.from.run, n, connector.to.t);
        }
      }
      break;
    }
    case "wait": {
      const wait = target.item;
      const cause = callCause(ctx, wait.site, wait.run, wait.callId, null, null);
      return {
        ...cause,
        title: "waiting on a remote call",
        sentence: `${cause.sentence} Thread ${wait.thread} waited here for ${formatMs(wait.end - wait.start)}${wait.cancelled ? ", until the call was cancelled" : ""}.`,
      };
    }
    case "segment":
      return segmentCause(ctx, target.item);
    case "thread":
      return threadCause(ctx, target.item);
    case "gap": {
      const gap = target.item;
      if (gap.kind === "sleeping") return sleepCause(ctx, gap, now);
      if (gap.kind === "fork") {
        const record = recordOf(ctx, gap.site, gap.run);
        const from = record?.forked_from;
        if (from) return forkCause(ctx, gap.site, gap.run, from.run, from.n, gap.start);
      }
      const position = lastPositionBefore(ctx, gap.run, gap.start + 1, null);
      const location = approximateLocation(gap.site, position, `the last line ${gap.run} reported`);
      return {
        title: "no process",
        sentence:
          `The run has no process. Its state lives in ${gap.snapshotN === null ? "a snapshot" : `snapshot #${gap.snapshotN}`}` +
          `${location === null ? "." : `, taken at ${locationLabel(location)}.`}`,
        rows: [
          ["state lives in", gap.snapshotN === null ? "a snapshot" : `snapshot #${gap.snapshotN}`],
          [gap.open ? "paused for" : "duration", gap.open ? "not resumed yet" : formatMs(gap.end - gap.start)],
        ],
        location,
        missing: location === null ? "The run reported no position before it was suspended." : null,
      };
    }
    case "marker": {
      const marker = target.item;
      switch (marker.kind) {
        case "snapshot":
          return snapshotCause(ctx, marker);
        case "cancel":
          return cancelCause(ctx, marker.site, marker.run, marker.callId, marker.source, {
            site: marker.childSite,
            run: marker.childRun,
          });
        case "fork":
          return forkCause(ctx, marker.site, marker.run, marker.fromRun, marker.n, marker.t);
        case "pause_request":
          return {
            title: "pause requested",
            sentence:
              marker.waitingOn.length > 0
                ? "A Pause command reached the worker, which had operations in flight and wrote the snapshot at the next clean point."
                : "A Pause command reached the worker. No operation in flight delayed the snapshot.",
            rows: [["waits on", marker.waitingOn.length === 0 ? "nothing" : String(marker.waitingOn.length)]],
            location: approximateLocation(
              marker.site,
              lastPositionBefore(ctx, marker.run, marker.t, null, marker.segment),
              `the last line ${marker.run} reported`,
            ),
            missing: null,
          };
        case "blocked":
          return {
            title: "pause blocked",
            sentence: `The worker could not write a snapshot: ${marker.reason}. The pause request stays open and the worker tries again at its next yield.`,
            rows: [["reason", marker.reason], ["path", marker.path.join(" › ") || "n/a"]],
            location: approximateLocation(
              marker.site,
              lastPositionBefore(ctx, marker.run, marker.t, null, marker.segment),
              `the last line ${marker.run} reported`,
            ),
            missing: null,
          };
        case "resume":
          return {
            title: "resume",
            sentence: "A new process restored the snapshot and continued the run where it stopped.",
            rows: [],
            location: approximateLocation(
              marker.site,
              lastPositionBefore(ctx, marker.run, marker.t + 5000, null, marker.segment),
              `the first line segment ${marker.segment} reported`,
            ),
            missing: null,
          };
        case "wake":
          return {
            title: "wake",
            sentence: `The ${marker.reason === "manual" ? "resume command" : marker.reason === "restart" ? "restart of the site server" : "wake timer of the site server"} started the next segment of the sleeping run.`,
            rows: [
              ["woken by", marker.reason],
              ["timer set for", marker.wakeAt === null ? "n/a" : formatClock(marker.wakeAt)],
              ["without a process for", formatMs(marker.sleptMs)],
            ],
            location: null,
            missing: null,
          };
      }
      break;
    }
  }
  return { title: "object", sentence: "", rows: [], location: null, missing: null };
}
