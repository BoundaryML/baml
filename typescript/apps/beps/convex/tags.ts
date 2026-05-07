import { query, mutation } from "./_generated/server";
import { v } from "convex/values";

// Helper to check if a role has team-level permissions (team, bdfl, or legacy equivalents)
function hasTeamPermissions(role: string): boolean {
  return role === "bdfl" || role === "team" || role === "admin" || role === "shepherd";
}

// ─────────────────────────────────────────────────────────────────────────────
// TAG QUERIES
// ─────────────────────────────────────────────────────────────────────────────

export const list = query({
  args: {},
  handler: async (ctx) => {
    const tags = await ctx.db.query("tags").collect();
    return tags.sort((a, b) => a.name.localeCompare(b.name));
  },
});

export const get = query({
  args: { id: v.id("tags") },
  handler: async (ctx, args) => {
    return await ctx.db.get(args.id);
  },
});

export const getByName = query({
  args: { name: v.string() },
  handler: async (ctx, args) => {
    return await ctx.db
      .query("tags")
      .withIndex("by_name", (q) => q.eq("name", args.name))
      .unique();
  },
});

// Get tags for a specific BEP
export const getForBep = query({
  args: { bepId: v.id("beps") },
  handler: async (ctx, args) => {
    const bepTags = await ctx.db
      .query("bepTags")
      .withIndex("by_bep", (q) => q.eq("bepId", args.bepId))
      .collect();

    const tags = await Promise.all(
      bepTags.map(async (bt) => {
        const tag = await ctx.db.get(bt.tagId);
        return tag;
      })
    );

    return tags.filter((t) => t !== null).sort((a, b) => a!.name.localeCompare(b!.name));
  },
});

// Get tag usage counts
export const listWithCounts = query({
  args: {},
  handler: async (ctx) => {
    const tags = await ctx.db.query("tags").collect();

    const tagsWithCounts = await Promise.all(
      tags.map(async (tag) => {
        const bepTags = await ctx.db
          .query("bepTags")
          .withIndex("by_tag", (q) => q.eq("tagId", tag._id))
          .collect();
        return {
          ...tag,
          bepCount: bepTags.length,
        };
      })
    );

    return tagsWithCounts.sort((a, b) => a.name.localeCompare(b.name));
  },
});

// ─────────────────────────────────────────────────────────────────────────────
// TAG MUTATIONS
// ─────────────────────────────────────────────────────────────────────────────

export const create = mutation({
  args: {
    name: v.string(),
    description: v.optional(v.string()),
    color: v.string(),
    requesterId: v.id("users"),
  },
  handler: async (ctx, args) => {
    const requester = await ctx.db.get(args.requesterId);
    if (!requester) {
      throw new Error("User not found");
    }

    if (!hasTeamPermissions(requester.role)) {
      throw new Error("Unauthorized: Only team members and BDFL can manage tags");
    }

    // Check if tag with same name already exists
    const existing = await ctx.db
      .query("tags")
      .withIndex("by_name", (q) => q.eq("name", args.name))
      .unique();

    if (existing) {
      throw new Error("A tag with this name already exists");
    }

    const now = Date.now();
    const tagId = await ctx.db.insert("tags", {
      name: args.name,
      description: args.description,
      color: args.color,
      createdBy: args.requesterId,
      createdAt: now,
      updatedAt: now,
    });

    return tagId;
  },
});

export const update = mutation({
  args: {
    id: v.id("tags"),
    name: v.optional(v.string()),
    description: v.optional(v.string()),
    color: v.optional(v.string()),
    requesterId: v.id("users"),
  },
  handler: async (ctx, args) => {
    const requester = await ctx.db.get(args.requesterId);
    if (!requester) {
      throw new Error("User not found");
    }

    if (!hasTeamPermissions(requester.role)) {
      throw new Error("Unauthorized: Only team members and BDFL can manage tags");
    }

    const tag = await ctx.db.get(args.id);
    if (!tag) {
      throw new Error("Tag not found");
    }

    // If name is being changed, check for duplicates
    if (args.name && args.name !== tag.name) {
      const newName = args.name;
      const existing = await ctx.db
        .query("tags")
        .withIndex("by_name", (q) => q.eq("name", newName))
        .unique();

      if (existing) {
        throw new Error("A tag with this name already exists");
      }
    }

    const updates: Record<string, unknown> = { updatedAt: Date.now() };
    if (args.name !== undefined) updates.name = args.name;
    if (args.description !== undefined) updates.description = args.description;
    if (args.color !== undefined) updates.color = args.color;

    await ctx.db.patch(args.id, updates);
  },
});

export const remove = mutation({
  args: {
    id: v.id("tags"),
    requesterId: v.id("users"),
  },
  handler: async (ctx, args) => {
    const requester = await ctx.db.get(args.requesterId);
    if (!requester) {
      throw new Error("User not found");
    }

    if (!hasTeamPermissions(requester.role)) {
      throw new Error("Unauthorized: Only team members and BDFL can manage tags");
    }

    const tag = await ctx.db.get(args.id);
    if (!tag) {
      throw new Error("Tag not found");
    }

    // Delete all bepTags associations first
    const bepTags = await ctx.db
      .query("bepTags")
      .withIndex("by_tag", (q) => q.eq("tagId", args.id))
      .collect();

    for (const bt of bepTags) {
      await ctx.db.delete(bt._id);
    }

    // Delete the tag
    await ctx.db.delete(args.id);
  },
});

// ─────────────────────────────────────────────────────────────────────────────
// BEP TAG MUTATIONS
// ─────────────────────────────────────────────────────────────────────────────

export const addTagToBep = mutation({
  args: {
    bepId: v.id("beps"),
    tagId: v.id("tags"),
    userId: v.id("users"),
  },
  handler: async (ctx, args) => {
    // Check if association already exists
    const existing = await ctx.db
      .query("bepTags")
      .withIndex("by_bep_tag", (q) => q.eq("bepId", args.bepId).eq("tagId", args.tagId))
      .unique();

    if (existing) {
      return existing._id;
    }

    const bepTagId = await ctx.db.insert("bepTags", {
      bepId: args.bepId,
      tagId: args.tagId,
      addedBy: args.userId,
      addedAt: Date.now(),
    });

    return bepTagId;
  },
});

export const removeTagFromBep = mutation({
  args: {
    bepId: v.id("beps"),
    tagId: v.id("tags"),
  },
  handler: async (ctx, args) => {
    const existing = await ctx.db
      .query("bepTags")
      .withIndex("by_bep_tag", (q) => q.eq("bepId", args.bepId).eq("tagId", args.tagId))
      .unique();

    if (existing) {
      await ctx.db.delete(existing._id);
    }
  },
});

// Set all tags for a BEP (replaces existing tags)
export const setTagsForBep = mutation({
  args: {
    bepId: v.id("beps"),
    tagIds: v.array(v.id("tags")),
    userId: v.id("users"),
  },
  handler: async (ctx, args) => {
    // Get existing tags for this BEP
    const existingBepTags = await ctx.db
      .query("bepTags")
      .withIndex("by_bep", (q) => q.eq("bepId", args.bepId))
      .collect();

    const existingTagIds = new Set(existingBepTags.map((bt) => bt.tagId));
    const newTagIds = new Set(args.tagIds);

    // Remove tags that are no longer in the list
    for (const bepTag of existingBepTags) {
      if (!newTagIds.has(bepTag.tagId)) {
        await ctx.db.delete(bepTag._id);
      }
    }

    // Add new tags
    const now = Date.now();
    for (const tagId of args.tagIds) {
      if (!existingTagIds.has(tagId)) {
        await ctx.db.insert("bepTags", {
          bepId: args.bepId,
          tagId,
          addedBy: args.userId,
          addedAt: now,
        });
      }
    }
  },
});
