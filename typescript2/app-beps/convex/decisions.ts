import { query, mutation, internalQuery } from "./_generated/server";
import { v } from "convex/values";

// ─────────────────────────────────────────────────────────────────────────────
// QUERIES
// ─────────────────────────────────────────────────────────────────────────────

export const byBep = query({
  args: { bepId: v.id("beps") },
  handler: async (ctx, args) => {
    const decisions = await ctx.db
      .query("decisions")
      .withIndex("by_bep", (q) => q.eq("bepId", args.bepId))
      .collect();

    // Enrich with participant names and source comments with author info
    const enrichedDecisions = await Promise.all(
      decisions.map(async (decision) => {
        const participants = await Promise.all(
          decision.participants.map((id) => ctx.db.get(id))
        );

        const sourceComments = await Promise.all(
          decision.sourceCommentIds.map(async (id) => {
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
          ...decision,
          participantNames: participants
            .filter((p) => p !== null)
            .map((p) => p!.name),
          sourceComments: sourceComments.filter((c) => c !== null),
        };
      })
    );

    // Sort by decidedAt descending
    return enrichedDecisions.sort((a, b) => b.decidedAt - a.decidedAt);
  },
});

export const get = query({
  args: { id: v.id("decisions") },
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
    description: v.string(),
    rationale: v.optional(v.string()),
    sourceCommentIds: v.array(v.id("comments")),
    participants: v.array(v.id("users")),
  },
  handler: async (ctx, args) => {
    const now = Date.now();

    const decisionId = await ctx.db.insert("decisions", {
      bepId: args.bepId,
      title: args.title,
      description: args.description,
      rationale: args.rationale,
      sourceCommentIds: args.sourceCommentIds,
      participants: args.participants,
      decidedAt: now,
      createdAt: now,
    });

    // Note: We intentionally do NOT auto-resolve comments here.
    // The user can manually resolve them if they want.

    return decisionId;
  },
});

export const createFromComment = mutation({
  args: {
    commentId: v.id("comments"),
    title: v.string(),
    rationale: v.optional(v.string()),
    userId: v.id("users"),
  },
  handler: async (ctx, args) => {
    const comment = await ctx.db.get(args.commentId);
    if (!comment) throw new Error("Comment not found");

    const now = Date.now();

    // Get unique participants from the comment thread
    const participants = new Set<string>([comment.authorId, args.userId]);

    const decisionId = await ctx.db.insert("decisions", {
      bepId: comment.bepId,
      title: args.title,
      description: comment.content,
      rationale: args.rationale,
      sourceCommentIds: [args.commentId],
      participants: Array.from(participants) as typeof comment.authorId[],
      decidedAt: now,
      createdAt: now,
    });

    // Note: We intentionally do NOT auto-resolve the comment here.
    // The user can manually resolve it if they want.

    return decisionId;
  },
});

export const update = mutation({
  args: {
    id: v.id("decisions"),
    title: v.optional(v.string()),
    description: v.optional(v.string()),
    rationale: v.optional(v.string()),
  },
  handler: async (ctx, args) => {
    const updates: Record<string, unknown> = {};
    if (args.title !== undefined) updates.title = args.title;
    if (args.description !== undefined) updates.description = args.description;
    if (args.rationale !== undefined) updates.rationale = args.rationale;

    await ctx.db.patch(args.id, updates);
  },
});

export const remove = mutation({
  args: { id: v.id("decisions") },
  handler: async (ctx, args) => {
    await ctx.db.delete(args.id);
  },
});

export const attachComment = mutation({
  args: {
    id: v.id("decisions"),
    commentId: v.id("comments"),
  },
  handler: async (ctx, args) => {
    const decision = await ctx.db.get(args.id);
    if (!decision) throw new Error("Decision not found");

    if (!decision.sourceCommentIds.includes(args.commentId)) {
      await ctx.db.patch(args.id, {
        sourceCommentIds: [...decision.sourceCommentIds, args.commentId],
      });
    }
  },
});

export const detachComment = mutation({
  args: {
    id: v.id("decisions"),
    commentId: v.id("comments"),
  },
  handler: async (ctx, args) => {
    const decision = await ctx.db.get(args.id);
    if (!decision) throw new Error("Decision not found");

    await ctx.db.patch(args.id, {
      sourceCommentIds: decision.sourceCommentIds.filter((id) => id !== args.commentId),
    });
  },
});

// ─────────────────────────────────────────────────────────────────────────────
// INTERNAL QUERIES (for AI actions)
// ─────────────────────────────────────────────────────────────────────────────

export const byBepInternal = internalQuery({
  args: { bepId: v.id("beps") },
  handler: async (ctx, args) => {
    const decisions = await ctx.db
      .query("decisions")
      .withIndex("by_bep", (q) => q.eq("bepId", args.bepId))
      .collect();

    return Promise.all(
      decisions.map(async (decision) => {
        const participants = await Promise.all(
          decision.participants.map((id) => ctx.db.get(id))
        );
        return {
          ...decision,
          participantNames: participants
            .filter((p) => p !== null)
            .map((p) => p!.name),
        };
      })
    );
  },
});
