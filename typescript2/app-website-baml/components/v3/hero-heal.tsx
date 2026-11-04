'use client';

import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { Code, type LineMark } from './code';
import { classifySource, FIXES, RED } from './story';

// A function and a live strip of its production calls, like a heart monitor. When calls start failing,
// a line runs from them up into the prompt, the prompt gains a line, and the strip goes clean again.
const BARS = 56;
const TICK_MS = 170;
const TICKS = 52;
const READ = 24;
const FIX = 30;
const SETTLE = 42;

// A call is its bar's height in percent, or FAILED.
const FAILED = -1;

type State = { calls: number[]; failed: number; lap: number; tick: number };

function next(s: State): State {
  const tick = (s.tick + 1) % TICKS;
  const lap = tick === 0 ? s.lap + 1 : s.lap;
  const fails = tick > 4 && tick < FIX && Math.random() < 0.3;
  return {
    calls: [...s.calls.slice(1), fails ? FAILED : 35 + Math.random() * 40],
    failed: (tick === 0 ? 0 : s.failed) + (fails ? 1 : 0),
    lap,
    tick,
  };
}

const START: State = {
  calls: Array.from({ length: BARS }, (_, i) => 35 + ((i * 37) % 40)),
  failed: 0,
  lap: 0,
  tick: 0,
};

export default function HeroHeal() {
  const [state, setState] = useState(START);
  const frame = useRef<HTMLDivElement>(null);
  const strip = useRef<HTMLDivElement>(null);
  const [wire, setWire] = useState('');

  useEffect(() => {
    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
      let s = START;
      for (let i = 0; i < FIX + 2; i++) s = next(s);
      setState(s);
      return;
    }
    const id = setInterval(() => setState(next), TICK_MS);
    return () => clearInterval(id);
  }, []);

  const { calls, failed, lap, tick } = state;
  const phase = tick < READ ? 'failing' : tick < FIX ? 'reading' : 'fixed';
  const round = lap % FIXES.length;
  const done = round + (phase === 'fixed' ? 1 : 0);
  const { lines, prose } = classifySource(
    FIXES.slice(0, done).map((f) => f.rule),
  );
  const added = 3 + done;
  const marks: Record<number, LineMark> =
    phase === 'reading'
      ? { 2: 'removed' }
      : phase === 'fixed' && tick < SETTLE
        ? { [added]: 'added' }
        : {};
  const wired = phase === 'reading' || (phase === 'fixed' && tick < SETTLE);

  // The wire runs from the newest failed call up to the line it changes.
  useLayoutEffect(() => {
    const box = frame.current?.getBoundingClientRect();
    const newest = calls.lastIndexOf(FAILED);
    const bar = strip.current?.children[newest]?.getBoundingClientRect();
    const row = frame.current?.querySelector(
      `[data-line="${phase === 'reading' ? 2 : added}"] > span:last-child`,
    );
    const text = row?.getBoundingClientRect();
    if (!wired || !box || !bar || !text || newest < 0) {
      setWire('');
      return;
    }
    const x = bar.left + bar.width / 2 - box.left;
    const y = text.top + text.height / 2 - box.top;
    const end = Math.min(text.right - box.left + 14, x - 24);
    setWire(
      `M${x} ${bar.top - box.top - 6}V${y + 10}Q${x} ${y} ${x - 10} ${y}H${end}`,
    );
  }, [calls, phase, added, wired]);

  const fix = FIXES[round];

  return (
    <figure
      aria-label="A BAML function above a live strip of its production calls. When calls fail, a line runs from them to the prompt, the prompt gains a line, and the calls pass again."
      className="relative pb-16"
    >
      <div
        aria-hidden
        className="v3-glow left-[-10%] right-[-10%] -top-24 h-[720px]"
      />
      <div className="v3-window relative overflow-hidden" ref={frame}>
        <div className="flex items-center justify-between h-11 px-4 border-b border-white/8 text-[13px]">
          <span className="text-white/70">triage.baml</span>
          <span className="font-mono text-white/40">
            v{41 + lap + (phase === 'fixed' ? 1 : 0)}
          </span>
        </div>
        <div className="min-h-[300px] md:min-h-[320px] py-4">
          <Code lines={lines} marks={marks} prose={prose} />
        </div>

        <div className="px-4 md:px-6 pt-4 pb-5 border-t border-white/8">
          <div className="flex flex-wrap items-baseline justify-between gap-x-6 gap-y-1 mb-3 text-[14px]">
            <span className="text-white/50">Production calls</span>
            {/* Sits above the wire, which passes behind it on the way up. */}
            <span
              aria-live="polite"
              className="relative z-10 flex items-center gap-2 pl-3 bg-[#0b0b0d]"
            >
              {phase === 'fixed' ? (
                <span className="v3-enter text-clover" key="fixed">
                  Fixed with one line in the prompt. 0 failing.
                </span>
              ) : phase === 'reading' ? (
                <span className="v3-enter text-white" key="reading">
                  Your agent is reading {failed} failed calls
                </span>
              ) : (
                <span className="flex items-center gap-2 text-white/75">
                  <span
                    className="size-2 rounded-full"
                    style={{
                      background: failed ? RED : 'var(--color-clover-core)',
                    }}
                  />
                  {failed
                    ? `${failed} failing: ${fix.failure}`
                    : 'All calls passing'}
                </span>
              )}
            </span>
          </div>
          <div className="flex items-end gap-[3px] h-12 md:h-16" ref={strip}>
            {calls.map((call, i) => (
              <span
                className="flex-1 min-w-0 rounded-[2px]"
                // biome-ignore lint/suspicious/noArrayIndexKey: a bar is a slot in the strip, not a call
                key={i}
                style={
                  call === FAILED
                    ? {
                        background: RED,
                        boxShadow: `0 0 14px ${RED}`,
                        height: '100%',
                      }
                    : {
                        background:
                          'rgb(from var(--color-clover-core) r g b / 0.45)',
                        height: `${call}%`,
                      }
                }
              />
            ))}
          </div>
        </div>

        {wire && (
          <svg
            aria-hidden="true"
            className="absolute inset-0 size-full overflow-visible pointer-events-none"
          >
            <path
              d={wire}
              fill="none"
              stroke={phase === 'reading' ? RED : 'var(--color-clover-core)'}
              strokeWidth={1.5}
              style={{
                filter: `drop-shadow(0 0 6px ${phase === 'reading' ? RED : 'var(--color-clover-core)'})`,
              }}
            />
          </svg>
        )}
      </div>
    </figure>
  );
}
