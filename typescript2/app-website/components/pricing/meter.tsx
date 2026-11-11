'use client';

import { Pause, Play, RotateCcw } from 'lucide-react';
import {
  type ReactNode,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from 'react';
import { LAMB_PATHS } from '@/components/home/lamb';
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

const size = (bytes: number) =>
  bytes >= 1_000_000
    ? `${(bytes / 1_000_000).toFixed(2)} MB`
    : bytes >= 1000
      ? `${(bytes / 1000).toFixed(1)} KB`
      : `${bytes} B`;

// Billed is purple, free is green, both bright: what you don't pay for should be as easy to see as
// what you do.
const FREE = '#4ade80';
const FREE_BG = 'rgba(74,222,128,0.14)';

/** A recorded value: purple when it's billed, green when it's free. */
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
      className="rounded px-1 transition-[background-color,color] duration-500"
      style={{
        background: !on
          ? 'transparent'
          : billed
            ? 'rgba(202,170,242,0.18)'
            : FREE_BG,
        color: !on ? 'var(--color-ink)' : billed ? 'var(--color-clover)' : FREE,
      }}
    >
      {children}
    </span>
  );
}

/** The tag at the end of a line: the price, or "free" in green. */
function Tag({
  bytes,
  on,
  text,
}: {
  bytes?: number;
  on: boolean;
  text?: string;
}) {
  return (
    <span
      className="shrink-0 font-sans text-[12px] transition-opacity duration-300"
      style={{
        color: bytes ? 'var(--color-clover)' : FREE,
        opacity: on ? 1 : 0,
      }}
    >
      {bytes ? `+${size(bytes)}` : (text ?? 'free')}
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
      free={sent - billed}
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
                <Tag
                  bytes={isNew(i) ? s.bytes : undefined}
                  on={on}
                  text="free, already stored"
                />
              </div>
            );
          })}
        </Pane>
      }
      right={
        <Pane title="What we store, by fingerprint">
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
                {hit === s.hash ? 'match, free' : size(s.bytes)}
              </span>
            </div>
          ))}
        </Pane>
      }
      say="We fingerprint every value by its content. Send the same prompt again and the fingerprint matches what’s stored, so it isn’t billed again."
    />
  );
}

// ---------- repeated images ----------

/** The images, drawn rather than loaded, each unlike the others so a repeat is easy to spot: our
 *  lamb, a photo of a damaged box, a sales chart and an error screenshot. */
const PICS = {
  chart: { bytes: 160_000, hash: '0dc1', label: 'q3-sales.png' },
  error: { bytes: 240_000, hash: 'f48e', label: 'error.png' },
  logo: { bytes: 48_000, hash: '3e9a', label: 'boundary-logo.png' },
  photo: { bytes: 820_000, hash: 'b752', label: 'damaged-box.jpg' },
} as const;
type Pic = keyof typeof PICS;

// Each call sends an array of images; every image is a step. The logo rides along on every call,
// and the photo is sent again in the third.
const CALLS: Pic[][] = [
  ['logo', 'photo'],
  ['logo', 'chart'],
  ['logo', 'photo', 'error'],
];
const SHOTS = CALLS.flatMap((images, call) =>
  images.map((pic) => ({ call, pic })),
);

function Picture({ pic }: { pic: Pic }) {
  switch (pic) {
    case 'logo':
      return (
        <svg aria-hidden="true" className="size-full" viewBox="-80 -60 516 542">
          <rect fill="#7c5cd6" height="542" width="516" x="-80" y="-60" />
          {LAMB_PATHS.map((d) => (
            <path d={d} fill="#fff" key={d.slice(0, 24)} />
          ))}
        </svg>
      );
    case 'photo':
      return (
        <svg aria-hidden="true" className="size-full" viewBox="0 0 48 48">
          <rect fill="#7dd3fc" height="48" width="48" />
          <rect fill="#65a30d" height="12" width="48" y="36" />
          <path d="M12 38V24l12-6 12 6v14z" fill="#d6a35c" />
          <path
            d="M12 24l12 6 12-6M24 30v8"
            fill="none"
            stroke="#8a5a20"
            strokeWidth="1.5"
          />
          <path
            d="M29 20l3 6-4 2"
            fill="none"
            stroke="#7c2d12"
            strokeWidth="2"
          />
        </svg>
      );
    case 'chart':
      return (
        <svg aria-hidden="true" className="size-full" viewBox="0 0 48 48">
          <rect fill="#f8fafc" height="48" width="48" />
          <rect fill="#22c55e" height="10" width="6" x="9" y="30" />
          <rect fill="#22c55e" height="18" width="6" x="18" y="22" />
          <rect fill="#22c55e" height="26" width="6" x="27" y="14" />
          <rect fill="#16a34a" height="32" width="6" x="36" y="8" />
          <rect fill="#94a3b8" height="1.5" width="36" x="7" y="40" />
        </svg>
      );
    default:
      return (
        <svg aria-hidden="true" className="size-full" viewBox="0 0 48 48">
          <rect fill="#1f1f23" height="48" width="48" />
          <rect fill="#2c2c33" height="7" width="48" />
          <circle cx="5" cy="3.5" fill="#ef4444" r="1.5" />
          <circle cx="10" cy="3.5" fill="#eab308" r="1.5" />
          <circle cx="24" cy="24" fill="#ef4444" r="9" />
          <path d="M20 20l8 8M28 20l-8 8" stroke="#fff" strokeWidth="2.5" />
          <rect fill="#ef4444" height="2" rx="1" width="22" x="13" y="38" />
        </svg>
      );
  }
}

/** One image as a thumbnail. A repeat gets the same picture, dimmed, with a ring in the stored
 *  copy's color so the match is visible. */
function Tile({ pic, ring, dim }: { dim?: boolean; pic: Pic; ring?: boolean }) {
  return (
    <span
      className={`block size-11 shrink-0 overflow-hidden rounded-md transition-[opacity,box-shadow] duration-500 ${ring ? 'ring-2 ring-clover' : 'ring-1 ring-white/15'}`}
      style={{ opacity: dim ? 0.45 : 1 }}
    >
      <Picture pic={pic} />
    </span>
  );
}

function Images({ step }: { step: number }) {
  const seen = SHOTS.slice(0, step);
  const first = (i: number) =>
    SHOTS.findIndex((s) => s.pic === SHOTS[i].pic) === i;
  const stored = seen.filter((_, i) => first(i)).map((s) => s.pic);
  const hit = step > 0 && !first(step - 1) ? SHOTS[step - 1].pic : null;
  const sent = seen.reduce((sum, s) => sum + PICS[s.pic].bytes, 0);
  const billed = stored.reduce((sum, p) => sum + PICS[p].bytes, 0);
  // When an image matches one already stored, an arrow runs from it to the stored copy.
  const box = useRef<HTMLDivElement>(null);
  const [arrow, setArrow] = useState<{
    hash: string;
    x1: number;
    x2: number;
    y1: number;
    y2: number;
  } | null>(null);
  useLayoutEffect(() => {
    const el = box.current;
    if (!el || !hit) {
      setArrow(null);
      return;
    }
    const from = el.querySelector(`[data-shot="${step - 1}"]`);
    const to = el.querySelector(`[data-store="${hit}"]`);
    if (!from || !to) return;
    const o = el.getBoundingClientRect();
    const a = from.getBoundingClientRect();
    const b = to.getBoundingClientRect();
    setArrow({
      hash: PICS[hit].hash,
      x1: a.right - o.left + 4,
      x2: b.left - o.left - 6,
      y1: a.top + a.height / 2 - o.top,
      y2: b.top + b.height / 2 - o.top,
    });
  }, [hit, step]);
  let n = 0;
  return (
    <div className="relative" ref={box}>
      {arrow && (
        <svg
          aria-hidden="true"
          className="pointer-events-none absolute inset-0 z-10 size-full overflow-visible"
        >
          {/* The dashes run toward the stored copy, so the arrow reads as a push into the store. */}
          <style>
            {
              '.match-path{animation:match-dash .7s linear infinite}@keyframes match-dash{to{stroke-dashoffset:-8}}@media (prefers-reduced-motion:reduce){.match-path{animation:none}}'
            }
          </style>
          <defs>
            <marker
              id="match-head"
              markerHeight="8"
              markerWidth="8"
              orient="auto"
              refX="6"
              refY="4"
            >
              <path d="M0 0 8 4 0 8z" fill="var(--color-clover)" />
            </marker>
          </defs>
          <path
            className="match-path"
            d={`M${arrow.x1} ${arrow.y1} C${(arrow.x1 + arrow.x2) / 2} ${arrow.y1}, ${(arrow.x1 + arrow.x2) / 2} ${arrow.y2}, ${arrow.x2} ${arrow.y2}`}
            fill="none"
            key={`${arrow.x1}-${arrow.y1}`}
            markerEnd="url(#match-head)"
            stroke="var(--color-clover)"
            strokeDasharray="4 4"
            strokeWidth="1.5"
          />
          <foreignObject
            height="24"
            width="220"
            x={(arrow.x1 + arrow.x2) / 2 - 110}
            y={(arrow.y1 + arrow.y2) / 2 - 12}
          >
            <span className="flex h-full items-center justify-center">
              <span className="whitespace-nowrap rounded-full bg-clover px-2 py-0.5 font-sans text-[11px] font-medium text-black">
                same fingerprint #{arrow.hash} · $0
              </span>
            </span>
          </foreignObject>
        </svg>
      )}
      <Scene
        billed={billed}
        free={sent - billed}
        left={
          <Pane title="What your program sends">
            {CALLS.map((images, call) => (
              <div
                className="flex items-center gap-4 pt-1 pb-9"
                // biome-ignore lint/suspicious/noArrayIndexKey: fixed calls
                key={call}
              >
                <span className="w-12 shrink-0 text-muted">
                  call {call + 1}
                </span>
                <span className="text-muted">[</span>
                {images.map((pic) => {
                  const i = n++;
                  const on = i < step;
                  const fresh = first(i);
                  return (
                    <span
                      className="relative transition-opacity duration-500"
                      data-shot={i}
                      key={`${call}-${pic}`}
                      style={{ opacity: on ? 1 : 0.2 }}
                    >
                      <Tile
                        dim={on && !fresh}
                        pic={pic}
                        ring={on && !fresh && hit === pic}
                      />
                      <span
                        className="absolute top-full left-1/2 mt-1 -translate-x-1/2 text-center font-sans text-[10px] leading-tight transition-opacity"
                        style={{
                          color: fresh ? 'var(--color-clover)' : FREE,
                          opacity: on ? 1 : 0,
                        }}
                      >
                        <span className="block font-mono text-ink-2">
                          #{PICS[pic].hash}
                        </span>
                        <span className="block whitespace-nowrap">
                          {fresh ? `+${size(PICS[pic].bytes)}` : 'free'}
                        </span>
                      </span>
                    </span>
                  );
                })}
                <span className="text-muted">]</span>
              </div>
            ))}
          </Pane>
        }
        right={
          <Pane title="What we store, by fingerprint">
            {stored.length === 0 && <p className="text-muted">empty</p>}
            {stored.map((pic) => (
              <div
                className={`flex items-center justify-between gap-3 rounded px-1 py-0.5 transition-colors duration-300 ${hit === pic ? 'bg-white/10' : ''}`}
                key={pic}
              >
                <span className="flex min-w-0 items-center gap-3">
                  <span data-store={pic}>
                    <Tile pic={pic} ring={hit === pic} />
                  </span>
                  <span className="truncate">
                    <span className="text-muted">#{PICS[pic].hash}</span>{' '}
                    <span className="text-ink">{PICS[pic].label}</span>
                  </span>
                </span>
                <span className="shrink-0 font-sans text-[12px] text-muted">
                  {hit === pic ? 'match, free' : size(PICS[pic].bytes)}
                </span>
              </div>
            ))}
          </Pane>
        }
        say="We fingerprint every image by its content. The same logo on every call has the same fingerprint, so it’s stored once and billed once."
      />
    </div>
  );
}

// ---------- what you pay for ----------

// Everything one call records, in the order it lands. Free lines are the call's identity and timing
// and the type it uses; billed lines are a field's value, and that field's name rides along free.
// The free sizes are rough: enough to show how much comes along at no charge.
type Rec =
  | { bytes: number; key: string; kind: 'call' }
  | {
      bytes: number;
      kind: 'field';
      name: string;
      nameBytes: number;
      value: string;
    };

const RECORD: Rec[] = [
  { bytes: 19, key: 'function  draft_reply(ticket)', kind: 'call' },
  { bytes: 16, key: 'started   10:42:07.118', kind: 'call' },
  { bytes: 8, key: 'took      1.84s', kind: 'call' },
  { bytes: 64, key: 'type      class Ticket { … }', kind: 'call' },
  { bytes: 4, kind: 'field', name: 'ticket_id', nameBytes: 9, value: '4812' },
  {
    bytes: 11,
    kind: 'field',
    name: 'customer_email',
    nameBytes: 14,
    value: '"sam@acme.co"',
  },
  { bytes: 4, kind: 'field', name: 'priority', nameBytes: 8, value: 'High' },
  {
    bytes: 21,
    kind: 'field',
    name: 'body',
    nameBytes: 4,
    value: '"My refund came twice."',
  },
];
// One step per record.
const COUNT_STEPS = RECORD.length;

function Counts({ step }: { step: number }) {
  const seen = RECORD.slice(0, step);
  const billed = seen.reduce(
    (sum, r) => sum + (r.kind === 'field' ? r.bytes : 0),
    0,
  );
  const free = seen.reduce(
    (sum, r) => sum + (r.kind === 'call' ? r.bytes : r.nameBytes),
    0,
  );
  const fieldsSeen = seen.filter((r) => r.kind === 'field');
  return (
    <Scene
      billed={billed}
      free={free}
      left={
        <Pane title="What one call records">
          {RECORD.map((r, i) => {
            const on = i < step;
            if (r.kind === 'call')
              return (
                <div
                  className="flex items-center justify-between gap-3 transition-opacity duration-500"
                  key={r.key}
                  style={{ opacity: on ? 1 : 0.25 }}
                >
                  <span className="min-w-0 truncate whitespace-pre">
                    <Chip billed={false} on={on}>
                      {r.key}
                    </Chip>
                  </span>
                  <Tag on={on} />
                </div>
              );
            const name = r.name;
            return (
              <div
                className="flex items-center justify-between gap-3 transition-opacity duration-500"
                key={r.name}
                style={{ opacity: on ? 1 : 0.25 }}
              >
                <span className="min-w-0 truncate whitespace-pre">
                  <Chip billed={false} on={on}>
                    {`${name}:`}
                  </Chip>{' '}
                  <Chip billed on={on}>
                    {r.value}
                  </Chip>
                </span>
                <Tag bytes={r.bytes} on={on} />
              </div>
            );
          })}
        </Pane>
      }
      right={
        <Pane title="What we store">
          <p
            className="font-sans text-[11px] uppercase tracking-[0.08em]"
            style={{ color: FREE }}
          >
            Free
          </p>
          <ul className="font-sans text-[13px] leading-relaxed text-ink-2">
            {[
              ['The call: name, start, duration', step >= 3],
              ['The type, stored once for every call', step >= 4],
              ['Field names, as short IDs', fieldsSeen.length > 0],
            ].map(([text, on]) => (
              <li
                className="flex items-center gap-2 transition-opacity duration-500"
                key={String(text)}
                style={{ opacity: on ? 1 : 0.25 }}
              >
                <span style={{ color: FREE }}>✓</span>
                {text}
              </li>
            ))}
          </ul>
          <p className="mt-4 font-sans text-[11px] uppercase tracking-[0.08em] text-clover">
            Billed
          </p>
          <div className="break-words">
            <span className="text-muted">{'{ '}</span>
            {fieldsSeen.map((r, i) => (
              <span key={r.kind === 'field' ? r.name : i}>
                <span className="text-ink-2">{i + 1}:</span>
                <span className="text-clover">
                  {r.kind === 'field' ? r.value : ''}
                </span>
                {i < fieldsSeen.length - 1 && (
                  <span className="text-muted">, </span>
                )}
              </span>
            ))}
            <span className="text-muted">{' }'}</span>
          </div>
        </Pane>
      }
      say="You pay for the values. The call’s name, timing and types, and every field name, come along free."
    />
  );
}

// ---------- the frame ----------

function Scene({
  billed,
  free,
  left,
  right,
  say,
}: {
  billed: number;
  free: number;
  left: ReactNode;
  right: ReactNode;
  say: string;
}) {
  return (
    <>
      <div className="grid divide-rule md:min-h-[330px] md:grid-cols-2 md:divide-x max-md:divide-y">
        {left}
        {right}
      </div>
      <div className="grid grid-cols-2 gap-4 border-t border-rule bg-white/[0.02] px-5 py-4 sm:grid-cols-[9.5rem_9.5rem_minmax(0,1fr)] sm:items-center sm:gap-6">
        <div>
          <p className="text-[12px] text-muted">Billed</p>
          <p className="mt-0.5 font-display text-[32px] leading-none font-medium text-clover tabular-nums">
            {size(billed)}
          </p>
        </div>
        <div>
          <p className="text-[12px] text-muted">Free</p>
          <p
            className="mt-0.5 font-display text-[32px] leading-none font-medium tabular-nums"
            style={{ color: FREE }}
          >
            {size(free)}
          </p>
        </div>
        {/* Fixed widths and height, so nothing shifts as the numbers count up or the line changes. */}
        <p className="col-span-2 flex min-h-[3.2em] items-center text-[14px] text-head sm:col-span-1">
          {say}
        </p>
      </div>
    </>
  );
}

const SCENES = [
  { steps: COUNT_STEPS, title: 'What you pay for', View: Counts },
  { steps: SHOTS.length, title: 'Images', View: Images },
  { steps: SENT.length, title: 'Repeated values', View: Repeated },
];

/** The two scenes as tabs: they play in turn while on screen, and a click jumps to one. */
export function Meter() {
  const ref = useRef<HTMLDivElement>(null);
  const visible = useVisible(ref);
  const reduce = useReducedMotion();
  const [scene, setScene] = useState(0);
  const [step, setStep] = useState(0);
  // Plays a scene once, then stops on it so it can be read; a click pauses or plays it.
  const [playing, setPlaying] = useState(true);
  const current = SCENES[scene] ?? SCENES[0];
  const last = current.steps;
  const done = step >= last;

  useEffect(() => {
    if (reduce) {
      setStep(last);
      return;
    }
    if (!visible || !playing || done) return;
    const timer = setTimeout(
      () => setStep((n) => n + 1),
      step === 0 ? 600 : STEP_MS,
    );
    return () => clearTimeout(timer);
  }, [step, last, visible, reduce, playing, done]);

  const go = (i: number) => {
    setScene(i);
    setStep(reduce ? SCENES[i].steps : 0);
    setPlaying(true);
  };
  const next = SCENES[scene + 1];

  const View = current.View;
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
            onClick={() => go(i)}
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
      {/* A click anywhere on the scene pauses it, or plays it again. */}
      {/* biome-ignore lint/a11y/useKeyWithClickEvents: the controls below do the same from the keyboard */}
      <div
        className="cursor-pointer"
        onClick={() => {
          if (done) setStep(0);
          setPlaying((p) => (done ? true : !p));
        }}
      >
        <View step={step} />
      </div>
      <div className="flex items-center gap-3 border-t border-rule px-4 py-2.5 text-[12.5px]">
        <button
          aria-label={done ? 'Replay' : playing ? 'Pause' : 'Play'}
          className="focus-ring grid size-7 cursor-pointer place-items-center rounded-full bg-white/[0.06] text-ink hover:bg-white/10"
          onClick={() => {
            if (done) setStep(0);
            setPlaying((p) => (done ? true : !p));
          }}
          type="button"
        >
          {done ? (
            <RotateCcw className="size-3.5" />
          ) : playing ? (
            <Pause className="size-3.5" />
          ) : (
            <Play className="size-3.5" />
          )}
        </button>
        {/* One dot a step: click one to jump there and stop. */}
        <div className="flex flex-1 items-center gap-1.5">
          {Array.from({ length: last }, (_, i) => (
            <button
              aria-label={`Step ${i + 1}`}
              className="h-1.5 flex-1 cursor-pointer rounded-full transition-colors"
              // biome-ignore lint/suspicious/noArrayIndexKey: steps
              key={i}
              onClick={() => {
                setStep(i + 1);
                setPlaying(false);
              }}
              style={{
                background:
                  i < step ? 'var(--color-clover)' : 'rgba(255,255,255,0.1)',
              }}
              type="button"
            />
          ))}
        </div>
        {next ? (
          <button
            className="cursor-pointer whitespace-nowrap text-ink-2 transition-colors hover:text-head"
            onClick={() => go(scene + 1)}
            type="button"
          >
            Next: {next.title} →
          </button>
        ) : (
          <button
            className="cursor-pointer whitespace-nowrap text-ink-2 transition-colors hover:text-head"
            onClick={() => go(0)}
            type="button"
          >
            Start over
          </button>
        )}
      </div>
    </div>
  );
}
