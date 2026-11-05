import boundaryMark from '@boundaryml/brand/logo/svg/boundary-logomark-white.svg';
import Image from 'next/image';
import type { CSSProperties, ReactNode } from 'react';
import { MarkTile } from './mark';
import { MARKS } from './marks';
import { Phase } from './phase';
import { RED } from './story';

// Section 5, Deploy: where program data goes, as one figure. The app fans out into three streams of
// program data (AI calls, profiles, and errors), each a small card with its own figure. The streams
// fan back in to Boundary Cloud's data lake, and the terminal reads the lake with SQL. On wide screens
// it runs left to right; below that it stacks top to bottom.

/** An end of the figure: an icon in a tile, with a name and an optional line under it. A node with a
 *  logo is a Boundary product, so its name is set lowercase in Geist SemiBold, like the Boundary
 *  wordmark, instead of in Geist Pixel, which the brand keeps for captions and BAML. */
function Node({
  detail,
  icon,
  lit,
  mark,
  name,
}: {
  detail?: string;
  icon: ReactNode;
  lit?: boolean;
  mark?: ReactNode;
  name: string;
}) {
  return (
    <div className="flex flex-col items-center text-center">
      <div
        className={`node-tile grid place-items-center size-24 rounded-2xl border ${lit ? 'border-clover/50 bg-clover/10 text-clover' : 'border-rule bg-raised text-ink-2'}`}
      >
        {icon}
      </div>
      <p
        className={`mt-3 flex items-baseline gap-2 whitespace-nowrap text-head ${mark ? 'font-sans font-semibold text-[17px] leading-none tracking-[-0.01em]' : 'v4-caption'}`}
      >
        {mark}
        {name}
      </p>
      {detail && (
        <p className="mt-2 max-w-[16rem] text-[13px] leading-[1.45] text-muted text-pretty">
          {detail}
        </p>
      )}
    </div>
  );
}

/** A labelled link between two parts of the figure. It runs down on narrow screens; with `across`, it
 *  runs left to right on wide ones. */
function Link({ across, label }: { across?: boolean; label?: string }) {
  return (
    <div
      aria-hidden
      className={`flex flex-col items-center justify-center gap-1.5 py-3 ${across ? 'xl:py-0 xl:px-0' : 'xl:hidden'}`}
    >
      <span className={`h-3 w-px bg-clover/70 ${across ? 'xl:hidden' : ''}`} />
      {label && (
        <span className="v4-caption whitespace-nowrap text-clover">
          {label}
        </span>
      )}
      <span
        className={`relative h-5 w-px bg-clover/70 ${across ? 'xl:h-px xl:w-full' : ''}`}
      >
        <span
          className={`absolute -bottom-px left-1/2 -translate-x-1/2 size-0 border-x-[4px] border-t-[6px] border-x-transparent border-t-clover ${across ? 'xl:bottom-auto xl:left-auto xl:-right-px xl:top-1/2 xl:-translate-y-1/2 xl:translate-x-0 xl:border-y-[4px] xl:border-l-[6px] xl:border-r-0 xl:border-y-transparent xl:border-l-clover' : ''}`}
        />
      </span>
    </div>
  );
}

// The gap between the streams, in pixels; the fan's vertical lines reach across it.
const ROW_GAP = 12;

/** One row of a fan on wide screens: a line across the row's middle, and the share of the vertical
 *  line that joins the three rows. `out` fans from the app to the streams, with the vertical line on the
 *  left and arrows into each stream; `in` fans from the streams to the lake, with the vertical line on
 *  the right and one arrow, from the middle row, into the lake. */
function Fan({ dir, row }: { dir: 'in' | 'out'; row: 0 | 1 | 2 }) {
  const top = row === 0 ? '50%' : 0;
  const bottom = row === 2 ? '50%' : -ROW_GAP;

  return (
    <div
      aria-hidden
      className="fan relative hidden xl:block"
      data-row={row}
      style={{ gridColumn: dir === 'out' ? 2 : 4, gridRow: row + 1 }}
    >
      <span
        className={`absolute w-px bg-clover/70 ${dir === 'out' ? 'left-0' : 'right-0'}`}
        style={{ bottom, top }}
      />
      <span className="fan-h absolute inset-x-0 top-1/2 h-px bg-clover/70">
        {dir === 'out' && <Arrowhead />}
      </span>
    </div>
  );
}

/** An arrowhead pointing right, at the right end of a horizontal line. */
function Arrowhead() {
  return (
    <span className="absolute -right-px top-1/2 -translate-y-1/2 size-0 border-y-[4px] border-l-[6px] border-y-transparent border-l-clover" />
  );
}

/** Line icons for the ends of the figure, drawn on a 48-unit grid in the current color. */
function Icon({ children }: { children: ReactNode }) {
  return (
    <svg
      aria-hidden="true"
      fill="none"
      height={44}
      stroke="currentColor"
      strokeLinecap="round"
      strokeLinejoin="round"
      strokeWidth={1.5}
      viewBox="0 0 48 48"
      width={44}
    >
      {children}
    </svg>
  );
}

/** An app window with code in it. */
const APP = (
  <Icon>
    <rect height={32} rx={4} width={40} x={4} y={8} />
    <path d="M4 16h40" />
    <path d="M17 23l-5 5 5 5M31 23l5 5-5 5M26.5 21l-5 14" />
  </Icon>
);

/** A database. */
const LAKE = (
  <Icon>
    <ellipse cx={24} cy={11} rx={15} ry={5} />
    <path d="M9 11v26c0 2.8 6.7 5 15 5s15-2.2 15-5V11" />
    <path d="M9 24c0 2.8 6.7 5 15 5s15-2.2 15-5" />
  </Icon>
);

/** One stream of program data: a name and a line about it beside a small figure of what it records. */
function Stream({
  body,
  children,
  row,
  title,
}: {
  body: ReactNode;
  children: ReactNode;
  row: number;
  title: string;
}) {
  return (
    <div
      className="stream grid sm:grid-cols-[minmax(0,12rem)_minmax(0,1fr)] gap-x-6 gap-y-4 items-center rounded-xl border border-rule bg-raised p-4 md:p-5"
      data-row={row}
      style={{ gridColumn: 3, gridRow: row + 1 }}
    >
      <div>
        <h3 className="text-[16px] leading-[1.35] text-head">{title}</h3>
        <p className="mt-1 text-[13.5px] leading-[1.45] text-body text-pretty">
          {body}
        </p>
      </div>
      <div className="min-w-0 font-mono text-[12px]">{children}</div>
    </div>
  );
}

const CALLS = [
  { cost: '$0.0042', mark: MARKS.openai, model: 'gpt-6-luna', tokens: '1,204' },
  {
    cost: '$0.0061',
    mark: MARKS.anthropic,
    model: 'claude-sonnet-5-5',
    tokens: '1,388',
  },
];

// One call to route, profiled: [name, depth, start, width], with start and width in shares of the
// whole call, which took 1.1 seconds. Most of it is the model call inside classify.
const PROFILE: [string, number, number, number][] = [
  ['route', 0, 0, 1],
  ['load_ticket', 1, 0, 0.2],
  ['classify', 1, 0.21, 0.6],
  ['notify', 1, 0.82, 0.18],
  ['db_get', 2, 0.005, 0.12],
  ['parse_json', 2, 0.13, 0.065],
  ['render_prompt', 2, 0.22, 0.07],
  ['gpt-6-luna', 2, 0.3, 0.42],
  ['parse', 2, 0.73, 0.075],
  ['send_email', 2, 0.83, 0.16],
  ['query', 3, 0.01, 0.1],
  ['http', 3, 0.305, 0.41],
  ['coerce', 3, 0.735, 0.05],
  ['smtp', 3, 0.84, 0.12],
  ['stream', 4, 0.36, 0.35],
];
const PROFILE_MS = 1100;
const TICKS = [0, 250, 500, 750, 1000];
// Deeper frames are lighter, so the outline of the call reads first.
const DEPTH_ALPHA = [0.5, 0.38, 0.3, 0.24, 0.18];

/** Each frame as a sentence, for screen readers: when it started, how long it took, and which frame
 *  called it. */
const PROFILE_TEXT = PROFILE.map(([name, depth, start, width], i) => {
  const at = Math.round(start * PROFILE_MS);
  const ms = Math.round(width * PROFILE_MS);
  const parent = PROFILE.slice(0, i).findLast(
    ([, d, s, w]) => d === depth - 1 && s <= start && start < s + w,
  );
  const timing = `${name}, ${ms}ms, starting at ${at}ms`;
  return parent ? `${timing}, called by ${parent[0]}` : timing;
});

/** A flame chart of one call, drawn for sighted readers with a list of its frames for screen readers. */
function FlameChart() {
  return (
    <div>
      <ul className="sr-only">
        {PROFILE_TEXT.map((line) => (
          <li key={line}>{line}</li>
        ))}
      </ul>
      <FlameBars />
    </div>
  );
}

/** The drawn chart: a time ruler, then each frame as a bar under the frame that called it. */
function FlameBars() {
  const row = 15;
  return (
    <div aria-hidden>
      <div className="relative h-3.5 text-[10px] text-muted">
        {TICKS.map((ms) => (
          <span
            className={`absolute top-0 ${ms ? '-translate-x-1/2' : ''}`}
            key={ms}
            style={{ left: `${(ms / PROFILE_MS) * 100}%` }}
          >
            {ms ? (ms < 1000 ? `${ms}ms` : `${ms / 1000}s`) : '0'}
          </span>
        ))}
      </div>
      <div
        className="relative mt-1.5"
        style={{ height: row * (DEPTH_ALPHA.length - 1) + 13 }}
      >
        {TICKS.slice(1).map((ms) => (
          <span
            className="absolute inset-y-0 w-px bg-rule/70"
            key={ms}
            style={{ left: `${(ms / PROFILE_MS) * 100}%` }}
          />
        ))}
        {PROFILE.map(([name, depth, start, width]) => (
          <span
            className="flame-bar absolute h-[13px] px-1 rounded-[2px] text-[10px] leading-[13px] text-ink truncate"
            key={name}
            style={
              {
                '--depth': depth,
                background: `rgb(from var(--color-clover) r g b / ${DEPTH_ALPHA[depth]})`,
                left: `${start * 100}%`,
                top: depth * row,
                width: `calc(${width * 100}% - 1px)`,
              } as CSSProperties
            }
          >
            {width >= 0.12 ? name : ''}
          </span>
        ))}
      </div>
    </div>
  );
}

const STACK = [
  ['classify', 'triage.baml:14'],
  ['route', 'triage.baml:31'],
];

export default function Deploy() {
  return (
    <Phase
      heading="Turn your program data into a queryable data lake."
      id="deploy"
      intro="The BAML runtime records your program as it runs and sends it to Boundary Cloud. There's no SDK to install and no collector to run."
      name="Deploy"
    >
      <div className="deploy-fig flex flex-col xl:grid xl:grid-cols-[7rem_2rem_minmax(0,1fr)_2rem_10.5rem_5rem_14rem] xl:grid-rows-[repeat(3,minmax(0,1fr))] xl:gap-y-3">
        <div
          className="xl:relative"
          style={{ gridColumn: 1, gridRow: '1 / -1' }}
        >
          <div className="xl:absolute xl:right-0 xl:top-[calc(50%-3rem)]">
            <Node icon={APP} name="Your app" />
          </div>
        </div>
        <Link label="program data" />
        <Fan dir="out" row={0} />
        <Fan dir="out" row={1} />
        <Fan dir="out" row={2} />

        <div className="flex flex-col gap-3 xl:contents">
          <Stream
            body="The provider, model, tokens, and cost of every AI call, next to its arguments and result."
            row={0}
            title="AI calls"
          >
            <div className="grid grid-cols-[auto_minmax(0,1fr)_auto] gap-x-3 gap-y-2.5 items-center">
              {CALLS.map((c, i) => (
                <div
                  className="contents"
                  key={c.model}
                  style={{ '--i': i } as CSSProperties}
                >
                  <span className="replay text-ink">
                    <MarkTile mark={c.mark} size={18} />
                  </span>
                  <span className="replay min-w-0">
                    <span className="block truncate text-ink-2">{c.model}</span>
                    <span className="block text-[11px] text-muted">
                      {c.tokens} tokens
                    </span>
                  </span>
                  <span className="replay tabular-nums text-clover">
                    {c.cost}
                  </span>
                </div>
              ))}
            </div>
          </Stream>
          <Stream
            body="How long every function took. It costs so little that Boundary Cloud doesn't charge for it."
            row={1}
            title="Profiles"
          >
            <FlameChart />
          </Stream>
          <Stream
            body="Where each error started and how it moved up the stack, without changing code."
            row={2}
            title="Errors"
          >
            <div className="leading-[1.8]">
              <p
                className="replay flex items-center gap-2"
                style={{ '--i': 0, color: RED } as CSSProperties}
              >
                <span className="error-dot size-1.5 rounded-full bg-current" />
                ParseError in parse
              </p>
              {STACK.map(([fn, where], i) => (
                <p
                  className="replay flex justify-between gap-3 pl-3.5"
                  key={fn}
                  style={{ '--i': i + 1 } as CSSProperties}
                >
                  <span className="text-ink-2">at {fn}</span>
                  <span className="truncate text-muted">{where}</span>
                </p>
              ))}
            </div>
          </Stream>
        </div>

        <Link />
        <Fan dir="in" row={0} />
        <Fan dir="in" row={1} />
        <Fan dir="in" row={2} />
        <div
          className="xl:relative"
          style={{ gridColumn: 5, gridRow: '1 / -1' }}
        >
          {/* The tile is centered in the column, so these reach from the column's edges to its sides. */}
          <span
            aria-hidden
            className="hidden xl:block absolute left-0 top-1/2 h-px w-[calc(50%-3rem)] bg-clover/70"
          >
            <Arrowhead />
          </span>
          <span
            aria-hidden
            className="hidden xl:block absolute right-0 top-1/2 h-px w-[calc(50%-3rem)] bg-clover/70"
          />
          <div className="lake xl:absolute xl:inset-x-0 xl:top-[calc(50%-3rem)]">
            <Node
              detail="A data lake hosted by us, in your cloud, or on your machine"
              icon={LAKE}
              lit
              mark={
                <Image
                  alt=""
                  className="h-[12px] w-auto"
                  height={12}
                  src={boundaryMark}
                  width={19}
                />
              }
              name="boundary cloud"
            />
          </div>
        </div>

        <div
          className="xl:relative"
          style={{ gridColumn: 6, gridRow: '1 / -1' }}
        >
          {/* The link's line is its last pixel, so its bottom sits 1px below the middle: the line then
              covers the same pixel row as the lines beside the lake, which start at the middle. */}
          <div className="xl:absolute xl:inset-x-0 xl:bottom-[calc(50%-1px)]">
            <Link across label="SQL" />
          </div>
        </div>

        <div
          className="self-center"
          style={{ gridColumn: 7, gridRow: '1 / -1' }}
        >
          <div className="mx-auto max-w-[20rem] rounded-xl border border-rule bg-raised font-mono text-[12px] leading-[1.7]">
            <p className="v4-caption px-4 py-2.5 border-b border-rule text-head">
              Your terminal
            </p>
            <div className="p-4">
              <p className="text-ink">
                <span className="text-muted">$ </span>baml query
              </p>
              <p className="text-ink-2">SELECT span_name,</p>
              <p className="pl-4 text-ink-2">p95(duration)</p>
              <p className="text-ink-2">FROM spans</p>
              <p className="text-ink-2">GROUP BY span_name</p>
              <p className="mt-2 flex justify-between text-clover">
                <span>classify</span>
                <span>1.2s</span>
              </p>
              <p className="flex justify-between text-clover">
                <span>route</span>
                <span>6ms</span>
              </p>
            </div>
          </div>
        </div>
      </div>
    </Phase>
  );
}
