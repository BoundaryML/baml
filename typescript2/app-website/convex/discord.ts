import { v } from 'convex/values';

import { internal } from './_generated/api';
import type { Doc } from './_generated/dataModel';
import { internalAction, internalQuery } from './_generated/server';
import { internalMutation } from './triggers';

// Discord REST API. Bot auth (`Authorization: Bot <token>`), v10.
// Two calls: search the guild for the member by their Discord username, then
// PUT the Sheep Council role onto that member.
const DISCORD_BASE = 'https://discord.com/api/v10';

function discordFetch(token: string, method: string, path: string) {
  return fetch(`${DISCORD_BASE}${path}`, {
    headers: { Authorization: `Bot ${token}` },
    method,
  });
}

// Resolve a Discord username to a guild member's user id. Uses the guild member
// search endpoint (requires the bot's privileged SERVER MEMBERS intent, and the
// person must already be in the server). Returns undefined if not found — the
// `discord` field is a free-text username, so a miss is expected and recorded,
// not thrown as a hard failure.
async function resolveUserId(
  token: string,
  guildId: string,
  username: string,
): Promise<string | undefined> {
  const q = encodeURIComponent(username.replace(/^@/, '').trim());
  const res = await discordFetch(
    token,
    'GET',
    `/guilds/${guildId}/members/search?query=${q}&limit=10`,
  );
  if (!res.ok) {
    throw new Error(
      `Discord member search failed: ${res.status} ${await res.text()}`,
    );
  }
  const members: { user?: { id: string; username: string } }[] =
    await res.json();
  const wanted = username.replace(/^@/, '').trim().toLowerCase();
  const match =
    members.find((m) => m.user?.username?.toLowerCase() === wanted) ??
    members[0];
  return match?.user?.id;
}

export const getSubmission = internalQuery({
  args: { submissionId: v.id('councilSubmissions') },
  handler: (ctx, { submissionId }) => ctx.db.get(submissionId),
});

export const listPendingRoles = internalQuery({
  args: {},
  handler: (ctx) =>
    ctx.db
      .query('councilSubmissions')
      .withIndex('by_discord_role_sync', (q) =>
        q.eq('discordRoleSyncedAt', undefined),
      )
      .take(20),
});

export const markRoleSynced = internalMutation({
  args: {
    discordUserId: v.string(),
    submissionId: v.id('councilSubmissions'),
  },
  handler: async (ctx, { discordUserId, submissionId }) => {
    await ctx.db.patch(submissionId, {
      discordRoleError: undefined,
      discordRoleSyncedAt: Date.now(),
      discordUserId,
    });
  },
});

export const recordRoleError = internalMutation({
  args: { error: v.string(), submissionId: v.id('councilSubmissions') },
  handler: async (ctx, { error, submissionId }) => {
    const sub = await ctx.db.get(submissionId);
    if (!sub) {
      return;
    }
    await ctx.db.patch(submissionId, {
      discordRoleAttempts: (sub.discordRoleAttempts ?? 0) + 1,
      discordRoleError: error,
    });
  },
});

// Give one submitter the Sheep Council role. Idempotent: no-ops if already
// assigned, so the insert trigger and a manual backfill can both call it.
export const assignRole = internalAction({
  args: { submissionId: v.id('councilSubmissions') },
  handler: async (ctx, { submissionId }) => {
    const token = process.env.DISCORD_BOT_TOKEN;
    if (!token) {
      throw new Error('DISCORD_BOT_TOKEN is not set');
    }
    const guildId = process.env.DISCORD_GUILD_ID;
    const roleId = process.env.DISCORD_SHEEP_COUNCIL_ROLE_ID;
    if (!guildId || !roleId) {
      throw new Error('DISCORD_GUILD_ID / DISCORD_SHEEP_COUNCIL_ROLE_ID not set');
    }

    const sub: Doc<'councilSubmissions'> | null = await ctx.runQuery(
      internal.discord.getSubmission,
      { submissionId },
    );
    if (!sub || sub.discordRoleSyncedAt) {
      return; // gone, or already assigned
    }

    try {
      const userId = await resolveUserId(token, guildId, sub.discord);
      if (!userId) {
        throw new Error(
          `No Discord member found for username "${sub.discord}" (are they in the server?)`,
        );
      }
      const res = await discordFetch(
        token,
        'PUT',
        `/guilds/${guildId}/members/${userId}/roles/${roleId}`,
      );
      // 204 = added, 201/200 also OK; Discord returns 204 No Content on success.
      if (!res.ok) {
        throw new Error(
          `Discord add-role failed: ${res.status} ${await res.text()}`,
        );
      }
      await ctx.runMutation(internal.discord.markRoleSynced, {
        discordUserId: userId,
        submissionId,
      });
    } catch (err) {
      await ctx.runMutation(internal.discord.recordRoleError, {
        error: err instanceof Error ? err.message : String(err),
        submissionId,
      });
      throw err; // surface in logs
    }
  },
});

// On-demand backfill: assign the role to everyone not yet role-synced. NOT wired
// to a cron — run it deliberately (e.g. to grant existing members the role)
// rather than letting a sweep fire automatically. Staggered for rate limits.
export const syncPendingRoles = internalAction({
  args: {},
  handler: async (ctx) => {
    const pending = await ctx.runQuery(internal.discord.listPendingRoles, {});
    let i = 0;
    for (const sub of pending) {
      await ctx.scheduler.runAfter(i * 500, internal.discord.assignRole, {
        submissionId: sub._id,
      });
      i += 1;
    }
  },
});
