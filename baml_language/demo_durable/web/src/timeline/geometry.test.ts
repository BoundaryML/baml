import { describe, expect, it } from "vitest";
import { buildCentralFixture, CENTRAL_PARENT } from "../fixtures/central";
import { buildChainFixture, CHAIN_PARENT } from "../fixtures/chain";
import { buildSpawnFixture, SPAWN_PARENT } from "../fixtures/spawn";
import type { Run } from "../protocol";
import { runKey, runTree } from "../state";
import { playFixture } from "../testing";
import { anchorY, axisTicks, connectorRoute, fitView, GUTTER, MARKER_SPACING, placeBarLabels, plotGeometry, spreadMarkers, SUB_H, timeScale } from "./geometry";
import { computeTimelineLayout } from "./layout";

function layoutFor(fixture: ReturnType<typeof buildCentralFixture>, id: string) {
  const state = playFixture(fixture);
  const runs = runTree(state, runKey("local", id)).flatMap((key) => (state.runs[key] ? [state.runs[key] as Run] : []));
  return computeTimelineLayout({ runs, events: state.events, now: 0 });
}

describe("timeline geometry", () => {
  it("maps the fitted view into the plot area and back", () => {
    const layout = layoutFor(buildCentralFixture(), CENTRAL_PARENT);
    const view = fitView(layout);
    const scale = timeScale(view, 900);
    expect(scale.x(layout.t0)).toBeGreaterThan(GUTTER);
    expect(scale.x(layout.t1)).toBeLessThan(900);
    expect(scale.t(scale.x(layout.t0 + 1234))).toBeCloseTo(layout.t0 + 1234, 6);
  });

  it("keeps clustered markers apart and the snapshot at its exact time", () => {
    const layout = layoutFor(buildCentralFixture(), CENTRAL_PARENT);
    const scale = timeScale(fitView(layout), 900);
    const positions = spreadMarkers(layout.markers, scale.x);
    const request = layout.markers.find((marker) => marker.kind === "pause_request");
    const snapshot = layout.markers.find((marker) => marker.kind === "snapshot");
    expect(request && snapshot).toBeTruthy();
    const snapshotX = positions.get(snapshot?.id ?? "") ?? 0;
    const requestX = positions.get(request?.id ?? "") ?? 0;
    expect(snapshotX).toBeCloseTo(scale.x(snapshot?.t ?? 0), 6);
    expect(snapshotX - requestX).toBeGreaterThanOrEqual(MARKER_SPACING);
  });

  it("stacks the cloud lane under the local lane and makes room for thread sub-bars", () => {
    const layout = layoutFor(buildSpawnFixture(), SPAWN_PARENT);
    const plot = plotGeometry(layout.lanes);
    const [local, cloud] = plot.lanes;
    expect(local?.site).toBe("local");
    expect(cloud?.top).toBeGreaterThan((local?.top ?? 0) + (local?.height ?? 0) - 1);
    const call = layout.connectors.find((connector) => connector.kind === "call");
    expect(call).toBeDefined();
    if (!call) return;
    const fromY = anchorY(plot, call.from, "bottom");
    const toY = anchorY(plot, call.to, "top");
    // The call leaves from the bottom of the sub-bar, below the main bar, and goes down to the child.
    expect(fromY).toBe((local?.rows[0]?.barBottom ?? 0) + 3 + SUB_H);
    expect(toY).toBeGreaterThan(fromY);
    expect(plot.height).toBeGreaterThan(plot.axisY);
  });

  it("stacks three lanes in registry order", () => {
    const plot = plotGeometry(layoutFor(buildChainFixture(), CHAIN_PARENT).lanes);
    expect(plot.lanes.map((lane) => lane.site)).toEqual(["local", "cloud", "cloud2"]);
    for (let index = 1; index < plot.lanes.length; index++) {
      const above = plot.lanes[index - 1];
      expect(plot.lanes[index]?.top).toBeGreaterThan((above?.top ?? 0) + (above?.height ?? 0));
    }
    expect(plot.axisY).toBeGreaterThan((plot.lanes[2]?.top ?? 0) + (plot.lanes[2]?.height ?? 0) - 1);
  });

  describe("connectors between lanes that are not adjacent", () => {
    const layout = layoutFor(buildChainFixture(), CHAIN_PARENT);
    const plot = plotGeometry(layout.lanes);
    const scale = timeScale(fitView(layout), 1000);
    const [local, cloud, cloud2] = plot.lanes;
    const back = layout.connectors.find((connector) => connector.kind === "migration" && connector.from.site === "cloud2");
    /** The vertical pieces `Lx,y` of a path, as `[x, y]`. */
    const lineTo = (d: string): number[][] => [...d.matchAll(/L(-?[\d.]+),(-?[\d.]+)/g)].map((match) => [Number(match[1]), Number(match[2])]);

    it("goes from the top of the cloud2 bar up to the bottom of the local bar", () => {
      expect(back).toBeDefined();
      if (!back || !local || !cloud2) return;
      const route = connectorRoute(plot, back, scale.x, layout.segments);
      expect(route.d.startsWith(`M${scale.x(back.from.t)},${cloud2.rows[0]?.barY}`)).toBe(true);
      expect(route.tip).toEqual([scale.x(back.to.t), local.rows[back.to.row]?.barBottom]);
    });

    it("crosses the lane in between as one straight vertical and turns only in the band between lanes", () => {
      if (!back || !local || !cloud || !cloud2) return;
      const route = connectorRoute(plot, back, scale.x, layout.segments);
      expect(route.crossX).toBe(scale.x(back.to.t));
      // The curve ends in the band between cloud2 and cloud, below the cloud lane.
      const curveEnd = /C[^ ]+ [^ ]+ (-?[\d.]+),(-?[\d.]+)/.exec(route.d);
      expect(Number(curveEnd?.[1])).toBe(route.crossX);
      expect(Number(curveEnd?.[2])).toBeGreaterThan(cloud.top + cloud.height);
      expect(Number(curveEnd?.[2])).toBeLessThan(cloud2.top);
      // From there a single vertical line passes the cloud lane and ends under the local bar.
      const [line] = lineTo(route.d);
      expect(lineTo(route.d)).toHaveLength(1);
      expect(line?.[0]).toBe(route.crossX);
      expect(line?.[1]).toBeLessThan(cloud.top);
    });

    it("moves the crossing to the source's x when only the target's x is under a bar of the lane in between", () => {
      if (!local || !cloud || !cloud2) return;
      const connector = {
        from: { site: "local", run: "r-a", t: 1000, row: 0, subRow: null },
        to: { site: "cloud2", run: "r-b", t: 5000, row: 0, subRow: null },
        pending: false,
      };
      const x = (t: number): number => t / 10;
      const clear = connectorRoute(plot, connector, x, []);
      expect(clear.crossX).toBe(500);
      const blockedAtTarget = connectorRoute(plot, connector, x, [{ site: "cloud", start: 4000, end: 6000 }]);
      expect(blockedAtTarget.crossX).toBe(100);
      // The vertical runs at the source's x down to the band above cloud2, and the curve follows.
      expect(blockedAtTarget.d.startsWith(`M100,${local.rows[0]?.barBottom} L100,`)).toBe(true);
      const [line] = lineTo(blockedAtTarget.d);
      expect(line?.[1]).toBeGreaterThan(cloud.top + cloud.height);
      expect(line?.[1]).toBeLessThan(cloud2.top);
      // A bar on the source or target lane is not in the way.
      expect(connectorRoute(plot, connector, x, [{ site: "cloud2", start: 4000, end: 6000 }]).crossX).toBe(500);
      // When both positions are under a bar, the arrow keeps its straight arrival.
      expect(connectorRoute(plot, connector, x, [{ site: "cloud", start: 0, end: 6000 }]).crossX).toBe(500);
    });

    it("keeps the single S-curve between adjacent lanes", () => {
      const call = layout.connectors.find((connector) => connector.kind === "call");
      if (!call) throw new Error("the chain fixture has a call connector");
      const route = connectorRoute(plot, call, scale.x, layout.segments);
      expect(route.crossX).toBeNull();
      expect(lineTo(route.d)).toHaveLength(0);
    });
  });

  it("labels a short segment at the right edge towards the left", () => {
    const layout = layoutFor(buildCentralFixture(), CENTRAL_PARENT);
    const scale = timeScale(fitView(layout), 900);
    const labels = placeBarLabels(layout.segments, scale.x, 900 - 14);
    const [first, second] = layout.segments.filter((bar) => bar.run === CENTRAL_PARENT);
    expect(labels.get(first?.id ?? "")).toMatchObject({ anchor: "start", text: `${CENTRAL_PARENT} · pid 41201 · seg 1` });
    expect(labels.get(second?.id ?? "")).toMatchObject({ anchor: "end", text: `${CENTRAL_PARENT} · pid 41377 · seg 2` });
  });

  it("chooses round tick steps", () => {
    const { step, ticks } = axisTicks({ start: 1000, end: 11_200 }, 1000);
    expect(step).toBe(2000);
    expect(ticks).toEqual([1000, 3000, 5000, 7000, 9000, 11_000]);
  });
});
