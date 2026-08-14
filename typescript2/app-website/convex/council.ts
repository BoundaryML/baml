import { v } from 'convex/values';

import { query } from './_generated/server';
// Build `submit` from the trigger-wrapped `mutation` so its insert fires the
// councilSubmissions onboarding trigger (see convex/triggers.ts).
import { mutation } from './triggers';

// Validate the council password SERVER-SIDE. The secret lives only in the
// Convex deployment's env (`SHEEP_COUNCIL_PASSWORD`) — it is never shipped to
// the browser. The client sends a guess and gets back only a boolean.
export const checkPassword = query({
  args: { password: v.string() },
  handler: async (_ctx, { password }) => {
    const expected = process.env.SHEEP_COUNCIL_PASSWORD;
    return (
      !!expected &&
      password.trim().toLowerCase() === expected.trim().toLowerCase()
    );
  },
});

// Store one Sheep Council registry submission.
export const submit = mutation({
  // The newer fields are OPTIONAL args so older deployed form versions (which
  // don't send them yet) keep working through the deploy window. The form
  // itself requires first/last name; this just avoids a deploy-ordering break.
  args: {
    address: v.string(),
    discord: v.string(),
    email: v.string(),
    firstName: v.optional(v.string()),
    lastName: v.optional(v.string()),
  },
  handler: async (ctx, args) => {
    await ctx.db.insert('councilSubmissions', {
      address: args.address.trim(),
      createdAt: Date.now(),
      discord: args.discord.trim(),
      email: args.email.trim(),
      firstName: args.firstName?.trim(),
      lastName: args.lastName?.trim(),
    });
  },
});
