'use client';

import { useEffect, useRef, useState } from 'react';
import { useReducedMotion, useVisible } from '@/components/home/pinned';

// What a captured value is billed for, played line by line: the values light up and add to the bill;
// the field names, type names and definitions around them stay gray and add nothing. Then a field
// gets a much longer name and the bill doesn't move. It's a picture of the runtime's size model
// (`logical_bytes_approx_v1` in baml_language/crates/btel_snapshot/src/logical_bytes.rs: a field is a
// fixed-size ID, never its name; definitions and names count zero), with the byte counts rounded to
// what a reader can follow rather than its exact accounting.

type Line = { bytes: number; key?: string; value?: string; text?: string };

const SHORT = 'customer_email';
const LONG = 'customer_email_address_on_file';

const LINES: Line[] = [
  { bytes: 0, text: 'Ticket {' },
  { bytes: 4, key: 'ticket_id', value: '4812' },
  { bytes: 11, key: SHORT, value: '"sam@acme.co"' },
  { bytes: 4, key: 'priority', value: 'Priority.High' },
  { bytes: 21, key: 'body', value: '"My refund came twice."' },
  { bytes: 0, text: '}' },
];

const STEPS = LINES.length + 2; // the lines, the definitions, the rename
const RENAME = STEPS;
const STEP_MS = 1000;
const HOLD_MS = 4500;

export function Meter() {
  const ref = useRef<HTMLDivElement>(null);
  const visible = useVisible(ref);
  const reduce = useReducedMotion();
  const [step, setStep] = useState(0);

  useEffect(() => {
    if (reduce) {
      setStep(STEPS);
      return;
    }
    if (!visible) return;
    const wait = step === 0 ? 500 : step >= STEPS ? HOLD_MS : STEP_MS;
    const timer = setTimeout(
      () => setStep((n) => (n >= STEPS ? 0 : n + 1)),
      wait,
    );
    return () => clearTimeout(timer);
  }, [step, visible, reduce]);

  const renamed = step >= RENAME;
  const billed = LINES.slice(0, step).reduce((sum, l) => sum + l.bytes, 0);
  const defs = step > LINES.length;

  return (
    <div
      className="overflow-hidden rounded-lg border border-rule bg-raised"
      ref={ref}
    >
      <div className="grid gap-0 md:grid-cols-[minmax(0,1fr)_14rem]">
        <div className="px-5 py-4 font-mono text-[13px] leading-[2]">
          {LINES.map((l, i) => {
            const on = i < step;
            if (l.text)
              return (
                <div className="text-muted" key={l.text}>
                  {l.text}
                </div>
              );
            const key = renamed && l.key === SHORT ? LONG : l.key;
            return (
              <div
                className="flex items-center justify-between gap-4"
                key={l.key}
              >
                <span className="min-w-0 truncate whitespace-pre">
                  <span
                    className={`transition-colors duration-500 ${renamed && l.key === SHORT ? 'text-ink-2' : 'text-muted'}`}
                  >
                    {'  '}
                    {key}:{' '}
                  </span>
                  <span
                    className="rounded px-1 transition-[background-color,color] duration-500"
                    style={{
                      background: on ? 'rgba(202,170,242,0.18)' : 'transparent',
                      color: on ? 'var(--color-clover)' : 'var(--color-ink)',
                    }}
                  >
                    {l.value}
                  </span>
                </span>
                <span
                  className="shrink-0 font-sans text-[12px] text-clover transition-opacity duration-300"
                  style={{ opacity: on ? 1 : 0 }}
                >
                  +{l.bytes} B
                </span>
              </div>
            );
          })}
          <div
            className="mt-3 border-t border-dashed border-rule pt-3 text-muted transition-opacity duration-500"
            style={{ opacity: defs ? 1 : 0.35 }}
          >
            class Ticket {'{ … }'}
            <span className="ml-3 font-sans text-[12px]">free</span>
          </div>
        </div>

        {/* The bill, and what each color means. */}
        <div className="flex flex-col justify-between gap-6 border-t border-rule px-5 py-4 md:border-t-0 md:border-l">
          <div>
            <p className="text-[12px] text-muted">Billed</p>
            <p className="mt-1 font-display text-[40px] leading-none font-medium text-clover tabular-nums">
              {billed} B
            </p>
            <p
              className="mt-3 text-[12.5px] text-head transition-opacity duration-500"
              style={{ opacity: renamed ? 1 : 0 }}
            >
              Longer field name, same bill.
            </p>
          </div>
          <div className="space-y-2 text-[12.5px]">
            <p className="flex items-center gap-2 text-ink-2">
              <span className="size-3 rounded-sm bg-clover/40" />
              Values: billed
            </p>
            <p className="flex items-center gap-2 text-ink-2">
              <span className="size-3 rounded-sm bg-white/15" />
              Names, types, structure: free
            </p>
          </div>
        </div>
      </div>
    </div>
  );
}
