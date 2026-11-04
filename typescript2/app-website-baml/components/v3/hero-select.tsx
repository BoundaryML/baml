'use client';

import { useEffect, useRef, useState } from 'react';
import { proseLines } from './code';
import { Line, type Mark } from './hero-tabs';
import { HIGHLIGHT, RED } from './story';
import { clamp, easeInOut, type Frame, useCanvas } from './use-canvas';

// Production calls stream out of a BAML file, the way they do in "Failures come back", drawn as spans:
// each one is as wide as the call took, and the ones that failed are red from the start. They run on fixed
// lanes, and the shape button draws them as spans, round dots, or square pixels. A terminal beside the stream runs one baml query
// at a time. Each query lights up the code it is about, picks out the calls it matches, and pulls them into
// a small widget that it adds under the terminal, where the count goes up as each one lands. After four
// queries the widgets clear and it starts again.

// triage.baml as baml fmt writes it.
const FILE = [
  'type Label = "billing" | "bug" | "other";',
  '',
  'function classify(ticket: string) -> Label {',
  '    client: "openai/gpt-6-luna"',
  '    prompt: `',
  '        Classify this support ticket.',
  // biome-ignore lint/suspicious/noTemplateCurlyInString: BAML source, shown as text
  '        ${ticket}',
  '    `',
  '}',
  '',
  'function reply(ticket: string, label: Label) -> string {',
  '    client: "openai/gpt-6.1-sol"',
  '    prompt: `',
  // biome-ignore lint/suspicious/noTemplateCurlyInString: BAML source, shown as text
  '        Write a short reply to this ${label} ticket.',
  // biome-ignore lint/suspicious/noTemplateCurlyInString: BAML source, shown as text
  '        ${ticket}',
  '    `',
  '}',
  '',
  'function handle(ticket: string) -> string {',
  '    let label = classify(ticket);',
  '    if (label == "bug") {',
  '        return "Sent to engineering.";',
  '    }',
  '    return reply(ticket, label);',
  '}',
];
const PROSE = proseLines(FILE);

const QUERIES: {
  bad?: boolean;
  /** Matching calls recorded before the demo starts. */
  base: number;
  command: string;
  label: string;
  marks: Mark[];
}[] = [
  {
    bad: true,
    base: 31,
    command: `baml query --watch "SELECT * FROM spans WHERE span_name = 'classify' AND output_value = 'other'"`,
    label: 'Labelled other',
    marks: [
      { line: 2, text: 'classify' },
      { bad: true, line: 0, text: '"other"' },
    ],
  },
  {
    base: 318,
    command: `baml query --watch "SELECT * FROM spans WHERE span_name = 'classify' AND output_value = 'bug'"`,
    label: 'Labelled bug',
    marks: [
      { line: 0, text: '"bug"' },
      { line: 20, text: '"bug"' },
      { line: 21, text: '"Sent to engineering."' },
    ],
  },
  {
    base: 12,
    command: `baml query --watch "SELECT * FROM spans WHERE span_name = 'reply' AND duration > 3e9"`,
    label: 'Replies over 3s',
    marks: [
      { line: 10, text: 'reply' },
      { line: 11, text: '"openai/gpt-6.1-sol"' },
      { line: 23, text: 'reply(ticket, label)' },
    ],
  },
  {
    bad: true,
    base: 4,
    command: `baml query --watch "SELECT * FROM spans WHERE status != 'return'"`,
    label: 'Failed calls',
    marks: [{ bad: true, line: 2, text: 'Label' }],
  },
];

const STEP_MS = 7500;
const LAP_MS = STEP_MS * QUERIES.length;
const TYPE_MS = 900;
/** When a query starts picking calls, and when it stops, within its step. */
const PICK_FROM = TYPE_MS + 250;
const PICK_TO = STEP_MS - 1000;
/** Space between lanes when calls are spread out, and from the lanes to the edges of the stream. */
const LANE_GAP = 26;
const INSET = 26;
/** The room a dot or a pixel takes on its lane. */
const DOT_W = 4;
/** How long a call takes to cross the stream. */
const CROSS_MS = 11000;
const HOLD_MS = 450;
const FLY_MS = 1100;
/** How tall a span is drawn. */
const SPAN_H = 8;
/** Calls in the stream are clover: a faint fill under a fine outline for spans, solid for dots. */
const CLOVER = '202 170 242';
const RED_RGB = '229 72 77';

/** One production call, drawn as a span: its width is how long it took. */
type Span = {
  at: number;
  from: { x: number; y: number };
  /** For each query, whether this call matches it. The last one, a failed call, is red from the start. */
  match: boolean[];
  /** How far across the stream this call is picked, from 0 to 1. */
  pick: number;
  state: 'out' | 'picked' | 'fly';
  target: number;
  w: number;
  /** The span's left edge while it streams, and its center once it is picked. */
  x: number;
  y: number;
};

/** A new call: which queries it matches, and a width from its duration. Slow replies are the long ones. */
function newCall() {
  const r = Math.random();
  const slow = Math.random() < 0.15;
  const match = [r < 0.1, r >= 0.1 && r < 0.38, slow, Math.random() < 0.08];
  const w = slow ? 54 + Math.random() * 24 : 10 + Math.random() * 30;
  return { match, w };
}

const ZERO = QUERIES.map(() => 0);
export default function HeroSelect({
  options,
}: {
  options?: Record<string, string>;
}) {
  const shape = options?.shape ?? 'span';
  const stage = useRef<HTMLDivElement>(null);
  const card = useRef<HTMLDivElement>(null);
  const field = useRef<HTMLDivElement>(null);
  const widgets = useRef<(HTMLDivElement | null)[]>([]);
  const [local, setLocal] = useState(0);
  const [step, setStep] = useState(0);
  const [counts, setCounts] = useState(ZERO);
  const [still, setStill] = useState(false);
  const sim = useRef({
    clock: 0,
    lanes: [] as {
      gap: number;
      next: ReturnType<typeof newCall>;
      tail: number;
    }[],
    lap: 0,
    spans: [] as Span[],
    step: 0,
  });

  useEffect(() => {
    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
      setStill(true);
    }
  }, []);

  // A new shape starts the stream again.
  // biome-ignore lint/correctness/useExhaustiveDependencies: it runs again whenever the shape changes
  useEffect(() => {
    sim.current.spans = [];
    sim.current.lanes = [];
  }, [shape]);

  const draw = ({ ctx, dt, width }: Frame) => {
    const s = sim.current;
    const sr = stage.current?.getBoundingClientRect();
    const cr = card.current?.getBoundingClientRect();
    const fr = field.current?.getBoundingClientRect();
    if (!sr || !cr || !fr || !width) return;
    s.clock += dt;

    const lap = Math.floor(s.clock / LAP_MS);
    const now = Math.floor((s.clock % LAP_MS) / STEP_MS);
    const t = s.clock % STEP_MS;
    if (lap !== s.lap) {
      s.lap = lap;
      setCounts(ZERO);
    }
    if (now !== s.step) {
      s.step = now;
      setStep(now);
    }
    setLocal(t);

    const f = {
      bottom: fr.bottom - sr.top,
      left: fr.left - sr.left,
      top: fr.top - sr.top,
    };
    const middle = (f.top + f.bottom) / 2;
    // From the side of the file on a wide screen, and from the left of the stream on a phone.
    const side = cr.right <= fr.left;
    const origin = side
      ? { x: cr.right - sr.left, y: (cr.top + cr.bottom) / 2 - sr.top }
      : { x: f.left, y: middle };
    const reach = width - origin.x;
    const picking = t >= PICK_FROM && t < PICK_TO;
    const h = SPAN_H;

    // Calls run along fixed lanes. They never change lane,
    // and they all move at the same speed, so on each lane a call comes out only once the one ahead of it
    // has made room.
    const count = Math.max(
      4,
      Math.floor((f.bottom - f.top - 2 * INSET) / LANE_GAP) + 1,
    );
    if (s.lanes.length !== count) {
      s.lanes = Array.from({ length: count }, () => ({
        gap: Math.random() * 200,
        next: newCall(),
        tail: origin.x,
      }));
    }
    const speed = reach / CROSS_MS;
    s.lanes.forEach((l, i) => {
      l.tail += speed * dt;
      // A new call starts under the file and slides out once the one ahead has made room.
      if (l.tail < origin.x + l.gap) return;
      const w = shape === 'span' ? l.next.w : DOT_W;
      s.spans.push({
        at: 0,
        from: { x: 0, y: 0 },
        match: l.next.match,
        pick: 0.08 + Math.random() * 0.72,
        state: 'out',
        target: 0,
        w,
        x: origin.x - w,
        y: f.top + INSET + i * ((f.bottom - f.top - 2 * INSET) / (count - 1)),
      });
      l.tail = origin.x - w;
      l.next = newCall();
      l.gap =
        shape === 'span' ? 24 + Math.random() * 160 : 8 + Math.random() * 70;
    });

    const calm: Span[] = [];
    const lit: Span[] = [];
    s.spans = s.spans.filter((d) => {
      if (d.state === 'out') {
        d.x += speed * dt;
        const center = d.x + d.w / 2;
        if (
          picking &&
          d.match[now] &&
          center > f.left + (width - f.left) * d.pick
        ) {
          d.state = 'picked';
          d.at = s.clock;
          d.target = now;
          d.x = center;
          d.from = { x: center, y: d.y };
        }
        if (d.x > width + 8) return false;
        if (d.state === 'out') calm.push(d);
        else lit.push(d);
        return true;
      }
      if (d.state === 'picked') {
        if (s.clock - d.at > HOLD_MS) {
          d.state = 'fly';
          d.at = s.clock;
          d.from = { x: d.x, y: d.y };
        }
        lit.push(d);
        return true;
      }
      const wr = widgets.current[d.target]?.getBoundingClientRect();
      if (!wr || !wr.width) return false;
      const to = {
        x: wr.left + wr.width / 2 - sr.left,
        y: wr.top + wr.height / 2 - sr.top,
      };
      const u = easeInOut(clamp((s.clock - d.at) / FLY_MS));
      const ctl = { x: to.x, y: d.from.y };
      d.x = (1 - u) ** 2 * d.from.x + 2 * (1 - u) * u * ctl.x + u * u * to.x;
      d.y = (1 - u) ** 2 * d.from.y + 2 * (1 - u) * u * ctl.y + u * u * to.y;
      if (u >= 1) {
        const q = d.target;
        setCounts((c) => c.map((n, i) => (i === q ? n + 1 : n)));
        return false;
      }
      lit.push(d);
      return true;
    });

    // Draws one call centered on (x, y): a span as wide as it took, a round dot, or a square pixel.
    const mark = (x: number, y: number, w: number, size: number) => {
      if (shape === 'dot') {
        ctx.moveTo(x + size / 2, y);
        ctx.arc(x, y, size / 2, 0, Math.PI * 2);
      } else if (shape === 'pixel') {
        ctx.rect(
          Math.round(x - size / 2),
          Math.round(y - size / 2),
          size,
          size,
        );
      } else {
        // Snapped to the pixel grid so the 1px outline stays sharp.
        ctx.rect(
          Math.round(x - w / 2) + 0.5,
          Math.round(y - h / 2) + 0.5,
          Math.max(1, Math.round(w) - 1),
          h - 1,
        );
      }
    };
    // The outline on a call while it is held after being picked.
    const ring = (x: number, y: number, w: number) => {
      if (shape === 'dot') {
        ctx.moveTo(x + 7, y);
        ctx.arc(x, y, 7, 0, Math.PI * 2);
      } else if (shape === 'pixel') {
        ctx.rect(Math.round(x) - 6.5, Math.round(y) - 6.5, 13, 13);
      } else {
        ctx.rect(
          Math.round(x - w / 2) - 3.5,
          Math.round(y - h / 2) - 3.5,
          Math.round(w) + 7,
          h + 7,
        );
      }
    };

    // Calls in the stream: clover, or red when they already failed. Spans are a faint fill under a fine
    // outline; dots and pixels are solid. Flat, no blur.
    for (const failed of [false, true]) {
      const rgb = failed ? RED_RGB : CLOVER;
      ctx.beginPath();
      for (const d of calm) {
        if (d.match[3] !== failed) continue;
        mark(d.x + d.w / 2, d.y, d.w, shape === 'pixel' ? 3 : 3.4);
      }
      if (shape === 'span') {
        ctx.fillStyle = `rgb(${rgb} / ${failed ? 0.22 : 0.1})`;
        ctx.fill();
        ctx.strokeStyle = `rgb(${rgb} / ${failed ? 1 : 0.6})`;
        ctx.lineWidth = 1;
        ctx.stroke();
      } else {
        ctx.fillStyle = `rgb(${rgb} / ${failed ? 1 : 0.7})`;
        ctx.fill();
      }
    }

    // Picked calls, centered on x: solid, outlined while they are held, and shrinking as they fly.
    for (const d of lit) {
      const color = QUERIES[d.target].bad ? RED : HIGHLIGHT;
      const flying = d.state === 'fly';
      const w = flying
        ? d.w + (6 - d.w) * easeInOut(clamp((s.clock - d.at) / FLY_MS))
        : d.w;
      ctx.fillStyle = color;
      ctx.beginPath();
      mark(
        d.x,
        d.y,
        w,
        shape === 'pixel' ? (flying ? 4 : 5) : flying ? 5.2 : 6.8,
      );
      ctx.fill();
      if (!flying) {
        ctx.strokeStyle = color;
        ctx.lineWidth = 1;
        ctx.beginPath();
        ring(d.x, d.y, w);
        ctx.stroke();
      }
    }
  };

  const canvas = useCanvas(draw);

  // Under reduced motion every query has run and every widget is there.
  const current = still ? QUERIES.length - 1 : step;
  const elapsed = still ? STEP_MS : local;
  const query = QUERIES[current];
  const typed = query.command.slice(
    0,
    Math.round(clamp(elapsed / TYPE_MS) * query.command.length),
  );
  const live = elapsed >= PICK_FROM;

  return (
    <figure
      aria-label="Production calls stream out of a BAML file. A terminal runs baml query four times: calls labelled other, calls labelled bug, replies slower than three seconds, and failed calls. Each query lights up the code it is about and pulls the calls it matches into a widget with a count."
      className="relative pb-6"
    >
      <div
        className="relative grid lg:grid-cols-[480px_minmax(0,1fr)] gap-x-16 gap-y-6 items-center"
        ref={stage}
      >
        <canvas
          className="pointer-events-none absolute inset-0 z-10 size-full"
          ref={canvas}
        />

        {/* Above the stream, so calls slide out from under it. */}
        <div className="v3-card relative z-20 overflow-hidden" ref={card}>
          <p className="h-10 flex items-center px-4 border-b border-white/10 font-mono text-[12px] text-white/50">
            triage.baml
          </p>
          <div className="py-3 overflow-hidden font-mono text-[10px] leading-[17px] sm:text-[12.5px] sm:leading-[20px] text-[#BDBDC4]">
            {FILE.map((line, i) => (
              <div
                className="flex pr-4"
                // biome-ignore lint/suspicious/noArrayIndexKey: the file's lines keep their positions
                key={i}
              >
                <span className="shrink-0 w-7 sm:w-11 pr-2.5 sm:pr-4 text-right text-white/20 select-none">
                  {i + 1}
                </span>
                <span className="whitespace-pre min-w-0" key={current}>
                  <Line
                    line={line}
                    marks={query.marks.filter((m) => m.line === i)}
                    prose={PROSE.has(i)}
                  />
                </span>
              </div>
            ))}
          </div>
        </div>

        <div className="flex flex-col min-w-0">
          <div className="rounded-xl border border-white/15 bg-[#0e0e10] overflow-hidden font-mono text-[11px] leading-[19px] sm:text-[12.5px] sm:leading-[21px]">
            <div className="flex items-center gap-2 h-9 px-4 border-b border-white/[0.08] text-[12px]">
              <span className="text-white/45">~/triage</span>
              <span className="ml-auto flex gap-1">
                {QUERIES.map((q, i) => (
                  <span
                    className={`size-1.5 rounded-full transition-colors ${i === current ? 'bg-clover' : 'bg-white/20'}`}
                    key={q.label}
                  />
                ))}
              </span>
            </div>
            <div className="px-4 py-3">
              <p className="truncate text-white/90">
                <span className="text-clover">$</span> {typed}
                {!live && (
                  <span className="inline-block w-[7px] h-[13px] ml-px align-[-2px] bg-white/80" />
                )}
              </p>
              {/* A fixed height, so the terminal doesn't grow when the count appears. */}
              <p className="flex items-center gap-2 h-[19px] sm:h-[21px] text-white/45">
                {live ? (
                  <>
                    <span
                      className="size-1.5 rounded-full animate-pulse"
                      style={{ background: query.bad ? RED : HIGHLIGHT }}
                    />
                    <span className="tabular-nums text-white/75">
                      {(query.base + counts[current]).toLocaleString('en-US')}{' '}
                      rows
                    </span>
                    watching
                  </>
                ) : null}
              </p>
            </div>
          </div>

          {/* The stream of calls runs through this band. */}
          <div className="h-[160px] lg:h-[230px]" ref={field} />

          <div className="grid grid-cols-2 sm:grid-cols-4 gap-2">
            {QUERIES.map((q, i) => {
              const shown = i <= current;
              const color = q.bad ? RED : HIGHLIGHT;
              return (
                <div
                  className={`relative rounded-lg border px-3 py-2.5 bg-[#0e0e10] transition-colors ${shown ? 'v3-enter border-white/15' : 'invisible border-transparent'}`}
                  key={`${q.label}-${shown}`}
                  ref={(el) => {
                    widgets.current[i] = shown ? el : null;
                  }}
                >
                  <p className="flex items-center gap-1.5 text-[11.5px] sm:text-[12.5px] text-white/55 truncate">
                    <span
                      className="size-1.5 shrink-0 rounded-[2px]"
                      style={{ background: color }}
                    />
                    {q.label}
                  </p>
                  <p
                    className="mt-0.5 font-mono text-[18px] sm:text-[22px] leading-tight tabular-nums"
                    style={{
                      color: i === current ? color : 'rgb(255 255 255 / 0.85)',
                    }}
                  >
                    {(q.base + (still ? 0 : counts[i])).toLocaleString('en-US')}
                  </p>
                </div>
              );
            })}
          </div>
        </div>
      </div>
    </figure>
  );
}
