import { describe, expect, it } from "vitest";
import { FIXTURE_EPOCH } from "../fixtures/builder";
import { buildCentralFixture, CENTRAL_CHILD, CENTRAL_PARENT } from "../fixtures/central";
import { buildChainFixture, CHAIN_CHILD, CHAIN_PARENT } from "../fixtures/chain";
import { buildForkFixture, FORK_CHILD, FORK_RUN, FORK_SOURCE } from "../fixtures/fork";
import { buildPoolFixture, POOL_CHILD, POOL_PARENT } from "../fixtures/pool";
import { buildSpawnFixture, SPAWN_CHILD, SPAWN_PARENT } from "../fixtures/spawn";
import { normalizeRun, parseSseEvent, type Run } from "../protocol";
import { reducer, runKey, runTree, type AppState, type RunKey } from "../state";
import { playFixture } from "../testing";
import { computeTimelineLayout, snapshotNumberFromPath, type TimelineLayout } from "./layout";

const at = (ms: number): number => FIXTURE_EPOCH + ms;

/** Appends one site event to the state, as the live stream does. */
function reducer2(state: AppState, event: Parameters<typeof parseSseEvent>[0]): AppState {
  const parsed = parseSseEvent(event, "local");
  if (parsed === null) throw new Error("the test event does not parse");
  return reducer(state, { type: "sse", site: parsed.site, event: parsed });
}

function layoutOf(state: AppState, root: RunKey, now: number): TimelineLayout {
  const runs = runTree(state, root).flatMap((key) => (state.runs[key] ? [state.runs[key] as Run] : []));
  return computeTimelineLayout({ runs, events: state.events, now });
}

describe("central fixture", () => {
  const fixture = buildCentralFixture();
  const root = runKey("local", CENTRAL_PARENT);
  const done = layoutOf(playFixture(fixture), root, at(60_000));

  it("draws two parent segments with different pids and one child segment", () => {
    const parent = done.segments.filter((bar) => bar.run === CENTRAL_PARENT);
    expect(parent.map((bar) => [bar.site, bar.segment, bar.pid, bar.endKind, bar.mode])).toEqual([
      ["local", 1, 41201, "paused", "start"],
      ["local", 2, 41377, "completed", "resume"],
    ]);
    expect(parent[0]?.start).toBe(at(40));
    // The bar ends at `worker_exit`, 2 ms after the `paused` event.
    expect(parent[0]?.end).toBe(at(5433));
    expect(parent[1]?.start).toBe(at(9848));
    const child = done.segments.filter((bar) => bar.run === CENTRAL_CHILD);
    expect(child.map((bar) => [bar.site, bar.pid, bar.endKind])).toEqual([["cloud", 52077, "completed"]]);
    expect(done.live).toBe(false);
    expect(done.t0).toBe(at(40));
    expect(done.t1).toBe(at(10_228));
  });

  it("draws the paused interval as a gap labeled with the snapshot size", () => {
    expect(done.gaps).toHaveLength(1);
    expect(done.gaps[0]).toMatchObject({
      run: CENTRAL_PARENT, site: "local", start: at(5433), end: at(9848), open: false, snapshotN: 1, bytes: 2948,
    });
  });

  it("shades the wait on the remote call in both segments", () => {
    const waits = done.waits.filter((wait) => wait.run === CENTRAL_PARENT);
    expect(waits.map((wait) => [wait.segment, wait.start, wait.end, wait.subRow])).toEqual([
      [1, at(4557), at(5433), null],
      // The carried wait starts at `resumed`, not at `hello`: the real worker
      // loads the program for about a second in between.
      [2, at(9852), at(9853), null],
    ]);
  });

  it("connects the call to the child and the return to the point of receipt", () => {
    const call = done.connectors.find((connector) => connector.kind === "call");
    expect(call).toMatchObject({
      from: { site: "local", run: CENTRAL_PARENT, t: at(4557), subRow: null },
      to: { site: "cloud", run: CENTRAL_CHILD, t: at(4610) },
    });
    const back = done.connectors.find((connector) => connector.kind === "return");
    expect(back).toMatchObject({
      from: { site: "cloud", run: CENTRAL_CHILD, t: at(7623) },
      to: { site: "local", run: CENTRAL_PARENT, t: at(9853) },
      ok: true,
      pending: false,
    });
    expect(done.connectors.filter((connector) => connector.kind === "migration")).toHaveLength(0);
  });

  it("places markers for the pause request, the snapshot, and the resume", () => {
    expect(done.markers.map((marker) => [marker.kind, marker.t, marker.segment])).toEqual([
      ["pause_request", at(5400), 1],
      ["snapshot", at(5431), 1],
      ["resume", at(9852), 2],
    ]);
    const snapshot = done.markers.find((marker) => marker.kind === "snapshot");
    expect(snapshot).toMatchObject({ n: 1, bytes: 2948, automatic: false });
    expect(snapshot?.kind === "snapshot" ? snapshot.stats?.pause_latency_ms : null).toBe(31);
  });

  it("keeps the gap open and the child bar growing while the parent is paused", () => {
    const now = at(6500);
    const paused = layoutOf(playFixture(fixture, now), root, now);
    expect(paused.live).toBe(true);
    expect(paused.gaps[0]).toMatchObject({ open: true, start: at(5433), end: now });
    const child = paused.segments.find((bar) => bar.run === CENTRAL_CHILD);
    expect(child).toMatchObject({ endKind: "open", end: now });
    expect(paused.connectors.map((connector) => connector.kind)).toEqual(["call"]);
    expect(paused.t1).toBe(now);
  });

  it("lands the return on the gap while the result waits for the resume", () => {
    const now = at(8500);
    const waiting = layoutOf(playFixture(fixture, now), root, now);
    const back = waiting.connectors.find((connector) => connector.kind === "return");
    expect(back).toMatchObject({ to: { site: "local", t: at(7631) }, pending: true });
    expect(waiting.live).toBe(false);
    // Nothing runs: the open gap ends at the last known time instead of the clock.
    expect(waiting.gaps[0]?.end).toBe(at(7623));
  });

  it("derives the pause request from pause_latency_ms when no status change was seen", () => {
    const state = playFixture(fixture);
    const key = runKey("local", CENTRAL_PARENT);
    const withoutStatus: AppState = {
      ...state,
      events: { ...state.events, [key]: (state.events[key] ?? []).filter((event) => event.type !== "ui_status") },
    };
    const layout = layoutOf(withoutStatus, root, at(60_000));
    expect(layout.markers.find((marker) => marker.kind === "pause_request")?.t).toBe(at(5400));
  });

  it("uses one row per lane", () => {
    expect(done.lanes.map((lane) => [lane.site, lane.rows.length, lane.rows[0]?.subRows])).toEqual([
      ["local", 1, 0],
      ["cloud", 1, 0],
      // Every site of the registry has a lane, also when no run of the tree is on it.
      ["cloud2", 1, 0],
    ]);
  });
});

describe("spawn fixture", () => {
  const fixture = buildSpawnFixture();
  const root = runKey("local", SPAWN_PARENT);
  const done = layoutOf(playFixture(fixture), root, at(60_000));

  it("draws the spawned thread as a sub-bar under the parent's bar", () => {
    expect(done.threads).toHaveLength(1);
    expect(done.threads[0]).toMatchObject({
      site: "local", run: SPAWN_PARENT, segment: 1, thread: 2, parentThread: 1, start: at(58), end: at(3150), subRow: 0,
    });
    expect(done.lanes.find((lane) => lane.site === "local")?.rows[0]?.subRows).toBe(1);
  });

  it("shades the wait on the sub-bar, not on the main bar", () => {
    expect(done.waits).toHaveLength(1);
    expect(done.waits[0]).toMatchObject({ thread: 2, subRow: 0, start: at(61), end: at(3148) });
  });

  it("leaves from the sub-bar, crosses lanes, and returns to the sub-bar", () => {
    const call = done.connectors.find((connector) => connector.kind === "call");
    expect(call).toMatchObject({ from: { site: "local", subRow: 0, t: at(61) }, to: { site: "cloud", run: SPAWN_CHILD, t: at(120) } });
    const back = done.connectors.find((connector) => connector.kind === "return");
    expect(back).toMatchObject({ from: { site: "cloud", t: at(3134) }, to: { site: "local", subRow: 0, t: at(3148) }, pending: false });
  });

  it("draws the migration as a gap on the origin and a connector between lanes", () => {
    const parent = done.segments.filter((bar) => bar.run === SPAWN_PARENT);
    expect(parent.map((bar) => [bar.site, bar.segment, bar.pid, bar.endKind])).toEqual([
      ["local", 1, 41502, "paused"],
      ["cloud", 2, 52140, "completed"],
    ]);
    expect(done.gaps).toHaveLength(1);
    expect(done.gaps[0]).toMatchObject({ site: "local", start: at(3164), end: at(5202), open: false, bytes: 3204 });
    const migration = done.connectors.find((connector) => connector.kind === "migration");
    expect(migration).toMatchObject({ from: { site: "local", t: at(5202) }, to: { site: "cloud", t: at(5273) } });
  });

  it("marks the pause request, both blocked attempts, the snapshot, and the resume", () => {
    expect(done.markers.map((marker) => [marker.kind, marker.site, marker.t])).toEqual([
      ["pause_request", "local", at(1900)],
      ["blocked", "local", at(1906)],
      ["blocked", "local", at(3076)],
      ["snapshot", "local", at(3162)],
      ["resume", "cloud", at(5277)],
    ]);
    const request = done.markers[0];
    expect(request?.kind === "pause_request" ? request.waitingOn : []).toHaveLength(1);
    // The imported record lists the origin's snapshot. It must not produce a second marker.
    expect(done.markers.filter((marker) => marker.kind === "snapshot")).toHaveLength(1);
  });

  it("shares a cloud row between the child and the continuation, which do not overlap", () => {
    expect(done.lanes.find((lane) => lane.site === "cloud")?.rows).toHaveLength(1);
  });
});

describe("process loss", () => {
  const fixture = buildCentralFixture();
  const key = runKey("local", CENTRAL_PARENT);

  it("ends the bar of a killed plain run at the status change and marks it lost", () => {
    const now = at(3000);
    let state = playFixture(fixture, now);
    const run = state.runs[key] as Run;
    const lost: Run = { ...run, status: "lost", pid: null, updated_ts: at(3100) };
    state = { ...state, runs: { ...state.runs, [key]: lost }, events: { ...state.events, [key]: [...(state.events[key] ?? []), { type: "ui_status", ts: at(3100), site: "local", run: run.id, segment: 1, pid: null, status: "lost" }] } };
    const layout = layoutOf(state, key, at(9000));
    expect(layout.segments[0]).toMatchObject({ endKind: "lost", end: at(3100) });
    expect(layout.gaps).toHaveLength(0);
    expect(layout.live).toBe(false);
  });

  it("falls back to updated_ts when the status change was not observed", () => {
    const state = playFixture(fixture, at(3000));
    const run = state.runs[key] as Run;
    const lost: Run = { ...run, status: "lost", pid: null, updated_ts: at(3100) };
    const layout = computeTimelineLayout({ runs: [lost], events: state.events, now: at(9000) });
    expect(layout.segments[0]).toMatchObject({ endKind: "lost", end: at(3100) });
  });
});

describe("fork fixture: worker_exit, automatic snapshots, fork, cancelled", () => {
  const fixture = buildForkFixture();
  const root = runKey("local", FORK_SOURCE);
  const state = playFixture(fixture);
  const done = layoutOf(state, root, at(60_000));

  /** The state after a page reload: `events.jsonl` has no `run` messages, so no status change was observed. */
  const reloaded = (from: AppState): AppState => ({
    ...from,
    events: Object.fromEntries(
      Object.entries(from.events).map(([key, list]) => [key, (list ?? []).filter((event) => event.type !== "ui_status")]),
    ) as AppState["events"],
  });

  it("ends the killed segment at worker_exit and marks it killed", () => {
    const bars = done.segments.filter((bar) => bar.run === FORK_SOURCE);
    expect(bars.map((bar) => [bar.segment, bar.pid, bar.start, bar.end, bar.endKind])).toEqual([
      [1, 41810, at(38), at(3700), "killed"],
      [2, 41902, at(6448), at(11_053), "completed"],
    ]);
  });

  it("keeps the kill time after a page reload", () => {
    const layout = layoutOf(reloaded(state), root, at(60_000));
    const first = layout.segments.find((bar) => bar.run === FORK_SOURCE && bar.segment === 1);
    // The last worker event of the segment is at 3060. Only `worker_exit` has the real end.
    expect(first).toMatchObject({ end: at(3700), endKind: "killed" });
    expect(layout.gaps.find((gap) => gap.run === FORK_SOURCE)).toMatchObject({ start: at(3700), end: at(6448), snapshotN: 2, bytes: 2790 });
  });

  it("keeps the kill time while the run is paused after a reload", () => {
    const now = at(4500);
    const layout = layoutOf(reloaded(playFixture(fixture, now)), root, now);
    expect(layout.segments.find((bar) => bar.run === FORK_SOURCE)).toMatchObject({ end: at(3700), endKind: "killed" });
    expect(layout.gaps[0]).toMatchObject({ open: true, start: at(3700), snapshotN: 2 });
  });

  it("maps a worker_exit with status lost to a lost segment", () => {
    const key = runKey("local", FORK_SOURCE);
    const partial = reloaded(playFixture(fixture, at(3700)));
    const events = (partial.events[key] ?? []).map((event) => (event.type === "worker_exit" ? { ...event, status: "lost" as const } : event));
    const record: Run = { ...(partial.runs[key] as Run), status: "lost", snapshots: [], updated_ts: at(9999) };
    const layout = computeTimelineLayout({ runs: [record], events: { [key]: events }, now: at(20_000) });
    expect(layout.segments[0]).toMatchObject({ end: at(3700), endKind: "lost" });
    expect(layout.gaps).toHaveLength(0);
  });

  it("draws automatic snapshots from snapshot events as hollow markers", () => {
    const snapshots = done.markers.filter((marker) => marker.kind === "snapshot" && marker.run === FORK_SOURCE);
    expect(snapshots.map((marker) => (marker.kind === "snapshot" ? [marker.n, marker.t, marker.segment, marker.automatic, marker.bytes] : []))).toEqual([
      [1, at(1548), 1, true, 2610],
      [2, at(3056), 1, true, 2790],
      [3, at(7960), 2, true, 2960],
    ]);
    // An automatic snapshot does not end its segment and requests no pause.
    expect(done.markers.filter((marker) => marker.kind === "pause_request")).toHaveLength(0);
  });

  it("draws automatic snapshots from snapshots[].automatic and .segment when the events are missing", () => {
    const key = runKey("local", FORK_SOURCE);
    const without: AppState = {
      ...state,
      events: { ...state.events, [key]: (state.events[key] ?? []).filter((event) => event.type !== "snapshot") },
    };
    const layout = layoutOf(without, root, at(60_000));
    const snapshots = layout.markers.filter((marker) => marker.kind === "snapshot" && marker.run === FORK_SOURCE);
    expect(snapshots.map((marker) => (marker.kind === "snapshot" ? [marker.n, marker.segment, marker.automatic] : []))).toEqual([
      [1, 1, true],
      [2, 1, true],
      [3, 2, true],
    ]);
    // The fork lists the snapshot of its source. It gets no marker on the fork's row.
    expect(layout.markers.filter((marker) => marker.kind === "snapshot" && marker.run === FORK_RUN)).toHaveLength(0);
    const record = state.runs[key] as Run;
    const requested: Run = { ...record, snapshots: record.snapshots.map((snapshot) => ({ ...snapshot, automatic: false })) };
    const runs = [requested, ...Object.values(state.runs).filter((run) => run.id !== FORK_SOURCE)];
    const solid = computeTimelineLayout({ runs, events: without.events, now: at(60_000) });
    expect(solid.markers.filter((marker) => marker.kind === "snapshot" && marker.automatic)).toHaveLength(0);
  });

  it("shows the fork as a marker, a gap without a process, and a connector from the snapshot", () => {
    expect(runTree(state, root)).toContain(runKey("local", FORK_RUN));
    const marker = done.markers.find((candidate) => candidate.kind === "fork");
    expect(marker).toMatchObject({ site: "local", run: FORK_RUN, t: at(5200), fromRun: FORK_SOURCE, n: 1 });
    const gap = done.gaps.find((candidate) => candidate.run === FORK_RUN);
    expect(gap).toMatchObject({ site: "local", start: at(5200), end: at(8648), open: false, fork: true, snapshotN: 1, bytes: 2610 });
    const connector = done.connectors.find((candidate) => candidate.kind === "fork");
    expect(connector).toMatchObject({
      from: { site: "local", run: FORK_SOURCE, t: at(1548) },
      to: { site: "local", run: FORK_RUN, t: at(5200) },
    });
    // The source and the fork overlap in time, so they take two rows of the local lane.
    expect(done.lanes.find((lane) => lane.site === "local")?.rows).toHaveLength(2);
    expect(connector?.from.row).not.toBe(connector?.to.row);
    expect(marker?.row).toBe(connector?.to.row);
  });

  it("draws the fork gap open while the fork waits for its first resume", () => {
    const now = at(7000);
    const layout = layoutOf(playFixture(fixture, now), root, now);
    expect(layout.gaps.find((gap) => gap.run === FORK_RUN)).toMatchObject({ open: true, fork: true, start: at(5200), end: now });
    expect(layout.segments.filter((bar) => bar.run === FORK_RUN)).toHaveLength(0);
  });

  it("derives the fork from forked_from when the forked event is missing", () => {
    const key = runKey("local", FORK_RUN);
    const without: AppState = { ...state, events: { ...state.events, [key]: (state.events[key] ?? []).filter((event) => event.type !== "forked") } };
    const layout = layoutOf(without, root, at(60_000));
    expect(layout.markers.find((marker) => marker.kind === "fork")).toMatchObject({ run: FORK_RUN, t: at(5200), n: 1 });
  });

  it("ends the cancelled segment at worker_exit, with or without the cancelled event", () => {
    expect(done.segments.find((bar) => bar.run === FORK_RUN)).toMatchObject({ segment: 2, pid: 41955, start: at(8648), end: at(9506), endKind: "cancelled" });
    const key = runKey("local", FORK_RUN);
    const base = reloaded(state);
    const without: AppState = { ...base, events: { ...base.events, [key]: (base.events[key] ?? []).filter((event) => event.type !== "cancelled") } };
    expect(layoutOf(without, root, at(60_000)).segments.find((bar) => bar.run === FORK_RUN)).toMatchObject({ end: at(9506), endKind: "cancelled" });
    // A worker that emits `cancelled` on a server that sends no worker_exit.
    const eventOnly: AppState = { ...base, events: { ...base.events, [key]: (base.events[key] ?? []).filter((event) => event.type !== "worker_exit") } };
    expect(layoutOf(eventOnly, root, at(60_000)).segments.find((bar) => bar.run === FORK_RUN)).toMatchObject({ end: at(9504), endKind: "cancelled" });
  });

  it("still connects the remote call of the resumed source", () => {
    expect(done.connectors.find((connector) => connector.kind === "call")).toMatchObject({ from: { run: FORK_SOURCE }, to: { site: "cloud", run: FORK_CHILD } });
    expect(done.connectors.find((connector) => connector.kind === "return")).toMatchObject({ to: { run: FORK_SOURCE, t: at(11_043) }, pending: false });
  });
});

describe("migration while a remote call is pending", () => {
  it("continues the wait on the destination site and returns the result there", () => {
    const parent = "r-mig";
    const child = "r-kid";
    const callId = `${parent}-c1`;
    const worker = (site: "local" | "cloud", run: string, segment: number, pid: number, ts: number, body: Record<string, unknown>) =>
      parseSseEvent({ v: 1, ts, run, segment, pid, site, ...body }, site);
    const local = [
      worker("local", parent, 1, 11, 100, { type: "hello", mode: "start", function: "durable_f", durable: true }),
      worker("local", parent, 1, 11, 200, { type: "remote_call", call_id: callId, thread: 1, function: "remote_g", args: {} }),
      parseSseEvent({ type: "remote_dispatched", ts: 210, site: "local", run: parent, call_id: callId, child_site: "cloud", child_run: child, function: "remote_g" }, "local"),
      worker("local", parent, 1, 11, 400, { type: "paused", snapshot_path: "snap-1.bamlsnap", state_path: "snap-1.json", stats: { compressed_bytes: 512 } }),
      parseSseEvent({ type: "migrated_out", ts: 600, site: "local", run: parent, to_site: "cloud" }, "local"),
    ];
    const cloudParent = [
      worker("cloud", parent, 2, 22, 700, { type: "hello", mode: "resume", function: "durable_f", durable: true }),
      worker("cloud", parent, 2, 22, 1500, { type: "remote_result_received", call_id: callId, thread: 1 }),
      worker("cloud", parent, 2, 22, 1600, { type: "completed", value: 1 }),
    ];
    const cloudChild = [
      worker("cloud", child, 1, 33, 260, { type: "hello", mode: "start", function: "remote_g", durable: false }),
      worker("cloud", child, 1, 33, 1400, { type: "completed", value: 1 }),
    ];
    const events = {
      [runKey("local", parent)]: local,
      [runKey("cloud", parent)]: cloudParent,
      [runKey("cloud", child)]: cloudChild,
    } as AppState["events"];
    const record = (site: "local" | "cloud", id: string, status: Run["status"]): Run =>
      normalizeRun({ id, site, status } as Run);
    const layout = computeTimelineLayout({
      runs: [record("local", parent, "migrated"), record("cloud", parent, "completed"), record("cloud", child, "completed")],
      events,
      now: 5000,
    });
    expect(layout.waits.map((wait) => [wait.site, wait.segment, wait.start, wait.end])).toEqual([
      ["local", 1, 200, 400],
      ["cloud", 2, 700, 1500],
    ]);
    const back = layout.connectors.find((connector) => connector.kind === "return");
    expect(back).toMatchObject({ from: { site: "cloud", run: child, t: 1400 }, to: { site: "cloud", run: parent, t: 1500 }, pending: false });
    expect(layout.connectors.find((connector) => connector.kind === "migration")).toMatchObject({
      from: { site: "local", t: 600 },
      to: { site: "cloud", t: 700 },
    });
    // The child and the continuation overlap in time on cloud, so they take two rows.
    expect(layout.lanes.find((lane) => lane.site === "cloud")?.rows).toHaveLength(2);
  });
});

describe("pool fixture: three sites", () => {
  const fixture = buildPoolFixture();
  const root = runKey("local", POOL_PARENT);
  const done = layoutOf(playFixture(fixture), root, at(60_000));

  it("draws one lane per site in registry order", () => {
    expect(done.lanes.map((lane) => lane.site)).toEqual(["local", "cloud", "cloud2"]);
    const reordered = computeTimelineLayout({ sites: ["cloud2", "local"], runs: [], events: {}, now: 0 });
    expect(reordered.lanes.map((lane) => lane.site)).toEqual(["cloud2", "local"]);
  });

  it("adds a lane for a site outside the registry that holds a run of the tree", () => {
    const state = playFixture(fixture);
    const runs = runTree(state, root).flatMap((key) => (state.runs[key] ? [state.runs[key] as Run] : []));
    const layout = computeTimelineLayout({ sites: ["local", "cloud"], runs, events: state.events, now: at(60_000) });
    expect(layout.lanes.map((lane) => lane.site)).toEqual(["local", "cloud", "cloud2"]);
    expect(layout.segments.filter((bar) => bar.site === "cloud2")).toHaveLength(1);
  });

  it("draws the parent on local and cloud and the child on cloud2", () => {
    expect(done.segments.map((bar) => [bar.site, bar.run, bar.segment, bar.endKind, bar.mode])).toEqual([
      ["local", POOL_PARENT, 1, "paused", "start"],
      ["cloud", POOL_PARENT, 2, "completed", "resume"],
      ["cloud2", POOL_CHILD, 1, "completed", "start"],
    ]);
    expect(done.gaps).toHaveLength(1);
    expect(done.gaps[0]).toMatchObject({ site: "local", run: POOL_PARENT, start: at(2330), end: at(4202), open: false, snapshotN: 1 });
  });

  it("connects the migration, the call into cloud2, and the return to cloud", () => {
    expect(done.connectors.map((connector) => [connector.kind, connector.from.site, connector.to.site])).toEqual([
      ["migration", "local", "cloud"],
      ["call", "cloud", "cloud2"],
      ["return", "cloud2", "cloud"],
    ]);
    const back = done.connectors.find((connector) => connector.kind === "return");
    expect(back).toMatchObject({ ok: true, pending: false, from: { run: POOL_CHILD }, to: { run: POOL_PARENT } });
    const wait = done.waits.find((candidate) => candidate.run === POOL_PARENT);
    expect(wait).toMatchObject({ site: "cloud", segment: 2, end: back?.to.t });
  });

  it("keeps the gap open on local and draws no bar elsewhere while the run is paused", () => {
    const paused = layoutOf(playFixture(fixture, at(3000)), root, at(3000));
    expect(paused.segments.map((bar) => bar.site)).toEqual(["local"]);
    expect(paused.gaps[0]).toMatchObject({ site: "local", open: true });
    expect(paused.lanes.map((lane) => lane.site)).toEqual(["local", "cloud", "cloud2"]);
  });
});

describe("chain fixture: local, cloud, cloud2, and back to local", () => {
  const fixture = buildChainFixture();
  const root = runKey("local", CHAIN_PARENT);
  const state = playFixture(fixture);
  const done = layoutOf(state, root, at(60_000));

  it("draws the four segments of the run on three lanes, in order", () => {
    const parent = done.segments.filter((bar) => bar.run === CHAIN_PARENT).sort((x, y) => x.segment - y.segment);
    expect(parent.map((bar) => [bar.site, bar.segment, bar.endKind])).toEqual([
      ["local", 1, "paused"],
      ["cloud", 2, "paused"],
      ["cloud2", 3, "paused"],
      ["local", 4, "completed"],
    ]);
    // Segments 1 and 4 share the row of the run on the local lane.
    expect(new Set(parent.filter((bar) => bar.site === "local").map((bar) => bar.row)).size).toBe(1);
  });

  it("draws two migration connectors for local to cloud to cloud2, and a third one back to local", () => {
    const migrations = done.connectors.filter((connector) => connector.kind === "migration");
    expect(migrations.map((connector) => [connector.from.site, connector.to.site])).toEqual([
      ["local", "cloud"],
      ["cloud", "cloud2"],
      ["cloud2", "local"],
    ]);
    // Each connector leaves at the `migrated_out` of its own site.
    expect(migrations.map((connector) => connector.from.t)).toEqual([at(2002), at(4602), at(8602)]);
    expect(migrations.map((connector) => connector.to.t)).toEqual([at(2073), at(4673), at(8673)]);
    const before = layoutOf(playFixture(fixture, at(5000)), root, at(5000));
    expect(before.connectors.filter((connector) => connector.kind === "migration")).toHaveLength(2);
  });

  it("closes one gap per pause on the site that held the snapshot", () => {
    expect(done.gaps.map((gap) => [gap.site, gap.snapshotN, gap.open, gap.end])).toEqual([
      ["local", 1, false, at(2002)],
      ["cloud", 2, false, at(4602)],
      ["cloud2", 3, false, at(8602)],
    ]);
  });

  it("uses the first migrated_out after a segment when a run leaves a site twice", () => {
    // Leave `local` a second time: the gap of segment 1 must still end at the first departure.
    const again = reducer2(state, { type: "migrated_out", ts: at(20_000), site: "local", run: CHAIN_PARENT, to_site: "cloud" });
    const layout = layoutOf(again, root, at(60_000));
    expect(layout.gaps[0]).toMatchObject({ site: "local", end: at(2002) });
  });

  it("calls from cloud2 into cloud and returns the result to local, where the run continued", () => {
    const call = done.connectors.find((connector) => connector.kind === "call");
    expect(call).toMatchObject({ from: { site: "cloud2", run: CHAIN_PARENT }, to: { site: "cloud", run: CHAIN_CHILD } });
    const back = done.connectors.find((connector) => connector.kind === "return");
    expect(back).toMatchObject({ from: { site: "cloud", run: CHAIN_CHILD }, to: { site: "local", run: CHAIN_PARENT }, pending: false });
    // The wait continues in the segment on `local` until the result is received there.
    const waits = done.waits
      .filter((wait) => wait.run === CHAIN_PARENT)
      .sort((x, y) => x.segment - y.segment)
      .map((wait) => [wait.site, wait.segment]);
    expect(waits).toEqual([["cloud2", 3], ["local", 4]]);
  });

  it("places the snapshot and resume markers on the lanes of their segments", () => {
    expect(done.markers.filter((marker) => marker.kind === "snapshot").map((marker) => [marker.site, marker.kind === "snapshot" ? marker.n : 0])).toEqual([
      ["local", 1],
      ["cloud", 2],
      ["cloud2", 3],
    ]);
    expect(done.markers.filter((marker) => marker.kind === "resume").map((marker) => marker.site)).toEqual(["cloud", "cloud2", "local"]);
  });
});

describe("snapshotNumberFromPath", () => {
  it("reads padded and plain numbers", () => {
    expect(snapshotNumberFromPath(".baml/runs/r-1/snap-0002.bamlsnap")).toBe(2);
    expect(snapshotNumberFromPath("snap-11.bamlsnap")).toBe(11);
    expect(snapshotNumberFromPath("other.bin")).toBeNull();
  });
});
