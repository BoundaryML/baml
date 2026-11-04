import type { Metadata } from 'next';
import type { ReactNode } from 'react';
import { section } from '@/components/analytics';
import Command from '@/components/command';
import Footer from '@/components/footer';
import Install from '@/components/install';
import Nav from '@/components/nav';
import PageHeader from '@/components/page-header';
import { SectionViews } from '@/components/track';
import { Arrow, Cmd, LANES, LEFT, LINKS, PAD_Y, WRAP } from '@/components/ui';

// The steps of the quickstart that boundaryml.com/quickstart had (typescript2/app-website's
// content/quickstart.md), set in this site's design: one section per step, the explanation in the
// left lane and its commands in the right, the way the Status and Stack sections lay out.

export const metadata: Metadata = {
  alternates: { canonical: '/quickstart' },
  description:
    'Install the BAML toolchain, start a project, give your coding agent the version-matched BAML skill, and run your first function.',
  title: 'Quickstart · BAML',
};

type CommandSpec = { label: string; lines: string[] };

function Step({
  id,
  n,
  title,
  children,
  commands,
}: {
  id: string;
  n: number;
  title: string;
  children: ReactNode;
  commands: CommandSpec[];
}) {
  return (
    <section
      {...section(`step_${id}`)}
      aria-labelledby={`${id}-title`}
      className={`${WRAP} ${PAD_Y} ${LANES}`}
    >
      <div className={`${LEFT} flex flex-col gap-4`}>
        <span className="font-mono text-[13px] text-muted">
          {String(n).padStart(2, '0')}
        </span>
        <h2
          className="font-display font-medium text-[clamp(28px,2.4vw,34px)] leading-10 tracking-[-0.03em] text-head"
          id={`${id}-title`}
        >
          {title}
        </h2>
        <div className="flex flex-col gap-4 text-base leading-[26px] text-body">
          {children}
        </div>
      </div>
      <div className="flex-1 min-w-0 flex flex-col gap-4">
        {commands.map((c) => (
          <Command key={c.label} label={c.label} lines={c.lines} />
        ))}
      </div>
    </section>
  );
}

const HELP = [
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
  {
    href: LINKS.github,
    label: 'Source code',
    url: 'github.com/boundaryml/baml',
  },
];

export default function Quickstart() {
  return (
    <>
      <Nav />
      <main className="flex-1" id="main">
        <PageHeader
          lead={
            <>
              Install the toolchain, start a project, give your coding agent the
              BAML skill that matches it, and run your first function. Paste the
              prompt into an agent, or run the steps yourself.
            </>
          }
          title="Quickstart"
        >
          <div className="w-full max-w-[640px]">
            <Install />
          </div>
        </PageHeader>

        <Step
          commands={[
            { label: 'Homebrew, macOS or Linux', lines: ['brew install baml'] },
            {
              label: 'Install script, macOS or Linux',
              lines: [
                'curl -fsSL https://pkg.boundaryml.com/install.sh | sh -s',
              ],
            },
            { label: 'Arch Linux', lines: ['yay -S baml-bin'] },
            {
              label: 'Windows PowerShell',
              lines: ['irm https://pkg.boundaryml.com/install.ps1 | iex'],
            },
            { label: 'Check the install', lines: ['baml --version'] },
          ]}
          id="install"
          n={1}
          title="Install the toolchain"
        >
          <p>
            Pick one method for your operating system, then check that{' '}
            <Cmd>baml</Cmd> is on your path.
          </p>
        </Step>

        <Step
          commands={[{ label: 'In your repository', lines: ['baml init'] }]}
          id="init"
          n={2}
          title="Start a project"
        >
          <p>
            Run <Cmd>baml init</Cmd> in the repository where you want BAML. It
            creates the project&apos;s first files. Namespaces follow the
            filesystem, so an agent can read the layout with tools like{' '}
            <Cmd>ls</Cmd>.
          </p>
        </Step>

        <Step
          commands={[
            { label: 'Agent guidance', lines: ['baml agent install'] },
          ]}
          id="agent"
          n={3}
          title="Give your agent the BAML skill"
        >
          <p>
            <Cmd>baml agent install</Cmd> installs the BAML skill for coding
            agents such as Claude Code and Codex. It belongs to the project and
            matches its toolchain, so when it disagrees with older docs online,
            the skill is right.
          </p>
          <p>When an agent works on .baml files, it uses:</p>
          <ul className="flex flex-col gap-2 pl-5 list-disc marker:text-muted">
            <li>the installed skill, for the language and the workflow</li>
            <li>
              <Cmd>baml describe</Cmd>, for definitions, signatures,
              dependencies and references
            </li>
            <li>
              <Cmd>baml run</Cmd> to run functions, and <Cmd>baml test</Cmd> to
              run tests
            </li>
          </ul>
        </Step>

        <Step
          commands={[
            { label: 'The default entry point', lines: ['baml run main'] },
            {
              label: 'Any function, its parameters as flags',
              lines: [
                'baml run greet -- --name "world"',
                'baml run greet -- --help',
              ],
            },
            {
              label: 'An expression, without a file',
              lines: [`baml run -e '"a,b,c".split(",")'`],
            },
          ]}
          id="run"
          n={4}
          title="Run it"
        >
          <p>
            <Cmd>baml run main</Cmd> runs the default <Cmd>main</Cmd> function.
            Any BAML function is a CLI target, and its parameters become
            command-line flags.
          </p>
        </Step>

        <Step
          commands={[
            { label: 'VS Code', lines: ['baml ide install --code'] },
            { label: 'Cursor', lines: ['baml ide install --cursor'] },
            {
              label: 'Other VS Code-compatible editors: save a .vsix here',
              lines: ['baml ide install --path .'],
            },
            {
              label: 'Zed, JetBrains, Neovim and other LSP clients',
              lines: ['baml lsp'],
            },
            {
              label: 'No editor: the browser playground',
              lines: ['baml playground'],
            },
          ]}
          id="editor"
          n={5}
          title="Set up an editor"
        >
          <p>
            Optional: agents work through the CLI. For other VS Code-compatible
            editors, install the saved .vsix from the Extensions view. For any
            other editor, point its language server for .baml files at{' '}
            <Cmd>baml lsp</Cmd>.
          </p>
        </Step>

        <Step
          commands={[
            { label: 'With Homebrew', lines: ['brew upgrade baml'] },
            { label: 'Or the built-in updater', lines: ['baml self-update'] },
          ]}
          id="update"
          n={6}
          title="Keep BAML up to date"
        >
          <p>
            The top-level <Cmd>baml</Cmd> is a version manager, so it rarely
            needs updating itself. After you change a project&apos;s toolchain
            version, run <Cmd>baml agent install</Cmd> again if the CLI says the
            agent guidance needs a refresh.
          </p>
        </Step>

        <section
          {...section('help')}
          aria-labelledby="help-title"
          className={`${WRAP} ${PAD_Y} ${LANES}`}
        >
          <div className={`${LEFT} flex flex-col gap-4`}>
            <h2
              className="font-display font-medium text-[clamp(28px,2.4vw,34px)] leading-10 tracking-[-0.03em] text-head"
              id="help-title"
            >
              Getting help
            </h2>
            <p className="text-base leading-[26px] text-body">
              When you ask, include the output of <Cmd>baml --version</Cmd> and
              the command or file that caused the problem.
            </p>
          </div>
          <ul className="flex-1 min-w-0 border-t border-rule-accent">
            {HELP.map((l) => (
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
      </main>
      <Footer />
      <SectionViews />
    </>
  );
}
