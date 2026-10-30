/** biome-ignore-all lint/performance/noImgElement: execution-dark.svg, boards-dark.svg and the paintings below are cropped and positioned by hand; next/image would re-encode the paintings */
/** biome-ignore-all lint/suspicious/noArrayIndexKey: the visuals are a static list and never reorder */
'use client';

import { type ReactNode, useEffect, useRef, useState } from 'react';
import { Rise } from './motion';
import { track } from './track';

// How a team starts: send the traces they have, query them, build on them, then add BAML for richer
// data. The steps and visuals work like slides: on desktop the block pins and scrolling moves through them.
const STEPS: {
  title: string;
  body: string;
}[] = [
  {
    body: "Point your existing OTLP exporter at Boundary. Book a meeting, and we'll set it up with you.",
    title: 'Send your traces',
  },
  {
    body: 'Run SQL over every call from Studio, the baml query CLI, or HTTP.',
    title: 'Query them with SQL',
  },
  {
    body: 'Turn a query into a dashboard in Studio, or have your coding agent wire it into Slack, GitHub Actions, or your admin panel.',
    title: 'Build your tools',
  },
  {
    body: 'Move one AI function into BAML. Each call then records its typed result, the step that failed, and the code version.',
    title: 'Add BAML for richer data',
  },
];

const SHADOW =
  'shadow-[0_1px_2px_rgba(10,25,10,0.2),0_24px_48px_-12px_rgba(10,25,10,0.5)]';

// Centres a visual in the 860×700 panel on desktop; on phones (inline) it sits in the flow instead.
export function Win({
  inline,
  w = '78%',
  className = '',
  children,
}: {
  inline?: boolean;
  w?: string;
  className?: string;
  children: ReactNode;
}) {
  return (
    <div
      className={`${inline ? 'relative w-full' : 'absolute left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2'} rounded-[10px] overflow-hidden bg-[#0F1011] border border-white/8 ${SHADOW} ${className}`}
      style={inline ? undefined : { width: w }}
    >
      {children}
    </div>
  );
}

export function Bar({ title, sub }: { title: string; sub?: string }) {
  return (
    <div className="flex gap-5 px-5 py-3 border-b border-white/8 font-mono text-xs">
      <span className="text-white">{title}</span>
      {sub && <span className="text-[#6E756A]">{sub}</span>}
    </div>
  );
}

export const MONO = (inline?: boolean) =>
  `font-mono leading-[1.7] ${inline ? 'text-[11px]' : 'text-[clamp(11px,1vw,14px)]'}`;

// 01 Send: the standard OpenTelemetry exporter settings
function Exporter({ inline }: { inline?: boolean }) {
  return (
    <Win inline={inline}>
      <Bar sub="your service" title=".env" />
      <pre
        className={`px-5 py-5 text-[#D8DDD3] whitespace-pre-wrap break-words ${MONO(inline)}`}
      >
        <span className="text-clover">OTEL_EXPORTER_OTLP_ENDPOINT</span>=
        <span className="text-pasture-200">your-boundary-endpoint</span>
        {'\n'}
        <span className="text-clover">OTEL_EXPORTER_OTLP_HEADERS</span>=
        <span className="text-pasture-200">
          authorization=your-boundary-key
        </span>
      </pre>
    </Win>
  );
}

// 02 Query: a real query against the table baml query reads
function Query({ inline }: { inline?: boolean }) {
  return (
    <Win inline={inline}>
      <Bar sub="spans" title="baml query" />
      <pre
        className={`px-5 pt-5 pb-4 text-[#D8DDD3] ${MONO(inline)} ${inline ? 'overflow-x-auto' : ''}`}
      >
        <span className="text-clover">SELECT</span> span_name, count(*){' '}
        <span className="text-clover">AS</span> errors{'\n'}
        <span className="text-clover">FROM</span> spans{' '}
        <span className="text-clover">WHERE</span> status ={' '}
        <span className="text-pasture-200">&apos;user_error&apos;</span>
        {'\n'}
        <span className="text-clover">GROUP BY</span> span_name
      </pre>
      <div
        className={`mx-5 mb-5 rounded-md border border-white/10 overflow-hidden ${MONO(inline)}`}
      >
        <div className="grid grid-cols-[2fr_1fr] px-3 py-2 text-[#6E756A] border-b border-white/10">
          <span>span_name</span>
          <span>errors</span>
        </div>
        <div className="grid grid-cols-[2fr_1fr] px-3 py-2 text-[#D8DDD3] border-b border-white/10">
          <span>user.ExtractInvoice</span>
          <span>14</span>
        </div>
        <div className="grid grid-cols-[2fr_1fr] px-3 py-2 text-[#D8DDD3]">
          <span>user.ClassifyTicket</span>
          <span>3</span>
        </div>
      </div>
    </Win>
  );
}

// One visual per step; they crossfade inside the pinned panel on desktop.
const VISUALS: ReactNode[] = [
  <Exporter key="send" />,
  <Query key="query" />,
  // the executions-per-minute panel from Studio
  <div
    className={`absolute left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 w-[84%] aspect-[687/469] rounded-[10px] overflow-hidden bg-[#141517] ${SHADOW}`}
    key="build"
  >
    <img
      alt=""
      className="absolute max-w-none"
      src="/boundary/boards-dark.svg"
      style={{ height: '218%', left: '-42.4%', top: '-42.4%', width: '249%' }}
    />
  </div>,
  <div
    className={`absolute left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 w-[92%] aspect-[1712/1024] rounded-[10px] overflow-hidden bg-[#0F1011] ${SHADOW}`}
    key="baml"
  >
    <img alt="" className="w-full h-full" src="/boundary/execution-dark.svg" />
  </div>,
];

// Phones: the card-like steps sit in the flow; the screenshots keep the panel's proportions.
const INLINE: Record<number, ReactNode> = {
  0: <Exporter inline />,
  1: <Query inline />,
};

// Phones: the execution trace is too dense at full width, so show its top-left (the call and its spans) larger.
const ZOOMED: Record<number, ReactNode> = {
  3: (
    <div
      className={`absolute inset-[6%] rounded-[10px] overflow-hidden bg-[#0F1011] ${SHADOW}`}
    >
      <img
        alt=""
        className="absolute left-0 top-0 max-w-none w-[175%]"
        src="/boundary/execution-dark.svg"
      />
    </div>
  ),
};

function MobileVisual({ i }: { i: number }) {
  return (
    <div
      className={`lg:hidden relative mt-5 rounded-xl overflow-hidden bg-pasture ${INLINE[i] ? 'p-3' : 'aspect-[860/700]'}`}
      data-on=""
      data-theme="light"
    >
      <img
        alt=""
        className="absolute inset-0 w-full h-full object-cover"
        loading="lazy"
        src="/boundary/painting-hills.jpg"
      />
      {INLINE[i] ?? ZOOMED[i] ?? VISUALS[i]}
    </div>
  );
}

export default function GetStarted() {
  const trackRef = useRef<HTMLDivElement>(null);
  const [active, setActive] = useState(0);
  const [pinned, setPinned] = useState(false);

  // On desktop the steps + visual pin while the track scrolls past; scroll position picks the step.
  useEffect(() => {
    const mq = window.matchMedia('(min-width: 1024px)');
    let raf = 0;
    const update = () => {
      raf = 0;
      const el = trackRef.current;
      if (!el || !mq.matches) return;
      const r = el.getBoundingClientRect();
      const p = Math.min(
        1,
        Math.max(0, -r.top / (r.height - window.innerHeight)),
      );
      setActive(Math.min(STEPS.length - 1, Math.floor(p * STEPS.length)));
    };
    const onScroll = () => {
      if (!raf) raf = requestAnimationFrame(update);
    };
    const onMq = () => {
      setPinned(mq.matches);
      update();
    };
    onMq();
    mq.addEventListener('change', onMq);
    window.addEventListener('scroll', onScroll, { passive: true });
    return () => {
      cancelAnimationFrame(raf);
      mq.removeEventListener('change', onMq);
      window.removeEventListener('scroll', onScroll);
    };
  }, []);

  // When pinned, clicking a step scrolls to it rather than just switching.
  const choose = (i: number) => {
    track('get_started_step_selected', { index: i, step: STEPS[i].title });
    const el = trackRef.current;
    if (!pinned || !el) {
      setActive(i);
      return;
    }
    const top = el.getBoundingClientRect().top + window.scrollY;
    const range = el.offsetHeight - window.innerHeight;
    window.scrollTo({
      behavior: 'smooth',
      top: top + ((i + 0.5) / STEPS.length) * range,
    });
  };

  return (
    <section className="w-full max-w-[1440px] mx-auto px-5 md:px-12 pt-16 md:pt-30 pb-20 md:pb-34 lg:pb-16 border-t border-hair">
      <div className="flex flex-col lg:flex-row lg:justify-between lg:items-end gap-5 md:gap-8 mb-6 lg:mb-4">
        <Rise className="max-w-215 h2-display" text="Get started in minutes." />
        <p className="lg:w-95 shrink-0 text-[17px] leading-[27px] text-ink-2">
          Start with the traces you already have, and add BAML when you want
          richer data.
        </p>
      </div>

      {/* Track: tall on desktop so the pinned block stays put for one screen per step. */}
      <div className="lg:h-[calc(100vh*3.5)]" ref={trackRef}>
        <div className="lg:sticky lg:top-18 lg:h-[calc(100vh-72px)] flex flex-col lg:flex-row gap-10 lg:gap-16 items-start lg:items-center">
          <ol className="w-full lg:w-105 shrink-0 border-t border-ink">
            {STEPS.map((s, i) => {
              const on = i === active;
              return (
                <li className="border-b border-hair" key={s.title}>
                  <button
                    aria-current={on ? 'step' : undefined}
                    className="w-full flex py-5.5 text-left cursor-pointer group"
                    onClick={() => choose(i)}
                    type="button"
                  >
                    <span className="flex-1 flex flex-col">
                      <span
                        className={`font-display font-medium text-[19px] leading-[22px] tracking-[-0.03em] transition-colors max-lg:text-ink ${on ? 'text-ink' : 'text-ink-3 group-hover:text-ink-2'}`}
                      >
                        {s.title}
                      </span>
                      <span
                        className={`grid transition-[grid-template-rows,opacity] duration-300 max-lg:grid-rows-[1fr] max-lg:opacity-100 ${on ? 'grid-rows-[1fr] opacity-100' : 'grid-rows-[0fr] opacity-0'}`}
                      >
                        <span className="overflow-hidden flex flex-col">
                          <span className="pt-3.5 text-[15px] leading-6 text-ink-2">
                            {s.body}
                          </span>
                        </span>
                      </span>
                    </span>
                  </button>
                  <div className="-mt-1 pb-6 lg:hidden">
                    <MobileVisual i={i} />
                  </div>
                </li>
              );
            })}
          </ol>

          <div className="hidden w-full min-w-0 lg:flex-1 lg:flex justify-end">
            <div
              className="relative w-full lg:w-auto lg:h-[min(700px,calc(100vh-152px))] max-w-full aspect-[860/700] rounded-[14px] overflow-hidden bg-pasture"
              data-theme="light"
            >
              <img
                alt=""
                className="absolute inset-0 w-full h-full object-cover object-[50%_65%]"
                src="/boundary/painting-hills.jpg"
              />
              {VISUALS.map((v, i) => (
                <div
                  aria-hidden={i !== active}
                  className={`absolute inset-0 transition-[opacity,transform] duration-600 ease-[cubic-bezier(0.22,1,0.36,1)] ${
                    i === active
                      ? 'opacity-100 translate-y-0'
                      : 'opacity-0 translate-y-3 pointer-events-none'
                  }`}
                  data-on={i === active ? '' : undefined}
                  key={i}
                >
                  {v}
                </div>
              ))}
            </div>
          </div>
        </div>
      </div>
    </section>
  );
}
