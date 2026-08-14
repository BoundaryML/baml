"use client";

// Comment threads on a run transcript: a per-turn thread (anchored to turn i)
// and a run-level thread. Comments are queued into dedup, which turns
// actionable ones into issue-board tickets.
//
// Flow: existing comments are always visible. "add comment" opens the composer;
// if the shared password isn't stored yet, a small popup asks for it once
// (verified server-side on post; a rejected password re-prompts).

import { useEffect, useState } from "react";
import {
  type TranscriptComment,
  clearCommentsKey,
  getCommentAuthor,
  getCommentsKey,
  setCommentAuthor,
  setCommentsKey,
  useAddComment,
} from "@/app/atb/_lib/comments";
import { timeAgo } from "@/app/atb/_lib/format";

/** Split a leading `> `-prefixed blockquote (a folded highlight) off the body,
 *  so it can render in a styled quote box above the comment text. */
function splitQuoted(body: string): { quote: string | null; text: string } {
  const lines = body.split("\n");
  let i = 0;
  const q: string[] = [];
  while (i < lines.length && lines[i].startsWith("> ")) {
    q.push(lines[i].slice(2));
    i++;
  }
  if (q.length && lines[i] === "") i++; // drop the blank separator
  return q.length
    ? { quote: q.join("\n"), text: lines.slice(i).join("\n") }
    : { quote: null, text: body };
}

function PasswordPopup({
  onUnlock,
  onCancel,
}: {
  onUnlock: () => void;
  onCancel: () => void;
}) {
  const [key, setKey] = useState("");
  const submit = () => {
    if (!key.trim()) return;
    setCommentsKey(key.trim());
    onUnlock();
  };
  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-atb-ink/30 backdrop-blur-[2px]"
      onClick={onCancel}
    >
      <div
        className="w-80 rounded-2xl border border-atb-line bg-atb-cloud p-5 shadow-xl"
        onClick={(e) => e.stopPropagation()}
      >
        <p className="mb-1 font-atb-serif text-lg text-atb-ink">Comments password</p>
        <p className="mb-3 text-xs text-atb-ink-3 leading-relaxed">
          Commenting needs the shared team password. It's checked on post and
          remembered on this device.
        </p>
        <input
          type="password"
          autoFocus
          value={key}
          onChange={(e) => setKey(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && submit()}
          placeholder="password"
          className="w-full rounded-lg border border-atb-line bg-atb-ivory/60 px-3 py-2 font-atb-mono text-sm text-atb-ink placeholder:text-atb-ink-3 outline-none focus:border-atb-line-strong"
        />
        <div className="mt-3 flex justify-end gap-2">
          <button
            onClick={onCancel}
            className="rounded-full px-3 py-1 text-xs text-atb-ink-3 hover:text-atb-ink"
          >
            cancel
          </button>
          <button
            onClick={submit}
            disabled={!key.trim()}
            className="rounded-full bg-atb-ink px-4 py-1 text-xs font-medium text-atb-cloud disabled:opacity-40"
          >
            unlock
          </button>
        </div>
      </div>
    </div>
  );
}

export function CommentThread({
  trophyId,
  taskId,
  turnIndex,
  comments,
  quoteRequest,
}: {
  trophyId: string;
  taskId?: string;
  turnIndex?: number;
  comments: TranscriptComment[];
  /** A snippet the reader highlighted in the transcript; opens the composer
   *  with the quote attached. `nonce` re-triggers on each fresh selection. */
  quoteRequest?: { text: string; nonce: number } | null;
}) {
  const addComment = useAddComment();
  const [composing, setComposing] = useState(false);
  const [askKey, setAskKey] = useState(false);
  const [body, setBody] = useState("");
  const [author, setAuthor] = useState(getCommentAuthor());
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [quote, setQuote] = useState<string>("");

  const beginCompose = (withQuote?: string) => {
    setError("");
    if (withQuote != null) setQuote(withQuote);
    if (getCommentsKey()) setComposing(true);
    else setAskKey(true);
  };

  // A highlight in the transcript opens this turn's composer with the snippet.
  // biome-ignore lint/correctness/useExhaustiveDependencies: fire once per selection (nonce)
  useEffect(() => {
    if (quoteRequest?.text) beginCompose(quoteRequest.text);
  }, [quoteRequest?.nonce]);

  const submit = async () => {
    if (!body.trim() || busy) return;
    setBusy(true);
    setError("");
    try {
      if (author.trim()) setCommentAuthor(author.trim());
      // Fold the highlighted snippet into the body as a blockquote so the write
      // needs no extra schema field on the backend `transcriptComments` table.
      const q = quote.trim();
      const fullBody = q
        ? `${q.replace(/^/gm, "> ")}\n\n${body.trim()}`
        : body.trim();
      await addComment({
        trophyId,
        taskId,
        turnIndex,
        author: author.trim() || "anonymous",
        body: fullBody,
      });
      setBody("");
      setQuote("");
      setComposing(false);
    } catch (e) {
      // Convex masks a thrown Error's text as a generic "Server Error", so a
      // rejected password can't be positively identified. Treat both the
      // explicit message and the masked one as a password failure and re-prompt.
      const msg = String(e);
      if (
        msg.includes("invalid comments password") ||
        msg.includes("Server Error")
      ) {
        clearCommentsKey();
        setComposing(false);
        setAskKey(true);
        setError("post rejected: re-enter the team password and try again");
      } else {
        setError("couldn't post, try again");
      }
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="mt-2 space-y-2">
      {comments.map((c) => {
        const folded = splitQuoted(c.body);
        const shownQuote = c.quote || folded.quote;
        return (
          <div
            key={c._id}
            className="rounded-lg border border-atb-line bg-atb-ivory/60 px-3 py-2"
          >
            <div className="flex items-baseline gap-2 mb-0.5">
              <span className="font-atb-mono text-[11px] font-semibold text-atb-ink-2">
                {c.author}
              </span>
              <span className="font-atb-mono text-[10.5px] text-atb-ink-3">
                {timeAgo(c.createdAt, Date.now())}
              </span>
              {c.status !== "done" && (
                <span className="font-atb-mono text-[10.5px] text-atb-amber">
                  → dedup queue
                </span>
              )}
            </div>
            {shownQuote && (
              <pre className="mb-1 max-h-40 overflow-auto rounded border-l-2 border-atb-line-strong bg-atb-ivory px-2 py-1 font-atb-mono text-[11px] text-atb-ink-2 whitespace-pre-wrap">
                {shownQuote}
              </pre>
            )}
            <p className="text-sm text-atb-ink leading-relaxed whitespace-pre-wrap">
              {folded.text}
            </p>
          </div>
        );
      })}

      {!composing && (
        <button
          onClick={() => beginCompose()}
          className="font-atb-mono text-[11.5px] text-atb-ink-3 hover:text-atb-ink transition-colors"
        >
          ＋ add comment
        </button>
      )}

      {askKey && (
        <PasswordPopup
          onUnlock={() => {
            setAskKey(false);
            setComposing(true);
          }}
          onCancel={() => setAskKey(false)}
        />
      )}

      {composing && (
        <div className="rounded-lg border border-atb-line bg-atb-cloud px-3 py-2">
          {quote && (
            <div className="mb-1.5 flex items-start gap-1">
              <pre className="flex-1 max-h-40 overflow-auto rounded border-l-2 border-atb-accent-deep/50 bg-atb-ivory/70 px-2 py-1 font-atb-mono text-[11px] text-atb-ink-2 whitespace-pre-wrap">
                {quote}
              </pre>
              <button
                onClick={() => setQuote("")}
                title="remove quote"
                className="shrink-0 px-1 text-[13px] leading-none text-atb-ink-3 hover:text-atb-ink"
              >
                ×
              </button>
            </div>
          )}
          <textarea
            autoFocus
            value={body}
            onChange={(e) => setBody(e.target.value)}
            placeholder={
              turnIndex != null
                ? `Comment on turn ${turnIndex}…`
                : "Comment on this run…"
            }
            rows={2}
            className="w-full resize-y bg-transparent text-sm text-atb-ink placeholder:text-atb-ink-3 outline-none"
          />
          <div className="flex flex-wrap items-center gap-2 mt-1.5">
            <input
              value={author}
              onChange={(e) => setAuthor(e.target.value)}
              placeholder="your name"
              className="w-32 rounded border border-atb-line bg-atb-ivory/60 px-2 py-1 font-atb-mono text-[11px] text-atb-ink placeholder:text-atb-ink-3 outline-none"
            />
            <div className="ml-auto flex items-center gap-2">
              <button
                onClick={() => {
                  setComposing(false);
                  setQuote("");
                }}
                className="rounded-full px-2 py-1 text-[11px] text-atb-ink-3 hover:text-atb-ink"
              >
                cancel
              </button>
              <button
                onClick={submit}
                disabled={busy || !body.trim()}
                className="rounded-full bg-atb-ink px-3 py-1 text-[11px] font-medium text-atb-cloud transition-opacity disabled:opacity-40"
              >
                {busy ? "posting…" : "post"}
              </button>
            </div>
          </div>
          {error && (
            <p className="mt-1 font-atb-mono text-[11px] text-atb-rust">{error}</p>
          )}
        </div>
      )}
      {!composing && error && (
        <p className="font-atb-mono text-[11px] text-atb-rust">{error}</p>
      )}
    </div>
  );
}
