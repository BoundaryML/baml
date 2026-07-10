'use client';

import { type ReactNode, useState } from 'react';
import styles from './try-baml.module.css';

// Shared "Try BAML" install unit used by /quickstart and the /explore
// "Try it out!" section. Humans pick an install method + editor and get the
// commands; agents get one paste-in prompt that sets everything up.

type Line = { cmd?: string; note?: string };
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
    cmd: 'brew install boundaryml/tap/baml',
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

const PROMPT = `Set up BAML in this repo so I can write great agent first code.

Install the toolchain (pick one for this machine):
- Homebrew (macOS/Linux): \`brew install boundaryml/tap/baml\`
- curl (macOS/Linux): \`curl -fsSL https://pkg.boundaryml.com/install.sh | sh -s\`
- Arch: \`yay -S baml-bin\`
- Windows: \`irm https://pkg.boundaryml.com/install.ps1 | iex\`

Then set up the project:
- \`baml init\`
- \`baml agent install\`
- \`baml run main\`

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
      {lines.map((l) =>
        l.cmd ? (
          <div className={styles.row} key={l.cmd}>
            <span className={styles.sh}>$ </span>
            {l.cmd}
          </div>
        ) : (
          <div className={`${styles.row} ${styles.cmt}`} key={l.note}>
            {l.note}
          </div>
        ),
      )}
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

export function TryBaml() {
  const [tab, setTab] = useState<'humans' | 'agents'>('humans');
  const [os, setOs] = useState('brew');
  const [env, setEnv] = useState('code');
  const [expanded, setExpanded] = useState(false);
  const [copied, setCopied] = useState(false);
  const [agentCopied, setAgentCopied] = useState(false);

  const osOpt = OS_OPTS.find((o) => o.id === os) ?? OS_OPTS[0];
  const envOpt = ENV_OPTS.find((e) => e.id === env) ?? ENV_OPTS[0];
  const envLines = envOpt.lines ?? [];

  const humansText = [
    osOpt.cmd,
    'baml init',
    'baml agent install',
    'baml run main',
    ...envLines.filter((l) => l.cmd).map((l) => l.cmd),
  ].join('\n');
  const promptText = PROMPT.replace(/`/g, '');
  const headText = tab === 'humans' ? humansText : promptText;

  const project: Line[] = [
    { cmd: 'baml init' },
    { cmd: 'baml agent install' },
    { note: '# sets up skills for Claude Code, Codex, and more' },
    { cmd: 'baml run main' },
  ];

  return (
    <div className={styles.unit}>
      <div aria-label="Install path" className={styles.head} role="tablist">
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
          aria-selected={tab === 'agents'}
          className={styles.tab}
          onClick={() => setTab('agents')}
          role="tab"
          type="button"
        >
          for agents
        </button>
        <button
          className={styles.copyBtn}
          onClick={() =>
            copyNow(headText, () => {
              setCopied(true);
              setTimeout(() => setCopied(false), 1600);
            })
          }
          type="button"
        >
          {copied ? 'copied!' : 'copy'}
        </button>
      </div>

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
        </div>
      ) : (
        <div className={`${styles.body} ${styles.agentPane}`}>
          <div className={styles.promptLead}>paste into your coding agent</div>
          <div
            className={`${styles.promptWrap}${expanded ? ` ${styles.expanded}` : ''}`}
          >
            <div className={styles.promptBox}>{renderPrompt(PROMPT)}</div>
          </div>
          <div className={styles.agentActions}>
            <button
              className={styles.agentCopy}
              onClick={() =>
                copyNow(promptText, () => {
                  setAgentCopied(true);
                  setTimeout(() => setAgentCopied(false), 1600);
                })
              }
              type="button"
            >
              {agentCopied ? 'Copied!' : 'Copy prompt'}
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
      )}
    </div>
  );
}
