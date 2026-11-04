'use client';

import Image from 'next/image';
import { Card, Chip, Frame, Lamb, useCycle } from './kit';

// The stack, the way infrastructure companies draw theirs: one wide card per layer, from the agent at the
// top down to Boundary. A call walks down the spine and its trace walks back up, lighting each layer.
const LAYERS: {
  chips: { icon?: string; mono?: boolean; name: string }[];
  lamb?: boolean;
  name: string;
  note: string;
}[] = [
  {
    chips: [
      { name: 'triage' },
      { name: 'support-bot' },
      { name: 'nightly evals' },
    ],
    name: 'Your agent',
    note: 'Calls BAML functions like any other function.',
  },
  {
    chips: [
      { name: 'Types' },
      { name: 'Functions' },
      { name: 'Prompts' },
      { name: 'Tests' },
    ],
    lamb: true,
    name: 'BAML language',
    note: 'Prompts as typed functions, with their tests beside them.',
  },
  {
    chips: [
      { mono: true, name: 'baml check' },
      { mono: true, name: 'baml test' },
      { mono: true, name: 'baml pack' },
      { mono: true, name: 'baml query' },
    ],
    name: 'Toolchain',
    note: 'Checks, tests, ships, and queries what you wrote.',
  },
  {
    chips: [
      { icon: '/langs/python.png', name: 'Python' },
      { icon: '/langs/typescript.svg', name: 'TypeScript' },
      { icon: '/langs/go.svg', name: 'Go' },
      { icon: '/langs/rust-orange.svg', name: 'Rust' },
    ],
    name: 'Runtime',
    note: 'One runtime under every language you call it from.',
  },
  {
    chips: [{ name: 'Traces' }, { name: 'Dashboards' }, { name: 'Alerts' }],
    name: 'Boundary',
    note: 'Every call recorded, so you can see and query production.',
  },
];

// Down the stack and back up: 0, 1, 2, 3, 4, 3, 2, 1.
const PATH = [...LAYERS.keys(), ...[...LAYERS.keys()].slice(1, -1).reverse()];

// How much of a layer's spine segment is filled. The first segment starts at its layer's node and the last
// ends at its node, so the layer the call is on fills the top half of its segment, or none or all of it.
function fill(i: number, active: number) {
  if (i < active) return 'h-full';
  if (i > active || i === 0) return 'h-0';
  return i === LAYERS.length - 1 ? 'h-full' : 'h-1/2';
}

export default function Layers() {
  const step = useCycle(PATH.length, 900, 1);
  const active = PATH[step];
  const down = step < LAYERS.length;

  return (
    <Frame label="BAML's stack as five layers: your agent, the BAML language, the toolchain, the runtime for Python, TypeScript, Go, and Rust, and Boundary. A call passes down through each layer and its trace passes back up.">
      <div className="p-3 @min-[640px]:p-8 @min-[768px]:p-12">
        <p className="flex items-center gap-2 mb-4 pl-10 @min-[640px]:pl-12 text-[12.5px] text-(--ds-ink)/45">
          <span className="size-1.5 rounded-full bg-(--ds-accent)" />
          {down ? 'Call going down' : 'Trace coming back'}
        </p>
        <ol className="flex flex-col">
          {LAYERS.map((l, i) => (
            <li
              className="relative py-1.5 pl-10 @min-[640px]:pl-12"
              key={l.name}
            >
              {/* The spine, one segment per layer, filled up to the layer the call has reached. */}
              <span
                className={`absolute left-[15px] w-px ${i === 0 ? 'top-1/2' : 'top-0'} ${i === LAYERS.length - 1 ? 'bottom-1/2' : 'bottom-0'} bg-(--ds-ink)/10`}
              >
                <span
                  className={`block w-full bg-(--ds-accent) transition-[height] duration-500 ${fill(i, active)}`}
                />
              </span>
              <span
                className={`absolute left-[11px] top-1/2 -translate-y-1/2 size-[9px] rounded-full border transition-colors duration-300 ${i === active ? 'border-(--ds-accent) bg-(--ds-accent)' : i < active ? 'border-(--ds-accent) bg-(--ds-ground)' : 'border-(--ds-ink)/25 bg-(--ds-ground)'}`}
              >
                {i === active && (
                  <span className="ds-ping absolute inset-0 rounded-full bg-(--ds-accent)" />
                )}
              </span>

              <Card
                className="flex flex-col @min-[768px]:flex-row @min-[768px]:items-center gap-3 @min-[768px]:gap-6 p-4 @min-[768px]:px-5"
                lit={i === active}
              >
                <div className="flex items-center gap-3 @min-[768px]:w-[300px] shrink-0">
                  {l.lamb && (
                    <span className="grid place-items-center size-8 rounded-lg border border-(--ds-accent)/40 bg-(--ds-accent)/[0.06] shrink-0">
                      <Lamb size={18} />
                    </span>
                  )}
                  <div>
                    <p className="text-[15px] text-(--ds-ink)/90">{l.name}</p>
                    <p className="mt-0.5 text-[13px] leading-snug text-(--ds-ink)/45">
                      {l.note}
                    </p>
                  </div>
                </div>
                <div className="flex flex-wrap gap-1.5 @min-[768px]:ml-auto @min-[768px]:justify-end">
                  {l.chips.map((c) => (
                    <Chip className={c.mono ? 'font-mono' : ''} key={c.name}>
                      {c.icon && (
                        <Image
                          alt=""
                          className="size-3.5 object-contain"
                          height={14}
                          src={c.icon}
                          width={14}
                        />
                      )}
                      {c.name}
                    </Chip>
                  ))}
                </div>
              </Card>
            </li>
          ))}
        </ol>
      </div>
    </Frame>
  );
}
