import { section } from './analytics';
import { Arrow, Cmd, LANES, LEFT, LINKS, PAD_Y, WRAP } from './ui';

const LINKS_OUT = [
  {
    href: LINKS.github,
    label: 'Source code',
    url: 'github.com/boundaryml/baml',
  },
  {
    href: LINKS.demos,
    label: 'Demo projects',
    url: 'github.com/boundaryml/baml-demos',
  },
  {
    href: LINKS.discord,
    label: 'Community Discord',
    url: 'discord.gg/yzaTpQ3tdT',
  },
];

// boundaryml.com's "Project status", stated plainly.
export default function Status() {
  return (
    <section
      {...section('status')}
      aria-labelledby="status-title"
      className={`${WRAP} ${PAD_Y} ${LANES}`}
    >
      <div className={`${LEFT} flex flex-col gap-4`}>
        <h2
          className="font-display font-medium text-[clamp(28px,2.4vw,34px)] leading-10 tracking-[-0.03em]"
          id="status-title"
        >
          {/* A phone breaks this after the hyphen, leaving a line that reads "…and pre-". */}
          Open source, and <span className="whitespace-nowrap">pre-1.0.</span>
        </h2>
        <p className="text-base leading-[26px] text-body">
          BAML is usable today, but some APIs and tooling may still change. The
          agent skill, the local CLI and <Cmd>baml describe</Cmd> always match
          the version you have installed.
        </p>
      </div>
      <ul className="flex-1 min-w-0 border-t border-rule-accent">
        {LINKS_OUT.map((l) => (
          <li className="border-b border-rule" key={l.label}>
            <a
              className="group flex justify-between items-baseline gap-4 py-[22px]"
              href={l.href}
            >
              <span className="font-display font-medium text-[22px] leading-7 tracking-[-0.03em] group-hover:text-accent transition-colors">
                {l.label}
              </span>
              <span className="max-sm:hidden font-mono text-[13px] text-muted truncate group-hover:text-body transition-colors">
                {l.url} <Arrow />
              </span>
              <span className="sm:hidden text-muted">
                <Arrow />
              </span>
            </a>
          </li>
        ))}
      </ul>
    </section>
  );
}
