import Image from 'next/image';
import { InView, Rise } from './motion';

const CAPABILITIES = [
  'A sound type system, like Rust',
  'Fast compilation times, like Go',
  'Low-cost telemetry, built into the runtime',
  'Built-in AI capabilities',
  'Fully integrated with Boundary Cloud (or bring your own)',
  'Cross-compatible with Python, TypeScript, Rust, and more',
];

// What each language's toolchain took on, and what BAML adds after deploy. Go's pprof is here on
// purpose: production profiling isn't new, and the difference is what's kept of each call.
const LANGS = ['C (1972)', 'Go (2009)', 'Rust (2015)', 'BAML'];
const LIFECYCLE: { stage: string; cells: [string, string, string, string] }[] =
  [
    {
      cells: ['✓', '✓', '✓', '✓, also at runtime'],
      stage: 'Compile and typecheck',
    },
    { cells: ['', 'gofmt', 'rustfmt', 'baml fmt'], stage: 'Format' },
    { cells: ['', 'go test', 'cargo test', 'baml test'], stage: 'Test' },
    {
      cells: ['', 'go build, go mod', 'cargo', 'baml generate'],
      stage: 'Build',
    },
    {
      cells: ['', 'pprof, sampled', '', 'Exact counts per call path'],
      stage: 'Profile in production',
    },
    {
      cells: ['', '', '', 'On by default'],
      stage: 'Record every call, with types',
    },
    { cells: ['', '', '', 'baml query'], stage: 'Query what ran' },
    { cells: ['', '', '', 'baml describe'], stage: 'Find code, for agents' },
  ];

const isCommand = (cell: string) =>
  /^(baml|go|cargo|gofmt|rustfmt|pprof)\b/.test(cell);

export default function Toolchain() {
  return (
    <section className="w-full max-w-[1440px] mx-auto px-5 md:px-12 pt-16 md:pt-30 pb-20 md:pb-30 border-t border-hair">
      <div className="flex flex-col lg:flex-row lg:justify-between gap-8 md:gap-12 mb-12 md:mb-16">
        <div className="lg:w-140 shrink-0 flex flex-col gap-6">
          <Rise
            className="h2-display"
            text="The all-in-one toolchain for agents."
          />
          <p className="text-[17px] leading-[27px] text-ink-2">
            Existing languages leave you hanging once you want to deploy to
            production. There is no unified toolchain to debug issues in
            production.
          </p>
          <p className="text-[17px] leading-[27px] text-ink-2">
            BAML is the only language designed for agents.
          </p>
        </div>
        <ol className="flex-1 max-w-160 border-t border-ink">
          {CAPABILITIES.map((c, i) => (
            <li
              className="flex gap-4 py-3.5 border-b border-hair font-display text-[17px] leading-6 tracking-[-0.02em] text-ink"
              key={c}
            >
              <span className="w-5 shrink-0 font-mono text-[13px] leading-6 tracking-normal text-ink-3">
                {i + 1}
              </span>
              {c}
            </li>
          ))}
        </ol>
      </div>

      <h3 className="mb-6 md:mb-8 font-display font-medium text-[clamp(24px,2.2vw,32px)] leading-[1.2] tracking-[-0.03em] text-ink">
        BAML covers the full development lifecycle.
      </h3>
      <InView
        className="relative rounded-[14px] overflow-hidden bg-pasture p-3 sm:p-6 md:p-10"
        light
      >
        <Image
          alt=""
          className="object-cover"
          fill
          quality={90}
          sizes="(min-width: 1440px) 1344px, 100vw"
          src="/boundary/painting-footer-4k.jpg"
        />
        {/* The table scrolls inside its card on a phone, so the page itself never scrolls sideways. */}
        <div className="relative overflow-x-auto rounded-[10px] bg-white text-ink shadow-[0_1px_2px_rgba(20,40,20,0.12),0_24px_48px_-16px_rgba(20,40,20,0.45)]">
          <table className="w-full min-w-[720px] text-left text-sm leading-5">
            <thead>
              <tr className="text-ink-3">
                <th className="px-4 py-3.5 font-normal" scope="col">
                  Stage
                </th>
                {LANGS.map((l) => (
                  <th
                    className={`px-4 py-3.5 font-normal ${l === 'BAML' ? 'bg-pasture-800 font-display font-medium text-white' : ''}`}
                    key={l}
                    scope="col"
                  >
                    {l}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {LIFECYCLE.map((row) => (
                <tr className="border-t border-hair" key={row.stage}>
                  <th className="px-4 py-3.5 font-normal" scope="row">
                    {row.stage}
                  </th>
                  {row.cells.map((cell, i) => (
                    <td
                      className={`px-4 py-3.5 ${i === 3 ? 'bg-pasture-800 text-white font-medium' : 'text-ink-2'} ${isCommand(cell) ? 'font-mono text-[13px]' : ''}`}
                      key={LANGS[i]}
                    >
                      {cell || <span className="sr-only">Not included</span>}
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </InView>
    </section>
  );
}
