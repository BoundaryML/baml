'use client';

import bamlMark from '@boundaryml/brand/logo/svg/baml-logomark-white.svg';
import bamlWhite from '@boundaryml/brand/logo/svg/baml-primary-white.svg';
import Image from 'next/image';
import { Fragment, useEffect, useState } from 'react';
import { Card, Frame } from '../diagram-styles/kit';
import { MarkTile } from './mark';
import { MARKS, type Mark } from './marks';

// The v4 hero diagram: where each toolchain reaches, from build to production. The languages fill
// Build and Test, then hand everything past the deploy line to separate products, drawn as grids of
// their logos. BAML's row is one block across every column, and a line runs from its end back to
// Build. It is drawn with the parts from /diagram-styles, so the style button can show it in any of
// that board's styles.

const BEFORE = ['Build', 'Test'];
const AFTER = ['Trace', 'Dashboards', 'Alerts', 'Feature flags'];
const STAGES = [...BEFORE, ...AFTER];

export const PRODUCTS: Mark[][] = [
  [MARKS.datadog, MARKS.honeycomb, MARKS.langsmith, MARKS.logfire],
  [MARKS.grafana, MARKS.datadog, MARKS.metabase],
  [MARKS.sentry, MARKS.pagerduty, MARKS.datadog],
  [MARKS.launchdarkly, MARKS.statsig, MARKS.posthog],
];

type Test = { command: string } | { marks: Mark[] } | { own: true };

const LANGUAGES: { build: string; mark: Mark; name: string; test: Test }[] = [
  { build: 'cc', mark: MARKS.c, name: 'C', test: { own: true } },
  {
    build: 'tsc',
    mark: MARKS.typescript,
    name: 'TypeScript',
    test: { marks: [MARKS.jest, MARKS.vitest, MARKS.mocha] },
  },
  {
    build: 'go build',
    mark: MARKS.go,
    name: 'Go',
    test: { command: 'go test' },
  },
];

const BAML = [
  'baml check',
  'baml test',
  'baml query',
  'baml dashboard',
  'baml alerts',
  'baml flags',
];

const FILL_MS = 6000;
const HOLD_MS = 3400;
const LAP_MS = FILL_MS + HOLD_MS;
// Once BAML's bar is full, a band of light crosses it, starting just after the fill lands.
const SWEEP_AT = FILL_MS + 150;
const SWEEP_MS = 1400;

const MUTED = 'rgb(from var(--ds-ink) r g b / 0.55)';
const GRID =
  'grid grid-cols-[52px_repeat(2,minmax(0,1fr))_14px_repeat(4,minmax(0,1fr))] md:grid-cols-[132px_repeat(2,minmax(0,1fr))_32px_repeat(4,minmax(0,1fr))] gap-x-1 md:gap-x-2 items-center';
const ROW = 'h-11 md:h-14';

const clamp = (n: number) => Math.min(1, Math.max(0, n));
const ease = (t: number) => (t < 0.5 ? 2 * t * t : 1 - (-2 * t + 2) ** 2 / 2);

/** The grid column a stage sits in: past the name column, and past the deploy line after Test. */
const column = (stage: number) => stage + 2 + (stage >= BEFORE.length ? 1 : 0);

/** Milliseconds into the lap, every frame. Under reduced motion it holds at the end of the fill. */
function useLap() {
  const [t, setT] = useState(0);
  useEffect(() => {
    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
      setT(FILL_MS + 1);
      return;
    }
    const begin = performance.now();
    let frame = requestAnimationFrame(function tick(now) {
      setT((now - begin) % LAP_MS);
      frame = requestAnimationFrame(tick);
    });
    return () => cancelAnimationFrame(frame);
  }, []);
  return t;
}

/** A card that fills from the left as the lap reaches its stage. */
function Fill({ fill, lit }: { fill: number; lit?: boolean }) {
  return (
    <span
      aria-hidden
      className="absolute inset-0 origin-left"
      style={{
        background: lit
          ? 'rgb(from var(--ds-accent) r g b / 0.35)'
          : 'rgb(from var(--ds-ink) r g b / 0.12)',
        transform: `scaleX(${fill})`,
      }}
    />
  );
}

function Command({ children, lit }: { children: string; lit?: boolean }) {
  return (
    <span
      className={`relative hidden md:flex h-full items-center justify-center px-1 font-mono text-[12px] lg:text-[13px] whitespace-nowrap truncate ${lit ? 'text-(--ds-accent-ink)' : 'text-(--ds-ink)/80'}`}
    >
      {children}
    </span>
  );
}

export default function Tracker({ style }: { style: string }) {
  const t = useLap();
  // How many stages the lap has filled, in stage widths.
  const p = ease(clamp(t / FILL_MS)) * STAGES.length;
  const deployed = p >= BEFORE.length;
  const handed = p >= BEFORE.length + 0.3;
  const back = t >= FILL_MS + 300;
  const full = t >= FILL_MS;
  // Where the light is across the bar, from 0 to 1. Under reduced motion it never starts.
  const sweep = ease(clamp((t - SWEEP_AT) / SWEEP_MS));
  const sweeping = t > SWEEP_AT && t < SWEEP_AT + SWEEP_MS;

  return (
    <div className={`ds-theme ds-theme-${style} rounded-[28px] p-2 sm:p-3`}>
      <Frame label="Where each toolchain reaches. C builds with cc and leaves testing to you. TypeScript builds with tsc and tests with Jest, Vitest, or Mocha. Go builds and tests with go build and go test. Past deploy, each of them hands tracing, dashboards, alerts, and feature flags to 11 separate products. BAML covers every stage itself, and a line runs from production back to build, labelled Improve.">
        <div className="relative p-3 md:p-6 lg:p-8">
          {/* Column labels. On a phone, the two sides of the line are labelled instead. */}
          <div
            className={`${GRID} mb-2 md:mb-3 text-[12.5px] md:text-[14px]`}
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
            <span className="md:hidden" style={{ gridColumn: '5 / span 4' }}>
              In production
            </span>
          </div>

          <div className="relative flex flex-col gap-1.5 md:gap-2">
            {/* The deploy line runs through every row. */}
            <div
              aria-hidden
              className={`${GRID} absolute inset-0 pointer-events-none`}
            >
              <span
                className="justify-self-center w-px h-[calc(100%+12px)] -mt-3 border-l border-dashed transition-[border-color] duration-500"
                style={{
                  borderColor: deployed ? 'var(--ds-accent)' : 'var(--ds-line)',
                  gridColumn: column(BEFORE.length) - 1,
                  gridRow: 1,
                }}
              />
            </div>

            {/* One grid for the language rows, so each product column can span all three. */}
            <div className={`${GRID} gap-y-1.5 md:gap-y-2`}>
              {LANGUAGES.map((lang, i) => (
                <Fragment key={lang.name}>
                  <div
                    className="flex items-center gap-2 min-w-0 text-(--ds-ink)/80"
                    style={{ gridColumn: 1, gridRow: i + 1 }}
                  >
                    <MarkTile mark={lang.mark} size={18} />
                    <span className="hidden md:block text-[15px] truncate">
                      {lang.name}
                    </span>
                  </div>

                  <Card
                    className={`relative ${ROW} overflow-hidden`}
                    style={{ gridColumn: column(0), gridRow: i + 1 }}
                  >
                    <Fill fill={clamp(p)} />
                    <Command>{lang.build}</Command>
                  </Card>

                  {'command' in lang.test ? (
                    <Card
                      className={`relative ${ROW} overflow-hidden`}
                      style={{ gridColumn: column(1), gridRow: i + 1 }}
                    >
                      <Fill fill={clamp(p - 1)} />
                      <Command>{lang.test.command}</Command>
                    </Card>
                  ) : (
                    // No test tool of its own: a dashed cell, like the products past the line.
                    <div
                      className={`${ROW} hidden md:flex items-center justify-center gap-2.5 rounded-(--ds-card-radius) border border-dashed border-(--ds-line) text-[12.5px] text-center text-(--ds-ink)/80`}
                      style={{
                        gridColumn: column(1),
                        gridRow: i + 1,
                        ...('own' in lang.test ? { color: MUTED } : {}),
                      }}
                    >
                      {'own' in lang.test
                        ? 'Bring your own'
                        : lang.test.marks.map((m) => (
                            <MarkTile key={m.name} mark={m} size={18} />
                          ))}
                    </div>
                  )}
                </Fragment>
              ))}

              {/* Past the line, each stage is a set of separate products, the same for every language. */}
              {PRODUCTS.map((marks, i) => (
                <div
                  className="self-stretch hidden md:flex flex-wrap content-center items-center justify-center gap-3 px-2 rounded-(--ds-card-radius) border border-dashed transition-[border-color,opacity] duration-500 text-(--ds-ink)/75"
                  key={AFTER[i]}
                  style={{
                    borderColor: handed
                      ? 'var(--ds-line)'
                      : 'rgb(from var(--ds-line) r g b / 0.35)',
                    gridColumn: column(BEFORE.length + i),
                    gridRow: `1 / span ${LANGUAGES.length}`,
                    opacity: handed ? 1 : 0.15,
                  }}
                >
                  {marks.map((m) => (
                    <MarkTile key={m.name} mark={m} size={22} />
                  ))}
                </div>
              ))}
              <p
                className="self-stretch md:hidden flex items-center justify-center rounded-(--ds-card-radius) border border-dashed border-(--ds-line) px-1 text-center text-[11.5px] transition-opacity duration-500"
                style={{
                  color: MUTED,
                  gridColumn: `${column(BEFORE.length)} / span ${AFTER.length}`,
                  gridRow: `1 / span ${LANGUAGES.length}`,
                  opacity: handed ? 1 : 0.15,
                }}
              >
                11 separate products
              </p>
            </div>

            <div className={`${GRID}`}>
              <p
                className="hidden md:block text-center text-[12.5px] md:text-[13px] pt-0.5"
                style={{
                  color: MUTED,
                  gridColumn: `${column(BEFORE.length)} / span ${AFTER.length}`,
                }}
              >
                Separate products, each with its own SDK
              </p>
            </div>

            <div className={`${GRID} relative mt-1 md:mt-2`}>
              {/* BAML's logo at the brand's 32px minimum: the logomark fits the phone's narrow name
                  column, the primary logo the wide one. Every diagram style is dark, so both are white. */}
              <div className="flex items-center min-w-0">
                <Image
                  alt="BAML"
                  className="md:hidden h-8 w-auto"
                  height={32}
                  src={bamlMark}
                  width={27}
                />
                <Image
                  alt="BAML"
                  className="hidden md:block h-8 w-auto"
                  height={32}
                  src={bamlWhite}
                  width={83}
                />
              </div>
              <Card
                className={`relative ${ROW} overflow-hidden`}
                lit
                style={{ gridColumn: '2 / -1' }}
              >
                <span
                  aria-hidden
                  className="absolute inset-0 origin-left"
                  style={{
                    // The fill brightens a step when it reaches the end.
                    background: `rgb(from var(--ds-accent) r g b / ${full ? 0.42 : 0.3})`,
                    transform: `scaleX(${clamp(p / STAGES.length)})`,
                    transition: 'background-color 400ms',
                  }}
                />
                {/* A soft band of light crosses the full bar once per lap, as if the bar were charged. */}
                <span
                  aria-hidden
                  className="absolute inset-0 pointer-events-none"
                  style={{
                    opacity: sweeping ? 1 : 0,
                    transform: `translateX(${-40 + sweep * 180}%)`,
                  }}
                >
                  <span
                    className="absolute inset-y-0 left-0 w-[36%]"
                    style={{
                      background:
                        'linear-gradient(90deg, transparent, rgb(from var(--ds-accent-ink) r g b / 0.4), transparent)',
                      filter: 'blur(4px)',
                    }}
                  />
                </span>
              </Card>
              {/* The commands sit over the block, in the same columns as the stages above them. */}
              <div className={`${GRID} absolute inset-0 pointer-events-none`}>
                {BAML.map((command, i) => (
                  <span
                    className={`${ROW} min-w-0 transition-opacity duration-300`}
                    key={command}
                    style={{
                      gridColumn: column(i),
                      gridRow: 1,
                      opacity: clamp(p - i) > 0.4 ? 1 : 0.35,
                    }}
                  >
                    <Command lit>{command}</Command>
                  </span>
                ))}
              </div>
            </div>

            {/* BAML's row runs back from feature flags to build. */}
            <div aria-hidden className={`${GRID} h-9 md:h-12`}>
              <div
                className="relative self-start h-[70%] mx-[4%]"
                style={{ gridColumn: '2 / -1' }}
              >
                <span
                  className="absolute inset-0 rounded-b-[12px] border border-t-0 border-(--ds-accent)"
                  style={{
                    clipPath: back
                      ? 'inset(0 -2px -2px 0)'
                      : 'inset(0 -2px -2px 100%)',
                    transition: back
                      ? 'clip-path 1400ms cubic-bezier(0.65,0,0.35,1)'
                      : 'none',
                  }}
                />
                <span
                  className="absolute left-1/2 bottom-0 translate-y-1/2 -translate-x-1/2 px-3 bg-(--ds-ground) text-[12.5px] md:text-[14px] text-(--ds-accent-ink) transition-opacity duration-500"
                  style={{ opacity: back ? 1 : 0 }}
                >
                  Improve
                </span>
              </div>
            </div>
          </div>
        </div>
      </Frame>
    </div>
  );
}
