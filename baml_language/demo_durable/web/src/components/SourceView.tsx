import { useEffect, useMemo, useRef, useState } from "react";
import type { Api } from "../api";
import { tokenizeLine } from "../highlight";
import type { Run, Site } from "../protocol";
import { latestPosition, threadPositions, type TimelineEvent } from "../state";

/**
 * A line that another panel asked to show. `origin` separates the frame that
 * the state tree selected from the source location of a cause that the
 * timeline offered (contract section 10.3): the two get their own mark, so a
 * clicked location is never confused with the paused line.
 */
export interface SourceFocus {
  file: string;
  line: number;
  /** The site whose source to load. Defaults to the site of the selected run. */
  site?: Site;
  origin?: "frame" | "cause";
  /** What the location stands for, for the tooltip and the panel head. */
  what?: string;
}

interface Props {
  api: Api;
  run: Run | null;
  events: readonly TimelineEvent[] | undefined;
  focus: SourceFocus | null;
  /** Clears a focused line. The head shows a button while one is set. */
  onClearFocus?: () => void;
}

type Mark = "current" | "paused" | "thread" | "focus" | "cause";

interface LineMark {
  mark: Mark;
  tag: string;
}

const MARK_TITLES: Record<Mark, string> = {
  current: "latest position event",
  paused: "paused here: the snapshot holds this position",
  thread: "latest position of another thread",
  focus: "frame selected in the state tree",
  cause: "the source location of the selected timeline object",
};

const sourceCache = new Map<string, Promise<string>>();

function loadSource(api: Api, site: Run["site"], file: string): Promise<string> {
  const key = `${api.mode}:${site}:${file}`;
  let pending = sourceCache.get(key);
  if (!pending) {
    pending = api.source(site, file).then((response) => response.text);
    pending.catch(() => sourceCache.delete(key));
    sourceCache.set(key, pending);
  }
  return pending;
}

export function SourceView({ api, run, events, focus, onClearFocus }: Props) {
  const latest = useMemo(() => latestPosition(events), [events]);
  const position = latest ?? run?.position ?? null;
  const file = focus?.file ?? position?.file ?? null;
  // A cause can point at a file of another site, for example the source of a
  // remote child. The link names the site that serves it.
  const site = focus?.site ?? run?.site ?? null;

  const [text, setText] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    setError(null);
    if (file === null || site === null) {
      setText(null);
      return;
    }
    let cancelled = false;
    loadSource(api, site, file)
      .then((loaded) => !cancelled && setText(loaded))
      .catch((cause: unknown) => {
        if (cancelled) return;
        setText(null);
        setError(cause instanceof Error ? cause.message : String(cause));
      });
    return () => {
      cancelled = true;
    };
  }, [api, site, file]);

  const paused = run?.status === "paused" || run?.status === "pausing";
  const marks = useMemo(() => {
    const result = new Map<number, LineMark>();
    if (!run || file === null) return result;
    const threads = threadPositions(events, latest?.segment ?? run.segment).filter((entry) => entry.file === file);
    const isPaused = run.status === "paused";
    for (const entry of threads) {
      const isLatest = latest !== null && entry.thread === latest.thread;
      if (isPaused) result.set(entry.line, { mark: "paused", tag: `‖ t${entry.thread}` });
      else if (!isLatest) result.set(entry.line, { mark: "thread", tag: `t${entry.thread}` });
    }
    if (position && position.file === file) {
      if (isPaused) result.set(position.line, { mark: "paused", tag: `‖ t${position.thread}` });
      else result.set(position.line, { mark: "current", tag: `▶ t${position.thread}` });
    }
    // A clicked location wins over every other mark on its line, so that it is
    // never hidden behind the paused line (contract section 10.3).
    if (focus && focus.file === file) {
      const cause = focus.origin === "cause";
      result.set(focus.line, { mark: cause ? "cause" : "focus", tag: cause ? "▸ why" : "frame" });
    }
    return result;
  }, [run, file, events, latest, position, focus]);

  const lines = useMemo(() => (text === null ? [] : text.replace(/\n$/, "").split("\n")), [text]);
  const tokens = useMemo(() => lines.map(tokenizeLine), [lines]);

  // Keep the highlighted line in view.
  const bodyRef = useRef<HTMLDivElement | null>(null);
  const scrollTarget = focus && focus.file === file ? focus.line : position && position.file === file ? position.line : null;
  useEffect(() => {
    if (scrollTarget === null || lines.length === 0) return;
    const body = bodyRef.current;
    const element = body?.querySelector<HTMLElement>(`[data-line="${scrollTarget}"]`);
    if (!body || !element) return;
    const top = element.offsetTop;
    if (top < body.scrollTop + 34 || top > body.scrollTop + body.clientHeight - 51) {
      body.scrollTo({ top: Math.max(top - body.clientHeight / 2, 0) });
    }
  }, [scrollTarget, lines.length]);

  return (
    <section className="panel" data-testid="source-view">
      <div className="panel-head">
        <span className="panel-title">Source</span>
        <span className="mono clip">{file ?? ""}</span>
        <span className="spacer" />
        {focus && onClearFocus && (
          <button className="btn small" data-testid="clear-focus" title={`Clear the highlighted line ${focus.file}:${focus.line}`}
            onClick={onClearFocus}>
            ✕ {focus.line}
          </button>
        )}
        {position && (
          <span className="mono muted">
            {position.function}:{position.line}
            {latest ? ` · ${latest.reason}${latest.op ? ` ${latest.op}` : ""}` : ""}
            {paused ? " · paused" : ""}
          </span>
        )}
      </div>
      <div className="panel-body" ref={bodyRef}>
        {error && <div className="error-line" style={{ margin: 8 }}>{error}</div>}
        {!error && lines.length === 0 && (
          <div className="empty">{run ? "No position reported for this run yet." : "Select a run to see its source."}</div>
        )}
        {lines.length > 0 && (
          <div className="source">
            {tokens.map((lineTokens, index) => {
              const number = index + 1;
              const mark = marks.get(number);
              return (
                <div key={number} className="src-line" data-line={number} data-mark={mark?.mark} data-site={site ?? undefined}>
                  <span className="ln">{number}</span>
                  <span className="tag" title={mark ? MARK_TITLES[mark.mark] : undefined}>{mark?.tag ?? ""}</span>
                  <span className="code">
                    {lineTokens.map((token, tokenIndex) =>
                      token.kind === "plain" ? token.text : (
                        <span key={tokenIndex} className={`tok-${token.kind}`}>{token.text}</span>
                      ),
                    )}
                  </span>
                </div>
              );
            })}
          </div>
        )}
      </div>
    </section>
  );
}
