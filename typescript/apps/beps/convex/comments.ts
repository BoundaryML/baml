import { query, mutation, internalQuery } from "./_generated/server";
import { v } from "convex/values";
import { commentType } from "./schema";

// ─────────────────────────────────────────────────────────────────────────────
// QUERIES
// ─────────────────────────────────────────────────────────────────────────────

export const byBep = query({
  args: { bepId: v.id("beps") },
  handler: async (ctx, args) => {
    const comments = await ctx.db
      .query("comments")
      .withIndex("by_bep", (q) => q.eq("bepId", args.bepId))
      .collect();

    // Enrich with author info
    const enrichedComments = await Promise.all(
      comments.map(async (comment) => {
        const author = await ctx.db.get(comment.authorId);
        const resolvedByUser = comment.resolvedBy
          ? await ctx.db.get(comment.resolvedBy)
          : null;

        return {
          ...comment,
          authorName: author?.name ?? "Unknown",
          authorAvatarUrl: author?.avatarUrl,
          resolvedByName: resolvedByUser?.name,
        };
      })
    );

    return enrichedComments;
  },
});

export const byBepPage = query({
  args: {
    bepId: v.id("beps"),
    pageId: v.optional(v.id("bepPages")),
    versionId: v.optional(v.id("bepVersions")),
  },
  handler: async (ctx, args) => {
    // Require versionId for proper version-scoped display
    if (!args.versionId) {
      console.warn("byBepPage called without versionId - returning empty array");
      return [];
    }

    // Filter by version - get all comments for this BEP and version, then filter by page
    const versionComments = await ctx.db
      .query("comments")
      .withIndex("by_bep_version", (q) =>
        q.eq("bepId", args.bepId).eq("versionId", args.versionId)
      )
      .collect();
    const comments = versionComments.filter((c) => c.pageId === args.pageId);

    const enrichedComments = await Promise.all(
      comments.map(async (comment) => {
        const author = await ctx.db.get(comment.authorId);
        const resolvedByUser = comment.resolvedBy
          ? await ctx.db.get(comment.resolvedBy)
          : null;
        return {
          ...comment,
          authorName: author?.name ?? "Unknown",
          authorAvatarUrl: author?.avatarUrl,
          resolvedByName: resolvedByUser?.name,
        };
      })
    );

    return enrichedComments;
  },
});

// Get comment counts grouped by page for a BEP
// Returns: { "_main": number, "page-slug": number, ... }
export const countsByPage = query({
  args: {
    bepId: v.id("beps"),
    versionId: v.optional(v.id("bepVersions")),
  },
  handler: async (ctx, args) => {
    let comments;

    if (args.versionId) {
      // Filter by version
      comments = await ctx.db
        .query("comments")
        .withIndex("by_bep_version", (q) =>
          q.eq("bepId", args.bepId).eq("versionId", args.versionId)
        )
        .collect();
    } else {
      // No version specified - return empty counts
      // All callers should now provide a versionId for proper version-scoped display
      console.warn("countsByPage called without versionId - returning empty counts");
      return {};
    }

    // Only count top-level, unresolved comments
    const topLevelUnresolved = comments.filter(
      (c) => !c.parentId && !c.resolved
    );

    const counts: Record<string, number> = {};

    for (const comment of topLevelUnresolved) {
      // Determine the page key
      let pageKey: string;
      if (comment.pageId) {
        // Comment on a page - get the slug
        const page = await ctx.db.get(comment.pageId);
        pageKey = page?.slug ?? "unknown";
      } else {
        // Main content comment (no page)
        pageKey = "_main";
      }

      counts[pageKey] = (counts[pageKey] ?? 0) + 1;
    }

    return counts;
  },
});

export const getReplies = query({
  args: { parentId: v.id("comments") },
  handler: async (ctx, args) => {
    const replies = await ctx.db
      .query("comments")
      .withIndex("by_parent", (q) => q.eq("parentId", args.parentId))
      .collect();

    const enrichedReplies = await Promise.all(
      replies.map(async (reply) => {
        const author = await ctx.db.get(reply.authorId);
        return {
          ...reply,
          authorName: author?.name ?? "Unknown",
          authorAvatarUrl: author?.avatarUrl,
        };
      })
    );

    return enrichedReplies;
  },
});

// Get linked issues and decisions for a comment
export const getLinkedItems = query({
  args: { commentId: v.id("comments") },
  handler: async (ctx, args) => {
    const comment = await ctx.db.get(args.commentId);
    if (!comment) return { issues: [], decisions: [] };

    // Find issues that reference this comment
    const allIssues = await ctx.db
      .query("openIssues")
      .withIndex("by_bep", (q) => q.eq("bepId", comment.bepId))
      .collect();

    const linkedIssues = allIssues.filter(
      (issue) =>
        issue.sourceCommentId === args.commentId ||
        issue.relatedCommentIds?.includes(args.commentId)
    );

    // Find decisions that reference this comment
    const allDecisions = await ctx.db
      .query("decisions")
      .withIndex("by_bep", (q) => q.eq("bepId", comment.bepId))
      .collect();

    const linkedDecisions = allDecisions.filter((decision) =>
      decision.sourceCommentIds.includes(args.commentId)
    );

    return {
      issues: linkedIssues.map((i) => ({
        _id: i._id,
        title: i.title,
        resolved: i.resolved,
      })),
      decisions: linkedDecisions.map((d) => ({
        _id: d._id,
        title: d.title,
      })),
    };
  },
});

// Batch get linked items for multiple comments (more efficient for lists)
export const getLinkedItemsBatch = query({
  args: { bepId: v.id("beps") },
  handler: async (ctx, args) => {
    // Get all issues and decisions for this BEP
    const allIssues = await ctx.db
      .query("openIssues")
      .withIndex("by_bep", (q) => q.eq("bepId", args.bepId))
      .collect();

    const allDecisions = await ctx.db
      .query("decisions")
      .withIndex("by_bep", (q) => q.eq("bepId", args.bepId))
      .collect();

    // Build a map of commentId -> linked items
    const linkedByComment: Record<
      string,
      {
        issues: { _id: string; title: string; resolved: boolean }[];
        decisions: { _id: string; title: string }[];
      }
    > = {};

    // Process issues
    for (const issue of allIssues) {
      const commentIds = new Set<string>();
      if (issue.sourceCommentId) commentIds.add(issue.sourceCommentId);
      if (issue.relatedCommentIds) {
        for (const id of issue.relatedCommentIds) {
          commentIds.add(id);
        }
      }

      for (const commentId of commentIds) {
        if (!linkedByComment[commentId]) {
          linkedByComment[commentId] = { issues: [], decisions: [] };
        }
        linkedByComment[commentId].issues.push({
          _id: issue._id,
          title: issue.title,
          resolved: issue.resolved,
        });
      }
    }

    // Process decisions
    for (const decision of allDecisions) {
      for (const commentId of decision.sourceCommentIds) {
        if (!linkedByComment[commentId]) {
          linkedByComment[commentId] = { issues: [], decisions: [] };
        }
        linkedByComment[commentId].decisions.push({
          _id: decision._id,
          title: decision.title,
        });
      }
    }

    return linkedByComment;
  },
});

// ─────────────────────────────────────────────────────────────────────────────
// MUTATIONS
// ─────────────────────────────────────────────────────────────────────────────

export const add = mutation({
  args: {
    bepId: v.id("beps"),
    versionId: v.id("bepVersions"),
    pageId: v.optional(v.id("bepPages")),
    authorId: v.id("users"),
    parentId: v.optional(v.id("comments")),
    type: commentType,
    content: v.string(),
    anchor: v.optional(
      v.object({
        nodeId: v.string(),
        nodeType: v.string(),
        nodeText: v.string(),
      })
    ),
  },
  handler: async (ctx, args) => {
    const now = Date.now();

    const commentId = await ctx.db.insert("comments", {
      bepId: args.bepId,
      versionId: args.versionId,
      pageId: args.pageId,
      authorId: args.authorId,
      parentId: args.parentId,
      type: args.type,
      content: args.content,
      anchor: args.anchor,
      resolved: false,
      createdAt: now,
      updatedAt: now,
    });

    return commentId;
  },
});

export const update = mutation({
  args: {
    id: v.id("comments"),
    content: v.string(),
  },
  handler: async (ctx, args) => {
    await ctx.db.patch(args.id, {
      content: args.content,
      updatedAt: Date.now(),
    });
  },
});

export const remove = mutation({
  args: { id: v.id("comments") },
  handler: async (ctx, args) => {
    // Also delete all replies
    const replies = await ctx.db
      .query("comments")
      .withIndex("by_parent", (q) => q.eq("parentId", args.id))
      .collect();

    for (const reply of replies) {
      await ctx.db.delete(reply._id);
    }

    await ctx.db.delete(args.id);
  },
});

export const toggleReaction = mutation({
  args: {
    commentId: v.id("comments"),
    userId: v.id("users"),
    emoji: v.union(
      v.literal("thumbsUp"),
      v.literal("thumbsDown"),
      v.literal("heart"),
      v.literal("thinking")
    ),
  },
  handler: async (ctx, args) => {
    const comment = await ctx.db.get(args.commentId);
    if (!comment) throw new Error("Comment not found");

    const reactions = comment.reactions ?? {
      thumbsUp: [],
      thumbsDown: [],
      heart: [],
      thinking: [],
    };

    const currentReactions = reactions[args.emoji] ?? [];
    const userIndex = currentReactions.indexOf(args.userId);

    if (userIndex === -1) {
      // Add reaction
      currentReactions.push(args.userId);
    } else {
      // Remove reaction
      currentReactions.splice(userIndex, 1);
    }

    reactions[args.emoji] = currentReactions;

    await ctx.db.patch(args.commentId, { reactions });
  },
});

export const resolve = mutation({
  args: {
    commentId: v.id("comments"),
    userId: v.id("users"),
  },
  handler: async (ctx, args) => {
    await ctx.db.patch(args.commentId, {
      resolved: true,
      resolvedBy: args.userId,
      resolvedAt: Date.now(),
    });
  },
});

export const unresolve = mutation({
  args: { commentId: v.id("comments") },
  handler: async (ctx, args) => {
    await ctx.db.patch(args.commentId, {
      resolved: false,
      resolvedBy: undefined,
      resolvedAt: undefined,
    });
  },
});

// ─────────────────────────────────────────────────────────────────────────────
// INTERNAL QUERIES (for actions)
// ─────────────────────────────────────────────────────────────────────────────

export const getById = internalQuery({
  args: { id: v.id("comments") },
  handler: async (ctx, args) => {
    const comment = await ctx.db.get(args.id);
    if (!comment) return null;

    const author = await ctx.db.get(comment.authorId);
    return {
      ...comment,
      authorName: author?.name ?? "Unknown",
    };
  },
});

// Get all comments for a specific version of a BEP
export const byVersion = internalQuery({
  args: {
    bepId: v.id("beps"),
    versionId: v.id("bepVersions"),
  },
  handler: async (ctx, args) => {
    const comments = await ctx.db
      .query("comments")
      .withIndex("by_bep_version", (q) =>
        q.eq("bepId", args.bepId).eq("versionId", args.versionId)
      )
      .collect();

    return Promise.all(
      comments.map(async (comment) => {
        const author = await ctx.db.get(comment.authorId);
        return {
          ...comment,
          authorName: author?.name ?? "Unknown",
        };
      })
    );
  },
});
