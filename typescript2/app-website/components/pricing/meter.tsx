'use client';

import { useEffect, useRef, useState } from 'react';
import { useReducedMotion, useVisible } from '@/components/home/pinned';

// What the meter bills, as a few short scenes that play in turn: what's recorded lights up, purple
// where it's billed and gray where it's free, and the bill adds up beside it. It's a picture of the
// rules, with byte counts kept simple enough to follow. The rules are the runtime's size model
// (`logical_bytes_approx_v1`, baml_language/crates/btel_snapshot/src/logical_bytes.rs: a field is a
// fixed-size ID, never its name; definitions, names and source count zero) and BCS's billing
// (control-plane/src/projector/billing.rs: spans and profiles are never billed, a retried upload
// counts once, one that never arrives counts zero). A repeated value billed once is the founders'
// decision, not yet in that code.

type Line = {
  /** Bytes billed; 0 with `free` for a line that costs nothing. */
  bytes: number;
  /** Why a line is free, shown in place of a price. */
  free?: string;
  /** The gray lead-in: a field name, a call, a span. */
  lead: string;
  /** The recorded value, highlighted when it's billed. */
  value?: string;
};

type Scene = {
  /** The line to rewrite at the end, and what it becomes, with the point it makes. */
  after?: { index: number; lead: string; say: string };
  lines: Line[];
  say: string;
  title: string;
};

const SCENES: Scene[] = [
  {
    after: {
      index: 2,
      lead: 'customer_email_address_on_file:',
      say: 'Longer field name, same bill.',
    },
    lines: [
      { bytes: 0, free: 'type', lead: 'Ticket {' },
      { bytes: 4, lead: '  ticket_id:', value: '4812' },
      { bytes: 11, lead: '  customer_email:', value: '"sam@acme.co"' },
      { bytes: 4, lead: '  priority:', value: 'Priority.High' },
      { bytes: 21, lead: '  body:', value: '"My refund came twice."' },
      { bytes: 0, free: 'definition', lead: 'class Ticket { … }' },
    ],
    say: 'You pay for the values, not the names around them.',
    title: 'Field names',
  },
  {
    lines: [
      {
        bytes: 1200,
        lead: 'call 1  prompt:',
        value: '"You are a support agent…"',
      },
      { bytes: 40, lead: '        ticket:', value: '"My refund came twice."' },
      {
        bytes: 0,
        free: 'already stored',
        lead: 'call 2  prompt:',
        value: '"You are a support agent…"',
      },
      { bytes: 36, lead: '        ticket:', value: '"Where is my order?"' },
      {
        bytes: 0,
        free: 'already stored',
        lead: 'call 3  prompt:',
        value: '"You are a support agent…"',
      },
      { bytes: 33, lead: '        ticket:', value: '"Cancel my plan."' },
    ],
    say: 'A prompt sent a thousand times is billed once.',
    title: 'Repeated values',
  },
  {
    lines: [
      { bytes: 0, free: 'free', lead: 'span   draft_reply · 1.84s' },
      { bytes: 0, free: 'free', lead: 'span   classify · 0.61s' },
      { bytes: 0, free: 'free', lead: 'profile  call tree, self and I/O time' },
      { bytes: 0, free: 'free', lead: 'source  function draft_reply(…)' },
      { bytes: 40, lead: 'input', value: '"My refund came twice."' },
      { bytes: 120, lead: 'output', value: '"Sorry, Sam. Refunded…"' },
    ],
    say: 'Spans, timings and profiles are free. Never sample to save money.',
    title: 'Structure',
  },
  {
    lines: [
      { bytes: 2400, lead: 'upload 1', value: 'recording, 2.4 KB' },
      {
        bytes: 0,
        free: 'counted once',
        lead: 'upload 1, retried',
        value: 'recording, 2.4 KB',
      },
      {
        bytes: 0,
        free: 'never arrived',
        lead: 'upload 2',
        value: 'lost in transit',
      },
    ],
    say: 'A retry counts once. What never arrives counts as nothing.',
    title: 'Retries',
  },
];

const STEP_MS = 900;
const HOLD_MS = 3500;

const size = (bytes: number) =>
  bytes >= 1000 ? `${(bytes / 1000).toFixed(1)} KB` : `${bytes} B`;

export function Meter() {
  const ref = useRef<HTMLDivElement>(null);
  const visible = useVisible(ref);
  const reduce = useReducedMotion();
  const [scene, setScene] = useState(0);
  const [step, setStep] = useState(0);

  const s = SCENES[scene];
  // A step per line, one more for the rewrite if the scene has one.
  const last = s.lines.length + (s.after ? 1 : 0);

  useEffect(() => {
    if (reduce) {
      setStep(last);
      return;
    }
    if (!visible) return;
    const timer = setTimeout(
      () => {
        if (step < last) setStep(step + 1);
        else {
          setScene((n) => (n + 1) % SCENES.length);
          setStep(0);
        }
      },
      step === 0 ? 500 : step >= last ? HOLD_MS : STEP_MS,
    );
    return () => clearTimeout(timer);
  }, [step, last, visible, reduce]);

  const rewritten = s.after !== undefined && step > s.lines.length;
  const billed = s.lines.slice(0, step).reduce((sum, l) => sum + l.bytes, 0);

  return (
    <div
      className="overflow-hidden rounded-lg border border-rule bg-raised"
      ref={ref}
    >
      {/* The scenes, as tabs: they play in turn, and a click jumps to one. */}
      <div className="flex gap-1 overflow-x-auto border-b border-rule px-2 pt-2">
        {SCENES.map((sc, i) => (
          <button
            className={`relative cursor-pointer whitespace-nowrap px-3 pt-1.5 pb-2.5 text-[12.5px] transition-colors ${i === scene ? 'text-head' : 'text-muted hover:text-ink'}`}
            key={sc.title}
            onClick={() => {
              setScene(i);
              setStep(
                reduce ? SCENES[i].lines.length + (SCENES[i].after ? 1 : 0) : 0,
              );
            }}
            type="button"
          >
            {sc.title}
            <span
              className="absolute inset-x-3 bottom-0 h-0.5 rounded-full bg-clover transition-opacity"
              style={{ opacity: i === scene ? 1 : 0 }}
            />
          </button>
        ))}
      </div>

      <div className="grid md:grid-cols-[minmax(0,1fr)_14rem]">
        <div className="min-h-[248px] px-5 py-4 font-mono text-[13px] leading-[2]">
          {s.lines.map((l, i) => {
            const on = i < step;
            const lead =
              rewritten && s.after?.index === i ? s.after.lead : l.lead;
            const paid = l.bytes > 0;
            return (
              <div
                className="flex items-center justify-between gap-4 transition-opacity duration-500"
                // biome-ignore lint/suspicious/noArrayIndexKey: fixed scenes
                key={`${scene}-${i}`}
                style={{ opacity: on ? 1 : 0.3 }}
              >
                <span className="min-w-0 truncate whitespace-pre">
                  <span
                    className={
                      rewritten && s.after?.index === i
                        ? 'text-ink-2'
                        : 'text-muted'
                    }
                  >
                    {lead}
                  </span>
                  {l.value && (
                    <>
                      {' '}
                      <span
                        className={`rounded px-1 transition-[background-color,color] duration-500 ${on && !paid ? 'line-through decoration-white/30' : ''}`}
                        style={{
                          background:
                            on && paid
                              ? 'rgba(202,170,242,0.18)'
                              : 'transparent',
                          color: !on
                            ? 'var(--color-ink)'
                            : paid
                              ? 'var(--color-clover)'
                              : 'var(--muted)',
                        }}
                      >
                        {l.value}
                      </span>
                    </>
                  )}
                </span>
                <span
                  className={`shrink-0 font-sans text-[12px] transition-opacity duration-300 ${paid ? 'text-clover' : 'text-muted'}`}
                  style={{ opacity: on ? 1 : 0 }}
                >
                  {paid ? `+${size(l.bytes)}` : l.free}
                </span>
              </div>
            );
          })}
        </div>

        {/* The bill, the scene's point, and what the colors mean. */}
        <div className="flex flex-col justify-between gap-6 border-t border-rule px-5 py-4 md:border-t-0 md:border-l">
          <div>
            <p className="text-[12px] text-muted">Billed</p>
            <p className="mt-1 font-display text-[40px] leading-none font-medium text-clover tabular-nums">
              {size(billed)}
            </p>
            <p className="mt-4 text-[13px] text-head">
              {rewritten ? s.after?.say : s.say}
            </p>
          </div>
          <div className="space-y-2 text-[12.5px]">
            <p className="flex items-center gap-2 text-ink-2">
              <span className="size-3 rounded-sm bg-clover/40" />
              Billed
            </p>
            <p className="flex items-center gap-2 text-ink-2">
              <span className="size-3 rounded-sm bg-white/15" />
              Free
            </p>
          </div>
        </div>
      </div>
    </div>
  );
}
