import { section } from '../analytics';

// A card with an href becomes a link, so each one can point at its example once it's published.
const EXAMPLES: { body: string; href?: string; title: string }[] = [
  {
    body: 'Queries its own failed calls with baml query, turns them into test cases, rewrites its prompt, and keeps the change only if the tests pass.',
    title: 'Self-improving agent',
  },
  {
    body: 'Runs Claude Code from a BAML function with only the tools you allow, and returns a typed result.',
    title: 'Claude Code with approved tools',
  },
  {
    body: 'When CI fails, reads the log, fixes the code on a branch, runs baml test, and opens a pull request.',
    title: 'CI fixer',
  },
  {
    body: 'Spawns security, performance, and correctness reviewers on one diff, then merges their typed findings into one report.',
    title: 'Parallel code review',
  },
  {
    body: 'Starts from an alert, reads logs and metrics through MCP tools, and posts a typed root-cause report to Slack.',
    title: 'Incident responder',
  },
  {
    body: 'Reads data through a read-only SQL tool. Any write waits for a person to approve it, in ordinary code.',
    title: 'Database agent with approvals',
  },
];

export default function Examples() {
  return (
    <section
      {...section('examples')}
      aria-labelledby="examples-title"
      className="max-w-[1440px] mx-auto px-5 md:px-12 pt-20 md:pt-36 scroll-mt-16"
      id="examples"
    >
      <h2 className="v2-h2" id="examples-title">
        Build anything.
      </h2>
      <p className="max-w-160 mt-6 text-xl leading-[1.5] text-white/70">
        In BAML, an agent is an ordinary program, with loops, branches, tools,
        and model calls in one language.
      </p>
      <ul className="grid sm:grid-cols-2 lg:grid-cols-3 gap-px mt-10 md:mt-14 rounded-2xl overflow-hidden bg-white/12 border border-white/12">
        {EXAMPLES.map((e) => {
          const body = (
            <>
              <h3 className="font-display font-medium text-[21px] leading-[27px] tracking-[-0.03em]">
                {e.title}
              </h3>
              <p className="mt-2.5 text-[15px] leading-[23px] text-white/65">
                {e.body}
              </p>
            </>
          );
          return (
            <li className="bg-black min-h-44" key={e.title}>
              {e.href ? (
                <a
                  className="focus-ring block h-full p-6 md:p-7 hover:bg-white/5 transition-colors"
                  href={e.href}
                >
                  {body}
                </a>
              ) : (
                <div className="p-6 md:p-7">{body}</div>
              )}
            </li>
          );
        })}
      </ul>
    </section>
  );
}
