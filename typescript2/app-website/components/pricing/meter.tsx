'use client';

import { Check } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { useReducedMotion, useVisible } from '@/components/home/pinned';

// One made-up call, recorded piece by piece, with the meter deciding each piece as it arrives: the
// values it records are billed at their size, the structure around them is free, a prompt sent again
// folds into the first copy, and a retried upload counts once. The rules are BCS's billing code
// (control-plane/src/projector/billing.rs), except billing a repeated value once, which is the
// founders' decision. The sizes are illustrative.

type Piece = {
  /** Bytes billed, or 0 for free. */
  bytes: number;
  kind: string;
  /** Why it costs what it costs, shown once it's decided. */
  why: string;
  what: string;
};

const PIECES: Piece[] = [
  { bytes: 0, kind: 'span', what: 'draft_reply · 1.84s', why: 'structure' },
  { bytes: 2100, kind: 'input', what: 'ticket { id, body, … }', why: '2.1 KB' },
  {
    bytes: 1200,
    kind: 'prompt',
    what: '"You are a support agent…"',
    why: '1.2 KB',
  },
  { bytes: 800, kind: 'output', what: '"Hi Sam, your refund…"', why: '0.8 KB' },
  {
    bytes: 0,
    kind: 'profile',
    what: 'call tree and timings',
    why: 'structure',
  },
  { bytes: 100, kind: 'log', what: 'drafted', why: '0.1 KB' },
  {
    bytes: 0,
    kind: 'prompt',
    what: '"You are a support agent…"',
    why: 'already stored',
  },
  {
    bytes: 0,
    kind: 'retry',
    what: 'same upload, sent twice',
    why: 'counted once',
  },
];

// Everything the call recorded, billed or not, to set the bill against.
const RECORDED = 9800;
const STEP_MS = 1100;
const HOLD_MS = 4000;

const kb = (bytes: number) => `${(bytes / 1000).toFixed(1)} KB`;

/** The meter, playing through the pieces while it's on screen, then holding and starting over. */
export function Meter() {
  const ref = useRef<HTMLDivElement>(null);
  const visible = useVisible(ref);
  const reduce = useReducedMotion();
  const [shown, setShown] = useState(0);

  useEffect(() => {
    if (reduce) {
      setShown(PIECES.length);
      return;
    }
    if (!visible) return;
    const timer = setTimeout(
      () => setShown((n) => (n >= PIECES.length ? 0 : n + 1)),
      shown >= PIECES.length ? HOLD_MS : shown === 0 ? 400 : STEP_MS,
    );
    return () => clearTimeout(timer);
  }, [shown, visible, reduce]);

  const billed = PIECES.slice(0, shown).reduce((sum, p) => sum + p.bytes, 0);

  return (
    <div
      className="overflow-hidden rounded-lg border border-rule bg-raised"
      ref={ref}
    >
      <div className="flex items-center justify-between border-b border-rule px-4 py-2.5 text-[12px] text-muted">
        <span className="font-mono">support.baml · one call</span>
        <span>recorded → billed</span>
      </div>
      <ul className="divide-y divide-rule font-mono text-[12.5px]">
        {PIECES.map((p, i) => {
          const on = i < shown;
          const free = p.bytes === 0;
          return (
            <li
              className="grid grid-cols-[4.5rem_minmax(0,1fr)_auto] items-center gap-3 px-4 py-2 transition-[opacity,transform] duration-500"
              // biome-ignore lint/suspicious/noArrayIndexKey: fixed list
              key={i}
              style={{
                opacity: on ? 1 : 0.18,
                transform: on ? 'none' : 'translateX(-6px)',
              }}
            >
              <span className="text-muted">{p.kind}</span>
              <span
                className={`truncate ${on && free ? 'text-ink-2 line-through decoration-white/25' : 'text-ink'}`}
              >
                {p.what}
              </span>
              <span
                className={`flex items-center gap-1.5 whitespace-nowrap font-sans text-[12px] transition-opacity duration-300 ${free ? 'text-clover' : 'text-head'}`}
                style={{ opacity: on ? 1 : 0 }}
              >
                {free && <Check aria-hidden="true" className="size-3.5" />}
                {free ? `free, ${p.why}` : p.why}
              </span>
            </li>
          );
        })}
      </ul>
      <div className="flex items-baseline justify-between gap-4 border-t border-rule bg-white/[0.02] px-4 py-3">
        <span className="text-[12px] text-muted">Recorded {kb(RECORDED)}</span>
        <span className="text-[13px] text-head">
          Billed{' '}
          <span className="font-mono tabular-nums text-clover">
            {kb(billed)}
          </span>
        </span>
      </div>
    </div>
  );
}
