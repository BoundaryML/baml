/**
 * Application state and the single reducer that both event streams feed.
 *
 * The reducer keeps run records and one event list per run. The timeline, the
 * log lanes, and the source view derive everything else from these two maps.
 */

import {
  DEFAULT_SITES,
  isLiveStatus,
  isSuspendedStatus,
  normalizeRun,
  type FunctionInfo,
  type PauseBlocked,
  type PositionEvent,
  type Run,
  type RunHistoryEvent,
  type RunStatus,
  type Site,
  type SiteEntry,
  type SiteInfo,
  type SseEvent,
} from "./protocol";

/** Identifies a run record. A migrated run has the same id on every site that held it. */
export type RunKey = `${Site}/${string}`;

export function runKey(site: Site, id: string): RunKey {
  return `${site}/${id}`;
}

export function splitRunKey(key: RunKey): { site: Site; id: string } {
  const slash = key.indexOf("/");
  return { site: key.slice(0, slash) as Site, id: key.slice(slash + 1) };
}

/**
 * A status change of a run record, as observed by the app.
 *
 * The worker emits no event when its process is killed, and it emits no event
 * at the moment a pause is requested. The reducer records the change of the
 * run record instead, so that the timeline can place both.
 */
export interface StatusEvent {
  type: "ui_status";
  ts: number;
  site: Site;
  run: string;
  segment: number;
  pid: number | null;
  status: RunStatus;
}

export type TimelineEvent = RunHistoryEvent | StatusEvent;

export type ConnectionStatus = "connecting" | "open" | "disconnected" | "fixture";

export interface SiteConnection {
  status: ConnectionStatus;
  /** Number of `init` messages received. It increases at every reconnect. */
  generation: number;
  info: SiteInfo | null;
}

export interface AppState {
  /** The site registry, in registry order (section 8.1). Lanes and pickers follow this order. */
  sites: readonly SiteEntry[];
  connections: Record<Site, SiteConnection>;
  runs: Record<RunKey, Run>;
  events: Record<RunKey, TimelineEvent[]>;
  selected: RunKey | null;
}

export type Action =
  | { type: "sites"; sites: readonly SiteEntry[] }
  | { type: "connection"; site: Site; status: ConnectionStatus }
  | { type: "info"; site: Site; info: SiteInfo }
  | { type: "sse"; site: Site; event: SseEvent }
  | { type: "run_response"; site: Site; run: Run; ts: number }
  | { type: "backfill"; site: Site; run: string; events: SseEvent[] }
  | { type: "select"; key: RunKey | null }
  | { type: "reset" };

const NEW_CONNECTION: SiteConnection = { status: "connecting", generation: 0, info: null };

/** The connection record of a site. A site without one has not been heard of yet. */
export function connectionOf(state: Pick<AppState, "connections">, site: Site): SiteConnection {
  return state.connections[site] ?? NEW_CONNECTION;
}

export function initialState(sites: readonly SiteEntry[] = DEFAULT_SITES): AppState {
  return {
    sites,
    connections: Object.fromEntries(sites.map((site) => [site.name, NEW_CONNECTION])),
    runs: {},
    events: {},
    selected: null,
  };
}

// ---------------------------------------------------------------------------
// Event list maintenance
// ---------------------------------------------------------------------------

function field(event: TimelineEvent, name: string): string {
  const value = (event as unknown as Record<string, unknown>)[name];
  return value === undefined || value === null ? "" : String(value);
}

/** Identity of an event for the merge of a backfill with the live stream. */
export function eventIdentity(event: TimelineEvent): string {
  return [
    event.type,
    event.ts,
    field(event, "segment"),
    field(event, "thread"),
    field(event, "call_id"),
    field(event, "line"),
    field(event, "status"),
    field(event, "text"),
  ].join("|");
}

function sortByTs(events: TimelineEvent[]): TimelineEvent[] {
  // Array.prototype.sort is stable, so events with equal `ts` keep their order.
  return events.sort((a, b) => a.ts - b.ts);
}

function appendEvent(list: readonly TimelineEvent[] | undefined, event: TimelineEvent): TimelineEvent[] {
  const next = list ? [...list, event] : [event];
  const previous = next[next.length - 2];
  if (previous && previous.ts > event.ts) sortByTs(next);
  return next;
}

/**
 * Merges the history of a run with the events that arrived on the live stream.
 * An event that is present in both lists appears once in the result.
 */
export function mergeEvents(history: readonly TimelineEvent[], live: readonly TimelineEvent[]): TimelineEvent[] {
  const counts = new Map<string, number>();
  for (const event of history) {
    const id = eventIdentity(event);
    counts.set(id, (counts.get(id) ?? 0) + 1);
  }
  const merged = [...history];
  for (const event of live) {
    const id = eventIdentity(event);
    const remaining = counts.get(id) ?? 0;
    if (remaining > 0) {
      counts.set(id, remaining - 1);
    } else {
      merged.push(event);
    }
  }
  return sortByTs(merged);
}

function statusEvent(run: Run, ts: number): StatusEvent {
  return {
    type: "ui_status",
    ts,
    site: run.site,
    run: run.id,
    segment: run.segment,
    pid: run.pid,
    status: run.status,
  };
}

// ---------------------------------------------------------------------------
// Reducer
// ---------------------------------------------------------------------------

/** The newest run that is neither a remote child nor a migrated continuation. */
function defaultSelection(runs: Record<RunKey, Run>): RunKey | null {
  let best: Run | null = null;
  for (const run of Object.values(runs)) {
    if (run.parent !== null || run.origin !== null) continue;
    if (best === null || run.created_ts > best.created_ts) best = run;
  }
  return best ? runKey(best.site, best.id) : null;
}

function upsertRun(state: AppState, incoming: Run, ts: number): AppState {
  const run = normalizeRun(incoming);
  const key = runKey(run.site, run.id);
  const previous = state.runs[key];
  if (previous && previous.updated_ts > run.updated_ts) {
    // A command response can arrive after a newer record from the stream.
    return state;
  }
  const runs = { ...state.runs, [key]: run };
  let events = state.events;
  if (!previous || previous.status !== run.status || previous.segment !== run.segment) {
    events = { ...events, [key]: appendEvent(events[key], statusEvent(run, ts)) };
  }
  return {
    ...state,
    runs,
    events,
    selected: state.selected ?? defaultSelection(runs),
  };
}

function reduceSse(state: AppState, site: Site, event: SseEvent): AppState {
  switch (event.type) {
    case "init": {
      const runs: Record<RunKey, Run> = {};
      for (const [key, run] of Object.entries(state.runs) as [RunKey, Run][]) {
        if (run.site !== site) runs[key] = run;
      }
      for (const raw of event.runs) {
        const run = normalizeRun({ ...raw, site });
        runs[runKey(site, run.id)] = run;
      }
      const connection = connectionOf(state, site);
      const selectedStillExists = state.selected !== null && runs[state.selected] !== undefined;
      return {
        ...state,
        runs,
        connections: {
          ...state.connections,
          [site]: { ...connection, generation: connection.generation + 1 },
        },
        selected: selectedStillExists ? state.selected : defaultSelection(runs),
      };
    }
    case "run":
      return upsertRun(state, { ...event.run, site }, event.ts);
    default: {
      const key = runKey(site, event.run);
      return { ...state, events: { ...state.events, [key]: appendEvent(state.events[key], event) } };
    }
  }
}

/**
 * Converts the contents of `events.jsonl` into timeline events. A `run`
 * message in the file becomes a status event when the status or the segment
 * differs from the previous `run` message.
 */
export function historyToTimelineEvents(run: string, history: readonly SseEvent[]): TimelineEvent[] {
  const result: TimelineEvent[] = [];
  let lastStatus: RunStatus | null = null;
  let lastSegment: number | null = null;
  for (const event of history) {
    if (event.type === "init") continue;
    if (event.type === "run") {
      if (event.run.id !== run) continue;
      const record = normalizeRun(event.run);
      if (record.status !== lastStatus || record.segment !== lastSegment) {
        result.push(statusEvent({ ...record, site: event.site }, event.ts));
        lastStatus = record.status;
        lastSegment = record.segment;
      }
      continue;
    }
    if (event.run === run) result.push(event);
  }
  return sortByTs(result);
}

export function reducer(state: AppState, action: Action): AppState {
  switch (action.type) {
    case "sites": {
      const same =
        state.sites.length === action.sites.length &&
        state.sites.every((site, index) => site.name === action.sites[index]?.name && site.url === action.sites[index]?.url);
      if (same) return state;
      // A site that leaves the registry keeps its runs. Only the lanes and pickers follow the registry.
      const connections = { ...state.connections };
      for (const site of action.sites) connections[site.name] ??= NEW_CONNECTION;
      return { ...state, sites: action.sites, connections };
    }
    case "connection": {
      const connection = connectionOf(state, action.site);
      if (connection.status === action.status) return state;
      return {
        ...state,
        connections: { ...state.connections, [action.site]: { ...connection, status: action.status } },
      };
    }
    case "info": {
      const connection = connectionOf(state, action.site);
      return {
        ...state,
        connections: { ...state.connections, [action.site]: { ...connection, info: action.info } },
      };
    }
    case "sse":
      return reduceSse(state, action.site, action.event);
    case "run_response":
      return upsertRun(state, { ...action.run, site: action.run.site ?? action.site }, action.ts);
    case "backfill": {
      const key = runKey(action.site, action.run);
      const history = historyToTimelineEvents(action.run, action.events);
      return { ...state, events: { ...state.events, [key]: mergeEvents(history, state.events[key] ?? []) } };
    }
    case "select":
      return state.selected === action.key ? state : { ...state, selected: action.key };
    case "reset":
      return initialState(state.sites);
  }
}

// ---------------------------------------------------------------------------
// Selectors
// ---------------------------------------------------------------------------

/**
 * The run tree of a run: the run, its migrated continuations (the same run id
 * on any other site), its forks, the run it was forked from, and,
 * transitively, the remote children of all of them.
 *
 * Children are found through three sources, because any one of them can be
 * missing at a given moment: the `parent` field of the child's record, the
 * `waiting_on` list of the parent's record, and `remote_dispatched` events.
 */
export function runTree(state: Pick<AppState, "runs" | "events">, root: RunKey | null): RunKey[] {
  if (root === null) return [];
  const members = new Set<RunKey>();
  const queue: RunKey[] = [];
  const visit = (key: RunKey): void => {
    if (members.has(key)) return;
    members.add(key);
    queue.push(key);
  };
  // Every key under which a run id is known. A migrated run has one key per site that held it.
  const keysById = new Map<string, RunKey[]>();
  for (const key of [...Object.keys(state.runs), ...Object.keys(state.events)] as RunKey[]) {
    const { id } = splitRunKey(key);
    const keys = keysById.get(id) ?? [];
    if (!keys.includes(key)) keys.push(key);
    keysById.set(id, keys);
  }
  const visitAllSites = (id: string, knownSite: Site): void => {
    visit(runKey(knownSite, id));
    for (const key of keysById.get(id) ?? []) visit(key);
  };

  const rootParts = splitRunKey(root);
  visitAllSites(rootParts.id, rootParts.site);

  const allRuns = Object.values(state.runs);
  while (queue.length > 0) {
    const key = queue.shift() as RunKey;
    const { id } = splitRunKey(key);
    for (const candidate of allRuns) {
      if (candidate.parent?.run === id) visitAllSites(candidate.id, candidate.site);
      // A fork is created on the site of its source (section 7.3).
      if (candidate.forked_from?.run === id) visitAllSites(candidate.id, candidate.site);
    }
    const self = state.runs[key];
    if (self?.forked_from) visitAllSites(self.forked_from.run, self.site);
    for (const waiting of state.runs[key]?.waiting_on ?? []) {
      visitAllSites(waiting.child_run, waiting.child_site);
    }
    for (const event of state.events[key] ?? []) {
      if (event.type === "remote_dispatched") visitAllSites(event.child_run, event.child_site);
      if (event.type === "forked") visitAllSites(event.from_run, event.site);
    }
  }
  return [...members];
}

/** The most recent `position` event of a run, over all of its threads. */
export function latestPosition(events: readonly TimelineEvent[] | undefined): PositionEvent | null {
  if (!events) return null;
  for (let i = events.length - 1; i >= 0; i--) {
    const event = events[i];
    if (event && event.type === "position") return event;
  }
  return null;
}

/** The most recent `position` event of every thread in the current segment. */
export function threadPositions(events: readonly TimelineEvent[] | undefined, segment: number): PositionEvent[] {
  const byThread = new Map<number, PositionEvent>();
  for (const event of events ?? []) {
    if (event.type === "position" && event.segment === segment) byThread.set(event.thread, event);
    if (event.type === "thread_ended" && event.segment === segment) byThread.delete(event.thread);
  }
  return [...byThread.values()];
}

/**
 * The latest `blocked` answer to the pause request of a run whose status is
 * `pausing`, or `null`. The run record carries it (section 7.3). Without the
 * field, the `blocked` events of the current segment give the same answer.
 */
export function pauseBlocked(run: Run | null | undefined, events: readonly TimelineEvent[] | undefined): PauseBlocked | null {
  if (!run || run.status !== "pausing") return null;
  if (run.blocked) return run.blocked;
  let latest: PauseBlocked | null = null;
  let attempts = 0;
  for (const event of events ?? []) {
    if (event.type === "ui_status" && event.status === "pausing") attempts = 0;
    if (event.type !== "blocked" || event.segment !== run.segment) continue;
    attempts += 1;
    latest = { reason: event.reason, path: event.path ?? [], ts: event.ts, attempts };
  }
  return latest;
}

/** `resume_on` resumes the run on another site of the registry. The command names that site. */
export type RunAction = "pause" | "resume_here" | "resume_on" | "kill" | "cancel" | "fork";

/** The sites that a paused run can move to: every site of the registry except the one that holds it. */
export function resumeTargets(sites: readonly SiteEntry[], run: Pick<Run, "site"> | null | undefined): Site[] {
  return sites.flatMap((site) => (run && site.name === run.site ? [] : [site.name]));
}

/** The commands that are valid for a run in its current status (section 3.3). */
export function validActions(run: Run | null | undefined): Record<RunAction, boolean> {
  const none = { pause: false, resume_here: false, resume_on: false, kill: false, cancel: false, fork: false };
  if (!run) return none;
  const live = isLiveStatus(run.status);
  const hasSnapshot = run.snapshots.length > 0;
  // Section 9.3: a `sleeping` run can be resumed before its timer fires, and a
  // run without a process (`paused`, `sleeping`) can be cancelled. `kill` needs
  // a process, so it is refused for both.
  const suspended = isSuspendedStatus(run.status);
  return {
    pause: run.status === "running",
    resume_here: suspended && hasSnapshot,
    resume_on: suspended && hasSnapshot,
    kill: live,
    cancel: live || suspended,
    fork: hasSnapshot,
  };
}

/** Why a row is listed under the row above it. */
export type RunRel = "child" | "fork" | "moved";

/** One row of the runs list. Related runs follow their parent, one level deeper. */
export interface RunRow {
  key: RunKey;
  run: Run;
  depth: number;
  /** Number of rows listed under this one: remote children, forks, and records left behind by a migration. */
  children: number;
  /** Why this row is nested, or null for a top-level row. */
  rel: RunRel | null;
}

/**
 * The record of `run.forked_from` to list the fork under. A fork is created on
 * the site of its source, and a fork that migrated keeps `forked_from`, so the
 * source can be more than one migration away.
 */
export function forkSourceKey(run: Run, known: ReadonlySet<RunKey>): RunKey | null {
  if (!run.forked_from) return null;
  const id = run.forked_from.run;
  const preferred = [runKey(run.site, id), ...(run.origin ? [runKey(run.origin.site, id)] : [])];
  const anywhere = [...known].filter((key) => key.endsWith(`/${id}`));
  return [...preferred, ...anywhere].find((key) => known.has(key)) ?? preferred[0] ?? null;
}

/**
 * The runs list, grouped: every run that no listed run called, newest first,
 * each followed by its remote children in call order, and their children in
 * turn. A child whose parent is not in the list is a top-level row. A parent
 * that migrated has one record per site. Its children follow the record on the
 * site that made the call (`parent.site`), or the first record of that id.
 */
export function groupRuns(runs: readonly Run[]): RunRow[] {
  const keyOf = (run: Run): RunKey => runKey(run.site, run.id);
  const byId = new Map<string, Run[]>();
  for (const run of runs) byId.set(run.id, [...(byId.get(run.id) ?? []), run]);
  const known = new Set(runs.map(keyOf));
  const nested = new Map<RunKey, { run: Run; rel: RunRel }[]>();
  const relOf = new Map<RunKey, RunRel>();
  const top: Run[] = [];
  const attach = (parent: Run, run: Run, rel: RunRel): void => {
    const key = keyOf(parent);
    nested.set(key, [...(nested.get(key) ?? []), { run, rel }]);
    relOf.set(keyOf(run), rel);
  };
  for (const run of runs) {
    // A record that a migration left behind belongs under the record that carries the run now.
    if (run.migrated_to) {
      const live = (byId.get(run.id) ?? []).find((candidate) => !candidate.migrated_to);
      if (live && live !== run) {
        attach(live, run, "moved");
        continue;
      }
    }
    if (run.parent) {
      const candidates = byId.get(run.parent.run) ?? [];
      const parent = candidates.find((candidate) => candidate.site === run.parent?.site) ?? candidates[0];
      if (parent && parent !== run) {
        attach(parent, run, "child");
        continue;
      }
    }
    if (run.forked_from) {
      const sourceKey = forkSourceKey(run, known);
      const source = sourceKey ? runs.find((candidate) => keyOf(candidate) === sourceKey) : undefined;
      if (source && source !== run) {
        attach(source, run, "fork");
        continue;
      }
    }
    top.push(run);
  }
  const rows: RunRow[] = [];
  const seen = new Set<RunKey>();
  const visit = (run: Run, depth: number): void => {
    const key = keyOf(run);
    if (seen.has(key)) return;
    seen.add(key);
    const children = [...(nested.get(key) ?? [])].sort(
      (a, b) => a.run.created_ts - b.run.created_ts || a.run.id.localeCompare(b.run.id),
    );
    rows.push({ key, run, depth, children: children.length, rel: relOf.get(key) ?? null });
    for (const child of children) visit(child.run, depth + 1);
  };
  for (const run of [...top].sort((a, b) => b.created_ts - a.created_ts || a.site.localeCompare(b.site))) visit(run, 0);
  // A cycle of parents cannot happen on a real server. A record that it would hide is still listed.
  for (const run of runs) visit(run, 0);
  return rows;
}

/** The functions to offer in the picker. Remote functions sort last. */
export function pickerFunctions(info: SiteInfo | null | undefined): FunctionInfo[] {
  const functions = [...(info?.functions ?? [])];
  return functions.sort((a, b) => Number(a.remote) - Number(b.remote));
}
