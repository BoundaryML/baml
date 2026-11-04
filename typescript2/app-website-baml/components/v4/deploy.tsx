import boundaryMark from '@boundaryml/brand/logo/svg/boundary-logomark-white.svg';
import Image from 'next/image';
import type { ReactNode } from 'react';
import { Code } from '../v3/code';
import { Cmd, Panel, Phase, Points } from './phase';

// Section 5, Deploy: what the runtime records on its own, where it goes, and how to ask for more.

const POINTS = [
  {
    body: "Every call to an AI function records its arguments, the model's reply, the parsed result, and any error.",
    title: 'AI functions are traced automatically.',
  },
  {
    body: "Ordinary functions record how long each call took. It costs so little that Boundary Cloud doesn't charge for it.",
    title: 'Every other function is timed.',
  },
  {
    body: 'When a function throws, the runtime records where the error started and how it moved up the stack.',
    title: 'Errors leave a trace.',
  },
  {
    body: (
      <>
        Turn on a full trace for one function with a comment, or for one call
        with <Cmd>$trace</Cmd>.
      </>
    ),
    title: 'Ask for more where you need it.',
  },
];

const CODE = [
  '/// baml:$trace=trace.span',
  'function route(ticket: string) -> string {',
  '  ...',
  '}',
  '',
  'route(ticket, $trace = trace.span(inputs = true))',
];

/** One stop on the path a call takes: a name, and a line on what happens there. A stop with a logo is a
 *  Boundary product, so its name is set lowercase in Geist SemiBold, like the Boundary wordmark,
 *  instead of in Geist Pixel, which the brand keeps for captions and BAML. The gate sits on the
 *  baseline at the letters' ascender height, the way it sits beside the wordmark in the logo. */
function Stop({
  children,
  mark,
  name,
}: {
  children: ReactNode;
  mark?: ReactNode;
  name: string;
}) {
  return (
    <div className="rounded-xl border border-rule bg-raised px-5 py-4">
      <p
        className={`flex gap-2.5 text-head ${mark ? 'items-baseline font-sans font-semibold text-[20px] leading-none tracking-[-0.01em]' : 'items-center v4-caption'}`}
      >
        {mark}
        {name}
      </p>
      <div className="mt-2 text-[15px] leading-[1.5] text-body">{children}</div>
    </div>
  );
}

/** The link between two stops: across on wide screens, down on narrow ones. */
function Link({ label }: { label: string }) {
  return (
    <div className="flex flex-col md:flex-row items-center justify-center gap-2 py-1 md:py-0 md:px-1">
      <span className="h-5 w-px md:h-px md:w-auto md:flex-1 bg-clover/70" />
      <span className="v4-caption shrink-0 text-clover">{label}</span>
      <span className="h-5 w-px md:h-px md:w-auto md:flex-1 bg-clover/70" />
    </div>
  );
}

export default function Deploy() {
  return (
    <Phase
      heading="Ship faster with built-in tracing."
      id="deploy"
      intro="The BAML runtime records your program as it runs and sends it to Boundary Cloud. There's no SDK to install and no collector to run."
      name="Deploy"
    >
      <div className="grid md:grid-cols-[minmax(0,1fr)_7rem_minmax(0,1fr)_7rem_minmax(0,1fr)] items-stretch">
        <Stop name="Your app">triage.baml, running on the BAML runtime</Stop>
        <Link label="every call" />
        <Stop
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
          Hosted by us, in your own cloud, or on your machine
        </Stop>
        <Link label="SQL" />
        <Stop name="Your terminal">
          <span className="flex flex-wrap gap-x-3 font-mono text-[13px] text-ink-2">
            <span>baml query</span>
            <span>baml dashboard</span>
            <span>baml alerts</span>
          </span>
        </Stop>
      </div>

      <div className="mt-12 md:mt-16 grid lg:grid-cols-[minmax(0,1.1fr)_minmax(0,1fr)] gap-x-12 gap-y-10 items-start">
        {/* min-w-0 lets the long code line scroll inside the panel instead of widening the page. */}
        <div className="min-w-0">
          <Panel title="triage.baml">
            <div className="py-4 overflow-x-auto">
              <Code lines={CODE} />
            </div>
          </Panel>
          <p className="mt-5 v4-body text-body">
            Set <Cmd>BOUNDARY_API_KEY</Cmd> to send everything to Boundary
            Cloud. You can also run Boundary Cloud in your own cloud account, or
            set <Cmd>BOUNDARY_API_KEY=local</Cmd> to keep it all on your
            machine.
          </p>
        </div>
        <Points points={POINTS} />
      </div>
    </Phase>
  );
}
