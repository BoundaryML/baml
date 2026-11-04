'use client';

import { useEffect, useState } from 'react';
import { Card, Frame, Lamb } from '../diagram-styles/kit';

// The delivery tracker again, one row per language, drawn as a calm product screen. Every toolchain fills
// its row up to the deploy line. Past it, the other languages hand off to separate products, and only
// BAML's row keeps going through production and back to build. It is drawn with the parts from
// /diagram-styles, so the switcher can show it in any of that board's styles: every color, border, and
// radius comes from the .ds-theme-* class around it.
const STAGES = [
  'Build',
  'Test',
  'Trace',
  'Query',
  'Errors',
  'Dashboards',
  'Alerts',
];
const BEFORE = 2;

const LANGUAGES = [
  { commands: ['python', 'pytest'], name: 'Python' },
  { commands: ['tsc', 'vitest'], name: 'TypeScript' },
  { commands: ['go build', 'go test'], name: 'Go' },
];

// One color per separate product, so the hand-wired side reads as a patchwork.
const PRODUCTS = ['#E8A35C', '#6FB7C9', '#D98BB3', '#A7C47A', '#C9A0E8'];

const FILL_MS = 6000;
const HOLD_MS = 3200;
const LAP_MS = FILL_MS + HOLD_MS;

const MUTED = 'rgb(from var(--ds-ink) r g b / 0.55)';
const GRID =
  'grid grid-cols-[78px_repeat(2,minmax(0,1.25fr))_16px_repeat(5,minmax(0,1fr))] md:grid-cols-[150px_repeat(2,minmax(0,1.25fr))_44px_repeat(5,minmax(0,1fr))] gap-x-1 md:gap-x-1.5 items-center';

const clamp = (n: number) => Math.min(1, Math.max(0, n));
const ease = (t: number) => (t < 0.5 ? 2 * t * t : 1 - (-2 * t + 2) ** 2 / 2);

/** The grid column a stage sits in, past the name column and around the deploy line. */
const column = (stage: number) => stage + 2 + (stage >= BEFORE ? 1 : 0);

function Cell({
  baml,
  fill,
  label,
  stage,
}: {
  baml?: boolean;
  fill: number;
  label?: string;
  stage: number;
}) {
  return (
    <Card
      className="relative h-10 md:h-12 overflow-hidden"
      lit={baml}
      style={{ gridColumn: column(stage), gridRow: 1 }}
    >
      <span
        aria-hidden
        className="absolute inset-0 origin-left"
        style={{
          background: baml
            ? 'rgb(from var(--ds-accent) r g b / 0.4)'
            : 'rgb(from var(--ds-ink) r g b / 0.12)',
          transform: `scaleX(${fill})`,
        }}
      />
      {label && (
        <span
          className="relative hidden md:flex h-full items-center justify-center px-1 font-mono text-[12px] text-(--ds-ink) whitespace-nowrap transition-opacity duration-300"
          style={{ opacity: fill > 0.4 ? 0.8 : 0.3 }}
        >
          {label}
        </span>
      )}
    </Card>
  );
}

export default function HeroCoverage({
  style = 'midnight',
}: {
  style?: string;
}) {
  const [t, setT] = useState(0);

  useEffect(() => {
    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
      setT(FILL_MS + 1);
      return;
    }
    const begin = performance.now();
    let frame = 0;
    const tick = (now: number) => {
      setT((now - begin) % LAP_MS);
      frame = requestAnimationFrame(tick);
    };
    frame = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(frame);
  }, []);

  // How many stages the lap has filled, in stage widths.
  const p = ease(clamp(t / FILL_MS)) * STAGES.length;
  const deployed = p >= BEFORE;
  const handed = p >= BEFORE + 0.35;
  const back = t >= FILL_MS + 300;

  return (
    <div
      className={`ds-theme ds-theme-${style} rounded-[28px] p-2 sm:p-3 mb-16`}
    >
      <Frame label="A product screen comparing toolchains. Python, TypeScript, and Go fill build and test, then stop at the deploy line and hand off to separate products. BAML fills build, test, trace, query, errors, dashboards, and alerts, and runs back to build.">
        <div className="flex flex-wrap items-center justify-between gap-x-6 gap-y-3 px-4 md:px-7 pt-4 md:pt-6">
          <div>
            <p className="font-display font-medium text-[16px] md:text-[19px] tracking-[-0.02em]">
              Where each toolchain reaches
            </p>
            <p
              className="mt-0.5 text-[13px] md:text-[14px]"
              style={{ color: MUTED }}
            >
              One change to a classify function, from build to production
            </p>
          </div>
          <ul
            className="flex items-center gap-4 text-[12.5px] md:text-[13px]"
            style={{ color: MUTED }}
          >
            <li className="flex items-center gap-2">
              <span className="w-4 h-2.5 rounded-[3px] bg-(--ds-accent)/60" />
              In the toolchain
            </li>
            <li className="flex items-center gap-2">
              <span className="flex -space-x-1">
                {PRODUCTS.slice(0, 3).map((c) => (
                  <span
                    className="size-2.5 rounded-full ring-2 ring-(--ds-ground)"
                    key={c}
                    style={{ background: c }}
                  />
                ))}
              </span>
              Separate products
            </li>
          </ul>
        </div>

        <div className="relative px-4 md:px-7 pt-6 md:pt-8 pb-5 md:pb-7">
          {/* Column labels. On a phone, the two sides of the line are labelled instead. */}
          <div
            className={`${GRID} mb-2.5 text-[12.5px] md:text-[13px]`}
            style={{ color: MUTED }}
          >
            {STAGES.map((s, i) => (
              <span
                className="hidden md:block text-center truncate"
                key={s}
                style={{ gridColumn: column(i) }}
              >
                {s}
              </span>
            ))}
            <span className="md:hidden" style={{ gridColumn: '2 / span 2' }}>
              Before
            </span>
            <span className="md:hidden" style={{ gridColumn: '5 / span 5' }}>
              In production
            </span>
            <span
              className="hidden md:block text-center"
              style={{
                color: deployed ? 'var(--ds-ink)' : MUTED,
                gridColumn: 4,
              }}
            >
              Deploy
            </span>
          </div>

          <div className="relative flex flex-col gap-2 md:gap-2.5">
            {/* The deploy line runs through every row. */}
            <div
              aria-hidden
              className={`${GRID} absolute inset-0 pointer-events-none`}
            >
              <span
                className="justify-self-center w-px h-[calc(100%+24px)] transition-[background-color] duration-500"
                style={{
                  background: deployed ? 'var(--ds-accent)' : 'var(--ds-line)',
                  gridColumn: 4,
                  gridRow: 1,
                }}
              />
            </div>

            {LANGUAGES.map((lang) => (
              <div className={`${GRID} relative`} key={lang.name}>
                <div className="min-w-0 pr-2" style={{ gridRow: 1 }}>
                  <p className="text-[13px] md:text-[15px] truncate">
                    {lang.name}
                  </p>
                </div>
                {lang.commands.map((command, i) => (
                  <Cell
                    fill={clamp(Math.min(p, BEFORE) - i)}
                    key={command}
                    label={command}
                    stage={i}
                  />
                ))}
                {/* Past the line, the row is handed to products this toolchain doesn't include. */}
                <div
                  className={`flex items-center justify-center gap-2 md:gap-3 h-10 md:h-12 rounded-[9px] md:rounded-[10px] border border-dashed transition-[border-color] duration-500 *:transition-opacity *:duration-500 ${handed ? '' : '*:opacity-0'}`}
                  style={{
                    borderColor: handed
                      ? 'var(--ds-line)'
                      : 'rgb(from var(--ds-line) r g b / 0.4)',
                    gridColumn: `${column(BEFORE)} / span ${STAGES.length - BEFORE}`,
                    gridRow: 1,
                  }}
                >
                  <span className="flex -space-x-1">
                    {PRODUCTS.map((c) => (
                      <span
                        className="size-2 md:size-2.5 rounded-full ring-2 ring-(--ds-ground)"
                        key={c}
                        style={{ background: c }}
                      />
                    ))}
                  </span>
                  <span
                    className="text-[11.5px] md:text-[13px] whitespace-nowrap"
                    style={{ color: MUTED }}
                  >
                    <span className="md:hidden">5 products</span>
                    <span className="hidden md:inline">
                      5 separate products, wired by hand
                    </span>
                  </span>
                </div>
              </div>
            ))}

            <div
              className={`${GRID} relative mt-2 md:mt-3 py-2 md:py-2.5 -mx-2 md:-mx-3 px-2 md:px-3 rounded-[14px] transition-[background-color,box-shadow] duration-500`}
              style={{
                background: back
                  ? 'rgb(from var(--ds-accent) r g b / 0.08)'
                  : 'transparent',
                boxShadow: back
                  ? 'inset 0 0 0 1px rgb(from var(--ds-accent) r g b / 0.4)'
                  : 'inset 0 0 0 1px var(--ds-line)',
              }}
            >
              <div className="flex items-center gap-2 min-w-0">
                <span className="hidden md:block">
                  <Lamb size={20} />
                </span>
                <p className="font-display font-medium text-[15px] md:text-[19px] tracking-[-0.02em]">
                  BAML
                </p>
              </div>
              {STAGES.map((s, i) => (
                <Cell baml fill={clamp(p - i)} key={s} stage={i} />
              ))}
            </div>

            {/* BAML's row runs back from alerts to build. */}
            <div aria-hidden className={`${GRID} h-7 md:h-9`}>
              <span
                className="self-start h-[80%] mx-[3%] rounded-b-[12px] border border-t-0 border-(--ds-accent)"
                style={{
                  clipPath: back
                    ? 'inset(0 -20px -20px 0)'
                    : 'inset(0 -20px -20px 100%)',
                  gridColumn: '2 / -1',
                  transition: back
                    ? 'clip-path 1400ms cubic-bezier(0.65,0,0.35,1)'
                    : 'none',
                }}
              />
            </div>
          </div>
        </div>
      </Frame>
    </div>
  );
}
