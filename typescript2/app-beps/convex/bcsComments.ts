// Comments on BCS proposals: threads on a page or on one block of a Markdown page, with replies,
// resolved by anyone on the team. Like bcs.ts, every public function takes the server secret, so
// only the app's server, after checking the viewer, can read or write them.
import { v } from "convex/values";
import { mutation, query } from "./_generated/server";
import type { MutationCtx } from "./_generated/server";
import type { Doc, Id } from "./_generated/dataModel";
import { byNumber, fail, outlineOf, tellSlack, versionRow } from "./bcs";
import { requireServerSecret } from "./serverSecret";

/** The longest comment, which keeps a thread readable and a Slack reply within its limits. */
export const MAX_COMMENT_CHARS = 10_000;

function checkBody(body: string): string {
  const trimmed = body.trim();
  if (!trimmed) fail("invalid", "The comment is empty.");
  if (trimmed.length > MAX_COMMENT_CHARS) fail("invalid", "The comment is longer than 10,000 characters.");
  return trimmed;
}

function shown(comment: Doc<"bcsComments">) {
  return {
    id: comment._id,
    page: comment.page,
    version: comment.version,
    anchor: comment.anchor,
    author: comment.author,
    body: comment.body,
    resolved: comment.resolved,
    resolvedBy: comment.resolvedBy,
    resolvedAt: comment.resolvedAt,
    createdAt: comment.createdAt,
  };
}

/**
 * A proposal's comment threads on one page, oldest first, each with its replies; and how many open
 * threads each page has, for the page list.
 */
export const list = query({
  args: { secret: v.string(), number: v.number(), page: v.string() },
  handler: async (ctx, args) => {
    requireServerSecret(args.secret);
    const doc = await byNumber(ctx, args.number);
    if (!doc) return null;
    const all = await ctx.db
      .query("bcsComments")
      .withIndex("by_doc", (q) => q.eq("docId", doc._id))
      .collect();
    const replies = new Map<string, Doc<"bcsComments">[]>();
    const openByPage: Record<string, number> = {};
    for (const comment of all) {
      if (comment.parentId) {
        replies.set(comment.parentId, [...(replies.get(comment.parentId) ?? []), comment]);
      } else if (!comment.resolved) {
        openByPage[comment.page] = (openByPage[comment.page] ?? 0) + 1;
      }
    }
    const threads = all
      .filter((comment) => !comment.parentId && comment.page === args.page)
      .sort((a, b) => a.createdAt - b.createdAt)
      .map((comment) => ({
        ...shown(comment),
        replies: (replies.get(comment._id) ?? []).sort((a, b) => a.createdAt - b.createdAt).map(shown),
      }));
    return { threads, openByPage };
  },
});

/**
 * Where a thread lives: its page, and the version it was started on (which has that page even if a
 * later version removed it), so a bare ?comment= link can open the right page.
 */
export const locate = query({
  args: { secret: v.string(), number: v.number(), commentId: v.string() },
  handler: async (ctx, args) => {
    requireServerSecret(args.secret);
    const doc = await byNumber(ctx, args.number);
    const id = ctx.db.normalizeId("bcsComments", args.commentId);
    const found = doc && id ? await ctx.db.get(id) : null;
    if (!doc || !found || found.docId !== doc._id) return null;
    const thread = found.parentId ? await ctx.db.get(found.parentId) : found;
    return thread ? { id: thread._id, page: thread.page, version: thread.version } : null;
  },
});

async function commentOrFail(ctx: MutationCtx, id: string, docId: Id<"bcsDocs">) {
  const normalized = ctx.db.normalizeId("bcsComments", id);
  const comment = normalized ? await ctx.db.get(normalized) : null;
  if (!comment || comment.docId !== docId) fail("missing", "No such comment.");
  return comment;
}

/** Starts a thread on a page of the version the commenter was reading, or on one of its blocks. */
export const add = mutation({
  args: {
    secret: v.string(),
    number: v.number(),
    page: v.string(),
    version: v.number(),
    anchor: v.optional(v.object({ block: v.string(), text: v.string() })),
    body: v.string(),
    author: v.string(),
    authorGithubId: v.optional(v.string()),
  },
  handler: async (ctx, args) => {
    requireServerSecret(args.secret);
    const doc = await byNumber(ctx, args.number);
    if (!doc) fail("missing", "No such proposal.");
    const row = await versionRow(ctx, doc._id, args.version);
    if (!row || !outlineOf(row, doc.title).some((page) => page.slug === args.page)) {
      fail("missing", "No such page.");
    }
    const body = checkBody(args.body);
    const anchor = args.anchor && {
      block: args.anchor.block.slice(0, 100),
      text: args.anchor.text.trim().slice(0, 300),
    };
    const commentId = await ctx.db.insert("bcsComments", {
      docId: doc._id,
      page: args.page,
      version: args.version,
      anchor,
      author: args.author,
      body,
      resolved: false,
      createdAt: Date.now(),
    });
    await tellSlack(ctx, doc._id, { kind: "comment", commentId }, args.author, args.authorGithubId);
    return commentId;
  },
});

/**
 * Replies to a thread, written while reading `version`; a reply to a reply joins the same thread,
 * and a reply to a resolved thread reopens it, since it asks for attention again.
 */
export const reply = mutation({
  args: {
    secret: v.string(),
    number: v.number(),
    commentId: v.string(),
    version: v.optional(v.number()),
    body: v.string(),
    author: v.string(),
    authorGithubId: v.optional(v.string()),
  },
  handler: async (ctx, args) => {
    requireServerSecret(args.secret);
    const doc = await byNumber(ctx, args.number);
    if (!doc) fail("missing", "No such proposal.");
    const target = await commentOrFail(ctx, args.commentId, doc._id);
    const thread = target.parentId ? await commentOrFail(ctx, target.parentId, doc._id) : target;
    const body = checkBody(args.body);
    const version = args.version ?? doc.currentVersion;
    if (!(await versionRow(ctx, doc._id, version))) fail("missing", "No such version.");
    if (thread.resolved) {
      await ctx.db.patch(thread._id, { resolved: false, resolvedBy: undefined, resolvedAt: undefined });
    }
    const replyId = await ctx.db.insert("bcsComments", {
      docId: doc._id,
      page: thread.page,
      version,
      parentId: thread._id,
      author: args.author,
      body,
      resolved: false,
      createdAt: Date.now(),
    });
    await tellSlack(ctx, doc._id, { kind: "comment", commentId: replyId }, args.author, args.authorGithubId);
    return replyId;
  },
});

/** Resolves a thread, or reopens it. */
export const setResolved = mutation({
  args: {
    secret: v.string(),
    number: v.number(),
    commentId: v.string(),
    resolved: v.boolean(),
    author: v.string(),
    authorGithubId: v.optional(v.string()),
  },
  handler: async (ctx, args) => {
    requireServerSecret(args.secret);
    const doc = await byNumber(ctx, args.number);
    if (!doc) fail("missing", "No such proposal.");
    const comment = await commentOrFail(ctx, args.commentId, doc._id);
    if (comment.parentId) fail("invalid", "Only a thread's first comment can be resolved.");
    if (comment.resolved === args.resolved) return;
    await ctx.db.patch(comment._id, {
      resolved: args.resolved,
      resolvedBy: args.resolved ? args.author : undefined,
      resolvedAt: args.resolved ? Date.now() : undefined,
    });
    await tellSlack(
      ctx,
      doc._id,
      { kind: "resolved", commentId: comment._id, resolved: args.resolved },
      args.author,
      args.authorGithubId
    );
  },
});
