'use client';

import { useEffect, useRef, useState, type ReactNode } from 'react';

// Line-prefix → CSS class, matching the markers emitted by
// bench_core.transcript.render_terminal_transcript (Claude-Code-terminal style).
const MARKERS: Array<[string, string]> = [
  ['⏺ ', 't-asst'],
  ['✻', 't-think'],
  ['  ⎿', 't-result'],
  ['> ', 't-user'],
];

// newer transcripts stamp marker lines with a leading [HH:MM:SS]
const TS_PREFIX = /^\[(\d{2}:\d{2}:\d{2})\] /;

/** Splits an optional [HH:MM:SS] stamp off a transcript line. */
function splitStamp(line: string): { ts: string | null; rest: string } {
  const m = line.match(TS_PREFIX);
  return m ? { ts: m[1], rest: line.slice(m[0].length) } : { ts: null, rest: line };
}

// `⏺ ToolName(args…)` — bold the tool name like Claude Code does.
const TOOL_CALL = /^⏺ ([A-Z]\w*)(\(.*)$/;
// shell-ish separators that put the next word in command position
const SHELL_SPLIT = /([;()`]|&&?|\|\|?|\$\()/;
// a chunk whose first word is `baml` is an actual CLI invocation
const BAML_CMD = /^\s*baml(\s|$)/;

/** True when shell text executes the baml CLI in command position
 * (start of line/subshell or after && || | ; ` $( ), not a mere mention. */
function textInvokesBaml(text: string): boolean {
  return text.split(SHELL_SPLIT).some((c) => BAML_CMD.test(c));
}

// readable BAML purple on the dark terminal background
const BAML_PURPLE = 'text-[#c4b5fd]';

/** Renders shell text with `baml …` command segments in BAML purple.
 * SHELL_SPLIT keeps separators (incl. "(") as their own tokens, so
 * command-position chunks can be tested and tinted independently. */
function bamlArgs(text: string): ReactNode {
  return text.split(SHELL_SPLIT).map((p, i) =>
    BAML_CMD.test(p) ? (
      <span key={i} className={`font-semibold ${BAML_PURPLE}`}>
        {p}
      </span>
    ) : (
      <span key={i}>{p}</span>
    ),
  );
}

/**
 * Renders one transcript line with its Claude-Code marker styling.
 * @param line - the raw line
 * @param baml - this line sits in a Bash block that invokes the baml CLI:
 *   the tool line gets the 🐑 marker and command-position `baml …` segments
 *   (including on continuation lines of multi-line commands) run purple
 */
function termLine(raw: string, baml = false): ReactNode {
  // newer transcripts stamp marker lines; render the stamp dimmed
  const { ts, rest: line } = splitStamp(raw);
  const stamp = ts ? (
    <span className="mr-1.5 text-[10.5px] text-[#5c5c58]">{ts}</span>
  ) : null;
  const tool = line.match(TOOL_CALL);
  if (tool) {
    return (
      <>
        {stamp}
        {baml ? (
          <span title="BAML CLI call">🐑</span>
        ) : (
          <span className="t-dot">⏺</span>
        )}{' '}
        <span className="font-semibold text-[#f0eee6]">{tool[1]}</span>
        <span className="text-[#9a978e]">
          {baml ? bamlArgs(tool[2]) : tool[2]}
        </span>
      </>
    );
  }
  if (line.startsWith('⏺ ')) {
    return (
      <>
        {stamp}
        <span className="t-dot">⏺</span>
        {line.slice(1)}
      </>
    );
  }
  if (line.startsWith('> ')) {
    return (
      <>
        {stamp}
        <span className="text-[#4a7a4a]">❯</span>
        {line.slice(1)}
      </>
    );
  }
  if (stamp) {
    return (
      <>
        {stamp}
        {line}
      </>
    );
  }
  // continuation line of a multi-line baml-invoking command (e.g. after a
  // heredoc): highlight any command-position `baml …` segment
  if (baml && line) return bamlArgs(line);
  return line || ' ';
}

// Blocks longer than this collapse to the first HEAD lines + an expander.
const BLOCK_LIMIT = 10;
const BLOCK_HEAD = 6;

type TermEntry = { line: string; mode: string };

/**
 * One marker-to-marker section of the transcript. Long sections render
 * trimmed with a Claude-Code-style "… +N lines" expander toggling in place.
 */
function TermBlock({
  entries,
  onToggle,
}: {
  entries: TermEntry[];
  onToggle?: () => void;
}) {
  const [open, setOpen] = useState(false);
  // ignore trailing blank lines when deciding whether to trim
  let end = entries.length;
  while (end > 0 && entries[end - 1].line === '') end--;
  const long = end > BLOCK_LIMIT;
  const shown = long && !open ? entries.slice(0, BLOCK_HEAD) : entries;
  // a Bash tool block invokes baml if ANY of its lines (incl. continuation
  // lines of multi-line commands) has `baml` in command position
  const toolMatch = splitStamp(entries[0]?.line ?? '').rest.match(TOOL_CALL);
  const isBaml =
    toolMatch?.[1] === 'Bash' &&
    entries.some((e, i) =>
      textInvokesBaml(i === 0 ? toolMatch[2].replace(/^\(/, '') : e.line),
    );
  return (
    <>
      {shown.map((e, i) => (
        <div key={i} className={e.mode || undefined}>
          {termLine(e.line, isBaml)}
        </div>
      ))}
      {long ? (
        <div>
          <button
            className="cursor-pointer border-0 bg-transparent p-0 font-mono text-[12px] italic text-[#8a8a86] hover:text-[#d7d3c8]"
            onClick={() => {
              setOpen((v) => !v);
              onToggle?.();
            }}
          >
            {open
              ? '  ⎿ collapse'
              : `  … +${end - BLOCK_HEAD} lines (click to expand)`}
          </button>
        </div>
      ) : null}
    </>
  );
}

/**
 * Renders the raw transcript as a colorized terminal window (macOS chrome +
 * Claude-Code-style line markers): lines are grouped into same-mode blocks
 * (assistant / thinking / tool result / user), and long blocks collapse
 * behind a "… +N lines" expander. Note: collapsed lines are out of the DOM,
 * so Ctrl-F only searches what's expanded.
 * @param text - the rendered terminal transcript
 */
// timeline event kinds → tick color / height (baml + error stand out)
const TICK: Record<string, string> = {
  baml: 'bg-[#c4b5fd] h-3.5',
  tool: 'bg-[#3fb950] h-2.5',
  user: 'bg-[#6cb6ff] h-3.5',
  think: 'bg-[#6e7681] h-1.5',
  error: 'bg-[#ff5f57] h-3.5',
};

type TermEvent = { block: number; at: number; kind: string; label: string };

/** Classifies a block into a timeline event (or null for plain text). */
function classifyBlock(entries: TermEntry[], block: number, at: number): TermEvent | null {
  const { ts, rest: first } = splitStamp(entries[0]?.line ?? '');
  const when = ts ? `${ts} · ` : '';
  const tool = first.match(TOOL_CALL);
  if (tool) {
    const isBaml =
      tool[1] === 'Bash' &&
      entries.some((e, i) =>
        textInvokesBaml(i === 0 ? tool[2].replace(/^\(/, '') : e.line),
      );
    return {
      block,
      at,
      kind: isBaml ? 'baml' : 'tool',
      label: `${when}${isBaml ? '🐑 ' : ''}${tool[1]}${tool[2].slice(0, 110)}`,
    };
  }
  if (first.startsWith('> '))
    return { block, at, kind: 'user', label: `${when}${first.slice(0, 110)}` };
  if (first.startsWith('✻'))
    return { block, at, kind: 'think', label: `${when}thinking` };
  if (first.startsWith('  ⎿') && /\[error\]/i.test(entries.map((e) => e.line).join('\n')))
    return {
      block,
      at,
      kind: 'error',
      label: `${when}${first.slice(4, 64).trim() || 'error'}`,
    };
  return null;
}

function TerminalView({ text }: { text: string }) {
  const termRef = useRef<HTMLDivElement>(null);
  // measured fraction (rendered offsetTop / scrollHeight) per block index, so
  // ticks, the viewport band, and click-jumps share one pixel coordinate space
  const [pos, setPos] = useState<Record<number, number>>({});
  const [view, setView] = useState({ left: 0, width: 1 });
  const [hover, setHover] = useState<TermEvent | null>(null);

  const measure = () => {
    const el = termRef.current;
    if (!el) return;
    const total = Math.max(1, el.scrollHeight);
    const next: Record<number, number> = {};
    el.querySelectorAll<HTMLElement>('[data-block]').forEach((b) => {
      next[Number(b.dataset.block)] = b.offsetTop / total;
    });
    setPos(next);
    setView({
      left: el.scrollTop / total,
      width: el.clientHeight / total,
    });
  };
  useEffect(() => {
    measure();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [text]);
  const lines = text.split('\n');
  // sections run from one marker line to the next (blank lines stay inside),
  // so a 80-line tool result or thinking stretch trims as one unit
  const blocks: TermEntry[][] = [];
  const blockStart: number[] = [];
  let mode = '';
  lines.forEach((line, n) => {
    // marker detection ignores the optional [HH:MM:SS] stamp prefix
    const bare = splitStamp(line).rest;
    if (line === '') {
      mode = '';
    } else {
      const hit = MARKERS.find(([m]) => bare.startsWith(m));
      if (hit) mode = hit[1];
    }
    const isMarker = MARKERS.some(([m]) => bare.startsWith(m));
    if (blocks.length === 0 || isMarker) {
      blocks.push([]);
      blockStart.push(n);
    }
    blocks[blocks.length - 1].push({ line, mode });
  });
  const events = blocks
    .map((b, i) => classifyBlock(b, i, blockStart[i] / Math.max(1, lines.length)))
    .filter((e): e is TermEvent => e !== null);

  // click a tick → smooth-scroll the terminal to that block
  const jump = (block: number) => {
    const el = termRef.current;
    const target = el?.querySelector<HTMLElement>(`[data-block="${block}"]`);
    if (el && target) {
      // the terminal is position:relative, so offsetTop is container-relative
      el.scrollTo({ top: target.offsetTop - 8, behavior: 'smooth' });
    }
  };
  // scrolling the terminal slides the viewport band along the timeline
  const onScroll = () => {
    const el = termRef.current;
    if (!el) return;
    const total = Math.max(1, el.scrollHeight);
    setView({ left: el.scrollTop / total, width: el.clientHeight / total });
  };

  return (
    <div className="overflow-hidden rounded-lg border border-[#3a3a38] shadow-[0_8px_30px_rgba(0,0,0,0.25)]">
      {/* title bar: traffic lights + session label */}
      <div className="flex items-center gap-2 bg-[#2a2a28] px-3.5 py-2" aria-hidden>
        <span className="size-3 rounded-full bg-[#ff5f57]" />
        <span className="size-3 rounded-full bg-[#febc2e]" />
        <span className="size-3 rounded-full bg-[#28c840]" />
        <span className="ml-2 font-mono text-[11px] text-[#8a8a86]">
          claude — agent transcript
        </span>
        <span className="ml-auto font-mono text-[11px] text-[#5c5c58]">
          {lines.length} lines
        </span>
      </div>
      {/* event timeline: every tool call / user turn / error over the session */}
      <div className="relative border-b border-[#3a3a38] bg-[#222220] px-3.5 py-1.5">
        <div className="relative h-5">
          {/* baseline */}
          <div className="absolute top-1/2 right-0 left-0 h-px bg-[#3a3a38]" />
          {events.map((e, i) => (
            <button
              key={i}
              onClick={() => jump(e.block)}
              onMouseEnter={() => setHover(e)}
              onMouseLeave={() => setHover(null)}
              onFocus={() => setHover(e)}
              onBlur={() => setHover(null)}
              aria-label={`jump to: ${e.label}`}
              className={`absolute top-1/2 w-[3px] -translate-x-1/2 -translate-y-1/2 cursor-pointer rounded-full border-0 p-0 ${TICK[e.kind]} hover:scale-y-150 hover:brightness-125 focus-visible:scale-y-150 focus-visible:brightness-125`}
              style={{ left: `${(pos[e.block] ?? e.at) * 100}%` }}
            />
          ))}
          {/* viewport band: the slice of the session currently on screen */}
          <div
            className="pointer-events-none absolute top-0 h-full rounded border border-[#f0eee6]/50 bg-[#f0eee6]/10"
            style={{
              left: `${view.left * 100}%`,
              width: `${Math.max(0.8, view.width * 100)}%`,
            }}
          />
        </div>
        {/* hover preview of the command behind a tick */}
        {hover ? (
          <div
            className="pointer-events-none absolute top-full z-10 mt-1 max-w-[70%] -translate-x-1/2 truncate rounded border border-[#4a4a46] bg-[#333330] px-2 py-1 font-mono text-[11px] text-[#d7d3c8] shadow-[0_4px_12px_rgba(0,0,0,0.4)]"
            style={{
              left: `${Math.min(80, Math.max(20, (pos[hover.block] ?? hover.at) * 100))}%`,
            }}
          >
            {hover.label}
          </div>
        ) : null}
      </div>
      <div
        ref={termRef}
        onScroll={onScroll}
        className="terminal relative !rounded-none"
      >
        {blocks.map((b, i) => (
          <div key={i} data-block={i}>
            {/* expanding/collapsing reflows the content → re-measure ticks */}
            <TermBlock
              entries={b}
              onToggle={() => requestAnimationFrame(measure)}
            />
          </div>
        ))}
        {/* resting prompt with a blinking block cursor */}
        <div className="mt-1">
          <span className="text-[#4a7a4a]">❯</span>{' '}
          <span className="t-cursor">▋</span>
        </div>
      </div>
    </div>
  );
}

// Toggle between the structured per-call turn log and the raw terminal transcript.
// The raw view is the full conversation rendered Claude-Code-style, so the
// browser's native Ctrl-F searches everything; the structured view stays default.
/**
 * Client component that switches the transcript section between the structured
 * turn log and the raw, Ctrl-F-able terminal transcript. When no raw transcript
 * is available it renders the structured view alone (no toggle).
 * @param structured - the server-rendered structured turn blocks
 * @param raw - the raw terminal transcript, or null when unavailable
 * @returns the toggle (when raw exists) and the active view
 */
export default function TranscriptTabs({
  structured,
  raw,
}: {
  structured: ReactNode;
  raw: string | null;
}) {
  // the terminal is the primary view; structured stays as the secondary toggle
  const [mode, setMode] = useState<'structured' | 'raw'>('raw');
  // deep links (?call=N from CallScroller, #call-N from evidence cards) target
  // the structured view's anchors — flip to it and land on the right call
  useEffect(() => {
    const hash = window.location.hash.match(/^#call-(\d+)$/);
    const hasQuery = new URLSearchParams(window.location.search).has('call');
    if (!hash && !hasQuery) return;
    setMode('structured');
    if (hash) {
      const raf = requestAnimationFrame(() => {
        const el = document.getElementById(`call-${hash[1]}`);
        if (el instanceof HTMLDetailsElement) {
          el.open = true;
          el.scrollIntoView({ behavior: 'smooth', block: 'start' });
        }
      });
      return () => cancelAnimationFrame(raf);
    }
  }, []);
  if (!raw) return <>{structured}</>;

  const seg = (on: boolean) =>
    `cursor-pointer rounded border bg-transparent px-2.5 py-0.5 text-[13px] ${
      on
        ? 'border-foreground bg-muted text-foreground'
        : 'border-border text-muted-foreground'
    }`;

  return (
    <div>
      <div className="mb-3 inline-flex gap-1" role="tablist">
        <button className={seg(mode === 'raw')} onClick={() => setMode('raw')}>
          terminal
        </button>
        <button
          className={seg(mode === 'structured')}
          onClick={() => setMode('structured')}
        >
          structured
        </button>
      </div>
      {mode === 'structured' ? structured : <TerminalView text={raw} />}
    </div>
  );
}
