"use client";

// A proposal page and its comments: the sandboxed frame, and beside it the page's threads. A
// thread is on the whole page or on one block of a Markdown page; hovering a block in the frame
// offers to start one, and a block's badge or a thread's quote brings the other into view.
import { useActionState, useEffect, useMemo, useRef, useState } from "react";
import { formatDistanceToNow } from "date-fns";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import type { CommentState } from "../actions";
import { BcsFrame, type BcsFrameHandle } from "./frame";

export type Comment = {
  id: string;
  version: number;
  anchor?: { block: string; text: string };
  author: string;
  body: string;
  resolved: boolean;
  resolvedBy?: string;
  createdAt: number;
};
export type Thread = Comment & { replies: Comment[] };

type CommentAction = (previous: CommentState, form: FormData) => Promise<CommentState>;

type Props = {
  title: string;
  document: string;
  number: number;
  /** Ids of the page's commentable blocks; empty for an HTML page, which takes page comments only. */
  blocks: string[];
  version: number;
  threads: Thread[];
  /** The thread a link pointed at (?comment=). */
  focusThread?: string;
  addComment: CommentAction;
  /** Server actions that read the thread from the form's commentId (and resolved). */
  reply: CommentAction;
  resolve: CommentAction;
};

export function BcsReader(props: Props) {
  const frame = useRef<BcsFrameHandle>(null);
  const [open, setOpen] = useState(props.threads.length > 0 || props.focusThread !== undefined);
  const [draft, setDraft] = useState<{ block?: string; text?: string } | null>(null);
  const [focused, setFocused] = useState<string | undefined>(props.focusThread);
  const onPage = useMemo(() => new Set(props.blocks), [props.blocks]);

  const counts = useMemo(() => {
    const result: Record<string, number> = {};
    for (const thread of props.threads) {
      if (!thread.resolved && thread.anchor && onPage.has(thread.anchor.block)) {
        result[thread.anchor.block] = (result[thread.anchor.block] ?? 0) + 1;
      }
    }
    return result;
  }, [props.threads, onPage]);

  // A ?comment= link: show its thread, and its block once the page has loaded.
  useEffect(() => {
    const thread = props.threads.find((each) => each.id === props.focusThread);
    if (!thread?.anchor) return;
    const timer = setTimeout(() => frame.current?.focus(thread.anchor!.block), 800);
    return () => clearTimeout(timer);
  }, [props.focusThread, props.threads]);

  const openThreads = props.threads.filter((thread) => !thread.resolved);
  const resolvedThreads = props.threads.filter((thread) => thread.resolved);

  return (
    <div className="flex flex-1 min-w-0">
      <BcsFrame
        ref={frame}
        title={props.title}
        document={props.document}
        number={props.number}
        counts={counts}
        onComment={(block, text) => {
          setDraft({ block, text });
          setOpen(true);
        }}
        onShowThread={(block) => {
          const thread = openThreads.find((each) => each.anchor?.block === block);
          setOpen(true);
          if (thread) setFocused(thread.id);
        }}
      />
      {open ? (
        <aside aria-label="Comments" className="w-[22rem] shrink-0 border-l flex flex-col min-h-0">
          <div className="flex items-center justify-between border-b px-3 py-2">
            <h2 className="text-sm font-semibold">
              Comments{openThreads.length > 0 ? ` · ${openThreads.length} open` : ""}
            </h2>
            <button
              type="button"
              onClick={() => setOpen(false)}
              className="text-sm text-muted-foreground hover:text-foreground"
            >
              Hide
            </button>
          </div>
          <div className="flex-1 overflow-y-auto p-3 space-y-3">
            {draft ? (
              <Composer
                key={`${draft.block ?? "page"}`}
                action={props.addComment}
                block={draft.block}
                text={draft.text}
                onDone={() => setDraft(null)}
              />
            ) : (
              <Button variant="outline" size="sm" className="w-full" onClick={() => setDraft({})}>
                Comment on this page
              </Button>
            )}
            {props.blocks.length > 0 && !draft && (
              <p className="text-xs text-muted-foreground">
                Or hover a paragraph, list item or diagram and click &ldquo;+ Comment&rdquo;.
              </p>
            )}
            {openThreads.length === 0 && !draft && (
              <p className="text-sm text-muted-foreground">No open comments on this page.</p>
            )}
            {openThreads.map((thread) => (
              <ThreadCard
                key={thread.id}
                thread={thread}
                version={props.version}
                stale={thread.anchor !== undefined && !onPage.has(thread.anchor.block)}
                focused={focused === thread.id}
                onQuote={() => thread.anchor && frame.current?.focus(thread.anchor.block)}
                reply={props.reply}
                resolve={props.resolve}
              />
            ))}
            {resolvedThreads.length > 0 && (
              <details open={resolvedThreads.some((thread) => thread.id === focused)}>
                <summary className="cursor-pointer text-sm text-muted-foreground">
                  Resolved ({resolvedThreads.length})
                </summary>
                <div className="mt-2 space-y-3">
                  {resolvedThreads.map((thread) => (
                    <ThreadCard
                      key={thread.id}
                      thread={thread}
                      version={props.version}
                      stale={thread.anchor !== undefined && !onPage.has(thread.anchor.block)}
                      focused={focused === thread.id}
                      onQuote={() => thread.anchor && frame.current?.focus(thread.anchor.block)}
                      reply={props.reply}
                      resolve={props.resolve}
                    />
                  ))}
                </div>
              </details>
            )}
          </div>
        </aside>
      ) : (
        <button
          type="button"
          onClick={() => setOpen(true)}
          className="shrink-0 border-l px-2 text-xs text-muted-foreground hover:text-foreground [writing-mode:vertical-rl]"
        >
          Comments{openThreads.length > 0 ? ` · ${openThreads.length}` : ""}
        </button>
      )}
    </div>
  );
}

function Refused({ error }: { error?: string }) {
  if (!error) return null;
  return (
    <p role="alert" className="rounded-md border border-destructive/40 bg-destructive/10 p-2 text-xs">
      {error}
    </p>
  );
}

function Quote({ text, stale, onClick }: { text: string; stale: boolean; onClick?: () => void }) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={stale || !onClick}
      className="block w-full text-left border-l-2 border-violet-400 pl-2 text-xs text-muted-foreground line-clamp-3 enabled:hover:text-foreground"
      title={stale ? "This text changed since the comment was written" : "Show it in the page"}
    >
      {text || "A diagram"}
    </button>
  );
}

function Composer({
  action,
  block,
  text,
  onDone,
}: {
  action: CommentAction;
  block?: string;
  text?: string;
  onDone: () => void;
}) {
  const [state, submit, pending] = useActionState(action, {});
  useEffect(() => {
    if (state.posted) onDone();
  }, [state.posted, onDone]);
  return (
    <form action={submit} className="space-y-2 rounded-md border p-2">
      {block && <Quote text={text ?? ""} stale={false} />}
      <Refused error={state.error} />
      <input type="hidden" name="block" value={block ?? ""} />
      <input type="hidden" name="blockText" value={text ?? ""} />
      <Textarea
        name="body"
        required
        autoFocus
        rows={4}
        defaultValue={state.body}
        aria-label={block ? "Comment on this part" : "Comment on this page"}
        placeholder={block ? "Comment on this part…" : "Comment on this page…"}
        className="text-sm"
      />
      <div className="flex gap-2">
        <Button type="submit" size="sm" disabled={pending}>
          {pending ? "Posting…" : "Comment"}
        </Button>
        <Button type="button" size="sm" variant="ghost" onClick={onDone}>
          Cancel
        </Button>
      </div>
    </form>
  );
}

function Meta({ comment, version }: { comment: Comment; version: number }) {
  return (
    <div className="flex flex-wrap items-baseline gap-x-2 text-xs text-muted-foreground">
      <span className="font-medium text-foreground">{comment.author}</span>
      <span>{formatDistanceToNow(comment.createdAt, { addSuffix: true })}</span>
      {comment.version !== version && <span>on v{comment.version}</span>}
    </div>
  );
}

function ThreadCard({
  thread,
  version,
  stale,
  focused,
  onQuote,
  reply,
  resolve,
}: {
  thread: Thread;
  version: number;
  stale: boolean;
  focused: boolean;
  onQuote: () => void;
  reply: CommentAction;
  resolve: CommentAction;
}) {
  const card = useRef<HTMLDivElement>(null);
  const [replying, setReplying] = useState(false);
  const [replyState, submitReply, replyPending] = useActionState(reply, {});
  const [resolveState, submitResolve, resolvePending] = useActionState(resolve, {});

  useEffect(() => {
    if (focused) card.current?.scrollIntoView({ block: "nearest", behavior: "smooth" });
  }, [focused]);
  useEffect(() => {
    if (replyState.posted) setReplying(false);
  }, [replyState.posted]);

  return (
    <div
      ref={card}
      id={`comment-${thread.id}`}
      className={`rounded-md border p-2 space-y-2 ${focused ? "ring-2 ring-violet-500" : ""} ${
        thread.resolved ? "opacity-75" : ""
      }`}
    >
      {thread.anchor && (
        <div className="space-y-1">
          <Quote text={thread.anchor.text} stale={stale} onClick={onQuote} />
          {stale && <p className="text-xs text-muted-foreground">This text changed in a later version.</p>}
        </div>
      )}
      <Meta comment={thread} version={version} />
      <p className="text-sm whitespace-pre-wrap break-words">{thread.body}</p>
      {thread.replies.map((each) => (
        <div key={each.id} className="border-l pl-2 space-y-1">
          <Meta comment={each} version={version} />
          <p className="text-sm whitespace-pre-wrap break-words">{each.body}</p>
        </div>
      ))}
      {thread.resolved && thread.resolvedBy && (
        <p className="text-xs text-muted-foreground">Resolved by {thread.resolvedBy}</p>
      )}
      <Refused error={resolveState.error} />
      {replying ? (
        <form action={submitReply} className="space-y-2">
          <input type="hidden" name="commentId" value={thread.id} />
          <Refused error={replyState.error} />
          <Textarea
            name="body"
            required
            autoFocus
            rows={3}
            defaultValue={replyState.body}
            aria-label="Reply"
            placeholder="Reply…"
            className="text-sm"
          />
          <div className="flex gap-2">
            <Button type="submit" size="sm" disabled={replyPending}>
              {replyPending ? "Replying…" : "Reply"}
            </Button>
            <Button type="button" size="sm" variant="ghost" onClick={() => setReplying(false)}>
              Cancel
            </Button>
          </div>
        </form>
      ) : (
        <div className="flex gap-2">
          <Button type="button" size="sm" variant="ghost" onClick={() => setReplying(true)}>
            Reply
          </Button>
          <form action={submitResolve}>
            <input type="hidden" name="commentId" value={thread.id} />
            <input type="hidden" name="resolved" value={thread.resolved ? "false" : "true"} />
            <Button type="submit" size="sm" variant="ghost" disabled={resolvePending}>
              {thread.resolved ? "Reopen" : "Resolve"}
            </Button>
          </form>
        </div>
      )}
    </div>
  );
}
