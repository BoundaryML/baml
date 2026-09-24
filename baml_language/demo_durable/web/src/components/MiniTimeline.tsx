import { useMemo } from "react";
import { loadFixture, type FixtureName } from "../fixtures";
import { DEFAULT_SITES, type Run } from "../protocol";
import { runKey, runTree } from "../state";
import { playFixture } from "../testing";
import { computeTimelineLayout } from "../timeline/layout";

const WIDTH = 236;
const LANE_H = 15;
const BAR_H = 3.2;

/**
 * A small picture of what a scenario looks like on the timeline, drawn from
 * the recording of the scenario: one band per site, a bar per process, a soft
 * band for a sleep, a dashed line for a pause, and a faded bar for a cancelled
 * process.
 */
export function MiniTimeline({ fixture }: { fixture: FixtureName }) {
  const layout = useMemo(() => {
    const recording = loadFixture(fixture);
    const state = playFixture(recording);
    const roots = Object.values(recording.roles ?? {});
    const keys = new Set(roots.flatMap((root) => runTree(state, runKey(root.site, root.id))));
    const runs = [...keys].flatMap((key) => (state.runs[key] ? [state.runs[key] as Run] : []));
    const last = recording.events[recording.events.length - 1]?.ts ?? 0;
    return computeTimelineLayout({ sites: DEFAULT_SITES.map((site) => site.name), runs, events: state.events, now: last });
  }, [fixture]);

  const span = Math.max(layout.t1 - layout.t0, 1);
  const x = (t: number): number => 38 + ((t - layout.t0) / span) * (WIDTH - 44);
  const laneTop = (site: string): number => Math.max(layout.lanes.findIndex((lane) => lane.site === site), 0) * LANE_H;
  const y = (site: string, row: number): number => laneTop(site) + 2.5 + Math.min(row, 2) * (BAR_H + 1);
  const height = layout.lanes.length * LANE_H;

  return (
    <svg className="mini-tl" width={WIDTH} height={height} viewBox={`0 0 ${WIDTH} ${height}`} role="img" aria-label="Shape of the scenario on the timeline">
      {layout.lanes.map((lane, index) => (
        <g key={lane.site} data-site={lane.site}>
          <rect className="mini-lane" x={0} y={index * LANE_H + 0.5} width={WIDTH} height={LANE_H - 1} rx={2} />
          <text className="mini-label" x={4} y={index * LANE_H + 10}>{lane.site}</text>
        </g>
      ))}
      {layout.gaps.map((gap) =>
        gap.kind === "sleeping" ? (
          <rect key={gap.id} className="tl-sleep" data-site={gap.site} x={x(gap.start)} y={y(gap.site, gap.row) - 0.6} width={Math.max(x(gap.end) - x(gap.start), 1)} height={BAR_H + 1.2} rx={2} />
        ) : (
          <line key={gap.id} className="tl-gap mini-gap" data-site={gap.site} x1={x(gap.start)} x2={x(gap.end)} y1={y(gap.site, gap.row) + BAR_H / 2} y2={y(gap.site, gap.row) + BAR_H / 2} />
        ),
      )}
      {layout.segments.map((bar) => (
        <rect key={bar.id} className="tl-bar" data-site={bar.site} opacity={bar.endKind === "cancelled" || bar.endKind === "lost" ? 0.4 : 1}
          x={x(bar.start)} y={y(bar.site, bar.row)} width={Math.max(x(bar.end) - x(bar.start), 1.5)} height={BAR_H} rx={1} />
      ))}
      {layout.connectors
        .filter((connector) => connector.kind === "cancel" || connector.kind === "migration")
        .map((connector) => (
          <line key={connector.id} className={connector.kind === "cancel" ? "mini-cancel" : "mini-move"}
            x1={x(connector.from.t)} y1={y(connector.from.site, connector.from.row) + BAR_H / 2}
            x2={x(connector.to.t)} y2={y(connector.to.site, connector.to.row) + BAR_H / 2} />
        ))}
    </svg>
  );
}
