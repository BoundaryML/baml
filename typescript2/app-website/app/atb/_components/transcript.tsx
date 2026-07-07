"use client";

// The transcript reader. "Turns" renders the structured turnLog: thinking
// (collapsed), assistant text, and tool calls with expandable input/output.
// "Terminal" streams the raw Claude Code transcript through the server
// proxy and renders it as a colorized terminal with the event timeline.

import { AnimatePresence, motion } from "framer-motion";
import { useCallback, useEffect, useMemo, useState } from "react";
import type { Turn, TurnTool } from "@/app/atb/_lib/types";
import type { TranscriptComment } from "@/app/atb/_lib/comments";
import { EASE } from "@/app/atb/_components/ui";
import { TerminalView } from "@/app/atb/_components/terminal";
import { CodeView } from "@/app/atb/_components/code-view";
import { CommentThread } from "@/app/atb/_components/comments";

// Floating "comment on selection" affordance, positioned at a viewport point.
// mousedown-preventDefault keeps the text selection alive through the click.
function QuoteButton({
  top,
  left,
  onClick,
}: {
  top: number;
  left: number;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onMouseDown={(e) => e.preventDefault()}
      onClick={onClick}
      style={{
        position: "fixed",
        top: Math.max(8, top - 38),
        left,
        transform: "translateX(-50%)",
        zIndex: 40,
      }}
      className="rounded-full bg-atb-ink px-3 py-1 font-atb-mono text-[11px] text-atb-cloud shadow-lg hover:bg-atb-ink-2"
    >
      💬 comment on selection
    </button>
  );
}

/** Reads the current text selection and returns its trimmed text + a viewport
 *  anchor, or null. Ignores selections inside form controls. `within` optionally
 *  requires the selection to sit inside a matching ancestor (e.g. a turn). */
function readSelection(within?: string): {
  text: string;
  top: number;
  left: number;
  host: HTMLElement | null;
} | null {
  const s = window.getSelection();
  if (!s || s.isCollapsed || !s.rangeCount) return null;
  const text = s.toString().replace(/\s+$/, "");
  if (!text.trim()) return null;
  const anchor = s.anchorNode;
  const el = anchor instanceof Element ? anchor : anchor?.parentElement;
  if (el?.closest("textarea, input")) return null;
  const host = within
    ? (el?.closest(within) as HTMLElement | null)
    : ((el as HTMLElement | null) ?? null);
  if (within && !host) return null;
  const rect = s.getRangeAt(0).getBoundingClientRect();
  return { text, top: rect.top, left: rect.left + rect.width / 2, host };
}

export function TranscriptViewer({
  turnLog,
  transcriptStorageId,
  trophyId,
  taskId,
  comments,
  onQuote,
}: {
  turnLog: Turn[];
  transcriptStorageId?: string | null;
  trophyId?: string;
  taskId?: string;
  comments?: TranscriptComment[];
  /** Highlight-to-quote from the raw terminal routes here (run-level comment). */
  onQuote?: (text: string) => void;
}) {
  const [view, setView] = useState<"turns" | "terminal">(
    transcriptStorageId ? "terminal" : "turns",
  );
  const [expandAll, setExpandAll] = useState(false);
  // If the raw terminal can't be fetched (proxy down, storage gone), fall back
  // to the structured Turns view instead of dead-ending on "unavailable".
  const [terminalDead, setTerminalDead] = useState(false);
  const terminalOk = !!transcriptStorageId && !terminalDead;
  const onTerminalUnavailable = useCallback(() => {
    setTerminalDead(true);
    setView("turns");
  }, []);

  return (
    <div>
      <div className="flex items-center gap-2 mb-4">
        <div className="flex bg-atb-ivory border border-atb-line rounded-full p-0.5">
          {(["terminal", "turns"] as const).map((v) => (
            <button
              key={v}
              onClick={() => setView(v)}
              disabled={v === "terminal" && !terminalOk}
              className={`relative px-4 py-1 text-xs font-medium rounded-full transition-colors disabled:opacity-40 ${
                view === v ? "text-atb-cloud" : "text-atb-ink-2 hover:text-atb-ink"
              }`}
            >
              {view === v && (
                <motion.span
                  layoutId="transcript-tab"
                  className="absolute inset-0 bg-atb-ink rounded-full"
                  transition={{ type: "spring", stiffness: 400, damping: 34 }}
                />
              )}
              <span className="relative capitalize">
                {v === "turns" ? `Turns (${turnLog.length})` : "Terminal"}
              </span>
            </button>
          ))}
        </div>
        {view === "turns" && (
          <button
            onClick={() => setExpandAll((e) => !e)}
            className="ml-auto text-xs text-atb-ink-3 hover:text-atb-ink transition-colors"
          >
            {expandAll ? "collapse all" : "expand all"}
          </button>
        )}
      </div>

      {view === "turns" ? (
        <TurnList
          turnLog={turnLog}
          expandAll={expandAll}
          trophyId={trophyId}
          taskId={taskId}
          comments={comments}
        />
      ) : (
        <RawTerminal
          storageId={transcriptStorageId!}
          onUnavailable={onTerminalUnavailable}
          onQuote={trophyId ? onQuote : undefined}
        />
      )}
    </div>
  );
}

// ---- structured turn view ----

function TurnList({
  turnLog,
  expandAll,
  trophyId,
  taskId,
  comments,
}: {
  turnLog: Turn[];
  expandAll: boolean;
  trophyId?: string;
  taskId?: string;
  comments?: TranscriptComment[];
}) {
  // elapsed time between consecutive timestamped turns
  const elapsed = useMemo(() => {
    const out = new Map<number, number>();
    let prev: number | null = null;
    for (const t of turnLog) {
      if (!t.ts) continue;
      const ms = Date.parse(t.ts);
      if (prev != null) out.set(t.i, ms - prev);
      prev = ms;
    }
    return out;
  }, [turnLog]);

  // Highlight-to-quote: a text selection inside a turn raises a floating
  // "comment on selection" button; clicking it opens that turn's composer with
  // the highlighted snippet attached (via `pending`, keyed by a nonce).
  const [sel, setSel] = useState<{
    turnIndex: number;
    text: string;
    top: number;
    left: number;
  } | null>(null);
  const [pending, setPending] = useState<{
    turnIndex: number;
    text: string;
    nonce: number;
  } | null>(null);

  const onMouseUp = () => {
    if (!trophyId) return;
    const r = readSelection("[data-turn-index]");
    if (!r?.host?.dataset.turnIndex) return setSel(null);
    setSel({
      turnIndex: Number(r.host.dataset.turnIndex),
      text: r.text,
      top: r.top,
      left: r.left,
    });
  };

  return (
    <div className="relative">
      {/* timeline spine */}
      <div className="absolute left-[15px] top-2 bottom-2 w-px bg-atb-line" aria-hidden />
      {/** biome-ignore lint/a11y/noStaticElementInteractions: capturing text selection, not a control */}
      <div className="space-y-1" onMouseUp={onMouseUp}>
        {turnLog.map((turn) => (
          <TurnBlock
            key={turn.i}
            turn={turn}
            gapMs={elapsed.get(turn.i)}
            expandAll={expandAll}
            trophyId={trophyId}
            taskId={taskId}
            comments={(comments ?? []).filter((c) => c.turnIndex === turn.i)}
            quoteRequest={
              pending?.turnIndex === turn.i
                ? { text: pending.text, nonce: pending.nonce }
                : null
            }
          />
        ))}
      </div>
      {sel && (
        <QuoteButton
          top={sel.top}
          left={sel.left}
          onClick={() => {
            setPending({
              turnIndex: sel.turnIndex,
              text: sel.text,
              nonce: Date.now(),
            });
            setSel(null);
            window.getSelection()?.removeAllRanges();
          }}
        />
      )}
    </div>
  );
}

function TurnBlock({
  turn,
  gapMs,
  expandAll,
  trophyId,
  taskId,
  comments = [],
  quoteRequest,
}: {
  turn: Turn;
  gapMs?: number;
  expandAll: boolean;
  trophyId?: string;
  taskId?: string;
  comments?: TranscriptComment[];
  quoteRequest?: { text: string; nonce: number } | null;
}) {
  const [showComments, setShowComments] = useState(false);
  // A highlight-to-quote request must mount the thread so it can open.
  // biome-ignore lint/correctness/useExhaustiveDependencies: fire once per selection (nonce)
  useEffect(() => {
    if (quoteRequest?.nonce) setShowComments(true);
  }, [quoteRequest?.nonce]);
  const hasContent =
    turn.thinking_preview || turn.text_preview || (turn.tools?.length ?? 0) > 0;
  if (!hasContent) return null;

  return (
    <motion.div
      id={`turn-${turn.i}`}
      data-turn-index={turn.i}
      initial={{ opacity: 0, y: 10 }}
      whileInView={{ opacity: 1, y: 0 }}
      viewport={{ once: true, margin: "-20px" }}
      transition={{ duration: 0.5, ease: EASE }}
      className="relative pl-10 py-3 scroll-mt-24"
    >
      {/* turn marker */}
      <div className="absolute left-0 top-4 w-[31px] flex justify-center">
        <span className="w-[9px] h-[9px] rounded-full bg-atb-cloud border-2 border-atb-line-strong" />
      </div>
      <div className="flex items-baseline gap-3 mb-1.5">
        <span className="font-atb-mono text-[11px] text-atb-ink-3">turn {turn.i}</span>
        {turn.ts && (
          <span className="font-atb-mono text-[11px] text-atb-ink-3/70">
            {new Date(turn.ts).toLocaleTimeString()}
          </span>
        )}
        {gapMs != null && gapMs > 1500 && (
          <span className="font-atb-mono text-[11px] text-atb-accent-deep/70">
            +{(gapMs / 1000).toFixed(0)}s
          </span>
        )}
        {trophyId && (
          <button
            onClick={() => setShowComments((v) => !v)}
            className={`ml-auto font-atb-mono text-[11px] transition-colors ${
              comments.length > 0
                ? "text-atb-amber hover:text-atb-ink"
                : "text-atb-ink-3/60 hover:text-atb-ink-2"
            }`}
            title="comment on this turn"
          >
            💬 {comments.length > 0 ? comments.length : ""}
          </button>
        )}
      </div>

      {turn.thinking_preview && (
        <Collapsible
          forceOpen={expandAll}
          summary={
            <span className="font-atb-serif italic text-atb-ink-3">
              thinking · {fmtChars(turn.thinking_chars)}
            </span>
          }
        >
          <p className="font-atb-serif italic text-sm text-atb-ink-2 leading-relaxed whitespace-pre-wrap">
            {turn.thinking_preview}
          </p>
        </Collapsible>
      )}

      {turn.text_preview && (
        <div className="text-[15px] text-atb-ink leading-relaxed whitespace-pre-wrap my-1.5">
          {turn.text_preview}
        </div>
      )}

      {(turn.tools ?? []).map((tool, j) => (
        <ToolCall key={j} tool={tool} forceOpen={expandAll} />
      ))}

      {trophyId && (showComments || comments.length > 0) && (
        <CommentThread
          trophyId={trophyId}
          taskId={taskId}
          turnIndex={turn.i}
          comments={comments}
          quoteRequest={quoteRequest}
        />
      )}
    </motion.div>
  );
}

function ToolCall({ tool, forceOpen }: { tool: TurnTool; forceOpen: boolean }) {
  const headline = toolHeadline(tool);
  const file = fileInput(tool);
  return (
    <Collapsible
      forceOpen={forceOpen}
      summary={
        <span className="flex items-center gap-2 min-w-0">
          <span
            className={`font-atb-mono text-xs font-semibold shrink-0 ${
              tool.is_error ? "text-atb-rust" : "text-atb-accent-deep"
            }`}
          >
            {tool.name ?? "tool"}
          </span>
          {headline && (
            <span className="font-atb-mono text-xs text-atb-ink-3 truncate">
              {headline}
            </span>
          )}
          {tool.is_error && (
            <span className="shrink-0 text-[10px] font-medium uppercase tracking-wide bg-atb-rust-soft text-atb-rust px-1.5 py-px rounded-full">
              error
            </span>
          )}
        </span>
      }
    >
      <div className="space-y-2">
        {tool.input != null && (
          <div>
            <p className="text-[10px] uppercase tracking-wider text-atb-ink-3 mb-1">
              input
            </p>
            {file ? (
              // file-writing tools get real syntax highlighting
              <CodeView
                path={file.path}
                content={file.content}
                className="rounded-lg max-h-72"
              />
            ) : (
              <pre className="atb-scroll bg-[#1a1a1a] text-[#ece9df] text-xs rounded-lg p-3 overflow-auto max-h-72 leading-relaxed">
                {fmtInput(tool.input)}
              </pre>
            )}
          </div>
        )}
        {tool.result_preview != null && (
          <div>
            <p className="text-[10px] uppercase tracking-wider text-atb-ink-3 mb-1">
              result · {fmtChars(tool.result_chars)}
            </p>
            <pre
              className={`atb-scroll text-xs rounded-lg p-3 overflow-auto max-h-72 leading-relaxed border ${
                tool.is_error
                  ? "bg-atb-rust-soft/50 border-atb-rust/20 text-atb-rust"
                  : "bg-atb-ivory border-atb-line text-atb-ink-2"
              }`}
            >
              {tool.result_preview}
            </pre>
          </div>
        )}
      </div>
    </Collapsible>
  );
}

/** A one-line summary of a tool call's input (command, file, pattern…). */
function toolHeadline(tool: TurnTool): string | null {
  const input = tool.input as Record<string, unknown> | null | undefined;
  if (!input || typeof input !== "object") return null;
  const pick =
    input.description ??
    input.command ??
    input.file_path ??
    input.pattern ??
    input.path ??
    input.prompt ??
    null;
  if (pick == null) return null;
  return String(pick).replace(/\s+/g, " ").slice(0, 110);
}

/** Write/Edit-style inputs carry a path + content worth highlighting. */
function fileInput(
  tool: TurnTool,
): { path: string; content: string } | null {
  const input = tool.input as Record<string, unknown> | null | undefined;
  if (!input || typeof input !== "object") return null;
  const path = input.file_path ?? input.path;
  const content = input.content ?? input.new_string;
  if (typeof path === "string" && typeof content === "string")
    return { path, content };
  return null;
}

function fmtInput(input: unknown): string {
  if (typeof input === "string") return input;
  const obj = input as Record<string, unknown>;
  // single string field (e.g. Bash command) reads better unquoted
  if (
    obj &&
    typeof obj === "object" &&
    Object.keys(obj).length === 1 &&
    typeof Object.values(obj)[0] === "string"
  ) {
    return String(Object.values(obj)[0]);
  }
  return JSON.stringify(input, null, 2);
}

function fmtChars(n?: number): string {
  if (n == null) return "";
  if (n >= 1000) return `${(n / 1000).toFixed(1)}k chars`;
  return `${n} chars`;
}

// ---- shared collapsible with smooth height animation ----

function Collapsible({
  summary,
  children,
  forceOpen,
}: {
  summary: React.ReactNode;
  children: React.ReactNode;
  forceOpen?: boolean;
}) {
  const [open, setOpen] = useState(false);
  useEffect(() => {
    setOpen(!!forceOpen);
  }, [forceOpen]);

  return (
    <div className="border border-atb-line rounded-xl bg-atb-ivory/50 my-1.5 overflow-hidden">
      <button
        onClick={() => setOpen((o) => !o)}
        className="w-full flex items-center gap-2 px-3 py-2 text-left hover:bg-atb-oat/40 transition-colors"
      >
        <motion.span
          animate={{ rotate: open ? 90 : 0 }}
          transition={{ duration: 0.25, ease: EASE }}
          className="text-atb-ink-3 text-[10px] shrink-0"
        >
          ▶
        </motion.span>
        <span className="flex-1 min-w-0">{summary}</span>
      </button>
      <AnimatePresence initial={false}>
        {open && (
          <motion.div
            initial={{ height: 0, opacity: 0 }}
            animate={{ height: "auto", opacity: 1 }}
            exit={{ height: 0, opacity: 0 }}
            transition={{ duration: 0.35, ease: EASE }}
          >
            <div className="px-3 pb-3">{children}</div>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}

// ---- raw terminal view ----

function RawTerminal({
  storageId,
  onUnavailable,
  onQuote,
}: {
  storageId: string;
  onUnavailable?: () => void;
  onQuote?: (text: string) => void;
}) {
  const [text, setText] = useState<string | null>(null);
  const [error, setError] = useState(false);
  const [sel, setSel] = useState<{ text: string; top: number; left: number } | null>(
    null,
  );

  useEffect(() => {
    let cancelled = false;
    fetch(`/api/atb/transcript/${storageId}`)
      .then((r) => (r.ok ? r.text() : Promise.reject(new Error(`${r.status}`))))
      .then((t) => !cancelled && setText(t))
      .catch(() => {
        if (cancelled) return;
        setError(true);
        onUnavailable?.();
      });
    return () => {
      cancelled = true;
    };
  }, [storageId, onUnavailable]);

  if (error)
    return (
      <p className="text-sm text-atb-ink-3 py-8 text-center">
        raw transcript unavailable
      </p>
    );
  if (text == null) return <div className="bg-atb-oat/70 rounded-xl h-64 atb-blink-soft" />;
  return (
    <motion.div
      initial={{ opacity: 0, y: 8 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.5, ease: EASE }}
    >
      {/** biome-ignore lint/a11y/noStaticElementInteractions: capturing text selection, not a control */}
      <div onMouseUp={() => onQuote && setSel(readSelection())}>
        <TerminalView text={text} />
      </div>
      {sel && onQuote && (
        <QuoteButton
          top={sel.top}
          left={sel.left}
          onClick={() => {
            onQuote(sel.text);
            setSel(null);
            window.getSelection()?.removeAllRanges();
          }}
        />
      )}
    </motion.div>
  );
}
