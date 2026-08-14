'use client';

import { useAnimateInView } from '../../learn3/_lib/use-animate-in-view';

/**
 * Minimalist task-group scheduling vignette: two lanes, six blocks, no
 * labels, no frame. Each bar draws itself in at its scheduled time; a thin
 * sweep line marks "now". The whole timeline loops every PERIOD seconds.
 *
 * Per-bar keyframes are generated from the static schedule below (the
 * percentages differ per bar, so they can't share one @keyframes). The
 * schedule itself is the point: lane capacity 2, extras start exactly when
 * a lane frees up — the same fifo behaviour as a TaskGroup with a cap.
 */

const PERIOD = 8; // seconds
const SPAN = 6.6; // seconds of schedule inside each loop (rest is rest)

interface Bar {
  lane: 0 | 1;
  start: number; // seconds
  dur: number; // seconds
}

const BARS: Bar[] = [
  { dur: 2.2, lane: 0, start: 0.2 },
  { dur: 1.4, lane: 1, start: 0.2 },
  { dur: 2.6, lane: 1, start: 1.6 },
  { dur: 1.8, lane: 0, start: 2.4 },
  { dur: 2.0, lane: 0, start: 4.2 },
  { dur: 1.6, lane: 1, start: 4.2 },
];

const pct = (s: number) => ((s / PERIOD) * 100).toFixed(2);

const BAR_KEYFRAMES = BARS.map((b, i) => {
  const grow = `@keyframes l6-sch-${i} {
  0%, ${pct(b.start)}% { transform: scaleX(0); opacity: 0.9; }
  ${pct(b.start + b.dur)}% { transform: scaleX(1); opacity: 0.9; }
  ${pct(b.start + b.dur + 0.15)}%, 93% { transform: scaleX(1); opacity: 0.35; }
  100% { transform: scaleX(1); opacity: 0; }
}`;
  return grow;
}).join('\n');

export function Scheduler() {
  const { ref, holdClass } = useAnimateInView();
  return (
    <div aria-hidden className={`l6-sch${holdClass}`} ref={ref}>
      <style>{BAR_KEYFRAMES}</style>
      {[0, 1].map((lane) => (
        <div className="l6-sch-lane" key={lane} />
      ))}
      {BARS.map((b, i) => (
        <span
          className="l6-sch-bar"
          key={`${b.lane}-${b.start}`}
          style={
            {
              animation: `l6-sch-${i} ${PERIOD}s linear infinite`,
              left: `${(b.start / SPAN) * 92}%`,
              top: b.lane === 0 ? 'calc(50% - 17px)' : 'calc(50% + 3px)',
              width: `${(b.dur / SPAN) * 92}%`,
            } as React.CSSProperties
          }
        />
      ))}
      <span className="l6-sch-now" />
    </div>
  );
}
