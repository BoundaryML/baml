import Image from 'next/image';
import type { ReactNode } from 'react';
import { createMetadata } from '@/app/_lib/metadata';
import { DiscordCard } from '@/components/community-cards';
import { section } from '@/components/home/analytics';
import { BOOK } from '@/components/home/links';
import { PAGE_WRAP, PageShell } from '@/components/home/page-shell';
import { Button, Cmd } from '@/components/home/ui';
// CONTENT PARITY: keep substantive copy and commands in sync with
// content/quickstart.md. Update both representations in the same change.
import { type Command, Commands } from './_components/commands';
import { TryBaml } from './_components/try-baml';

export const metadata = createMetadata({
  description:
    'Install BAML and its toolchain, agent skills, and editor extensions.',
  ogSubtitle: 'Mac, Windows, and Linux.',
  ogTitle: 'Install BAML in seconds',
  path: '/quickstart',
  title: 'Quickstart',
});

// The install page, in the homepage's BAML lane. It isn't linked from the nav or the footer. It
// offers one prompt to hand to a coding agent, then the same setup as numbered steps for anyone who
// would rather run the commands, then where to go once BAML is running: Boundary's cloud for
// production, Discord, and a few links.

type Step = {
  after?: { body: ReactNode; commands: Command[] };
  body: ReactNode;
  commands: Command[];
  title: string;
};

const STEPS: Step[] = [
  {
    after: {
      body: 'Check that it worked:',
      commands: [{ cmd: 'baml --version' }],
    },
    body: 'Pick the line for your machine.',
    commands: [
      { cmd: 'brew install baml', note: 'macOS or Linux, with Homebrew' },
      {
        cmd: 'curl -fsSL https://pkg.boundaryml.com/install.sh | sh -s',
        note: 'macOS or Linux, with the install script',
      },
      { cmd: 'yay -S baml-bin', note: 'Arch Linux' },
      {
        cmd: 'irm https://pkg.boundaryml.com/install.ps1 | iex',
        note: 'Windows, in PowerShell',
      },
    ],
    title: 'Install the toolchain',
  },
  {
    body: (
      <>
        Run this from the repository where you want BAML. Namespaces follow the
        folders, so an agent can find its way around with <Cmd>ls</Cmd>.
      </>
    ),
    commands: [{ cmd: 'baml init' }],
    title: 'Start a project',
  },
  {
    body: (
      <>
        This installs a BAML skill for Claude Code, Codex, and other coding
        agents, matched to your toolchain&rsquo;s version. With it, your agent
        reads code with <Cmd>baml describe</Cmd>, runs functions with{' '}
        <Cmd>baml run</Cmd>, and runs tests with <Cmd>baml test</Cmd>.
      </>
    ),
    commands: [{ cmd: 'baml agent install' }],
    title: 'Give your agent the BAML skill',
  },
  {
    body: 'Any BAML function runs from the command line, with its parameters as flags.',
    commands: [
      { cmd: 'baml run main', note: 'the default entry point' },
      { cmd: 'baml run greet -- --name "world"', note: 'any function' },
      {
        cmd: `baml run -e '"a,b,c".split(",")'`,
        note: 'an expression, without a file',
      },
    ],
    title: 'Run something',
  },
  {
    body: 'This step is optional. Agents work through the CLI, and the playground runs in a browser.',
    commands: [
      { cmd: 'baml ide install --code', note: 'VS Code' },
      { cmd: 'baml ide install --cursor', note: 'Cursor' },
      {
        cmd: 'baml ide install --path .',
        note: 'another VS Code-based editor: writes a .vsix here to install',
      },
      { cmd: 'baml lsp', note: 'Zed, JetBrains, Neovim, or any LSP client' },
      { cmd: 'baml playground', note: 'no editor at all' },
    ],
    title: 'Set up your editor',
  },
];

const HANDY_LINKS = [
  {
    external: true,
    href: 'https://github.com/boundaryml/baml-demos',
    title: 'Demo repo',
    value: 'github.com/boundaryml/baml-demos',
  },
];

// The smallest workflow that runs baml-action on pull requests. The action's README has the rest.
const WORKFLOW = `name: BAML
on: [pull_request]
permissions:
  contents: read
  pull-requests: write # for the code health comment
jobs:
  baml:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v5
      - uses: BoundaryML/baml-action@v1`;

const H2 =
  'font-display font-medium text-[24px] leading-[1.2] tracking-[-0.02em] text-head';
const BODY = 'text-[16px] leading-[1.6] text-body';

/** The quickstart: an agent prompt, the same setup as numbered steps, then production and help. */
export default function QuickstartPage() {
  return (
    <PageShell>
      <section
        {...section('quickstart_install')}
        className={`${PAGE_WRAP} pt-14 md:pt-20 pb-12 md:pb-16`}
      >
        <div className="max-w-[45rem]">
          <h1 className="v4-h1">Quickstart</h1>
          <p className="mt-6 md:mt-8 v4-body text-ink-2">
            Install BAML, give your coding agent the BAML skill, and run your
            first function. It takes a few minutes on macOS, Linux, or Windows.
          </p>

          <h2 className={`${H2} mt-12 md:mt-14`}>Set it up with your agent</h2>
          <p className={`mt-3 ${BODY}`}>
            Copy one prompt into Claude Code, Codex, or Cursor, and it runs the
            steps below for you.
          </p>
          {/* The install unit spans the column so it lines up with the steps below. */}
          <div className="mt-6 [&>div]:max-w-none">
            <TryBaml />
          </div>
        </div>
      </section>

      <section
        {...section('quickstart_steps')}
        aria-labelledby="steps"
        className={`${PAGE_WRAP} pb-12 md:pb-16`}
      >
        <div className="max-w-[45rem]">
          <h2 className={H2} id="steps">
            Or run the commands yourself
          </h2>
          <ol className="mt-8 flex flex-col gap-10">
            {STEPS.map((step, i) => (
              <li
                className="grid grid-cols-[2rem_minmax(0,1fr)] gap-x-4"
                key={step.title}
              >
                <span
                  aria-hidden
                  className="mt-0.5 size-7 grid place-items-center rounded-full border border-clover/50 font-mono text-[13px] text-clover"
                >
                  {i + 1}
                </span>
                <div className="flex flex-col gap-3">
                  <h3 className="text-[18px] font-medium leading-[1.4] text-head">
                    {step.title}
                  </h3>
                  <p className={BODY}>{step.body}</p>
                  <Commands commands={step.commands} />
                  {step.after && (
                    <>
                      <p className={`mt-1 ${BODY}`}>{step.after.body}</p>
                      <Commands commands={step.after.commands} />
                    </>
                  )}
                </div>
              </li>
            ))}
          </ol>
        </div>
      </section>

      <section
        {...section('quickstart_ci')}
        aria-labelledby="ci"
        className={`${PAGE_WRAP} pb-12 md:pb-16`}
      >
        <div className="max-w-[45rem] flex flex-col gap-4">
          <h2 className={H2} id="ci">
            Install the BAML code quality action
          </h2>
          <p className={BODY}>
            <a
              className="underline underline-offset-4 hover:text-ink"
              href="https://github.com/BoundaryML/baml-action"
              rel="noopener noreferrer"
              target="_blank"
            >
              <Cmd>BoundaryML/baml-action</Cmd>
            </a>{' '}
            checks, format-checks, and tests your project on each pull request,
            then comments with how its code quality compares to the base branch.
          </p>
          <pre className="overflow-x-auto rounded-xl border border-rule bg-raised px-4 py-3.5 font-mono text-[13.5px] leading-[1.6] text-ink">
            <span className="text-muted"># .github/workflows/baml.yml</span>
            {`\n${WORKFLOW}`}
          </pre>
          <Image
            alt="A baml-action comment on a pull request. One function, shipping_cost, got better: cognitive complexity went from 12 to 0, cyclomatic from 8 to 1, NPath from 36 to 1, and nesting from 3 to 0."
            className="h-auto w-full rounded-xl border border-rule"
            height={1018}
            sizes="(min-width: 768px) 720px, 100vw"
            src="/blog/teaching-agents-to-write-maintainable-code/baml-action-comment.png"
            width={1600}
          />
        </div>
      </section>

      <section
        {...section('quickstart_next')}
        className={`${PAGE_WRAP} pb-16 md:pb-24`}
      >
        <div className="max-w-[45rem] flex flex-col gap-12">
          <div className="grid sm:grid-cols-2 gap-3">
            <div className="flex flex-col gap-4 rounded-2xl border border-clover/40 bg-raised p-5 md:p-6">
              <div className="flex items-center gap-3">
                <svg
                  aria-hidden="true"
                  className="size-6 shrink-0 text-clover"
                  fill="none"
                  stroke="currentColor"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth={1.7}
                  viewBox="0 0 24 24"
                >
                  <path d="M7 18.5h10.5a4 4 0 0 0 .6-7.96A6 6 0 0 0 6.4 9.1 4.75 4.75 0 0 0 7 18.5Z" />
                </svg>
                <h2 className="font-display font-medium text-[20px] leading-[1.2] tracking-[-0.02em] text-head">
                  Run it in production
                </h2>
              </div>
              <p className="text-[16px] leading-[1.5] text-body">
                Boundary&rsquo;s cloud traces every BAML call and adds
                dashboards, alerts, and experiments on top. It&rsquo;s in early
                access, so get on a call with the founders and we&rsquo;ll set
                it up with you.
              </p>
              <div className="mt-auto pt-1">
                <Button href={BOOK}>Book a call</Button>
              </div>
            </div>
            <DiscordCard />
          </div>

          <div>
            <h2 className={H2}>Handy links</h2>
            <ul className="mt-4 grid sm:grid-cols-2 gap-3">
              {HANDY_LINKS.map((link) => (
                <li key={link.title}>
                  <a
                    className="group flex h-full flex-col gap-1 rounded-xl border border-rule bg-raised px-4 py-3.5 transition-colors hover:border-accent/50"
                    href={link.href}
                    {...(link.external
                      ? { rel: 'noopener noreferrer', target: '_blank' }
                      : {})}
                  >
                    <span className="flex items-center justify-between gap-3 text-[15px] font-medium text-head">
                      {link.title}
                      <span
                        aria-hidden
                        className="text-muted transition-colors group-hover:text-accent"
                      >
                        ↗
                      </span>
                    </span>
                    <span className="text-[14px] leading-normal text-body break-words">
                      {link.value}
                    </span>
                  </a>
                </li>
              ))}
            </ul>
          </div>

          <div>
            <h2 className={H2}>Keeping BAML updated</h2>
            <p className={`mt-4 border-l-2 border-rule pl-4 ${BODY}`}>
              The <Cmd>baml</Cmd> command is a version manager, so it rarely
              needs updating. Update it with your package manager (
              <Cmd>brew upgrade baml</Cmd>) or with <Cmd>baml self-update</Cmd>.
              If you change your project&rsquo;s toolchain version, run{' '}
              <Cmd>baml agent install</Cmd> again when the CLI asks. If
              something doesn&rsquo;t work, tell us on Discord.
            </p>
          </div>
        </div>
      </section>
    </PageShell>
  );
}
