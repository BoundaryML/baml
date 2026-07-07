import { mutation, query } from "./_generated/server";
import { v } from "convex/values";

export const log = mutation({
  args: {
    source: v.string(),
    suite: v.optional(v.string()),
    caseId: v.optional(v.string()),
    agent: v.string(),
    utterance: v.string(),
    toolCalls: v.array(
      v.object({
        name: v.string(),
        args: v.any(),
        tool: v.string(),
        data: v.any(),
        say: v.string(),
      }),
    ),
    finalText: v.string(),
    ms: v.union(v.number(), v.null()),
    pass: v.optional(v.boolean()),
    detail: v.optional(v.string()),
    realtimeModel: v.string(),
    thinkerModel: v.optional(v.string()),
  },
  handler: async (ctx, args) => {
    return await ctx.db.insert("turns", { ...args, at: Date.now() });
  },
});

export const recent = query({
  args: { limit: v.optional(v.number()) },
  handler: async (ctx, { limit }) => {
    return await ctx.db
      .query("turns")
      .withIndex("by_source")
      .order("desc")
      .take(limit ?? 50);
  },
});
