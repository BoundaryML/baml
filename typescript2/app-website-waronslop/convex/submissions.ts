import { mutation, query } from './_generated/server';
import { v } from 'convex/values';

const MAX_NAME = 80;
const MAX_EMAIL = 200;
const MAX_DESC = 280;

// Submit a pledge. `website` is a hidden honeypot — real users never fill it.
export const submit = mutation({
  args: {
    name: v.string(),
    email: v.string(),
    description: v.string(),
    website: v.optional(v.string()),
  },
  handler: async (ctx, args) => {
    if (args.website && args.website.length > 0) return; // bot — silently drop

    const name = args.name.trim().slice(0, MAX_NAME);
    const email = args.email.trim();
    const description = args.description.trim().slice(0, MAX_DESC);

    if (!name || !email || !description) {
      throw new Error('Name, email, and description are all required.');
    }
    // Reject (don't silently truncate) an oversized email — truncation would
    // corrupt the address into a different, valid-looking one.
    if (email.length > MAX_EMAIL) {
      throw new Error('Please enter a shorter email.');
    }
    if (!email.includes('@')) {
      throw new Error('Please enter a valid email.');
    }

    await ctx.db.insert('submissions', {
      name,
      email,
      description,
      createdAt: Date.now(),
    });
  },
});

// Public list — newest first, WITHOUT emails.
export const list = query({
  args: {},
  handler: async (ctx) => {
    const rows = await ctx.db
      .query('submissions')
      .withIndex('by_created')
      .order('desc')
      .take(100);
    return rows.map((r) => ({
      id: r._id,
      name: r.name,
      description: r.description,
      createdAt: r.createdAt,
    }));
  },
});
