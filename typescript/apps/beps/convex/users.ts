import { query, mutation, internalQuery } from "./_generated/server";
import { v } from "convex/values";
import { userRole } from "./schema";

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
    // Validate passkey - supports two passwords
    const passkey1 = process.env.LOGIN_PASSKEY;
    const passkey2 = process.env.LOGIN_PASSKEY_2;
    const validPasskeys = [passkey1, passkey2].filter(Boolean);
    
    if (validPasskeys.length === 0 || !validPasskeys.includes(args.passkey)) {
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

    // Create new user with default role
    const userId = await ctx.db.insert("users", {
      name: args.name,
      role: "member",
      createdAt: Date.now(),
    });

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
      role: args.role ?? "member",
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

export const updateAvatar = mutation({
  args: {
    id: v.id("users"),
    avatarUrl: v.string(),
  },
  handler: async (ctx, args) => {
    await ctx.db.patch(args.id, { avatarUrl: args.avatarUrl });
  },
});
