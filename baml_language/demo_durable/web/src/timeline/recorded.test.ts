/**
 * The timeline of a run that the site servers recorded: `durable_fan_out` with
 * the mock worker, from the run stores of the three sites after the run
 * completed. The test loads the data the way a page reload does: the run
 * records first, then the contents of every `events.jsonl` as a backfill.
 *
 * The recording differs from the hand-written fixture in ways that matter: the
 * run suspends itself before its site has dispatched the calls, so
 * `remote_dispatched` follows the `paused` event, the four children end in
 * another order than they were called, and the resumed worker announces every
 * restored thread.
 */

import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { parseSseEvent, type Run, type SseEvent } from "../protocol";
import { scenarioById } from "../scenarios/catalog";
import { scenarioProgress, scenarioView } from "../scenarios/engine";
import { groupRuns, initialState, reducer, runKey, runTree, splitRunKey, type AppState, type RunKey } from "../state";
import { causeOf, type CauseContext } from "./cause";
import { computeTimelineLayout } from "./layout";

interface Recording {
  runs: Run[];
  events: Record<string, unknown[]>;
}

const recording = JSON.parse(readFileSync(new URL("../fixtures/recorded/fanout-mock.json", import.meta.url), "utf8")) as Recording;
const ROOT = "r-t7xyzw";

function load(): AppState {
  let state = initialState();
  for (const run of recording.runs) {
    state = reducer(state, { type: "sse", site: run.site, event: { type: "run", ts: run.updated_ts, site: run.site, run } });
  }
  for (const [key, raw] of Object.entries(recording.events)) {
    const { site, id } = splitRunKey(key as RunKey);
    const events = raw.flatMap((item): SseEvent[] => {
      const event = parseSseEvent(item, site);
      return event ? [event] : [];
    });
    state = reducer(state, { type: "backfill", site, run: id, events });
  }
  return state;
}

describe("recorded fan-out (mock worker, three site servers)", () => {
  const state = load();
  const root = runKey("local", ROOT);
  const runs = runTree(state, root).flatMap((key) => (state.runs[key] ? [state.runs[key] as Run] : []));
  const layout = computeTimelineLayout({ runs, events: state.events, now: (state.runs[root] as Run).updated_ts + 60_000 });
  const t0 = layout.t0;

  it("finds the four children on cloud and cloud2 and lists them under the parent", () => {
    expect(runs).toHaveLength(5);
    expect(groupRuns(Object.values(state.runs)).map((row) => [row.run.site, row.depth])).toEqual([["local", 0], ["cloud", 1], ["cloud2", 1], ["cloud2", 1], ["cloud", 1]]);
  });

  it("draws the sleep as a closed sleeping gap that the timer ended", () => {
    expect(layout.gaps).toHaveLength(1);
    const gap = layout.gaps[0];
    expect(gap).toMatchObject({ kind: "sleeping", open: false, wakeReason: "timer", snapshotN: 1, bytes: 1835 });
    // The timer was set for 7 s after the snapshot. It fired 3 ms late, and the new process wrote its first event 27 ms after that.
    expect((gap?.wakeAt ?? 0) - t0).toBe(7013);
    expect([(gap?.start ?? 0) - t0, (gap?.end ?? 0) - t0]).toEqual([17, 7043]);
    expect(layout.markers.filter((marker) => marker.kind === "wake").map((marker) => marker.t - t0)).toEqual([7016]);
    expect(layout.markers.filter((marker) => marker.kind === "pause_request")).toHaveLength(0);
  });

  it("continues the four spawned threads across the sleep on their lines", () => {
    const threads = layout.threads.filter((thread) => thread.run === ROOT);
    for (const id of [2, 3, 4, 5]) {
      const bars = threads.filter((thread) => thread.thread === id);
      expect(bars.map((bar) => [bar.segment, bar.continued, bar.continues])).toEqual([[1, false, true], [2, true, false]]);
      expect(bars[0]?.subRow).toBe(bars[1]?.subRow);
    }
    expect(layout.threadLinks.map((link) => link.thread).sort()).toEqual([2, 3, 4, 5]);
    // Thread 6 is the thread of `baml.future.all`. It exists in the second segment only.
    expect(threads.filter((thread) => thread.thread === 6).map((bar) => [bar.segment, bar.continued])).toEqual([[2, false]]);
    expect(layout.segments.filter((bar) => bar.run === ROOT).map((bar) => bar.rootThread)).toEqual([1, 1]);
  });

  it("joins every child to the parent with a call and a return, although the dispatch followed the snapshot", () => {
    const kinds = layout.connectors.map((connector) => `${connector.kind}:${connector.from.site}>${connector.to.site}`).sort();
    expect(kinds).toEqual([
      "call:local>cloud", "call:local>cloud", "call:local>cloud2", "call:local>cloud2",
      "return:cloud2>local", "return:cloud2>local", "return:cloud>local", "return:cloud>local",
    ]);
    // Every result was delivered to the resumed process, not left pending.
    expect(layout.connectors.filter((connector) => connector.kind === "return").every((connector) => !connector.pending && connector.to.t - t0 === 7044)).toBe(true);
    // Two children per cloud lane run at the same time, so each lane has two rows.
    expect(layout.lanes.map((lane) => [lane.site, lane.rows.length])).toEqual([["local", 1], ["cloud", 2], ["cloud2", 2]]);
  });

  // Contract section 10.3: this recording predates section 10, so no worker
  // event and no run record names a call site. The cause panel must still
  // explain a call, from the last position of the calling thread, and it must
  // say that the location is approximate.
  it("names the cause of a call of an older recording from the last position, marked approximate", () => {
    const call = layout.connectors.find((connector) => connector.kind === "call");
    if (call === undefined) throw new Error("the recording has no call connector");
    const ctx: CauseContext = { runs, events: state.events, snapshot: null };
    const cause = causeOf({ kind: "connector", item: call }, ctx, layout.t1);
    expect(cause.title).toBe("remote call");
    expect(cause.sentence).toMatch(/^durable_fan_out called remote_get_quote at quotes\.baml:\d+, placed on cloud2?\.$/);
    expect(cause.location).toMatchObject({ file: "baml_src/quotes.baml", approximate: true });
    expect(cause.missing).toBeNull();
  });

  it("walks the scenario guide to its end", () => {
    const scenario = scenarioById("fanout");
    if (!scenario) throw new Error("no fanout scenario");
    const progress = scenarioProgress(scenario, scenarioView(state, { root: { site: "local", id: ROOT } }, layout.t1));
    expect(progress.finished).toBe(true);
    expect(progress.states).toEqual(["done", "done", "done", "done"]);
  });
});
