'use client';

import bamlMark from '@boundaryml/brand/logo/svg/baml-logomark-white.svg';
import bamlWhite from '@boundaryml/brand/logo/svg/baml-primary-white.svg';
import Image from 'next/image';
import {
  type CSSProperties,
  Fragment,
  useEffect,
  useRef,
  useState,
} from 'react';
import { Card, Frame } from './kit';
import { MarkTile } from './mark';
import { MARKS, type Mark } from './marks';

// The v4 hero diagram: where each toolchain reaches, from build to production. The languages fill
// Build and Test, then hand everything past the deploy line to separate products, drawn as grids of
// their logos, which appear one stage at a time. Deploy is a labelled line in a gap of its own, so the
// cells on either side of it never touch. BAML's row is the one thing that crosses it: a single bar that
// narrows to a thick neck over the line, and a line runs from its end back to Build. It is drawn with
// the parts from /diagram-styles, so the style button can show it in any of that board's styles.

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

// Each language is named, with no logo, since C's logo is just the letter again. `short` is the name
// on a phone, where the column is narrow. `year` is when the language first appeared, shown faintly
// under its name.
const LANGUAGES: {
  build: string;
  name: string;
  short: string;
  test: Test;
  year: number;
}[] = [
  { build: 'cc', name: 'C', short: 'C', test: { own: true }, year: 1972 },
  {
    build: 'tsc',
    name: 'TypeScript',
    short: 'TS',
    test: { marks: [MARKS.jest, MARKS.vitest, MARKS.mocha] },
    year: 2012,
  },
  {
    build: 'go build',
    name: 'Go',
    short: 'Go',
    test: { command: 'go test' },
    year: 2009,
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
  'grid grid-cols-[52px_repeat(2,minmax(0,1fr))_14px_repeat(4,minmax(0,1fr))] md:grid-cols-[132px_repeat(2,minmax(0,1fr))_48px_repeat(4,minmax(0,1fr))] gap-x-1 md:gap-x-2 items-center';
// Row and padding sizes are variables set by .tracker in home.css, which shrinks them on short screens.
const ROW = 'h-(--tracker-row)';

/** `n` held between 0 and 1. */
const clamp = (n: number) => Math.min(1, Math.max(0, n));
/** Quadratic ease in and out over 0 to 1. */
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

/** A card that fills from the left as the lap reaches its stage, or from the top in the phone's
 *  vertical diagram. */
function Fill({ fill, vertical }: { fill: number; vertical?: boolean }) {
  return (
    <span
      aria-hidden
      className={`absolute inset-0 ${vertical ? 'origin-top' : 'origin-left'}`}
      style={{
        background: 'rgb(from var(--ds-ink) r g b / 0.12)',
        transform: vertical ? `scaleY(${fill})` : `scaleX(${fill})`,
      }}
    />
  );
}

/** The outline of BAML's bar, `w` long and `h` thick, pinched between `a` and `b` (the deploy gap) to
 *  a neck 24% of its thickness. Its outer corners take the style's card radius `r`. It runs left to
 *  right, or top to bottom when `vertical`: the same outline with x and y swapped, which mirrors it, so
 *  its corner arcs turn the other way. */
function barPath(
  w: number,
  h: number,
  a: number,
  b: number,
  r: number,
  vertical = false,
) {
  const t = h * 0.38;
  const s = Math.min(12, (b - a) / 4);
  /** The point `x` along the bar and `y` across it, in the drawing's coordinates. */
  const pt = (x: number, y: number) => (vertical ? `${y} ${x}` : `${x} ${y}`);
  const arc = (x: number, y: number) =>
    `A ${r} ${r} 0 0 ${vertical ? 0 : 1} ${pt(x, y)}`;
  return [
    `M ${pt(r, 0)}`,
    `L ${pt(a - s, 0)}`,
    `C ${pt(a, 0)} ${pt(a, t)} ${pt(a + s, t)}`,
    `L ${pt(b - s, t)}`,
    `C ${pt(b, t)} ${pt(b, 0)} ${pt(b + s, 0)}`,
    `L ${pt(w - r, 0)}`,
    arc(w, r),
    `L ${pt(w, h - r)}`,
    arc(w - r, h),
    `L ${pt(b + s, h)}`,
    `C ${pt(b, h)} ${pt(b, h - t)} ${pt(b - s, h - t)}`,
    `L ${pt(a + s, h - t)}`,
    `C ${pt(a, h - t)} ${pt(a, h)} ${pt(a - s, h)}`,
    `L ${pt(r, h)}`,
    arc(0, h - r),
    `L ${pt(0, r)}`,
    arc(r, 0),
    'Z',
  ].join(' ');
}

type Shape = {
  a: number;
  b: number;
  d: string;
  h: number;
  len: number;
  w: number;
};

/** BAML's bar, from Build to Feature flags in one piece. It is measured against its grid so its neck
 *  sits exactly over the deploy gap: across the columns, or down the rows when `vertical` (the phone's
 *  diagram, where `style` places it in its column). Until it is measured it draws as a plain card. It is
 *  keyed by the diagram style, so a style with a different card radius measures it again. */
function BamlBar({
  full,
  p,
  style,
  sweep,
  vertical = false,
}: {
  full: boolean;
  p: number;
  style?: CSSProperties;
  sweep: number;
  vertical?: boolean;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [shape, setShape] = useState<Shape | null>(null);

  useEffect(() => {
    const bar = ref.current;
    const grid = bar?.parentElement;
    if (!bar || !grid) return;
    const measure = () => {
      const css = getComputedStyle(grid);
      const tracks = (vertical ? css.gridTemplateRows : css.gridTemplateColumns)
        .split(' ')
        .map(Number.parseFloat);
      const gap = Number.parseFloat(vertical ? css.rowGap : css.columnGap) || 0;
      const w = bar.clientWidth;
      const h = bar.clientHeight;
      const [len, thick] = vertical ? [h, w] : [w, h];
      const r = Math.min(
        thick / 2,
        Number.parseFloat(
          getComputedStyle(bar).getPropertyValue('--ds-card-radius'),
        ) || 0,
      );
      // The bar starts at the first stage's track; the neck runs from the end of Test to the start
      // of Trace, gaps included.
      const a = tracks[1] + gap + tracks[2];
      const b = a + gap + tracks[3] + gap;
      setShape({ a, b, d: barPath(len, thick, a, b, r, vertical), h, len, w });
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(grid);
    return () => observer.disconnect();
  }, [vertical]);

  // Where the fill has reached, as a share of the bar's length: each side moves at its own stages'
  // pace, and the neck fills quickly in between.
  const at = (() => {
    if (!shape?.len) return p / STAGES.length;
    const { a, b, len } = shape;
    const n = BEFORE.length;
    const x =
      p <= n
        ? (p / n) * a
        : p <= n + 0.2
          ? a + ((p - n) / 0.2) * (b - a)
          : b + ((p - n - 0.2) / (AFTER.length - 0.2)) * (len - b);
    return clamp(x / len);
  })();

  const inner = (
    <>
      <span
        aria-hidden
        className={`absolute inset-0 ${vertical ? 'origin-top' : 'origin-left'}`}
        style={{
          // The fill brightens a step when the whole bar is full.
          background: `rgb(from var(--ds-accent) r g b / ${full ? 0.42 : 0.3})`,
          transform: vertical ? `scaleY(${at})` : `scaleX(${at})`,
          transition: 'background-color 400ms',
        }}
      />
      <span
        aria-hidden
        className="absolute inset-0 pointer-events-none"
        style={{
          opacity: sweep > 0 && sweep < 1 ? 1 : 0,
          transform: vertical
            ? `translateY(${-18 + sweep * 118}%)`
            : `translateX(${-18 + sweep * 118}%)`,
        }}
      >
        <span
          className={`absolute left-0 top-0 ${vertical ? 'inset-x-0 h-[18%]' : 'inset-y-0 w-[18%]'}`}
          style={{
            background: `linear-gradient(${vertical ? 180 : 90}deg, transparent, rgb(from var(--ds-accent-ink) r g b / 0.4), transparent)`,
            filter: 'blur(4px)',
          }}
        />
      </span>
    </>
  );

  return (
    <div
      className={vertical ? 'relative self-stretch' : `relative ${ROW}`}
      ref={ref}
      style={style ?? { gridColumn: '2 / -1' }}
    >
      {shape?.len ? (
        <>
          <div
            className="absolute inset-0 overflow-hidden"
            // No backdrop blur: Chrome blurs the whole box behind a path clip, which showed as square
            // corners. The frame under the bar is blurred already.
            style={{
              background: 'var(--ds-card)',
              clipPath: `path('${shape.d}')`,
            }}
          >
            {inner}
          </div>
          <svg
            aria-hidden
            className="absolute inset-0 overflow-visible pointer-events-none"
            height={shape.h}
            viewBox={`0 0 ${shape.w} ${shape.h}`}
            width={shape.w}
          >
            <title>BAML's bar outline</title>
            <path
              d={shape.d}
              fill="none"
              stroke="var(--ds-lit-border)"
              strokeWidth={1}
            />
          </svg>
        </>
      ) : (
        <Card className="absolute inset-0 overflow-hidden" lit>
          {inner}
        </Card>
      )}
    </div>
  );
}

/** A build or test command in a stage's cell. From 640px it shows in small type, wrapping onto two
 *  lines where a cell is narrow; from 768px it sits on one line. Phones have no room for it. `lit`
 *  gives it the accent. */
function Command({ children, lit }: { children: string; lit?: boolean }) {
  return (
    <span
      className={`relative hidden sm:flex h-full items-center justify-center px-1 font-mono text-[10px] leading-[1.2] text-center md:text-[12px] lg:text-[13px] md:whitespace-nowrap md:truncate ${lit ? 'text-(--ds-accent-ink)' : 'text-(--ds-ink)/80'}`}
    >
      {children}
    </span>
  );
}

/** The lap's state at one moment, shared by the wide diagram and the phone's. */
type Lap = {
  back: boolean;
  deployed: boolean;
  full: boolean;
  handed: boolean;
  lineColor: string;
  p: number;
  shown: (i: number) => boolean;
  style: string;
  sweep: number;
};

// The phone's diagram turns the wide one on its side: stages run down the rows, the languages and
// BAML across the columns. The columns are the stage names, the three languages, BAML, and a narrow
// one for the line back from feature flags to build. The rows are the column heads, Build and Test,
// the deploy line, then the four stages past it; `column` numbers them the same way.
const VGRID =
  'grid grid-cols-[68px_repeat(3,minmax(0,1fr))_92px_12px] grid-rows-[32px_repeat(2,36px)_20px_repeat(4,36px)] gap-1.5';

/** The hero diagram on a phone, with BAML's bar filling from the top down. */
function Vertical({
  back,
  deployed,
  full,
  handed,
  lineColor,
  p,
  shown,
  style,
  sweep,
}: Lap) {
  const deployRow = column(BEFORE.length) - 1;
  return (
    <div className="sm:hidden">
      <div className={`${VGRID} relative`}>
        {LANGUAGES.map((lang, i) => (
          <span
            className="self-center text-center text-[13px] text-(--ds-ink)/80"
            key={lang.name}
            style={{ gridColumn: i + 2, gridRow: 1 }}
          >
            {lang.short}
          </span>
        ))}
        <span
          className="self-center justify-self-center"
          style={{ gridColumn: 5, gridRow: 1 }}
        >
          <Image
            alt="BAML"
            className="h-8 w-auto"
            height={32}
            src={bamlMark}
            width={27}
          />
        </span>

        {STAGES.map((s, i) => (
          <span
            className="self-center text-[12px] leading-tight transition-opacity duration-500"
            key={s}
            style={{
              color: MUTED,
              gridColumn: 1,
              gridRow: column(i),
              opacity: i < BEFORE.length || shown(i - BEFORE.length) ? 1 : 0,
            }}
          >
            {s}
          </span>
        ))}

        {/* The deploy line, across the languages and through BAML's neck. */}
        <span
          className="self-center font-mono text-[11px] transition-colors duration-500"
          style={{
            color: deployed ? 'var(--ds-accent-ink)' : MUTED,
            gridColumn: 1,
            gridRow: deployRow,
          }}
        >
          deploy
        </span>
        <span
          aria-hidden
          className="self-center h-0 border-t-2 border-dashed transition-[border-color] duration-500"
          style={{
            borderColor: lineColor,
            gridColumn: '2 / 6',
            gridRow: deployRow,
          }}
        />

        {LANGUAGES.map((lang, i) => (
          <Fragment key={lang.name}>
            <Card
              className="relative overflow-hidden"
              style={{ gridColumn: i + 2, gridRow: column(0) }}
            >
              <Fill fill={clamp(p)} vertical />
            </Card>
            {'command' in lang.test ? (
              <Card
                className="relative overflow-hidden"
                style={{ gridColumn: i + 2, gridRow: column(1) }}
              >
                <Fill fill={clamp(p - 1)} vertical />
              </Card>
            ) : (
              // No test tool of its own: a dashed cell, like the products past the line.
              <span
                className="rounded-(--ds-card-radius) border border-dashed border-(--ds-line)"
                style={{ gridColumn: i + 2, gridRow: column(1) }}
              />
            )}
          </Fragment>
        ))}

        {PRODUCTS.map((marks, i) => (
          <div
            className="flex items-center justify-center gap-1.5 rounded-(--ds-card-radius) border border-dashed border-(--ds-line) text-(--ds-ink)/75 transition-opacity duration-500"
            key={AFTER[i]}
            style={{
              gridColumn: '2 / 5',
              gridRow: column(BEFORE.length + i),
              opacity: shown(i) ? 1 : 0,
            }}
          >
            {marks.map((m) => (
              <MarkTile key={m.name} mark={m} size={16} />
            ))}
          </div>
        ))}

        <BamlBar
          full={full}
          key={style}
          p={p}
          style={{ gridColumn: 5, gridRow: `${column(0)} / -1` }}
          sweep={sweep}
          vertical
        />
        {BAML.map((command, i) => (
          <span
            className="self-center text-center font-mono text-[10px] text-(--ds-accent-ink) whitespace-nowrap pointer-events-none transition-opacity duration-300"
            key={command}
            style={{
              gridColumn: 5,
              gridRow: column(i),
              opacity: clamp(p - i) > 0.4 ? 1 : 0.35,
            }}
          >
            {command}
          </span>
        ))}

        {/* BAML's line runs back up from feature flags to build. */}
        <span
          aria-hidden
          className="relative"
          style={{ gridColumn: 6, gridRow: `${column(0)} / -1` }}
        >
          <span
            className="absolute inset-y-[18px] -left-1.5 right-0.5 rounded-r-[10px] border border-l-0 border-(--ds-accent)"
            style={{
              clipPath: back
                ? 'inset(-2px -2px -2px 0)'
                : 'inset(100% -2px -2px 0)',
              opacity: back ? 1 : 0,
              transition: back
                ? 'clip-path 1400ms cubic-bezier(0.65,0,0.35,1)'
                : 'none',
            }}
          />
          <span
            className="absolute right-0.5 top-1/2 translate-x-1/2 -translate-y-1/2 rotate-180 py-2 bg-(--ds-ground) text-[11px] text-(--ds-accent-ink) [writing-mode:vertical-rl] transition-opacity duration-500"
            style={{ opacity: back ? 1 : 0 }}
          >
            Improve
          </span>
        </span>
      </div>
      <p
        className="mt-3 text-[11.5px] transition-opacity duration-500"
        style={{ color: MUTED, opacity: handed ? 1 : 0 }}
      >
        Each logo past deploy is a separate product with its own SDK.
      </p>
    </div>
  );
}

/** The v4 hero diagram in the given /diagram-styles style, played as a looping lap from build to
 *  production. */
export default function Tracker({ style }: { style: string }) {
  const t = useLap();
  // How many stages the lap has filled, in stage widths.
  const p = ease(clamp(t / FILL_MS)) * STAGES.length;
  const deployed = p >= BEFORE.length;
  const handed = p >= BEFORE.length + 0.3;
  const back = t >= FILL_MS + 300;
  const full = t >= FILL_MS;
  // Where the light is across the bar, from 0 to 1. Under reduced motion it never starts.
  const sweep = t > SWEEP_AT ? ease(clamp((t - SWEEP_AT) / SWEEP_MS)) : 0;
  // Past the line, each stage's products appear as the lap reaches that stage, one after another.
  const shown = (i: number) => p >= BEFORE.length + i + 0.15;
  const lineColor = deployed
    ? 'var(--ds-accent)'
    : 'rgb(from var(--ds-ink) r g b / 0.45)';

  return (
    <div
      className={`tracker ds-theme ds-theme-${style} rounded-[28px] p-2 sm:p-3`}
    >
      <Frame label="Where each toolchain reaches. C builds with cc and leaves testing to you. TypeScript builds with tsc and tests with Jest, Vitest, or Mocha. Go builds and tests with go build and go test. Past deploy, each of them hands tracing, dashboards, alerts, and feature flags to 11 separate products. BAML covers every stage itself, and a line runs from production back to build, labelled Improve.">
        <div className="relative p-(--tracker-pad)">
          <Vertical
            back={back}
            deployed={deployed}
            full={full}
            handed={handed}
            lineColor={lineColor}
            p={p}
            shown={shown}
            style={style}
            sweep={sweep}
          />
          <div className="relative hidden sm:block">
            {/* The deploy line, drawn once from under its label to the bottom of BAML's row, in the gap
                column, so it runs through every row without touching the cells on either side. */}
            <div
              aria-hidden
              className={`${GRID} absolute inset-x-0 top-(--tracker-head) bottom-[calc(var(--tracker-ret)+6px)] md:bottom-[calc(var(--tracker-ret)+8px)] pointer-events-none`}
            >
              <span
                className="justify-self-center self-stretch w-0 border-l-2 border-dashed transition-[border-color] duration-500"
                style={{
                  borderColor: lineColor,
                  gridColumn: column(BEFORE.length) - 1,
                  gridRow: 1,
                }}
              />
            </div>

            <div className={`${GRID} h-(--tracker-head)`}>
              <span
                className="justify-self-center font-mono text-[11px] md:text-[13px] whitespace-nowrap transition-colors duration-500"
                style={{
                  color: deployed ? 'var(--ds-accent-ink)' : MUTED,
                  gridColumn: column(BEFORE.length) - 1,
                }}
              >
                deploy
              </span>
              <span
                className="hidden md:block justify-self-center text-[15px] font-medium text-(--ds-ink)"
                style={{
                  gridColumn: `${column(BEFORE.length)} / span ${AFTER.length}`,
                }}
              >
                Production
              </span>
            </div>

            {/* Column labels. On a phone, the two sides of the line are labelled instead. */}
            <div
              className={`${GRID} mb-2 md:mb-3 text-[12.5px] md:text-[14px]`}
              style={{ color: MUTED }}
            >
              {STAGES.map((s, i) => (
                <span
                  className="hidden md:block text-center truncate transition-opacity duration-500"
                  key={s}
                  style={{
                    gridColumn: column(i),
                    opacity:
                      i < BEFORE.length || shown(i - BEFORE.length) ? 1 : 0,
                  }}
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
              {/* One grid for the language rows, so each product column can span all three. */}
              <div className={`${GRID} gap-y-1.5 md:gap-y-2`}>
                {LANGUAGES.map((lang, i) => (
                  <Fragment key={lang.name}>
                    <div
                      className="flex items-center gap-2 min-w-0 text-(--ds-ink)/80"
                      style={{ gridColumn: 1, gridRow: i + 1 }}
                    >
                      <span className="md:hidden text-[14px]">
                        {lang.short}
                      </span>
                      <span className="hidden md:flex flex-col min-w-0 leading-tight">
                        <span className="text-[15px] truncate">
                          {lang.name}
                        </span>
                        <span className="text-[12px] text-faint">
                          {lang.year}
                        </span>
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
                      // No test tool of its own: a dashed cell, like the products past the line. On a
                      // phone it stays, empty, so every language row has a build and a test cell.
                      <div
                        className={`${ROW} flex items-center justify-center gap-2.5 rounded-(--ds-card-radius) border border-dashed border-(--ds-line) text-[12.5px] text-center text-(--ds-ink)/80 *:max-md:hidden`}
                        style={{
                          gridColumn: column(1),
                          gridRow: i + 1,
                          ...('own' in lang.test ? { color: MUTED } : {}),
                        }}
                      >
                        {'own' in lang.test ? (
                          <span>Bring your own</span>
                        ) : (
                          lang.test.marks.map((m) => (
                            <MarkTile key={m.name} mark={m} size={18} />
                          ))
                        )}
                      </div>
                    )}
                  </Fragment>
                ))}

                {/* Past the line, each stage is a set of separate products, the same for every language. */}
                {PRODUCTS.map((marks, i) => (
                  <div
                    className="self-stretch hidden md:flex flex-wrap content-center items-center justify-center gap-3 px-2 rounded-(--ds-card-radius) border border-dashed border-(--ds-line) transition-[opacity,translate] duration-500 text-(--ds-ink)/75"
                    key={AFTER[i]}
                    style={{
                      gridColumn: column(BEFORE.length + i),
                      gridRow: `1 / span ${LANGUAGES.length}`,
                      opacity: shown(i) ? 1 : 0,
                      translate: shown(i) ? '0 0' : '0 6px',
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
                    opacity: handed ? 1 : 0,
                  }}
                >
                  11 separate products
                </p>
              </div>

              <div className={`${GRID} tracker-caption`}>
                <p
                  className="hidden md:block text-center text-[12.5px] md:text-[13px] pt-0.5 transition-opacity duration-500"
                  style={{
                    color: MUTED,
                    gridColumn: `${column(BEFORE.length)} / span ${AFTER.length}`,
                    opacity: shown(AFTER.length - 1) ? 1 : 0,
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
                <BamlBar full={full} key={style} p={p} sweep={sweep} />
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
              <div aria-hidden className={`${GRID} h-(--tracker-ret)`}>
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
                      // A fully clipped border still left a sliver of its right edge under the bar.
                      opacity: back ? 1 : 0,
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
        </div>
      </Frame>
    </div>
  );
}
