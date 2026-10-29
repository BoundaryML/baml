/** biome-ignore-all lint/performance/noImgElement: execution-dark.svg, boards-dark.svg and the paintings below are cropped and positioned by hand; next/image would re-encode the paintings */
/** biome-ignore-all lint/suspicious/noArrayIndexKey: the diff lines are static and never reordered */
/** biome-ignore-all lint/suspicious/noTemplateCurlyInString: the diff lines are BAML source, where ${...} is BAML's own interpolation */
'use client';

import {
  type CSSProperties,
  type ReactNode,
  useEffect,
  useRef,
  useState,
} from 'react';
import { Rise } from './motion';
import { track } from './track';

// One trip around the loop (Boundary's "Close the loop" diagram): you set the goal, an agent runs the steps,
// you approve. Release is dashed in their diagram: flags are coming, so its title says so.
const STEPS: {
  title: string;
  body: string;
  instead: string;
  soon?: boolean;
}[] = [
  {
    body: 'You set the goal: cut cost, keep accuracy. An agent swaps ExtractInvoice to a cheaper model, in one line of BAML.',
    instead: 'prompts as strings in code',
    title: 'Write',
  },
  {
    body: 'baml test runs the change against real examples captured from production. Everything passes.',
    instead: 'Promptfoo and Braintrust',
    title: 'Test',
  },
  {
    body: 'A flag ramps the cheaper model to 5% of users. If one customer has an emergency, switch what the code does for just them.',
    instead: 'LaunchDarkly and Statsig',
    soon: true,
    title: 'Release',
  },
  {
    body: "Every call is traced, in your own cloud. There's no SDK to wire up and no spans to add.",
    instead: 'Datadog, Sentry and Langfuse',
    title: 'Observe',
  },
  {
    body: 'baml query lines the two models up side by side: cost down 32%, accuracy the same.',
    instead: 'PostHog and notebooks',
    title: 'Compare',
  },
  {
    body: 'Studio dashboards show the team what changed, from the same data the agent used. You approve, and it goes to 100%.',
    instead: 'Slack threads and Looker',
    title: 'Learn',
  },
];

const SHADOW =
  'shadow-[0_1px_2px_rgba(10,25,10,0.2),0_24px_48px_-12px_rgba(10,25,10,0.5)]';

// Centres a visual in the 860×700 panel on desktop; on phones (inline) it sits in the flow instead.
function Win({
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

function Bar({ title, sub }: { title: string; sub?: string }) {
  return (
    <div className="flex gap-5 px-5 py-3 border-b border-white/8 font-mono text-xs">
      <span className="text-white">{title}</span>
      {sub && <span className="text-[#6E756A]">{sub}</span>}
    </div>
  );
}

const MONO = (inline?: boolean) =>
  `font-mono leading-[1.7] ${inline ? 'text-[11px]' : 'text-[clamp(11px,1vw,14px)]'}`;

// 01 Write: the agent's one-line model swap
const DIFF: [string, 'add' | 'del' | null][] = [
  ['class Invoice {', null],
  ['  vendor: string,', null],
  ['  total: float,', null],
  ['  due_date: string?,', null],
  ['}', null],
  ['', null],
  ['function ExtractInvoice(file: pdf) -> Invoice {', null],
  ['  client: "openai/gpt-4o"', 'del'],
  ['  client: "openai/gpt-4o-mini"', 'add'],
  ['  prompt: `', null],
  ['    Pull the fields out of ${file}.', null],
  ['    ${ctx.output_format()}', null],
  ['  `', null],
  ['}', null],
];

function Editor({ inline }: { inline?: boolean }) {
  return (
    <Win inline={inline}>
      <Bar sub="main.py" title="extract.baml" />
      <pre
        className={`py-5 text-[#D8DDD3] ${MONO(inline)} ${inline ? 'overflow-x-auto' : ''}`}
      >
        {DIFF.map(([l, kind], i) => (
          <div
            className={`px-5 ${kind === 'add' ? 'diff-add text-[#E1F3DC]' : kind === 'del' ? 'bg-[#E5484D]/12 text-[#F2B8BA] line-through decoration-[#E5484D]/50' : ''}`}
            key={i}
            style={kind === 'add' ? ({ '--k': 0 } as CSSProperties) : undefined}
          >
            <span className="inline-block w-4 text-[#6E756A]">
              {kind === 'add' ? '+' : kind === 'del' ? '−' : ' '}
            </span>
            <span className="diff-text">{l || ' '}</span>
          </div>
        ))}
      </pre>
    </Win>
  );
}

// 02 Test: baml test against production examples
const TESTS = [
  'acme_0412.pdf',
  'lumen_eu_vat.pdf',
  'northwind_q3.pdf',
  'kestrel_scan.pdf',
  'halden_multi_page.pdf',
  'sundial_credit.pdf',
];
function Terminal({ inline }: { inline?: boolean }) {
  return (
    <Win inline={inline}>
      <Bar sub="~/invoices" title="zsh" />
      <div className={`px-5 py-5 text-[#D8DDD3] ${MONO(inline)}`}>
        <div>
          <span className="text-pasture-300">$</span> baml test ExtractInvoice
        </div>
        <div className="text-[#6E756A]">
          running 24 tests from production examples
        </div>
        <div className="h-2" />
        {TESTS.map((t, i) => (
          <div className="flex justify-between gap-4" key={t}>
            <span>
              <span className="text-pasture-300">✓</span> {t}
            </span>
            <span className="text-[#6E756A]">
              {(0.62 + i * 0.07).toFixed(2)}s
            </span>
          </div>
        ))}
        <div className="text-[#6E756A]">… 18 more</div>
        <div className="h-2" />
        <div>
          <span className="text-pasture-300">24 passed</span> · 0 failed
        </div>
      </div>
    </Win>
  );
}

// 03 Release (coming): a flag on the function, ramped to 5%, with a per-customer override
function Flag({ inline }: { inline?: boolean }) {
  return (
    <Win className="bg-white! border-black/8" inline={inline}>
      <div className="flex items-center justify-between px-5 py-3.5 border-b border-hair">
        <span className="text-[13px] font-medium text-ink">
          ExtractInvoice · model
        </span>
        <span className="text-[12px] text-ink-3">coming soon</span>
      </div>
      <div className="flex flex-col gap-4 px-5 py-5">
        {[
          ['openai/gpt-4o', 95, 'bg-[#C9CCC0]'],
          ['openai/gpt-4o-mini', 5, 'bg-pasture'],
        ].map(([name, pctv, c]) => (
          <div className="flex flex-col gap-2" key={name as string}>
            <div className="flex justify-between font-mono text-[12px]">
              <span className="text-ink">{name}</span>
              <span className="text-ink-3">{pctv}% of users</span>
            </div>
            <div className="h-2 rounded-full bg-panel overflow-hidden">
              <div
                className={`h-full rounded-full ${c}`}
                style={{ width: `${pctv}%` }}
              />
            </div>
          </div>
        ))}
        <div className="flex items-center justify-between gap-3 mt-1 px-3 py-2.5 rounded-md bg-panel text-[12px]">
          <span className="text-ink-2">
            Override for <span className="text-ink font-medium">acme-corp</span>
          </span>
          <span className="font-mono text-accent">→ gpt-4o</span>
        </div>
      </div>
    </Win>
  );
}

// 05 Compare: baml query, the two models side by side
function Query({ inline }: { inline?: boolean }) {
  return (
    <Win inline={inline}>
      <Bar sub="calls_v1" title="baml query" />
      <pre
        className={`px-5 pt-5 pb-4 text-[#D8DDD3] ${MONO(inline)} ${inline ? 'overflow-x-auto' : ''}`}
      >
        <span className="text-clover">SELECT</span> model, avg(cost_usd),
        avg(accuracy){'\n'}
        <span className="text-clover">FROM</span> calls_v1{' '}
        <span className="text-clover">WHERE</span> fn ={' '}
        <span className="text-pasture-200">&apos;ExtractInvoice&apos;</span>
        {'\n'}
        <span className="text-clover">GROUP BY</span> model
      </pre>
      <div
        className={`mx-5 mb-5 rounded-md border border-white/10 overflow-hidden ${MONO(inline)}`}
      >
        <div className="grid grid-cols-[1.6fr_1fr_1fr] px-3 py-2 text-[#6E756A] border-b border-white/10">
          <span>model</span>
          <span>cost</span>
          <span>accuracy</span>
        </div>
        <div className="grid grid-cols-[1.6fr_1fr_1fr] px-3 py-2 text-[#D8DDD3] border-b border-white/10">
          <span>gpt-4o</span>
          <span>$0.0124</span>
          <span>98.3%</span>
        </div>
        <div className="grid grid-cols-[1.6fr_1fr_1fr] px-3 py-2 text-[#E1F3DC] bg-white/4">
          <span>gpt-4o-mini</span>
          <span>
            $0.0084 <span className="text-pasture-300">−32%</span>
          </span>
          <span>98.2%</span>
        </div>
      </div>
    </Win>
  );
}

// One visual per step; they crossfade inside the pinned panel on desktop.
const VISUALS: ReactNode[] = [
  <Editor key="write" />,
  <Terminal key="test" />,
  <Win className="bg-transparent! border-0" key="release" w="62%">
    <Flag inline />
  </Win>,
  <div
    className={`absolute left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 w-[92%] aspect-[1712/1024] rounded-[10px] overflow-hidden bg-[#0F1011] ${SHADOW}`}
    key="observe"
  >
    <img alt="" className="w-full h-full" src="/boundary/execution-dark.svg" />
  </div>,
  <Query key="compare" />,
  // the executions-per-minute panel, with the r42 deploy marker
  <div
    className={`absolute left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 w-[84%] aspect-[687/469] rounded-[10px] overflow-hidden bg-[#141517] ${SHADOW}`}
    key="learn"
  >
    <img
      alt=""
      className="absolute max-w-none"
      src="/boundary/boards-dark.svg"
      style={{ height: '218%', left: '-42.4%', top: '-42.4%', width: '249%' }}
    />
  </div>,
];

// Phones: the card-like steps sit in the flow; the screenshots keep the panel's proportions.
const INLINE: Record<number, ReactNode> = {
  0: <Editor inline />,
  1: <Terminal inline />,
  2: <Flag inline />,
  4: <Query inline />,
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

export default function Loop() {
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
    track('loop_step_selected', { index: i, step: STEPS[i].title });
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
        <Rise
          className="max-w-215 h2-display"
          text="Close the loop, so an agent can run it and you steer."
        />
        <p className="lg:w-95 shrink-0 text-[17px] leading-[27px] text-ink-2">
          Here&apos;s one trip around it. You set the goal and approve the
          result; an agent does everything in between, and every step is joined
          by one name.
        </p>
      </div>

      {/* Track: tall on desktop so the pinned block stays put for one screen per step. */}
      <div className="lg:h-[calc(100vh*5)]" ref={trackRef}>
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
                        {s.soon && (
                          <span className="ml-2 font-sans font-normal text-[14px] tracking-normal text-ink-3">
                            coming soon
                          </span>
                        )}
                      </span>
                      <span
                        className={`grid transition-[grid-template-rows,opacity] duration-300 max-lg:grid-rows-[1fr] max-lg:opacity-100 ${on ? 'grid-rows-[1fr] opacity-100' : 'grid-rows-[0fr] opacity-0'}`}
                      >
                        <span className="overflow-hidden flex flex-col gap-3.5">
                          <span className="pt-3.5 text-[15px] leading-6 text-ink-2">
                            {s.body}
                          </span>
                          <span className="text-[13px] leading-5 text-ink-3">
                            Today: {s.instead}
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
            <li className="py-5.5 text-sm leading-[22px] text-ink-2">
              And round again.
            </li>
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
