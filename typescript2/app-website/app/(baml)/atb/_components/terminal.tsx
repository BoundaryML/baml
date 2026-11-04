"use client";

// The raw transcript rendered as a colorized terminal window, ported from
// the original agent-tries-baml ui: macOS chrome, an event timeline above
// the scrollback (one tick per tool call / user turn / error, click to
// jump, the band tracks the viewport), Claude-Code line markers, and long
// blocks collapsed behind a "+N lines" expander.

import { useEffect, useRef, useState, type ReactNode } from "react";

// Line-prefix to CSS class, matching the markers emitted by
// bench_core.transcript.render_terminal_transcript.
const MARKERS: Array<[string, string]> = [
  ["⏺ ", "t-asst"],
  ["✻", "t-think"],
  ["  ⎿", "t-result"],
  ["> ", "t-user"],
];

// newer transcripts stamp marker lines with a leading [HH:MM:SS]
const TS_PREFIX = /^\[(\d{2}:\d{2}:\d{2})\] /;

function splitStamp(line: string): { ts: string | null; rest: string } {
  const m = line.match(TS_PREFIX);
  return m
    ? { ts: m[1], rest: line.slice(m[0].length) }
    : { ts: null, rest: line };
}

// `⏺ ToolName(args…)`: bold the tool name like Claude Code does.
const TOOL_CALL = /^⏺ ([A-Z]\w*)(\(.*)$/;
// shell-ish separators that put the next word in command position
const SHELL_SPLIT = /([;()`]|&&?|\|\|?|\$\()/;
// a chunk whose first word is `baml` is an actual CLI invocation
const BAML_CMD = /^\s*baml(\s|$)/;

function textInvokesBaml(text: string): boolean {
  return text.split(SHELL_SPLIT).some((c) => BAML_CMD.test(c));
}

// readable BAML purple on the dark terminal background
const BAML_PURPLE = "text-[#c4b5fd]";

/** Renders shell text with the `baml` command word tinted purple (brand
 * accent on the invocation itself; arguments stay neutral so purple never
 * competes with the error red). */
function bamlArgs(text: string): ReactNode {
  return text.split(SHELL_SPLIT).map((p, i) => {
    const m = p.match(/^(\s*)(baml)(\s|$)([\s\S]*)$/);
    return m ? (
      <span key={i}>
        {m[1]}
        <span className={`font-semibold ${BAML_PURPLE}`}>{m[2]}</span>
        {m[3]}
        {m[4]}
      </span>
    ) : (
      <span key={i}>{p}</span>
    );
  });
}

function termLine(raw: string, baml = false): ReactNode {
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
          <span title="BAML CLI call" className={BAML_PURPLE}>
            ⏺
          </span>
        ) : (
          <span className="t-dot">⏺</span>
        )}{" "}
        <span className="font-semibold text-[#f0eee6]">{tool[1]}</span>
        <span className="text-[#9a978e]">
          {baml ? bamlArgs(tool[2]) : tool[2]}
        </span>
      </>
    );
  }
  if (line.startsWith("⏺ ")) {
    return (
      <>
        {stamp}
        <span className="t-dot">⏺</span>
        {line.slice(1)}
      </>
    );
  }
  if (line.startsWith("> ")) {
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
  // continuation line of a multi-line baml-invoking command
  if (baml && line) return bamlArgs(line);
  return line || " ";
}

// Blocks longer than this collapse to the first HEAD lines + an expander.
const BLOCK_LIMIT = 10;
const BLOCK_HEAD = 6;

type TermEntry = { line: string; mode: string };

function TermBlock({
  entries,
  onToggle,
}: {
  entries: TermEntry[];
  onToggle?: () => void;
}) {
  const [open, setOpen] = useState(false);
  let end = entries.length;
  while (end > 0 && entries[end - 1].line === "") end--;
  const long = end > BLOCK_LIMIT;
  const shown = long && !open ? entries.slice(0, BLOCK_HEAD) : entries;
  const toolMatch = splitStamp(entries[0]?.line ?? "").rest.match(TOOL_CALL);
  const isBaml =
    toolMatch?.[1] === "Bash" &&
    entries.some((e, i) =>
      textInvokesBaml(i === 0 ? toolMatch[2].replace(/^\(/, "") : e.line),
    );
  // A result block that reports an error renders red end-to-end; errors must
  // never read as ordinary grey output (or worse, get outshone by brand purple).
  const isError =
    entries[0]?.mode === "t-result" &&
    /\[error\]/i.test(entries.map((e) => e.line).join("\n"));
  return (
    <>
      {shown.map((e, i) => (
        <div
          key={i}
          className={
            (isError && e.mode === "t-result" ? "t-error" : e.mode) || undefined
          }
        >
          {termLine(e.line, isBaml)}
        </div>
      ))}
      {long ? (
        <div>
          <button
            className={`cursor-pointer border-0 bg-transparent p-0 font-atb-mono text-[12px] italic ${
              isError
                ? "text-[#c96a60] hover:text-[#ff6b63]"
                : "text-[#8a8a86] hover:text-[#d7d3c8]"
            }`}
            onClick={() => {
              setOpen((v) => !v);
              onToggle?.();
            }}
          >
            {open
              ? "  ⎿ collapse"
              : `  … +${end - BLOCK_HEAD} lines (click to expand${isError ? "; contains error" : ""})`}
          </button>
        </div>
      ) : null}
    </>
  );
}

// timeline event kinds: tick color / height (baml + error stand out)
const TICK: Record<string, string> = {
  baml: "bg-[#c4b5fd] h-3.5",
  tool: "bg-[#3fb950] h-2.5",
  user: "bg-[#6cb6ff] h-3.5",
  think: "bg-[#6e7681] h-1.5",
  error: "bg-[#ff5f57] h-3.5",
};

type TermEvent = { block: number; at: number; kind: string; label: string };

function classifyBlock(
  entries: TermEntry[],
  block: number,
  at: number,
): TermEvent | null {
  const { ts, rest: first } = splitStamp(entries[0]?.line ?? "");
  const when = ts ? `${ts} · ` : "";
  const tool = first.match(TOOL_CALL);
  if (tool) {
    const isBaml =
      tool[1] === "Bash" &&
      entries.some((e, i) =>
        textInvokesBaml(i === 0 ? tool[2].replace(/^\(/, "") : e.line),
      );
    return {
      block,
      at,
      kind: isBaml ? "baml" : "tool",
      label: `${when}${isBaml ? "baml · " : ""}${tool[1]}${tool[2].slice(0, 110)}`,
    };
  }
  if (first.startsWith("> "))
    return { block, at, kind: "user", label: `${when}${first.slice(0, 110)}` };
  if (first.startsWith("✻"))
    return { block, at, kind: "think", label: `${when}thinking` };
  if (
    first.startsWith("  ⎿") &&
    /\[error\]/i.test(entries.map((e) => e.line).join("\n"))
  )
    return {
      block,
      at,
      kind: "error",
      label: `${when}${first.slice(4, 64).trim() || "error"}`,
    };
  return null;
}

export function TerminalView({ text }: { text: string }) {
  const termRef = useRef<HTMLDivElement>(null);
  // measured fraction (offsetTop / scrollHeight) per block index, so ticks,
  // the viewport band, and click-jumps share one coordinate space
  const [pos, setPos] = useState<Record<number, number>>({});
  const [view, setView] = useState({ left: 0, width: 1 });
  const [hover, setHover] = useState<TermEvent | null>(null);

  const measure = () => {
    const el = termRef.current;
    if (!el) return;
    const total = Math.max(1, el.scrollHeight);
    const next: Record<number, number> = {};
    el.querySelectorAll<HTMLElement>("[data-block]").forEach((b) => {
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

  const lines = text.split("\n");
  // sections run from one marker line to the next (blank lines stay
  // inside), so an 80-line tool result trims as one unit
  const blocks: TermEntry[][] = [];
  const blockStart: number[] = [];
  let mode = "";
  lines.forEach((line, n) => {
    const bare = splitStamp(line).rest;
    if (line === "") {
      mode = "";
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
    .map((b, i) =>
      classifyBlock(b, i, blockStart[i] / Math.max(1, lines.length)),
    )
    .filter((e): e is TermEvent => e !== null);

  const jump = (block: number) => {
    const el = termRef.current;
    const target = el?.querySelector<HTMLElement>(`[data-block="${block}"]`);
    if (el && target) {
      el.scrollTo({ top: target.offsetTop - 8, behavior: "smooth" });
    }
  };
  const onScroll = () => {
    const el = termRef.current;
    if (!el) return;
    const total = Math.max(1, el.scrollHeight);
    setView({ left: el.scrollTop / total, width: el.clientHeight / total });
  };

  return (
    <div className="overflow-hidden rounded-lg border border-[#3a3a38] shadow-[0_8px_30px_rgba(0,0,0,0.25)]">
      {/* title bar: traffic lights + session label */}
      <div
        className="flex items-center gap-2 bg-[#2a2a28] px-3.5 py-2"
        aria-hidden
      >
        <span className="size-3 rounded-full bg-[#ff5f57]" />
        <span className="size-3 rounded-full bg-[#febc2e]" />
        <span className="size-3 rounded-full bg-[#28c840]" />
        <span className="ml-2 font-atb-mono text-[11px] text-[#8a8a86]">
          claude · agent transcript
        </span>
        <span className="ml-auto font-atb-mono text-[11px] text-[#5c5c58]">
          {lines.length} lines
        </span>
      </div>
      {/* event timeline: every tool call / user turn / error in the session */}
      <div className="relative border-b border-[#3a3a38] bg-[#222220] px-3.5 py-1.5">
        <div className="relative h-5">
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
            className="pointer-events-none absolute top-full z-10 mt-1 max-w-[70%] -translate-x-1/2 truncate rounded border border-[#4a4a46] bg-[#333330] px-2 py-1 font-atb-mono text-[11px] text-[#d7d3c8] shadow-[0_4px_12px_rgba(0,0,0,0.4)]"
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
        className="atb-terminal atb-scroll relative !rounded-none"
      >
        {blocks.map((b, i) => (
          <div key={i} data-block={i}>
            {/* expanding/collapsing reflows the content, so re-measure */}
            <TermBlock
              entries={b}
              onToggle={() => requestAnimationFrame(measure)}
            />
          </div>
        ))}
        {/* resting prompt with a blinking block cursor */}
        <div className="mt-1">
          <span className="text-[#4a7a4a]">❯</span>{" "}
          <span className="t-cursor">▋</span>
        </div>
      </div>
    </div>
  );
}
