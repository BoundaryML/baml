import { describe, expect, it } from "vitest";
import { FIXTURE_EPOCH } from "./fixtures/builder";
import { buildCentralFixture, CENTRAL_CHILD, CENTRAL_PARENT } from "./fixtures/central";
import { buildChainFixture, CHAIN_CHILD, CHAIN_PARENT } from "./fixtures/chain";
import { buildForkFixture, FORK_CHILD, FORK_RUN, FORK_SOURCE } from "./fixtures/fork";
import { buildPoolFixture, POOL_CHILD, POOL_PARENT } from "./fixtures/pool";
import { buildSpawnFixture, SPAWN_CHILD, SPAWN_PARENT } from "./fixtures/spawn";
import { DEFAULT_SITES, isSite, normalizeRun, parseSseEvent, sitePort, sitesFromInfo, type Run, type SseEvent } from "./protocol";
import { loadSiteRegistry, reopenDelay } from "./session";
import {
  connectionOf,
  historyToTimelineEvents,
  initialState,
  latestPosition,
  mergeEvents,
  pauseBlocked,
  reducer,
  resumeTargets,
  runKey,
  runTree,
  validActions,
} from "./state";
import { playFixture } from "./testing";

const central = buildCentralFixture();
const spawn = buildSpawnFixture();
const fork = buildForkFixture();
const pool = buildPoolFixture();
const chain = buildChainFixture();
const at = (ms: number): number => FIXTURE_EPOCH + ms;

describe("reducer", () => {
  it("upserts runs and selects the root run, not the remote child", () => {
    const state = playFixture(central);
    const parent = state.runs[runKey("local", CENTRAL_PARENT)];
    const child = state.runs[runKey("cloud", CENTRAL_CHILD)];
    expect(parent?.status).toBe("completed");
    expect(parent?.segment).toBe(2);
    expect(parent?.snapshots).toHaveLength(1);
    expect(child?.status).toBe("completed");
    expect(child?.parent).toEqual({ site: "local", run: CENTRAL_PARENT, call_id: `${CENTRAL_PARENT}-c1` });
    expect(state.selected).toBe(runKey("local", CENTRAL_PARENT));
  });

  it("appends worker events and site events to the list of their run", () => {
    const state = playFixture(central);
    const events = state.events[runKey("local", CENTRAL_PARENT)] ?? [];
    const types = new Set(events.map((event) => event.type));
    for (const type of ["hello", "log", "position", "remote_call", "remote_dispatched", "paused", "remote_returned", "resumed", "remote_result_received", "completed"]) {
      expect(types.has(type as never), type).toBe(true);
    }
    expect(events.every((event, index) => index === 0 || (events[index - 1]?.ts ?? 0) <= event.ts)).toBe(true);
    const cloudEvents = state.events[runKey("cloud", CENTRAL_CHILD)] ?? [];
    expect(cloudEvents.some((event) => event.type === "log" && event.text.includes("[remote]"))).toBe(true);
  });

  it("records status changes of the run record as ui_status events", () => {
    const state = playFixture(central);
    const statuses = (state.events[runKey("local", CENTRAL_PARENT)] ?? []).flatMap((event) =>
      event.type === "ui_status" ? [`${event.status}#${event.segment}`] : [],
    );
    expect(statuses).toEqual(["starting#1", "running#1", "pausing#1", "paused#1", "starting#2", "running#2", "completed#2"]);
  });

  it("shows the paused parent with no process while the child still runs", () => {
    const state = playFixture(central, at(6000));
    const parent = state.runs[runKey("local", CENTRAL_PARENT)] as Run;
    expect(parent.status).toBe("paused");
    expect(parent.pid).toBeNull();
    expect(state.runs[runKey("cloud", CENTRAL_CHILD)]?.status).toBe("running");
    // Section 9.3: a paused run has no process to kill, and it can be cancelled on the site server.
    expect(validActions(parent)).toEqual({ pause: false, resume_here: true, resume_on: true, kill: false, cancel: true, fork: true });
    expect(latestPosition(state.events[runKey("local", CENTRAL_PARENT)])?.reason).toBe("remote_call");
  });

  it("replaces the runs of one site on init and keeps the other site", () => {
    const state = playFixture(central);
    const next = reducer(state, { type: "sse", site: "cloud", event: { type: "init", ts: at(20_000), site: "cloud", runs: [] } });
    expect(next.runs[runKey("cloud", CENTRAL_CHILD)]).toBeUndefined();
    expect(next.runs[runKey("local", CENTRAL_PARENT)]).toBeDefined();
    expect(connectionOf(next, "cloud").generation).toBe(connectionOf(state, "cloud").generation + 1);
    expect(connectionOf(next, "cloud2").generation).toBe(connectionOf(state, "cloud2").generation);
    // Event lists survive a reconnect.
    expect(next.events[runKey("cloud", CENTRAL_CHILD)]?.length).toBeGreaterThan(0);
  });

  it("ignores a stale run record from a command response", () => {
    const state = playFixture(central);
    const parent = state.runs[runKey("local", CENTRAL_PARENT)] as Run;
    const stale: Run = { ...parent, status: "pausing", updated_ts: parent.updated_ts - 5000 };
    const next = reducer(state, { type: "run_response", site: "local", run: stale, ts: at(30_000) });
    expect(next.runs[runKey("local", CENTRAL_PARENT)]?.status).toBe("completed");
  });

  it("merges a backfill with live events without duplicates", () => {
    const partial = playFixture(central, at(5000));
    const key = runKey("local", CENTRAL_PARENT);
    const liveCount = partial.events[key]?.length ?? 0;
    const history = central.events.filter(
      (event) => event.site === "local" && event.type !== "init" && event.ts <= at(5000),
    );
    const once = reducer(partial, { type: "backfill", site: "local", run: CENTRAL_PARENT, events: history });
    expect(once.events[key]).toHaveLength(liveCount);
    const twice = reducer(once, { type: "backfill", site: "local", run: CENTRAL_PARENT, events: history });
    expect(twice.events[key]).toHaveLength(liveCount);
  });

  it("backfills a run that was never seen live", () => {
    const history = central.events.filter((event) => event.site === "local" && event.type !== "init" && event.type !== "run");
    const state = reducer(initialState(), { type: "backfill", site: "local", run: CENTRAL_PARENT, events: history });
    expect(state.events[runKey("local", CENTRAL_PARENT)]?.length).toBe(history.length);
  });
});

describe("mergeEvents", () => {
  it("keeps two identical log lines that both appear in the history", () => {
    const line = parseSseEvent(
      { v: 1, type: "log", ts: 10, run: "r-1", segment: 1, pid: 7, stream: "stdout", text: "tick", thread: 1 },
      "local",
    );
    expect(line).not.toBeNull();
    const event = line as Exclude<SseEvent, { type: "init" | "run" }>;
    expect(mergeEvents([event, event], [event])).toHaveLength(2);
    expect(mergeEvents([event], [event, event])).toHaveLength(2);
  });
});

describe("historyToTimelineEvents", () => {
  it("turns run messages of events.jsonl into status events", () => {
    const history = central.events.filter((event) => event.site === "local");
    const events = historyToTimelineEvents(CENTRAL_PARENT, history);
    expect(events.filter((event) => event.type === "ui_status").map((event) => (event.type === "ui_status" ? event.status : ""))).toEqual([
      "starting", "running", "pausing", "paused", "starting", "running", "completed",
    ]);
  });
});

describe("parseSseEvent", () => {
  it("ignores unknown event types and keeps unknown fields", () => {
    expect(parseSseEvent({ type: "gc_stats", ts: 1, site: "local", run: "r-1" }, "local")).toBeNull();
    expect(parseSseEvent("nonsense", "local")).toBeNull();
    const event = parseSseEvent({ type: "thread_ended", ts: 1, run: "r-1", segment: 1, pid: 2, thread: 3, extra: "kept" }, "cloud");
    expect(event).toMatchObject({ type: "thread_ended", site: "cloud", extra: "kept" });
  });
});

describe("accepted extensions (contract section 7)", () => {
  it("parses worker_exit, forked, snapshot, and cancelled", () => {
    const exit = parseSseEvent({ type: "worker_exit", ts: 5, run: "r-1", segment: 1, pid: null, exit_code: -1, signal: "SIGKILL", status: "lost" }, "local");
    expect(exit).toMatchObject({ type: "worker_exit", site: "local", signal: "SIGKILL", status: "lost" });
    expect(parseSseEvent({ type: "forked", ts: 5, site: "cloud", run: "r-2", from_run: "r-1", n: 3 }, "local")).toMatchObject({ type: "forked", site: "cloud", n: 3 });
    expect(parseSseEvent({ v: 1, type: "snapshot", ts: 5, run: "r-1", segment: 1, pid: 9, snapshot_path: "snap-1.bamlsnap", state_path: "snap-1.json", stats: {}, automatic: true }, "local")).toMatchObject({ type: "snapshot", automatic: true });
    expect(parseSseEvent({ v: 1, type: "cancelled", ts: 5, run: "r-1", segment: 1, pid: 9 }, "local")).toMatchObject({ type: "cancelled" });
  });

  it("appends the new events to the list of their run", () => {
    const state = playFixture(fork);
    const types = (key: ReturnType<typeof runKey>) => new Set((state.events[key] ?? []).map((event) => event.type));
    expect([...types(runKey("local", FORK_SOURCE))]).toEqual(expect.arrayContaining(["snapshot", "worker_exit"]));
    expect([...types(runKey("local", FORK_RUN))]).toEqual(expect.arrayContaining(["forked", "cancelled", "worker_exit"]));
    expect(state.runs[runKey("local", FORK_RUN)]).toMatchObject({ status: "cancelled", forked_from: { run: FORK_SOURCE, n: 1 } });
    expect(state.runs[runKey("local", FORK_SOURCE)]?.snapshots.map((snapshot) => [snapshot.n, snapshot.segment, snapshot.automatic])).toEqual([
      [1, 1, true], [2, 1, true], [3, 2, true],
    ]);
  });

  it("fills in the extension fields of a run record from a server that omits them", () => {
    const run = normalizeRun({ id: "r-1", site: "local", status: "running" } as Run);
    expect(run).toMatchObject({ remote_results: [], forked_from: null, result_delivered: false, blocked: null });
  });

  it("merges a backfill that contains worker_exit and forked without duplicates", () => {
    const state = playFixture(fork);
    const key = runKey("local", FORK_RUN);
    const history = fork.events.filter((event) => event.site === "local" && event.type !== "init" && event.type !== "run");
    const next = reducer(state, { type: "backfill", site: "local", run: FORK_RUN, events: history });
    expect(next.events[key]).toHaveLength(state.events[key]?.length ?? 0);
  });
});

describe("pauseBlocked", () => {
  const key = runKey("local", SPAWN_PARENT);

  it("returns the latest blocked reason of the run record while the status is pausing", () => {
    const state = playFixture(spawn, at(3100));
    const run = state.runs[key] as Run;
    expect(run.status).toBe("pausing");
    expect(pauseBlocked(run, state.events[key])).toMatchObject({ reason: "a pending future cannot be written to a snapshot yet", attempts: 2, ts: at(3076) });
  });

  it("falls back to the blocked events when the record has no blocked field", () => {
    const state = playFixture(spawn, at(2500));
    const run: Run = { ...(state.runs[key] as Run), blocked: null };
    expect(pauseBlocked(run, state.events[key])).toMatchObject({ attempts: 1, ts: at(1906), path: expect.arrayContaining(["Future<string>"]) });
    expect(pauseBlocked(run, undefined)).toBeNull();
  });

  it("returns null before a blocked answer and after the pause succeeded", () => {
    const before = playFixture(spawn, at(1903));
    expect(before.runs[key]?.status).toBe("pausing");
    expect(pauseBlocked(before.runs[key], before.events[key])).toBeNull();
    const after = playFixture(spawn, at(4000));
    expect(after.runs[key]?.status).toBe("paused");
    expect(after.runs[key]?.blocked).toBeNull();
    expect(pauseBlocked(after.runs[key], after.events[key])).toBeNull();
  });
});

describe("runTree", () => {
  it("contains the forks of a run, and the source of a fork", () => {
    const state = playFixture(fork);
    const expected = [runKey("local", FORK_SOURCE), runKey("local", FORK_RUN), runKey("cloud", FORK_CHILD)].sort();
    expect(runTree(state, runKey("local", FORK_SOURCE)).sort()).toEqual(expected);
    expect(runTree(state, runKey("local", FORK_RUN)).sort()).toEqual(expected);
  });

  it("contains the run and its remote child", () => {
    const state = playFixture(central);
    expect(runTree(state, runKey("local", CENTRAL_PARENT)).sort()).toEqual(
      [runKey("cloud", CENTRAL_CHILD), runKey("local", CENTRAL_PARENT)].sort(),
    );
    // The tree of the child does not include its parent.
    expect(runTree(state, runKey("cloud", CENTRAL_CHILD))).toEqual([runKey("cloud", CENTRAL_CHILD)]);
  });

  it("contains the migrated continuation on the other site", () => {
    const state = playFixture(spawn);
    const expected = [runKey("local", SPAWN_PARENT), runKey("cloud", SPAWN_PARENT), runKey("cloud", SPAWN_CHILD)].sort();
    expect(runTree(state, runKey("local", SPAWN_PARENT)).sort()).toEqual(expected);
    expect(runTree(state, runKey("cloud", SPAWN_PARENT)).sort()).toEqual(expected);
    expect(state.runs[runKey("local", SPAWN_PARENT)]?.status).toBe("migrated");
    expect(state.runs[runKey("cloud", SPAWN_PARENT)]?.origin).toEqual({ site: "local", run: SPAWN_PARENT });
  });

  it("finds the child from remote_dispatched before the child's record arrives", () => {
    const state = playFixture(central);
    const withoutChildRecord = { runs: { [runKey("local", CENTRAL_PARENT)]: state.runs[runKey("local", CENTRAL_PARENT)] as Run }, events: state.events };
    expect(runTree(withoutChildRecord, runKey("local", CENTRAL_PARENT))).toContain(runKey("cloud", CENTRAL_CHILD));
  });
});

describe("validActions", () => {
  const base = playFixture(central).runs[runKey("local", CENTRAL_PARENT)] as Run;
  it("enables only the commands that fit the status", () => {
    expect(validActions({ ...base, status: "running", snapshots: [] })).toEqual({ pause: true, resume_here: false, resume_on: false, kill: true, cancel: true, fork: false });
    expect(validActions({ ...base, status: "pausing" }).pause).toBe(false);
    expect(validActions({ ...base, status: "completed" })).toEqual({ pause: false, resume_here: false, resume_on: false, kill: false, cancel: false, fork: true });
    expect(validActions({ ...base, status: "lost", snapshots: [] })).toEqual({ pause: false, resume_here: false, resume_on: false, kill: false, cancel: false, fork: false });
    expect(validActions(null).pause).toBe(false);
  });

  it("resumes and cancels a sleeping run, and refuses to kill it (section 9.3)", () => {
    expect(validActions({ ...base, status: "sleeping" })).toEqual({ pause: false, resume_here: true, resume_on: true, kill: false, cancel: true, fork: true });
    // Without a snapshot there is nothing to resume from.
    expect(validActions({ ...base, status: "sleeping", snapshots: [] })).toMatchObject({ resume_here: false, resume_on: false, cancel: true });
  });
});

describe("named sites (contract section 8)", () => {
  it("starts with the default registry of three sites, all connecting", () => {
    const state = initialState();
    expect(state.sites.map((site) => site.name)).toEqual(["local", "cloud", "cloud2"]);
    expect(Object.keys(state.connections)).toEqual(["local", "cloud", "cloud2"]);
    expect(connectionOf(state, "cloud2")).toEqual({ status: "connecting", generation: 0, info: null });
  });

  it("takes the registry from the sites action, in registry order, and keeps known connections", () => {
    let state = reducer(initialState(), { type: "connection", site: "cloud", status: "open" });
    const sites = [
      { name: "edge", url: "http://127.0.0.1:9001" },
      { name: "cloud", url: "http://127.0.0.1:8788" },
    ];
    state = reducer(state, { type: "sites", sites });
    expect(state.sites).toEqual(sites);
    expect(connectionOf(state, "cloud").status).toBe("open");
    expect(connectionOf(state, "edge").status).toBe("connecting");
    // The same registry again is not a change.
    expect(reducer(state, { type: "sites", sites: [...sites] })).toBe(state);
    // A reset keeps the registry.
    expect(reducer(state, { type: "reset" }).sites).toEqual(sites);
  });

  it("keeps the state of the other sites when one site is disconnected", () => {
    let state = playFixture(pool);
    for (const site of ["local", "cloud", "cloud2"]) state = reducer(state, { type: "connection", site, status: "open" });
    const next = reducer(state, { type: "connection", site: "cloud2", status: "disconnected" });
    expect(connectionOf(next, "cloud2").status).toBe("disconnected");
    expect(connectionOf(next, "local").status).toBe("open");
    expect(connectionOf(next, "cloud").status).toBe("open");
    expect(next.runs).toBe(state.runs);
    expect(next.events).toBe(state.events);
  });

  it("accepts events and runs of a site that is not in the registry", () => {
    const run = normalizeRun({ id: "r-edge", site: "edge", status: "running" } as Run);
    let state = reducer(initialState(), { type: "sse", site: "edge", event: { type: "init", ts: at(0), site: "edge", runs: [run] } });
    state = reducer(state, { type: "connection", site: "edge", status: "open" });
    expect(state.runs[runKey("edge", "r-edge")]?.status).toBe("running");
    expect(connectionOf(state, "edge")).toMatchObject({ status: "open", generation: 1 });
  });

  it("offers every other site of the registry as a resume target", () => {
    expect(resumeTargets(DEFAULT_SITES, { site: "local" })).toEqual(["cloud", "cloud2"]);
    expect(resumeTargets(DEFAULT_SITES, { site: "cloud" })).toEqual(["local", "cloud2"]);
    expect(resumeTargets(DEFAULT_SITES, { site: "cloud2" })).toEqual(["local", "cloud"]);
    expect(resumeTargets(DEFAULT_SITES, null)).toEqual(["local", "cloud", "cloud2"]);
  });

  it("plays the pool fixture: paused on local, resumed on cloud, remote child on cloud2", () => {
    const state = playFixture(pool);
    expect(state.runs[runKey("local", POOL_PARENT)]).toMatchObject({ status: "migrated", migrated_to: "cloud", segment: 1 });
    expect(state.runs[runKey("cloud", POOL_PARENT)]).toMatchObject({ status: "completed", segment: 2, origin: { site: "local", run: POOL_PARENT } });
    expect(state.runs[runKey("cloud2", POOL_CHILD)]).toMatchObject({
      status: "completed",
      parent: { site: "cloud", run: POOL_PARENT, call_id: `${POOL_PARENT}-c1` },
    });
    expect(state.selected).toBe(runKey("local", POOL_PARENT));
    const expected = [runKey("local", POOL_PARENT), runKey("cloud", POOL_PARENT), runKey("cloud2", POOL_CHILD)].sort();
    expect(runTree(state, runKey("local", POOL_PARENT)).sort()).toEqual(expected);
    expect(runTree(state, runKey("cloud", POOL_PARENT)).sort()).toEqual(expected);
    const cloud2Events = state.events[runKey("cloud2", POOL_CHILD)] ?? [];
    expect(cloud2Events.some((event) => event.type === "log")).toBe(true);
    const dispatched = (state.events[runKey("cloud", POOL_PARENT)] ?? []).find((event) => event.type === "remote_dispatched");
    expect(dispatched).toMatchObject({ site: "cloud", child_site: "cloud2", child_run: POOL_CHILD });
  });

  it("puts the record of a run on every site of a migration chain into one tree", () => {
    const state = playFixture(chain);
    const expected = [
      runKey("local", CHAIN_PARENT),
      runKey("cloud", CHAIN_PARENT),
      runKey("cloud2", CHAIN_PARENT),
      runKey("cloud", CHAIN_CHILD),
    ].sort();
    for (const site of ["local", "cloud", "cloud2"]) {
      expect(runTree(state, runKey(site, CHAIN_PARENT)).sort(), site).toEqual(expected);
    }
    expect(state.runs[runKey("cloud", CHAIN_PARENT)]).toMatchObject({ status: "migrated", migrated_to: "cloud2" });
    expect(state.runs[runKey("cloud2", CHAIN_PARENT)]).toMatchObject({ status: "migrated", migrated_to: "local" });
    // The run came back to `local`: the imported record replaced the one that had moved away.
    expect(state.runs[runKey("local", CHAIN_PARENT)]).toMatchObject({ status: "completed", segment: 4, origin: { site: "cloud2" } });
  });
});

describe("site registry", () => {
  it("reads sites in registry order from GET /api/info", () => {
    const info = {
      site: "local",
      sites: [
        { name: "local", url: "http://127.0.0.1:18787" },
        { name: "cloud", url: "http://127.0.0.1:18788" },
        { name: "cloud2", url: "http://127.0.0.1:18789" },
        { name: "cloud", url: "duplicate" },
        { name: "Not A Site", url: "x" },
        "junk",
      ],
      remote_pool: ["cloud", "cloud2"],
    };
    expect(sitesFromInfo(info)).toEqual([
      { name: "local", url: "http://127.0.0.1:18787" },
      { name: "cloud", url: "http://127.0.0.1:18788" },
      { name: "cloud2", url: "http://127.0.0.1:18789" },
    ]);
  });

  it("reads a registry of two sites from a server that predates section 8", () => {
    expect(sitesFromInfo({ site: "cloud", peer_site: "local", peer_url: "http://127.0.0.1:8787" })).toEqual([
      { name: "local", url: "http://127.0.0.1:8787" },
      { name: "cloud", url: "http://127.0.0.1:8788" },
    ]);
    expect(sitesFromInfo({ site: "local" })).toBeNull();
    expect(sitesFromInfo({ site: "local", sites: [] })).toBeNull();
    expect(sitesFromInfo(null)).toBeNull();
  });

  it("falls back to local, cloud, cloud2 when the info request fails or hangs", async () => {
    const failing = { info: () => Promise.reject(new Error("local: 502 Bad Gateway")) };
    expect((await loadSiteRegistry(failing)).sites.map((site) => site.name)).toEqual(["local", "cloud", "cloud2"]);
    const hanging = { info: () => new Promise<never>(() => undefined) };
    expect((await loadSiteRegistry(hanging, 5)).sites).toEqual(DEFAULT_SITES);
  });

  it("asks the local site for the registry", async () => {
    const asked: string[] = [];
    const api = {
      info: (site: string) => {
        asked.push(site);
        return Promise.resolve({ site, program_dir: "", functions: [], sites: [{ name: "local", url: "http://h:1" }, { name: "edge", url: "http://h:2" }] });
      },
    };
    const { sites } = await loadSiteRegistry(api);
    expect(asked).toEqual(["local"]);
    expect(sites.map((site) => site.name)).toEqual(["local", "edge"]);
  });

  it("accepts any site name of the form [a-z0-9]+", () => {
    expect(["local", "cloud2", "eu1"].every(isSite)).toBe(true);
    expect(["", "Cloud", "a/b", "a-b", 7, null].some(isSite)).toBe(false);
    expect(parseSseEvent({ type: "migrated_out", ts: 1, run: "r-1", site: "cloud2", to_site: "edge9" }, "local")).toMatchObject({ site: "cloud2", to_site: "edge9" });
  });

  it("reads the port of a site URL for the lane label", () => {
    expect(sitePort("http://127.0.0.1:18789")).toBe("18789");
    expect(sitePort("https://sites.example/cloud2")).toBeNull();
    expect(sitePort("")).toBeNull();
  });

  it("waits longer between reconnect attempts to a site that stays down", () => {
    expect([1, 2, 3, 4, 9].map(reopenDelay)).toEqual([2000, 4000, 8000, 8000, 8000]);
  });
});
