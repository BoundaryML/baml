import { query, mutation, internalQuery, internalMutation } from "./_generated/server";
import type { MutationCtx, QueryCtx } from "./_generated/server";
import { ConvexError, v } from "convex/values";
import { requireServerSecret } from "./serverSecret";
import { userRole } from "./schema";
import { internal } from "./_generated/api";

// Helper to check if a role has admin-level permissions (bdfl or legacy admin)
function hasAdminPermissions(role: string): boolean {
  return role === "bdfl" || role === "admin";
}

// Helper to check if a role has team-level permissions (team, bdfl, or legacy equivalents)
function hasTeamPermissions(role: string): boolean {
  return role === "bdfl" || role === "team" || role === "admin" || role === "shepherd";
}

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

    // Only BDFL and Team members (or legacy equivalents) can view user management
    if (!hasTeamPermissions(requester.role)) {
      throw new Error("Unauthorized: Only BDFL and Team members can view users");
    }

    const users = await ctx.db.query("users").collect();
    return users;
  },
});

// Check if a user has management permissions (BDFL or Team, including legacy roles)
export const hasManagementPermissions = query({
  args: { userId: v.id("users") },
  handler: async (ctx, args) => {
    const user = await ctx.db.get(args.userId);
    if (!user) {
      return false;
    }
    return hasTeamPermissions(user.role);
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

type GitHubProfile = {
  githubId: string;
  name: string;
  avatarUrl?: string;
  // A verified @boundaryml.com email, which only the server (the GitHub callback) may give.
  boundaryEmail?: string;
};

/**
 * Finds the user for a GitHub account, or a passkey user of the same name not yet linked to
 * GitHub, creating one if new. Only `trusted` callers (the server, with what GitHub verified)
 * update an existing user.
 */
async function upsertGitHubUser(ctx: MutationCtx, profile: GitHubProfile, trusted: boolean) {
  const isSpecialAccount = trusted && !!profile.boundaryEmail;
  const sameName = await ctx.db
    .query("users")
    .withIndex("by_name", (q) => q.eq("name", profile.name))
    .collect();
  const existing =
    (await ctx.db
      .query("users")
      .withIndex("by_github_id", (q) => q.eq("githubId", profile.githubId))
      .unique()) ??
    // A user from the old passkey login, linked to GitHub on their first GitHub sign-in.
    sameName.find((user) => !user.githubId) ??
    null;

  if (existing && !trusted) return existing._id;
  if (existing) {
    const updates: Record<string, unknown> = {};
    if (existing.githubId !== profile.githubId) updates.githubId = profile.githubId;
    if (profile.avatarUrl && existing.avatarUrl !== profile.avatarUrl) {
      updates.avatarUrl = profile.avatarUrl;
    }
    if (isSpecialAccount) {
      updates.isSpecialAccount = true;
      updates.boundaryEmail = profile.boundaryEmail;
    }
    if (Object.keys(updates).length > 0) await ctx.db.patch(existing._id, updates);
    if (isSpecialAccount && !existing.slackUserId) {
      await ctx.scheduler.runAfter(0, internal.slack.lookupAndLinkSlackUser, {
        userId: existing._id,
        email: profile.boundaryEmail!,
      });
    }
    return existing._id;
  }

  const userId = await ctx.db.insert("users", {
    name: profile.name,
    githubId: profile.githubId,
    avatarUrl: profile.avatarUrl,
    role: "unset",
    createdAt: Date.now(),
    isSpecialAccount,
    boundaryEmail: profile.boundaryEmail,
  });
  if (isSpecialAccount) {
    await ctx.scheduler.runAfter(0, internal.slack.lookupAndLinkSlackUser, {
      userId,
      email: profile.boundaryEmail!,
    });
  }
  return userId;
}

/**
 * The browser's half of GitHub sign-in: the user ID for the account the callback sent it. The
 * browser is not trusted with the team status, so `boundaryEmail` is accepted from older pages but
 * ignored; the callback records it through recordGitHubSignIn.
 */
export const getOrCreateFromGitHub = mutation({
  args: {
    githubId: v.string(),
    name: v.string(),
    avatarUrl: v.optional(v.string()),
    boundaryEmail: v.optional(v.string()),
  },
  handler: async (ctx, args) => {
    return upsertGitHubUser(
      ctx,
      { githubId: args.githubId, name: args.name, avatarUrl: args.avatarUrl },
      false
    );
  },
});

/** The server's half of GitHub sign-in, with what GitHub verified: the account and its email. */
export const recordGitHubSignIn = mutation({
  args: {
    secret: v.string(),
    githubId: v.string(),
    name: v.string(),
    avatarUrl: v.optional(v.string()),
    boundaryEmail: v.optional(v.string()),
  },
  handler: async (ctx, args) => {
    requireServerSecret(args.secret);
    return upsertGitHubUser(
      ctx,
      {
        githubId: args.githubId,
        name: args.name,
        avatarUrl: args.avatarUrl,
        boundaryEmail: args.boundaryEmail,
      },
      true
    );
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

    // Only BDFL and Team members (or legacy equivalents) can update roles
    if (!hasTeamPermissions(requester.role)) {
      throw new Error("Unauthorized: Only BDFL and Team members can update roles");
    }

    // Only BDFL (or legacy admin) can assign BDFL role
    if (args.role === "bdfl" && !hasAdminPermissions(requester.role)) {
      throw new Error("Unauthorized: Only BDFL can assign the BDFL role");
    }

    const targetUser = await ctx.db.get(args.targetUserId);
    if (!targetUser) {
      throw new Error("Target user not found");
    }

    // Cannot change the role of a BDFL (or legacy admin) unless you are also BDFL/admin
    if (hasAdminPermissions(targetUser.role) && !hasAdminPermissions(requester.role)) {
      throw new Error("Unauthorized: Only BDFL can change another BDFL's role");
    }

    await ctx.db.patch(args.targetUserId, { role: args.role });
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

// ─────────────────────────────────────────────────────────────────────────────
// API TOKEN MANAGEMENT
// ─────────────────────────────────────────────────────────────────────────────
// Tokens live in `apiTokens`, which only functions taking the server secret read: the app's
// /api/token route issues them to BoundaryML organization members, checked at GitHub sign-in.

function generateApiToken(): string {
  const chars = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
  const bytes = new Uint8Array(32);
  crypto.getRandomValues(bytes);
  let token = "bep_";
  for (const byte of bytes) token += chars.charAt(byte % chars.length);
  return token;
}

async function userByToken(ctx: QueryCtx, token: string) {
  if (!token) return null;
  const row = await ctx.db
    .query("apiTokens")
    .withIndex("by_token", (q) => q.eq("token", token))
    .unique();
  return row ? await ctx.db.get(row.userId) : null;
}

/**
 * The API token of the user signed in with GitHub account `githubId`, made on first use; with
 * `regenerate`, a new one that replaces it. Only the /api/token route calls this, after checking
 * the viewer is a BoundaryML organization member.
 */
export const apiTokenFor = mutation({
  args: { secret: v.string(), githubId: v.string(), regenerate: v.optional(v.boolean()) },
  handler: async (ctx, args) => {
    requireServerSecret(args.secret);
    const user = await ctx.db
      .query("users")
      .withIndex("by_github_id", (q) => q.eq("githubId", args.githubId))
      .unique();
    if (!user) {
      throw new ConvexError({ kind: "missing", message: "Sign in with GitHub on this site first." });
    }
    const existing = await ctx.db
      .query("apiTokens")
      .withIndex("by_user", (q) => q.eq("userId", user._id))
      .collect();
    if (existing.length > 0 && !args.regenerate) {
      return { token: existing[0].token, isNew: false };
    }
    for (const row of existing) await ctx.db.delete(row._id);
    const token = generateApiToken();
    await ctx.db.insert("apiTokens", { userId: user._id, token, createdAt: Date.now() });
    return { token, isNew: true };
  },
});

// Get user by API token (for authentication) - internal version
export const getByApiToken = internalQuery({
  args: { token: v.string() },
  handler: async (ctx, args) => userByToken(ctx, args.token),
});

// Authenticate via API token - returns user info if valid
export const authenticateWithToken = query({
  args: { token: v.string() },
  handler: async (ctx, args) => {
    const user = await userByToken(ctx, args.token);
    if (!user) return null;
    return { _id: user._id, name: user.name, role: user.role };
  },
});

/**
 * Removes the tokens users had before they moved to `apiTokens`. They no longer work, but user
 * documents still carry them to the browser until this runs once:
 * `npx convex run users:clearLegacyApiTokens --prod`, or Functions in the Convex dashboard.
 */
export const clearLegacyApiTokens = internalMutation({
  args: {},
  handler: async (ctx) => {
    let cleared = 0;
    for (const user of await ctx.db.query("users").collect()) {
      if (user.apiToken !== undefined) {
        await ctx.db.patch(user._id, { apiToken: undefined });
        cleared++;
      }
    }
    return { cleared };
  },
});
