// Decisions and open issues on BCS proposals, as on BEPs: what was decided and why, and what is
// left to settle. Either can come from a comment thread. Like bcs.ts, every function takes the
// server secret, so only the app's server, after checking the viewer, can reach them.
import { v } from "convex/values";
import { mutation, query } from "./_generated/server";
import type { MutationCtx } from "./_generated/server";
import type { Doc, Id } from "./_generated/dataModel";
import { byNumber, checkLogins, fail, tellSlack } from "./bcs";
import { requireServerSecret } from "./serverSecret";

const MAX_TITLE = 200;
const MAX_TEXT = 10_000;

function text(value: string | undefined, what: string, required: boolean, max = MAX_TEXT) {
  const trimmed = value?.trim() ?? "";
  if (required && !trimmed) fail("invalid", `${what} is empty.`);
  if (trimmed.length > max) fail("invalid", `${what} is longer than ${max.toLocaleString("en")} characters.`);
  return trimmed || undefined;
}

async function docOrFail(ctx: MutationCtx, number: number) {
  const doc = await byNumber(ctx, number);
  if (!doc) fail("missing", "No such proposal.");
  return doc;
}

/** A comment thread of this proposal, from any of its comments: the first comment's id and its authors. */
async function thread(ctx: MutationCtx, docId: Id<"bcsDocs">, commentId: string) {
  const id = ctx.db.normalizeId("bcsComments", commentId);
  const found = id ? await ctx.db.get(id) : null;
  if (!found || found.docId !== docId) fail("missing", "No such comment.");
  const root = found.parentId ? await ctx.db.get(found.parentId) : found;
  if (!root) fail("missing", "No such comment.");
  const replies = await ctx.db
    .query("bcsComments")
    .withIndex("by_parent", (q) => q.eq("parentId", root._id))
    .collect();
  return { id: root._id, authors: [root.author, ...replies.map((each) => each.author)] };
}

async function countOpenIssues(ctx: MutationCtx, docId: Id<"bcsDocs">) {
  const issues = await ctx.db
    .query("bcsIssues")
    .withIndex("by_doc", (q) => q.eq("docId", docId))
    .collect();
  await ctx.db.patch(docId, { openIssues: issues.filter((each) => !each.resolved).length });
}

/** A proposal's decisions, newest first, and its issues, open ones first. */
export const list = query({
  args: { secret: v.string(), number: v.number() },
  handler: async (ctx, args) => {
    requireServerSecret(args.secret);
    const doc = await byNumber(ctx, args.number);
    if (!doc) return null;
    const decisions = await ctx.db
      .query("bcsDecisions")
      .withIndex("by_doc", (q) => q.eq("docId", doc._id))
      .collect();
    const issues = await ctx.db
      .query("bcsIssues")
      .withIndex("by_doc", (q) => q.eq("docId", doc._id))
      .collect();
    const decision = (each: Doc<"bcsDecisions">) => ({
      id: each._id,
      title: each.title,
      description: each.description,
      rationale: each.rationale,
      sourceCommentIds: each.sourceCommentIds,
      participants: each.participants,
      decidedBy: each.decidedBy,
      decidedAt: each.decidedAt,
    });
    const issue = (each: Doc<"bcsIssues">) => ({
      id: each._id,
      title: each.title,
      description: each.description,
      raisedBy: each.raisedBy,
      assignedTo: each.assignedTo,
      sourceCommentId: each.sourceCommentId,
      resolved: each.resolved,
      resolution: each.resolution,
      resolvedBy: each.resolvedBy,
      resolvedAt: each.resolvedAt,
      createdAt: each.createdAt,
    });
    return {
      decisions: decisions.sort((a, b) => b.decidedAt - a.decidedAt).map(decision),
      issues: issues
        .sort((a, b) => Number(a.resolved) - Number(b.resolved) || b.createdAt - a.createdAt)
        .map(issue),
    };
  },
});

/** Records a decision, from a comment thread when one is named; its authors become participants. */
export const decide = mutation({
  args: {
    secret: v.string(),
    number: v.number(),
    title: v.string(),
    description: v.string(),
    rationale: v.optional(v.string()),
    sourceCommentId: v.optional(v.string()),
    author: v.string(),
    authorGithubId: v.optional(v.string()),
  },
  handler: async (ctx, args) => {
    requireServerSecret(args.secret);
    const doc = await docOrFail(ctx, args.number);
    const title = text(args.title, "The title", true, MAX_TITLE)!;
    const description = text(args.description, "The decision", true)!;
    const rationale = text(args.rationale, "The rationale", false);
    const source = args.sourceCommentId ? await thread(ctx, doc._id, args.sourceCommentId) : null;
    const now = Date.now();
    const id = await ctx.db.insert("bcsDecisions", {
      docId: doc._id,
      title,
      description,
      rationale,
      sourceCommentIds: source ? [source.id] : [],
      participants: [...new Set([args.author, ...(source?.authors ?? [])])],
      decidedBy: args.author,
      decidedAt: now,
      createdAt: now,
    });
    await tellSlack(ctx, doc._id, { kind: "decision", title }, args.author, args.authorGithubId);
    return id;
  },
});

/** Removes a decision recorded by mistake. */
export const removeDecision = mutation({
  args: { secret: v.string(), number: v.number(), decisionId: v.string() },
  handler: async (ctx, args) => {
    requireServerSecret(args.secret);
    const doc = await docOrFail(ctx, args.number);
    const id = ctx.db.normalizeId("bcsDecisions", args.decisionId);
    const found = id ? await ctx.db.get(id) : null;
    if (!found || found.docId !== doc._id) fail("missing", "No such decision.");
    await ctx.db.delete(found._id);
  },
});

/** Opens an issue, from a comment thread when one is named. */
export const openIssue = mutation({
  args: {
    secret: v.string(),
    number: v.number(),
    title: v.string(),
    description: v.optional(v.string()),
    assignedTo: v.optional(v.string()),
    sourceCommentId: v.optional(v.string()),
    author: v.string(),
    authorGithubId: v.optional(v.string()),
  },
  handler: async (ctx, args) => {
    requireServerSecret(args.secret);
    const doc = await docOrFail(ctx, args.number);
    const title = text(args.title, "The title", true, MAX_TITLE)!;
    const description = text(args.description, "The description", false);
    const assignedTo = args.assignedTo ? checkLogins([args.assignedTo], "assignee")[0] : undefined;
    const source = args.sourceCommentId ? await thread(ctx, doc._id, args.sourceCommentId) : null;
    const id = await ctx.db.insert("bcsIssues", {
      docId: doc._id,
      title,
      description,
      raisedBy: args.author,
      assignedTo,
      sourceCommentId: source?.id,
      resolved: false,
      createdAt: Date.now(),
    });
    await countOpenIssues(ctx, doc._id);
    await tellSlack(ctx, doc._id, { kind: "issue", title, change: "opened" }, args.author, args.authorGithubId);
    return id;
  },
});

/** Resolves an issue with a resolution, or reopens it; or changes who it is assigned to. */
export const updateIssue = mutation({
  args: {
    secret: v.string(),
    number: v.number(),
    issueId: v.string(),
    resolved: v.optional(v.boolean()),
    resolution: v.optional(v.string()),
    assignedTo: v.optional(v.string()),
    author: v.string(),
    authorGithubId: v.optional(v.string()),
  },
  handler: async (ctx, args) => {
    requireServerSecret(args.secret);
    const doc = await docOrFail(ctx, args.number);
    const id = ctx.db.normalizeId("bcsIssues", args.issueId);
    const issue = id ? await ctx.db.get(id) : null;
    if (!issue || issue.docId !== doc._id) fail("missing", "No such issue.");
    const patch: Partial<Doc<"bcsIssues">> = {};
    if (args.assignedTo !== undefined) {
      patch.assignedTo = args.assignedTo.trim() ? checkLogins([args.assignedTo], "assignee")[0] : undefined;
    }
    let change: "resolved" | "reopened" | undefined;
    if (args.resolved === true && !issue.resolved) {
      const resolution = text(args.resolution, "The resolution", true)!;
      Object.assign(patch, { resolved: true, resolution, resolvedBy: args.author, resolvedAt: Date.now() });
      change = "resolved";
    } else if (args.resolved === false && issue.resolved) {
      Object.assign(patch, { resolved: false, resolution: undefined, resolvedBy: undefined, resolvedAt: undefined });
      change = "reopened";
    }
    await ctx.db.patch(issue._id, patch);
    if (change) {
      await countOpenIssues(ctx, doc._id);
      await tellSlack(
        ctx,
        doc._id,
        { kind: "issue", title: issue.title, change, resolution: patch.resolution },
        args.author,
        args.authorGithubId
      );
    }
  },
});

/** Removes an issue opened by mistake. */
export const removeIssue = mutation({
  args: { secret: v.string(), number: v.number(), issueId: v.string() },
  handler: async (ctx, args) => {
    requireServerSecret(args.secret);
    const doc = await docOrFail(ctx, args.number);
    const id = ctx.db.normalizeId("bcsIssues", args.issueId);
    const found = id ? await ctx.db.get(id) : null;
    if (!found || found.docId !== doc._id) fail("missing", "No such issue.");
    await ctx.db.delete(found._id);
    await countOpenIssues(ctx, doc._id);
  },
});
