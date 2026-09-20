/**
 * Timeline layout: converts run records and event lists into geometry in time
 * units and row indices. The React component maps the result to pixels. This
 * module has no dependency on React or the DOM, so it is unit tested directly.
 */

import {
  DEFAULT_SITES,
  isLiveStatus,
  type PauseStats,
  type ResumeStats,
  type Run,
  type Site,
} from "../protocol";
import { runKey, type RunKey, type TimelineEvent } from "../state";

export type SegmentEnd =
  /** The process still exists. */
  | "open"
  | "paused"
  | "completed"
  | "failed"
  | "cancelled"
  /** The process ended without a terminal event, and the run has no snapshot to resume from. */
  | "lost"
  /** The process ended without a terminal event, and the run continued or can continue from a snapshot. */
  | "killed";

export interface SegmentBar {
  id: string;
  site: Site;
  run: string;
  segment: number;
  pid: number | null;
  mode: "start" | "resume" | null;
  start: number;
  end: number;
  endKind: SegmentEnd;
  /** Row inside the lane of `site`. */
  row: number;
  /** The root thread of the segment, when it is known. */
  rootThread: number | null;
}

/** An interval in which the run has no process. */
export interface Gap {
  id: string;
  run: string;
  /** The site that holds the snapshot during the interval. */
  site: Site;
  row: number;
  start: number;
  end: number;
  /** True when the run has not been resumed yet. */
  open: boolean;
  snapshotN: number | null;
  bytes: number | null;
  /** True for the interval between the creation of a fork and its first segment. */
  fork: boolean;
}

/** A spawned thread, drawn as a thin bar under the run's bar. */
export interface ThreadBar {
  id: string;
  site: Site;
  run: string;
  segment: number;
  thread: number;
  parentThread: number | null;
  start: number;
  end: number;
  row: number;
  subRow: number;
}

/** An interval in which a thread waits for the result of a remote call. */
export interface WaitInterval {
  id: string;
  site: Site;
  run: string;
  segment: number;
  thread: number;
  callId: string;
  start: number;
  end: number;
  row: number;
  /** `null` when the waiting thread is the root thread: the run's main bar is shaded. */
  subRow: number | null;
}

export interface Anchor {
  site: Site;
  run: string;
  t: number;
  row: number;
  /** `null` anchors to the run's main bar. */
  subRow: number | null;
}

export interface Connector {
  id: string;
  kind: "call" | "return" | "migration" | "fork";
  from: Anchor;
  to: Anchor;
  callId: string | null;
  /** For `return`: whether the child completed without an error. */
  ok: boolean | null;
  /**
   * For `return`: the result reached the parent's site while the parent had
   * no process, and the parent has not been resumed yet.
   */
  pending: boolean;
  label: string;
}

export type Marker = {
  id: string;
  site: Site;
  run: string;
  segment: number;
  t: number;
  row: number;
} & (
  | { kind: "pause_request"; waitingOn: string[] }
  | { kind: "blocked"; reason: string; path: string[] }
  | { kind: "snapshot"; n: number; bytes: number | null; stats: PauseStats | null; automatic: boolean }
  | { kind: "resume"; stats: ResumeStats }
  /** The run was created from snapshot `n` of `fromRun` (section 7.2, `forked`). */
  | { kind: "fork"; fromRun: string; n: number }
);

export type MarkerKind = Marker["kind"];

export interface Lane {
  site: Site;
  /** Number of thread sub-rows under each row of the lane. */
  rows: { subRows: number }[];
}

export interface TimelineLayout {
  t0: number;
  t1: number;
  /** True when a process of the run tree exists, so `t1` moves with the clock. */
  live: boolean;
  lanes: Lane[];
  segments: SegmentBar[];
  gaps: Gap[];
  threads: ThreadBar[];
  waits: WaitInterval[];
  connectors: Connector[];
  markers: Marker[];
}

export interface LayoutInput {
  /**
   * The site names of the registry, in registry order. Every site gets a lane,
   * also when it holds no run of the tree. A site that is not in the registry
   * gets a lane after them when a run of the tree is on it. Default: the
   * default registry.
   */
  sites?: readonly Site[];
  /** The run records of the run tree. */
  runs: readonly Run[];
  /** Event lists by run key. Lists of runs outside the tree are ignored. */
  events: Readonly<Record<string, readonly TimelineEvent[] | undefined>>;
  now: number;
}

// ---------------------------------------------------------------------------
// Pass 1: segments, threads, waits, and markers of one run record
// ---------------------------------------------------------------------------

interface RawSegment {
  site: Site;
  run: string;
  segment: number;
  pid: number | null;
  mode: "start" | "resume" | null;
  start: number;
  lastTs: number;
  terminal: { ts: number; kind: "paused" | "completed" | "failed" | "cancelled" } | null;
  /** The `worker_exit` event of the segment: the real end of the process (section 7.2). */
  exit: { ts: number; status: string; exitCode: number } | null;
  /** Time of the first change of the run record to a status without a process. */
  notLiveTs: number | null;
  rootThread: number | null;
  pausedStats: PauseStats | null;
  pausedPath: string | null;
}

interface RawThread {
  segment: number;
  thread: number;
  parentThread: number | null;
  start: number;
  end: number | null;
}

interface RawWait {
  segment: number;
  thread: number;
  callId: string;
  start: number;
  end: number | null;
}

interface RawCall {
  callId: string;
  thread: number;
  segment: number;
  ts: number;
  function: string;
  childSite: Site | null;
  childRun: string | null;
  receivedTs: number | null;
  receivedSegment: number | null;
  returnedTs: number | null;
  ok: boolean | null;
  /**
   * True when the record was created from a result event alone. This happens
   * on the destination site of a migration: the call was made on the origin.
   */
  stub: boolean;
}

type RawMarker =
  | { kind: "pause_request"; segment: number; t: number; waitingOn: string[]; exact: boolean }
  | { kind: "blocked"; segment: number; t: number; reason: string; path: string[] }
  | { kind: "snapshot"; segment: number; t: number; path: string; stats: PauseStats; automatic: boolean }
  | { kind: "resume"; segment: number; t: number; stats: ResumeStats };

interface RunScan {
  site: Site;
  run: string;
  record: Run | null;
  segments: RawSegment[];
  threads: RawThread[];
  waits: RawWait[];
  calls: Map<string, RawCall>;
  markers: RawMarker[];
  /** The times of the `migrated_out` events. A run can leave a site more than once. */
  migratedOutTs: number[];
  /** Set for a run that was created by a fork on this site. */
  fork: { ts: number; fromRun: string; n: number } | null;
}

function scanRun(site: Site, run: string, record: Run | null, events: readonly TimelineEvent[]): RunScan {
  const segments = new Map<number, RawSegment>();
  const threads: RawThread[] = [];
  const openThreads = new Map<string, RawThread>();
  const waits: RawWait[] = [];
  const calls = new Map<string, RawCall>();
  const markers: RawMarker[] = [];
  const migratedOutTs: number[] = [];
  let fork: RunScan["fork"] = null;
  let pendingRequest: Extract<RawMarker, { kind: "pause_request" }> | null = null;
  let currentSegment: RawSegment | null = null;

  const segmentOf = (event: { segment: number; pid: number; ts: number }): RawSegment => {
    let segment = segments.get(event.segment);
    if (!segment) {
      segment = {
        site,
        run,
        segment: event.segment,
        pid: typeof event.pid === "number" ? event.pid : null,
        mode: null,
        start: event.ts,
        lastTs: event.ts,
        terminal: null,
        exit: null,
        notLiveTs: null,
        rootThread: null,
        pausedStats: null,
        pausedPath: null,
      };
      segments.set(event.segment, segment);
      // A call that is still waiting continues to wait in the new segment.
      for (const call of calls.values()) {
        if (call.receivedTs === null && !call.stub) {
          waits.push({ segment: event.segment, thread: call.thread, callId: call.callId, start: event.ts, end: null });
        }
      }
      pendingRequest = null;
    }
    segment.lastTs = Math.max(segment.lastTs, event.ts);
    currentSegment = segment;
    return segment;
  };

  const requestPause = (segment: number, t: number, exact: boolean): Extract<RawMarker, { kind: "pause_request" }> => {
    if (pendingRequest === null) {
      pendingRequest = { kind: "pause_request", segment, t, waitingOn: [], exact };
      markers.push(pendingRequest);
    }
    return pendingRequest;
  };

  for (const event of events) {
    switch (event.type) {
      case "ui_status": {
        const segment = segments.get(event.segment);
        if (event.status === "pausing" && segment) requestPause(event.segment, event.ts, true);
        if (segment && !isLiveStatus(event.status) && segment.notLiveTs === null && event.ts >= segment.start) {
          segment.notLiveTs = event.ts;
        }
        break;
      }
      case "remote_dispatched": {
        const call = calls.get(event.call_id);
        if (call) {
          call.childSite = event.child_site;
          call.childRun = event.child_run;
        } else {
          calls.set(event.call_id, {
            callId: event.call_id,
            thread: -1,
            segment: (currentSegment as RawSegment | null)?.segment ?? 1,
            ts: event.ts,
            function: event.function,
            childSite: event.child_site,
            childRun: event.child_run,
            receivedTs: null,
            receivedSegment: null,
            returnedTs: null,
            ok: null,
            stub: false,
          });
        }
        break;
      }
      case "remote_returned": {
        const call = calls.get(event.call_id);
        if (call) {
          call.returnedTs = event.ts;
          call.ok = event.ok;
          call.childSite ??= event.child_site;
          call.childRun ??= event.child_run;
        } else {
          calls.set(event.call_id, {
            callId: event.call_id,
            thread: -1,
            segment: 0,
            ts: event.ts,
            function: "",
            childSite: event.child_site,
            childRun: event.child_run,
            receivedTs: null,
            receivedSegment: null,
            returnedTs: event.ts,
            ok: event.ok,
            stub: true,
          });
        }
        break;
      }
      case "migrated_out":
        migratedOutTs.push(event.ts);
        break;
      case "migrated_in":
        break;
      case "worker_exit": {
        // The event carries `segment` and `pid`, but it must not open a segment:
        // a process that never wrote an event has no bar.
        const segment = segments.get(event.segment);
        if (segment) {
          segment.exit = { ts: event.ts, status: event.status, exitCode: event.exit_code };
          segment.lastTs = Math.max(segment.lastTs, event.ts);
        }
        break;
      }
      case "forked":
        fork = { ts: event.ts, fromRun: event.from_run, n: event.n };
        break;
      default: {
        const segment = segmentOf(event);
        switch (event.type) {
          case "hello":
            segment.mode = event.mode;
            segment.start = Math.min(segment.start, event.ts);
            break;
          case "thread_started": {
            if (event.parent_thread === null) {
              segment.rootThread = event.thread;
            } else {
              const thread: RawThread = {
                segment: event.segment,
                thread: event.thread,
                parentThread: event.parent_thread,
                start: event.ts,
                end: null,
              };
              threads.push(thread);
              openThreads.set(`${event.segment}:${event.thread}`, thread);
            }
            break;
          }
          case "thread_ended": {
            const thread = openThreads.get(`${event.segment}:${event.thread}`);
            if (thread) {
              thread.end = event.ts;
              openThreads.delete(`${event.segment}:${event.thread}`);
            }
            break;
          }
          case "position":
            // Without a `thread_started` event, the first thread that reports a
            // position is taken as the root thread.
            segment.rootThread ??= openThreads.has(`${event.segment}:${event.thread}`) ? null : event.thread;
            break;
          case "remote_call": {
            const known = calls.get(event.call_id);
            calls.set(event.call_id, {
              callId: event.call_id,
              thread: event.thread,
              segment: event.segment,
              ts: event.ts,
              function: event.function,
              childSite: known?.childSite ?? null,
              childRun: known?.childRun ?? null,
              receivedTs: null,
              receivedSegment: null,
              returnedTs: known?.returnedTs ?? null,
              ok: known?.ok ?? null,
              stub: false,
            });
            waits.push({ segment: event.segment, thread: event.thread, callId: event.call_id, start: event.ts, end: null });
            break;
          }
          case "remote_result_received": {
            const call = calls.get(event.call_id);
            if (call) {
              call.receivedTs = event.ts;
              call.receivedSegment = event.segment;
            } else {
              calls.set(event.call_id, {
                callId: event.call_id,
                thread: event.thread,
                segment: 0,
                ts: event.ts,
                function: "",
                childSite: null,
                childRun: null,
                receivedTs: event.ts,
                receivedSegment: event.segment,
                returnedTs: null,
                ok: null,
                stub: true,
              });
            }
            for (const wait of waits) {
              if (wait.callId === event.call_id && wait.end === null) wait.end = event.ts;
            }
            break;
          }
          case "pausing": {
            const request = requestPause(event.segment, event.ts, false);
            request.waitingOn = event.waiting_on ?? [];
            break;
          }
          case "blocked":
            requestPause(event.segment, event.ts, false);
            markers.push({
              kind: "blocked",
              segment: event.segment,
              t: event.ts,
              reason: event.reason,
              path: event.path ?? [],
            });
            break;
          case "paused": {
            const latency = event.stats?.pause_latency_ms;
            const request = requestPause(event.segment, event.ts, false);
            if (!request.exact && typeof latency === "number") {
              request.t = Math.max(segment.start, Math.min(request.t, event.ts - latency));
            }
            pendingRequest = null;
            segment.terminal = { ts: event.ts, kind: "paused" };
            segment.pausedStats = event.stats ?? null;
            segment.pausedPath = event.snapshot_path;
            markers.push({
              kind: "snapshot",
              segment: event.segment,
              t: event.ts,
              path: event.snapshot_path,
              stats: event.stats,
              automatic: false,
            });
            break;
          }
          case "snapshot":
            // An automatic snapshot. The process keeps running.
            markers.push({
              kind: "snapshot",
              segment: event.segment,
              t: event.ts,
              path: event.snapshot_path,
              stats: event.stats,
              automatic: true,
            });
            break;
          case "resumed":
            markers.push({ kind: "resume", segment: event.segment, t: event.ts, stats: event.stats });
            // A wait that was carried into this segment starts when the thread
            // exists again. The real worker loads the program for about a
            // second between `hello` and `resumed`, and nothing waits then.
            for (const wait of waits) {
              if (wait.segment === event.segment && wait.end === null && wait.start < event.ts) wait.start = event.ts;
            }
            break;
          case "completed":
            segment.terminal = { ts: event.ts, kind: "completed" };
            break;
          case "failed":
            segment.terminal = { ts: event.ts, kind: "failed" };
            break;
          case "cancelled":
            segment.terminal = { ts: event.ts, kind: "cancelled" };
            break;
          case "log":
            break;
        }
      }
    }
  }

  return {
    site,
    run,
    record,
    segments: [...segments.values()].sort((a, b) => a.segment - b.segment),
    threads,
    waits,
    calls,
    markers,
    migratedOutTs,
    fork: fork ?? forkFromRecord(record),
  };
}

/**
 * The fork of a record whose `forked` event is not in the event list. A record
 * that was imported by migration copies `forked_from` from its origin, where
 * the fork happened, so it does not count.
 */
function forkFromRecord(record: Run | null): RunScan["fork"] {
  if (record === null || record.forked_from === null || record.origin !== null) return null;
  return { ts: record.created_ts, fromRun: record.forked_from.run, n: record.forked_from.n };
}

/** The way a segment ended, from the run status in its `worker_exit` event. */
function endKindOfExit(exit: NonNullable<RawSegment["exit"]>): SegmentEnd {
  switch (exit.status) {
    case "completed":
    case "failed":
    case "cancelled":
    case "lost":
      return exit.status;
    case "paused":
      // Exit code 75 follows a `paused` event. Any other exit of a run that is
      // `paused` afterwards is a lost process with a snapshot to continue from.
      return exit.exitCode === 75 ? "paused" : "killed";
    default:
      return "killed";
  }
}

// ---------------------------------------------------------------------------
// Pass 2: assemble logical runs, rows, connectors
// ---------------------------------------------------------------------------

/** The snapshot number in a path such as `.baml/runs/r-1/snap-0002.bamlsnap`. */
export function snapshotNumberFromPath(path: string): number | null {
  const match = /snap-0*(\d+)\.[a-z]+$/i.exec(path);
  return match?.[1] !== undefined ? Number(match[1]) : null;
}

/** The first time at or after `t` at which the run left the site of `scan`, or `null`. */
function migratedOutAfter(scan: RunScan | undefined, t: number): number | null {
  return scan?.migratedOutTs.find((ts) => ts >= t - 1) ?? null;
}

/** The registry order, followed by every other site in order of first appearance. */
function laneOrder(registry: readonly Site[], seen: Iterable<Site>): Site[] {
  const order = [...registry];
  for (const site of seen) {
    if (!order.includes(site)) order.push(site);
  }
  return order;
}

function assignRows<T extends { start: number; end: number }>(items: T[]): Map<T, number> {
  const rowEnds: number[] = [];
  const rows = new Map<T, number>();
  for (const item of [...items].sort((a, b) => a.start - b.start)) {
    let row = rowEnds.findIndex((end) => end <= item.start);
    if (row === -1) {
      row = rowEnds.length;
      rowEnds.push(item.end);
    } else {
      rowEnds[row] = item.end;
    }
    rows.set(item, row);
  }
  return rows;
}

export function computeTimelineLayout(input: LayoutInput): TimelineLayout {
  const { now } = input;
  const records = new Map<RunKey, Run>();
  for (const run of input.runs) records.set(runKey(run.site, run.id), run);

  // Scan every run record of the tree, and every event list that belongs to a
  // run id of the tree even when the record has not arrived yet.
  const runIds = new Set(input.runs.map((run) => run.id));
  const eventSites: Site[] = [];
  for (const key of Object.keys(input.events)) {
    const slash = key.indexOf("/");
    if (slash > 0 && runIds.has(key.slice(slash + 1))) eventSites.push(key.slice(0, slash));
  }
  const sites = laneOrder(input.sites ?? DEFAULT_SITES.map((site) => site.name), [
    ...input.runs.map((run) => run.site),
    ...eventSites,
  ]);
  const scans: RunScan[] = [];
  for (const id of runIds) {
    for (const site of sites) {
      const key = runKey(site, id);
      const events = input.events[key];
      const record = records.get(key) ?? null;
      if (record === null && events === undefined) continue;
      scans.push(scanRun(site, id, record, events ?? []));
    }
  }

  const live = input.runs.some((run) => isLiveStatus(run.status));

  // Segments of each logical run (one run id over every site that held it), in order.
  const segments: SegmentBar[] = [];
  const rawBySegmentBar = new Map<SegmentBar, RawSegment>();
  const barsByRun = new Map<string, SegmentBar[]>();
  for (const scan of scans) {
    for (const raw of scan.segments) {
      const bar: SegmentBar = {
        id: `${raw.site}/${raw.run}#${raw.segment}`,
        site: raw.site,
        run: raw.run,
        segment: raw.segment,
        pid: raw.pid,
        mode: raw.mode,
        start: raw.start,
        end: raw.lastTs,
        endKind: "open",
        row: 0,
        rootThread: raw.rootThread,
      };
      segments.push(bar);
      rawBySegmentBar.set(bar, raw);
      const list = barsByRun.get(raw.run) ?? [];
      list.push(bar);
      barsByRun.set(raw.run, list);
    }
  }
  for (const list of barsByRun.values()) list.sort((a, b) => a.segment - b.segment || a.start - b.start);

  for (const list of barsByRun.values()) {
    list.forEach((bar, index) => {
      const raw = rawBySegmentBar.get(bar) as RawSegment;
      const record = records.get(runKey(bar.site, bar.run)) ?? null;
      const next = list[index + 1];
      if (raw.terminal) {
        // The process exits shortly after its terminal event.
        bar.end = Math.max(raw.terminal.ts, raw.exit?.ts ?? raw.terminal.ts);
        bar.endKind = raw.terminal.kind;
      } else if (raw.exit) {
        // No terminal event: the process was killed or lost. `worker_exit` has
        // the time, also after a page reload, when no status change was seen.
        bar.end = raw.exit.ts;
        bar.endKind = endKindOfExit(raw.exit);
      } else if (next) {
        bar.end = Math.min(raw.notLiveTs ?? raw.lastTs, next.start);
        bar.endKind = "killed";
      } else if (record && isLiveStatus(record.status) && record.segment <= bar.segment) {
        bar.end = Math.max(now, raw.lastTs);
        bar.endKind = "open";
      } else if (record) {
        const recordTs =
          record.status === "lost" || record.status === "cancelled" ? Math.max(raw.lastTs, record.updated_ts) : raw.lastTs;
        bar.end = raw.notLiveTs ?? recordTs;
        bar.endKind =
          record.status === "cancelled"
            ? "cancelled"
            : record.status === "lost"
              ? "lost"
              : record.status === "failed"
                ? "failed"
                : record.status === "completed"
                  ? "completed"
                  : "killed";
      } else {
        bar.end = raw.lastTs;
        bar.endKind = "killed";
      }
      bar.end = Math.max(bar.end, bar.start);
    });
  }

  // Gaps between consecutive segments, and the open gap of a paused run.
  const scanByKey = new Map<RunKey, RunScan>();
  for (const scan of scans) scanByKey.set(runKey(scan.site, scan.run), scan);

  const lastFinite = Math.max(
    ...segments.filter((bar) => bar.endKind !== "open").map((bar) => bar.end),
    ...segments.map((bar) => bar.start),
    Number.NEGATIVE_INFINITY,
  );
  const openGapEnd = live ? now : lastFinite;

  const snapshotFor = (bar: SegmentBar): { n: number | null; bytes: number | null } => {
    const raw = rawBySegmentBar.get(bar) as RawSegment;
    const record = records.get(runKey(bar.site, bar.run));
    if (raw.pausedPath !== null) {
      const fromRecord = record?.snapshots.find((snapshot) => snapshot.snapshot_path === raw.pausedPath);
      return {
        n: fromRecord?.n ?? snapshotNumberFromPath(raw.pausedPath),
        bytes: raw.pausedStats?.compressed_bytes ?? fromRecord?.bytes ?? null,
      };
    }
    // Without a `paused` event, the run continues from its latest snapshot at the time.
    const candidates = (record?.snapshots ?? []).filter((snapshot) => snapshot.ts <= bar.end + 1);
    const latest = candidates[candidates.length - 1];
    return { n: latest?.n ?? null, bytes: latest?.bytes ?? null };
  };

  const gaps: Gap[] = [];
  const connectors: Connector[] = [];
  for (const [run, list] of barsByRun) {
    list.forEach((bar, index) => {
      const next = list[index + 1];
      const snapshot = snapshotFor(bar);
      if (next) {
        let end = next.start;
        if (next.site !== bar.site) {
          const outTs = migratedOutAfter(scanByKey.get(runKey(bar.site, run)), bar.end);
          end = outTs === null ? next.start : Math.min(Math.max(outTs, bar.end), next.start);
        }
        gaps.push({
          id: `gap:${bar.id}`,
          run,
          site: bar.site,
          row: 0,
          start: bar.end,
          end: Math.max(end, bar.end),
          open: false,
          snapshotN: snapshot.n,
          bytes: snapshot.bytes,
          fork: false,
        });
        return;
      }
      const record = records.get(runKey(bar.site, run));
      const waitsForResume = record?.status === "paused" || record?.status === "migrated";
      if (waitsForResume && bar.endKind !== "open") {
        gaps.push({
          id: `gap:${bar.id}`,
          run,
          site: bar.site,
          row: 0,
          start: bar.end,
          end: Math.max(openGapEnd, bar.end),
          open: true,
          snapshotN: snapshot.n,
          bytes: snapshot.bytes,
          fork: false,
        });
      }
    });
  }

  // A fork has no process from its creation until its first segment.
  for (const scan of scans) {
    if (scan.fork === null) continue;
    const first = barsByRun.get(scan.run)?.[0];
    const record = scan.record;
    const bytes = record?.snapshots.find((snapshot) => snapshot.n === scan.fork?.n)?.bytes ?? null;
    let end: number;
    let open = false;
    if (first) {
      end = first.start;
      if (first.site !== scan.site) {
        // The fork migrated before its first resume.
        const outTs = migratedOutAfter(scan, scan.fork.ts);
        end = outTs === null ? first.start : Math.min(Math.max(outTs, scan.fork.ts), first.start);
        connectors.push({
          id: `migration:fork:${scan.site}/${scan.run}`,
          kind: "migration",
          from: { site: scan.site, run: scan.run, t: end, row: 0, subRow: null },
          to: { site: first.site, run: scan.run, t: first.start, row: 0, subRow: null },
          callId: null,
          ok: null,
          pending: false,
          label: `${scan.run} migrates ${scan.site} → ${first.site}`,
        });
      }
    } else if (record?.status === "paused" || record?.status === "migrated") {
      end = Math.max(openGapEnd, scan.fork.ts);
      open = true;
    } else {
      continue;
    }
    gaps.push({
      id: `gap:fork:${scan.site}/${scan.run}`,
      run: scan.run,
      site: scan.site,
      row: 0,
      start: scan.fork.ts,
      end: Math.max(end, scan.fork.ts),
      open,
      snapshotN: scan.fork.n,
      bytes,
      fork: true,
    });
  }

  // Rows. One row per run record per lane, so the segments of a run line up.
  interface RowItem {
    site: Site;
    run: string;
    start: number;
    end: number;
  }
  const rowItems = new Map<RunKey, RowItem>();
  const extend = (site: Site, run: string, start: number, end: number): void => {
    const key = runKey(site, run);
    const item = rowItems.get(key);
    if (item) {
      item.start = Math.min(item.start, start);
      item.end = Math.max(item.end, end);
    } else {
      rowItems.set(key, { site, run, start, end });
    }
  };
  for (const bar of segments) extend(bar.site, bar.run, bar.start, bar.end);
  for (const gap of gaps) extend(gap.site, gap.run, gap.start, gap.end);

  const rowOf = new Map<RunKey, number>();
  const lanes: Lane[] = sites.map((site) => {
    const items = [...rowItems.values()].filter((item) => item.site === site);
    const rows = assignRows(items);
    let count = 0;
    for (const [item, row] of rows) {
      rowOf.set(runKey(item.site, item.run), row);
      count = Math.max(count, row + 1);
    }
    return { site, rows: Array.from({ length: Math.max(count, 1) }, () => ({ subRows: 0 })) };
  });
  const laneOf = (site: Site): Lane => lanes.find((lane) => lane.site === site) as Lane;
  const row = (site: Site, run: string): number => rowOf.get(runKey(site, run)) ?? 0;

  for (const bar of segments) bar.row = row(bar.site, bar.run);
  for (const gap of gaps) gap.row = row(gap.site, gap.run);
  for (const connector of connectors) {
    connector.from.row = row(connector.from.site, connector.from.run);
    connector.to.row = row(connector.to.site, connector.to.run);
  }

  // A call that was still waiting when the run migrated continues to wait in
  // the segments on the destination site, until the result is received there.
  for (const scan of scans) {
    for (const call of scan.calls.values()) {
      if (call.stub || call.receivedTs !== null) continue;
      for (const sibling of scans) {
        if (sibling === scan || sibling.run !== scan.run) continue;
        const receivedTs = sibling.calls.get(call.callId)?.receivedTs ?? null;
        for (const segment of sibling.segments) {
          if (segment.segment <= call.segment) continue;
          if (receivedTs !== null && segment.start > receivedTs) continue;
          sibling.waits.push({
            segment: segment.segment,
            thread: call.thread,
            callId: call.callId,
            start: segment.start,
            end: receivedTs,
          });
        }
      }
    }
  }

  // Threads and waits.
  const barOf = (site: Site, run: string, segment: number): SegmentBar | undefined =>
    segments.find((bar) => bar.site === site && bar.run === run && bar.segment === segment);

  const threads: ThreadBar[] = [];
  const waits: WaitInterval[] = [];
  const subRowOf = new Map<string, number>();
  for (const scan of scans) {
    const candidates = scan.threads.flatMap((thread) => {
      const bar = barOf(scan.site, scan.run, thread.segment);
      if (!bar) return [];
      return [{ thread, start: thread.start, end: Math.max(thread.end ?? bar.end, thread.start) }];
    });
    const subRows = assignRows(candidates);
    const laneRow = laneOf(scan.site).rows[row(scan.site, scan.run)];
    for (const [candidate, subRow] of subRows) {
      const { thread } = candidate;
      subRowOf.set(`${scan.site}/${scan.run}#${thread.segment}:${thread.thread}`, subRow);
      if (laneRow) laneRow.subRows = Math.max(laneRow.subRows, subRow + 1);
      threads.push({
        id: `thread:${scan.site}/${scan.run}#${thread.segment}:${thread.thread}`,
        site: scan.site,
        run: scan.run,
        segment: thread.segment,
        thread: thread.thread,
        parentThread: thread.parentThread,
        start: candidate.start,
        end: candidate.end,
        row: row(scan.site, scan.run),
        subRow,
      });
    }
    for (const wait of scan.waits) {
      const bar = barOf(scan.site, scan.run, wait.segment);
      if (!bar) continue;
      const subRow = subRowOf.get(`${scan.site}/${scan.run}#${wait.segment}:${wait.thread}`) ?? null;
      const threadBar =
        subRow === null
          ? undefined
          : threads.find(
              (candidate) =>
                candidate.site === scan.site &&
                candidate.run === scan.run &&
                candidate.segment === wait.segment &&
                candidate.thread === wait.thread,
            );
      const limit = threadBar?.end ?? bar.end;
      waits.push({
        id: `wait:${scan.site}/${scan.run}#${wait.segment}:${wait.callId}`,
        site: scan.site,
        run: scan.run,
        segment: wait.segment,
        thread: wait.thread,
        callId: wait.callId,
        start: wait.start,
        end: Math.max(Math.min(wait.end ?? limit, limit), wait.start),
        row: bar.row,
        subRow,
      });
    }
  }

  // Markers.
  const markers: Marker[] = [];
  for (const scan of scans) {
    const markerRow = row(scan.site, scan.run);
    const seenSnapshots = new Set<number>();
    let pausedCount = 0;
    scan.markers.forEach((raw, index) => {
      const base = {
        id: `marker:${scan.site}/${scan.run}:${index}`,
        site: scan.site,
        run: scan.run,
        segment: raw.segment,
        t: raw.t,
        row: markerRow,
      };
      switch (raw.kind) {
        case "pause_request":
          markers.push({ ...base, kind: "pause_request", waitingOn: raw.waitingOn });
          break;
        case "blocked":
          markers.push({ ...base, kind: "blocked", reason: raw.reason, path: raw.path });
          break;
        case "resume":
          markers.push({ ...base, kind: "resume", stats: raw.stats });
          break;
        case "snapshot": {
          pausedCount += 1;
          const fromRecord = scan.record?.snapshots.find((snapshot) => snapshot.snapshot_path === raw.path);
          const n = fromRecord?.n ?? snapshotNumberFromPath(raw.path) ?? pausedCount;
          seenSnapshots.add(n);
          markers.push({
            ...base,
            kind: "snapshot",
            n,
            bytes: raw.stats?.compressed_bytes ?? fromRecord?.bytes ?? null,
            stats: raw.stats ?? null,
            automatic: raw.automatic,
          });
          break;
        }
      }
    });
    if (scan.fork !== null) {
      markers.push({
        id: `marker:${scan.site}/${scan.run}:fork`,
        site: scan.site,
        run: scan.run,
        // A fork is created without a process, so it belongs to no segment.
        segment: 0,
        t: scan.fork.ts,
        row: markerRow,
        kind: "fork",
        fromRun: scan.fork.fromRun,
        n: scan.fork.n,
      });
    }
    // Snapshots in the run record that no `paused` or `snapshot` event of the
    // event list announced, for example when the list was not loaded.
    for (const snapshot of scan.record?.snapshots ?? []) {
      if (seenSnapshots.has(snapshot.n)) continue;
      // A run imported by migration lists the snapshot of its origin, and a
      // fork lists the snapshot of its source. They belong to another row.
      const owner =
        typeof snapshot.segment === "number"
          ? scan.segments.find((segment) => segment.segment === snapshot.segment)
          : scan.segments.find((segment) => {
              const bar = barOf(scan.site, scan.run, segment.segment);
              return bar !== undefined && snapshot.ts >= bar.start - 1 && snapshot.ts <= bar.end + 1;
            });
      if (!owner || snapshot.ts < owner.start - 1) continue;
      markers.push({
        id: `marker:${scan.site}/${scan.run}:snap${snapshot.n}`,
        site: scan.site,
        run: scan.run,
        segment: owner.segment,
        t: snapshot.ts,
        row: markerRow,
        kind: "snapshot",
        n: snapshot.n,
        bytes: snapshot.bytes ?? null,
        stats: snapshot.stats ?? null,
        // Servers that predate `automatic` list such a snapshot only when it was automatic.
        automatic: snapshot.automatic ?? true,
      });
    }
  }
  markers.sort((a, b) => a.t - b.t);

  // Migration connectors.
  for (const [run, list] of barsByRun) {
    list.forEach((bar, index) => {
      const next = list[index + 1];
      if (!next || next.site === bar.site) return;
      const gap = gaps.find((candidate) => candidate.id === `gap:${bar.id}`);
      connectors.push({
        id: `migration:${bar.id}`,
        kind: "migration",
        from: { site: bar.site, run, t: gap?.end ?? bar.end, row: bar.row, subRow: null },
        to: { site: next.site, run, t: next.start, row: next.row, subRow: null },
        callId: null,
        ok: null,
        pending: false,
        label: `${run} migrates ${bar.site} → ${next.site}`,
      });
    });
  }

  // Fork connectors: from the snapshot on the source's row to the new run.
  for (const scan of scans) {
    if (scan.fork === null) continue;
    const source = scans.find((other) => other.site === scan.site && other.run === scan.fork?.fromRun);
    const snapshot = source?.record?.snapshots.find((candidate) => candidate.n === scan.fork?.n);
    if (!source || !snapshot) continue;
    connectors.push({
      id: `fork:${scan.site}/${scan.run}`,
      kind: "fork",
      from: { site: source.site, run: source.run, t: Math.min(snapshot.ts, scan.fork.ts), row: row(source.site, source.run), subRow: null },
      to: { site: scan.site, run: scan.run, t: scan.fork.ts, row: row(scan.site, scan.run), subRow: null },
      callId: null,
      ok: null,
      pending: false,
      label: `${scan.run} forks ${source.run} at snapshot #${scan.fork.n}`,
    });
  }

  // Remote call and return connectors.
  for (const scan of scans) {
    for (const call of scan.calls.values()) {
      if (call.stub) continue;
      const child = findChild(call, scan, input.runs);
      if (!child) continue;
      const childBars = barsByRun.get(child.run) ?? [];
      const childRecord = records.get(runKey(child.site, child.run));
      const firstChildBar = childBars[0];
      const childStart = firstChildBar?.start ?? childRecord?.created_ts ?? null;
      const callSubRow = subRowOf.get(`${scan.site}/${scan.run}#${call.segment}:${call.thread}`) ?? null;
      const parentRow = row(scan.site, scan.run);
      if (childStart !== null) {
        connectors.push({
          id: `call:${call.callId}`,
          kind: "call",
          from: { site: scan.site, run: scan.run, t: Math.min(call.ts, childStart), row: parentRow, subRow: callSubRow },
          to: {
            site: firstChildBar?.site ?? child.site,
            run: child.run,
            t: childStart,
            row: firstChildBar?.row ?? row(child.site, child.run),
            subRow: null,
          },
          callId: call.callId,
          ok: null,
          pending: false,
          label: `${call.function}() → ${child.site}/${child.run}`,
        });
      }

      const lastChildBar = childBars[childBars.length - 1];
      if (!lastChildBar || (lastChildBar.endKind !== "completed" && lastChildBar.endKind !== "failed")) continue;
      const childEnd = lastChildBar.end;
      const parentBars = barsByRun.get(scan.run) ?? [];

      // The result can be received on the other site when the parent migrated in between.
      const siblings = [scan, ...scans.filter((other) => other !== scan && other.run === scan.run)];
      let received: { t: number; site: Site; row: number; subRow: number | null; pending: boolean } | null = null;
      for (const sibling of siblings) {
        const entry = sibling.calls.get(call.callId);
        if (!entry || entry.receivedTs === null) continue;
        const sameThreadBar = sibling === scan && entry.receivedSegment === call.segment;
        received = {
          t: entry.receivedTs,
          site: sibling.site,
          row: row(sibling.site, sibling.run),
          subRow: sameThreadBar ? callSubRow : null,
          pending: false,
        };
        break;
      }
      if (received === null) {
        const resumedIn = parentBars.find((bar) => bar.segment > call.segment && bar.start >= childEnd);
        const stillRunning = parentBars.some((bar) => bar.endKind === "open");
        const returned = siblings
          .map((sibling) => ({ sibling, entry: sibling.calls.get(call.callId) }))
          .find(({ entry }) => entry !== undefined && entry.returnedTs !== null);
        if (resumedIn) {
          received = { t: resumedIn.start, site: resumedIn.site, row: resumedIn.row, subRow: null, pending: false };
        } else if (returned?.entry && returned.entry.returnedTs !== null && !stillRunning) {
          received = {
            t: returned.entry.returnedTs,
            site: returned.sibling.site,
            row: row(returned.sibling.site, scan.run),
            subRow: null,
            pending: true,
          };
        }
      }
      if (received === null) continue;
      connectors.push({
        id: `return:${call.callId}`,
        kind: "return",
        from: { site: lastChildBar.site, run: child.run, t: childEnd, row: lastChildBar.row, subRow: null },
        to: { site: received.site, run: scan.run, t: Math.max(received.t, childEnd), row: received.row, subRow: received.subRow },
        callId: call.callId,
        ok: call.ok ?? lastChildBar.endKind === "completed",
        pending: received.pending,
        label: `${child.site}/${child.run} returns to ${scan.run}`,
      });
    }
  }

  // Time range.
  const times: number[] = [];
  for (const bar of segments) times.push(bar.start, bar.end);
  for (const gap of gaps) times.push(gap.start, gap.end);
  for (const marker of markers) times.push(marker.t);
  for (const connector of connectors) times.push(connector.from.t, connector.to.t);
  if (times.length === 0) {
    for (const run of input.runs) times.push(run.created_ts);
  }
  if (live) times.push(now);
  const t0 = times.length > 0 ? Math.min(...times) : now;
  const t1 = times.length > 0 ? Math.max(...times) : now;

  return { t0, t1: Math.max(t1, t0), live, lanes, segments, gaps, threads, waits, connectors, markers };
}

function findChild(call: RawCall, scan: RunScan, runs: readonly Run[]): { site: Site; run: string } | null {
  if (call.childSite !== null && call.childRun !== null) return { site: call.childSite, run: call.childRun };
  const waiting = scan.record?.waiting_on.find((entry) => entry.call_id === call.callId);
  if (waiting) return { site: waiting.child_site, run: waiting.child_run };
  const child = runs.find((run) => run.parent?.run === scan.run && run.parent.call_id === call.callId);
  return child ? { site: child.site, run: child.id } : null;
}
