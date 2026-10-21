// The session as a trace: one bar per edit, lines added above the axis and
// lines removed below. Time inside a burst is drawn to scale; the long gaps
// between bursts (where agents spend nearly all their time) are compressed
// and labeled with their real length, so bursts stay readable in an hour-long
// session. Click a bar to see the project as it was after that edit.

import { useLayoutEffect, useMemo, useRef, useState } from 'react';
import { BURST_GAP_MS, type Edit } from '../engine/model';
import { useModel, usePlayer, useRuntime, useTicker } from '../runtime';
import { formatClock, formatDuration, splitPath } from './symbols';

const PAD_LEFT = 18;
const PAD_RIGHT = 28;
const AXIS_Y = 50;

function stepFor(gap: number): number {
  if (gap <= BURST_GAP_MS)
    return Math.min(34, Math.max(6, 4 + (gap / 1000) * 2.2));
  return 38 + 18 * Math.log2(gap / BURST_GAP_MS);
}

function barHeight(lines: number, max: number): number {
  return lines <= 0 ? 0 : Math.min(max, 3 + 3.6 * Math.sqrt(lines));
}

export function Timeline() {
  const model = useModel();
  const snap = usePlayer();
  const { player, source } = useRuntime();
  useTicker(1000);
  const ref = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(800);
  const [hover, setHover] = useState<number | null>(null);

  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const ro = new ResizeObserver(() =>
      setWidth(el.getBoundingClientRect().width),
    );
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const edits = model.edits;
  const now = source.now();
  const lastTime = edits.at(-1)?.time ?? now;
  const tailGap = Math.max(0, now - lastTime);

  const layout = useMemo(() => {
    const raw: number[] = [];
    let x = 0;
    edits.forEach((e, i) => {
      if (i > 0) x += stepFor(e.time - edits[i - 1].time);
      raw.push(x);
    });
    const tail = edits.length
      ? Math.min(160, stepFor(Math.max(1000, tailGap)))
      : 0;
    const natural = x + tail;
    const available = Math.max(100, width - PAD_LEFT - PAD_RIGHT);
    const scale = natural > available ? available / natural : 1;
    return {
      scale,
      tailEnd: PAD_LEFT + natural * scale,
      xs: raw.map((r) => PAD_LEFT + r * scale),
    };
  }, [edits.length, width, Math.round(tailGap / 5000)]);

  const xs = layout.xs;
  const commits = model.git?.commits ?? [];

  /** x for a moment between edits: interpolated by time within its gap. */
  const xAt = (time: number): number => {
    if (!edits.length) return PAD_LEFT;
    const after = edits.findIndex((e) => e.time > time);
    if (after === 0) return PAD_LEFT - 8;
    if (after < 0) {
      const last = edits.at(-1)?.time ?? now;
      const lastX = xs.at(-1) ?? PAD_LEFT;
      const span = Math.max(1, now - last);
      return (
        lastX +
        10 +
        (layout.tailEnd - lastX - 10) * Math.min(1, (time - last) / span)
      );
    }
    const a = edits[after - 1];
    const b = edits[after];
    const f = (time - a.time) / Math.max(1, b.time - a.time);
    return xs[after - 1] + 8 + (xs[after] - xs[after - 1] - 16) * f;
  };

  const commitXs = commits.map((c) => ({ c, x: xAt(c.time) }));
  const runs = model.runs;

  const playheadX = snap.played > 0 ? xs[snap.played - 1] : PAD_LEFT - 8;

  const pick = (clientX: number) => {
    const el = ref.current;
    if (!el || !xs.length) return null;
    const x = clientX - el.getBoundingClientRect().left;
    let best = 0;
    for (let i = 1; i < xs.length; i++)
      if (Math.abs(xs[i] - x) < Math.abs(xs[best] - x)) best = i;
    return Math.abs(xs[best] - x) < 24 ? best : null;
  };

  const sessionStart = model.session?.startedAt ?? lastTime;
  const touched = model.touchedFiles(snap.played);
  let added = 0;
  let removed = 0;
  for (const f of touched) {
    const s = model.sessionStats(f.path, snap.played);
    added += s.added;
    removed += s.removed;
  }

  const hovered = hover !== null ? edits[hover] : null;

  return (
    <footer className="aa-timeline">
      <div
        onMouseLeave={() => setHover(null)}
        onMouseMove={(e) => setHover(pick(e.clientX))}
        ref={ref}
        style={{ minWidth: 0, position: 'relative' }}
      >
        {/* The keyboard and screen-reader control; the trace below is its picture. */}
        <input
          aria-label="Edits shown"
          aria-valuetext={`${snap.played} of ${edits.length} edits`}
          className="aa-timeline-range"
          max={edits.length}
          min={0}
          onChange={(e) => player.seek(Number(e.target.value))}
          type="range"
          value={snap.played}
        />
        <svg
          aria-hidden="true"
          onClick={(e) => {
            const i = pick(e.clientX);
            if (i !== null) player.seek(i + 1);
          }}
        >
          <line
            stroke="var(--aa-line)"
            x1={PAD_LEFT - 10}
            x2={width - 8}
            y1={AXIS_Y}
            y2={AXIS_Y}
          />
          {model.bursts.map((b) => {
            const x0 = xs[b.start] - 6;
            const x1 = xs[b.end - 1] + 6;
            return (
              <rect
                fill="var(--aa-raised)"
                height={68}
                key={b.index}
                rx={5}
                width={Math.max(12, x1 - x0)}
                x={x0}
                y={14}
              />
            );
          })}
          {edits.map((e, i) => {
            if (i === 0) return null;
            const gap = e.time - edits[i - 1].time;
            const dx = xs[i] - xs[i - 1];
            if (gap <= BURST_GAP_MS || dx < 44) return null;
            return (
              <text
                fill="var(--aa-text-3)"
                fontSize={10.5}
                key={`gap${e.index}`}
                textAnchor="middle"
                x={(xs[i] + xs[i - 1]) / 2}
                y={86}
              >
                {formatDuration(gap)}
              </text>
            );
          })}
          {edits.length > 0 && (
            <line
              stroke="var(--aa-text-3)"
              strokeDasharray="2 4"
              x1={(xs.at(-1) ?? PAD_LEFT) + 4}
              x2={layout.tailEnd}
              y1={AXIS_Y}
              y2={AXIS_Y}
            />
          )}
          {commitXs.map(({ c, x }) => (
            <g key={c.sha}>
              <line
                opacity={0.7}
                stroke="var(--aa-mod)"
                strokeDasharray="3 3"
                x1={x}
                x2={x}
                y1={10}
                y2={84}
              />
              <rect
                fill="var(--aa-mod-soft)"
                height={15}
                rx={4}
                width={52}
                x={x - 26}
                y={2}
              />
              <text
                fill="var(--aa-mod)"
                fontFamily="var(--aa-mono)"
                fontSize={10}
                textAnchor="middle"
                x={x}
                y={13}
              >
                {c.sha.slice(0, 7)}
              </text>
            </g>
          ))}
          {edits.map((e, i) => (
            <Bar
              active={snap.active?.edit === e}
              edit={e}
              key={e.index}
              queued={i >= snap.played}
              x={xs[i]}
            />
          ))}
          {runs.map((r) => {
            const x = xAt(r.startedAt);
            const color =
              r.status === 'failed'
                ? 'var(--aa-del)'
                : r.status === 'passed'
                  ? 'var(--aa-add)'
                  : 'var(--aa-agent)';
            return (
              <g key={r.id} opacity={r.index >= snap.runsShown ? 0.28 : 1}>
                <title>{`${r.command} (${r.status})`}</title>
                {snap.activeRun?.run === r && (
                  <circle
                    cx={x}
                    cy={AXIS_Y}
                    fill="var(--aa-agent-soft)"
                    r={11}
                  />
                )}
                <rect
                  fill="var(--aa-surface)"
                  height={12}
                  rx={3}
                  stroke={color}
                  strokeWidth={1.5}
                  width={14}
                  x={x - 7}
                  y={AXIS_Y - 6}
                />
                <path d={`M${x - 2} ${AXIS_Y - 3}l4.5 3-4.5 3z`} fill={color} />
              </g>
            );
          })}
          <g
            style={{
              transform: `translateX(${playheadX}px)`,
              transition: 'transform 300ms cubic-bezier(0.3,0.7,0.2,1)',
            }}
          >
            <line
              stroke="var(--aa-agent)"
              strokeWidth={1.5}
              x1={0}
              x2={0}
              y1={8}
              y2={88}
            />
            <circle cx={0} cy={8} fill="var(--aa-agent)" r={3.5} />
          </g>
        </svg>
        {hovered && hover !== null && (
          <div className="aa-tip" style={{ left: xs[hover] }}>
            <b>{splitPath(hovered.path).name}</b>{' '}
            <span className="add">+{hovered.added}</span>{' '}
            <span className="del">−{hovered.removed}</span>
            <br />
            <span>
              {formatClock(hovered.time)}, edit {hover + 1}
            </span>
          </div>
        )}
      </div>
      <div className="aa-timeline-stats">
        <div>
          <b>{snap.played}</b>edits
        </div>
        <div>
          <b>{touched.length}</b>files
        </div>
        <div>
          <b className="add">+{added}</b>
          <b className="del">−{removed}</b>
        </div>
        <div>
          <b>
            {formatDuration(
              (edits[snap.played - 1]?.time ?? sessionStart) - sessionStart,
            )}
          </b>
          in
        </div>
      </div>
    </footer>
  );
}

function Bar({
  edit,
  x,
  queued,
  active,
}: {
  edit: Edit;
  x: number;
  queued: boolean;
  active: boolean;
}) {
  const up = barHeight(edit.added, 30);
  const down = barHeight(edit.removed, 24);
  const opacity = queued ? 0.28 : 1;
  return (
    <g opacity={opacity}>
      {active && (
        <circle cx={x} cy={AXIS_Y} fill="var(--aa-agent-soft)" r={9} />
      )}
      {up > 0 && (
        <rect
          fill="var(--aa-add)"
          height={up}
          rx={1.5}
          width={3}
          x={x - 1.5}
          y={AXIS_Y - up - 1}
        />
      )}
      {down > 0 && (
        <rect
          fill="var(--aa-del)"
          height={down}
          rx={1.5}
          width={3}
          x={x - 1.5}
          y={AXIS_Y + 1}
        />
      )}
      {up === 0 && down === 0 && (
        <circle cx={x} cy={AXIS_Y} fill="var(--aa-text-3)" r={2} />
      )}
    </g>
  );
}
