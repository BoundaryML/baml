import boundaryMark from '@boundaryml/brand/logo/svg/boundary-logomark-white.svg';
import Image from 'next/image';
import type { ReactNode } from 'react';
import { MarkTile } from './mark';
import { MARKS } from './marks';
import { Phase } from './phase';
import { RED } from './story';

// Section 5, Deploy: where program data goes. A simple diagram of three icons shows the app sending it
// to Boundary Cloud, which keeps it in a data lake the terminal reads with SQL. Three cards follow,
// one for each kind of thing the runtime records.

/** One node of the diagram: an icon in a tile, with a name and a short line under it. A node with a
 *  logo is a Boundary product, so its name is set lowercase in Geist SemiBold, like the Boundary
 *  wordmark, instead of in Geist Pixel, which the brand keeps for captions and BAML. */
function Node({
  detail,
  icon,
  lit,
  mark,
  name,
}: {
  detail: string;
  icon: ReactNode;
  lit?: boolean;
  mark?: ReactNode;
  name: string;
}) {
  return (
    <div className="flex flex-col items-center text-center">
      <div
        className={`grid place-items-center size-24 rounded-2xl border ${lit ? 'border-clover/50 bg-clover/10 text-clover' : 'border-rule bg-raised text-ink-2'}`}
      >
        {icon}
      </div>
      <p
        className={`mt-4 flex items-baseline gap-2 text-head ${mark ? 'font-sans font-semibold text-[20px] leading-none tracking-[-0.01em]' : 'v4-caption'}`}
      >
        {mark}
        {name}
      </p>
      <p className="mt-2 max-w-[16rem] text-[14px] leading-[1.45] text-muted text-pretty">
        {detail}
      </p>
    </div>
  );
}

/** The link between two nodes, pointing the way the data moves. On wide screens it runs across the gap
 *  at the height of the icons; on narrow ones it runs down between the nodes. */
function Link({ label }: { label: string }) {
  return (
    <div
      aria-hidden
      className="flex flex-col items-center justify-center gap-1.5 py-4 md:py-0 md:h-24 md:px-2"
    >
      <span className="md:hidden h-4 w-px bg-clover/70" />
      <span className="v4-caption whitespace-nowrap text-clover">{label}</span>
      <span className="relative h-5 w-px md:h-px md:w-full bg-clover/70">
        <span className="absolute -bottom-px left-1/2 -translate-x-1/2 md:bottom-auto md:left-auto md:-right-px md:top-1/2 md:-translate-y-1/2 md:translate-x-0 size-0 border-x-[4px] border-t-[6px] border-x-transparent border-t-clover md:border-y-[4px] md:border-l-[6px] md:border-r-0 md:border-y-transparent md:border-l-clover" />
      </span>
    </div>
  );
}

/** Line icons for the three nodes, drawn on a 48-unit grid in the current color. */
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

/** A terminal with a prompt. */
const TERMINAL = (
  <Icon>
    <rect height={32} rx={4} width={40} x={4} y={8} />
    <path d="M13 19l6 5-6 5M23 30h12" />
  </Icon>
);

function Architecture() {
  return (
    <div className="mx-auto max-w-[60rem] grid md:grid-cols-[minmax(0,1fr)_minmax(6rem,9rem)_minmax(0,1fr)_minmax(6rem,9rem)_minmax(0,1fr)] items-start">
      <Node
        detail="Your BAML code and the runtime that records it"
        icon={APP}
        name="Your app"
      />
      <Link label="program data" />
      <Node
        detail="A data lake of types, spans, and blobs, hosted by us, in your cloud, or on your machine"
        icon={LAKE}
        lit
        mark={
          <Image
            alt=""
            className="h-[14px] w-auto"
            height={14}
            src={boundaryMark}
            width={22}
          />
        }
        name="boundary cloud"
      />
      <Link label="SQL" />
      <Node
        detail="Queries with baml query, from you or your agent"
        icon={TERMINAL}
        name="Your terminal"
      />
    </div>
  );
}

/** A card: a small figure on top, then a title and a sentence or two. */
function Card({
  body,
  children,
  title,
}: {
  body: ReactNode;
  children: ReactNode;
  title: string;
}) {
  return (
    <div className="flex flex-col rounded-xl border border-rule bg-raised">
      <div className="h-40 flex flex-col justify-center px-5 border-b border-rule font-mono text-[12.5px]">
        {children}
      </div>
      <div className="p-5">
        <h3 className="v4-body text-head">{title}</h3>
        <p className="mt-1 v4-body text-body">{body}</p>
      </div>
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
            className="absolute h-[13px] flex items-center px-1 rounded-[2px] text-[10px] leading-none text-ink overflow-hidden whitespace-nowrap"
            key={name}
            style={{
              background: `rgb(from var(--color-clover) r g b / ${DEPTH_ALPHA[depth]})`,
              left: `${start * 100}%`,
              top: depth * row,
              width: `calc(${width * 100}% - 1px)`,
            }}
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
      <Architecture />

      <div className="mt-12 md:mt-16 grid md:grid-cols-3 gap-4">
        <Card
          body="Every call to an AI function records the provider, the model, the tokens it used, and what it cost, next to its arguments and the parsed result."
          title="Trace AI calls automatically."
        >
          <div className="grid grid-cols-[auto_minmax(0,1fr)_auto] gap-x-3 gap-y-2.5 items-center">
            {CALLS.map((c) => (
              <div className="contents" key={c.model}>
                <span className="text-ink">
                  <MarkTile mark={c.mark} size={18} />
                </span>
                <span className="min-w-0">
                  <span className="block truncate text-ink-2">{c.model}</span>
                  <span className="block text-[11.5px] text-muted">
                    {c.tokens} tokens
                  </span>
                </span>
                <span className="tabular-nums text-clover">{c.cost}</span>
              </div>
            ))}
          </div>
        </Card>

        <Card
          body="Ordinary functions record how long each call took. It costs so little that Boundary Cloud doesn't charge for it."
          title="Profile everything."
        >
          <FlameChart />
          <p className="mt-2.5 flex justify-between text-muted">
            <span>Cost to profile</span>
            <span className="text-clover">$0</span>
          </p>
        </Card>

        <Card
          body="When a function throws, the runtime records where the error started and how it moved up the stack."
          title="Get traces on errors, without changing code."
        >
          <div className="leading-[1.8]">
            <p className="flex items-center gap-2" style={{ color: RED }}>
              <span className="size-1.5 rounded-full bg-current" />
              ParseError in parse
            </p>
            {STACK.map(([fn, where]) => (
              <p className="flex justify-between gap-3 pl-3.5" key={fn}>
                <span className="text-ink-2">at {fn}</span>
                <span className="truncate text-muted">{where}</span>
              </p>
            ))}
          </div>
        </Card>
      </div>
    </Phase>
  );
}
