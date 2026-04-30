'use client';

import {
  type FormEvent,
  type KeyboardEvent,
  useEffect,
  useRef,
  useState,
} from 'react';

type Line = { kind: 'cmd' | 'out' | 'err' | 'info'; text: string };

const FILE_NAME = 'main.baml';
const RUN_CMD = `baml-cli run ${FILE_NAME}`;
const HINT = `this isn't actually a real terminal! type \`${RUN_CMD}\` to run the code above and see the result.`;

const PROMPT_USER = 'baml';
const PROMPT_HOST = 'site';

export function PlaygroundTerminal() {
  const [history, setHistory] = useState<Line[]>(() => [
    {
      kind: 'info',
      text: `try \`${RUN_CMD}\` to run the file above.`,
    },
  ]);
  const [draft, setDraft] = useState(RUN_CMD);
  const [pastCommands, setPastCommands] = useState<string[]>([]);
  const [historyCursor, setHistoryCursor] = useState<number | null>(null);
  const [running, setRunning] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const scrollRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const el = scrollRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [history]);

  const append = (lines: Line[]) => setHistory((prev) => [...prev, ...lines]);

  const runPlayground = () => {
    setRunning(true);
    append([{ kind: 'out', text: `▸ running ${FILE_NAME}…` }]);
    if (typeof window !== 'undefined') {
      window.dispatchEvent(new CustomEvent('baml-playground-run'));
    }
    window.setTimeout(() => {
      append([
        {
          kind: 'out',
          text: '  ✓ dispatched. results render in the playground above.',
        },
      ]);
      setRunning(false);
    }, 500);
  };

  const handleCommand = (raw: string) => {
    const trimmed = raw.trim();
    append([
      {
        kind: 'cmd',
        text: `${PROMPT_USER}@${PROMPT_HOST} % ${trimmed}`,
      },
    ]);
    if (!trimmed) return;

    setPastCommands((prev) => [...prev, trimmed]);
    setHistoryCursor(null);

    const normalized = trimmed.replace(/\s+/g, ' ').toLowerCase();
    if (normalized === RUN_CMD.toLowerCase()) {
      runPlayground();
      return;
    }

    append([{ kind: 'err', text: HINT }]);
  };

  const handleSubmit = (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault();
    const value = draft;
    setDraft('');
    handleCommand(value);
  };

  const handleKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'ArrowUp') {
      e.preventDefault();
      if (pastCommands.length === 0) return;
      const next =
        historyCursor === null
          ? pastCommands.length - 1
          : Math.max(0, historyCursor - 1);
      setHistoryCursor(next);
      setDraft(pastCommands[next] ?? '');
    } else if (e.key === 'ArrowDown') {
      e.preventDefault();
      if (historyCursor === null) return;
      const next = historyCursor + 1;
      if (next >= pastCommands.length) {
        setHistoryCursor(null);
        setDraft('');
      } else {
        setHistoryCursor(next);
        setDraft(pastCommands[next] ?? '');
      }
    }
  };

  return (
    <div
      onClick={() => inputRef.current?.focus()}
      style={{
        background: '#0E0B14',
        color: '#E8E3DB',
        cursor: 'text',
        display: 'flex',
        flexDirection: 'column',
        fontFamily:
          '"IBM Plex Mono", ui-monospace, SFMono-Regular, Menlo, Consolas, monospace',
        fontSize: 12,
        height: '100%',
        lineHeight: 1.55,
      }}
    >
      <div
        style={{
          alignItems: 'center',
          background: '#15101F',
          borderBottom: '1px solid rgba(255,255,255,0.08)',
          display: 'flex',
          gap: 8,
          padding: '8px 12px',
        }}
      >
        <span style={dotStyle('#FF5F57')} />
        <span style={dotStyle('#FEBC2E')} />
        <span style={dotStyle('#28C840')} />
        <span
          style={{
            color: 'rgba(232,227,219,0.55)',
            fontSize: 11,
            letterSpacing: '0.04em',
            marginLeft: 8,
          }}
        >
          baml / zsh
        </span>
      </div>

      <div
        ref={scrollRef}
        style={{
          flex: 1,
          minHeight: 0,
          overflow: 'auto',
          padding: '10px 14px 4px',
        }}
      >
        {history.map((line, i) => (
          <div
            // biome-ignore lint/suspicious/noArrayIndexKey: history is append-only
            key={i}
            style={{
              color:
                line.kind === 'err'
                  ? 'rgba(232,227,219,0.72)'
                  : line.kind === 'cmd'
                    ? '#E8E3DB'
                    : line.kind === 'info'
                      ? 'rgba(232,227,219,0.65)'
                      : '#C9C3FF',
              whiteSpace: 'pre-wrap',
            }}
          >
            {line.text}
          </div>
        ))}

        <form
          onSubmit={handleSubmit}
          style={{ alignItems: 'center', display: 'flex', marginTop: 4 }}
        >
          <span style={{ color: '#A78BFA', flexShrink: 0 }}>
            {PROMPT_USER}@{PROMPT_HOST}
          </span>
          <span style={{ color: '#E8E3DB', margin: '0 6px' }}>%</span>
          <input
            aria-label="terminal input"
            autoCapitalize="off"
            autoComplete="off"
            autoCorrect="off"
            disabled={running}
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={handleKeyDown}
            ref={inputRef}
            spellCheck={false}
            style={{
              background: 'transparent',
              border: 'none',
              caretColor: '#A78BFA',
              color: '#E8E3DB',
              flex: 1,
              fontFamily: 'inherit',
              fontSize: 'inherit',
              outline: 'none',
            }}
            value={draft}
          />
        </form>
      </div>
    </div>
  );
}

function dotStyle(color: string): React.CSSProperties {
  return {
    background: color,
    borderRadius: '50%',
    display: 'inline-block',
    height: 10,
    width: 10,
  };
}
