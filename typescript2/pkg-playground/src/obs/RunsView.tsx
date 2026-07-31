/**
 * Runs tab (design §9.6, delivery stage 2): runs list + run detail rendered
 * from BQF1 frames over the `/api/obs` WebSocket.
 *
 * - Runs list: live `runs` subscription; row click opens the run detail.
 * - Run detail: per-thread timeline lanes (§9.4 aggregate tier), a
 *   Left-Heavy flame canvas (preorder fold rows, synthetic "N smaller"
 *   rows), and a top-functions table joined with `run_meta` fqn names.
 * - Live runs stay fresh through subscriptions (server pushes ≤4 Hz).
 */

import { ArrowLeft } from 'lucide-react';
import type {
  FC,
  MouseEvent as ReactMouseEvent,
  RefObject,
} from 'react';
import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from 'react';

import { cn } from '../lib/utils';
import {
  asLeftHeavy,
  asRunMeta,
  asRunsList,
  asTimeline,
  asTopFunctions,
  FOLD_ROW_FUNCTION,
  FrameKind,
  type LeftHeavyColumns,
  type RunsListColumns,
  type TimelineColumns,
  type TopFunctionsColumns,
} from './bqf1';
import { defaultObsUrl, WsObserveClient } from './observe-client';

// ---------------------------------------------------------------------------
// Shared bits
// ---------------------------------------------------------------------------

/** Selected run survives tab switches (the tab unmounts when hidden). */
let lastSelectedRunKey: string | null = null;

function functionHue(functionId: number): number {
  // Knuth multiplicative hash → stable, well-spread hue per function id.
  return ((functionId + 1) * 2654435761) % 360;
}

function functionColor(functionId: number): string {
  return `hsl(${functionHue(functionId)} 55% 48%)`;
}

const FOLD_COLOR = '#6b6f76';

function formatNs(ns: number): string {
  if (ns >= 1e9) return `${(ns / 1e9).toFixed(2)}s`;
  if (ns >= 1e6) return `${(ns / 1e6).toFixed(1)}ms`;
  if (ns >= 1e3) return `${(ns / 1e3).toFixed(1)}µs`;
  return `${Math.round(ns)}ns`;
}

function formatCreated(ms: number): string {
  if (!ms) return '';
  const d = new Date(ms);
  const today = new Date();
  const sameDay = d.toDateString() === today.toDateString();
  const time = d.toLocaleTimeString(undefined, { hour12: false });
  return sameDay ? time : `${d.toLocaleDateString()} ${time}`;
}

function shortRevision(revision: string): string {
  return revision.length > 10 ? revision.slice(0, 10) : revision;
}

function statusBadgeClass(status: string): string {
  switch (status) {
    case 'running':
      return 'bg-vsc-accent/20 text-vsc-accent';
    case 'succeeded':
    case 'success':
    case 'ok':
    case 'completed':
      return 'bg-vsc-green/20 text-vsc-green';
    case 'crashed':
    case 'failed':
    case 'error':
      return 'bg-vsc-red/20 text-vsc-red';
    case 'cancelled':
      return 'bg-vsc-yellow/20 text-vsc-yellow';
    default:
      return 'bg-vsc-surface text-vsc-text-muted';
  }
}

/** Measured element width (ResizeObserver-driven). */
function useElementWidth<T extends HTMLElement>(): [RefObject<T>, number] {
  const ref = useRef<T>(null);
  const [width, setWidth] = useState(0);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    setWidth(el.clientWidth);
    const observer = new ResizeObserver(() => setWidth(el.clientWidth));
    observer.observe(el);
    return () => observer.disconnect();
  }, []);
  return [ref, width];
}

// ---------------------------------------------------------------------------
// ObsRunsTab — owns the WsObserveClient lifecycle
// ---------------------------------------------------------------------------

export interface ObsRunsTabProps {
  /** Override the `/api/obs` URL (default: mirrors the `/api/ws` derivation). */
  obsUrl?: string;
}

export const ObsRunsTab: FC<ObsRunsTabProps> = ({ obsUrl }) => {
  const [client, setClient] = useState<WsObserveClient | null>(null);
  const [connected, setConnected] = useState(false);

  useEffect(() => {
    const c = new WsObserveClient(obsUrl ? () => obsUrl : defaultObsUrl);
    const offConnection = c.onConnectionChange(setConnected);
    setClient(c);
    return () => {
      offConnection();
      c.dispose();
      setClient(null);
    };
  }, [obsUrl]);

  if (!client) return null;
  return <RunsView client={client} connected={connected} />;
};

// ---------------------------------------------------------------------------
// Runs list
// ---------------------------------------------------------------------------

const RunsView: FC<{ client: WsObserveClient; connected: boolean }> = ({
  client,
  connected,
}) => {
  const [runs, setRuns] = useState<RunsListColumns | null>(null);
  const [selectedRunKey, setSelectedRunKey] = useState<string | null>(
    lastSelectedRunKey,
  );

  const selectRun = useCallback((runKey: string | null) => {
    lastSelectedRunKey = runKey;
    setSelectedRunKey(runKey);
  }, []);

  useEffect(
    () =>
      client.subscribe('runs', {}, (frame) => {
        if (frame.kind !== FrameKind.RunsList) return;
        try {
          setRuns(asRunsList(frame));
        } catch (error) {
          console.warn('Runs tab: bad RunsList frame', error);
        }
      }),
    [client],
  );

  if (selectedRunKey != null) {
    const row = runs ? runs.runKey.indexOf(selectedRunKey) : -1;
    return (
      <RunDetail
        client={client}
        connected={connected}
        onBack={() => selectRun(null)}
        runKey={selectedRunKey}
        status={row >= 0 && runs ? runs.status[row]! : undefined}
        target={row >= 0 && runs ? runs.target[row]! : undefined}
      />
    );
  }

  if (!runs) {
    return (
      <div className="flex-1 flex items-center justify-center text-vsc-text-faint text-xs bg-vsc-bg">
        {connected ? 'Loading runs…' : 'Connecting to observability server…'}
      </div>
    );
  }

  if (runs.runKey.length === 0) {
    return (
      <div className="flex-1 flex items-center justify-center text-vsc-text-faint text-xs bg-vsc-bg">
        No runs yet
      </div>
    );
  }

  return (
    <div className="flex-1 min-h-0 overflow-auto bg-vsc-bg font-vsc-mono text-xs">
      <div className="sticky top-0 z-10 grid grid-cols-[minmax(160px,1fr)_84px_120px_150px_100px] border-b border-vsc-border bg-vsc-surface text-[10px] font-semibold uppercase tracking-wide text-vsc-text-muted">
        <div className="px-2 py-1.5">Target</div>
        <div className="px-2 py-1.5">Status</div>
        <div className="px-2 py-1.5">Source</div>
        <div className="px-2 py-1.5">Created</div>
        <div className="px-2 py-1.5">Revision</div>
      </div>
      {runs.runKey.map((runKey, i) => (
        <button
          className="grid w-full grid-cols-[minmax(160px,1fr)_84px_120px_150px_100px] items-center border-b border-vsc-border-subtle text-left text-[11px] text-vsc-text hover:bg-vsc-surface"
          key={runKey}
          onClick={() => selectRun(runKey)}
          type="button"
        >
          <div className="truncate px-2 py-1.5">{runs.target[i] || runKey}</div>
          <div className="px-2 py-1.5">
            <StatusBadge status={runs.status[i]!} />
          </div>
          <div className="truncate px-2 py-1.5 text-vsc-text-muted">
            {runs.source[i]}
          </div>
          <div className="px-2 py-1.5 text-vsc-text-muted">
            {formatCreated(runs.createdMs[i]!)}
          </div>
          <div className="truncate px-2 py-1.5 text-vsc-text-faint">
            {shortRevision(runs.revision[i]!)}
          </div>
        </button>
      ))}
    </div>
  );
};

const StatusBadge: FC<{ status: string }> = ({ status }) => (
  <span
    className={cn(
      'inline-flex rounded px-1.5 py-0.5 text-[10px] font-semibold',
      statusBadgeClass(status),
    )}
  >
    {status}
  </span>
);

// ---------------------------------------------------------------------------
// Run detail
// ---------------------------------------------------------------------------

const RunDetail: FC<{
  client: WsObserveClient;
  connected: boolean;
  runKey: string;
  target: string | undefined;
  status: string | undefined;
  onBack: () => void;
}> = ({ client, connected, runKey, target, status, onBack }) => {
  const [names, setNames] = useState<Map<number, string>>(new Map());
  const [timeline, setTimeline] = useState<TimelineColumns | null>(null);
  const [leftHeavy, setLeftHeavy] = useState<LeftHeavyColumns | null>(null);
  const [topFunctions, setTopFunctions] = useState<TopFunctionsColumns | null>(
    null,
  );
  const [containerRef, width] = useElementWidth<HTMLDivElement>();
  const pixelWidth = Math.min(8192, Math.max(256, Math.round(width || 1024)));

  // Function dictionary (id ↔ fqn), sent once per run; re-queried after a
  // reconnect in case the first attempt raced the connection.
  useEffect(() => {
    if (!connected) return;
    let cancelled = false;
    client
      .query('run_meta', { run: runKey })
      .then((frame) => {
        if (cancelled) return;
        const meta = asRunMeta(frame);
        const map = new Map<number, string>();
        for (let i = 0; i < meta.functionId.length; i += 1) {
          map.set(meta.functionId[i]!, meta.fqn[i]!);
        }
        setNames(map);
      })
      .catch((error) => {
        console.warn('Runs tab: run_meta failed', error);
      });
    return () => {
      cancelled = true;
    };
  }, [client, connected, runKey]);

  useEffect(
    () =>
      client.subscribe('timeline', { run: runKey, pixelWidth }, (frame) => {
        if (frame.kind !== FrameKind.Timeline) return;
        try {
          setTimeline(asTimeline(frame));
        } catch (error) {
          console.warn('Runs tab: bad Timeline frame', error);
        }
      }),
    [client, runKey, pixelWidth],
  );

  useEffect(
    () =>
      client.subscribe('left_heavy', { run: runKey, pixelWidth }, (frame) => {
        if (frame.kind !== FrameKind.LeftHeavy) return;
        try {
          setLeftHeavy(asLeftHeavy(frame));
        } catch (error) {
          console.warn('Runs tab: bad LeftHeavy frame', error);
        }
      }),
    [client, runKey, pixelWidth],
  );

  useEffect(
    () =>
      client.subscribe('top_functions', { run: runKey, limit: 50 }, (frame) => {
        if (frame.kind !== FrameKind.TopFunctions) return;
        try {
          setTopFunctions(asTopFunctions(frame));
        } catch (error) {
          console.warn('Runs tab: bad TopFunctions frame', error);
        }
      }),
    [client, runKey],
  );

  const fnName = useCallback(
    (id: number): string => names.get(id) ?? `fn#${id}`,
    [names],
  );

  return (
    <div className="flex-1 min-h-0 flex flex-col bg-vsc-bg font-vsc-mono text-xs">
      <div className="shrink-0 flex items-center gap-2 border-b border-vsc-border bg-vsc-surface px-2 py-1.5">
        <button
          className="flex items-center gap-1 rounded border border-vsc-border-subtle px-1.5 py-0.5 text-[11px] text-vsc-text-muted hover:bg-vsc-bg"
          onClick={onBack}
          type="button"
        >
          <ArrowLeft className="h-3 w-3" />
          Runs
        </button>
        <span className="truncate text-[11px] font-semibold text-vsc-accent">
          {target || runKey}
        </span>
        {status && <StatusBadge status={status} />}
        {!connected && (
          <span className="text-[10px] text-vsc-yellow">reconnecting…</span>
        )}
      </div>

      <div className="flex-1 min-h-0 overflow-auto" ref={containerRef}>
        <SectionLabel>Timeline</SectionLabel>
        {timeline && timeline.thread.length > 0 ? (
          <TimelineLanes fnName={fnName} timeline={timeline} width={width} />
        ) : (
          <EmptySection text={timeline ? 'No activity' : 'Loading timeline…'} />
        )}

        <SectionLabel>Left Heavy</SectionLabel>
        {leftHeavy && leftHeavy.depth.length > 0 ? (
          <LeftHeavyFlame fnName={fnName} rows={leftHeavy} width={width} />
        ) : (
          <EmptySection text={leftHeavy ? 'No calls' : 'Loading profile…'} />
        )}

        <SectionLabel>Top functions</SectionLabel>
        {topFunctions && topFunctions.functionId.length > 0 ? (
          <TopFunctionsTable fnName={fnName} rows={topFunctions} />
        ) : (
          <EmptySection
            text={topFunctions ? 'No functions' : 'Loading functions…'}
          />
        )}
      </div>
    </div>
  );
};

const SectionLabel: FC<{ children: string }> = ({ children }) => (
  <div className="px-2 pt-2 pb-1 text-[10px] font-semibold uppercase tracking-wide text-vsc-text-muted">
    {children}
  </div>
);

const EmptySection: FC<{ text: string }> = ({ text }) => (
  <div className="px-2 py-3 text-[11px] text-vsc-text-faint">{text}</div>
);

// ---------------------------------------------------------------------------
// Timeline lanes canvas — one lane per thread; §9.4 aggregate-tier activity
// bands colored by dominant function; red tick when the band saw errors.
// ---------------------------------------------------------------------------

const LANE_HEIGHT = 22;
const LANE_GAP = 4;
const LANE_LABEL_WIDTH = 64;

const TimelineLanes: FC<{
  timeline: TimelineColumns;
  width: number;
  fnName: (id: number) => string;
}> = ({ timeline, width, fnName }) => {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const [hover, setHover] = useState<string | null>(null);

  const lanes = useMemo(() => {
    // Group band rows by raw thread id (BigInt-safe keys).
    const byThread = new Map<string, number[]>();
    for (let i = 0; i < timeline.thread.length; i += 1) {
      const key = timeline.thread[i]!.toString();
      const rows = byThread.get(key);
      if (rows) rows.push(i);
      else byThread.set(key, [i]);
    }
    return [...byThread.entries()];
  }, [timeline]);

  const span = useMemo(() => {
    let min = Number.POSITIVE_INFINITY;
    let max = 0;
    for (let i = 0; i < timeline.firstTsNs.length; i += 1) {
      min = Math.min(min, timeline.firstTsNs[i]!);
      max = Math.max(max, timeline.lastTsNs[i]!);
    }
    return { min, span: Math.max(1, max - min) };
  }, [timeline]);

  const cssWidth = Math.max(64, width - LANE_LABEL_WIDTH);
  const cssHeight = lanes.length * (LANE_HEIGHT + LANE_GAP);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const dpr = window.devicePixelRatio || 1;
    canvas.width = Math.round(cssWidth * dpr);
    canvas.height = Math.round(cssHeight * dpr);
    const ctx = canvas.getContext('2d');
    if (!ctx) return;
    ctx.scale(dpr, dpr);
    ctx.clearRect(0, 0, cssWidth, cssHeight);

    lanes.forEach(([, rows], laneIndex) => {
      const y = laneIndex * (LANE_HEIGHT + LANE_GAP);
      // Lane track
      ctx.fillStyle = 'rgba(128, 128, 128, 0.08)';
      ctx.fillRect(0, y, cssWidth, LANE_HEIGHT);
      for (const row of rows) {
        const first = timeline.firstTsNs[row]!;
        const last = timeline.lastTsNs[row]!;
        const x = ((first - span.min) / span.span) * cssWidth;
        const w = Math.max(1, ((last - first) / span.span) * cssWidth);
        const busy = timeline.busyNs[row]!;
        const awaitNs = timeline.awaitNs[row]!;
        const busyFraction =
          busy + awaitNs > 0 ? busy / (busy + awaitNs) : 1;
        ctx.fillStyle = functionColor(timeline.dominantFunction[row]!);
        ctx.globalAlpha = 0.35 + 0.65 * busyFraction;
        ctx.fillRect(x, y + 2, w, LANE_HEIGHT - 4);
        ctx.globalAlpha = 1;
        if (timeline.errors[row]! > 0) {
          ctx.fillStyle = '#f85149';
          ctx.fillRect(x, y, Math.max(2, Math.min(w, 3)), LANE_HEIGHT);
        }
      }
    });
  }, [lanes, span, timeline, cssWidth, cssHeight]);

  const onMouseMove = useCallback(
    (event: ReactMouseEvent<HTMLCanvasElement>) => {
      const rect = event.currentTarget.getBoundingClientRect();
      const x = event.clientX - rect.left;
      const y = event.clientY - rect.top;
      const laneIndex = Math.floor(y / (LANE_HEIGHT + LANE_GAP));
      const lane = lanes[laneIndex];
      if (!lane) {
        setHover(null);
        return;
      }
      const tsNs = span.min + (x / cssWidth) * span.span;
      for (const row of lane[1]) {
        if (
          tsNs >= timeline.firstTsNs[row]! &&
          tsNs <= timeline.lastTsNs[row]!
        ) {
          setHover(
            `${fnName(timeline.dominantFunction[row]!)} — busy ${formatNs(timeline.busyNs[row]!)}, await ${formatNs(timeline.awaitNs[row]!)}${timeline.errors[row]! > 0 ? `, ${timeline.errors[row]} errors` : ''}`,
          );
          return;
        }
      }
      setHover(null);
    },
    [lanes, span, timeline, cssWidth, fnName],
  );

  return (
    <div className="px-2">
      <div className="flex">
        <div className="shrink-0" style={{ width: LANE_LABEL_WIDTH }}>
          {lanes.map(([thread]) => (
            <div
              className="flex items-center text-[10px] text-vsc-text-faint"
              key={thread}
              style={{ height: LANE_HEIGHT, marginBottom: LANE_GAP }}
            >
              <span className="truncate">t{thread}</span>
            </div>
          ))}
        </div>
        <canvas
          onMouseLeave={() => setHover(null)}
          onMouseMove={onMouseMove}
          ref={canvasRef}
          style={{ width: cssWidth, height: cssHeight }}
        />
      </div>
      <div className="h-4 truncate text-[10px] text-vsc-text-faint">
        {hover ?? ''}
      </div>
    </div>
  );
};

// ---------------------------------------------------------------------------
// Left-Heavy flame canvas — preorder rows (depth, total_ns); children start
// at their parent's left edge; synthetic fold rows render gray as "N smaller".
// ---------------------------------------------------------------------------

const FLAME_ROW_HEIGHT = 18;

interface FlameRect {
  x: number;
  y: number;
  w: number;
  row: number;
}

const LeftHeavyFlame: FC<{
  rows: LeftHeavyColumns;
  width: number;
  fnName: (id: number) => string;
}> = ({ rows, width, fnName }) => {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const rectsRef = useRef<FlameRect[]>([]);
  const [hover, setHover] = useState<string | null>(null);

  const cssWidth = Math.max(64, width - 16);

  const layout = useMemo(() => {
    const n = rows.depth.length;
    let rootTotal = 0;
    let maxDepth = 0;
    for (let i = 0; i < n; i += 1) {
      if (rows.depth[i] === 0) rootTotal += rows.totalNs[i]!;
      maxDepth = Math.max(maxDepth, rows.depth[i]!);
    }
    const scale = rootTotal > 0 ? cssWidth / rootTotal : 0;
    // Preorder cursor walk: a node at depth d is placed at cursor[d]; its
    // children start at its own left edge (cursor[d+1] resets to x).
    const cursors: number[] = [0];
    const rects: FlameRect[] = [];
    for (let i = 0; i < n; i += 1) {
      const depth = rows.depth[i]!;
      const x = cursors[depth] ?? 0;
      const w = rows.totalNs[i]! * scale;
      cursors[depth] = x + w;
      cursors[depth + 1] = x;
      cursors.length = depth + 2; // drop stale deeper cursors
      rects.push({ x, y: depth * FLAME_ROW_HEIGHT, w, row: i });
    }
    return { rects, maxDepth, rootTotal };
  }, [rows, cssWidth]);

  const cssHeight = (layout.maxDepth + 1) * FLAME_ROW_HEIGHT;

  useEffect(() => {
    rectsRef.current = layout.rects;
    const canvas = canvasRef.current;
    if (!canvas) return;
    const dpr = window.devicePixelRatio || 1;
    canvas.width = Math.round(cssWidth * dpr);
    canvas.height = Math.round(cssHeight * dpr);
    const ctx = canvas.getContext('2d');
    if (!ctx) return;
    ctx.scale(dpr, dpr);
    ctx.clearRect(0, 0, cssWidth, cssHeight);
    ctx.font = '10px ui-monospace, monospace';
    ctx.textBaseline = 'middle';

    for (const rect of layout.rects) {
      const row = rect.row;
      const functionId = rows.functionId[row]!;
      const isFold = functionId === FOLD_ROW_FUNCTION;
      const w = Math.max(1, rect.w - 0.5);
      ctx.fillStyle = isFold ? FOLD_COLOR : functionColor(functionId);
      ctx.fillRect(rect.x, rect.y + 1, w, FLAME_ROW_HEIGHT - 2);
      if (rows.errors[row]! > 0 && !isFold) {
        ctx.fillStyle = '#f85149';
        ctx.fillRect(rect.x, rect.y + 1, w, 2);
      }
      if (rect.w > 40) {
        const label = isFold
          ? `${rows.foldedCount[row]} smaller`
          : fnName(functionId);
        ctx.fillStyle = isFold ? '#d0d0d0' : '#f5f8fa';
        ctx.save();
        ctx.beginPath();
        ctx.rect(rect.x + 2, rect.y, rect.w - 4, FLAME_ROW_HEIGHT);
        ctx.clip();
        ctx.fillText(label, rect.x + 4, rect.y + FLAME_ROW_HEIGHT / 2 + 0.5);
        ctx.restore();
      }
    }
  }, [layout, rows, cssWidth, cssHeight, fnName]);

  const onMouseMove = useCallback(
    (event: ReactMouseEvent<HTMLCanvasElement>) => {
      const bounds = event.currentTarget.getBoundingClientRect();
      const x = event.clientX - bounds.left;
      const y = event.clientY - bounds.top;
      for (const rect of rectsRef.current) {
        if (
          x >= rect.x &&
          x <= rect.x + Math.max(1, rect.w) &&
          y >= rect.y &&
          y < rect.y + FLAME_ROW_HEIGHT
        ) {
          const row = rect.row;
          const functionId = rows.functionId[row]!;
          if (functionId === FOLD_ROW_FUNCTION) {
            setHover(
              `${rows.foldedCount[row]} smaller — total ${formatNs(rows.totalNs[row]!)}`,
            );
          } else {
            setHover(
              `${fnName(functionId)} — total ${formatNs(rows.totalNs[row]!)}, self ${formatNs(rows.selfNs[row]!)}, ${rows.enters[row]} calls${rows.errors[row]! > 0 ? `, ${rows.errors[row]} errors` : ''}`,
            );
          }
          return;
        }
      }
      setHover(null);
    },
    [rows, fnName],
  );

  return (
    <div className="px-2">
      <canvas
        onMouseLeave={() => setHover(null)}
        onMouseMove={onMouseMove}
        ref={canvasRef}
        style={{ width: cssWidth, height: cssHeight }}
      />
      <div className="h-4 truncate text-[10px] text-vsc-text-faint">
        {hover ?? ''}
      </div>
    </div>
  );
};

// ---------------------------------------------------------------------------
// Top functions table
// ---------------------------------------------------------------------------

const TopFunctionsTable: FC<{
  rows: TopFunctionsColumns;
  fnName: (id: number) => string;
}> = ({ rows, fnName }) => {
  let maxSelf = 0;
  for (let i = 0; i < rows.selfNs.length; i += 1) {
    maxSelf = Math.max(maxSelf, rows.selfNs[i]!);
  }
  return (
    <div className="px-2 pb-3">
      <div className="grid grid-cols-[minmax(160px,1fr)_70px_90px_90px_60px] border-b border-vsc-border text-[10px] font-semibold uppercase tracking-wide text-vsc-text-muted">
        <div className="px-2 py-1">Function</div>
        <div className="px-2 py-1 text-right">Calls</div>
        <div className="px-2 py-1 text-right">Total</div>
        <div className="px-2 py-1 text-right">Self</div>
        <div className="px-2 py-1 text-right">Errors</div>
      </div>
      {[...rows.functionId].map((functionId, i) => (
        <div
          className="grid grid-cols-[minmax(160px,1fr)_70px_90px_90px_60px] items-center border-b border-vsc-border-subtle text-[11px] text-vsc-text"
          key={`${functionId}-${i}`}
        >
          <div className="flex min-w-0 items-center gap-1.5 px-2 py-1">
            <span
              className="h-2 w-2 shrink-0 rounded-sm"
              style={{ backgroundColor: functionColor(functionId) }}
            />
            <span className="truncate">{fnName(functionId)}</span>
          </div>
          <div className="px-2 py-1 text-right">{rows.calls[i]}</div>
          <div className="px-2 py-1 text-right">
            {formatNs(rows.totalNs[i]!)}
          </div>
          <div className="relative px-2 py-1 text-right">
            <div
              className="absolute inset-y-1 left-0 bg-vsc-text-muted/20"
              style={{
                width: `${maxSelf > 0 ? (rows.selfNs[i]! / maxSelf) * 100 : 0}%`,
              }}
            />
            <span className="relative">{formatNs(rows.selfNs[i]!)}</span>
          </div>
          <div
            className={cn(
              'px-2 py-1 text-right',
              rows.errors[i]! > 0 ? 'text-vsc-red' : 'text-vsc-text-faint',
            )}
          >
            {rows.errors[i]}
          </div>
        </div>
      ))}
    </div>
  );
};
