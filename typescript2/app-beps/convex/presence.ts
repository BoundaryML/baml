import { query, mutation } from "./_generated/server";
import { v } from "convex/values";

// Presence timeout in milliseconds (30 seconds)
const PRESENCE_TIMEOUT = 30000;

// Get active viewers for a BEP
export const getViewers = query({
  args: { bepId: v.id("beps") },
  handler: async (ctx, args) => {
    const cutoff = Date.now() - PRESENCE_TIMEOUT;

    const presenceRecords = await ctx.db
      .query("presence")
      .withIndex("by_bep", (q) => q.eq("bepId", args.bepId))
      .collect();

    // Filter to active viewers and get user info
    const activeRecords = presenceRecords.filter((p) => p.lastSeen > cutoff);

    const viewers = await Promise.all(
      activeRecords.map(async (p) => {
        const user = await ctx.db.get(p.userId);
        return user
          ? {
              userId: p.userId,
              name: user.name,
              avatarUrl: user.avatarUrl,
            }
          : null;
      })
    );

    return viewers.filter((v) => v !== null);
  },
});

// Update presence (heartbeat)
export const heartbeat = mutation({
  args: {
    bepId: v.id("beps"),
    userId: v.id("users"),
  },
  handler: async (ctx, args) => {
    const now = Date.now();

    // Check if presence record exists
    const existing = await ctx.db
      .query("presence")
      .withIndex("by_user_bep", (q) =>
        q.eq("userId", args.userId).eq("bepId", args.bepId)
      )
      .unique();

    if (existing) {
      // Update existing record
      await ctx.db.patch(existing._id, { lastSeen: now });
    } else {
      // Create new record
      await ctx.db.insert("presence", {
        bepId: args.bepId,
        userId: args.userId,
        lastSeen: now,
      });
    }
  },
});

// Remove presence (when leaving page)
export const leave = mutation({
  args: {
    bepId: v.id("beps"),
    userId: v.id("users"),
  },
  handler: async (ctx, args) => {
    const existing = await ctx.db
      .query("presence")
      .withIndex("by_user_bep", (q) =>
        q.eq("userId", args.userId).eq("bepId", args.bepId)
      )
      .unique();

    if (existing) {
      await ctx.db.delete(existing._id);
    }
  },
});
