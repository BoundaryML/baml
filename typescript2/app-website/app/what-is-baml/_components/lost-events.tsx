'use client';

import { useCallback, useRef, useState, useSyncExternalStore } from 'react';
import { getDropped, getStartServer, subscribeDropped } from './dropped-store';

// The observability pair. Two rows, same ripple field: under OTEL only the
// sampled events keep their light after the ripple passes. Under BAML every
// event keeps its light. The full runnable example follows in the explorer
// below.

const COLS = 18;
const ROWS = 9;
const CELLS = COLS * ROWS;
const TICK_MS = 110;
// One calm wavefront sweeping left to right. The extra span leaves about
// 1.8 seconds of calm after the wave clears the slanted field.
const SWEEP_SPEED = 0.45; // columns per tick
const SWEEP_SPAN = COLS + 10;

const PURPLE = [109, 40, 217] as const;
const RED = [180, 52, 43] as const;
const BEIGE = [232, 227, 213] as const;

function settledGlow(i: number, remember: boolean, pass = 0) {
  if (remember) return 0.45;

  // A scattered sample of roughly one in nine events. Each pass changes the
  // offset so the next wave captures a different subset.
  return (i * 73 + pass * 41 + 19) % 101 < 11 ? 0.6 : 0;
}

// One animated ripple field, pinned to a mode.
function Field({ remember }: { remember: boolean }) {
  const [, setFrame] = useState(0);
  const tickRef = useRef(0);
  const passRef = useRef(0);
  const memoryRef = useRef<Float32Array>(new Float32Array(CELLS));
  const flashRef = useRef<Float32Array>(new Float32Array(CELLS));
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const rootRef = useCallback(
    (node: HTMLDivElement | null) => {
      if (!node) return undefined;
      const reduced = window.matchMedia(
        '(prefers-reduced-motion: reduce)',
      ).matches;
      if (reduced) {
        const memory = memoryRef.current;
        for (let i = 0; i < CELLS; i++) {
          memory[i] = settledGlow(i, remember);
        }
        setFrame((n) => n + 1);
        return undefined;
      }
      const step = () => {
        const t = ++tickRef.current;
        const memory = memoryRef.current;
        const flash = flashRef.current;
        // A single soft wavefront, slightly slanted, sweeping left to right.
        const front = (t * SWEEP_SPEED) % SWEEP_SPAN;
        const previousFront = ((t - 1) * SWEEP_SPEED) % SWEEP_SPAN;
        // Start each pass empty so dots are only revealed after the wave
        // reaches them. OTEL also selects a new sample for every pass.
        if (front < previousFront) {
          memory.fill(0);
          if (!remember) passRef.current += 1;
        }
        for (let i = 0; i < CELLS; i++) {
          const x = (i % COLS) + 0.5;
          const y = Math.floor(i / COLS) + 0.5;
          const d = Math.abs(x + y * 0.25 - front);
          const f = d < 1.6 ? 1 - d / 1.6 : 0;
          flash[i] = f * 0.7;
          if (f > 0.4) {
            // The wave reveals what was retained: every BAML event, but only
            // the sampled OTEL events. OTEL holds this result through the
            // pause, then clears it when the next pass starts.
            memory[i] = settledGlow(i, remember, passRef.current);
          }
        }
        setFrame((n) => n + 1);
      };
      timerRef.current = setInterval(step, TICK_MS);
      return () => {
        if (timerRef.current) clearInterval(timerRef.current);
      };
    },
    [remember],
  );

  const memory = memoryRef.current;
  const flash = flashRef.current;
  const tint = remember ? PURPLE : RED;

  return (
    <div
      aria-label={
        remember
          ? 'A field where every dot stays purple after the ripple passes.'
          : 'A field where only sampled dots stay red after the ripple passes.'
      }
      className="lev-field"
      ref={rootRef}
      role="img"
    >
      {Array.from({ length: CELLS }, (_, i) => {
        const glow = Math.min(1, memory[i] + flash[i]);
        const r = BEIGE[0] + (tint[0] - BEIGE[0]) * glow;
        const g = BEIGE[1] + (tint[1] - BEIGE[1]) * glow;
        const b = BEIGE[2] + (tint[2] - BEIGE[2]) * glow;
        return (
          <span
            className="lev-dot"
            // biome-ignore lint/suspicious/noArrayIndexKey: fixed grid
            key={i}
            style={{
              background: `rgb(${r | 0} ${g | 0} ${b | 0})`,
              transform: flash[i] > 0.45 ? 'scale(1.12)' : undefined,
            }}
          />
        );
      })}
    </div>
  );
}

const KINDS = [
  'llm call',
  'tool call',
  'retry 429',
  'parse',
  'spawn',
  'http fetch',
  'cache miss',
  'token count',
];

// A small live feed under each field: event by event, what the collector
// did with it. OTEL keeps roughly one in nine; BAML keeps them all.
function EventFeed({ keepAll }: { keepAll: boolean }) {
  const [count, setCount] = useState(0);
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const rootRef = useCallback((node: HTMLDivElement | null) => {
    if (!node) return undefined;
    const reduced = window.matchMedia(
      '(prefers-reduced-motion: reduce)',
    ).matches;
    if (reduced) {
      setCount(4);
      return undefined;
    }
    timerRef.current = setInterval(() => setCount((c) => c + 1), 1700);
    return () => {
      if (timerRef.current) clearInterval(timerRef.current);
    };
  }, []);

  const rows = Array.from({ length: 4 }, (_, j) => count - (3 - j)).filter(
    (i) => i >= 0,
  );
  return (
    <div aria-hidden className="lev-feed" ref={rootRef}>
      {rows.map((i) => {
        const captured = keepAll || i % 9 === 0;
        return (
          <div className="lev-feed-row" key={i}>
            <span className="lev-feed-ev">
              {KINDS[i % KINDS.length]}&hellip;
            </span>
            <span className={`lev-feed-verdict ${captured ? 'ok' : 'drop'}`}>
              {captured ? 'captured' : 'dropped'}
            </span>
          </div>
        );
      })}
    </div>
  );
}

export function LostEvents() {
  const dropped = useSyncExternalStore(
    subscribeDropped,
    getDropped,
    getStartServer,
  );

  return (
    <div className="lev">
      <div className="lev-col">
        <span className="lev-tag lev-tag--otel">OTEL</span>
        <Field remember={false} />
        <EventFeed keepAll={false} />
        <p className="lev-note">
          The field forgets as fast as it learns.{' '}
          {dropped.toLocaleString('en-US')} events dropped since you opened this
          page.
        </p>
      </div>
      <div className="lev-col">
        <span className="lev-tag lev-tag--baml">BAML</span>
        <Field remember />
        <EventFeed keepAll />
        <p className="lev-note">Nothing is forgotten.</p>
      </div>
    </div>
  );
}
