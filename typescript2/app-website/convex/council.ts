import { v } from 'convex/values';

import { mutation, query } from './_generated/server';

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
  args: {
    address: v.string(),
    discord: v.string(),
    email: v.string(),
  },
  handler: async (ctx, args) => {
    await ctx.db.insert('councilSubmissions', {
      address: args.address.trim(),
      createdAt: Date.now(),
      discord: args.discord.trim(),
      email: args.email.trim(),
    });
  },
});
