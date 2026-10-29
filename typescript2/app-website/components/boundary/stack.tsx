/** biome-ignore-all lint/performance/noImgElement: the Boundary wordmarks below are SVGs, and the partner logos are a data-driven mix of SVG and PNG sized by height with width:auto, which next/image can't express without an intrinsic width per logo */
/** biome-ignore-all lint/suspicious/noArrayIndexKey: the stage rows are static and never reordered */
import logoWhite from '@boundaryml/brand/logo/svg/boundary-primary-white.svg';
import Image from 'next/image';
import type { CSSProperties } from 'react';
import { InView, Rise } from './motion';

// "Today, a person is the glue": the six stages of shipping AI software, each in its own tool,
// all collapsing into one Boundary install where one function name joins every step.
// Positions are in a 1344×720 frame and scale with the panel.
const pct = (v: number, of: number) => `${(v / of) * 100}%`;
const box = (x: number, y: number, w: number, h: number) => ({
  height: pct(h, 720),
  left: pct(x, 1344),
  top: pct(y, 720),
  width: pct(w, 1344),
});

type Tool = { name: string; logo?: string; h?: number };

// Official logos where we have them (public/logos); the rest are set as plain wordmarks.
const STAGES: {
  stage: string;
  tools: Tool[];
  note?: string;
  ours: string;
  detail: string;
  soon?: boolean;
}[] = [
  {
    detail: 'function ExtractInvoice(file: pdf) -> Invoice',
    note: 'prompts as strings in code',
    ours: 'BAML',
    stage: 'Write',
    tools: [],
  },
  {
    detail: 'baml test ExtractInvoice',
    ours: 'baml test',
    stage: 'Test',
    tools: [{ name: 'Promptfoo' }, { name: 'Braintrust' }],
  },
  {
    detail: 'ExtractInvoice → 5% of users',
    ours: 'Flags',
    soon: true,
    stage: 'Release',
    tools: [
      { h: 14, logo: '/boundary/logos/launchdarkly.svg', name: 'LaunchDarkly' },
      { name: 'Statsig' },
    ],
  },
  {
    detail: 'every ExtractInvoice call',
    ours: 'Tracing, built in',
    stage: 'Observe',
    tools: [
      { h: 20, logo: '/boundary/logos/datadog.png', name: 'Datadog' },
      { h: 18, logo: '/boundary/logos/sentry.svg', name: 'Sentry' },
      { name: 'Langfuse' },
    ],
  },
  {
    detail: "WHERE fn = 'ExtractInvoice'",
    ours: 'baml query',
    stage: 'Compare',
    tools: [
      { h: 14, logo: '/boundary/logos/posthog.svg', name: 'PostHog' },
      { name: 'notebooks' },
    ],
  },
  {
    detail: 'ExtractInvoice health',
    ours: 'Studio dashboards',
    stage: 'Learn',
    tools: [{ name: 'Slack threads' }, { name: 'Looker' }],
  },
];

// Desktop geometry (frame units)
const CARD = { gap: 12, h: 88, w: 330, x: 48, y: 62 };
const CORE = { h: 596, w: 684, x: 612, y: 62 };
const ROW = { h: 64, inset: 24, step: 76, y: 182 };
const cardMid = (i: number) => CARD.y + CARD.h / 2 + i * (CARD.h + CARD.gap);
const rowMid = (i: number) => ROW.y + ROW.h / 2 + i * ROW.step;

function Tools({
  tools,
  note,
  small,
}: {
  tools: Tool[];
  note?: string;
  small?: boolean;
}) {
  if (note)
    return <span className="text-[13px] leading-5 text-ink-3">{note}</span>;
  return (
    <span
      className={`flex items-center ${small ? 'flex-wrap gap-x-3 gap-y-1.5' : 'flex-nowrap gap-x-3 xl:gap-x-4'}`}
    >
      {tools.map((t) =>
        t.logo ? (
          <img
            alt={t.name}
            className="w-auto"
            key={t.name}
            src={t.logo}
            style={{ height: Math.round((t.h ?? 16) * (small ? 0.85 : 1)) }}
          />
        ) : (
          <span
            className={`${small ? 'text-[13px]' : 'text-[13px] xl:text-[14px]'} whitespace-nowrap leading-5 font-medium tracking-[-0.01em] text-[#3A3A3A]`}
            key={t.name}
          >
            {t.name}
          </span>
        ),
      )}
    </span>
  );
}

function Soon() {
  return (
    <span className="text-[13px] font-normal text-white/55">coming soon</span>
  );
}

export default function Stack() {
  return (
    <section
      className="w-full max-w-[1440px] mx-auto px-5 md:px-12 pt-12 md:pt-20 pb-16 md:pb-30 border-t border-hair"
      id="product"
    >
      <div className="flex flex-col lg:flex-row lg:justify-between lg:items-end gap-5 md:gap-8 mb-10 md:mb-14">
        <Rise
          className="max-w-205 h2-display"
          text="Today, a person is the glue between six tools."
        />
        <p className="lg:w-100 shrink-0 text-[17px] leading-[27px] text-ink-2">
          Writing, testing, releasing, observing, comparing and learning each
          live in a different product. You copy IDs, CSVs and context between
          them, and your agent can&apos;t join the data. Boundary is one
          install, and one name joins every step.
        </p>
      </div>

      <StackMobile />

      {/* desktop: six tools on the left, each flowing into its row inside Boundary */}
      <div className="hidden lg:block rounded-[14px]">
        <InView
          className="relative aspect-[1344/720] rounded-[14px] overflow-hidden bg-pasture"
          light
        >
          {/* the whole panel sits on the hand-painted background */}
          <Image
            alt=""
            className="object-cover"
            fill
            quality={90}
            sizes="(min-width: 1440px) 1344px, 100vw"
            src="/boundary/painting-footer-4k.jpg"
          />

          <svg
            aria-hidden="true"
            className="stack-lines absolute inset-0 w-full h-full"
            fill="none"
            viewBox="0 0 1344 720"
          >
            <g
              className="stack-flow"
              stroke="#FFFFFF"
              strokeDasharray="4 5"
              strokeOpacity="0.6"
              strokeWidth="1.5"
            >
              {STAGES.map((_, i) => {
                const y1 = cardMid(i);
                const y2 = rowMid(i);
                const x1 = CARD.x + CARD.w;
                const x2 = CORE.x;
                return (
                  <path
                    d={`M${x1} ${y1} C${x1 + 120} ${y1} ${x2 - 120} ${y2} ${x2} ${y2}`}
                    key={i}
                  />
                );
              })}
            </g>
            {STAGES.map((_, i) => (
              <circle
                className="stack-dot"
                cx={CORE.x}
                cy={rowMid(i)}
                fill="#E1F3DC"
                key={i}
                r="3.5"
                style={{ '--i': i } as CSSProperties}
              />
            ))}
          </svg>

          {STAGES.map((s, i) => (
            <div
              className={`stack-card absolute flex flex-col justify-between rounded-[10px] px-[18px] py-3.5 border ${
                s.note
                  ? 'border-dashed border-[#C9CCC0] bg-white/85'
                  : 'border-card-line bg-white'
              }`}
              key={s.stage}
              style={
                {
                  ...box(
                    CARD.x,
                    CARD.y + i * (CARD.h + CARD.gap),
                    CARD.w,
                    CARD.h,
                  ),
                  '--i': i,
                } as CSSProperties
              }
            >
              <span className="text-[13px] leading-4 text-ink-3">
                {s.stage}
              </span>
              <Tools note={s.note} tools={s.tools} />
            </div>
          ))}

          <div
            className="stack-core absolute rounded-[14px] bg-pasture border border-white/15 shadow-[0_1px_2px_rgba(20,40,20,0.12),0_24px_48px_-16px_rgba(20,40,20,0.45)]"
            style={box(CORE.x, CORE.y, CORE.w, CORE.h)}
          >
            <div
              className="absolute flex flex-col gap-3"
              style={{
                left: pct(ROW.inset, CORE.w),
                right: pct(ROW.inset, CORE.w),
                top: pct(28, CORE.h),
              }}
            >
              <img
                alt="Boundary"
                className="w-[117px] h-5"
                src={logoWhite.src}
              />
              <p className="text-sm leading-5 text-white/80">
                One install. One name joins every step, so an agent can run the
                loop.
              </p>
            </div>
          </div>

          {STAGES.map((s, i) => (
            <div
              className={`stack-core absolute flex items-center gap-5 px-4 rounded-md border ${
                s.soon
                  ? 'border-dashed border-white/30 bg-white/4'
                  : 'border-white/15 bg-white/8'
              }`}
              key={s.stage}
              style={box(
                CORE.x + ROW.inset,
                ROW.y + i * ROW.step,
                CORE.w - ROW.inset * 2,
                ROW.h,
              )}
            >
              <span className="w-18 shrink-0 text-[13px] text-white/60">
                {s.stage}
              </span>
              <span className="flex items-center gap-2 w-48 shrink-0 text-[15px] font-medium text-white">
                {s.ours} {s.soon && <Soon />}
              </span>
              <span className="hidden xl:block truncate font-mono text-[12px] text-pasture-200">
                {s.detail}
              </span>
            </div>
          ))}
        </InView>
      </div>
    </section>
  );
}

/** A short vertical connector between the stacked groups on phones, flowing down like the desktop lines. */
function Drop() {
  return (
    <svg
      aria-hidden="true"
      className="stack-lines stack-flow block mx-auto w-3 h-10"
      fill="none"
      viewBox="0 0 12 40"
    >
      <path
        d="M6 0 V40"
        stroke="#FFFFFF"
        strokeDasharray="4 5"
        strokeOpacity={0.7}
        strokeWidth="1.5"
      />
    </svg>
  );
}

/** Phones and tablets: the six tools, then the same six steps inside Boundary, top to bottom on the painting. */
function StackMobile() {
  return (
    <InView
      className="lg:hidden relative rounded-[14px] overflow-hidden bg-pasture px-4 py-6 sm:px-10 sm:py-10"
      light
    >
      <Image
        alt=""
        className="object-cover"
        fill
        quality={90}
        sizes="100vw"
        src="/boundary/painting-footer-4k.jpg"
      />
      <div className="relative flex flex-col max-w-[520px] mx-auto">
        <div className="grid grid-cols-2 gap-2">
          {STAGES.map((s, i) => (
            <div
              className={`stack-card flex flex-col gap-2 rounded-[10px] px-3.5 py-3 border ${
                s.note
                  ? 'border-dashed border-[#C9CCC0] bg-white/85'
                  : 'border-card-line bg-white'
              }`}
              key={s.stage}
              style={{ '--i': i } as CSSProperties}
            >
              <span className="text-[12px] leading-4 text-ink-3">
                {s.stage}
              </span>
              <Tools note={s.note} small tools={s.tools} />
            </div>
          ))}
        </div>

        <Drop />

        <div className="stack-core rounded-[14px] p-4 flex flex-col gap-4 bg-pasture border border-white/15 shadow-[0_1px_2px_rgba(20,40,20,0.12),0_24px_48px_-16px_rgba(20,40,20,0.45)]">
          <div className="flex flex-col gap-2.5 px-1">
            <img
              alt="Boundary"
              className="w-[105px] h-[18px]"
              src={logoWhite.src}
            />
            <p className="text-sm leading-5 text-white/80">
              One install. One name joins every step, so an agent can run the
              loop.
            </p>
          </div>
          <div className="flex flex-col gap-1.5">
            {STAGES.map((s) => (
              <div
                className={`flex flex-col gap-1 px-3 py-2.5 rounded-md border ${s.soon ? 'border-dashed border-white/30 bg-white/4' : 'border-white/15 bg-white/8'}`}
                key={s.stage}
              >
                <span className="flex items-center gap-2.5">
                  <span className="w-16 shrink-0 text-[12px] text-white/60">
                    {s.stage}
                  </span>
                  <span className="text-[14px] font-medium text-white">
                    {s.ours}
                  </span>
                  {s.soon && <Soon />}
                </span>
                <span className="pl-[74px] truncate font-mono text-[11px] text-pasture-200">
                  {s.detail}
                </span>
              </div>
            ))}
          </div>
        </div>
      </div>
    </InView>
  );
}
