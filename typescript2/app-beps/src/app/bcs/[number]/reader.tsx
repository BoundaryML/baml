"use client";

// A proposal page and its comments: the sandboxed frame, and beside it the page's threads. A
// thread is on the whole page or on one block of a Markdown page; hovering a block in the frame
// offers to start one, and a block's badge or a thread's quote brings the other into view.
import { createContext, useActionState, useContext, useEffect, useMemo, useRef, useState } from "react";
import { formatDistanceToNow } from "date-fns";
import { AlertCircle, Brain, Heart, HelpCircle, ThumbsDown, ThumbsUp } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import type { CommentState } from "../actions";
import { BcsFrame, type BcsFrameHandle } from "./frame";

export type Reactions = {
  thumbsUp?: string[];
  thumbsDown?: string[];
  heart?: string[];
  thinking?: string[];
};

export type Comment = {
  id: string;
  version: number;
  anchor?: { block: string; text: string };
  author: string;
  body: string;
  type: "discussion" | "concern" | "question";
  reactions: Reactions;
  editedAt?: number;
  deleted: boolean;
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
  /** Why the comments could not be loaded; the page still shows, without them. */
  commentsError?: string;
  addComment: CommentAction;
  /** Server actions that read the thread from the form's commentId (and resolved). */
  reply: CommentAction;
  resolve: CommentAction;
  /** Server actions on one comment, named by the form's commentId. */
  edit: CommentAction;
  remove: CommentAction;
  react: CommentAction;
  /** The viewer's GitHub login: their own comments can be edited and deleted. */
  viewer: string;
};

/** What every comment needs to offer reactions, and editing and deleting to its author. */
const CommentTools = createContext<{
  edit: CommentAction;
  remove: CommentAction;
  react: CommentAction;
  viewer: string;
  number: number;
} | null>(null);

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

  // A ?comment= link: show its thread, and its block once the page's bridge is listening.
  const linkedBlock = props.threads.find((each) => each.id === props.focusThread)?.anchor?.block;
  const pendingFocus = useRef(linkedBlock);
  const onReady = () => {
    if (pendingFocus.current) frame.current?.focus(pendingFocus.current);
    pendingFocus.current = undefined;
  };

  const tools = useMemo(
    () => ({
      edit: props.edit,
      remove: props.remove,
      react: props.react,
      viewer: props.viewer,
      number: props.number,
    }),
    [props.edit, props.remove, props.react, props.viewer, props.number]
  );
  const openThreads = props.threads.filter((thread) => !thread.resolved);
  const resolvedThreads = props.threads.filter((thread) => thread.resolved);

  return (
    <CommentTools.Provider value={tools}>
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
        onReady={onReady}
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
            {props.commentsError ? (
              <p role="alert" className="rounded-md border border-destructive/40 bg-destructive/10 p-2 text-sm">
                Comments could not be loaded: {props.commentsError}
              </p>
            ) : draft ? (
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
            {props.blocks.length > 0 && !draft && !props.commentsError && (
              <p className="text-xs text-muted-foreground">
                Or hover a paragraph, list item or diagram and click &ldquo;+ Comment&rdquo;, or{" "}
                <button
                  type="button"
                  onClick={() => frame.current?.pick()}
                  className="underline hover:text-foreground"
                >
                  pick a part with the keyboard
                </button>{" "}
                (Tab between parts, Enter to comment, Escape to stop).
              </p>
            )}
            {openThreads.length === 0 && !draft && !props.commentsError && (
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
    </CommentTools.Provider>
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
      <TypeSelect />
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

/** A thread's kind badge, as BEP comments show it. */
export function KindBadge({ type }: { type: Comment["type"] }) {
  if (type === "concern") {
    return (
      <span className="inline-flex items-center gap-1 rounded px-1.5 py-0.5 text-xs text-amber-600 bg-amber-50 dark:bg-amber-950/40">
        <AlertCircle className="h-3 w-3" /> Concern
      </span>
    );
  }
  if (type === "question") {
    return (
      <span className="inline-flex items-center gap-1 rounded px-1.5 py-0.5 text-xs text-blue-600 bg-blue-50 dark:bg-blue-950/40">
        <HelpCircle className="h-3 w-3" /> Question
      </span>
    );
  }
  return null;
}

/** A new thread's kind, as on BEPs. */
function TypeSelect({ value = "discussion" }: { value?: string }) {
  return (
    <select
      name="type"
      defaultValue={value}
      aria-label="Kind"
      className="h-8 rounded-md border bg-background px-2 text-xs"
    >
      <option value="discussion">Discussion</option>
      <option value="concern">Concern (blocks acceptance)</option>
      <option value="question">Question</option>
    </select>
  );
}

function Meta({ comment, version, thread }: { comment: Comment; version: number; thread?: boolean }) {
  return (
    <div className="flex flex-wrap items-center gap-x-2 text-xs text-muted-foreground">
      {thread && <KindBadge type={comment.type} />}
      <span className="font-medium text-foreground">{comment.author}</span>
      <span>{formatDistanceToNow(comment.createdAt, { addSuffix: true })}</span>
      {comment.editedAt && <span title={new Date(comment.editedAt).toLocaleString()}>edited</span>}
      {comment.version !== version && <span>on v{comment.version}</span>}
    </div>
  );
}

/**
 * The reactions BEP comments offer, with their icons and the color of one's own; "thinking", which
 * BCS offered before, shows only on comments that still have it, so it can be taken back.
 */
const REACTIONS = [
  { key: "thumbsUp", Icon: ThumbsUp, mine: "text-blue-500", label: "Thumbs up", always: true },
  { key: "thumbsDown", Icon: ThumbsDown, mine: "text-blue-500", label: "Thumbs down", always: true },
  { key: "heart", Icon: Heart, mine: "text-red-500", label: "Heart", always: true },
  { key: "thinking", Icon: Brain, mine: "text-blue-500", label: "Thinking", always: false },
] as const;

/** A comment's reactions, and the buttons to add or take back the viewer's. */
function ReactionBar({ comment }: { comment: Comment }) {
  const tools = useContext(CommentTools)!;
  const [state, submit, pending] = useActionState(tools.react, {});
  return (
    <form action={submit} className="flex flex-wrap items-center gap-1">
      <input type="hidden" name="commentId" value={comment.id} />
      {REACTIONS.map(({ key, Icon, mine: mineColor, label, always }) => {
        const who = comment.reactions[key] ?? [];
        const mine = who.includes(tools.viewer);
        if (!always && who.length === 0) return null;
        return (
          <Button
            key={key}
            type="submit"
            name="reaction"
            value={key}
            variant="ghost"
            size="sm"
            disabled={pending}
            aria-pressed={mine}
            aria-label={who.length > 0 ? `${label}, ${who.length}` : label}
            title={who.length > 0 ? who.join(", ") : label}
            className={`h-7 px-2 ${mine ? mineColor : "text-muted-foreground"}`}
          >
            <Icon className="h-3.5 w-3.5" />
            {who.length > 0 && <span>{who.length}</span>}
          </Button>
        );
      })}
      <Refused error={state.error} />
    </form>
  );
}

/** A comment's text, with editing and deleting for its author. */
function CommentBody({ comment, thread }: { comment: Comment; thread?: boolean }) {
  const tools = useContext(CommentTools)!;
  const [editing, setEditing] = useState(false);
  const [editState, submitEdit, editPending] = useActionState(tools.edit, {});
  const [removeState, submitRemove, removePending] = useActionState(tools.remove, {});
  useEffect(() => {
    if (editState.posted) setEditing(false);
  }, [editState.posted]);
  if (comment.deleted) {
    return <p className="text-sm italic text-muted-foreground">This comment was deleted.</p>;
  }
  if (editing) {
    return (
      <form action={submitEdit} className="space-y-2">
        <input type="hidden" name="commentId" value={comment.id} />
        <Refused error={editState.error} />
        {thread && <TypeSelect value={comment.type} />}
        <Textarea
          name="body"
          required
          autoFocus
          rows={3}
          defaultValue={editState.body ?? comment.body}
          aria-label="Edit comment"
          className="text-sm"
        />
        <div className="flex gap-2">
          <Button type="submit" size="sm" disabled={editPending}>
            {editPending ? "Saving…" : "Save"}
          </Button>
          <Button type="button" size="sm" variant="ghost" onClick={() => setEditing(false)}>
            Cancel
          </Button>
        </div>
      </form>
    );
  }
  const own = comment.author === tools.viewer;
  return (
    <div className="space-y-1">
      <p className="text-sm whitespace-pre-wrap break-words">{comment.body}</p>
      <div className="flex flex-wrap items-center gap-2">
        <ReactionBar comment={comment} />
        {own && (
          <>
            <button
              type="button"
              onClick={() => setEditing(true)}
              className="text-xs text-muted-foreground hover:text-foreground"
            >
              Edit
            </button>
            <form
              action={submitRemove}
              onSubmit={(event) => {
                if (!confirm("Delete this comment?")) event.preventDefault();
              }}
            >
              <input type="hidden" name="commentId" value={comment.id} />
              <button
                type="submit"
                disabled={removePending}
                className="text-xs text-muted-foreground hover:text-destructive"
              >
                Delete
              </button>
            </form>
          </>
        )}
      </div>
      <Refused error={removeState.error} />
    </div>
  );
}

/** Turning a thread into a recorded decision or an open issue, on their pages. */
function ThreadLinks({ threadId }: { threadId: string }) {
  const tools = useContext(CommentTools)!;
  return (
    <>
      <a
        href={`/bcs/${tools.number}/decisions?from=${threadId}`}
        className="inline-flex h-8 items-center px-2 text-xs text-muted-foreground hover:text-foreground"
      >
        Record decision
      </a>
      <a
        href={`/bcs/${tools.number}/issues?from=${threadId}`}
        className="inline-flex h-8 items-center px-2 text-xs text-muted-foreground hover:text-foreground"
      >
        Open issue
      </a>
    </>
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
      <Meta comment={thread} version={version} thread />
      <CommentBody comment={thread} thread />
      {thread.replies.map((each) => (
        <div key={each.id} className="border-l pl-2 space-y-1">
          <Meta comment={each} version={version} />
          <CommentBody comment={each} />
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
        <div className="flex flex-wrap gap-x-2">
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
          <ThreadLinks threadId={thread.id} />
        </div>
      )}
    </div>
  );
}
