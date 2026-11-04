'use client';

import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { RED } from './story';

// One failed call, recorded twice. Without BAML it leaves the app as five copies, one per tool, and each
// tool keeps a piece. With BAML the same call is one record that holds all five. The tools are kinds of
// tools rather than products, and the call is example data.
const TOOLS = [
  {
    color: '#F5A524',
    fragment: 'llm.call, 1.8s',
    missing: 'No prompt or output',
    name: 'Tracing',
  },
  {
    color: '#3EC6E0',
    fragment: 'JSONDecodeError: line 1 column 52',
    missing: 'No model input',
    name: 'Error tracking',
  },
  {
    color: '#F472B6',
    fragment: 'llm_errors_total +1',
    missing: 'No reason',
    name: 'Metrics',
  },
  {
    color: '#A3E635',
    fragment: 'ticket_classified, label: null',
    missing: 'No link to the call',
    name: 'Product analytics',
  },
  {
    color: '#FB923C',
    fragment: 'Error rate above 2%',
    missing: 'Links to a dashboard',
    name: 'Alerts',
  },
];

const RECORD = [
  { label: 'Trace', text: 'classify, 1.8s, prompt and raw output' },
  { label: 'Error', text: 'Verdict.confidence missing from output' },
  { label: 'Metric', text: 'classify errors 2.4%, up since v42' },
  { label: 'Analytics', text: 'Labels since v42: billing down 29%' },
  { label: 'Alert', text: 'Sent to your agent with 412 like it' },
];

const LAP_MS = 13000;
const SPLIT = 300;
const SPLIT_MS = 1300;
const STAGGER = 140;
const JOIN = 5600;
const ONE = 6500;
const ONE_MS = 1400;
const ROW_MS = 160;

// Every moment in a lap where something on the page changes, in order.
const MOMENTS = [
  ...TOOLS.map((_, i) => SPLIT + SPLIT_MS + i * STAGGER),
  JOIN,
  ...RECORD.map((_, i) => ONE + ONE_MS + i * ROW_MS),
];
const ARRIVED = TOOLS.length;
const JOINED = ARRIVED + 1;

const ease = (t: number) => (t < 0.5 ? 2 * t * t : 1 - (-2 * t + 2) ** 2 / 2);

export default function HeroFanout() {
  const stage = useRef<HTMLDivElement>(null);
  const source = useRef<HTMLDivElement>(null);
  const cards = useRef<(HTMLLIElement | null)[]>([]);
  const wires = useRef<(SVGPathElement | null)[]>([]);
  const dots = useRef<(SVGCircleElement | null)[]>([]);
  const [paths, setPaths] = useState<string[]>([]);
  // How many of the moments have passed this lap.
  const [step, setStep] = useState(0);

  // Each wire runs from the call to a tool's card: across on a wide screen, and down the left on a phone.
  useLayoutEffect(() => {
    const measure = () => {
      const box = stage.current?.getBoundingClientRect();
      const from = source.current?.getBoundingClientRect();
      if (!box || !from) return;
      setPaths(
        cards.current.map((card) => {
          const to = card?.getBoundingClientRect();
          if (!to) return '';
          const y = to.top + to.height / 2 - box.top;
          const x = to.left - box.left - 4;
          if (to.top > from.bottom) {
            const sx = from.left - box.left + 18;
            const sy = from.bottom - box.top;
            return `M${sx} ${sy}V${y - 12}Q${sx} ${y} ${sx + 12} ${y}H${x}`;
          }
          const sx = from.right - box.left;
          const sy = from.top + from.height / 2 - box.top;
          const dx = (x - sx) * 0.5;
          return `M${sx} ${sy}C${sx + dx} ${sy} ${x - dx} ${y} ${x} ${y}`;
        }),
      );
    };
    measure();
    const observer = new ResizeObserver(measure);
    if (stage.current) observer.observe(stage.current);
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
      setStep(MOMENTS.length);
      return;
    }
    const start = performance.now();
    let frame = 0;
    let last = -1;
    const tick = (now: number) => {
      const t = (now - start) % LAP_MS;
      const passed = MOMENTS.filter((m) => t >= m).length;
      if (passed !== last) {
        last = passed;
        setStep(passed);
      }
      dots.current.forEach((dot, i) => {
        const wire = wires.current[i];
        if (!dot || !wire) return;
        // Five dots leave together, then only the middle one makes the second trip.
        const first = (t - SPLIT - i * STAGGER) / SPLIT_MS;
        const second = (t - ONE) / ONE_MS;
        let u = -1;
        if (first >= 0 && first <= 1) u = first;
        else if (i === 2 && second >= 0 && second <= 1) u = second;
        if (u < 0) {
          dot.style.opacity = '0';
          return;
        }
        const p = wire.getPointAtLength(wire.getTotalLength() * ease(u));
        dot.setAttribute('cx', String(p.x));
        dot.setAttribute('cy', String(p.y));
        const color = t > JOIN ? 'var(--color-clover-core)' : TOOLS[i].color;
        dot.style.opacity = '1';
        dot.style.fill = t > JOIN ? 'var(--color-clover-100)' : color;
        dot.style.filter = `drop-shadow(0 0 6px ${color})`;
      });
      frame = requestAnimationFrame(tick);
    };
    frame = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(frame);
  }, []);

  const joined = step >= JOINED;
  const rows = step - JOINED;

  return (
    <figure
      aria-label="One failed call. Without BAML it reaches five tools, tracing, error tracking, metrics, product analytics, and alerts, and each keeps a piece of it. In BAML it's one record that holds all five."
      className="relative pb-16"
    >
      <div
        aria-hidden
        className={`v3-glow left-[20%] right-[-10%] -top-10 h-[620px] transition-opacity duration-700 ${joined ? 'opacity-100' : 'opacity-30'}`}
      />
      <p
        aria-live="polite"
        className="v3-enter relative mb-8 md:mb-10 text-center text-[15px] md:text-xl text-white/75 text-balance"
        key={String(joined)}
      >
        {joined
          ? 'In BAML it’s one record, and your agent can read all of it.'
          : 'One failed call reaches five tools. Each keeps a piece of it.'}
      </p>

      <div
        className="relative grid md:grid-cols-[minmax(0,320px)_minmax(0,1fr)_minmax(0,440px)] items-center gap-y-8"
        ref={stage}
      >
        <svg
          aria-hidden="true"
          className="absolute inset-0 size-full overflow-visible pointer-events-none"
        >
          {paths.map((d, i) => (
            <path
              d={d}
              fill="none"
              // biome-ignore lint/suspicious/noArrayIndexKey: one wire per tool, in order
              key={i}
              ref={(el) => {
                wires.current[i] = el;
              }}
              stroke={
                joined && i === 2
                  ? 'rgb(from var(--color-clover-core) r g b / 0.7)'
                  : `rgb(from ${TOOLS[i].color} r g b / ${step > i ? 0.55 : 0.22})`
              }
              strokeDasharray={joined ? undefined : '3 5'}
              strokeWidth={1.25}
              style={{
                opacity: joined && i !== 2 ? 0 : 1,
                transition: 'opacity 500ms, stroke 500ms',
              }}
            />
          ))}
          {TOOLS.map((tool, i) => (
            <circle
              key={tool.name}
              r={4}
              ref={(el) => {
                dots.current[i] = el;
              }}
              style={{ opacity: 0 }}
            />
          ))}
        </svg>

        <div
          className="v3-card relative p-4 md:p-5 md:col-start-1"
          ref={source}
        >
          <p className="text-[13px] text-white/45">A call in production</p>
          <p className="mt-2 font-mono text-[13px] md:text-[14px] leading-6 text-white/85 break-words">
            classify(
            <span className="text-(--color-pasture-200)">
              "Third time asking. Refund me."
            </span>
            )
          </p>
          <p className="flex items-center gap-2 mt-3 text-[13px] md:text-[14px] text-white/70">
            <span className="size-2 rounded-full" style={{ background: RED }} />
            Failed after 1.8s
          </p>
        </div>

        <div className="relative pl-10 md:pl-0 md:col-start-3">
          <p
            className={`absolute left-10 md:left-0 -top-9 flex items-baseline gap-3 transition-opacity duration-500 ${joined ? 'opacity-100' : 'opacity-0'}`}
          >
            <span className="v2-baml-mark text-[20px]">BAML</span>
            <span className="text-[13px] text-white/50">
              One record per call
            </span>
          </p>
          <ul
            className={`flex flex-col transition-[gap,border-color,box-shadow] duration-500 ease-[cubic-bezier(0.65,0,0.35,1)] rounded-xl border ${joined ? 'v3-card v3-card-on gap-0' : 'gap-2.5 border-transparent'}`}
          >
            {TOOLS.map((tool, i) => {
              const arrived = step > i;
              const shown = rows > i;
              return (
                <li
                  className={`grid h-[62px] md:h-[68px] px-4 transition-[border-color,background-color,border-radius,box-shadow] duration-500 ${joined ? `rounded-none bg-transparent border-transparent ${i ? 'border-t-white/8' : ''}` : 'rounded-xl bg-[#121215] border-white/12'} border`}
                  key={tool.name}
                  ref={(el) => {
                    cards.current[i] = el;
                  }}
                  style={{
                    boxShadow:
                      arrived && !joined
                        ? `inset 3px 0 0 ${tool.color}, 0 20px 50px -20px #000`
                        : undefined,
                  }}
                >
                  {/* A tool's piece of the call, and the record's row in the same place. */}
                  <div
                    className={`[grid-area:1/1] self-center min-w-0 transition-opacity duration-300 ${joined ? 'opacity-0' : ''}`}
                  >
                    <p className="flex items-baseline justify-between gap-3 text-[13px] md:text-[14px]">
                      <span className="text-white/75">{tool.name}</span>
                      <span
                        className={`text-white/35 text-[12px] md:text-[13px] transition-opacity duration-300 ${arrived ? '' : 'opacity-0'}`}
                      >
                        {tool.missing}
                      </span>
                    </p>
                    <p
                      className={`mt-1 font-mono text-[12px] md:text-[13px] truncate transition-opacity duration-300 ${arrived ? '' : 'opacity-0'}`}
                      style={{ color: tool.color }}
                    >
                      {tool.fragment}
                    </p>
                  </div>
                  <div
                    className={`[grid-area:1/1] self-center flex items-center gap-4 min-w-0 transition-[opacity,translate] duration-300 ${shown ? '' : 'opacity-0 translate-x-2'}`}
                  >
                    <span className="shrink-0 w-[4.5rem] text-[13px] md:text-[14px] text-clover">
                      {RECORD[i].label}
                    </span>
                    <span className="min-w-0 text-[13px] md:text-[14px] text-white/85 truncate">
                      {RECORD[i].text}
                    </span>
                  </div>
                </li>
              );
            })}
          </ul>
        </div>
      </div>
    </figure>
  );
}
