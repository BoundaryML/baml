'use client';

import { useEffect, useState } from 'react';
import { FIXES } from './story';

// The loop as a delivery tracker, drawn like a figure in a research paper: flat fills, hairlines, and a
// caption, with no glow or gradients. The stages before production sit on the left, the ones in
// production on the right, and deploying is the line between them rather than a stage of its own. The
// numbers are example data.
type Stage = { note: string; name: string };

const BEFORE: Stage[] = [
  { name: 'Build', note: 'baml check' },
  { name: 'Test', note: 'baml test' },
];
const AFTER: Stage[] = [
  { name: 'Trace', note: 'every call' },
  { name: 'Query', note: 'baml query' },
  { name: 'Errors', note: 'by cause' },
  { name: 'Dashboards', note: 'per function' },
  { name: 'Alerts', note: 'to the agent' },
];
const DEPLOY = BEFORE.length;
const BACK = DEPLOY + 1 + AFTER.length;
const STAGE_MS = 1700;
const DEPLOY_MS = 1300;
const BACK_MS = 2000;

function status(stage: number, version: number, lap: number) {
  const fix = FIXES[lap % FIXES.length];
  return [
    'Your agent writes classify, and baml check passes.',
    `${8 + lap} tests run against the real model. All pass.`,
    `v${version} is live.`,
    'Every call to classify is traced. 1,204 so far.',
    'Your agent asks production what changed since the deploy.',
    `31 ${fix.failure}, grouped as one error.`,
    'Labels, latency, and cost for classify, by version.',
    'An alert hands those 31 calls to your agent.',
    'They become tests, and the next build starts.',
  ][stage];
}

function Segment({
  index,
  stage,
  step,
}: {
  index: number;
  stage: Stage;
  step: number;
}) {
  const done = step > index;
  const on = step === index;
  return (
    <div
      className={`relative flex-1 min-w-0 flex md:flex-col md:items-start items-center justify-between md:justify-end gap-1 h-12 md:h-[84px] px-3 md:pb-2.5 overflow-hidden rounded-[3px] border transition-colors duration-300 ${on ? 'border-clover' : done ? 'border-clover/40' : 'border-white/14'}`}
    >
      {(done || on) && (
        <span
          aria-hidden
          className={`absolute inset-0 origin-left ${on ? 'v3-cycle bg-clover' : 'bg-clover/[0.16]'}`}
          key={on ? 'on' : 'done'}
          style={{ '--ms': `${STAGE_MS}ms` } as React.CSSProperties}
        />
      )}
      <span
        className={`relative text-[15px] lg:text-[17px] font-medium tracking-[-0.01em] transition-colors duration-300 ${on ? 'text-black' : done ? 'text-white' : 'text-white/45'}`}
      >
        {stage.name}
      </span>
      <span
        className={`relative font-mono text-[11.5px] lg:text-[12.5px] whitespace-nowrap transition-colors duration-300 ${on ? 'text-black/70' : done ? 'text-white/55' : 'text-white/30'}`}
      >
        {stage.note}
      </span>
    </div>
  );
}

/** A bracket over a group of stages, with its label in the middle of the rule. */
function Bracket({ label }: { label: string }) {
  return (
    <div className="hidden md:flex items-center gap-3 mb-3 h-3 border-x border-white/25 font-mono text-[12px] text-white/50">
      <span className="flex-1 h-px bg-white/25" />
      {label}
      <span className="flex-1 h-px bg-white/25" />
    </div>
  );
}

export default function HeroTracker() {
  const [{ lap, step }, setState] = useState({ lap: 0, step: 0 });

  useEffect(() => {
    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
      setState({ lap: 0, step: BACK - 1 });
      return;
    }
    const ms = step === DEPLOY ? DEPLOY_MS : step === BACK ? BACK_MS : STAGE_MS;
    const id = setTimeout(
      () =>
        setState(({ lap, step }) =>
          step === BACK ? { lap: lap + 1, step: 0 } : { lap, step: step + 1 },
        ),
      ms,
    );
    return () => clearTimeout(id);
  }, [step]);

  const version = 42 + lap;
  const deployed = step > DEPLOY && step !== BACK;
  const back = step === BACK;

  return (
    <figure
      aria-label="The loop as a delivery tracker. Build and test come before production, and monitoring after it: trace, query, errors, dashboards, and alerts. Deploying is the line between the two. At the end, what production found goes back to build."
      className="relative pb-16 border-t border-white/15"
    >
      <div className="flex items-baseline justify-between gap-6 py-3 mb-10 md:mb-14 font-mono text-[12px] md:text-[13px] text-white/50">
        <span>Fig. 1</span>
        <span>classify v{version}</span>
      </div>

      <div className="relative flex flex-col md:flex-row">
        <div className="md:flex-[2] min-w-0">
          <p className="md:hidden mb-2 font-mono text-[12px] text-white/50">
            Before production
          </p>
          <Bracket label="Before production" />
          <div className="flex flex-col md:flex-row gap-1">
            {BEFORE.map((s, i) => (
              <Segment index={i} key={s.name} stage={s} step={step} />
            ))}
          </div>
        </div>

        {/* Deploying isn't a stage with a box, it's the line production starts at. */}
        <div className="relative shrink-0 h-16 md:h-auto md:w-28">
          <span
            aria-hidden
            className={`absolute left-0 right-0 top-1/2 md:left-1/2 md:right-auto md:-top-7 md:bottom-0 transition-colors duration-300 ${step === DEPLOY ? 'h-0.5 md:h-auto md:w-0.5 bg-clover' : deployed ? 'h-px md:h-auto md:w-px bg-clover/70' : 'h-px md:h-auto md:w-px bg-white/40'}`}
          />
          <p
            className={`absolute left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 md:top-auto md:translate-y-0 md:bottom-[calc(100%+30px)] px-2 bg-black font-mono text-[12px] md:text-[13px] whitespace-nowrap transition-colors duration-300 ${step === DEPLOY ? 'text-clover' : 'text-white/60'}`}
          >
            deploy (baml pack)
          </p>
        </div>

        <div className="md:flex-[5] min-w-0">
          <p className="md:hidden mb-2 font-mono text-[12px] text-white/50">
            In production
          </p>
          <Bracket label="In production" />
          <div className="flex flex-col md:flex-row gap-1">
            {AFTER.map((s, i) => (
              <Segment
                index={DEPLOY + 1 + i}
                key={s.name}
                stage={s}
                step={step}
              />
            ))}
          </div>
        </div>

        {/* What production found runs back under the tracker to build. */}
        <div
          aria-hidden
          className="hidden md:block absolute left-4 right-4 top-[108px] h-[52px] pointer-events-none"
        >
          <span className="absolute inset-0 border border-t-0 border-dashed border-white/15" />
          <span
            className="absolute inset-0 border border-t-0 border-clover"
            style={{
              clipPath: back
                ? 'inset(0 -2px -2px 0)'
                : 'inset(0 -2px -2px 100%)',
              transition: back
                ? `clip-path ${BACK_MS * 0.8}ms cubic-bezier(0.65,0,0.35,1)`
                : 'none',
            }}
          />
          <span
            className={`absolute -left-[4px] -top-px size-0 border-x-[4px] border-x-transparent border-b-[6px] transition-colors duration-300 ${back ? 'border-b-clover' : 'border-b-white/25'}`}
          />
        </div>
      </div>

      <div className="grid md:grid-cols-[minmax(0,1fr)_minmax(0,1fr)] gap-x-12 gap-y-4 mt-10 md:mt-24 pt-5 border-t border-white/10">
        <p className="text-[14px] md:text-[15px] leading-[1.55] text-white/55 max-w-[34rem]">
          <span className="text-white">Figure 1.</span> One change to a classify
          function, from build to production and back. Deploying is a boundary,
          not a stage. Everything after it runs in the same toolchain as
          everything before it.
        </p>
        <p
          aria-live="polite"
          className={`v3-enter font-mono text-[13px] md:text-[14px] leading-[1.55] ${back ? 'text-clover' : 'text-white/85'}`}
          key={`${lap}-${step}`}
        >
          <span className="text-white/35">
            {String(step + 1).padStart(2, '0')}/{BACK + 1}{' '}
          </span>
          {status(step, version, lap)}
        </p>
      </div>
    </figure>
  );
}
