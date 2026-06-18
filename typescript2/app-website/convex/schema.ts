import { defineSchema, defineTable } from 'convex/server';
import { v } from 'convex/values';

export default defineSchema({
  // Sheep Council registry — one row per form submission.
  // The loops*/discordRole* fields track the two onboarding side-effects of an
  // insert (Loops contact + welcome event, and the Discord role). All optional:
  // a row exists from the moment of insert, before either side-effect runs.
  councilSubmissions: defineTable({
    address: v.string(),
    createdAt: v.number(),
    discord: v.string(),
    // Discord role assignment (see convex/discord.ts).
    discordRoleAttempts: v.optional(v.number()),
    discordRoleError: v.optional(v.string()),
    discordRoleSyncedAt: v.optional(v.number()),
    discordUserId: v.optional(v.string()),
    email: v.string(),
    // Optional: legacy rows predate these fields. New submissions always send
    // them (the form requires them; `submit` validates).
    firstName: v.optional(v.string()),
    lastName: v.optional(v.string()),
    // Loops onboarding (see convex/loops.ts).
    loopsAttempts: v.optional(v.number()),
    loopsContactId: v.optional(v.string()),
    loopsError: v.optional(v.string()),
    loopsSyncedAt: v.optional(v.number()),
  }).index('by_discord_role_sync', ['discordRoleSyncedAt']),
});
