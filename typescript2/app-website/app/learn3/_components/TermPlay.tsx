'use client';

import { useState } from 'react';
import { cn } from '@/lib/utils';
import { useAnimateInView } from '../_lib/use-animate-in-view';

export type TermTone = 'out' | 'dim' | 'ok' | 'err' | 'accent' | 'warn';

export interface TermEvent {
  /** A shell command — rendered with a `$` prompt and a typing animation. */
  cmd?: string;
  /** An output line, revealed after the preceding command finishes typing. */
  text?: string;
  tone?: TermTone;
  /** Extra pause (seconds) before this event starts. */
  pause?: number;
}

const CHAR_S = 0.024; // typing speed, per character
const LINE_S = 0.1; // gap between output lines
const CMD_SETTLE_S = 0.45; // pause after a command before its output

/**
 * An animated terminal replay. The whole timeline is computed at render time
 * and driven by CSS animation delays — no effects, no timers. "Replay" bumps
 * a key to remount the body, which restarts every animation.
 *
 * Output text is real, captured CLI output — keep it that way.
 */
export function TermPlay({
  title = 'terminal',
  events,
}: {
  title?: string;
  events: TermEvent[];
}) {
  const [runId, setRunId] = useState(0);
  const { ref, holdClass } = useAnimateInView();

  let t = 0.35;
  const timed = events.map((e) => {
    t += e.pause ?? 0;
    const start = t;
    t += e.cmd ? e.cmd.length * CHAR_S + CMD_SETTLE_S : LINE_S;
    return { ...e, start };
  });

  return (
    <div className={`l3-term${holdClass}`} ref={ref}>
      <div className="l3-term-bar">
        <span className="l3-term-dots" aria-hidden>
          <i />
          <i />
          <i />
        </span>
        <span className="l3-term-title">{title}</span>
        <button
          type="button"
          className="l3-term-replay"
          onClick={() => setRunId((n) => n + 1)}
        >
          replay ↻
        </button>
      </div>
      <pre key={runId} className="l3-term-body">
        {timed.map((e, i) =>
          e.cmd != null ? (
            <div
              // biome-ignore lint/suspicious/noArrayIndexKey: static, order-stable script
              key={i}
              className="l3-term-row"
              style={{ animationDelay: `${e.start}s` }}
            >
              <span className="l3-term-prompt">$ </span>
              <span
                className="l3-term-cmd"
                style={{
                  ['--l3-w' as string]: `${e.cmd.length}ch`,
                  animationDelay: `${e.start}s`,
                  animationDuration: `${e.cmd.length * CHAR_S}s`,
                  animationTimingFunction: `steps(${e.cmd.length}, end)`,
                }}
              >
                {e.cmd}
              </span>
            </div>
          ) : (
            <div
              // biome-ignore lint/suspicious/noArrayIndexKey: static, order-stable script
              key={i}
              className={cn('l3-term-row', e.tone && `l3-tone-${e.tone}`)}
              style={{ animationDelay: `${e.start}s` }}
            >
              {e.text || ' '}
            </div>
          ),
        )}
      </pre>
    </div>
  );
}
