"use client";

// Comment threads on a run transcript: a per-turn thread (anchored to turn i)
// and a run-level thread. Comments are queued into dedup, which turns
// actionable ones into issue-board tickets.
//
// Flow: existing comments are always visible. "add comment" opens the composer;
// if the shared password isn't stored yet, a small popup asks for it once
// (verified server-side on post — a rejected password re-prompts).

import { useState } from "react";
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
}: {
  trophyId: string;
  taskId?: string;
  turnIndex?: number;
  comments: TranscriptComment[];
}) {
  const addComment = useAddComment();
  const [composing, setComposing] = useState(false);
  const [askKey, setAskKey] = useState(false);
  const [body, setBody] = useState("");
  const [author, setAuthor] = useState(getCommentAuthor());
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  const startComposing = () => {
    setError("");
    if (getCommentsKey()) setComposing(true);
    else setAskKey(true);
  };

  const submit = async () => {
    if (!body.trim() || busy) return;
    setBusy(true);
    setError("");
    try {
      if (author.trim()) setCommentAuthor(author.trim());
      await addComment({
        trophyId,
        taskId,
        turnIndex,
        author: author.trim() || "anonymous",
        body: body.trim(),
      });
      setBody("");
      setComposing(false);
    } catch (e) {
      if (String(e).includes("invalid comments password")) {
        clearCommentsKey();
        setError("wrong password — try again");
        setComposing(false);
        setAskKey(true);
      } else {
        setError("couldn't post — try again");
      }
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

      {!composing && (
        <button
          onClick={startComposing}
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
                onClick={() => setComposing(false)}
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
