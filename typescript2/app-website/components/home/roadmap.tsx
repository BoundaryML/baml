import { Phase } from './phase';

// What's coming, as five rows: the feature, where it stands, and one line on what it does. Furthest
// along first. The figure for each one is in ./improve.tsx, which the homepage no longer mounts:
// unshipped work shouldn't take more room than the call to action that follows it.
const ROADMAP: {
  link?: { href: string; label: string };
  name: string;
  says: string;
  // Filled marks something that runs today; hollow, something still on paper.
  started: boolean;
  status: string;
}[] = [
  {
    link: {
      href: 'https://github.com/BoundaryML/baml/pull/5218',
      label: 'PR #5218',
    },
    name: 'Compiled to native',
    says: 'Interpret for instant builds. Compile for instant execution.',
    started: true,
    status: 'In progress',
  },
  {
    link: {
      href: 'https://github.com/BoundaryML/baml/pull/4956',
      label: 'PR #4956',
    },
    name: 'Durable execution',
    says: 'Pause a run on one machine and pick it up on another.',
    started: true,
    status: 'Prototype',
  },
  {
    link: { href: 'https://beps.boundaryml.com/beps/63', label: 'BEP-063' },
    name: 'Sandboxes',
    says: 'Milliseconds are too slow, megabytes too big.',
    started: false,
    status: 'Proposed',
  },
  {
    name: 'Feature flags',
    says: 'Roll out experiments 100x faster than CI/CD.',
    started: false,
    status: 'Designing',
  },
  {
    name: 'Formal verification',
    says: 'Prove a rule holds for every input, not just the ones you tested.',
    started: false,
    status: 'Researching',
  },
];

/** The Improve phase: everything coming next, one row each. */
export default function Roadmap() {
  return (
    <Phase heading="More things to look forward to." id="improve" soon>
      <ul className="border-b border-rule">
        {ROADMAP.map((r) => (
          <li
            className="grid items-baseline gap-x-8 gap-y-1.5 border-t border-rule py-5 md:grid-cols-[12rem_minmax(0,15rem)_minmax(0,1fr)_6rem]"
            key={r.name}
          >
            <p className="flex items-center gap-2 font-mono text-[11px] uppercase tracking-wider text-ink-2">
              <span
                aria-hidden
                className={`size-1.5 rounded-full ${r.started ? 'bg-clover' : 'border border-white/40'}`}
              />
              {r.status}
            </p>
            <h3 className="font-display text-[20px] font-medium leading-snug text-head">
              {r.name}
            </h3>
            <p className="text-[16px] leading-normal text-ink-2">{r.says}</p>
            {r.link ? (
              <a
                className="focus-ring font-mono text-[13px] text-clover hover:underline md:text-right"
                href={r.link.href}
                rel="noopener"
                target="_blank"
              >
                {r.link.label} <span aria-hidden>↗</span>
              </a>
            ) : (
              <span aria-hidden className="max-md:hidden" />
            )}
          </li>
        ))}
      </ul>
    </Phase>
  );
}
