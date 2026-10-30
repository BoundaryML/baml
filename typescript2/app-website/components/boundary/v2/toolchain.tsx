import Seen from './seen';

const CAPABILITIES = [
  'A sound type system, like Rust',
  'Fast compilation times, like Go',
  'Low-cost telemetry, built into the runtime',
  'Built-in AI capabilities',
  'Fully integrated with Boundary Cloud (or bring your own)',
  'Cross-compatible with Python, TypeScript, Rust, and more',
];

// What each language's toolchain took on, and what BAML adds after deploy. The rows run bottom to top,
// so the filled blocks climb like a staircase. Go's pprof is here on purpose: production profiling
// isn't new, and the difference is what's kept of each call.
const LANGS = ['C', 'Go', 'Rust', 'BAML'];
const STAGES: { stage: string; cells: [string, string, string, string] }[] = [
  { cells: ['', '', '', 'baml describe'], stage: 'Find code, for agents' },
  { cells: ['', '', '', 'baml query'], stage: 'Query what ran' },
  {
    cells: ['', '', '', 'On by default'],
    stage: 'Record every call, with types',
  },
  {
    cells: ['', 'pprof, sampled', '', 'Exact counts per call path'],
    stage: 'Profile in production',
  },
  { cells: ['', 'go build, go mod', 'cargo', 'baml generate'], stage: 'Build' },
  { cells: ['', 'go test', 'cargo test', 'baml test'], stage: 'Test' },
  { cells: ['', 'gofmt', 'rustfmt', 'baml fmt'], stage: 'Format' },
  {
    cells: ['✓', '✓', '✓', '✓, also at runtime'],
    stage: 'Compile and typecheck',
  },
];

const blockStyle = (row: number, col: number) =>
  ({ '--col': col, '--row': STAGES.length - 1 - row }) as React.CSSProperties;

export default function Toolchain() {
  return (
    <section className="max-w-[1440px] mx-auto px-5 md:px-12 pt-20 md:pt-36 pb-20 md:pb-36">
      <div className="grid lg:grid-cols-[1.2fr_1fr] gap-10 lg:gap-20">
        <div>
          <h2 className="v2-h2">The all-in-one toolchain for agents.</h2>
          <p className="max-w-130 mt-6 text-xl leading-[1.5] text-white/70">
            Existing languages leave you hanging once you want to deploy to
            production. There is no unified toolchain to debug issues in
            production.
          </p>
          <p className="mt-4 text-xl leading-[1.5]">
            BAML is the language for programs and their data.
          </p>
        </div>
        <ul className="self-end border-t border-white/12">
          {CAPABILITIES.map((c) => (
            <li
              className="py-3.5 border-b border-white/12 text-[17px] leading-6 tracking-[-0.01em]"
              key={c}
            >
              {c}
            </li>
          ))}
        </ul>
      </div>

      <h3 className="mt-16 md:mt-24 font-display font-medium text-[clamp(24px,2.6vw,38px)] leading-[1.15] tracking-[-0.03em]">
        BAML covers the full development lifecycle.
      </h3>
      <Seen className="relative mt-8 overflow-x-auto">
        <table className="w-full min-w-[760px] table-fixed border-separate border-spacing-1 text-left text-sm">
          <thead>
            <tr>
              <th className="w-[24%] font-normal" scope="col">
                <span className="sr-only">Stage</span>
              </th>
              {LANGS.map((l) => (
                <th
                  className={`px-3 pb-3 font-display font-medium text-lg ${l === 'BAML' ? 'text-pasture-200' : 'text-white/60'}`}
                  key={l}
                  scope="col"
                >
                  {l}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {STAGES.map((row, r) => (
              <tr key={row.stage}>
                <th
                  className="pr-3 font-normal text-white/60 text-[13px] md:text-sm"
                  scope="row"
                >
                  {row.stage}
                </th>
                {row.cells.map((cell, c) => {
                  const baml = LANGS[c] === 'BAML';
                  const command = /^(baml|go|cargo|gofmt|rustfmt|pprof)\b/.test(
                    cell,
                  );
                  return (
                    <td
                      className={
                        cell
                          ? `v2-block h-12 px-3 rounded-[8px] ${baml ? 'bg-pasture-800 border border-pasture-300/60 font-medium' : 'bg-white/8 text-white/75'} ${command ? 'font-mono text-[12.5px]' : ''}`
                          : ''
                      }
                      key={LANGS[c]}
                      style={cell ? blockStyle(r, c) : undefined}
                    >
                      {cell || <span className="sr-only">Not included</span>}
                    </td>
                  );
                })}
              </tr>
            ))}
          </tbody>
        </table>
      </Seen>
    </section>
  );
}
