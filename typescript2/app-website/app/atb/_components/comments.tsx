"use client";

// Comment threads on a run transcript: a per-turn thread (anchored to turn i)
// and a run-level thread. Comments are queued into dedup, which turns
// actionable ones into issue-board tickets.

import { useState } from "react";
import {
  type TranscriptComment,
  getCommentAuthor,
  getCommentsKey,
  setCommentAuthor,
  setCommentsKey,
  useAddComment,
} from "@/app/atb/_lib/comments";
import { timeAgo } from "@/app/atb/_lib/format";

export function CommentThread({
  trophyId,
  taskId,
  turnIndex,
  comments,
}: {
  trophyId: string;
  taskId?: string;
  turnIndex?: number;
  comments: TranscriptComment[];
}) {
  const addComment = useAddComment();
  const [body, setBody] = useState("");
  const [author, setAuthor] = useState(getCommentAuthor());
  const [key, setKey] = useState(getCommentsKey());
  const [needsKey, setNeedsKey] = useState(!getCommentsKey());
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  const submit = async () => {
    if (!body.trim() || busy) return;
    setBusy(true);
    setError("");
    try {
      if (needsKey) {
        if (!key.trim()) throw new Error("password required");
        setCommentsKey(key.trim());
      }
      if (author.trim()) setCommentAuthor(author.trim());
      await addComment({
        trophyId,
        taskId,
        turnIndex,
        author: author.trim() || "anonymous",
        body: body.trim(),
      });
      setBody("");
      setNeedsKey(!getCommentsKey());
    } catch (e) {
      setNeedsKey(!getCommentsKey());
      setError(
        String(e).includes("invalid comments password")
          ? "wrong password"
          : "couldn't post — try again",
      );
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="mt-2 space-y-2">
      {comments.map((c) => (
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
          <p className="text-sm text-atb-ink leading-relaxed whitespace-pre-wrap">
            {c.body}
          </p>
        </div>
      ))}

      <div className="rounded-lg border border-atb-line bg-atb-cloud px-3 py-2">
        <textarea
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
          {needsKey && (
            <input
              type="password"
              value={key}
              onChange={(e) => setKey(e.target.value)}
              placeholder="comments password"
              className="w-40 rounded border border-atb-line bg-atb-ivory/60 px-2 py-1 font-atb-mono text-[11px] text-atb-ink placeholder:text-atb-ink-3 outline-none"
            />
          )}
          <button
            onClick={submit}
            disabled={busy || !body.trim()}
            className="ml-auto rounded-full bg-atb-ink px-3 py-1 text-[11px] font-medium text-atb-cloud transition-opacity disabled:opacity-40"
          >
            {busy ? "posting…" : "post"}
          </button>
        </div>
        {error && (
          <p className="mt-1 font-atb-mono text-[11px] text-atb-rust">{error}</p>
        )}
      </div>
    </div>
  );
}
