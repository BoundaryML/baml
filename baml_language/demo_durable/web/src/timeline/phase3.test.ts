/**
 * Layout and geometry tests for contract section 9.6: sleeping gaps, cancelled
 * children and their connectors, children stacked in rows, threads that
 * continue across segments, and the wake and cancel markers.
 */

import { describe, expect, it } from "vitest";
import { FIXTURE_EPOCH } from "../fixtures/builder";
import { buildDeadlineFixture, DEADLINE_CHILD, DEADLINE_PARENT } from "../fixtures/deadline";
import { buildFanoutFixture, FANOUT_CHILDREN, FANOUT_PARENT } from "../fixtures/fanout";
import { buildRaceFixture, RACE_CHILDREN, RACE_PARENT } from "../fixtures/race";
import { buildRecoverFixture, RECOVER_DURABLE } from "../fixtures/recover";
import { buildSettledFixture, SETTLED_PARENT } from "../fixtures/settled";
import { formatClockShort } from "../format";
import { parseSseEvent, type Run } from "../protocol";
import { reducer, runKey, runTree, type AppState, type RunKey } from "../state";
import { playFixture } from "../testing";
import { connectorRoute, fitView, GAP_LABEL_CHAR_W, LABEL_CHAR_W, LABEL_H, placeBarLabels, plotGeometry, RIGHT_PAD, sleepGapLabel, subRowY, timeScale } from "./geometry";
import { computeTimelineLayout, type Marker, type TimelineLayout } from "./layout";

const at = (ms: number): number => FIXTURE_EPOCH + ms;

function layoutOf(state: AppState, root: RunKey, now: number): TimelineLayout {
  const runs = runTree(state, root).flatMap((key) => (state.runs[key] ? [state.runs[key] as Run] : []));
  return computeTimelineLayout({ runs, events: state.events, now });
}

function send(state: AppState, event: Record<string, unknown>): AppState {
  const parsed = parseSseEvent(event, "local");
  if (parsed === null) throw new Error("the test event does not parse");
  return reducer(state, { type: "sse", site: parsed.site, event: parsed });
}

const markersOf = <K extends Marker["kind"]>(layout: TimelineLayout, kind: K): Extract<Marker, { kind: K }>[] =>
  layout.markers.filter((marker): marker is Extract<Marker, { kind: K }> => marker.kind === kind);

describe("sleeping gap (fanout fixture)", () => {
  const fixture = buildFanoutFixture();
  const root = runKey("local", FANOUT_PARENT);

  it("draws an open gap that reaches the wake time while the run sleeps", () => {
    const now = at(5000);
    const layout = layoutOf(playFixture(fixture, now), root, now);
    expect(layout.gaps).toHaveLength(1);
    const gap = layout.gaps[0];
    expect(gap).toMatchObject({ run: FANOUT_PARENT, site: "local", kind: "sleeping", open: true, fork: false, snapshotN: 1, wakeReason: null });
    // The site server set the timer for the receipt time plus `remaining_ms`.
    expect(gap?.wakeAt).toBe(at(137 + 3 + 11_984));
    expect(gap?.start).toBe(at(139));
    // The gap ends in the future, at the wake time, and the time range includes it.
    expect(gap?.end).toBe(gap?.wakeAt);
    expect(layout.t1).toBe(gap?.wakeAt);
    // No process exists, but the countdown moves, so the timeline keeps ticking.
    expect(layout.live).toBe(true);
    expect(fitView(layout).end).toBeGreaterThan(gap?.wakeAt ?? 0);
  });

  it("takes the wake time from the run record when the event list is not loaded", () => {
    const now = at(5000);
    const state = playFixture(fixture, now);
    const record = state.runs[root] as Run;
    // Only the `hello` of segment 1: the state right after a page load, before the history arrives.
    const hello = (state.events[root] ?? []).filter((event) => event.type === "hello" || event.type === "worker_exit");
    const layout = computeTimelineLayout({ runs: [record], events: { [root]: hello }, now });
    expect(layout.gaps[0]).toMatchObject({ kind: "sleeping", open: true, wakeAt: record.wake_at });
  });

  it("falls back to the worker's remaining_ms when the site server sent no timer event", () => {
    const now = at(5000);
    const state = playFixture(fixture, now);
    const events = (state.events[root] ?? []).filter((event) => event.type !== "sleep_scheduled");
    const record = { ...(state.runs[root] as Run), wake_at: null };
    const layout = computeTimelineLayout({ runs: [record], events: { [root]: events }, now });
    expect(layout.gaps[0]?.wakeAt).toBe(at(137 + 11_984));
  });

  it("closes the gap at the first event of the woken segment and names the reason", () => {
    const layout = layoutOf(playFixture(fixture), root, at(60_000));
    const gap = layout.gaps[0];
    expect(gap).toMatchObject({ kind: "sleeping", open: false, wakeReason: "timer", end: at(12_157) });
    expect(layout.live).toBe(false);
    const wake = markersOf(layout, "wake");
    expect(wake).toHaveLength(1);
    expect(wake[0]).toMatchObject({ reason: "timer", wakeAt: at(12_124), t: at(12_124), segment: 2, site: "local", run: FANOUT_PARENT });
    expect(wake[0]?.sleptMs).toBe(12_124 - 140);
  });

  it("marks the snapshot of a self-suspension and draws no pause request for it", () => {
    const layout = layoutOf(playFixture(fixture), root, at(60_000));
    expect(markersOf(layout, "pause_request")).toHaveLength(0);
    const snapshot = markersOf(layout, "snapshot");
    expect(snapshot).toHaveLength(1);
    expect(snapshot[0]?.wake).toEqual({ remainingMs: 11_984, wakeAt: at(12_124) });
    expect(snapshot[0]?.automatic).toBe(false);
  });

  it("keeps a requested pause apart from a sleep: the race fixture has a pause request and a plain gap", () => {
    const layout = layoutOf(playFixture(buildRaceFixture()), runKey("local", RACE_PARENT), at(60_000));
    expect(markersOf(layout, "pause_request")).toHaveLength(1);
    expect(layout.gaps.map((gap) => [gap.kind, gap.wakeAt])).toEqual([["paused", null]]);
    expect(markersOf(layout, "snapshot")[0]?.wake).toBeNull();
  });

  it("draws the results that arrive during the sleep as pending returns into the gap", () => {
    const now = at(4500);
    const layout = layoutOf(playFixture(fixture, now), root, now);
    const returns = layout.connectors.filter((connector) => connector.kind === "return");
    // Three of the four children are done at 4.5 s: the flight and the car on cloud, and the tour on cloud2.
    expect(returns.map((connector) => [connector.callId, connector.from.site, connector.to.site, connector.pending])).toEqual([
      [`${FANOUT_PARENT}-c1`, "cloud", "local", true],
      [`${FANOUT_PARENT}-c3`, "cloud", "local", true],
      [`${FANOUT_PARENT}-c4`, "cloud2", "local", true],
    ]);
    // After the wake, every result is delivered to the new process.
    const done = layoutOf(playFixture(fixture), root, at(60_000));
    const delivered = done.connectors.filter((connector) => connector.kind === "return");
    expect(delivered).toHaveLength(4);
    expect(delivered.every((connector) => !connector.pending && connector.to.t > at(12_157))).toBe(true);
  });

  it("labels the gap with the wake time and a countdown, and with the slept time afterwards", () => {
    const open = { open: true, start: at(139), end: at(12_124), wakeAt: at(12_124), snapshotN: 1, bytes: 6860 };
    const clock = formatClockShort(at(12_124));
    expect(sleepGapLabel(open, at(4124), 400)).toBe(`sleeping until ${clock} · 8.0 s`);
    expect(sleepGapLabel(open, at(12_000), 400)).toBe(`sleeping until ${clock} · 0.1 s`);
    // The countdown stops at zero when the timer is late.
    expect(sleepGapLabel(open, at(13_000), 400)).toBe(`sleeping until ${clock} · 0.0 s`);
    // A narrow gap keeps the countdown, which is the part that moves.
    expect(sleepGapLabel(open, at(4124), 140)).toBe(`until ${clock} · 8.0 s`);
    expect(sleepGapLabel(open, at(4124), 40)).toBe("8.0 s");
    expect(sleepGapLabel(open, at(4124), 10)).toBe("");
    const closed = { ...open, open: false, end: at(12_157) };
    expect(sleepGapLabel(closed, at(60_000), 400)).toBe("slept 12.02 s · no process · snapshot #1 6.70 KB");
    expect(sleepGapLabel(closed, at(60_000), 170)).toBe("slept 12.02 s · no process");
    expect(sleepGapLabel(closed, at(60_000), 100)).toBe("slept 12.02 s");
    // Every variant that is returned fits.
    for (const available of [60, 90, 150, 220, 400]) {
      expect(sleepGapLabel(closed, at(60_000), available).length * GAP_LABEL_CHAR_W).toBeLessThanOrEqual(available);
    }
  });

  it("marks a manual wake: a resume command that arrives before the timer", () => {
    const now = at(5000);
    let state = playFixture(fixture, now);
    const record = state.runs[root] as Run;
    state = send(state, { type: "woken", ts: at(5200), site: "local", run: FANOUT_PARENT, reason: "manual" });
    state = send(state, { type: "run", ts: at(5200), site: "local", run: { ...record, status: "starting", segment: 2, wake_at: null, updated_ts: at(5200) } });
    state = send(state, { v: 1, type: "hello", ts: at(5240), site: "local", run: FANOUT_PARENT, segment: 2, pid: 7, mode: "resume", function: "durable_fan_out", durable: true });
    const layout = layoutOf(state, root, at(5300));
    expect(layout.gaps[0]).toMatchObject({ kind: "sleeping", open: false, end: at(5240), wakeReason: "manual" });
    expect(markersOf(layout, "wake")[0]).toMatchObject({ reason: "manual", t: at(5200) });
  });
});

describe("cancelled children (race and deadline fixtures)", () => {
  it("ends the losers of a race as cancelled bars and joins each to the parent with a cancel connector", () => {
    const state = playFixture(buildRaceFixture());
    const layout = layoutOf(state, runKey("local", RACE_PARENT), at(60_000));
    const children = layout.segments.filter((bar) => RACE_CHILDREN.includes(bar.run as (typeof RACE_CHILDREN)[number]));
    expect(children.map((bar) => [bar.run, bar.site, bar.endKind])).toEqual([
      [RACE_CHILDREN[0], "cloud", "completed"],
      [RACE_CHILDREN[1], "cloud2", "cancelled"],
      [RACE_CHILDREN[2], "cloud", "cancelled"],
    ]);
    const cancels = layout.connectors.filter((connector) => connector.kind === "cancel");
    // The parent moved to cloud2 before the race settled, so the cancels leave from there.
    expect(cancels.map((connector) => [connector.from.site, connector.to.site, connector.to.run, connector.pending])).toEqual([
      ["cloud2", "cloud2", RACE_CHILDREN[1], false],
      ["cloud2", "cloud", RACE_CHILDREN[2], false],
    ]);
    for (const connector of cancels) {
      const child = children.find((bar) => bar.run === connector.to.run);
      // The connector leaves at `remote_cancel` and arrives where the child's process ended.
      expect(connector.to.t).toBe(child?.end);
      expect(connector.from.t).toBeLessThan(connector.to.t);
      // It starts on the sub-bar of the thread that was cancelled.
      expect(connector.from.subRow).not.toBeNull();
    }
    // The winner returns. A cancelled call has no return connector, and the migrated site draws no second call connector.
    expect(layout.connectors.filter((connector) => connector.kind === "return").map((connector) => connector.from.run)).toEqual([RACE_CHILDREN[0]]);
    expect(layout.connectors.filter((connector) => connector.kind === "call")).toHaveLength(3);
  });

  it("places one cancel marker per cancelled call on the parent's row, with the child it names", () => {
    const layout = layoutOf(playFixture(buildRaceFixture()), runKey("local", RACE_PARENT), at(60_000));
    const markers = markersOf(layout, "cancel");
    expect(markers.map((marker) => [marker.site, marker.callId, marker.thread, marker.childSite, marker.childRun, marker.source])).toEqual([
      ["cloud2", `${RACE_PARENT}-c2`, 3, "cloud2", RACE_CHILDREN[1], "worker"],
      ["cloud2", `${RACE_PARENT}-c3`, 4, "cloud", RACE_CHILDREN[2], "worker"],
    ]);
  });

  it("ends the wait of a cancelled call at the cancel, and marks only that wait as cancelled", () => {
    const layout = layoutOf(playFixture(buildDeadlineFixture()), runKey("local", DEADLINE_PARENT), at(60_000));
    const waits = layout.waits.filter((wait) => wait.run === DEADLINE_PARENT);
    expect(waits.map((wait) => [wait.segment, wait.cancelled, wait.end])).toEqual([
      [1, false, at(914)],
      [2, true, at(2052)],
    ]);
    const cancel = layout.connectors.find((connector) => connector.kind === "cancel");
    expect(cancel).toMatchObject({ from: { site: "local", t: at(2052) }, to: { site: "cloud", run: DEADLINE_CHILD, t: at(2076) }, pending: false });
    expect(layout.segments.find((bar) => bar.run === DEADLINE_CHILD)?.endKind).toBe("cancelled");
  });

  it("draws a cancel that is still in flight as pending, up to now", () => {
    const fixture = buildDeadlineFixture();
    // `remote_cancel` is at 2052 ms, and the child's process ends at 2076 ms.
    const now = at(2060);
    const layout = layoutOf(playFixture(fixture, now), runKey("local", DEADLINE_PARENT), now);
    const cancel = layout.connectors.find((connector) => connector.kind === "cancel");
    expect(cancel).toMatchObject({ pending: true, to: { t: now } });
  });

  it("draws the cancel of a run that ended from the site server's event alone", () => {
    // A parent that failed: no worker reports `remote_cancel`, and the site cancels the outstanding child (section 9.3).
    const fixture = buildDeadlineFixture();
    const state = playFixture({ ...fixture, events: fixture.events.filter((event) => event.type !== "remote_cancel") });
    const layout = layoutOf(state, runKey("local", DEADLINE_PARENT), at(60_000));
    expect(markersOf(layout, "cancel")[0]).toMatchObject({ source: "site", thread: null, childRun: DEADLINE_CHILD, t: at(2055) });
    expect(layout.connectors.find((connector) => connector.kind === "cancel")).toMatchObject({ from: { t: at(2055), subRow: null }, to: { run: DEADLINE_CHILD } });
  });

  it("draws no cancel connector to a child that had completed before the cancel arrived", () => {
    const fixture = buildSettledFixture();
    let state = playFixture(fixture);
    // A late cancel for the call of a child that is long done.
    state = send(state, { type: "remote_cancelled", ts: at(9000), site: "local", run: SETTLED_PARENT, call_id: `${SETTLED_PARENT}-c1`, child_site: "cloud", child_run: "r-qj3f7x" });
    const layout = layoutOf(state, runKey("local", SETTLED_PARENT), at(60_000));
    expect(layout.connectors.filter((connector) => connector.kind === "cancel")).toHaveLength(0);
  });

  it("routes a cancel inside one lane and between lanes", () => {
    const layout = layoutOf(playFixture(buildRaceFixture()), runKey("local", RACE_PARENT), at(60_000));
    const plot = plotGeometry(layout.lanes);
    const scale = timeScale(fitView(layout), 1000);
    for (const connector of layout.connectors.filter((candidate) => candidate.kind === "cancel")) {
      const route = connectorRoute(plot, connector, scale.x, layout.segments);
      expect(route.d).toMatch(/^M[\d.]+,[\d.]+ C/);
      expect(route.d).not.toContain("NaN");
      const lane = plot.lanes.find((candidate) => candidate.site === connector.to.site);
      expect(route.tip[1]).toBeGreaterThanOrEqual(lane?.top ?? Infinity);
      expect(route.tip[1]).toBeLessThanOrEqual((lane?.top ?? 0) + (lane?.height ?? 0));
    }
  });
});

describe("children stacked in rows (fan-out of 4 and of 8)", () => {
  for (const width of [4, 8]) {
    it(`stacks ${width} children in ${width / 2} rows per cloud lane without overlapping bars or labels`, () => {
      const state = playFixture(buildFanoutFixture(width));
      const layout = layoutOf(state, runKey("local", FANOUT_PARENT), at(60_000));
      const children = layout.segments.filter((bar) => bar.run !== FANOUT_PARENT);
      expect(children).toHaveLength(width);
      // Round robin: even calls on cloud, odd calls on cloud2, one row per child because they overlap in time.
      for (const site of ["cloud", "cloud2"]) {
        const inLane = children.filter((bar) => bar.site === site);
        expect(inLane).toHaveLength(width / 2);
        expect(inLane.map((bar) => bar.row).sort()).toEqual(Array.from({ length: width / 2 }, (_, index) => index));
        expect(layout.lanes.find((lane) => lane.site === site)?.rows).toHaveLength(width / 2);
      }
      expect(layout.lanes.find((lane) => lane.site === "local")?.rows).toHaveLength(1);

      // Bars of one row never overlap in time.
      for (const a of layout.segments) {
        for (const b of layout.segments) {
          if (a === b || a.site !== b.site || a.row !== b.row) continue;
          expect(a.end <= b.start || b.end <= a.start).toBe(true);
        }
      }

      // Label boxes, in pixels at the width of the demo: none intersects another.
      const widthPx = 760;
      const scale = timeScale(fitView(layout), widthPx);
      const plot = plotGeometry(layout.lanes);
      const labels = placeBarLabels(layout.segments, scale.x, widthPx - RIGHT_PAD);
      const boxes = layout.segments.flatMap((bar) => {
        const label = labels.get(bar.id);
        const row = plot.lanes.find((lane) => lane.site === bar.site)?.rows[bar.row];
        if (!label || label.text === "" || !row) return [];
        const w = label.text.length * LABEL_CHAR_W;
        const left = label.anchor === "start" ? label.x : label.x - w;
        return [{ id: bar.id, left, right: left + w, top: row.barY - LABEL_H, bottom: row.barY }];
      });
      // Every child keeps a label that names its run.
      expect(boxes.filter((box) => FANOUT_CHILDREN.some((child) => box.id.includes(child)))).toHaveLength(width);
      for (const a of boxes) {
        for (const b of boxes) {
          if (a === b) continue;
          const apart = a.right <= b.left || b.right <= a.left || a.bottom <= b.top || b.bottom <= a.top;
          expect(apart, `${a.id} and ${b.id}`).toBe(true);
        }
      }
      // A label never reaches into the bar of the row above it.
      for (const lane of plot.lanes) {
        lane.rows.forEach((row, index) => {
          const above = lane.rows[index - 1];
          if (above) expect(row.barY - LABEL_H).toBeGreaterThanOrEqual(above.barBottom);
        });
      }
    });
  }

  it("keeps the row of a child when later children arrive", () => {
    const fixture = buildFanoutFixture(8);
    const root = runKey("local", FANOUT_PARENT);
    const early = layoutOf(playFixture(fixture, at(110)), root, at(110));
    const late = layoutOf(playFixture(fixture), root, at(60_000));
    for (const bar of early.segments) {
      expect(late.segments.find((candidate) => candidate.id === bar.id)?.row).toBe(bar.row);
    }
  });
});

describe("threads across segments", () => {
  it("carries the spawned threads of a suspended run into the resumed segment, without thread_started events", () => {
    const layout = layoutOf(playFixture(buildFanoutFixture()), runKey("local", FANOUT_PARENT), at(60_000));
    const threads = layout.threads.filter((thread) => thread.run === FANOUT_PARENT);
    const byThread = (id: number) => threads.filter((thread) => thread.thread === id);
    for (const id of [2, 3, 4, 5]) {
      const [first, second] = byThread(id);
      expect(byThread(id)).toHaveLength(2);
      expect(first).toMatchObject({ segment: 1, continued: false, continues: true, parentThread: 1 });
      // The resumed worker announced only the root thread. The thread is back from `resumed` on, with its parent.
      expect(second).toMatchObject({ segment: 2, continued: true, continues: false, parentThread: 1, start: at(12_161) });
      // One sub-row for both bars, so the thread stays on its line.
      expect(second?.subRow).toBe(first?.subRow);
      // The first bar ends with its process, and the second one at `thread_ended`.
      expect(first?.end).toBe(at(139));
      expect(second?.end).toBeLessThan(at(12_180));
    }
    // Four lines, one per thread, and the thread of `baml.future.all` reuses a free one.
    expect(new Set(threads.filter((thread) => thread.thread <= 5).map((thread) => thread.subRow)).size).toBe(4);
    expect(byThread(6)).toHaveLength(1);
    expect(byThread(6)[0]).toMatchObject({ segment: 2, continued: false, continues: false });
    expect(layout.lanes.find((lane) => lane.site === "local")?.rows[0]?.subRows).toBe(4);
  });

  it("joins the two bars of a thread with a link through the gap, on the thread's sub-row", () => {
    const layout = layoutOf(playFixture(buildFanoutFixture()), runKey("local", FANOUT_PARENT), at(60_000));
    expect(layout.threadLinks.map((link) => link.thread)).toEqual([2, 3, 4, 5]);
    for (const link of layout.threadLinks) {
      const [first, second] = layout.threads.filter((thread) => thread.thread === link.thread);
      expect(link).toMatchObject({ site: "local", run: FANOUT_PARENT, subRow: first?.subRow, start: first?.end, end: second?.start });
    }
    // In pixels: the link is on the same y as both bars.
    const plot = plotGeometry(layout.lanes);
    const row = plot.lanes[0]?.rows[0];
    if (!row) throw new Error("the local lane has no row");
    const ys = layout.threadLinks.map((link) => subRowY(row, link.subRow));
    expect(new Set(ys).size).toBe(4);
  });

  it("does not link a thread that is still open while the run sleeps", () => {
    const now = at(5000);
    const layout = layoutOf(playFixture(buildFanoutFixture(), now), runKey("local", FANOUT_PARENT), now);
    expect(layout.threadLinks).toHaveLength(0);
    // The bars end with the process, not at `now`: no thread runs while the run sleeps.
    expect(layout.threads.every((thread) => thread.end === at(139))).toBe(true);
  });

  it("uses the thread_started events of a worker that announces the restored threads", () => {
    const layout = layoutOf(playFixture(buildDeadlineFixture()), runKey("local", DEADLINE_PARENT), at(60_000));
    const threads = layout.threads.filter((thread) => thread.run === DEADLINE_PARENT);
    expect(threads.map((thread) => [thread.segment, thread.thread, thread.continued, thread.continues, thread.start])).toEqual([
      [1, 2, false, true, at(50)],
      [2, 2, true, false, at(1546)],
      [1, 3, false, true, at(52)],
      [2, 3, true, false, at(1546)],
    ]);
    expect(layout.threadLinks).toHaveLength(2);
    // The root thread of the resumed segment stays thread 1, although the worker announces it again without a parent.
    expect(layout.segments.filter((bar) => bar.run === DEADLINE_PARENT).map((bar) => bar.rootThread)).toEqual([1, 1]);
  });

  it("continues a thread that the real worker reports as ended right before `paused`, because its process ends", () => {
    // The real worker closes every live thread with `thread_ended` at the time of the `paused` event
    // (so that every thread_started has a thread_ended) and announces the restored threads again.
    const fixture = buildDeadlineFixture();
    const paused = fixture.events.find((event) => event.type === "paused" && event.run === DEADLINE_PARENT);
    if (!paused) throw new Error("the deadline fixture has no paused event");
    const closing = [2, 3].map((thread) => ({ ...paused, type: "thread_ended" as const, thread }));
    const events = fixture.events.flatMap((event) => (event === paused ? [...closing, event] : [event]));
    const layout = layoutOf(playFixture({ ...fixture, events } as typeof fixture), runKey("local", DEADLINE_PARENT), at(60_000));
    const threads = layout.threads.filter((thread) => thread.run === DEADLINE_PARENT);
    expect(threads.map((thread) => [thread.segment, thread.thread, thread.continued, thread.continues])).toEqual([
      [1, 2, false, true],
      [2, 2, true, false],
      [1, 3, false, true],
      [2, 3, true, false],
    ]);
    expect(layout.threadLinks.map((link) => link.thread)).toEqual([2, 3]);
    // Both bars of a thread share one sub-row.
    for (const id of [2, 3]) expect(new Set(threads.filter((thread) => thread.thread === id).map((thread) => thread.subRow)).size).toBe(1);
  });

  it("does not continue a thread that ended well before the pause, even if a later thread reuses nothing of it", () => {
    const fixture = buildDeadlineFixture();
    const paused = fixture.events.find((event) => event.type === "paused" && event.run === DEADLINE_PARENT);
    if (!paused) throw new Error("the deadline fixture has no paused event");
    // Thread 3 ended 500 ms before the pause, and the resumed worker does not announce it.
    const ended = { ...paused, type: "thread_ended" as const, thread: 3, ts: paused.ts - 500 };
    const events = fixture.events
      .filter((event) => !((event.type === "thread_started" || event.type === "thread_ended") && event.run === DEADLINE_PARENT && event.segment === 2 && event.thread === 3))
      .flatMap((event) => (event === paused ? [ended, event] : [event]))
      .sort((a, b) => a.ts - b.ts);
    const layout = layoutOf(playFixture({ ...fixture, events } as typeof fixture), runKey("local", DEADLINE_PARENT), at(60_000));
    expect(layout.threads.filter((thread) => thread.run === DEADLINE_PARENT && thread.thread === 3).map((thread) => [thread.segment, thread.continues])).toEqual([[1, false]]);
  });

  it("keeps the root thread when a resumed worker announces every thread without a parent", () => {
    const fixture = buildDeadlineFixture();
    const events = fixture.events.map((event) =>
      event.type === "thread_started" && event.run === DEADLINE_PARENT && event.segment === 2 ? { ...event, parent_thread: null } : event,
    );
    // The spawned threads come first, so the first parentless thread of the segment is not the root.
    const reordered = [...events].sort((a, b) => {
      const rank = (event: (typeof events)[number]): number => (event.type === "thread_started" && event.segment === 2 && event.run === DEADLINE_PARENT ? (event.thread === 1 ? 1 : 0) : 0);
      return a.ts - b.ts || rank(a) - rank(b);
    });
    const layout = layoutOf(playFixture({ ...fixture, events: reordered }), runKey("local", DEADLINE_PARENT), at(60_000));
    expect(layout.segments.filter((bar) => bar.run === DEADLINE_PARENT).map((bar) => bar.rootThread)).toEqual([1, 1]);
    expect(layout.threads.filter((thread) => thread.segment === 2).map((thread) => [thread.thread, thread.continued, thread.parentThread])).toEqual([
      [2, true, 1],
      [3, true, 1],
    ]);
  });

  it("restores the threads on the site that the run moved to, and draws no link between lanes", () => {
    const layout = layoutOf(playFixture(buildRaceFixture()), runKey("local", RACE_PARENT), at(60_000));
    const moved = layout.threads.filter((thread) => thread.site === "cloud2" && thread.run === RACE_PARENT);
    expect(moved.map((thread) => [thread.thread, thread.segment, thread.continued])).toEqual([[2, 2, true], [3, 2, true], [4, 2, true], [5, 2, true]]);
    expect(layout.threads.filter((thread) => thread.site === "local").every((thread) => thread.continues)).toBe(true);
    expect(layout.threadLinks).toHaveLength(0);
    // The waits that were open at the pause continue on cloud2, on the sub-rows of their threads.
    const carried = layout.waits.filter((wait) => wait.site === "cloud2");
    expect(carried.map((wait) => [wait.thread, wait.cancelled, wait.subRow !== null])).toEqual([[2, false, true], [3, true, true], [4, true, true]]);
  });

  it("does not carry threads over a kill: the snapshot that the run continues from is older", () => {
    const layout = layoutOf(playFixture(buildRecoverFixture()), runKey("local", RECOVER_DURABLE), at(60_000));
    expect(layout.threads).toHaveLength(0);
    expect(layout.segments.filter((bar) => bar.run === RECOVER_DURABLE).map((bar) => bar.endKind)).toEqual(["killed", "completed"]);
  });
});

describe("program hash and program source", () => {
  it("names the program on the segment bar and the source of the program on the resume marker", () => {
    const layout = layoutOf(playFixture(buildFanoutFixture()), runKey("local", FANOUT_PARENT), at(60_000));
    expect(layout.segments.every((bar) => typeof bar.programHash === "string" && bar.programHash.length === 64)).toBe(true);
    expect(markersOf(layout, "resume")[0]?.stats.program_source).toBe("store");
  });
});
