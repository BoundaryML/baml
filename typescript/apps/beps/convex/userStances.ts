import { query, mutation } from "./_generated/server";
import { v } from "convex/values";
import { userStance } from "./schema";

// ─────────────────────────────────────────────────────────────────────────────
// QUERIES
// ─────────────────────────────────────────────────────────────────────────────

export const getByBepVersion = query({
  args: {
    bepId: v.id("beps"),
    versionId: v.id("bepVersions"),
  },
  handler: async (ctx, args) => {
    const stances = await ctx.db
      .query("userStances")
      .withIndex("by_bep_version", (q) =>
        q.eq("bepId", args.bepId).eq("versionId", args.versionId)
      )
      .collect();

    // Enrich with user info
    return Promise.all(
      stances.map(async (stance) => {
        const user = await ctx.db.get(stance.userId);
        return {
          ...stance,
          userName: user?.name ?? "Unknown",
          userAvatarUrl: user?.avatarUrl,
        };
      })
    );
  },
});

export const getByBep = query({
  args: {
    bepId: v.id("beps"),
  },
  handler: async (ctx, args) => {
    const stances = await ctx.db
      .query("userStances")
      .withIndex("by_bep", (q) => q.eq("bepId", args.bepId))
      .collect();

    // Enrich with user and version info
    return Promise.all(
      stances.map(async (stance) => {
        const [user, version] = await Promise.all([
          ctx.db.get(stance.userId),
          ctx.db.get(stance.versionId),
        ]);
        return {
          ...stance,
          userName: user?.name ?? "Unknown",
          userAvatarUrl: user?.avatarUrl,
          versionNumber: version?.version ?? 0,
        };
      })
    );
  },
});

export const getUserStanceForVersion = query({
  args: {
    bepId: v.id("beps"),
    versionId: v.id("bepVersions"),
    userId: v.id("users"),
  },
  handler: async (ctx, args) => {
    return await ctx.db
      .query("userStances")
      .withIndex("by_version_user", (q) =>
        q.eq("versionId", args.versionId).eq("userId", args.userId)
      )
      .unique();
  },
});

export const getStanceSummary = query({
  args: {
    bepId: v.id("beps"),
    versionId: v.id("bepVersions"),
  },
  handler: async (ctx, args) => {
    const stances = await ctx.db
      .query("userStances")
      .withIndex("by_bep_version", (q) =>
        q.eq("bepId", args.bepId).eq("versionId", args.versionId)
      )
      .collect();

    const summary = {
      support: 0,
      neutral: 0,
      oppose: 0,
      total: stances.length,
    };

    for (const stance of stances) {
      summary[stance.stance]++;
    }

    return summary;
  },
});

export const getHistoricalStances = query({
  args: {
    bepId: v.id("beps"),
  },
  handler: async (ctx, args) => {
    // Get all versions for this BEP
    const versions = await ctx.db
      .query("bepVersions")
      .withIndex("by_bep", (q) => q.eq("bepId", args.bepId))
      .order("asc")
      .collect();

    // Get all stances for this BEP
    const allStances = await ctx.db
      .query("userStances")
      .withIndex("by_bep", (q) => q.eq("bepId", args.bepId))
      .collect();

    // Group stances by version
    const stancesByVersion = new Map<string, typeof allStances>();
    for (const stance of allStances) {
      const existing = stancesByVersion.get(stance.versionId) || [];
      existing.push(stance);
      stancesByVersion.set(stance.versionId, existing);
    }

    // Build historical view
    return Promise.all(
      versions.map(async (version) => {
        const versionStances = stancesByVersion.get(version._id) || [];

        const summary = {
          support: 0,
          neutral: 0,
          oppose: 0,
          total: versionStances.length,
        };

        for (const stance of versionStances) {
          summary[stance.stance]++;
        }

        return {
          versionId: version._id,
          versionNumber: version.version,
          createdAt: version.createdAt,
          stanceSummary: summary,
        };
      })
    );
  },
});

// ─────────────────────────────────────────────────────────────────────────────
// MUTATIONS
// ─────────────────────────────────────────────────────────────────────────────

export const setStance = mutation({
  args: {
    bepId: v.id("beps"),
    versionId: v.id("bepVersions"),
    userId: v.id("users"),
    stance: userStance,
    comment: v.optional(v.string()),
  },
  handler: async (ctx, args) => {
    const now = Date.now();

    // Check if stance already exists for this user/version combination
    const existing = await ctx.db
      .query("userStances")
      .withIndex("by_version_user", (q) =>
        q.eq("versionId", args.versionId).eq("userId", args.userId)
      )
      .unique();

    if (existing) {
      // Update existing stance
      await ctx.db.patch(existing._id, {
        stance: args.stance,
        comment: args.comment,
        updatedAt: now,
      });
      return existing._id;
    } else {
      // Create new stance
      return await ctx.db.insert("userStances", {
        bepId: args.bepId,
        versionId: args.versionId,
        userId: args.userId,
        stance: args.stance,
        comment: args.comment,
        createdAt: now,
        updatedAt: now,
      });
    }
  },
});

export const removeStance = mutation({
  args: {
    stanceId: v.id("userStances"),
  },
  handler: async (ctx, args) => {
    await ctx.db.delete(args.stanceId);
  },
});
