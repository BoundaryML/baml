'use client';

import { type ReactNode, useEffect, useRef, useState } from 'react';
import { FaWindows } from 'react-icons/fa';
import {
  SiApple,
  SiClaude,
  SiGithub,
  SiGo,
  SiLinux,
  SiRust,
  SiTypescript,
} from 'react-icons/si';
import { PostDrawing } from '@/app/(home)/blog/_lib/art';
import { section } from '@/components/home/analytics';
import { BAML_LOGO, LAMB_PATHS } from '@/components/home/lamb';
import {
  Hyper,
  leftVertically,
  sweep,
  useInView,
  useReducedMotion,
  useTween,
  useVisible,
} from '@/components/home/pinned';
import { ramp } from '@/components/home/scroll';

// Can agents write good BAML? Real tooling output throughout: the errors come from `baml check`, the
// steps from baml-action, and the comment follows the action's render.baml. The metrics are
// `baml codequality` on three versions of one example function.

// The titles inside this section are set in the pixel font, like the BAML lamb.
const PIXEL = { fontFamily: 'var(--font-geist-pixel)' };

function Box({ children, title }: { children: ReactNode; title: string }) {
  return (
    <div className="mt-4 md:mt-6 rounded-xl border border-rule bg-raised p-5">
      <p className="v4-h3 text-head" style={PIXEL}>
        {title}
      </p>
      {children}
    </div>
  );
}

// ---------- errors ----------

const AGENT = `main.baml:15:5-19:6 error[E0062]: non-exhaustive match on type Label; missing: Label.Refund
main.baml:23:17-23:31 error[E0007]: type \`Ticket\` has no member \`priorty\`
main.baml:28:24-28:39 error[E0001]: mismatched types
  primary: expected \`string\`, found \`int\``;

const HUMAN_CHARS = 664;
const AGENT_CHARS = 260;
const SAVED = Math.round((1 - AGENT_CHARS / HUMAN_CHARS) * 100);

const ERROR_RED = '#f2777a';

function Line({ children, no }: { children: ReactNode; no: number }) {
  return (
    <div className="grid grid-cols-[32px_minmax(0,1fr)] gap-3">
      <span className="text-right tabular-nums text-white/35">{no}</span>
      <span className="whitespace-pre text-white/90">{children}</span>
    </div>
  );
}

function ErrorCard({
  children,
  code,
  file,
  title,
}: {
  children: ReactNode;
  code: string;
  file: string;
  title: string;
}) {
  return (
    <div className="rounded-lg border border-white/10 bg-white/[0.03]">
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1 border-b border-white/10 px-4 py-2.5">
        <span
          className="rounded px-1.5 py-0.5 font-mono text-[12px] font-semibold"
          style={{ background: 'rgb(242 119 122 / 0.15)', color: ERROR_RED }}
        >
          {code}
        </span>
        <span className="text-[15px] font-medium text-white">{title}</span>
        <span className="ml-auto font-mono text-[12px] text-white/45">
          {file}
        </span>
      </div>
      <div className="px-4 py-3 font-mono text-[14px] leading-[1.7]">
        {children}
      </div>
    </div>
  );
}

const Squiggle = ({ children }: { children: string }) => (
  <span
    className="underline decoration-wavy underline-offset-4"
    style={{ textDecorationColor: ERROR_RED }}
  >
    {children}
  </span>
);

function HumanErrors() {
  return (
    <div className="flex flex-col gap-3">
      <ErrorCard
        code="E0062"
        file="main.baml:15:5"
        title="non-exhaustive match on type Label"
      >
        <div
          className="border-l-2 pl-2 -ml-2"
          style={{ borderColor: ERROR_RED }}
        >
          <Line no={15}>
            {'    match ('}
            <Squiggle>label</Squiggle>
            {') {'}
          </Line>
          <Line no={16}>{'        Label.Billing => "finance",'}</Line>
          <Line no={17}>{'        Label.Bug => "engineering",'}</Line>
          <Line no={18}>{'        Label.Spam => "trash",'}</Line>
          <Line no={19}>{'    }'}</Line>
        </div>
        <p className="mt-2 font-sans text-[14px]" style={{ color: ERROR_RED }}>
          Missing: Label.Refund
        </p>
      </ErrorCard>
      <ErrorCard
        code="E0007"
        file="main.baml:23:17"
        title="type Ticket has no member priorty"
      >
        <Line no={23}>
          {'    let score = '}
          <Squiggle>ticket.priorty</Squiggle>
          {';'}
        </Line>
      </ErrorCard>
      <ErrorCard code="E0001" file="main.baml:28:24" title="mismatched types">
        <Line no={28}>
          {'    let rank: string = '}
          <Squiggle>ticket.priority</Squiggle>
          {';'}
        </Line>
        <p className="mt-2 font-sans text-[14px]" style={{ color: ERROR_RED }}>
          Expected string, found int
        </p>
      </ErrorCard>
    </div>
  );
}

// `baml check` as a person sees it, exactly as printed.
const TERMINAL = `E0062

  × non-exhaustive match on type Label; missing: Label.Refund
    ╭─[main.baml:15:5]
 15 │ ╭─▶     match (label) {
 16 │ │           Label.Billing => "finance",
 17 │ │           Label.Bug => "engineering",
 18 │ │           Label.Spam => "trash",
 19 │ ╰─▶     }
    ╰────

E0007

  × type \`Ticket\` has no member \`priorty\`
    ╭─[main.baml:23:17]
 23 │     let score = ticket.priorty;
    ·                 ──────────────
    ╰────

E0001

  × mismatched types
    ╭─[main.baml:28:24]
 28 │     let rank: string = ticket.priority;
    ·                        ───────┬───────
    ·                               ╰── expected \`string\`, found \`int\`
    ╰────`;

const TERMINAL_FONT = 'var(--font-mono)';

function lineColor(line: string) {
  if (/^E\d{4}/.test(line) || line.trim().startsWith('×')) return ERROR_RED;
  if (/^\s+[╭╰·]/.test(line) && !/[┬╰──]\s*expected/.test(line))
    return 'rgb(242 119 122 / 0.75)';
  return undefined;
}

function TerminalErrors() {
  return (
    <div>
      <p className="mb-2 font-mono text-[13px] text-white">
        <span className="text-white/45">$ </span>baml check
      </p>
      <pre
        className="overflow-x-auto text-[13px] text-white/90"
        style={{ fontFamily: TERMINAL_FONT, lineHeight: 1.3 }}
      >
        {TERMINAL.split('\n').map((line, i) => (
          <span
            className="block min-h-[1.3em]"
            // biome-ignore lint/suspicious/noArrayIndexKey: fixed output
            key={i}
            style={{ color: lineColor(line) }}
          >
            {line}
          </span>
        ))}
      </pre>
    </div>
  );
}

function ClaudeCodeErrors() {
  return (
    <div
      className="text-[13px] leading-[1.6] text-white/90"
      style={{ fontFamily: TERMINAL_FONT }}
    >
      <p className="mb-4 flex items-center gap-2 font-sans text-[14px] text-white/60">
        <SiClaude
          aria-hidden="true"
          className="text-[16px]"
          style={{ color: '#D97757' }}
        />
        Claude Code
      </p>
      <p>
        <span style={{ color: ERROR_RED }}>⏺</span>{' '}
        <span className="font-bold text-white">Bash</span>
        (baml check)
      </p>
      <div className="mt-0.5 grid grid-cols-[auto_1fr] gap-x-2.5 pl-2">
        <span className="text-white/45">⎿</span>
        <pre
          className="min-w-0 whitespace-pre-wrap"
          style={{ fontFamily: TERMINAL_FONT }}
        >
          <span style={{ color: ERROR_RED }}>Error: Exit code 4{'\n'}</span>
          {AGENT}
        </pre>
      </div>
    </div>
  );
}

const READERS = ['For IDEs', 'For humans', 'For agents'];

function Errors() {
  const [at, setAt] = useState(2);
  return (
    <Box title="Compiler errors built for every reader">
      <div className="mt-5 overflow-hidden rounded-xl border border-white/10 bg-black/40">
        <div className="flex flex-wrap items-center border-b border-white/10 px-2">
          {READERS.map((name, i) => {
            const on = i === at;
            return (
              <button
                aria-pressed={on}
                className={`relative h-11 whitespace-nowrap px-2.5 text-[14px] sm:px-3.5 sm:text-[15px] transition-colors ${on ? 'text-white' : 'text-white/50 hover:text-white/80'}`}
                key={name}
                onClick={() => setAt(i)}
                type="button"
              >
                {name}
                <span
                  className="absolute inset-x-3 bottom-0 h-0.5 rounded-full bg-clover transition-opacity"
                  style={{ opacity: on ? 1 : 0 }}
                />
              </button>
            );
          })}
          {at > 0 && (
            <span className="ml-auto flex items-center gap-3 pb-2 pr-2 sm:pb-0">
              {at === 2 && (
                <span className="rounded-full bg-clover px-3 py-1 text-[15px] font-semibold text-black">
                  {SAVED}% less context
                </span>
              )}
              <span className="hidden sm:inline font-mono text-[13px] text-white/45">
                {at === 1 ? HUMAN_CHARS : AGENT_CHARS} characters
              </span>
            </span>
          )}
        </div>
        <div className="min-h-[430px] p-4">
          {at === 0 && <HumanErrors />}
          {at === 1 && <TerminalErrors />}
          {at === 2 && <ClaudeCodeErrors />}
        </div>
      </div>
    </Box>
  );
}

// ---------- compile speed ----------

// `baml check`, `go vet`, `tsc` (7.0.2) and `cargo check` on the same program written in each
// language, about 30,000 lines, edited before every run so nothing is cached. Go had its standard
// library compiled already. Median of three, in seconds.
const LANGUAGES = ['TypeScript', 'Go', 'BAML', 'Rust'];
const KINDS = [
  { name: 'Simple code', times: [0.1, 0.151, 0.259, 0.799] },
  { name: 'Generics', times: [0.151, 0.271, 0.476, 1.61] },
  { name: 'Recursive types', times: [0.101, 0.192, 1.077, 0.56] },
  { name: 'Interfaces', times: [0.105, 0.164, 0.443, 1.205] },
];
const BAML_ROW = LANGUAGES.indexOf('BAML');

function CompilerName({ name }: { name: string }) {
  if (name === 'BAML') {
    return (
      <svg
        aria-label="BAML"
        className="h-[32px] w-auto"
        fill="#fff"
        role="img"
        viewBox={BAML_LOGO.viewBox}
      >
        {BAML_LOGO.paths.map((d) => (
          <path d={d} key={d} />
        ))}
      </svg>
    );
  }
  const Icon = name === 'Go' ? SiGo : name === 'Rust' ? SiRust : SiTypescript;
  return (
    <p className="flex items-center gap-2 whitespace-nowrap text-[16px] text-white">
      <Icon aria-hidden="true" className="shrink-0" />
      {name}
    </p>
  );
}

function Speed() {
  const [at, setAt] = useState(0);
  const { times } = KINDS[at];
  // BAML first, then the same order on every tab, so nothing jumps around.
  const rows = ['BAML', 'TypeScript', 'Go', 'Rust'].map((name) => ({
    name,
    s: times[LANGUAGES.indexOf(name)],
  }));
  const most = Math.max(...times);
  const base = times[BAML_ROW];
  return (
    <Box title="Honest compile times">
      <div className="mt-5 overflow-hidden rounded-xl border border-white/10 bg-black/40">
        <div className="flex flex-wrap items-center border-b border-white/10 px-2">
          {KINDS.map((kind, i) => (
            <button
              aria-pressed={i === at}
              className={`relative h-11 px-3.5 text-[15px] transition-colors ${i === at ? 'text-white' : 'text-white/50 hover:text-white/80'}`}
              key={kind.name}
              onClick={() => setAt(i)}
              type="button"
            >
              {kind.name}
              <span
                className="absolute inset-x-3 bottom-0 h-0.5 rounded-full bg-clover transition-opacity"
                style={{ opacity: i === at ? 1 : 0 }}
              />
            </button>
          ))}
          <span className="ml-auto pr-2 font-mono text-[13px] text-white/45">
            30,000 lines, cold
          </span>
        </div>
        <div className="flex flex-col gap-3 p-4">
          {rows.map((c) => {
            const lit = c.name === 'BAML';
            const ratio = c.s < base ? base / c.s : c.s / base;
            return (
              <div
                className={`grid grid-cols-[104px_minmax(0,1fr)_60px] items-center gap-x-3 gap-y-1 sm:grid-cols-[150px_minmax(0,1fr)_70px_104px] sm:gap-y-3 ${lit ? 'rounded-lg bg-clover/10 -mx-2 px-2 py-1.5' : ''}`}
                key={c.name}
              >
                <CompilerName name={c.name} />
                <span className="block h-3 rounded-full bg-white/8 overflow-hidden">
                  <span
                    className={`block h-full rounded-full transition-[width] duration-300 ${lit ? 'bg-clover' : 'bg-white/45'}`}
                    style={{ width: `${(c.s / most) * 100}%` }}
                  />
                </span>
                <p
                  className={`text-right font-mono text-[13px] tabular-nums sm:text-[15px] ${lit ? 'text-clover' : 'text-white/70'}`}
                >
                  {c.s >= 1
                    ? `${c.s.toFixed(2)} s`
                    : `${Math.round(c.s * 1000)} ms`}
                </p>
                {/* On a phone the ratio takes its own line under the row, so it's never clipped. */}
                <p className="col-span-3 whitespace-nowrap text-right font-display text-[15px] font-medium tabular-nums text-head sm:col-span-1 sm:text-[17px]">
                  {lit ? (
                    <span className="text-[15px] font-normal text-clover">
                      baseline
                    </span>
                  ) : (
                    `${ratio.toFixed(1)}× ${c.s < base ? 'faster' : 'slower'}`
                  )}
                </p>
              </div>
            );
          })}
        </div>
      </div>
      <p className="mt-4 flex flex-wrap items-center gap-x-2 text-[18px] text-white">
        Not our proudest chart. We'll get there, we just need a bit more time.
        <svg
          aria-hidden="true"
          className="h-[22px] w-auto"
          fill="#fff"
          viewBox="0 0 356 422"
        >
          {LAMB_PATHS.map((d) => (
            <path d={d} key={d} />
          ))}
        </svg>
      </p>
      <p className="mt-3 text-[13px] text-white/45">
        Measured on Apple M2 Max, 64 GB.
      </p>
    </Box>
  );
}

// ---------- baml run ----------

// Real commands and output on an example project.
// The usual way: write a scratch script, run it, read the output, then delete it or forget to.
const RUN_WINS = [
  { big: '1 tool call', small: 'Instead of script, run, delete' },
  { big: 'LLM functions too', small: 'Try non-deterministic code' },
  { big: '0 scratch files', small: 'No left-over slop' },
];

const PLATFORMS = [
  { arch: 'arm64, x64', Icon: SiApple, name: 'macOS' },
  { arch: 'arm64, x64, musl', Icon: SiLinux, name: 'Linux' },
  { arch: 'arm64, x64', Icon: FaWindows, name: 'Windows' },
];

const RUNS: {
  name: string;
  pill?: string;
  steps: { bad?: boolean; command: string; out: string[] }[];
}[] = [
  {
    name: 'Run',
    steps: [
      {
        command: 'baml run apply_discount -- --total 8000 --coupon SAVE20',
        out: ['6400'],
      },
      {
        command: `baml run classify_sentiment -- --text "Charged twice and nobody replied."`,
        out: ['Negative'],
      },
    ],
  },
  {
    name: 'Flags',
    pill: 'Flags from your types',
    steps: [
      {
        command: 'baml run apply_discount -- --help',
        out: [
          'function apply_discount(total: int, coupon: string | null) -> int',
          '',
          '  --total <int>',
          '  --coupon <string | null>',
        ],
      },
    ],
  },
  {
    name: 'Bad input',
    pill: 'Checked before it runs',
    steps: [
      {
        bad: true,
        command: 'baml run apply_discount -- --total abc',
        out: [
          'error: invalid value for `--total`: expected integer, got `abc`',
        ],
      },
    ],
  },
  {
    name: 'Ship it',
    steps: [
      {
        command: 'baml pack classify_sentiment -o sentiment',
        out: ['Finished sentiment [classify_sentiment, aarch64-apple-darwin]'],
      },
      {
        command:
          'baml pack classify_sentiment --target x86_64-unknown-linux-gnu -o sentiment-linux',
        out: [
          'Finished sentiment-linux [classify_sentiment, x86_64-unknown-linux-gnu]',
        ],
      },
      {
        command: './sentiment --text "Charged twice and nobody replied."',
        out: ['"Negative"'],
      },
    ],
  },
];

function Run() {
  const [at, setAt] = useState(0);
  const cur = RUNS[at];
  return (
    <Box title="Every function is a CLI">
      <p className="mt-2 text-[18px] text-white/70">
        The fastest way to understand a function is to call it.
        <br />
        For AI functions, it's the only way.
      </p>
      <div className="mt-5 overflow-hidden rounded-xl border border-white/10 bg-black/40">
        <div className="flex flex-wrap items-center border-b border-white/10 px-2">
          {RUNS.map((item, i) => (
            <button
              aria-pressed={i === at}
              className={`relative h-11 px-3.5 text-[15px] transition-colors ${i === at ? 'text-white' : 'text-white/50 hover:text-white/80'}`}
              key={item.name}
              onClick={() => setAt(i)}
              type="button"
            >
              {item.name}
              <span
                className="absolute inset-x-3 bottom-0 h-0.5 rounded-full bg-clover transition-opacity"
                style={{ opacity: i === at ? 1 : 0 }}
              />
            </button>
          ))}
          {cur.pill && (
            <span className="ml-auto mr-2 rounded-full bg-clover px-3 py-1 text-[15px] font-semibold text-black">
              {cur.pill}
            </span>
          )}
        </div>
        <div className="overflow-x-auto p-4 font-mono text-[14px] leading-[1.6]">
          {cur.steps.map((step, i) => (
            <div className={i ? 'mt-4' : ''} key={step.command}>
              <p className="whitespace-pre text-white">
                <span className="text-white/45">$ </span>
                {step.command}
              </p>
              {step.out.map((line, k) => (
                <p
                  className="min-h-[1.6em] whitespace-pre"
                  // biome-ignore lint/suspicious/noArrayIndexKey: fixed output
                  key={k}
                  style={{
                    color: step.bad ? ERROR_RED : 'var(--color-clover)',
                  }}
                >
                  {line}
                </p>
              ))}
            </div>
          ))}
        </div>
        {cur.name === 'Run' && (
          <div className="grid sm:grid-cols-3 border-t border-white/10">
            {RUN_WINS.map((win, i) => (
              <div
                className={`px-4 py-2.5 ${i ? 'sm:border-l border-white/10' : ''}`}
                key={win.big}
              >
                <p className="font-display text-[21px] font-medium text-clover">
                  {win.big}
                </p>
                <p className="text-[14px] leading-[1.3] text-white/70">
                  {win.small}
                </p>
              </div>
            ))}
          </div>
        )}
        {cur.name === 'Ship it' && (
          <div className="grid sm:grid-cols-3 border-t border-white/10">
            {PLATFORMS.map(({ Icon, arch, name }, i) => (
              <div
                className={`flex items-center gap-3 px-4 py-3 ${i ? 'sm:border-l border-white/10' : ''}`}
                key={name}
              >
                <Icon
                  aria-hidden="true"
                  className="shrink-0 text-[26px] text-clover"
                />
                <div className="min-w-0">
                  <p className="text-[16px] font-medium text-white">{name}</p>
                  <p className="font-mono text-[12px] text-white/50">{arch}</p>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </Box>
  );
}

// ---------- the research behind it ----------

const POST = {
  href: '/blog/teaching-agents-to-write-maintainable-code',
  slug: 'teaching-agents-to-write-maintainable-code',
};

/** The section's closing claim, with the post that backs it: its before-and-after drawing, then the
 *  claim and a link to the post. */
function Research() {
  return (
    <a
      className="focus-ring group mt-8 grid max-w-[880px] items-center gap-6 rounded-xl border border-rule bg-raised p-3 transition-colors hover:border-white/25 md:grid-cols-[minmax(0,1.3fr)_minmax(0,1fr)]"
      href={POST.href}
    >
      <PostDrawing
        alt="An agent's chess move generator with a cognitive complexity of 50, beside the version written with the new skill, with a cognitive complexity of 2."
        className="rounded-lg"
        slug={POST.slug}
      />
      <div className="px-3 pb-3 md:px-2 md:pb-0">
        <p className="v4-body text-ink">
          There's no bad BAML in the training data to learn from. And we make
          sure the only BAML that gets checked in is great BAML.
        </p>
        <p className="mt-5 text-[15px] text-clover">
          <span className="text-muted">Blog:</span> Teaching Agents to Write
          Maintainable Code{' '}
          <span className="inline-block transition-transform group-hover:translate-x-1">
            →
          </span>
        </p>
        <p className="mt-1 text-[14px] text-muted">Avery Townsend</p>
      </div>
    </a>
  );
}

// ---------- the pull request comment ----------

const GH = {
  border: '#30363d',
  canvas: '#0d1117',
  muted: '#8b949e',
  panel: '#161b22',
};
const AXES = ['Readable', 'Few branches', 'Few paths', 'Flat', 'Few exits'];
// What each axis measures, shown on hover. The names are render.baml's for `baml codequality`'s metrics.
const DEFINE: Record<string, string> = {
  'Few branches': 'Cyclomatic complexity: how many decisions a function makes.',
  'Few exits': 'How many places a function can return or throw.',
  'Few paths':
    'NPath complexity: how many different routes there are through a function.',
  Flat: 'Nesting: how deep the ifs and loops go.',
  Readable: 'Cognitive complexity: how hard a function is to follow.',
};
// The same, by the metric names the comment's rows use.
const DEFINE_ROW: Record<string, string> = {
  cognitive: DEFINE.Readable,
  NPath: DEFINE['Few paths'],
  nesting: DEFINE.Flat,
};
// A made-up pull request, to show what the comment does for you: an agent removes a feature, the
// code gets worse anyway, the comment says so, and the agent's next push fixes it.
const FIRST = [0.31, 0.52, 0.22, 0.6, 0.8];
const SECOND = [1.35, 1.2, 1.3, 1.1, 1.1];
const TOP = 1.45;
const MAIN_COLOR = '#82B571';
const BRANCH_COLOR = '#CAAAF2';

type Change = {
  change: string;
  kind: 'better' | 'new' | 'removed' | 'worse';
  name: string;
};

const PUSHES: { changes: Change[]; summary: string }[] = [
  {
    changes: [
      {
        change: 'cognitive 12 → 41, nesting 3 → 6',
        kind: 'worse',
        name: 'route_ticket',
      },
      {
        change: 'cognitive 8 → 27, NPath 16 → 240',
        kind: 'worse',
        name: 'build_prompt',
      },
      { change: 'cognitive 5 → 19', kind: 'worse', name: 'escalate' },
      { change: 'cognitive 23', kind: 'new', name: 'legacy_router_shim' },
    ],
    summary:
      'A feature was removed, and the code got longer: 3 functions got worse. Lines 1,240 → 1,556.',
  },
  {
    changes: [
      {
        change: 'cognitive 12 → 6, nesting 3 → 2',
        kind: 'better',
        name: 'route_ticket',
      },
      {
        change: 'cognitive 8 → 3, NPath 16 → 4',
        kind: 'better',
        name: 'build_prompt',
      },
      { change: 'cognitive 5 → 3', kind: 'better', name: 'escalate' },
      { change: 'cognitive 34', kind: 'removed', name: 'keyword_rules' },
    ],
    summary: '3 functions got better, 1 removed. Lines 1,240 → 980.',
  },
];

const CHECKS = ['check', 'fmt', 'test', 'code health'];

// The agent measures the problem, fixes it, checks its work, and pushes.
const WORK: { key: string; node: ReactNode }[] = [
  {
    key: 'who',
    node: (
      <p className="flex items-center gap-2">
        <span className="size-3 shrink-0 animate-spin rounded-full border-2 border-[#58a6ff] border-t-transparent" />
        <b className="text-[#e6edf3]">your agent</b>
        <span style={{ color: GH.muted }}>is working on #482</span>
      </p>
    ),
  },
  ...(
    [
      // It measures first, so it knows what to fix.
      ['$', 'baml codequality', '✗ route_ticket cognitive 41', '#f85149'],
      ['▸', 'Edit', 'routing.baml', 'flatten route_ticket and escalate'],
      ['▸', 'Edit', 'prompts.baml', 'simplify build_prompt'],
      ['▸', 'Delete', 'legacy_router_shim.baml', ''],
      ['$', 'baml check', '✓ no errors', '#3fb950'],
      ['$', 'baml test', '✓ 14 passed', '#3fb950'],
      // And measures again before it pushes.
      ['$', 'baml codequality', '✓ route_ticket cognitive 6', '#3fb950'],
      ['$', 'git push', 'a3f91c2', '#58a6ff'],
    ] as const
  ).map((row, i) => {
    const shell = row[0] === '$';
    const gap = i === 0 || i === 1 || i === 4 ? 'mt-4' : 'mt-2';
    return {
      key: `${i}-${row[1]}`,
      node: (
        <p className={`${gap} flex items-baseline gap-2 font-mono text-[13px]`}>
          <span style={{ color: GH.muted }}>{row[0]}</span>
          {shell ? (
            <>
              <span className="shrink-0 whitespace-nowrap text-[#e6edf3] sm:w-[9.5em]">
                {row[1]}
              </span>
              <span className="min-w-0 truncate" style={{ color: row[3] }}>
                {row[2]}
              </span>
            </>
          ) : (
            <>
              <span className="w-[4.5em] shrink-0 text-[#e6edf3]">
                {row[1]}
              </span>
              <span className="whitespace-nowrap text-[#58a6ff]">{row[2]}</span>
              {/* The reason, where there's room for it. */}
              {row[3] && (
                <span
                  className="hidden min-w-0 truncate sm:inline"
                  style={{ color: GH.muted }}
                >
                  {row[3]}
                </span>
              )}
            </>
          )}
        </p>
      ),
    };
  }),
];

const KIND = {
  better: MAIN_COLOR,
  new: GH.muted,
  removed: GH.muted,
  worse: '#f85149',
};

function Radar({ now }: { now: number[] }) {
  const c = 200;
  const R = 150;
  const at = (i: number, r: number) => {
    const a = -Math.PI / 2 + (i * 2 * Math.PI) / AXES.length;
    return [c + Math.cos(a) * r, c + Math.sin(a) * r];
  };
  const shape = (values: number[]) =>
    values
      .map((v, i) =>
        at(i, (R * v) / TOP)
          .map((n) => n.toFixed(1))
          .join(','),
      )
      .join(' ');
  return (
    <svg
      aria-label="Code health on five measures"
      className="mx-auto block h-auto w-full max-w-[400px]"
      role="img"
      viewBox="-110 20 620 370"
    >
      {AXES.map((name, i) => {
        const [x, y] = at(i, R);
        const [lx, ly] = at(i, R + 18);
        return (
          <g className="cursor-help" key={name}>
            <title>{DEFINE[name]}</title>
            <line
              stroke="#828280"
              strokeOpacity={0.5}
              strokeWidth="0.5"
              x1={c}
              x2={x}
              y1={c}
              y2={y}
            />
            <text
              fill="#c9d1d9"
              fontSize="15"
              textAnchor={
                lx < c - 10 ? 'end' : lx > c + 10 ? 'start' : 'middle'
              }
              x={lx}
              y={ly + 5}
            >
              {name} {Math.round(now[i] * 100)}%
            </text>
          </g>
        );
      })}
      <polygon
        fill={MAIN_COLOR}
        fillOpacity={0.25}
        points={shape(AXES.map(() => 1))}
        stroke={MAIN_COLOR}
        strokeWidth="2.5"
      />
      <polygon
        fill={BRANCH_COLOR}
        fillOpacity={0.3}
        points={shape(now)}
        stroke={BRANCH_COLOR}
        strokeWidth="2.5"
      />
    </svg>
  );
}

function Swatch({ color, label }: { color: string; label: string }) {
  return (
    <span className="flex items-center gap-1.5 text-[14px]">
      <span className="size-3 rounded-[3px]" style={{ background: color }} />
      {label}
    </span>
  );
}

// The PR story's stages, as points along `Comment`'s progress. It steps through them on its own once
// in view; clicking a stage goes straight to it and stops the stepping.
// Each step sets the comment animation, the time it stays on screen, and its caption.
const STAGES: {
  at: number;
  label: string;
  ms: number;
  says: ReactNode;
}[] = [
  {
    at: 0.31,
    label: 'Bad code is flagged',
    ms: 4500,
    says: (
      <>
        Removing features should leave simpler code.{' '}
        <span style={{ color: '#f85149' }}>This code got worse.</span>
      </>
    ),
  },
  {
    at: 0.6,
    label: 'Your agent pushes a fix',
    ms: 4200,
    says: 'Your agent reads the comment and rewrites the code.',
  },
  {
    at: 1,
    label: 'Good code is merged',
    ms: 4500,
    says: (
      <span className="text-clover">The code is now simpler than main.</span>
    ),
  },
];

function Story() {
  const ref = useRef<HTMLDivElement>(null);
  const dot = useRef<HTMLSpanElement>(null);
  const seen = useInView(ref);
  // The loop runs only while the figure is on screen, and not at all for reduced motion, where the
  // steps change only when picked.
  const visible = useVisible(ref);
  const reduce = useReducedMotion();
  const [stage, setStage] = useState(0);
  // A click holds its step while the pointer stays on the figure; leaving lets the loop carry on.
  const [held, setHeld] = useState(false);
  const at = useRef(0);
  useEffect(() => {
    at.current = stage;
  }, [stage]);
  // Scrolled away: a hold left by a sideways exit lets go.
  useEffect(() => {
    if (!visible) setHeld(false);
  }, [visible]);
  useEffect(() => {
    const el = dot.current;
    if (!el) return;
    if (!visible || held || reduce) {
      el.style.opacity = '0';
      return;
    }
    // The dot runs left to right under the steps, crossing each in its step's time, and when it
    // reaches the end it starts again on the left with the first step: always forward.
    let start = performance.now();
    let frame = 0;
    const tick = (now: number) => {
      let t = (now - start) / STAGES[at.current].ms;
      if (t >= 1) {
        start = now;
        t = 0;
        setStage((n) => (n + 1) % STAGES.length);
        at.current = (at.current + 1) % STAGES.length;
      }
      const x = (at.current + t) / STAGES.length;
      // Fades out at the right end and back in at the left, so the wrap reads as a loop.
      const edge = Math.min(
        1,
        x * STAGES.length * 4,
        (1 - x) * STAGES.length * 4,
      );
      el.style.opacity = String(edge);
      el.style.transform = `translate3d(${x * 100}%, 0, 0)`;
      frame = requestAnimationFrame(tick);
    };
    frame = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(frame);
  }, [visible, held, reduce]);
  // `Comment` reads its progress through `sweep`, so each stage's point is mapped back through it.
  // Back to the first step it jumps, so the wrap doesn't play the whole story in reverse.
  const p = useTween(0.04 + STAGES[stage].at * 0.9, 1200, stage === 0);
  const steps = (
    <div className="mt-5">
      <ol className="grid grid-cols-3 gap-2">
        {STAGES.map((s, i) => {
          const on = i === stage;
          return (
            <li className="min-w-0" key={s.label}>
              <button
                aria-current={on ? 'step' : undefined}
                className="focus-ring flex w-full cursor-pointer items-start gap-2.5 rounded-lg py-1.5 pr-2 text-left transition-colors hover:bg-white/5"
                onClick={() => {
                  setHeld(true);
                  setStage(i);
                }}
                type="button"
              >
                <span
                  className={`grid size-7 shrink-0 place-items-center rounded-full border font-mono text-[13px] transition-colors ${on ? 'border-clover bg-clover text-black' : 'border-white/20 text-white/50'}`}
                >
                  {i + 1}
                </span>
                <span
                  className={`sr-only text-[14px] leading-snug transition-colors md:not-sr-only md:pt-1 ${on ? 'text-white' : 'text-white/50'}`}
                >
                  {s.label}
                </span>
              </button>
            </li>
          );
        })}
      </ol>
      {/* The track the dot runs along, under the steps. */}
      <div
        aria-hidden
        className="relative mt-2 h-px overflow-x-clip bg-white/15"
      >
        <span
          className="absolute inset-x-0 top-0 block will-change-[transform,opacity]"
          ref={dot}
          style={{ opacity: 0 }}
        >
          <span className="absolute left-0 top-0 size-[7px] -translate-x-1/2 -translate-y-1/2 rounded-full bg-clover" />
        </span>
      </div>
      {/* What the step means, right above the pull request. */}
      <div className="mt-4 grid text-[17px] leading-snug text-white md:text-[20px]">
        {STAGES.map((st, i) => (
          <p
            aria-hidden={i !== stage}
            className="[grid-area:1/1] transition-opacity duration-300"
            key={st.label}
            style={{ opacity: i === stage ? 1 : 0 }}
          >
            {st.says}
          </p>
        ))}
      </div>
    </div>
  );
  return (
    <div
      onPointerLeave={(e) => {
        if (leftVertically(e)) setHeld(false);
      }}
      ref={ref}
    >
      <Comment fixing={seen && stage === 1} p={p} steps={steps} />
    </div>
  );
}

function Comment({
  fixing,
  p,
  steps,
}: {
  // The agent works through the fix before it pushes.
  fixing: boolean;
  p: number;
  steps: ReactNode;
}) {
  const s = sweep(p);
  const [working, setWorking] = useState(false);
  useEffect(() => {
    if (!fixing) {
      setWorking(false);
      return;
    }
    const frame = requestAnimationFrame(() => setWorking(true));
    return () => cancelAnimationFrame(frame);
  }, [fixing]);
  // Start with the flagged metrics, then animate to the fixed metrics after the push.
  const commit = ramp(s, 0.5, 0.58);
  const fix = ramp(s, 0.6, 0.8);
  const pushed = fix > 0.5;
  const push = PUSHES[pushed ? 1 : 0];
  const now = FIRST.map((v, i) => v + (SECOND[i] - v) * fix);
  return (
    // A box like the others in this section, so the story reads as one of them.
    <div className="mt-4 md:mt-6 rounded-xl border border-rule bg-raised p-5">
      <p className="v4-h3 text-head" style={PIXEL}>
        Our GitHub Action measures code quality on every pull request.
      </p>
      <p className="mt-2 text-[18px] text-white/70">
        No model in the loop, so the token bill is $0.
      </p>
      {/* CI/CD by default: the action that posts the comment, in one line. */}
      <p className="mt-4 flex flex-wrap items-baseline gap-x-3 gap-y-1 font-mono text-[14px]">
        <span className="rounded-md bg-black/50 px-2.5 py-1 text-white">
          <span className="text-white/50">- uses: </span>
          <span className="text-clover">BoundaryML/baml-action@v1</span>
        </span>
        <span className="text-white/50">check, fmt, test and codequality</span>
      </p>
      {steps}
      <div
        className="mt-5 rounded-md border text-[14px] text-[#e6edf3]"
        style={{ background: GH.canvas, borderColor: GH.border }}
      >
        <div className="border-b px-4 py-3" style={{ borderColor: GH.border }}>
          <p className="text-[18px] font-semibold">
            Remove the legacy keyword router{' '}
            <span style={{ color: GH.muted }}>#482</span>
          </p>
          <div className="mt-2 flex flex-wrap items-center gap-x-4 gap-y-1 text-[13px]">
            {CHECKS.map((name) => {
              const health = name === 'code health';
              const warn = health && !pushed;
              const color = warn ? '#d29922' : '#3fb950';
              return (
                <span
                  className={`flex items-center gap-1.5 ${health ? 'rounded-full border px-2.5 py-0.5 font-semibold' : ''}`}
                  key={name}
                  style={
                    health
                      ? {
                          background: `${color}26`,
                          borderColor: `${color}80`,
                          color,
                        }
                      : undefined
                  }
                >
                  <span style={{ color }}>{warn ? '⚠' : '✓'}</span>
                  <span style={{ color: health ? color : GH.muted }}>
                    BAML / {name}
                  </span>
                </span>
              );
            })}
          </div>
        </div>
        <div
          className="flex h-9 items-center gap-2 border-b px-4 font-mono text-[13px]"
          style={{
            borderColor: GH.border,
            opacity: commit,
            transform: `translateY(${(1 - commit) * -8}px)`,
          }}
        >
          <span className="size-2.5 rounded-full border-2 border-[#3fb950]" />
          <span style={{ color: '#58a6ff' }}>a3f91c2</span>
          <span className="min-w-0 truncate">
            Simplify routing now the agent picks the team
          </span>
          <span
            className="ml-auto hidden shrink-0 sm:inline"
            style={{ color: GH.muted }}
          >
            your agent
          </span>
        </div>
        <div className="relative">
          {/* The agent works over the comment it is answering. */}
          <div
            aria-hidden={!fixing}
            className="absolute inset-x-0 -top-9 bottom-0 z-10 px-4 pt-4 transition-opacity duration-300 will-change-[opacity]"
            style={{
              background: GH.canvas,
              opacity: fixing ? 1 : 0,
              pointerEvents: fixing ? 'auto' : 'none',
            }}
          >
            {WORK.map((row, i) => (
              <div
                className="transition-[opacity,transform] duration-300 ease-out"
                key={row.key}
                style={{
                  opacity: working ? 1 : 0,
                  transform: `translate3d(0, ${working ? 0 : 8}px, 0)`,
                  transitionDelay: `${working ? 150 + i * 340 : 0}ms`,
                }}
              >
                {row.node}
              </div>
            ))}
          </div>
          <div
            className="flex items-center gap-2 border-b px-4 py-2"
            style={{ background: GH.panel, borderColor: GH.border }}
          >
            <SiGithub aria-hidden="true" className="shrink-0 text-[18px]" />
            <b>github-actions</b>
            <span
              className="rounded-full border px-1.5 text-[11px]"
              style={{ borderColor: GH.border, color: GH.muted }}
            >
              bot
            </span>
            <span style={{ color: GH.muted }}>
              {pushed
                ? 'updated after your agent pushed a fix'
                : "commented on your agent's push"}
            </span>
          </div>
          <div className="p-4">
            <p className="font-semibold text-[15px]">{push.summary}</p>
            <div className="mt-2 grid md:grid-cols-[minmax(0,0.9fr)_minmax(0,1.1fr)] gap-x-6 gap-y-3 items-center">
              <div>
                <div className="flex justify-center gap-5">
                  <Swatch color={MAIN_COLOR} label="main" />
                  <Swatch color={BRANCH_COLOR} label="your branch" />
                </div>
                <Radar now={now} />
              </div>
              <div
                className="overflow-hidden rounded-md border"
                style={{ borderColor: GH.border }}
              >
                {push.changes.map((c, i) => (
                  <div
                    className="flex items-baseline gap-3 px-3 py-2"
                    key={c.name}
                    style={{
                      borderTop: i ? `1px solid ${GH.border}` : undefined,
                    }}
                  >
                    <span
                      className="w-[58px] shrink-0 text-[13px] font-semibold"
                      style={{ color: KIND[c.kind] }}
                    >
                      {c.kind}
                    </span>
                    <span className="min-w-0">
                      <code className="block truncate font-mono text-[13px] text-[#58a6ff]">
                        {c.name}
                      </code>
                      <span className="text-[13px]" style={{ color: GH.muted }}>
                        {c.change.split(', ').map((part, k) => {
                          const metric = part.split(' ')[0];
                          return (
                            <span key={part}>
                              {k ? ', ' : ''}
                              <span
                                className="cursor-help underline decoration-dotted decoration-white/30 underline-offset-2"
                                title={DEFINE_ROW[metric]}
                              >
                                {metric}
                              </span>
                              {part.slice(metric.length)}
                            </span>
                          );
                        })}
                      </span>
                    </span>
                  </div>
                ))}
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

// ---------- the section ----------

export default function Review() {
  return (
    <section
      {...section('review')}
      aria-labelledby="review"
      className="border-t border-rule"
    >
      <div className="v3-wrap mx-auto px-(--frame-pad) py-20 md:py-28">
        <h2 className="v4-h2 text-head max-w-[22em]" id="review">
          Can agents write high-quality <Hyper>BAML</Hyper>?
        </h2>
        <p className="mt-4 max-w-[40rem] v4-body text-ink-2">
          BAML isn't even close to a mainstream language, so we overcompensate
          by building amazing <span className="text-clover">backpressure</span>{' '}
          into the toolchain.
        </p>
        <div className="mt-10">
          <Errors />
        </div>
        <div className="mt-16">
          <Run />
        </div>
        <div className="mt-16">
          <Story />
        </div>
        <div className="mt-16">
          <Speed />
        </div>
        <div className="mt-16 md:mt-20">
          <p className="v4-h2 text-head max-w-[22em]">
            So yes, agents can write high-quality <Hyper>BAML</Hyper>. By
            design.
          </p>
        </div>
        <Research />
      </div>
    </section>
  );
}
