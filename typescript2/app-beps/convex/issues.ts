import { query, mutation, internalQuery } from "./_generated/server";
import { v } from "convex/values";

// ─────────────────────────────────────────────────────────────────────────────
// QUERIES
// ─────────────────────────────────────────────────────────────────────────────

export const byBep = query({
  args: { bepId: v.id("beps") },
  handler: async (ctx, args) => {
    const issues = await ctx.db
      .query("openIssues")
      .withIndex("by_bep", (q) => q.eq("bepId", args.bepId))
      .collect();

    // Enrich with user info and comments
    const enrichedIssues = await Promise.all(
      issues.map(async (issue) => {
        const raisedBy = await ctx.db.get(issue.raisedBy);
        const assignedTo = issue.assignedTo
          ? await ctx.db.get(issue.assignedTo)
          : null;
        const sourceComment = issue.sourceCommentId
          ? await ctx.db.get(issue.sourceCommentId)
          : null;

        // Get related comments with author info and version info
        const relatedComments = await Promise.all(
          (issue.relatedCommentIds ?? []).map(async (id) => {
            const comment = await ctx.db.get(id);
            if (!comment) return null;
            const author = await ctx.db.get(comment.authorId);
            // Get version info if available
            const version = comment.versionId
              ? await ctx.db.get(comment.versionId)
              : null;
            return {
              ...comment,
              authorName: author?.name ?? "Unknown",
              versionNumber: version?.version ?? null,
            };
          })
        );

        return {
          ...issue,
          raisedByName: raisedBy?.name ?? "Unknown",
          assignedToName: assignedTo?.name,
          sourceComment,
          relatedComments: relatedComments.filter((c) => c !== null),
        };
      })
    );

    // Sort: unresolved first, then by createdAt descending
    return enrichedIssues.sort((a, b) => {
      if (a.resolved !== b.resolved) {
        return a.resolved ? 1 : -1;
      }
      return b.createdAt - a.createdAt;
    });
  },
});

export const unresolvedByBep = query({
  args: { bepId: v.id("beps") },
  handler: async (ctx, args) => {
    const issues = await ctx.db
      .query("openIssues")
      .withIndex("by_bep_unresolved", (q) =>
        q.eq("bepId", args.bepId).eq("resolved", false)
      )
      .collect();

    const enrichedIssues = await Promise.all(
      issues.map(async (issue) => {
        const raisedBy = await ctx.db.get(issue.raisedBy);
        const assignedTo = issue.assignedTo
          ? await ctx.db.get(issue.assignedTo)
          : null;

        return {
          ...issue,
          raisedByName: raisedBy?.name ?? "Unknown",
          assignedToName: assignedTo?.name,
        };
      })
    );

    return enrichedIssues.sort((a, b) => b.createdAt - a.createdAt);
  },
});

export const get = query({
  args: { id: v.id("openIssues") },
  handler: async (ctx, args) => {
    return await ctx.db.get(args.id);
  },
});

// ─────────────────────────────────────────────────────────────────────────────
// MUTATIONS
// ─────────────────────────────────────────────────────────────────────────────

export const create = mutation({
  args: {
    bepId: v.id("beps"),
    title: v.string(),
    description: v.optional(v.string()),
    raisedBy: v.id("users"),
    assignedTo: v.optional(v.id("users")),
    sourceCommentId: v.optional(v.id("comments")),
  },
  handler: async (ctx, args) => {
    const issueId = await ctx.db.insert("openIssues", {
      bepId: args.bepId,
      title: args.title,
      description: args.description,
      raisedBy: args.raisedBy,
      assignedTo: args.assignedTo,
      sourceCommentId: args.sourceCommentId,
      resolved: false,
      createdAt: Date.now(),
    });

    return issueId;
  },
});

export const createFromComment = mutation({
  args: {
    commentId: v.id("comments"),
    title: v.string(),
    userId: v.id("users"),
  },
  handler: async (ctx, args) => {
    const comment = await ctx.db.get(args.commentId);
    if (!comment) throw new Error("Comment not found");

    const issueId = await ctx.db.insert("openIssues", {
      bepId: comment.bepId,
      title: args.title,
      description: comment.content,
      raisedBy: args.userId,
      sourceCommentId: args.commentId,
      relatedCommentIds: [args.commentId], // Include source comment in related comments
      resolved: false,
      createdAt: Date.now(),
    });

    return issueId;
  },
});

export const update = mutation({
  args: {
    id: v.id("openIssues"),
    title: v.optional(v.string()),
    description: v.optional(v.string()),
    assignedTo: v.optional(v.id("users")),
  },
  handler: async (ctx, args) => {
    const updates: Record<string, unknown> = {};
    if (args.title !== undefined) updates.title = args.title;
    if (args.description !== undefined) updates.description = args.description;
    if (args.assignedTo !== undefined) updates.assignedTo = args.assignedTo;

    await ctx.db.patch(args.id, updates);
  },
});

export const resolve = mutation({
  args: {
    id: v.id("openIssues"),
    resolution: v.string(),
  },
  handler: async (ctx, args) => {
    await ctx.db.patch(args.id, {
      resolved: true,
      resolution: args.resolution,
      resolvedAt: Date.now(),
    });
  },
});

export const reopen = mutation({
  args: { id: v.id("openIssues") },
  handler: async (ctx, args) => {
    await ctx.db.patch(args.id, {
      resolved: false,
      resolution: undefined,
      resolvedAt: undefined,
    });
  },
});

export const remove = mutation({
  args: { id: v.id("openIssues") },
  handler: async (ctx, args) => {
    await ctx.db.delete(args.id);
  },
});

export const attachComment = mutation({
  args: {
    id: v.id("openIssues"),
    commentId: v.id("comments"),
  },
  handler: async (ctx, args) => {
    const issue = await ctx.db.get(args.id);
    if (!issue) throw new Error("Issue not found");

    const relatedCommentIds = issue.relatedCommentIds ?? [];
    if (!relatedCommentIds.includes(args.commentId)) {
      await ctx.db.patch(args.id, {
        relatedCommentIds: [...relatedCommentIds, args.commentId],
      });
    }
  },
});

export const detachComment = mutation({
  args: {
    id: v.id("openIssues"),
    commentId: v.id("comments"),
  },
  handler: async (ctx, args) => {
    const issue = await ctx.db.get(args.id);
    if (!issue) throw new Error("Issue not found");

    const relatedCommentIds = issue.relatedCommentIds ?? [];
    await ctx.db.patch(args.id, {
      relatedCommentIds: relatedCommentIds.filter((id) => id !== args.commentId),
    });
  },
});

// ─────────────────────────────────────────────────────────────────────────────
// INTERNAL QUERIES (for AI actions)
// ─────────────────────────────────────────────────────────────────────────────

export const byBepInternal = internalQuery({
  args: { bepId: v.id("beps") },
  handler: async (ctx, args) => {
    const issues = await ctx.db
      .query("openIssues")
      .withIndex("by_bep", (q) => q.eq("bepId", args.bepId))
      .collect();

    return Promise.all(
      issues.map(async (issue) => {
        const raisedBy = await ctx.db.get(issue.raisedBy);
        return {
          ...issue,
          raisedByName: raisedBy?.name ?? "Unknown",
        };
      })
    );
  },
});
