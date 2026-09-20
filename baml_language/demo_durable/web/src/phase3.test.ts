/**
 * Reducer, protocol, and selector tests for contract section 9: the
 * `sleeping` status, the new worker and site events, the grouped runs list,
 * and the log lines.
 */

import { describe, expect, it } from "vitest";
import { systemLine } from "./components/LogLanes";
import { FIXTURE_NAMES, loadFixture } from "./fixtures";
import { FIXTURE_EPOCH, FIXTURE_PROGRAM_HASH } from "./fixtures/builder";
import { buildFanoutFixture, FANOUT_CHILDREN, FANOUT_PARENT } from "./fixtures/fanout";
import { buildRaceFixture, RACE_CHILDREN, RACE_PARENT } from "./fixtures/race";
import { FIXTURE_SOURCES } from "./fixtures/programs";
import { isTickingStatus, normalizeRun, parseSseEvent, shortHash, type Run, type SseEvent } from "./protocol";
import { groupRuns, initialState, reducer, runKey, runTree, validActions, type AppState } from "./state";
import { playFixture } from "./testing";

const at = (ms: number): number => FIXTURE_EPOCH + ms;

describe("protocol (section 9)", () => {
  it("parses the new worker and site events and keeps their fields", () => {
    const cancel = parseSseEvent({ v: 1, type: "remote_cancel", ts: 5, run: "r-1", segment: 2, pid: 9, call_id: "r-1-c2", thread: 3 }, "local");
    expect(cancel).toMatchObject({ type: "remote_cancel", site: "local", call_id: "r-1-c2", thread: 3 });
    expect(parseSseEvent({ type: "sleep_scheduled", ts: 6, site: "cloud", run: "r-1", wake_at: 99 }, "local")).toMatchObject({ site: "cloud", wake_at: 99 });
    expect(parseSseEvent({ type: "woken", ts: 7, run: "r-1", reason: "restart" }, "cloud2")).toMatchObject({ site: "cloud2", reason: "restart" });
    expect(parseSseEvent({ type: "remote_cancelled", ts: 8, run: "r-1", call_id: "c", child_site: "cloud", child_run: "r-2" }, "local")).toMatchObject({ child_run: "r-2" });
  });

  it("still drops an event type that it does not know, and keeps unknown fields of a known one", () => {
    expect(parseSseEvent({ type: "woke_early", ts: 1, run: "r-1" }, "local")).toBeNull();
    const paused = parseSseEvent({ v: 1, type: "paused", ts: 1, run: "r-1", segment: 1, pid: 2, wake: { reason: "sleep", remaining_ms: 10, at_ts: 11 }, later_field: true }, "local");
    expect(paused).toMatchObject({ wake: { remaining_ms: 10 }, later_field: true });
  });

  it("fills wake_at and program_hash in for a server that omits them", () => {
    const run = normalizeRun({ id: "r-1", site: "local", status: "sleeping" } as Run);
    expect(run.wake_at).toBeNull();
    expect(run.program_hash).toBeNull();
    expect(normalizeRun({ id: "r-1", site: "local", status: "sleeping", wake_at: 42, program_hash: "abcdef0123456789" } as Run)).toMatchObject({ wake_at: 42, program_hash: "abcdef0123456789" });
    expect(shortHash("abcdef0123456789", 8)).toBe("abcdef01");
    expect(shortHash(null)).toBeNull();
    expect(shortHash("")).toBeNull();
  });

  it("counts a sleeping run as ticking but not as live", () => {
    expect(isTickingStatus("sleeping")).toBe(true);
    expect(isTickingStatus("paused")).toBe(false);
    expect(isTickingStatus("running")).toBe(true);
  });
});

describe("reducer: a run that suspends itself (fanout fixture)", () => {
  const fixture = buildFanoutFixture();
  const key = runKey("local", FANOUT_PARENT);

  it("shows the run as sleeping with its wake time while no process exists", () => {
    const state = playFixture(fixture, at(5000));
    const run = state.runs[key] as Run;
    expect(run.status).toBe("sleeping");
    expect(run.pid).toBeNull();
    expect(run.wake_at).toBeGreaterThan(at(12_000));
    expect(run.program_hash).toBe(FIXTURE_PROGRAM_HASH);
    expect(run.snapshots).toHaveLength(1);
    // Three of the four children are done at 5 s: the car (2 s), the flight (3 s), and the tour (4 s).
    // Their results are stored in the order of arrival, not delivered.
    expect(run.remote_results.map((result) => [result.call_id, result.acked])).toEqual([
      [`${FANOUT_PARENT}-c3`, false],
      [`${FANOUT_PARENT}-c1`, false],
      [`${FANOUT_PARENT}-c4`, false],
    ]);
    expect(validActions(run)).toMatchObject({ pause: false, resume_here: true, kill: false, cancel: true });
  });

  it("keeps sleep_scheduled, woken, and the paused event with its wake in the run's history", () => {
    const events = playFixture(fixture).events[key] ?? [];
    const types = events.map((event) => event.type);
    expect(types.filter((type) => type === "sleep_scheduled" || type === "woken")).toEqual(["sleep_scheduled", "woken"]);
    const paused = events.find((event) => event.type === "paused");
    expect(paused && paused.type === "paused" ? paused.wake : null).toMatchObject({ reason: "sleep", remaining_ms: 11_984 });
    // The status entries that the reducer records follow the record: running, sleeping, starting, running, completed.
    expect(events.flatMap((event) => (event.type === "ui_status" ? [event.status] : []))).toEqual(["starting", "running", "sleeping", "starting", "running", "completed"]);
  });

  it("puts the four children into the tree of the parent", () => {
    const state = playFixture(fixture);
    const tree = runTree(state, key);
    expect(tree).toHaveLength(5);
    for (const [index, child] of FANOUT_CHILDREN.slice(0, 4).entries()) {
      expect(tree).toContain(runKey(index % 2 === 0 ? "cloud" : "cloud2", child));
    }
  });
});

describe("groupRuns", () => {
  it("lists remote children under their parent, in call order, below newer top-level runs", () => {
    const state = playFixture(buildFanoutFixture());
    const rows = groupRuns(Object.values(state.runs));
    expect(rows.map((row) => [row.run.id, row.depth, row.children])).toEqual([
      [FANOUT_PARENT, 0, 4],
      [FANOUT_CHILDREN[0], 1, 0],
      [FANOUT_CHILDREN[1], 1, 0],
      [FANOUT_CHILDREN[2], 1, 0],
      [FANOUT_CHILDREN[3], 1, 0],
    ]);
  });

  it("attaches children to the record on the site that made the call when the parent migrated", () => {
    const state = playFixture(buildRaceFixture());
    const rows = groupRuns(Object.values(state.runs));
    // The record on cloud2 is newer, so it leads. The children were called from local.
    expect(rows.map((row) => `${row.run.site}/${row.run.id}:${row.depth}`)).toEqual([
      `cloud2/${RACE_PARENT}:0`,
      `local/${RACE_PARENT}:0`,
      `cloud/${RACE_CHILDREN[0]}:1`,
      `cloud2/${RACE_CHILDREN[1]}:1`,
      `cloud/${RACE_CHILDREN[2]}:1`,
    ]);
  });

  it("keeps a child whose parent is not listed as a top-level row, and nests grandchildren", () => {
    const run = (id: string, created: number, parent: string | null): Run =>
      normalizeRun({ id, site: "cloud", status: "running", created_ts: created, parent: parent === null ? null : { site: "cloud", run: parent, call_id: `${parent}-c1` } } as Run);
    const rows = groupRuns([run("orphan", 1, "gone"), run("a", 2, null), run("a-child", 3, "a"), run("a-grandchild", 4, "a-child")]);
    expect(rows.map((row) => [row.run.id, row.depth])).toEqual([["a", 0], ["a-child", 1], ["a-grandchild", 2], ["orphan", 0]]);
  });

  it("lists every record once, also when two records name each other as parent", () => {
    const loop = (id: string, parent: string): Run => normalizeRun({ id, site: "local", status: "running", parent: { site: "local", run: parent, call_id: "c" } } as Run);
    expect(groupRuns([loop("a", "b"), loop("b", "a")]).map((row) => row.run.id).sort()).toEqual(["a", "b"]);
  });
});

describe("log lines (section 9)", () => {
  const events = (state: AppState, key: string) => state.events[key as keyof AppState["events"]] ?? [];
  it("states the self-suspension, the wake timer, the wake, and the program source", () => {
    const state = playFixture(buildFanoutFixture());
    const lines = events(state, runKey("local", FANOUT_PARENT)).flatMap((event) => systemLine(event) ?? []);
    expect(lines.some((line) => line.startsWith("suspended itself for a sleep with 11.98 s to go"))).toBe(true);
    expect(lines.some((line) => line.startsWith("sleeping without a process. The wake timer is set for"))).toBe(true);
    expect(lines).toContain("woken by the wake timer");
    expect(lines.some((line) => line.includes("The program came from the program store, without a compile"))).toBe(true);
  });

  it("states both halves of a remote cancellation", () => {
    const state = playFixture(buildRaceFixture());
    const lines = events(state, runKey("cloud2", RACE_PARENT)).flatMap((event) => systemLine(event) ?? []);
    expect(lines).toContain(`thread 3 was cancelled while it waited on ${RACE_PARENT}-c2. The run no longer waits on the call`);
    expect(lines).toContain(`cancelled ${RACE_PARENT}-c2: asked cloud2 to cancel ${RACE_CHILDREN[1]}`);
  });
});

describe("fixtures", () => {
  it("every fixture plays into the reducer in time order and refers to program lines that exist", () => {
    for (const name of FIXTURE_NAMES) {
      const fixture = loadFixture(name);
      let previous = 0;
      for (const event of fixture.events) {
        expect(event.ts).toBeGreaterThanOrEqual(previous);
        previous = event.ts;
      }
      const state = fixture.events.reduce((current, event: SseEvent) => reducer(current, { type: "sse", site: event.site, event }), initialState());
      for (const list of Object.values(state.events)) {
        for (const event of list) {
          if (event.type !== "position") continue;
          const lines = FIXTURE_SOURCES[event.file]?.split("\n") ?? [];
          expect(lines[event.line - 1], `${name}: ${event.file}:${event.line}`).toBeDefined();
        }
      }
      // Every run of a fixture ends in a final status or waits for a command.
      for (const run of Object.values(state.runs)) {
        expect(["completed", "failed", "cancelled", "lost", "migrated", "paused"], `${name}: ${run.id}`).toContain(run.status);
      }
    }
  });

  it("every scenario fixture names the run that its guide follows", () => {
    for (const name of ["pool", "fanout", "race", "deadline", "settled", "recover", "branch"] as const) {
      const fixture = loadFixture(name);
      const root = fixture.roles?.root;
      expect(root, name).toBeDefined();
      const state = playFixture(fixture);
      expect(state.runs[runKey(root?.site ?? "", root?.id ?? "")], name).toBeDefined();
    }
  });
});
