/**
 * Contract section 10.3: every timeline object names what made it happen, and
 * where in the program it happened.
 *
 * The rules under test are the three sources of a location (the worker's own
 * field, the run record, and the last `position` of a thread), the sentence
 * that each `cause` value of `remote_cancel` produces, and the promise that
 * the panel never shows a location it cannot attribute.
 */

import { describe, expect, it } from "vitest";
import { FIXTURE_EPOCH } from "../fixtures/builder";
import { buildCentralFixture, CENTRAL_CHILD, CENTRAL_PARENT } from "../fixtures/central";
import { buildDeadlineFixture, DEADLINE_PARENT } from "../fixtures/deadline";
import { buildFanoutFixture, FANOUT_PARENT } from "../fixtures/fanout";
import { buildForkFixture, FORK_RUN, FORK_SOURCE } from "../fixtures/fork";
import { buildRaceFixture, RACE_PARENT } from "../fixtures/race";
import { buildSpawnFixture, SPAWN_PARENT } from "../fixtures/spawn";
import { parseSseEvent, type CancelCause, type Run, type StateDump } from "../protocol";
import { initialState, reducer, runKey, runTree, splitRunKey, type AppState, type RunKey, type TimelineEvent } from "../state";
import { playFixture } from "../testing";
import { causeOf, CANCEL_CAUSE_TEXT, locationLabel, type Cause, type CauseContext, type DetailTarget } from "./cause";
import { computeTimelineLayout, type Connector, type Marker, type TimelineLayout } from "./layout";

const at = (ms: number): number => FIXTURE_EPOCH + ms;
const QUOTES = "baml_src/quotes.baml";
const TRIP = "baml_src/trip.baml";

function treeOf(state: AppState, root: RunKey): Run[] {
  return runTree(state, root).flatMap((key) => (state.runs[key] ? [state.runs[key] as Run] : []));
}

function layoutOf(state: AppState, root: RunKey, now: number): TimelineLayout {
  return computeTimelineLayout({ runs: treeOf(state, root), events: state.events, now });
}

function contextOf(state: AppState, root: RunKey, snapshot?: CauseContext["snapshot"]): CauseContext {
  return { runs: treeOf(state, root), events: state.events, snapshot: snapshot ?? null };
}

const connectorOf = (layout: TimelineLayout, kind: Connector["kind"]): Connector => {
  const found = layout.connectors.find((connector) => connector.kind === kind);
  if (!found) throw new Error(`no ${kind} connector in the layout`);
  return found;
};

const markerOf = <K extends Marker["kind"]>(layout: TimelineLayout, kind: K): Extract<Marker, { kind: K }> => {
  const found = layout.markers.find((marker): marker is Extract<Marker, { kind: K }> => marker.kind === kind);
  if (!found) throw new Error(`no ${kind} marker in the layout`);
  return found;
};

const causeFor = (target: DetailTarget, ctx: CauseContext, now = at(20_000)): Cause => causeOf(target, ctx, now);

// ---------------------------------------------------------------------------
// The three sources of a location
// ---------------------------------------------------------------------------

describe("the location of a call arrow", () => {
  const fixture = buildRaceFixture();
  const root = runKey("local", RACE_PARENT);
  const now = at(20_000);

  it("takes the exact call site from the remote_call event", () => {
    const state = playFixture(fixture, now);
    const layout = layoutOf(state, root, now);
    const cause = causeFor({ kind: "connector", item: connectorOf(layout, "call") }, contextOf(state, root), now);
    expect(cause.title).toBe("remote call");
    expect(cause.location).toMatchObject({ file: QUOTES, approximate: false, site: "local" });
    expect(cause.location?.line).toBeGreaterThan(1);
    // "durable_race called remote_get_quote at quotes.baml:190, placed on cloud"
    expect(cause.sentence).toBe(
      `durable_race called remote_get_quote at ${locationLabel(cause.location as { file: string; line: number })}, placed on cloud.`,
    );
    expect(cause.missing).toBeNull();
  });

  it("uses the call site that the run record keeps when the events are not loaded", () => {
    // A page reload before the backfill: only the run records are known.
    const state = playFixture(fixture, now);
    const layout = layoutOf(state, root, now);
    const connector = connectorOf(layout, "call");
    const ctx: CauseContext = { runs: treeOf(state, root), events: {}, snapshot: null };
    const cause = causeFor({ kind: "connector", item: connector }, ctx, now);
    expect(cause.location).toMatchObject({ file: QUOTES, approximate: false });
    expect(cause.location?.line).toBe(
      treeOf(state, root).find((run) => run.id === RACE_PARENT)?.calls?.find((call) => call.call_id === connector.callId)?.line,
    );
  });

  it("falls back to the last position of the calling thread and marks it approximate", () => {
    const state = playFixture(fixture, now);
    const layout = layoutOf(state, root, now);
    const connector = connectorOf(layout, "call");
    // A worker that reports no call site, and a run record that keeps none.
    const stripped = stripCallSites(state);
    const cause = causeFor({ kind: "connector", item: connector }, contextOf(stripped, root), now);
    expect(cause.location).toMatchObject({ file: QUOTES, approximate: true });
    expect(cause.location?.what).toContain("last line thread");
    expect(cause.missing).toBeNull();
  });

  it("shows no location, and says so, when nothing can be attributed", () => {
    const state = playFixture(fixture, now);
    const layout = layoutOf(state, root, now);
    const connector = connectorOf(layout, "call");
    const blind = stripPositions(stripCallSites(state));
    const cause = causeFor({ kind: "connector", item: connector }, contextOf(blind, root), now);
    expect(cause.location).toBeNull();
    expect(cause.missing).toContain("no call site");
    expect(cause.sentence).not.toContain(" at ");
  });
});

describe("the calling function of a call arrow", () => {
  const fixture = buildRaceFixture();
  const root = runKey("local", RACE_PARENT);
  const now = at(20_000);

  it("names the function that the worker reported the call from", () => {
    const state = playFixture(fixture, now);
    const layout = layoutOf(state, root, now);
    const cause = causeFor({ kind: "connector", item: connectorOf(layout, "call") }, contextOf(state, root), now);
    expect(cause.sentence).toMatch(/^durable_race called remote_get_quote at /);
    expect(cause.rows).toContainEqual(["calling function", "durable_race"]);
  });

  it("names the function the run record remembers, not the run's entry function", () => {
    // Section 9.7: a resumed run emits `remote_wait`, not `remote_call`, so no
    // event names the caller. The call may be written in a helper, and then the
    // run's entry function is a different function. Only the record can tell.
    const state = withEntryFunction(playFixture(fixture, now), RACE_PARENT, "durable_book_hotel");
    const layout = layoutOf(state, root, now);
    const ctx: CauseContext = { runs: treeOf(state, root), events: {}, snapshot: null };
    const cause = causeFor({ kind: "connector", item: connectorOf(layout, "call") }, ctx, now);
    expect(cause.sentence).toMatch(/^durable_race called remote_get_quote at /);
    expect(cause.sentence).not.toContain("durable_book_hotel");
  });

  it("names no caller when nothing reported one", () => {
    // A worker and a site server that predate the field. The call site is still
    // exact, so the panel must not pair it with a guessed caller.
    const state = stripCallers(withEntryFunction(playFixture(fixture, now), RACE_PARENT, "durable_book_hotel"));
    const layout = layoutOf(state, root, now);
    const ctx: CauseContext = { runs: treeOf(state, root), events: {}, snapshot: null };
    const cause = causeFor({ kind: "connector", item: connectorOf(layout, "call") }, ctx, now);
    expect(cause.location).toMatchObject({ file: QUOTES, approximate: false });
    expect(cause.sentence).toBe(
      `remote_get_quote was called at ${locationLabel(cause.location as { file: string; line: number })}, placed on cloud.`,
    );
    expect(cause.rows.map(([label]) => label)).not.toContain("calling function");
  });
});

/** Rewrites the entry function of one run, as a run whose call is in a helper has. */
function withEntryFunction(state: AppState, run: string, fn: string): AppState {
  const runs: AppState["runs"] = {};
  for (const [key, record] of Object.entries(state.runs)) {
    runs[key as RunKey] = record.id === run ? { ...record, function: fn } : record;
  }
  return { ...state, runs };
}

/** Removes every `caller` that a worker event and a run record carry (section 10.3). */
function stripCallers(state: AppState): AppState {
  const events: AppState["events"] = {};
  for (const [key, list] of Object.entries(state.events)) {
    events[key as RunKey] = (list ?? []).map((event) =>
      event.type === "remote_call" ? ({ ...event, caller: null } as TimelineEvent) : event,
    );
  }
  const runs: AppState["runs"] = {};
  for (const [key, run] of Object.entries(state.runs)) {
    runs[key as RunKey] = {
      ...run,
      waiting_on: run.waiting_on.map((entry) => ({ ...entry, caller: null })),
      calls: (run.calls ?? []).map((entry) => ({ ...entry, caller: null })),
    };
  }
  return { ...state, events, runs };
}

/** Removes every `file`/`line` that a worker event and a run record carry for a call. */
function stripCallSites(state: AppState): AppState {
  const events: AppState["events"] = {};
  for (const [key, list] of Object.entries(state.events)) {
    events[key as RunKey] = (list ?? []).map((event) =>
      event.type === "remote_call" || event.type === "remote_cancel" || event.type === "thread_started"
        ? ({ ...event, file: null, line: null } as TimelineEvent)
        : event,
    );
  }
  const runs: AppState["runs"] = {};
  for (const [key, run] of Object.entries(state.runs)) {
    runs[key as RunKey] = {
      ...run,
      waiting_on: run.waiting_on.map((entry) => ({ ...entry, file: null, line: null })),
      calls: (run.calls ?? []).map((entry) => ({ ...entry, file: null, line: null })),
    };
  }
  return { ...state, events, runs };
}

/** Removes every `position` event, so no fallback location exists. */
function stripPositions(state: AppState): AppState {
  const events: AppState["events"] = {};
  for (const [key, list] of Object.entries(state.events)) {
    events[key as RunKey] = (list ?? []).filter((event) => event.type !== "position");
  }
  const runs: AppState["runs"] = {};
  for (const [key, run] of Object.entries(state.runs)) runs[key as RunKey] = { ...run, position: null };
  return { ...state, events, runs };
}

// ---------------------------------------------------------------------------
// One sentence per cause value
// ---------------------------------------------------------------------------

describe("the cause of a cancel arrow", () => {
  const race = buildRaceFixture();
  const raceRoot = runKey("local", RACE_PARENT);
  const now = at(20_000);

  it("names a cancelled future for a race loser, with the call site of the cancelled call", () => {
    const state = playFixture(race, now);
    const layout = layoutOf(state, raceRoot, now);
    const cause = causeFor({ kind: "connector", item: connectorOf(layout, "cancel") }, contextOf(state, raceRoot), now);
    expect(cause.title).toBe("remote cancel");
    expect(cause.sentence).toContain(CANCEL_CAUSE_TEXT.future_cancel);
    expect(cause.sentence).toContain("a race loser");
    expect(cause.location).toMatchObject({ file: QUOTES, approximate: false });
    expect(cause.rows).toContainEqual(["cause", "a cancelled future"]);
  });

  it("names a cancel token for the deadline of with_timeout", () => {
    const fixture = buildDeadlineFixture();
    const root = runKey("local", DEADLINE_PARENT);
    const state = playFixture(fixture, now);
    const layout = layoutOf(state, root, now);
    const cause = causeFor({ kind: "connector", item: connectorOf(layout, "cancel") }, contextOf(state, root), now);
    expect(cause.sentence).toContain(CANCEL_CAUSE_TEXT.token);
    expect(cause.rows).toContainEqual(["cause", "a cancel token"]);
    expect(cause.location).toMatchObject({ file: QUOTES, approximate: false });
  });

  it.each([
    ["future_cancel", "a cancelled future"],
    ["token", "a cancel token"],
    ["parent", "a cancelled parent"],
    ["unknown", "not reported"],
  ] as [CancelCause, string][])("maps the cause %s to a readable sentence and a short label", (cause, label) => {
    const state = withCancelCause(playFixture(race, now), cause);
    const layout = layoutOf(state, raceRoot, now);
    const described = causeFor({ kind: "connector", item: connectorOf(layout, "cancel") }, contextOf(state, raceRoot), now);
    expect(described.sentence).toContain(CANCEL_CAUSE_TEXT[cause]);
    expect(described.rows).toContainEqual(["cause", label]);
    // The sentence always ends with the call site, which the run record keeps.
    expect(described.sentence).toMatch(/ The call was made at quotes\.baml:\d+\.$/);
  });

  it("does not turn an unreported cause into a claim about what cancelled the call", () => {
    // `unknown` means the engine could not tell. It does not mean the run was
    // cancelled: the worker also writes it for every call the end-of-run sweep
    // abandons, whatever ended the run.
    const state = withCancelCause(playFixture(race, now), "unknown");
    const layout = layoutOf(state, raceRoot, now);
    const cause = causeFor({ kind: "connector", item: connectorOf(layout, "cancel") }, contextOf(state, raceRoot), now);
    expect(cause.sentence).toContain("could not say");
    expect(cause.sentence).not.toContain("the run cancelled this call");
    expect(cause.rows).toContainEqual(["cause", "not reported"]);
  });

  it("stays honest when the worker reports no cause and no location", () => {
    const state = stripCallSites(stripPositions(withCancelCause(playFixture(race, now), null)));
    const layout = layoutOf(state, raceRoot, now);
    const cause = causeFor({ kind: "connector", item: connectorOf(layout, "cancel") }, contextOf(state, raceRoot), now);
    expect(cause.sentence).toBe("The worker did not say why the call was cancelled.");
    expect(cause.location).toBeNull();
    expect(cause.missing).toContain("names the call site");
    expect(cause.rows).toContainEqual(["cause", "not reported"]);
  });
});

/** Rewrites the `cause` of every `remote_cancel`. `null` removes the field, as an older worker would. */
function withCancelCause(state: AppState, cause: CancelCause | null): AppState {
  const events: AppState["events"] = {};
  for (const [key, list] of Object.entries(state.events)) {
    events[key as RunKey] = (list ?? []).map((event) => {
      if (event.type !== "remote_cancel") return event;
      const { cause: _drop, ...rest } = event;
      return (cause === null ? rest : { ...rest, cause }) as TimelineEvent;
    });
  }
  return { ...state, events };
}

// ---------------------------------------------------------------------------
// The other rows of the table in section 10.3
// ---------------------------------------------------------------------------

describe("the cause of the other timeline objects", () => {
  const now = at(20_000);

  it("a return arrow names where the parent took the result", () => {
    const state = playFixture(buildCentralFixture(), now);
    const root = runKey("local", CENTRAL_PARENT);
    const layout = layoutOf(state, root, now);
    const cause = causeFor({ kind: "connector", item: connectorOf(layout, "return") }, contextOf(state, root), now);
    expect(cause.title).toBe("remote result");
    expect(cause.sentence).toContain("succeeded on cloud");
    // The central fixture pauses the parent, so the result waits for the resume.
    expect(cause.sentence).toContain("taken at resume");
    expect(cause.location).toBeNull();
    expect(cause.missing).toContain("no process");
  });

  it("a return arrow that a running process took names the line and marks it approximate", () => {
    const state = playFixture(buildSpawnFixture(), now);
    const root = runKey("local", SPAWN_PARENT);
    const layout = layoutOf(state, root, now);
    const connectors = layout.connectors.filter((connector) => connector.kind === "return" && !connector.pending);
    const connector = connectors[0];
    if (connector === undefined) throw new Error("the spawn fixture has no delivered return");
    const cause = causeFor({ kind: "connector", item: connector }, contextOf(state, root), now);
    expect(cause.sentence).toContain("took the result");
    expect(cause.location).toMatchObject({ approximate: true });
  });

  it("a migration arrow names the command that moved the run", () => {
    const state = playFixture(buildRaceFixture(), now);
    const root = runKey("local", RACE_PARENT);
    const layout = layoutOf(state, root, now);
    const cause = causeFor({ kind: "connector", item: connectorOf(layout, "migration") }, contextOf(state, root), now);
    expect(cause.title).toBe("migration");
    expect(cause.sentence).toContain(`A "Resume on cloud2" command moved ${RACE_PARENT} from local to cloud2`);
    expect(cause.location).toMatchObject({ file: QUOTES, approximate: true });
  });

  it("a fork arrow names the snapshot it started from", () => {
    const state = playFixture(buildForkFixture(), now);
    const root = runKey("local", FORK_SOURCE);
    const layout = layoutOf(state, root, now);
    const cause = causeFor({ kind: "connector", item: connectorOf(layout, "fork") }, contextOf(state, root), now);
    expect(cause.title).toBe("fork");
    expect(cause.sentence).toContain(`${FORK_RUN} was forked from snapshot #1 of ${FORK_SOURCE}`);
    expect(cause.location).toMatchObject({ file: TRIP, approximate: true });
  });

  it("a sleeping gap names the sleep call site and the wake time", () => {
    const state = playFixture(buildFanoutFixture(), at(5000));
    const root = runKey("local", FANOUT_PARENT);
    const layout = layoutOf(state, root, at(5000));
    const gap = layout.gaps[0];
    if (gap === undefined) throw new Error("the fanout fixture has no gap");
    const cause = causeFor({ kind: "gap", item: gap }, contextOf(state, root), at(5000));
    expect(cause.sentence).toMatch(/^The run suspended itself for a sleep at quotes\.baml:\d+ and wakes at /);
    expect(cause.location).toMatchObject({ file: QUOTES, what: "the last sleep the run reported before it suspended" });
  });

  it("a sleeping gap marks a sleep it read from a position event as approximate", () => {
    // A `position` event is the last sleep that ANY thread of the segment
    // reported, not the sleep whose deadline this gap ends at. It is the same
    // class of evidence as every other position-derived location.
    const state = playFixture(buildFanoutFixture(), at(5000));
    const root = runKey("local", FANOUT_PARENT);
    const layout = layoutOf(state, root, at(5000));
    const gap = layout.gaps[0];
    if (gap === undefined) throw new Error("the fanout fixture has no gap");
    const cause = causeFor({ kind: "gap", item: gap }, contextOf(state, root), at(5000));
    expect(cause.location).toMatchObject({ file: QUOTES, approximate: true });
  });

  it("a sleeping gap ignores a sleep that another segment reported", () => {
    const state = playFixture(buildFanoutFixture(), at(5000));
    const root = runKey("local", FANOUT_PARENT);
    const layout = layoutOf(state, root, at(5000));
    const gap = layout.gaps[0];
    if (gap === undefined) throw new Error("the fanout fixture has no gap");
    const key = runKey("local", FANOUT_PARENT);
    const list = state.events[key] ?? [];
    const sleep = list.find((event) => event.type === "position" && event.op === "baml.sys.sleep");
    if (sleep === undefined || sleep.type !== "position") throw new Error("the fanout fixture reports no sleep");
    // A sleep of another segment of the same run, at a line of its own.
    const other = { ...sleep, segment: sleep.segment + 7, line: sleep.line + 100, ts: gap.start - 1 } as TimelineEvent;
    const injected: AppState = { ...state, events: { ...state.events, [key]: [...list, other] } };
    const cause = causeFor({ kind: "gap", item: gap }, contextOf(injected, root), at(5000));
    expect(cause.location?.line).not.toBe(sleep.line + 100);
  });

  it("a snapshot marker names the top user frame of its state dump", () => {
    const fixture = buildRaceFixture();
    const state = playFixture(fixture, now);
    const root = runKey("local", RACE_PARENT);
    const layout = layoutOf(state, root, now);
    const marker = markerOf(layout, "snapshot");
    const dump = fixture.states[`${marker.site}/${marker.run}/${marker.n}`] as StateDump;
    const frame = dump.threads[0]?.frames[0];
    const cause = causeFor(
      { kind: "marker", item: marker },
      contextOf(state, root, { site: marker.site, run: marker.run, n: marker.n, dump }),
      now,
    );
    expect(cause.title).toBe("snapshot #1");
    expect(cause.sentence).toContain("A Pause command asked for this snapshot");
    expect(cause.location).toMatchObject({ file: frame?.file, line: frame?.line, approximate: false });
    expect(cause.location?.what).toContain("top user frame");
  });

  it("a snapshot marker says which thread the frame belongs to", () => {
    // A phase 3 snapshot holds every live thread. The frame is the top frame of
    // one of them, not the position of the whole run.
    const fixture = buildFanoutFixture();
    const state = playFixture(fixture, now);
    const root = runKey("local", FANOUT_PARENT);
    const layout = layoutOf(state, root, now);
    const marker = markerOf(layout, "snapshot");
    const dump = fixture.states[`${marker.site}/${marker.run}/${marker.n}`] as StateDump;
    expect(dump.threads.length).toBeGreaterThan(1);
    const cause = causeFor(
      { kind: "marker", item: marker },
      contextOf(state, root, { site: marker.site, run: marker.run, n: marker.n, dump }),
      now,
    );
    const thread = dump.threads.find((entry) => entry.frames.some((frame) => !frame.file.startsWith("<builtin>")));
    expect(cause.location?.what).toContain(`thread ${thread?.thread}`);
    expect(cause.sentence).toContain(`Thread ${thread?.thread} stood at`);
  });

  it("a snapshot marker falls back to the last position when the dump is not loaded", () => {
    const state = playFixture(buildRaceFixture(), now);
    const root = runKey("local", RACE_PARENT);
    const layout = layoutOf(state, root, now);
    const cause = causeFor({ kind: "marker", item: markerOf(layout, "snapshot") }, contextOf(state, root), now);
    expect(cause.location).toMatchObject({ file: QUOTES, approximate: true });
  });

  it("a self-suspend snapshot says that the run suspended itself", () => {
    const state = playFixture(buildFanoutFixture(), now);
    const root = runKey("local", FANOUT_PARENT);
    const layout = layoutOf(state, root, now);
    const cause = causeFor({ kind: "marker", item: markerOf(layout, "snapshot") }, contextOf(state, root), now);
    expect(cause.rows).toContainEqual(["kind", "self-suspend"]);
    expect(cause.sentence).toContain("The run suspended itself");
  });

  it("a segment bar names its first and its last position", () => {
    const state = playFixture(buildCentralFixture(), now);
    const root = runKey("local", CENTRAL_PARENT);
    const layout = layoutOf(state, root, now);
    const bar = layout.segments.find((segment) => segment.run === CENTRAL_PARENT && segment.segment === 1);
    if (bar === undefined) throw new Error("the central fixture has no first segment");
    const cause = causeFor({ kind: "segment", item: bar }, contextOf(state, root), now);
    expect(cause.title).toBe("segment 1");
    expect(cause.sentence).toMatch(/It ran from trip\.baml:\d+ to trip\.baml:\d+\.$/);
    expect(cause.rows.map(([label]) => label)).toEqual(
      expect.arrayContaining(["first position", "last position"]),
    );
  });

  it("a thread sub-bar names the spawn site that created it", () => {
    const state = playFixture(buildRaceFixture(), now);
    const root = runKey("local", RACE_PARENT);
    const layout = layoutOf(state, root, now);
    const thread = layout.threads.find((bar) => bar.run === RACE_PARENT && bar.segment === 1);
    if (thread === undefined) throw new Error("the race fixture has no spawned thread in segment 1");
    const cause = causeFor({ kind: "thread", item: thread }, contextOf(state, root), now);
    expect(cause.title).toBe(`thread ${thread.thread}`);
    expect(cause.sentence).toMatch(/^Thread \d+ was spawned by thread 1 at quotes\.baml:\d+\.$/);
    expect(cause.location).toMatchObject({ file: QUOTES, approximate: false, what: "the spawn site in the parent thread" });
  });

  it("a thread sub-bar falls back to the last position of its parent thread", () => {
    const state = stripCallSites(playFixture(buildRaceFixture(), now));
    const root = runKey("local", RACE_PARENT);
    const layout = layoutOf(state, root, now);
    const thread = layout.threads.find((bar) => bar.run === RACE_PARENT && bar.segment === 1);
    if (thread === undefined) throw new Error("the race fixture has no spawned thread in segment 1");
    const cause = causeFor({ kind: "thread", item: thread }, contextOf(state, root), now);
    expect(cause.location).toMatchObject({ approximate: true });
    expect(cause.location?.what).toContain("before the spawn");
  });

  it("a wait band repeats the cause of the call it waits on", () => {
    const state = playFixture(buildRaceFixture(), now);
    const root = runKey("local", RACE_PARENT);
    const layout = layoutOf(state, root, now);
    const wait = layout.waits[0];
    if (wait === undefined) throw new Error("the race fixture has no wait");
    const cause = causeFor({ kind: "wait", item: wait }, contextOf(state, root), now);
    expect(cause.title).toBe("waiting on a remote call");
    expect(cause.sentence).toContain("called remote_get_quote at");
    expect(cause.location).toMatchObject({ file: QUOTES, approximate: false });
  });
});

// ---------------------------------------------------------------------------
// The fields on the wire
// ---------------------------------------------------------------------------

describe("the fixtures carry the fields of section 10.1", () => {
  const cases: [string, () => { events: unknown[] }][] = [
    ["race", buildRaceFixture],
    ["deadline", buildDeadlineFixture],
    ["fanout", buildFanoutFixture],
    ["central", buildCentralFixture],
    ["spawn", buildSpawnFixture],
    ["fork", buildForkFixture],
  ];
  it.each(cases)("%s: every remote_call names a call site and every spawned thread a spawn site", (_name, build) => {
    const events = build().events.flatMap((raw) => {
      const parsed = parseSseEvent(raw, "local");
      return parsed === null ? [] : [parsed];
    });
    const calls = events.filter((event) => event.type === "remote_call");
    expect(calls.length).toBeGreaterThan(0);
    for (const call of calls) {
      expect(typeof call.file).toBe("string");
      expect(typeof call.line).toBe("number");
    }
    for (const started of events.filter((event) => event.type === "thread_started")) {
      if (started.parent_thread === null) {
        expect([started.file, started.line]).toEqual([null, null]);
      } else {
        expect(typeof started.file).toBe("string");
        expect(typeof started.line).toBe("number");
      }
    }
    for (const cancel of events.filter((event) => event.type === "remote_cancel")) {
      expect(["future_cancel", "token", "parent", "unknown"]).toContain(cancel.cause);
      expect(typeof cancel.file).toBe("string");
      expect(typeof cancel.line).toBe("number");
    }
  });

  it("the run record of a parent keeps the call site in waiting_on and in calls (section 10.2)", () => {
    const state = playFixture(buildRaceFixture(), at(1200));
    const record = state.runs[runKey("local", RACE_PARENT)] as Run;
    expect(record.waiting_on.length).toBeGreaterThan(0);
    for (const entry of record.waiting_on) {
      expect(entry.file).toBe(QUOTES);
      expect(typeof entry.line).toBe("number");
    }
    for (const call of record.calls ?? []) {
      expect(call.file).toBe(QUOTES);
      expect(typeof call.line).toBe("number");
    }
  });

  it("a location survives a reload that rebuilds the history from events.jsonl", () => {
    // What a page reload does: the run records arrive on `init`, and the
    // history of each run is read from `GET /api/runs/:id/events` and merged.
    const fixture = buildRaceFixture();
    const now = at(20_000);
    const live = playFixture(fixture, now);
    let reloaded = initialState();
    for (const run of Object.values(live.runs)) {
      reloaded = reducer(reloaded, { type: "sse", site: run.site, event: { type: "run", ts: run.updated_ts, site: run.site, run } });
    }
    for (const [key, list] of Object.entries(live.events)) {
      const { site, id } = splitRunKey(key as RunKey);
      // `events.jsonl` holds the worker events and the server's events about the
      // run, as JSON text. It never holds `run` or `init` messages (section 7.5).
      const raw = (list ?? []).filter((event) => event.type !== "ui_status").map((event) => JSON.parse(JSON.stringify(event)) as unknown);
      const parsed = raw.flatMap((item) => {
        const event = parseSseEvent(item, site);
        return event === null ? [] : [event];
      });
      reloaded = reducer(reloaded, { type: "backfill", site, run: id, events: parsed });
    }
    const root = runKey("local", RACE_PARENT);
    const layout = layoutOf(reloaded, root, now);
    const call = causeFor({ kind: "connector", item: connectorOf(layout, "call") }, contextOf(reloaded, root), now);
    expect(call.location).toMatchObject({ file: QUOTES, approximate: false });
    const cancel = causeFor({ kind: "connector", item: connectorOf(layout, "cancel") }, contextOf(reloaded, root), now);
    expect(cancel.sentence).toContain(CANCEL_CAUSE_TEXT.future_cancel);
    expect(cancel.location).toMatchObject({ file: QUOTES, approximate: false });
  });

  it("a child run that is dispatched keeps its parent's call in the tree", () => {
    const state = playFixture(buildCentralFixture(), at(20_000));
    expect(state.runs[runKey("cloud", CENTRAL_CHILD)]).toBeDefined();
  });
});
