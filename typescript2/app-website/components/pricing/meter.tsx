'use client';

import { type ReactNode, useEffect, useRef, useState } from 'react';
import { useReducedMotion, useVisible } from '@/components/home/pinned';

// How the meter keeps the bill down, shown as the mechanism itself rather than told: on the left
// what a program records, on the right what Boundary Cloud stores, and the bill under both.
//
// - Repeated values: every value is stored under a hash of its content, so the same value sent again
//   finds itself already stored and adds nothing. Values are content-addressed in BCS today; billing
//   a repeat once is the founders' decision, not yet in its billing code.
// - Field names: a class's definition is stored once and never billed; each value points at it with
//   short IDs, so a field's name never reaches the bill. That's the runtime's size model
//   (`logical_bytes_approx_v1`, baml_language/crates/btel_snapshot/src/logical_bytes.rs).
//
// Sizes are kept round enough to follow; the hashes are made up.

const STEP_MS = 1300;
const HOLD_MS = 4500;

const size = (bytes: number) =>
  bytes >= 1000 ? `${(bytes / 1000).toFixed(1)} KB` : `${bytes} B`;

/** A value as the program sent it: purple when it's billed, struck through when it's free. */
function Chip({
  billed,
  children,
  on,
}: {
  billed: boolean;
  children: string;
  on: boolean;
}) {
  return (
    <span
      className={`rounded px-1 transition-[background-color,color] duration-500 ${on && !billed ? 'line-through decoration-white/30' : ''}`}
      style={{
        background: on && billed ? 'rgba(202,170,242,0.18)' : 'transparent',
        color: !on
          ? 'var(--color-ink)'
          : billed
            ? 'var(--color-clover)'
            : 'var(--muted)',
      }}
    >
      {children}
    </span>
  );
}

function Pane({ children, title }: { children: ReactNode; title: string }) {
  return (
    <div className="min-w-0 px-5 py-4">
      <p className="mb-3 font-sans text-[12px] text-muted">{title}</p>
      <div className="font-mono text-[12.5px] leading-[2]">{children}</div>
    </div>
  );
}

// ---------- repeated values ----------

const PROMPT = '"You are a support agent…"';

/** One value sent per step: its hash, whether it was new, and what it adds. */
const SENT = [
  { bytes: 1200, call: 1, hash: 'a41f', label: 'prompt', value: PROMPT },
  {
    bytes: 40,
    call: 1,
    hash: '77c0',
    label: 'ticket',
    value: '"My refund came twice."',
  },
  { bytes: 1200, call: 2, hash: 'a41f', label: 'prompt', value: PROMPT },
  {
    bytes: 36,
    call: 2,
    hash: '19be',
    label: 'ticket',
    value: '"Where is my order?"',
  },
  { bytes: 1200, call: 3, hash: 'a41f', label: 'prompt', value: PROMPT },
  {
    bytes: 33,
    call: 3,
    hash: 'c2d4',
    label: 'ticket',
    value: '"Cancel my plan."',
  },
];

const isNew = (i: number) =>
  SENT.findIndex((s) => s.hash === SENT[i].hash) === i;

function Repeated({ step }: { step: number }) {
  const seen = SENT.slice(0, step);
  const stored = seen.filter((_, i) => isNew(i));
  // The row that just matched an existing hash, to flash it.
  const hit = step > 0 && !isNew(step - 1) ? SENT[step - 1].hash : null;
  const sent = seen.reduce((sum, s) => sum + s.bytes, 0);
  const billed = stored.reduce((sum, s) => sum + s.bytes, 0);
  return (
    <Scene
      billed={billed}
      left={
        <Pane title="What your program sends">
          {SENT.map((s, i) => {
            const on = i < step;
            return (
              <div
                className="flex items-center justify-between gap-3 transition-opacity duration-500"
                key={`${s.call}-${s.label}`}
                style={{ opacity: on ? 1 : 0.25 }}
              >
                <span className="min-w-0 truncate whitespace-pre">
                  <span className="text-muted">
                    {s.label === 'prompt' ? `call ${s.call}  ` : '        '}
                    {s.label}:{' '}
                  </span>
                  <Chip billed={isNew(i)} on={on}>
                    {s.value}
                  </Chip>
                </span>
                <span
                  className={`shrink-0 font-sans text-[12px] transition-opacity duration-300 ${isNew(i) ? 'text-clover' : 'text-muted'}`}
                  style={{ opacity: on ? 1 : 0 }}
                >
                  {isNew(i) ? `+${size(s.bytes)}` : 'already stored'}
                </span>
              </div>
            );
          })}
        </Pane>
      }
      right={
        <Pane title="What we store, by content hash">
          {stored.length === 0 && <p className="text-muted">empty</p>}
          {stored.map((s) => (
            <div
              className={`flex items-center justify-between gap-3 rounded px-1 transition-colors duration-300 ${hit === s.hash ? 'bg-white/10' : ''}`}
              key={s.hash}
            >
              <span className="min-w-0 truncate">
                <span className="text-muted">#{s.hash}</span>{' '}
                <span className="text-ink">{s.value}</span>
              </span>
              <span className="shrink-0 font-sans text-[12px] text-muted">
                {hit === s.hash ? 'match' : size(s.bytes)}
              </span>
            </div>
          ))}
        </Pane>
      }
      say="Every value is stored under a hash of its content. Send the same prompt again and it's already there, so it isn't billed again."
      sent={sent}
    />
  );
}

// ---------- field names ----------

const FIELDS = [
  { bytes: 4, name: 'ticket_id', value: '4812' },
  { bytes: 11, name: 'customer_email', value: '"sam@acme.co"' },
  { bytes: 4, name: 'priority', value: 'High' },
  { bytes: 21, name: 'body', value: '"My refund came twice."' },
];
const LONG = 'customer_email_address_on_file';
const FIELD_STEPS = FIELDS.length + 1;

function Fields({ step }: { step: number }) {
  const renamed = step > FIELDS.length;
  const name = (i: number) =>
    renamed && i === 1 ? LONG : (FIELDS[i]?.name ?? '');
  const billed = FIELDS.slice(0, step).reduce((sum, f) => sum + f.bytes, 0);
  return (
    <Scene
      billed={billed}
      left={
        <Pane title="What your program records">
          <div className="text-muted">Ticket {'{'}</div>
          {FIELDS.map((f, i) => {
            const on = i < step;
            return (
              <div
                className="flex items-center justify-between gap-3 transition-opacity duration-500"
                key={f.name}
                style={{ opacity: on ? 1 : 0.25 }}
              >
                <span className="min-w-0 truncate whitespace-pre">
                  <span
                    className={renamed && i === 1 ? 'text-ink-2' : 'text-muted'}
                  >
                    {'  '}
                    {name(i)}:{' '}
                  </span>
                  <Chip billed on={on}>
                    {f.value}
                  </Chip>
                </span>
                <span
                  className="shrink-0 font-sans text-[12px] text-clover transition-opacity duration-300"
                  style={{ opacity: on ? 1 : 0 }}
                >
                  +{f.bytes} B
                </span>
              </div>
            );
          })}
          <div className="text-muted">{'}'}</div>
        </Pane>
      }
      right={
        <Pane title="What we store">
          <p className="font-sans text-[11px] text-muted uppercase tracking-[0.08em]">
            Schema, once, free
          </p>
          <div className="text-muted">class Ticket {'{'}</div>
          {FIELDS.map((f, i) => (
            <div className="whitespace-pre text-muted" key={f.name}>
              {'  '}
              <span className="text-ink-2">{i + 1}</span>{' '}
              <span
                className={`transition-colors duration-500 ${renamed && i === 1 ? 'text-clover' : ''}`}
              >
                {name(i)}
              </span>
            </div>
          ))}
          <div className="text-muted">{'}'}</div>
          <p className="mt-2 font-sans text-[11px] text-muted uppercase tracking-[0.08em]">
            The value, billed
          </p>
          <div className="truncate">
            <span className="text-muted">Ticket {'{ '}</span>
            {FIELDS.slice(0, step).map((f, i) => (
              <span key={f.name}>
                <span className="text-ink-2">{i + 1}:</span>
                <span className="text-clover">{f.value}</span>
                {i < Math.min(step, FIELDS.length) - 1 && (
                  <span className="text-muted">, </span>
                )}
              </span>
            ))}
            <span className="text-muted">{' }'}</span>
          </div>
        </Pane>
      }
      say={
        renamed
          ? 'Renamed to customer_email_address_on_file: the schema changes once, the value doesn’t, and neither does the bill.'
          : 'The schema is stored once and never billed. Each value points at it with short IDs, so you pay for 4812, not for ticket_id.'
      }
    />
  );
}

// ---------- the frame ----------

function Scene({
  billed,
  left,
  right,
  say,
  sent,
}: {
  billed: number;
  left: ReactNode;
  right: ReactNode;
  say: string;
  sent?: number;
}) {
  return (
    <>
      <div className="grid divide-rule md:grid-cols-2 md:divide-x max-md:divide-y">
        {left}
        {right}
      </div>
      <div className="grid gap-4 border-t border-rule bg-white/[0.02] px-5 py-4 sm:grid-cols-[auto_minmax(0,1fr)] sm:items-center sm:gap-8">
        <div>
          <p className="text-[12px] text-muted">Billed</p>
          <p className="mt-0.5 font-display text-[32px] leading-none font-medium text-clover tabular-nums">
            {size(billed)}
          </p>
          {sent !== undefined && (
            <p className="mt-1 text-[12px] text-muted tabular-nums">
              of {size(sent)} sent
            </p>
          )}
        </div>
        <p className="text-[14px] text-head">{say}</p>
      </div>
    </>
  );
}

const SCENES = [
  { steps: SENT.length, title: 'Repeated values', View: Repeated },
  { steps: FIELD_STEPS, title: 'Field names', View: Fields },
];

/** The two scenes as tabs: they play in turn while on screen, and a click jumps to one. */
export function Meter() {
  const ref = useRef<HTMLDivElement>(null);
  const visible = useVisible(ref);
  const reduce = useReducedMotion();
  const [scene, setScene] = useState(0);
  const [step, setStep] = useState(0);
  const last = SCENES[scene].steps;

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
      step === 0 ? 600 : step >= last ? HOLD_MS : STEP_MS,
    );
    return () => clearTimeout(timer);
  }, [step, last, visible, reduce]);

  const View = SCENES[scene].View;
  return (
    <div
      className="overflow-hidden rounded-lg border border-rule bg-raised"
      ref={ref}
    >
      <div className="flex gap-1 border-b border-rule px-2 pt-2">
        {SCENES.map((sc, i) => (
          <button
            className={`relative cursor-pointer whitespace-nowrap px-3 pt-1.5 pb-2.5 text-[13px] transition-colors ${i === scene ? 'text-head' : 'text-muted hover:text-ink'}`}
            key={sc.title}
            onClick={() => {
              setScene(i);
              setStep(reduce ? sc.steps : 0);
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
      <View step={step} />
    </div>
  );
}
