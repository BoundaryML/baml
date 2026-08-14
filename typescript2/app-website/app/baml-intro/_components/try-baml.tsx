'use client';

import { Fragment, type ReactNode, useEffect, useState } from 'react';
import styles from './try-baml.module.css';

// Shared "Try BAML" install unit used by /quickstart and the /explore
// "Try it out!" section. Humans pick an install method + editor and get the
// commands; agents get one paste-in prompt that sets everything up.

type Tab = 'humans' | 'agents';
// `out` renders as terminal output under the command: dimmer, no `$`, and
// never included in what the copy button puts on the clipboard.
type Line = { cmd?: string; note?: string; out?: string[] };
type Opt = {
  id: string;
  label: string;
  icon: ReactNode;
  cmd?: string;
  lines?: Line[];
};

const clip = (
  <svg
    aria-hidden="true"
    fill="none"
    stroke="currentColor"
    strokeWidth={1.8}
    viewBox="0 0 24 24"
  >
    <rect height="11" rx="2" width="11" x="9" y="9" />
    <path d="M5 15V5a2 2 0 0 1 2-2h10" />
  </svg>
);

const OS_OPTS: Opt[] = [
  {
    cmd: 'brew install baml',
    icon: (
      <svg
        aria-hidden="true"
        fill="none"
        stroke="currentColor"
        strokeLinejoin="round"
        strokeWidth={1.7}
        viewBox="0 0 24 24"
      >
        <path d="M6 4h9v13a3 3 0 0 1-3 3H9a3 3 0 0 1-3-3z" />
        <path d="M15 7h2.4a2.5 2.5 0 0 1 0 5H15" strokeLinecap="round" />
      </svg>
    ),
    id: 'brew',
    label: 'Homebrew',
  },
  {
    cmd: 'curl -fsSL https://pkg.boundaryml.com/install.sh | sh -s',
    icon: (
      <svg
        aria-hidden="true"
        fill="none"
        stroke="currentColor"
        strokeWidth={1.7}
        viewBox="0 0 24 24"
      >
        <rect height="15" rx="2.5" width="18" x="3" y="4.5" />
        <path
          d="M7.5 10l3 2-3 2M13 14.5h4"
          strokeLinecap="round"
          strokeLinejoin="round"
        />
      </svg>
    ),
    id: 'curl',
    label: 'curl',
  },
  {
    cmd: 'yay -S baml-bin',
    icon: (
      <svg aria-hidden="true" fill="currentColor" viewBox="0 0 24 24">
        <path d="M12 3.5 3.5 20.5h17z" />
      </svg>
    ),
    id: 'arch',
    label: 'Arch',
  },
  {
    cmd: 'irm https://pkg.boundaryml.com/install.ps1 | iex',
    icon: (
      <svg aria-hidden="true" fill="currentColor" viewBox="0 0 24 24">
        <path d="M3 5.6 10.4 4.5v6.9H3zM11.3 4.4 21 3v8.4h-9.7zM3 12.6h7.4v6.9L3 18.4zM11.3 12.6H21V21l-9.7-1.4z" />
      </svg>
    ),
    id: 'win',
    label: 'Windows',
  },
];

const ENV_OPTS: Opt[] = [
  {
    icon: (
      <svg aria-hidden="true" fill="currentColor" viewBox="0 0 24 24">
        <path d="M18.3 2.2 12.7 7.5 8.1 3.9 6.7 4.9l3.9 3.6-3.9 3.6L8.1 13l4.6-3.6 5.6 5.2 3.3-1.4V3.7zM18 6.5v5.8l-3.5-2.9z" />
      </svg>
    ),
    id: 'code',
    label: 'VS Code',
    lines: [{ cmd: 'baml ide install --code' }],
  },
  {
    icon: (
      <svg aria-hidden="true" fill="currentColor" viewBox="0 0 24 24">
        <path d="M12 2.4 3.6 7v10l8.4 4.6 8.4-4.6V7zm0 2.1 5.8 3.2L12 10.9 6.2 7.7zM5 9.2l6.2 3.4v6.9L5 15.9zm14 0v6.7l-6.2 3.5v-6.9z" />
      </svg>
    ),
    id: 'cursor',
    label: 'Cursor',
    lines: [{ cmd: 'baml ide install --cursor' }],
  },
  {
    icon: (
      <svg
        aria-hidden="true"
        fill="none"
        stroke="currentColor"
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth={1.8}
        viewBox="0 0 24 24"
      >
        <path d="M12 3v11m0 0 4-4m-4 4-4-4M5 17v2a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2v-2" />
      </svg>
    ),
    id: 'vsix',
    label: '.vsix',
    lines: [
      {
        cmd: 'baml ide install --path .',
        note: '# saves baml-vscode.vsix for other VS Code forks. drag it into Extensions to install',
      },
    ],
  },
  {
    icon: (
      <svg
        aria-hidden="true"
        fill="none"
        stroke="currentColor"
        strokeWidth={1.7}
        viewBox="0 0 24 24"
      >
        <circle cx="12" cy="12" r="9" />
        <path
          d="M3 12h18M12 3a15 15 0 0 1 0 18 15 15 0 0 1 0-18"
          strokeLinecap="round"
        />
      </svg>
    ),
    id: 'noide',
    label: 'No IDE',
    lines: [
      {
        cmd: 'baml playground',
        note: '# opens in your browser, no editor needed',
      },
    ],
  },
  {
    icon: (
      <svg aria-hidden="true" fill="currentColor" viewBox="0 0 24 24">
        <circle cx="5" cy="12" r="1.7" />
        <circle cx="12" cy="12" r="1.7" />
        <circle cx="19" cy="12" r="1.7" />
      </svg>
    ),
    id: 'other',
    label: 'Other',
    lines: [
      {
        cmd: 'baml lsp',
        note: "# run this as your editor's language server (Zed, JetBrains, Neovim)",
      },
      { cmd: 'baml playground', note: '# or open the browser playground' },
    ],
  },
];

// Host-language bridges. `add` is the exact output_type accepted by
// `baml bridge add`, straight from OutputType::add_name in
// baml_cli/src/generate.rs — the names are not free-form, so keep them in sync
// with that enum. `next` is what the CLI reports as the remaining host-side
// step. It renders as terminal output rather than as an instruction, so this
// page is never the authority on package names or versions. Kotlin has no
// generator of its own: it consumes the Java output through a Kotlin-flavored
// runtime the Gradle plugin wires up automatically.
type Bridge = {
  id: string;
  label: string;
  logo: string;
  add: string;
  next: string[];
};

// Deliberately no version numbers. BAML ships constantly, so anything written
// down here is wrong within days. The CLI knows its own version and prints it;
// the transcript elides it as <version> rather than guessing.
const BRIDGES: Bridge[] = [
  {
    add: 'python/pydantic',
    id: 'py',
    label: 'Python',
    logo: 'python',
    // baml_bridge only depends on protobuf + typing-extensions, so the models
    // the generator emits need pydantic named explicitly.
    next: ['uv add baml_bridge pydantic'],
  },
  {
    add: 'typescript/node',
    id: 'node',
    label: 'TypeScript',
    logo: 'nodejs',
    next: ['npm install @boundaryml/baml-bridge'],
  },
  {
    add: 'typescript/web',
    id: 'web',
    label: 'TS / Web',
    logo: 'wasm',
    next: ['npm install @boundaryml/baml-bridge-web'],
  },
  {
    add: 'go',
    id: 'go',
    label: 'Go',
    logo: 'go',
    next: ['go get github.com/boundaryml/baml-go'],
  },
  {
    add: 'rust',
    id: 'rs',
    label: 'Rust',
    logo: 'rust',
    next: ['cargo add baml_bridge'],
  },
  {
    add: 'java',
    id: 'java',
    label: 'Java',
    logo: 'java',
    // The Gradle plugin injects com.boundaryml:baml-bridge at its own version,
    // so applying the plugin is the whole setup.
    next: [
      'build.gradle.kts: plugins { id("com.boundaryml.baml") version "<version>" }',
    ],
  },
  {
    add: 'java',
    id: 'kt',
    label: 'Kotlin',
    logo: 'kotlin',
    // Same plugin: it detects org.jetbrains.kotlin.jvm and adds the Kotlin
    // ergonomics layer alongside the bridge, at the matching version.
    next: [
      'build.gradle.kts: plugins { id("com.boundaryml.baml") version "<version>" }',
      'kotlin jvm detected, baml-bridge-kotlin will be added too',
    ],
  },
  {
    add: 'csharp',
    id: 'cs',
    label: 'C#',
    logo: 'csharp',
    next: ['dotnet add package baml-bridge'],
  },
  {
    add: 'cpp',
    id: 'cpp',
    label: 'C++',
    logo: 'cplusplus',
    next: ['nothing to install, the generated tree just works!'],
  },
  {
    add: 'swift',
    id: 'swift',
    label: 'Swift',
    logo: 'swift',
    next: [
      'Package.swift: .package(url: "https://github.com/BoundaryML/baml-swift", from: "<version>")',
    ],
  },
  {
    add: 'python/pydantic/v1',
    id: 'py1',
    label: 'Pydantic v1',
    logo: 'python',
    next: ["uv add baml_bridge 'pydantic<2'"],
  },
];

// The agent prompt lists the same targets as the picker, derived from BRIDGES
// rather than restated, so the two can never disagree.
const BRIDGE_TARGETS = BRIDGES.map((b) => {
  const hint =
    b.id === 'go'
      ? ' (also pass --sdk-import-path <MODULE>/baml_sdk)'
      : b.id === 'kt'
        ? ' (the Gradle plugin adds the Kotlin layer)'
        : '';
  return `- ${b.label}: \`${b.add}\`${hint}`;
}).join('\n');

// A transcript, not a recipe: the two commands the reader runs, and what BAML
// says back, including the host package it resolved for them. The Go generator
// needs an import path for the package it emits, so that flag rides along in
// the command instead of waiting to be discovered from an error.
function bridgeLines(b: Bridge): Line[] {
  const add =
    b.add === 'go'
      ? 'baml bridge add go --sdk-import-path <MODULE>/baml_sdk'
      : `baml bridge add ${b.add}`;
  const dir = b.add === 'csharp' ? 'baml_client/' : 'baml_sdk/';
  return [
    {
      cmd: add,
      note: '# once per language',
      out: ['added generator to baml.toml', ...b.next.map((n) => `next: ${n}`)],
    },
    {
      cmd: 'baml bridge generate',
      note: '# after every change to your .baml files',
      out: [`wrote ${dir}`],
    },
  ];
}

const PROMPT = `Set up BAML in this repo so I can write great agent first code.

Install the toolchain (pick one for this machine):
- Homebrew (macOS/Linux): \`brew install baml\`
- curl (macOS/Linux): \`curl -fsSL https://pkg.boundaryml.com/install.sh | sh -s\`
- Arch: \`yay -S baml-bin\`
- Windows: \`irm https://pkg.boundaryml.com/install.ps1 | iex\`

Then set up the project:
- \`baml init\`
- \`baml agent install\`
- \`baml run main\`

If this repo already has an app in another language, wire up the bridge so it
can call BAML directly. Run \`baml bridge add <target>\` once, then
\`baml bridge generate\` whenever the .baml files change. The add command prints
the host package to install. Targets:
${BRIDGE_TARGETS}

After that, use the BAML skill and \`baml describe\` to write and edit .baml files.`;

function copyNow(text: string, done: () => void) {
  navigator.clipboard?.writeText(text);
  done();
}

function OptRow({
  cap,
  ariaLabel,
  opts,
  value,
  onChange,
}: {
  cap: string;
  ariaLabel: string;
  opts: Opt[];
  value: string;
  onChange: (id: string) => void;
}) {
  return (
    <div aria-label={ariaLabel} className={styles.opt} role="tablist">
      <span className={styles.optCap}>{cap}</span>
      {opts.map((o) => (
        <button
          aria-selected={value === o.id}
          className={styles.optBtn}
          key={o.id}
          onClick={() => onChange(o.id)}
          role="tab"
          type="button"
        >
          {o.icon}
          {o.label}
        </button>
      ))}
    </div>
  );
}

function Section({ lines, label }: { lines: Line[]; label: string }) {
  const [ok, setOk] = useState(false);
  const copyText = lines
    .filter((l) => l.cmd)
    .map((l) => l.cmd)
    .join('\n');
  return (
    <div className={styles.sect}>
      {lines.map((l) => (
        // Comments always render on their own line above the command.
        <Fragment key={l.cmd ?? l.note}>
          {l.note ? (
            <div className={`${styles.row} ${styles.cmt}`}>{l.note}</div>
          ) : null}
          {l.cmd ? (
            <div className={styles.row}>
              <span className={styles.sh}>$ </span>
              {l.cmd}
            </div>
          ) : null}
          {/* What BAML prints back. Never part of the copied text. */}
          {l.out?.map((o) => (
            <div className={`${styles.row} ${styles.out}`} key={o}>
              {o}
            </div>
          ))}
        </Fragment>
      ))}
      <button
        aria-label={label}
        className={`${styles.sectCopy}${ok ? ` ${styles.ok}` : ''}`}
        onClick={() =>
          copyNow(copyText, () => {
            setOk(true);
            setTimeout(() => setOk(false), 1400);
          })
        }
        type="button"
      >
        {clip}
      </button>
    </div>
  );
}

// humans/agents switch plus the copy-all button. Extracted so the compact
// variant can show it too.
function Head({
  tab,
  setTab,
  text,
}: {
  tab: Tab;
  setTab: (t: Tab) => void;
  text: string;
}) {
  const [copied, setCopied] = useState(false);
  return (
    <div aria-label="Install path" className={styles.head} role="tablist">
      <button
        aria-selected={tab === 'agents'}
        className={styles.tab}
        onClick={() => setTab('agents')}
        role="tab"
        type="button"
      >
        for agents
      </button>
      <button
        aria-selected={tab === 'humans'}
        className={styles.tab}
        onClick={() => setTab('humans')}
        role="tab"
        type="button"
      >
        for humans
      </button>
      <button
        className={styles.copyBtn}
        onClick={() =>
          copyNow(text, () => {
            setCopied(true);
            setTimeout(() => setCopied(false), 1600);
          })
        }
        type="button"
      >
        {copied ? 'copied!' : 'copy'}
      </button>
    </div>
  );
}

function renderPrompt(text: string) {
  return text.split(/`([^`]+)`/).map((part, i) =>
    i % 2 === 1 ? (
      // biome-ignore lint/suspicious/noArrayIndexKey: split output is positional
      <span className={styles.bin} key={`c${i}`}>
        {part}
      </span>
    ) : (
      part
    ),
  );
}

// The one-click path: copy a single prompt, paste into a coding agent.
function AgentPane({ text }: { text: string }) {
  const [expanded, setExpanded] = useState(false);
  const [copied, setCopied] = useState(false);
  return (
    <div className={`${styles.body} ${styles.agentPane}`}>
      <div
        className={`${styles.promptWrap}${expanded ? ` ${styles.expanded}` : ''}`}
      >
        <div className={styles.promptBox}>{renderPrompt(PROMPT)}</div>
      </div>
      <div className={styles.agentActions}>
        <button
          className={styles.agentCopy}
          onClick={() =>
            copyNow(text, () => {
              setCopied(true);
              setTimeout(() => setCopied(false), 1600);
            })
          }
          type="button"
        >
          {copied ? 'Copied!' : 'Copy prompt'}
        </button>
        <button
          aria-expanded={expanded}
          className={styles.agentExpand}
          onClick={() => setExpanded((v) => !v)}
          type="button"
        >
          {expanded ? 'collapse ↑' : 'expand ↓'}
        </button>
      </div>
    </div>
  );
}

// Agents lead everywhere: pasting one prompt into a coding agent is the
// cheapest way to get started, and the agent is what writes the .baml files
// anyway. The CLI commands are one click away on the humans tab.
export function TryBaml({ compact = false }: { compact?: boolean } = {}) {
  const [tab, setTab] = useState<Tab>('agents');
  const [os, setOs] = useState('brew');
  const [env, setEnv] = useState('code');
  const [bridge, setBridge] = useState('py');

  // Default the install method to the visitor's platform.
  useEffect(() => {
    const ua = navigator.userAgent;
    if (/Windows/i.test(ua)) {
      setOs('win');
    } else if (/Linux|X11/i.test(ua) && !/Android/i.test(ua)) {
      setOs('curl');
    }
  }, []);

  const osOpt = OS_OPTS.find((o) => o.id === os) ?? OS_OPTS[0];
  const envOpt = ENV_OPTS.find((e) => e.id === env) ?? ENV_OPTS[0];
  const envLines = envOpt.lines ?? [];
  const bridgeOpt = BRIDGES.find((b) => b.id === bridge) ?? BRIDGES[0];
  const bridgeOpts: Opt[] = BRIDGES.map((b) => ({
    // biome-ignore lint/performance/noImgElement: 12px static local svg sized in css; next/image buys nothing here
    icon: <img alt="" className={styles.logo} src={`/logos/${b.logo}.svg`} />,
    id: b.id,
    label: b.label,
  }));

  // Copy-all on the header takes the whole path the reader is looking at,
  // bridge included, so it matches what the panel below actually shows.
  const humansText = [
    osOpt.cmd,
    'baml init',
    'baml agent install',
    'baml run main',
    ...(compact
      ? []
      : [
          ...envLines.filter((l) => l.cmd).map((l) => l.cmd),
          ...bridgeLines(bridgeOpt).flatMap((l) => (l.cmd ? [l.cmd] : [])),
        ]),
  ].join('\n');
  const promptText = PROMPT.replace(/`/g, '');
  const headText = tab === 'humans' ? humansText : promptText;

  const project: Line[] = [
    { cmd: 'baml init' },
    {
      cmd: 'baml agent install',
      note: '# sets up skills for Claude Code, Codex, and more',
    },
    { cmd: 'baml run main' },
  ];

  // Slim variant: the humans tab drops the editor picker and keeps the platform
  // install plus project commands. Full options live in the complete unit
  // further down and on /quickstart.
  if (compact) {
    return (
      <div className={`${styles.unit} ${styles.compact}`}>
        <Head setTab={setTab} tab={tab} text={headText} />
        {tab === 'agents' ? (
          <AgentPane text={promptText} />
        ) : (
          <div className={styles.body}>
            <OptRow
              ariaLabel="Install method"
              cap="install with"
              onChange={setOs}
              opts={OS_OPTS}
              value={os}
            />
            <Section
              key={os}
              label="Copy install command"
              lines={[{ cmd: osOpt.cmd }]}
            />
            <div className={styles.optLabel}>in a project</div>
            <Section label="Copy project commands" lines={project} />
          </div>
        )}
        {/* The compact unit stops at "it runs". Editor and playground setup,
            and generating the bridge into your own codebase, are the next
            step wherever this appears. */}
        <p className={styles.next}>
          Next: <a href="/quickstart">editor and playground setup</a>, and{' '}
          <a href="/quickstart">calling BAML from your language</a>
        </p>
      </div>
    );
  }

  return (
    <div className={styles.unit}>
      <Head setTab={setTab} tab={tab} text={headText} />

      {tab === 'humans' ? (
        <div className={styles.body}>
          <OptRow
            ariaLabel="Install method"
            cap="install with"
            onChange={setOs}
            opts={OS_OPTS}
            value={os}
          />
          <Section
            key={os}
            label="Copy install command"
            lines={[{ cmd: osOpt.cmd }]}
          />
          <OptRow
            ariaLabel="Where you code"
            cap="editor"
            onChange={setEnv}
            opts={ENV_OPTS}
            value={env}
          />
          <Section key={env} label="Copy editor command" lines={envLines} />
          <div className={styles.optLabel}>in a project</div>
          <Section label="Copy project commands" lines={project} />
          <OptRow
            ariaLabel="Host language"
            cap="call from"
            onChange={setBridge}
            opts={bridgeOpts}
            value={bridge}
          />
          <Section
            key={bridge}
            label="Copy bridge commands"
            lines={bridgeLines(bridgeOpt)}
          />
        </div>
      ) : (
        <AgentPane text={promptText} />
      )}
    </div>
  );
}
