/**
 * Builds a fixture: a sequence of SSE messages in the form that the site
 * servers produce, plus the state dumps that the snapshots refer to.
 *
 * The builder keeps run records the way a site server does and emits a `run`
 * message after every change, so a fixture exercises the same reducer paths
 * as the live stream.
 */

import {
  DEFAULT_SITES,
  type JsonObject,
  type PauseStats,
  type Run,
  type RunStatus,
  type Site,
  type SiteRunEvent,
  type SourceLocation,
  type SseEvent,
  type StateDump,
  type WorkerEvent,
} from "../protocol";

export interface Fixture {
  name: string;
  title: string;
  /** SSE messages of every site, ordered by `ts`. The `site` field routes each message. */
  events: SseEvent[];
  /** State dumps by `<site>/<run>/<n>`. */
  states: Record<string, StateDump>;
  /**
   * The runs that the scenario guide follows, by role (`root`, and `plain` for
   * the second run of the recovery scenario). Each names the site that started
   * the run. Absent on a fixture that no scenario plays.
   */
  roles?: Record<string, { site: Site; id: string }>;
}

/** The program hash that the fixture workers report (section 9.2). Every site runs the same program. */
export const FIXTURE_PROGRAM_HASH = "9f2c41d7a8e05b36c1d4f7a90b2e6c58d3a1f4079e8b2c6d5a0f3e7b1c9d4a62";

/** A worker event without the fields that the builder fills in. */
type Omitted = "v" | "ts" | "run" | "segment" | "pid" | "site";
export type WorkerEventBody = WorkerEvent extends infer E ? (E extends WorkerEvent ? Omit<E, Omitted> : never) : never;

/** A site event without the timestamp that the builder fills in. */
export type SiteRunEventBody = SiteRunEvent extends infer E ? (E extends SiteRunEvent ? Omit<E, "ts"> : never) : never;

export const FIXTURE_EPOCH = Date.UTC(2026, 8, 19, 17, 0, 0);

export class FixtureBuilder {
  readonly events: SseEvent[] = [];
  readonly states: Record<string, StateDump> = {};
  private readonly runs = new Map<string, Run>();
  /** The last `position` of each thread, by `<site>/<run>#<segment>:<thread>`. */
  private readonly positions = new Map<string, SourceLocation & { function: string }>();
  /** The call site that a `remote_call` reported, by `<run>:<call_id>` (section 10.1). */
  private readonly callSites = new Map<string, SourceLocation | null>();
  /** The calling function that a `remote_call` reported, by `<run>:<call_id>` (section 10.3). */
  private readonly callers = new Map<string, string | null>();

  constructor(private readonly epoch: number = FIXTURE_EPOCH) {
    for (const { name: site } of DEFAULT_SITES) {
      this.events.push({ type: "init", ts: epoch, site, runs: [] });
    }
  }

  ts(offsetMs: number): number {
    return this.epoch + offsetMs;
  }

  run(site: Site, id: string): Run {
    const run = this.runs.get(`${site}/${id}`);
    if (!run) throw new Error(`fixture has no run ${site}/${id}`);
    return run;
  }

  createRun(
    at: number,
    site: Site,
    id: string,
    fn: string,
    args: JsonObject,
    extra: Partial<Pick<Run, "parent" | "origin" | "segment" | "snapshots" | "status" | "forked_from" | "position" | "waiting_on" | "remote_results" | "program_hash" | "calls">> = {},
  ): Run {
    const run: Run = {
      id,
      site,
      function: fn,
      args,
      durable: fn.includes("durable"),
      status: "starting",
      parent: null,
      origin: null,
      segment: 1,
      pid: null,
      position: null,
      waiting_on: [],
      snapshots: [],
      result: null,
      error: null,
      created_ts: this.ts(at),
      updated_ts: this.ts(at),
      remote_results: [],
      forked_from: null,
      result_delivered: false,
      blocked: null,
      wake_at: null,
      program_hash: FIXTURE_PROGRAM_HASH,
      // Section 9.7 and 10.2: the remote calls that workers of the run announced, with their call sites.
      calls: [],
      cancelled_calls: [],
      ...extra,
    };
    this.runs.set(`${site}/${id}`, run);
    this.emitRun(at, run);
    return run;
  }

  /** Applies a change to a run record and emits the `run` message. */
  update(at: number, site: Site, id: string, change: Partial<Run>): Run {
    const run = { ...this.run(site, id), ...change, updated_ts: this.ts(at) };
    this.runs.set(`${site}/${id}`, run);
    this.emitRun(at, run);
    return run;
  }

  setStatus(at: number, site: Site, id: string, status: RunStatus, change: Partial<Run> = {}): Run {
    return this.update(at, site, id, { ...change, status });
  }

  /** The call site that `remote_call` reported for a call, or `null` (section 10.1). */
  callSite(run: string, callId: string): SourceLocation | null {
    return this.callSites.get(`${run}:${callId}`) ?? null;
  }

  /** The calling function that `remote_call` reported for a call, or `null` (section 10.3). */
  caller(run: string, callId: string): string | null {
    return this.callers.get(`${run}:${callId}`) ?? null;
  }

  /**
   * Emits a worker event. A `position` or `blocked` event also updates the run
   * record, as the site server does.
   *
   * Section 10.1: a `remote_call` that names no call site takes it from the
   * `position` that the calling thread reported last, which is the frame the
   * real worker reports it from. A `remote_cancel` that names none repeats the
   * call site of the call it cancels, and the root thread of a segment has no
   * `spawn` site. A fixture that wants a missing location passes `null`.
   */
  worker(at: number, site: Site, id: string, body: WorkerEventBody): void {
    const run = this.run(site, id);
    if (run.pid === null) throw new Error(`fixture run ${site}/${id} has no process at ${at}`);
    const event = { v: 1, ts: this.ts(at), run: id, segment: run.segment, pid: run.pid, site, ...body } as WorkerEvent;
    const threadKey = (thread: number): string => `${site}/${id}#${run.segment}:${thread}`;
    if (event.type === "position") {
      this.positions.set(threadKey(event.thread), { file: event.file, line: event.line, function: event.function });
    }
    if (event.type === "thread_started" && event.parent_thread === null && !("file" in body)) {
      event.file = null;
      event.line = null;
    }
    if (event.type === "remote_call") {
      const from = this.positions.get(threadKey(event.thread)) ?? null;
      if (!("file" in body)) {
        event.file = from?.file ?? null;
        event.line = from?.line ?? null;
      }
      // Section 10.3: the calling function, from the same frame as the call site.
      if (!("caller" in body)) event.caller = from?.function ?? null;
      this.callSites.set(`${id}:${event.call_id}`, event.file != null && event.line != null ? { file: event.file, line: event.line } : null);
      this.callers.set(`${id}:${event.call_id}`, event.caller ?? null);
    }
    if (event.type === "remote_cancel" && !("file" in body)) {
      const from = this.callSite(id, event.call_id);
      event.file = from?.file ?? null;
      event.line = from?.line ?? null;
    }
    this.events.push(event);
    if (event.type === "position") {
      this.update(at, site, id, {
        position: { thread: event.thread, function: event.function, file: event.file, line: event.line },
      });
    }
    if (event.type === "blocked" && run.status === "pausing") {
      this.update(at, site, id, {
        blocked: { reason: event.reason, path: event.path, ts: event.ts, attempts: (run.blocked?.attempts ?? 0) + 1 },
      });
    }
  }

  /**
   * The end of a worker process: the `worker_exit` event, then the `run`
   * message with the status that results from the exit (section 7.2).
   */
  exit(at: number, site: Site, id: string, exitCode: number, status: RunStatus, change: Partial<Run> = {}, signal: string | null = null): Run {
    const run = this.run(site, id);
    this.siteEvent(at, { type: "worker_exit", site, run: id, segment: run.segment, pid: run.pid, exit_code: exitCode, signal, status });
    return this.setStatus(at, site, id, status, { ...change, pid: null, blocked: null });
  }

  /** An automatic snapshot: the `snapshot` worker event and the entry in the run record (section 7.1). */
  autoSnapshot(at: number, site: Site, id: string, n: number, stats: PauseStats, state: Omit<StateDump, "run" | "segment" | "created_ts">): void {
    const run = this.run(site, id);
    const base = `.baml/runs/${id}/snap-${n}`;
    this.states[`${site}/${id}/${n}`] = { run: id, segment: run.segment, created_ts: this.ts(at), ...state };
    this.worker(at, site, id, { type: "snapshot", snapshot_path: `${base}.bamlsnap`, state_path: `${base}.json`, stats, automatic: true });
    this.update(at, site, id, {
      snapshots: [
        ...run.snapshots,
        { n, snapshot_path: `${base}.bamlsnap`, state_path: `${base}.json`, bytes: stats.compressed_bytes ?? 0, ts: this.ts(at), stats, segment: run.segment, automatic: true },
      ],
    });
  }

  /** `POST /api/runs/:id/fork`: a new paused run from snapshot `n` of `source`, and the `forked` event. */
  fork(at: number, site: Site, source: string, id: string, n: number): Run {
    const from = this.run(site, source);
    const snapshot = from.snapshots.find((candidate) => candidate.n === n);
    if (!snapshot) throw new Error(`fixture run ${site}/${source} has no snapshot ${n}`);
    const base = `.baml/runs/${id}/snap-${n}`;
    const state = this.states[`${site}/${source}/${n}`];
    if (state) this.states[`${site}/${id}/${n}`] = state;
    this.siteEvent(at, { type: "forked", site, run: id, from_run: source, n });
    return this.createRun(at, site, id, from.function, from.args, {
      status: "paused",
      segment: snapshot.segment ?? from.segment,
      snapshots: [{ ...snapshot, snapshot_path: `${base}.bamlsnap`, state_path: `${base}.json` }],
      forked_from: { run: source, n },
      position: from.position,
      // A fork copies the call records of its source, and its inherited waits.
      calls: from.calls ?? [],
      waiting_on: from.snapshots.at(-1)?.n === n ? from.waiting_on.map((entry) => ({ ...entry, inherited: true })) : [],
    });
  }

  /**
   * `POST /api/runs/:id/resume {site}` (section 8.3): the record on `from`
   * becomes `migrated` and names the destination, and `to` imports the run with
   * the latest snapshot. The caller starts the next segment on `to`.
   */
  migrate(at: number, from: Site, to: Site, id: string): Run {
    const left = this.setStatus(at, from, id, "migrated", { migrated_to: to });
    this.siteEvent(at + 2, { type: "migrated_out", site: from, run: id, to_site: to });
    const latest = left.snapshots[left.snapshots.length - 1];
    const imported = this.createRun(at + 24, to, id, left.function, left.args, {
      origin: { site: from, run: id },
      segment: left.segment + 1,
      snapshots: latest ? [latest] : [],
      forked_from: left.forked_from,
      position: left.position,
      waiting_on: left.waiting_on,
      calls: left.calls ?? [],
      remote_results: left.remote_results.map((result) => ({ ...result, acked: false })),
    });
    const state = latest ? this.states[`${from}/${id}/${latest.n}`] : undefined;
    if (latest && state) this.states[`${to}/${id}/${latest.n}`] = state;
    this.siteEvent(at + 25, { type: "migrated_in", site: to, run: id, from_site: from });
    return imported;
  }

  siteEvent(at: number, event: SiteRunEventBody): void {
    this.events.push({ ...event, ts: this.ts(at) } as SiteRunEvent);
  }

  snapshot(at: number, site: Site, id: string, n: number, stats: PauseStats, state: Omit<StateDump, "run" | "segment" | "created_ts">): void {
    const run = this.run(site, id);
    const base = `.baml/runs/${id}/snap-${n}`;
    this.states[`${site}/${id}/${n}`] = { run: id, segment: run.segment, created_ts: this.ts(at), ...state };
    this.worker(at, site, id, {
      type: "paused",
      snapshot_path: `${base}.bamlsnap`,
      state_path: `${base}.json`,
      stats,
    });
    this.exit(at + 2, site, id, 75, "paused", {
      snapshots: [
        ...run.snapshots,
        {
          n,
          snapshot_path: `${base}.bamlsnap`,
          state_path: `${base}.json`,
          bytes: stats.compressed_bytes ?? 0,
          ts: this.ts(at),
          stats,
          segment: run.segment,
          automatic: false,
        },
      ],
    });
  }

  /**
   * A run suspends itself for a sleep (sections 9.2 and 9.3): the `paused`
   * event with `wake`, the exit with code 75, the `sleeping` status with
   * `wake_at`, and the `sleep_scheduled` event. Returns `wake_at` as an offset.
   */
  selfSuspend(at: number, site: Site, id: string, n: number, stats: PauseStats, remainingMs: number, state: Omit<StateDump, "run" | "segment" | "created_ts">): number {
    const run = this.run(site, id);
    const base = `.baml/runs/${id}/snap-${n}`;
    this.states[`${site}/${id}/${n}`] = { run: id, segment: run.segment, created_ts: this.ts(at), ...state };
    this.worker(at, site, id, {
      type: "paused",
      snapshot_path: `${base}.bamlsnap`,
      state_path: `${base}.json`,
      stats,
      wake: { reason: "sleep", remaining_ms: remainingMs, at_ts: this.ts(at + remainingMs) },
    });
    // The site server computes `wake_at` as the receipt time plus `remaining_ms`.
    const wakeAt = at + 3 + remainingMs;
    this.exit(at + 2, site, id, 75, "sleeping", {
      wake_at: this.ts(wakeAt),
      snapshots: [
        ...run.snapshots,
        { n, snapshot_path: `${base}.bamlsnap`, state_path: `${base}.json`, bytes: stats.compressed_bytes ?? 0, ts: this.ts(at), stats, segment: run.segment, automatic: false },
      ],
    });
    this.siteEvent(at + 3, { type: "sleep_scheduled", site, run: id, wake_at: this.ts(wakeAt) });
    return wakeAt;
  }

  /** The wake timer fires (section 9.3): the `woken` event, and the run starts its next segment. */
  wake(at: number, site: Site, id: string, reason: "timer" | "manual" | "restart" = "timer"): Run {
    this.siteEvent(at, { type: "woken", site, run: id, reason });
    return this.setStatus(at, site, id, "starting", { segment: this.run(site, id).segment + 1, wake_at: null });
  }

  build(name: string, title: string, extra: Pick<Fixture, "roles"> = {}): Fixture {
    // Stable sort: messages with equal `ts` keep their emission order.
    const events = [...this.events].sort((a, b) => a.ts - b.ts);
    return { name, title, events, states: this.states, ...extra };
  }

  private emitRun(at: number, run: Run): void {
    this.events.push({ type: "run", ts: this.ts(at), site: run.site, run: structuredClone(run) });
  }
}
