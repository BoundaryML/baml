import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { clock } from "../clock";
import { formatBytes, formatClock, formatOffset } from "../format";
import { isTickingStatus, sitePort, type Run, type Site, type SiteEntry } from "../protocol";
import type { AppState, RunKey } from "../state";
import { runKey } from "../state";
import { TimelineDetail, type DetailTarget } from "./Detail";
import {
  axisTicks,
  BAR_H,
  connectorRoute,
  fitView,
  GUTTER,
  LABEL_CHAR_W,
  placeBarLabels,
  plotGeometry,
  RIGHT_PAD,
  sleepGapLabel,
  spreadMarkers,
  SUB_H,
  subRowY,
  timeScale,
  type View,
} from "./geometry";
import { computeTimelineLayout, type SegmentBar, type TimelineLayout } from "./layout";
import { LegendMarker, MarkerGlyph } from "./MarkerGlyph";

export interface SnapshotRef {
  site: Site;
  run: string;
  n: number;
}

interface Props {
  /** The site registry. One lane per site, in registry order. */
  sites: readonly SiteEntry[];
  runs: readonly Run[];
  events: AppState["events"];
  selectedRun: RunKey | null;
  selectedSnapshot: SnapshotRef | null;
  onSelectSnapshot(snapshot: SnapshotRef | null): void;
}

type TargetRef = { kind: DetailTarget["kind"] | "threadlink"; id: string };

const END_GLYPHS: Partial<Record<SegmentBar["endKind"], string>> = {
  completed: "✓",
  failed: "✕",
  lost: "✕",
  killed: "✕",
  cancelled: "⊘",
};

function useWidth(): [React.RefObject<HTMLDivElement | null>, number] {
  const ref = useRef<HTMLDivElement | null>(null);
  const [width, setWidth] = useState(800);
  useLayoutEffect(() => {
    const element = ref.current;
    if (!element) return;
    setWidth(element.clientWidth);
    const observer = new ResizeObserver(() => setWidth(element.clientWidth));
    observer.observe(element);
    return () => observer.disconnect();
  }, []);
  return [ref, width];
}

/** The current time, refreshed ten times per second while a process of the tree exists. */
function useNow(live: boolean): number {
  const [now, setNow] = useState(() => clock.now());
  useEffect(() => {
    setNow(clock.now());
    if (!live) return;
    const id = setInterval(() => setNow(clock.now()), 100);
    return () => clearInterval(id);
  }, [live]);
  return now;
}

function resolveTarget(layout: TimelineLayout, ref: TargetRef | null): DetailTarget | null {
  if (ref === null) return null;
  const find = <T extends { id: string }>(items: readonly T[]): T | undefined => items.find((item) => item.id === ref.id);
  switch (ref.kind) {
    case "marker": {
      const item = find(layout.markers);
      return item ? { kind: "marker", item } : null;
    }
    case "segment": {
      const item = find(layout.segments);
      return item ? { kind: "segment", item } : null;
    }
    case "gap": {
      const item = find(layout.gaps);
      return item ? { kind: "gap", item } : null;
    }
    case "thread": {
      const item = find(layout.threads);
      return item ? { kind: "thread", item } : null;
    }
    case "threadlink": {
      // The line between two bars of one thread shows the thread that continues.
      const link = find(layout.threadLinks);
      const item = link && layout.threads.find((thread) => thread.site === link.site && thread.run === link.run && thread.thread === link.thread && thread.continues);
      return item ? { kind: "thread", item } : null;
    }
    case "wait": {
      const item = find(layout.waits);
      return item ? { kind: "wait", item } : null;
    }
    case "connector": {
      const item = find(layout.connectors);
      return item ? { kind: "connector", item } : null;
    }
  }
}

function fitLabel(candidates: string[], available: number): string {
  return candidates.find((candidate) => candidate.length * LABEL_CHAR_W <= available) ?? "";
}

export function Timeline({ sites, runs, events, selectedRun, selectedSnapshot, onSelectSnapshot }: Props) {
  const [plotRef, width] = useWidth();
  // A sleeping run has no process, but its countdown moves with the clock.
  const live = runs.some((run) => isTickingStatus(run.status));
  const tick = useNow(live);
  const siteNames = useMemo(() => sites.map((site) => site.name), [sites]);
  const layout = useMemo(
    () => computeTimelineLayout({ sites: siteNames, runs, events, now: live ? Math.max(tick, clock.now()) : clock.now() }),
    [siteNames, runs, events, live, tick],
  );

  const [follow, setFollow] = useState(true);
  const [frozen, setFrozen] = useState<View | null>(null);
  const [hover, setHover] = useState<TargetRef | null>(null);
  const [pinned, setPinned] = useState<TargetRef | null>(null);
  const [cursorX, setCursorX] = useState<number | null>(null);

  const auto = useMemo(() => fitView(layout), [layout]);
  const view = follow || frozen === null ? auto : frozen;
  const scale = useMemo(() => timeScale(view, width), [view, width]);
  const plot = useMemo(() => plotGeometry(layout.lanes), [layout.lanes]);
  const markerX = useMemo(() => spreadMarkers(layout.markers, scale.x), [layout.markers, scale]);
  const barLabels = useMemo(
    () => placeBarLabels(layout.segments, scale.x, width - RIGHT_PAD),
    [layout.segments, scale, width],
  );
  const { step, ticks } = useMemo(() => axisTicks(view, layout.t0), [view, layout.t0]);

  // Keep the pinned snapshot marker in step with the snapshot that the state tree shows.
  useEffect(() => {
    if (selectedSnapshot === null) {
      setPinned((current) => {
        if (current?.kind !== "marker") return current;
        const marker = layout.markers.find((candidate) => candidate.id === current.id);
        return marker?.kind === "snapshot" ? null : current;
      });
    }
    // Only a change of the selection matters here, not every new layout.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedSnapshot]);

  const changeView = useCallback((next: View): void => {
    setFollow(false);
    setFrozen(next);
  }, []);

  const zoom = useCallback(
    (factor: number, centerT?: number): void => {
      const center = centerT ?? (view.start + view.end) / 2;
      const span = Math.max((view.end - view.start) * factor, 50);
      const ratio = (center - view.start) / Math.max(view.end - view.start, 1);
      changeView({ start: center - span * ratio, end: center + span * (1 - ratio) });
    },
    [view, changeView],
  );

  // Wheel: ctrl or cmd plus wheel zooms around the cursor, a horizontal wheel pans.
  useEffect(() => {
    const element = plotRef.current;
    if (!element) return;
    const onWheel = (event: WheelEvent): void => {
      const rect = element.getBoundingClientRect();
      if (event.ctrlKey || event.metaKey) {
        event.preventDefault();
        zoom(Math.exp(event.deltaY * 0.004), scale.t(event.clientX - rect.left));
      } else if (Math.abs(event.deltaX) > Math.abs(event.deltaY)) {
        event.preventDefault();
        const dt = scale.t(event.deltaX) - scale.t(0);
        changeView({ start: view.start + dt, end: view.end + dt });
      }
    };
    element.addEventListener("wheel", onWheel, { passive: false });
    return () => element.removeEventListener("wheel", onWheel);
  }, [plotRef, scale, view, zoom, changeView]);

  const drag = useRef<{ x: number; view: View } | null>(null);
  const dragged = useRef(false);
  const onMouseDown = (event: React.MouseEvent): void => {
    if (event.button !== 0) return;
    drag.current = { x: event.clientX, view };
    dragged.current = false;
  };
  const onMouseMove = (event: React.MouseEvent<SVGSVGElement>): void => {
    const rect = event.currentTarget.getBoundingClientRect();
    const x = event.clientX - rect.left;
    setCursorX(x >= GUTTER && x <= width - RIGHT_PAD ? x : null);
    const start = drag.current;
    if (start && Math.abs(event.clientX - start.x) > 3) {
      dragged.current = true;
      const dt = ((start.x - event.clientX) / Math.max(width - GUTTER - RIGHT_PAD, 10)) * (start.view.end - start.view.start);
      changeView({ start: start.view.start + dt, end: start.view.end + dt });
    }
  };
  const endDrag = (): void => {
    drag.current = null;
  };

  const select = (ref: TargetRef) => (event: React.MouseEvent) => {
    event.stopPropagation();
    const same = pinned?.kind === ref.kind && pinned.id === ref.id;
    setPinned(same ? null : ref);
    if (ref.kind === "marker") {
      const marker = layout.markers.find((candidate) => candidate.id === ref.id);
      if (marker?.kind === "snapshot") {
        onSelectSnapshot(same ? null : { site: marker.site, run: marker.run, n: marker.n });
      }
    }
  };
  const hoverProps = (ref: TargetRef) => ({
    onMouseEnter: () => setHover(ref),
    onMouseLeave: () => setHover((current) => (current?.id === ref.id ? null : current)),
    onClick: select(ref),
  });

  const detail = resolveTarget(layout, hover) ?? resolveTarget(layout, pinned);
  const detailPinned = hover === null && pinned !== null;
  const isActive = (kind: TargetRef["kind"], id: string): boolean =>
    (pinned?.kind === kind && pinned.id === id) || (hover?.kind === kind && hover.id === id);

  const rowGeometry = (site: Site, row: number) => plot.lanes.find((lane) => lane.site === site)?.rows[row];
  const plotRight = width - RIGHT_PAD;
  const clipId = "tl-clip";
  const portOf = (site: Site): string | null => sitePort(sites.find((entry) => entry.name === site)?.url ?? "");

  const empty = layout.segments.length === 0;

  return (
    <section className="panel timeline" data-testid="timeline">
      <div className="panel-head">
        <span className="panel-title">Timeline</span>
        <div className="legend" aria-label="Legend">
          {layout.lanes.map((lane) => (
            <span key={lane.site} data-testid="legend-site"><i className="site-dot" data-site={lane.site} /> {lane.site}</span>
          ))}
          <span>
            <svg width={22} height={10} aria-hidden="true">
              <rect className="tl-bar tl-legend-ink" x={0} y={1} width={22} height={8} rx={2} />
              <rect className="tl-wait" x={9} y={1} width={13} height={8} />
            </svg>
            wait
          </span>
          <span>
            <svg width={22} height={10} aria-hidden="true">
              <line className="tl-gap tl-legend-ink" x1={0} x2={22} y1={5} y2={5} />
            </svg>
            no process
          </span>
          <span>
            <svg width={22} height={10} aria-hidden="true">
              <rect className="tl-sleep tl-legend-ink" x={0.5} y={1} width={21} height={8} rx={4} />
            </svg>
            sleeping
          </span>
          <span><LegendMarker kind="pause_request" /> pause</span>
          <span><LegendMarker kind="blocked" /> blocked</span>
          <span><LegendMarker kind="snapshot" /> snapshot</span>
          <span><LegendMarker kind="resume" /> resume</span>
          <span><LegendMarker kind="fork" /> fork</span>
          <span><LegendMarker kind="wake" /> wake</span>
          <span><LegendMarker kind="cancel" /> cancel</span>
        </div>
        <span className="spacer" />
        <button className="btn small" onClick={() => zoom(1 / 1.6)} aria-label="Zoom in" title="Zoom in. Ctrl or cmd + wheel zooms around the cursor. Drag or a horizontal wheel pans.">+</button>
        <button className="btn small" onClick={() => zoom(1.6)} aria-label="Zoom out">−</button>
        <button className="btn small" onClick={() => changeView(auto)}>Fit</button>
        <button
          className="btn small toggle"
          aria-pressed={follow}
          data-testid="follow-live"
          onClick={() => {
            setFrozen(view);
            setFollow(!follow);
          }}
        >
          {follow ? "● " : "○ "}Follow live
        </button>
      </div>
      <div className="timeline-body">
        <div className="timeline-plot" ref={plotRef}>
          {empty ? (
            <div className="empty">
              {runs.length === 0
                ? "Select a run. The timeline shows the run, its remote children, and its migrated continuations."
                : "No worker events for this run yet."}
            </div>
          ) : (
            <svg
              width={width}
              height={plot.height}
              role="img"
              aria-label="Timeline of the selected run tree, one lane per site"
              onMouseDown={onMouseDown}
              onMouseMove={onMouseMove}
              onMouseUp={endDrag}
              onMouseLeave={() => {
                endDrag();
                setCursorX(null);
              }}
              onClick={() => {
                if (dragged.current) return;
                setPinned(null);
                onSelectSnapshot(null);
              }}
            >
              <defs>
                <clipPath id={clipId}>
                  <rect x={GUTTER} y={0} width={Math.max(plotRight - GUTTER + RIGHT_PAD - 2, 0)} height={plot.height} />
                </clipPath>
                {/* Diagonal lines in the surface color. On a bar they read as "cancelled". */}
                <pattern id="tl-hatch" width={5} height={5} patternUnits="userSpaceOnUse" patternTransform="rotate(45)">
                  <line className="tl-hatch-line" x1={0} y1={0} x2={0} y2={5} />
                </pattern>
              </defs>

              {plot.lanes.map((lane) => (
                <g key={lane.site} data-testid={`lane-${lane.site}`}>
                  <rect className="tl-lane-bg" x={0} y={lane.top} width={width} height={lane.height} rx={4} />
                  <rect className="tl-bar" data-site={lane.site} x={0} y={lane.top} width={3} height={lane.height} />
                  <text className="tl-lane-label" x={10} y={lane.top + 18}>{lane.site}</text>
                  {portOf(lane.site) !== null && <text className="tl-lane-sub" x={10} y={lane.top + 31}>:{portOf(lane.site)}</text>}
                </g>
              ))}

              <g clipPath={`url(#${clipId})`}>
                {ticks.map((t) => (
                  <g key={t}>
                    <line className="tl-grid" x1={scale.x(t)} x2={scale.x(t)} y1={plot.lanes[0]?.top ?? 0} y2={plot.axisY} />
                    <text className="tl-axis-label" x={scale.x(t) + 3} y={plot.axisY + 13}>
                      {formatOffset(t - layout.t0, step)}
                    </text>
                  </g>
                ))}

                {layout.gaps.map((gap) => {
                  const row = rowGeometry(gap.site, gap.row);
                  if (!row) return null;
                  const x1 = scale.x(gap.start);
                  if (gap.kind === "sleeping") {
                    // A sleep ends at a known time, so the band of an open gap reaches into the future.
                    const x2 = Math.max(scale.x(gap.end), x1 + 2);
                    const label = sleepGapLabel(gap, tick, x2 - x1 - 22);
                    const active = isActive("gap", gap.id);
                    return (
                      <g key={gap.id} data-testid="tl-gap" data-kind="sleeping" data-open={gap.open} data-wake-at={gap.wakeAt ?? undefined}
                        {...hoverProps({ kind: "gap", id: gap.id })}>
                        <rect className="tl-sleep" data-site={gap.site} data-open={gap.open} x={x1} y={row.barY + 1} width={x2 - x1} height={BAR_H - 2}
                          rx={(BAR_H - 2) / 2} strokeWidth={active ? 2 : undefined} />
                        {x2 - x1 > 18 && <text className="tl-sleep-glyph" data-site={gap.site} x={x1 + 5} y={row.barMid + 3.5}>☾</text>}
                        {label && (
                          <text className="tl-sleep-label" data-testid="tl-sleep-label" x={x1 + 17} y={row.barMid + 3.5}>{label}</text>
                        )}
                        {gap.open && gap.wakeAt !== null && (
                          <line className="tl-sleep-wake" data-site={gap.site} x1={scale.x(gap.wakeAt)} x2={scale.x(gap.wakeAt)} y1={row.barY - 3} y2={row.barBottom + 3} />
                        )}
                        <rect className="tl-gap-hit" x={x1} y={row.barY} width={Math.max(x2 - x1, 0)} height={BAR_H} fill="transparent" />
                      </g>
                    );
                  }
                  const x2 = gap.open ? plotRight : scale.x(gap.end);
                  return (
                    <g key={gap.id} data-testid="tl-gap" data-kind={gap.kind} {...hoverProps({ kind: "gap", id: gap.id })}>
                      <line className="tl-gap" data-site={gap.site} x1={x1} x2={x2} y1={row.barMid} y2={row.barMid}
                        strokeWidth={isActive("gap", gap.id) ? 2.5 : undefined} />
                      <rect className="tl-gap-hit" x={x1} y={row.barY} width={Math.max(x2 - x1, 0)} height={BAR_H + 12} fill="transparent" />
                    </g>
                  );
                })}

                {layout.segments.map((bar) => {
                  const row = rowGeometry(bar.site, bar.row);
                  if (!row) return null;
                  const x1 = scale.x(bar.start);
                  const x2 = Math.max(scale.x(bar.end), x1 + 2);
                  const pid = `pid ${bar.pid ?? "?"}`;
                  const label = barLabels.get(bar.id);
                  const selected = selectedRun === runKey(bar.site, bar.run);
                  const glyph = END_GLYPHS[bar.endKind];
                  return (
                    <g key={bar.id} data-testid="tl-segment" data-run={bar.run} data-segment={bar.segment}
                      data-end-kind={bar.endKind} data-end={bar.endKind === "open" ? undefined : bar.end}>
                      <rect className="tl-bar" data-site={bar.site} data-end-kind={bar.endKind} x={x1} y={row.barY} width={x2 - x1} height={BAR_H} rx={3}
                        {...hoverProps({ kind: "segment", id: bar.id })}>
                        <title>{`${bar.site}/${bar.run} segment ${bar.segment}, ${pid}`}</title>
                      </rect>
                      {bar.endKind === "cancelled" && (
                        <rect className="tl-cancelled-hatch" data-testid="tl-cancelled-hatch" x={x1} y={row.barY} width={x2 - x1} height={BAR_H} rx={3} fill="url(#tl-hatch)" />
                      )}
                      {isActive("segment", bar.id) && (
                        <rect className="tl-outline" x={x1 - 1.5} y={row.barY - 1.5} width={x2 - x1 + 3} height={BAR_H + 3} rx={4} />
                      )}
                      {label && label.text !== "" && (
                        <text className={`tl-label${selected ? " strong" : ""}`} x={label.x} y={row.barY - 3} textAnchor={label.anchor}>
                          {label.text}
                        </text>
                      )}
                      {glyph && (
                        <text className="tl-endcap" data-kind={bar.endKind} x={x2 + 3} y={row.barMid + 3.5}>
                          {glyph}
                          <title>{bar.endKind}</title>
                        </text>
                      )}
                    </g>
                  );
                })}

                {layout.threadLinks.map((link) => {
                  const row = rowGeometry(link.site, link.row);
                  if (!row) return null;
                  const y = subRowY(row, link.subRow) + SUB_H / 2;
                  return (
                    <g key={link.id} data-testid="tl-thread-link" data-thread={link.thread} {...hoverProps({ kind: "threadlink", id: link.id })}>
                      <line className="tl-thread-link" data-site={link.site} x1={scale.x(link.start)} x2={scale.x(link.end)} y1={y} y2={y} />
                      <line className="tl-connector-hit" x1={scale.x(link.start)} x2={scale.x(link.end)} y1={y} y2={y}>
                        <title>{`thread ${link.thread} is in the snapshot and continues in the next segment`}</title>
                      </line>
                    </g>
                  );
                })}

                {layout.threads.map((thread) => {
                  const row = rowGeometry(thread.site, thread.row);
                  if (!row) return null;
                  const y = subRowY(row, thread.subRow);
                  const x1 = scale.x(thread.start);
                  const x2 = Math.max(scale.x(thread.end), x1 + 2);
                  // A branch joins the thread to the run's bar where the thread starts and where it ends,
                  // not where a suspension cuts it.
                  const branch = [
                    thread.continued ? "" : `M${x1 + 0.75},${row.barBottom} V${y + SUB_H / 2}`,
                    thread.continues ? "" : `M${x2 - 0.75},${y + SUB_H / 2} V${row.barBottom}`,
                  ].join(" ").trim();
                  return (
                    <g key={thread.id} data-testid="tl-thread" data-thread={thread.thread} data-segment={thread.segment} data-sub-row={thread.subRow}
                      data-continued={thread.continued || undefined} data-continues={thread.continues || undefined}>
                      {branch !== "" && <path className="tl-branch" data-site={thread.site} d={branch} />}
                      <rect className="tl-thread" data-site={thread.site} x={x1} y={y} width={x2 - x1} height={SUB_H} rx={2}
                        {...hoverProps({ kind: "thread", id: thread.id })}>
                        <title>{`spawned thread ${thread.thread}`}</title>
                      </rect>
                      {isActive("thread", thread.id) && (
                        <rect className="tl-outline" x={x1 - 1.5} y={y - 1.5} width={x2 - x1 + 3} height={SUB_H + 3} rx={3} />
                      )}
                    </g>
                  );
                })}

                {/* The labels of the pauses come after the thread lines, which pass under them. */}
                {layout.gaps.map((gap) => {
                  const row = rowGeometry(gap.site, gap.row);
                  if (!row || gap.kind === "sleeping" || gap.bytes === null) return null;
                  const x1 = scale.x(gap.start);
                  const x2 = gap.open ? plotRight : scale.x(gap.end);
                  const label = fitLabel(
                    [
                      `no process · snapshot${gap.snapshotN === null ? "" : ` #${gap.snapshotN}`} ${formatBytes(gap.bytes)}`,
                      `snapshot ${formatBytes(gap.bytes)}`,
                      formatBytes(gap.bytes),
                    ],
                    x2 - x1 - 8,
                  );
                  return label ? <text key={`${gap.id}:label`} className="tl-gap-label" x={(x1 + x2) / 2} y={row.barMid + 14} textAnchor="middle">{label}</text> : null;
                })}

                {layout.waits.map((wait) => {
                  const row = rowGeometry(wait.site, wait.row);
                  if (!row) return null;
                  const x1 = scale.x(wait.start);
                  const x2 = scale.x(wait.end);
                  if (x2 - x1 < 0.5) return null;
                  const y = wait.subRow === null ? row.barY : subRowY(row, wait.subRow);
                  const height = wait.subRow === null ? BAR_H : SUB_H;
                  return (
                    <rect key={wait.id} className="tl-wait" data-testid="tl-wait" data-cancelled={wait.cancelled || undefined} x={x1} y={y} width={x2 - x1} height={height}
                      {...hoverProps({ kind: "wait", id: wait.id })}>
                      <title>{`waits on ${wait.callId}`}</title>
                    </rect>
                  );
                })}

                {layout.connectors.map((connector) => {
                  const { d, arrow } = connectorRoute(plot, connector, scale.x, layout.segments);
                  const active = isActive("connector", connector.id);
                  return (
                    <g key={connector.id} data-testid="tl-connector" data-kind={connector.kind}
                      data-from-site={connector.from.site} data-to-site={connector.to.site}>
                      <path className="tl-connector" data-kind={connector.kind} data-pending={connector.pending}
                        data-ok={connector.ok === false ? "false" : undefined} d={d} strokeWidth={active ? 2.5 : undefined} />
                      <path className={`tl-arrow${connector.kind === "migration" || connector.kind === "fork" ? " migration" : ""}`} data-kind={connector.kind} d={arrow} />
                      <path className="tl-connector-hit" d={d} {...hoverProps({ kind: "connector", id: connector.id })}>
                        <title>{connector.label}</title>
                      </path>
                    </g>
                  );
                })}

                {layout.markers.map((marker) => {
                  const row = rowGeometry(marker.site, marker.row);
                  if (!row) return null;
                  const x = markerX.get(marker.id) ?? scale.x(marker.t);
                  const selected =
                    (pinned?.kind === "marker" && pinned.id === marker.id) ||
                    (marker.kind === "snapshot" &&
                      selectedSnapshot !== null &&
                      selectedSnapshot.site === marker.site &&
                      selectedSnapshot.run === marker.run &&
                      selectedSnapshot.n === marker.n);
                  return (
                    <g key={marker.id} className="tl-marker" data-testid="tl-marker" data-kind={marker.kind} data-selected={selected}
                      data-automatic={marker.kind === "snapshot" && marker.automatic ? "true" : undefined}
                      transform={`translate(${x},${row.barMid}) scale(${isActive("marker", marker.id) || selected ? 1.25 : 1})`}
                      {...hoverProps({ kind: "marker", id: marker.id })}>
                      <circle className="hit" r={9} />
                      <MarkerGlyph kind={marker.kind} />
                    </g>
                  );
                })}

                {layout.live && <line className="tl-now" x1={scale.x(tick)} x2={scale.x(tick)} y1={plot.lanes[0]?.top ?? 0} y2={plot.axisY} />}
                {cursorX !== null && (
                  <g>
                    <line className="tl-cursor" x1={cursorX} x2={cursorX} y1={10} y2={plot.axisY} />
                    <text className="tl-axis-label" x={Math.min(cursorX + 4, plotRight - 150)} y={9}>
                      {formatOffset(scale.t(cursorX) - layout.t0, 1)} · {formatClock(scale.t(cursorX))}
                    </text>
                  </g>
                )}
              </g>
            </svg>
          )}
        </div>
        <aside className="timeline-detail" aria-live="polite">
          <TimelineDetail target={detail} pinned={detailPinned} />
        </aside>
      </div>
    </section>
  );
}
