/**
 * Pixel geometry of the timeline: lane and row positions, the time scale, and
 * axis ticks. Pure functions, shared by the component and its tests.
 */

import { formatBytes, formatClockShort, formatCountdown, formatMs } from "../format";
import type { Site } from "../protocol";
import type { Anchor, Connector, Gap, Lane, Marker, SegmentBar, TimelineLayout } from "./layout";

export const GUTTER = 62;
export const RIGHT_PAD = 14;
export const LABEL_H = 13;
export const BAR_H = 14;
export const SUB_H = 7;
export const SUB_GAP = 3;
export const ROW_GAP = 12;
export const LANE_PAD = 7;
export const LANE_GAP = 6;
export const AXIS_H = 20;
export const TOP_PAD = 14;
export const MARKER_SPACING = 12;

export interface View {
  start: number;
  end: number;
}

export interface RowGeometry {
  top: number;
  barY: number;
  barMid: number;
  barBottom: number;
  height: number;
}

export interface LaneGeometry {
  site: Site;
  top: number;
  height: number;
  rows: RowGeometry[];
}

export interface PlotGeometry {
  lanes: LaneGeometry[];
  axisY: number;
  height: number;
}

export function rowHeight(subRows: number): number {
  return LABEL_H + BAR_H + subRows * (SUB_H + SUB_GAP) + ROW_GAP;
}

export function plotGeometry(lanes: readonly Lane[]): PlotGeometry {
  let y = TOP_PAD;
  const result: LaneGeometry[] = [];
  for (const lane of lanes) {
    const top = y;
    let rowTop = top + LANE_PAD;
    const rows: RowGeometry[] = lane.rows.map((row) => {
      const height = rowHeight(row.subRows);
      const barY = rowTop + LABEL_H;
      const geometry = { top: rowTop, barY, barMid: barY + BAR_H / 2, barBottom: barY + BAR_H, height };
      rowTop += height;
      return geometry;
    });
    const height = rowTop - top + LANE_PAD - ROW_GAP + 4;
    result.push({ site: lane.site, top, height, rows });
    y = top + height + LANE_GAP;
  }
  const axisY = y - LANE_GAP + 2;
  return { lanes: result, axisY, height: axisY + AXIS_H };
}

export function subRowY(row: RowGeometry, subRow: number): number {
  return row.barBottom + SUB_GAP + subRow * (SUB_H + SUB_GAP);
}

/** The view that shows the whole layout, with a margin on both sides. */
export function fitView(layout: Pick<TimelineLayout, "t0" | "t1" | "gaps">): View {
  const span = Math.max(layout.t1 - layout.t0, 1000);
  const hasOpenGap = layout.gaps.some((gap) => gap.open);
  return { start: layout.t0 - span * 0.02, end: layout.t0 + span * (hasOpenGap ? 1.1 : 1.04) };
}

export function timeScale(view: View, width: number): { x(t: number): number; t(x: number): number } {
  const plotWidth = Math.max(width - GUTTER - RIGHT_PAD, 10);
  const span = Math.max(view.end - view.start, 1);
  return {
    x: (t) => GUTTER + ((t - view.start) / span) * plotWidth,
    t: (x) => view.start + ((x - GUTTER) / plotWidth) * span,
  };
}

const TICK_STEPS = [1, 2, 5, 10, 20, 50, 100, 200, 500, 1000, 2000, 5000, 10_000, 15_000, 30_000, 60_000, 120_000, 300_000, 600_000];

/** Tick times at a round step, measured from `origin`, about `target` ticks across the view. */
export function axisTicks(view: View, origin: number, target = 8): { step: number; ticks: number[] } {
  const span = Math.max(view.end - view.start, 1);
  const step = TICK_STEPS.find((candidate) => span / candidate <= target) ?? 600_000;
  const first = Math.ceil((view.start - origin) / step) * step + origin;
  const ticks: number[] = [];
  for (let t = first; t <= view.end; t += step) ticks.push(t);
  return { step, ticks };
}

/**
 * Horizontal positions of markers. Markers of one row that would overlap are
 * spread to the left, so the last marker of a cluster keeps its exact time.
 * A snapshot ends its segment, and that position matters most.
 */
export function spreadMarkers(markers: readonly Marker[], x: (t: number) => number): Map<string, number> {
  const positions = new Map<string, number>();
  const groups = new Map<string, Marker[]>();
  for (const marker of markers) {
    const key = `${marker.site}:${marker.row}`;
    const group = groups.get(key) ?? [];
    group.push(marker);
    groups.set(key, group);
  }
  for (const group of groups.values()) {
    const sorted = [...group].sort((a, b) => a.t - b.t);
    let limit = Number.POSITIVE_INFINITY;
    for (let i = sorted.length - 1; i >= 0; i--) {
      const marker = sorted[i] as Marker;
      const position = Math.min(x(marker.t), limit);
      positions.set(marker.id, position);
      limit = position - MARKER_SPACING;
    }
  }
  return positions;
}

/** The y coordinate at which a connector touches the element that `anchor` names. */
export function anchorY(plot: PlotGeometry, anchor: Anchor, edge: "top" | "bottom" | "mid"): number {
  const lane = plot.lanes.find((candidate) => candidate.site === anchor.site);
  const row = lane?.rows[anchor.row];
  if (!row) return 0;
  if (anchor.subRow === null) {
    return edge === "top" ? row.barY : edge === "bottom" ? row.barBottom : row.barMid;
  }
  const y = subRowY(row, anchor.subRow);
  return edge === "top" ? y : edge === "bottom" ? y + SUB_H : y + SUB_H / 2;
}

export interface ConnectorRoute {
  /** SVG path data of the line. It ends 5 px before the tip, where the arrow head starts. */
  d: string;
  /** SVG path data of the arrow head. */
  arrow: string;
  tip: [number, number];
  /**
   * For a connector between lanes that are not adjacent: the x coordinate at
   * which the line crosses the lanes in between. `null` for every other connector.
   */
  crossX: number | null;
}

/** Horizontal room that a crossing line keeps from the ends of a bar. */
const CROSS_MARGIN = 2;

/**
 * The line of a connector between two anchors.
 *
 * Between adjacent lanes, and inside one lane, the line is one S-curve. Between
 * lanes that are not adjacent, the line crosses every lane in between as a
 * straight vertical, which covers the least of the bars there, and it travels
 * horizontally only in the free band between two lanes. The vertical is at the
 * target's x, so the arrow arrives straight. When a bar of a lane in between
 * is under that x and none is under the source's x, the vertical moves to the
 * source's x and the horizontal travel moves to the band next to the target.
 */
export function connectorRoute(
  plot: PlotGeometry,
  connector: Pick<Connector, "from" | "to" | "pending">,
  x: (t: number) => number,
  bars: readonly Pick<SegmentBar, "site" | "start" | "end">[],
): ConnectorRoute {
  const laneIndex = (site: Site): number => plot.lanes.findIndex((lane) => lane.site === site);
  const fromLane = laneIndex(connector.from.site);
  const toLane = laneIndex(connector.to.site);
  const sameLane = fromLane === toLane;
  const down = toLane > fromLane;
  const x1 = x(connector.from.t);
  const x2 = x(connector.to.t);
  const y1 = anchorY(plot, connector.from, sameLane ? "mid" : down ? "bottom" : "top");
  const y2 = connector.pending
    ? anchorY(plot, connector.to, "mid")
    : anchorY(plot, connector.to, sameLane ? "mid" : down ? "top" : "bottom");
  const direction = y2 >= y1 ? 1 : -1;
  const lineEnd = y2 - direction * 5;
  const arrow = `M${x2 - 3.5},${y2 - direction * 6.5} L${x2 + 3.5},${y2 - direction * 6.5} L${x2},${y2} Z`;
  const tip: [number, number] = [x2, y2];

  if (fromLane < 0 || toLane < 0 || Math.abs(toLane - fromLane) <= 1) {
    const ym = (y1 + y2) / 2;
    return { d: `M${x1},${y1} C${x1},${ym} ${x2},${ym} ${x2},${lineEnd}`, arrow, tip, crossX: null };
  }

  const step = down ? 1 : -1;
  /** The middle of the free band between two adjacent lanes. */
  const bandY = (a: number, b: number): number => {
    const upper = plot.lanes[Math.min(a, b)] as LaneGeometry;
    const lower = plot.lanes[Math.max(a, b)] as LaneGeometry;
    return (upper.top + upper.height + lower.top) / 2;
  };
  const between = new Set<Site>();
  for (let index = fromLane + step; index !== toLane; index += step) between.add((plot.lanes[index] as LaneGeometry).site);
  const covered = (at: number): boolean =>
    bars.some((bar) => {
      if (!between.has(bar.site)) return false;
      const left = x(bar.start);
      return at >= left - CROSS_MARGIN && at <= Math.max(x(bar.end), left + 2) + CROSS_MARGIN;
    });

  if (!covered(x2) || covered(x1)) {
    const yNear = bandY(fromLane, fromLane + step);
    const ym = (y1 + yNear) / 2;
    return { d: `M${x1},${y1} C${x1},${ym} ${x2},${ym} ${x2},${yNear} L${x2},${lineEnd}`, arrow, tip, crossX: x2 };
  }
  const yFar = bandY(toLane - step, toLane);
  const ym = (yFar + lineEnd) / 2;
  return { d: `M${x1},${y1} L${x1},${yFar} C${x1},${ym} ${x2},${ym} ${x2},${lineEnd}`, arrow, tip, crossX: x1 };
}

export interface BarLabel {
  text: string;
  x: number;
  anchor: "start" | "end";
}

/** Approximate advance of one character of the 10.5px monospace label font. */
export const LABEL_CHAR_W = 6.35;

/**
 * Places the label of every segment bar above the bar. A label starts at the
 * start of its bar and may extend over the gap that follows. When that room is
 * too small, for example for a short segment at the right edge, the label ends
 * at the end of the bar and extends to the left instead. The longest variant
 * that fits is used: run id, pid and segment, then run id and pid, then pid.
 */
export function placeBarLabels(
  segments: readonly SegmentBar[],
  x: (t: number) => number,
  plotRight: number,
): Map<string, BarLabel> {
  const labels = new Map<string, BarLabel>();
  const rows = new Map<string, SegmentBar[]>();
  for (const bar of segments) {
    const key = `${bar.site}:${bar.row}`;
    const row = rows.get(key) ?? [];
    row.push(bar);
    rows.set(key, row);
  }
  const fit = (candidates: string[], available: number): string =>
    candidates.find((candidate) => candidate.length * LABEL_CHAR_W <= available) ?? "";
  for (const row of rows.values()) {
    row.sort((a, b) => a.start - b.start);
    let previousEnd = GUTTER;
    row.forEach((bar, index) => {
      const x1 = x(bar.start);
      const x2 = Math.max(x(bar.end), x1 + 2);
      const next = row[index + 1];
      const nextX = next ? x(next.start) : plotRight;
      const pid = `pid ${bar.pid ?? "?"}`;
      const candidates = [`${bar.run} · ${pid} · seg ${bar.segment}`, `${bar.run} · ${pid}`, pid];
      const right = fit(candidates, nextX - x1 - 6);
      const left = fit(candidates, Math.min(x2, plotRight) - previousEnd - 8);
      if (left.length > right.length) {
        labels.set(bar.id, { text: left, x: Math.min(x2, plotRight), anchor: "end" });
        previousEnd = Math.min(x2, plotRight);
      } else {
        labels.set(bar.id, { text: right, x: x1, anchor: "start" });
        previousEnd = x1 + right.length * LABEL_CHAR_W;
      }
    });
  }
  return labels;
}

/** Approximate advance of one character of the 10px monospace font of a gap label. */
export const GAP_LABEL_CHAR_W = 6.05;

/**
 * The label inside a sleeping gap: the longest variant that fits into
 * `available` pixels, or `""`. While the run sleeps, the label names the wake
 * time and counts down to it. After the wake, it states how long the run had
 * no process.
 */
export function sleepGapLabel(
  gap: Pick<Gap, "open" | "start" | "end" | "wakeAt" | "snapshotN" | "bytes">,
  now: number,
  available: number,
): string {
  const candidates: string[] = [];
  if (gap.open) {
    const countdown = gap.wakeAt === null ? null : formatCountdown(gap.wakeAt - now);
    if (gap.wakeAt !== null && countdown !== null) {
      candidates.push(`sleeping until ${formatClockShort(gap.wakeAt)} · ${countdown}`, `until ${formatClockShort(gap.wakeAt)} · ${countdown}`, countdown);
    } else {
      candidates.push("sleeping · no process", "sleeping");
    }
  } else {
    const slept = formatMs(gap.end - gap.start);
    const snapshot = `snapshot${gap.snapshotN === null ? "" : ` #${gap.snapshotN}`} ${formatBytes(gap.bytes)}`;
    candidates.push(`slept ${slept} · no process · ${snapshot}`, `slept ${slept} · no process`, `slept ${slept}`);
  }
  return candidates.find((candidate) => candidate.length * GAP_LABEL_CHAR_W <= available) ?? "";
}
