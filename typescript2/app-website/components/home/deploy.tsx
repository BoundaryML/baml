import boundaryMark from '@boundaryml/brand/logo/svg/boundary-logomark-white.svg';
import Image from 'next/image';
import type { ReactNode } from 'react';
import { MarkTile } from './mark';
import { MARKS } from './marks';
import { Phase } from './phase';
import { RED } from './story';

// Section 5, Deploy: where program data goes. A diagram shows the app sending it to Boundary Cloud,
// which keeps it in a database the terminal reads with SQL. Three cards follow, one for each kind of
// thing the runtime records.

/** One node of the diagram: a caption or a product name over its contents. A node with a logo is a
 *  Boundary product, so its name is set lowercase in Geist SemiBold, like the Boundary wordmark,
 *  instead of in Geist Pixel, which the brand keeps for captions and BAML. */
function Node({
  children,
  footer,
  lit,
  mark,
  name,
}: {
  children: ReactNode;
  footer?: ReactNode;
  lit?: boolean;
  mark?: ReactNode;
  name: string;
}) {
  return (
    <div
      className={`flex flex-col rounded-xl border bg-raised ${lit ? 'border-clover/50' : 'border-rule'}`}
    >
      <p
        className={`flex gap-2.5 px-4 pt-4 pb-3 text-head ${mark ? 'items-baseline font-sans font-semibold text-[20px] leading-none tracking-[-0.01em]' : 'items-center v4-caption'}`}
      >
        {mark}
        {name}
      </p>
      <div className="flex-1 flex flex-col px-4 pb-4">{children}</div>
      {footer && (
        <p className="px-4 py-3 border-t border-rule text-[14px] leading-[1.45] text-body text-pretty">
          {footer}
        </p>
      )}
    </div>
  );
}

/** The link between two nodes, pointing the way the data moves. On wide screens the label sits over a
 *  line across the gap; on narrow ones it sits between two lines running down. */
function Link({ label }: { label: string }) {
  return (
    <div
      aria-hidden
      className="flex flex-col items-center justify-center gap-1.5 py-2 md:py-0 md:px-3"
    >
      <span className="md:hidden h-4 w-px bg-clover/70" />
      <span className="v4-caption whitespace-nowrap text-clover">{label}</span>
      <span className="relative h-5 w-px md:h-px md:w-full bg-clover/70">
        <span className="absolute -bottom-px left-1/2 -translate-x-1/2 md:bottom-auto md:left-auto md:-right-px md:top-1/2 md:-translate-y-1/2 md:translate-x-0 size-0 border-x-[4px] border-t-[6px] border-x-transparent border-t-clover md:border-y-[4px] md:border-l-[6px] md:border-r-0 md:border-y-transparent md:border-l-clover" />
      </span>
    </div>
  );
}

/** A database, drawn as a cylinder in the brand's accent. */
function Cylinder() {
  return (
    <svg
      aria-hidden="true"
      className="shrink-0 text-clover"
      fill="none"
      height={30}
      stroke="currentColor"
      strokeWidth={1.25}
      viewBox="0 0 26 30"
      width={26}
    >
      <ellipse cx={13} cy={5} rx={11} ry={4} />
      <path d="M2 5v20c0 2.2 4.9 4 11 4s11-1.8 11-4V5" />
      <path d="M2 15c0 2.2 4.9 4 11 4s11-1.8 11-4" />
    </svg>
  );
}

const ROWS = [
  ['classify', '812ms', 'billing'],
  ['route', '4ms', 'urgent'],
  ['classify', '790ms', 'bug'],
];

function Architecture() {
  return (
    <div className="grid md:grid-cols-[minmax(0,1fr)_8rem_minmax(0,1.3fr)_8rem_minmax(0,1fr)] items-stretch">
      <Node name="Your app">
        <div className="flex-1 flex flex-col gap-2 font-mono text-[12.5px]">
          <div className="flex-1 rounded-md border border-rule px-3 py-2">
            <p className="text-ink-2">triage.baml</p>
            {['classify', 'route', 'load_ticket'].map((fn) => (
              <p className="text-muted" key={fn}>
                <span className="text-clover">function</span> {fn}
              </p>
            ))}
          </div>
          <p className="rounded-md border border-clover/40 bg-clover/10 px-3 py-2 text-clover">
            BAML runtime
          </p>
        </div>
      </Node>

      <Link label="program data" />

      <Node
        footer="Hosted by us, in your own cloud, or on your machine"
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
      >
        <div className="flex items-center gap-3">
          <Cylinder />
          <p className="font-mono text-[12.5px] text-ink-2">
            spans
            <span className="block text-muted">one typed row per call</span>
          </p>
        </div>
        <table className="mt-3 w-full font-mono text-[12px] tabular-nums">
          <thead className="text-muted">
            <tr className="border-b border-rule">
              <th className="py-1.5 pr-2 text-left font-normal">span_name</th>
              <th className="py-1.5 pr-2 text-right font-normal">duration</th>
              <th className="py-1.5 text-right font-normal">output</th>
            </tr>
          </thead>
          <tbody className="text-ink-2">
            {ROWS.map(([name, duration, output]) => (
              <tr
                className="border-b border-rule/60"
                key={`${name}-${duration}`}
              >
                <td className="py-1.5 pr-2">{name}</td>
                <td className="py-1.5 pr-2 text-right">{duration}</td>
                <td className="py-1.5 text-right">{output}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </Node>

      <Link label="SQL" />

      <Node name="Your terminal">
        <div className="font-mono text-[12.5px] leading-[1.7]">
          <p className="text-ink">
            <span className="text-muted">$ </span>baml query
          </p>
          <p className="text-ink-2">SELECT span_name, p95(duration)</p>
          <p className="text-ink-2">FROM spans GROUP BY span_name</p>
          <p className="mt-2 flex justify-between text-clover">
            <span>classify</span>
            <span>1.2s</span>
          </p>
          <p className="flex justify-between text-clover">
            <span>route</span>
            <span>6ms</span>
          </p>
        </div>
      </Node>
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

// Nested timings for one call, as [name, depth, start, width] in shares of the whole.
const PROFILE: [string, number, number, number][] = [
  ['route', 0, 0, 1],
  ['load_ticket', 1, 0, 0.28],
  ['classify', 1, 0.3, 0.7],
  ['parse', 2, 0.62, 0.38],
];

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
          <div className="relative h-[76px]">
            {PROFILE.map(([name, depth, start, width]) => (
              <span
                className="absolute h-[22px] flex items-center px-2 rounded-[4px] bg-clover/20 text-[11.5px] text-ink-2 overflow-hidden whitespace-nowrap"
                key={name}
                style={{
                  left: `${start * 100}%`,
                  top: depth * 26,
                  width: `calc(${width * 100}% - 2px)`,
                }}
              >
                {name}
              </span>
            ))}
          </div>
          <p className="mt-3 flex justify-between text-muted">
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
