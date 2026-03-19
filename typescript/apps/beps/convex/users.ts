import { query, mutation, internalQuery, internalMutation } from "./_generated/server";
import { v } from "convex/values";
import { userRole } from "./schema";
import { internal } from "./_generated/api";

// ─────────────────────────────────────────────────────────────────────────────
// QUERIES
// ─────────────────────────────────────────────────────────────────────────────

export const getByName = query({
  args: { name: v.string() },
  handler: async (ctx, args) => {
    return await ctx.db
      .query("users")
      .withIndex("by_name", (q) => q.eq("name", args.name))
      .unique();
  },
});

export const get = query({
  args: { id: v.id("users") },
  handler: async (ctx, args) => {
    return await ctx.db.get(args.id);
  },
});

export const list = query({
  args: {},
  handler: async (ctx) => {
    return await ctx.db.query("users").collect();
  },
});

// List all users - requires BDFL or Team role
export const listForManagement = query({
  args: { requesterId: v.id("users") },
  handler: async (ctx, args) => {
    const requester = await ctx.db.get(args.requesterId);
    if (!requester) {
      throw new Error("Requester not found");
    }

    // Only BDFL and Team members can view user management
    if (requester.role !== "bdfl" && requester.role !== "team") {
      throw new Error("Unauthorized: Only BDFL and Team members can view users");
    }

    const users = await ctx.db.query("users").collect();
    return users;
  },
});

// Check if a user has management permissions (BDFL or Team)
export const hasManagementPermissions = query({
  args: { userId: v.id("users") },
  handler: async (ctx, args) => {
    const user = await ctx.db.get(args.userId);
    if (!user) {
      return false;
    }
    return user.role === "bdfl" || user.role === "team";
  },
});

// Internal query for use in actions
export const getById = internalQuery({
  args: { id: v.id("users") },
  handler: async (ctx, args) => {
    return await ctx.db.get(args.id);
  },
});

// ─────────────────────────────────────────────────────────────────────────────
// MUTATIONS
// ─────────────────────────────────────────────────────────────────────────────

export const getOrCreate = mutation({
  args: {
    name: v.string(),
    passkey: v.string(),
  },
  handler: async (ctx, args) => {
    // Validate passkey
    const expectedPasskey = process.env.LOGIN_PASSKEY;
    if (!expectedPasskey || args.passkey !== expectedPasskey) {
      throw new Error("Invalid passkey");
    }

    // Check if user already exists
    const existing = await ctx.db
      .query("users")
      .withIndex("by_name", (q) => q.eq("name", args.name))
      .unique();

    if (existing) {
      return existing._id;
    }

    // Create new user with default role (unset)
    const userId = await ctx.db.insert("users", {
      name: args.name,
      role: "unset",
      createdAt: Date.now(),
    });

    return userId;
  },
});

export const getOrCreateFromGitHub = mutation({
  args: {
    githubId: v.string(),
    name: v.string(),
    avatarUrl: v.optional(v.string()),
    boundaryEmail: v.optional(v.string()),
  },
  handler: async (ctx, args) => {
    const isSpecialAccount = !!args.boundaryEmail;

    // Check if user exists by GitHub ID
    const existingByGithub = await ctx.db
      .query("users")
      .withIndex("by_github_id", (q) => q.eq("githubId", args.githubId))
      .unique();

    if (existingByGithub) {
      const updates: Record<string, unknown> = {};

      // Update avatar if changed
      if (args.avatarUrl && existingByGithub.avatarUrl !== args.avatarUrl) {
        updates.avatarUrl = args.avatarUrl;
      }

      // Update special account status and boundaryEmail if they have one
      if (isSpecialAccount) {
        updates.isSpecialAccount = true;
        updates.boundaryEmail = args.boundaryEmail;
      }

      if (Object.keys(updates).length > 0) {
        await ctx.db.patch(existingByGithub._id, updates);
      }

      // Schedule Slack lookup if special account and no slackUserId yet
      if (isSpecialAccount && !existingByGithub.slackUserId) {
        await ctx.scheduler.runAfter(0, internal.slack.lookupAndLinkSlackUser, {
          userId: existingByGithub._id,
          email: args.boundaryEmail!,
        });
      }

      return existingByGithub._id;
    }

    // Check if user exists by name (for migration from passkey auth)
    const existingByName = await ctx.db
      .query("users")
      .withIndex("by_name", (q) => q.eq("name", args.name))
      .unique();

    if (existingByName) {
      // Link existing user to GitHub
      const updates: Record<string, unknown> = {
        githubId: args.githubId,
        avatarUrl: args.avatarUrl,
      };

      if (isSpecialAccount) {
        updates.isSpecialAccount = true;
        updates.boundaryEmail = args.boundaryEmail;
      }

      await ctx.db.patch(existingByName._id, updates);

      // Schedule Slack lookup if special account
      if (isSpecialAccount && !existingByName.slackUserId) {
        await ctx.scheduler.runAfter(0, internal.slack.lookupAndLinkSlackUser, {
          userId: existingByName._id,
          email: args.boundaryEmail!,
        });
      }

      return existingByName._id;
    }

    // Create new user with default role (unset)
    const userId = await ctx.db.insert("users", {
      name: args.name,
      githubId: args.githubId,
      avatarUrl: args.avatarUrl,
      role: "unset",
      createdAt: Date.now(),
      isSpecialAccount,
      boundaryEmail: args.boundaryEmail,
    });

    // Schedule Slack lookup if special account
    if (isSpecialAccount) {
      await ctx.scheduler.runAfter(0, internal.slack.lookupAndLinkSlackUser, {
        userId,
        email: args.boundaryEmail!,
      });
    }

    return userId;
  },
});

export const create = mutation({
  args: {
    name: v.string(),
    role: v.optional(userRole),
    avatarUrl: v.optional(v.string()),
  },
  handler: async (ctx, args) => {
    return await ctx.db.insert("users", {
      name: args.name,
      role: args.role ?? "unset",
      avatarUrl: args.avatarUrl,
      createdAt: Date.now(),
    });
  },
});

export const updateRole = mutation({
  args: {
    id: v.id("users"),
    role: userRole,
  },
  handler: async (ctx, args) => {
    await ctx.db.patch(args.id, { role: args.role });
  },
});

// Update user role - requires BDFL or Team role
export const updateRoleWithPermissions = mutation({
  args: {
    requesterId: v.id("users"),
    targetUserId: v.id("users"),
    role: userRole,
  },
  handler: async (ctx, args) => {
    const requester = await ctx.db.get(args.requesterId);
    if (!requester) {
      throw new Error("Requester not found");
    }

    // Only BDFL and Team members can update roles
    if (requester.role !== "bdfl" && requester.role !== "team") {
      throw new Error("Unauthorized: Only BDFL and Team members can update roles");
    }

    // Only BDFL can assign BDFL role
    if (args.role === "bdfl" && requester.role !== "bdfl") {
      throw new Error("Unauthorized: Only BDFL can assign the BDFL role");
    }

    const targetUser = await ctx.db.get(args.targetUserId);
    if (!targetUser) {
      throw new Error("Target user not found");
    }

    // Cannot change the role of a BDFL unless you are also BDFL
    if (targetUser.role === "bdfl" && requester.role !== "bdfl") {
      throw new Error("Unauthorized: Only BDFL can change another BDFL's role");
    }

    await ctx.db.patch(args.targetUserId, { role: args.role });
  },
});

export const updateAvatar = mutation({
  args: {
    id: v.id("users"),
    avatarUrl: v.string(),
  },
  handler: async (ctx, args) => {
    await ctx.db.patch(args.id, { avatarUrl: args.avatarUrl });
  },
});

// Internal mutation to link Slack user ID
export const linkSlackUserId = internalMutation({
  args: {
    userId: v.id("users"),
    slackUserId: v.string(),
  },
  handler: async (ctx, args) => {
    await ctx.db.patch(args.userId, { slackUserId: args.slackUserId });
  },
});
