import { mutation, query } from './_generated/server';
import { v } from 'convex/values';

const MAX_NAME = 80;
const MAX_EMAIL = 200;
const MAX_DESC = 280;

// Per-email rate limits (no IP is available inside a Convex mutation, so we key
// off the submitted email — combined with the honeypot this stops casual abuse).
const RATE_WINDOW_MS = 60 * 60 * 1000; // 1 hour
const RATE_MAX_PER_WINDOW = 5; // pledges per email per hour
const COOLDOWN_MS = 20 * 1000; // min gap between two pledges from one email

// Strip control characters and any HTML, then collapse whitespace. Keeps stored
// pledges plain-text and safe to render anywhere.
function clean(input: string): string {
  return input
    // eslint-disable-next-line no-control-regex
    .replace(/[\x00-\x1F\x7F]/g, ' ') // control chars
    .replace(/<[^>]*>/g, '') // strip any HTML
    .replace(/\s+/g, ' ') // collapse whitespace
    .trim();
}

// Submit a pledge. `website` is a hidden honeypot — real users never fill it.
// New pledges land with approved=0 and stay hidden until manually approved.
export const submit = mutation({
  args: {
    name: v.string(),
    email: v.string(),
    description: v.string(),
    website: v.optional(v.string()),
  },
  handler: async (ctx, args) => {
    if (args.website && args.website.length > 0) return; // bot — silently drop

    const name = clean(args.name).slice(0, MAX_NAME);
    const email = clean(args.email).toLowerCase();
    const description = clean(args.description).slice(0, MAX_DESC);

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

    // ── Rate limiting (per email) ────────────────────────────────────────────
    const now = Date.now();
    const priorByEmail = await ctx.db
      .query('slopPledges')
      .withIndex('by_email', (q) => q.eq('email', email))
      .collect();

    const recent = priorByEmail.filter((r) => now - r.createdAt < RATE_WINDOW_MS);
    if (recent.length >= RATE_MAX_PER_WINDOW) {
      throw new Error("You've pledged a few times already — please try again later.");
    }
    const lastAt = priorByEmail.reduce((max, r) => Math.max(max, r.createdAt), 0);
    if (now - lastAt < COOLDOWN_MS) {
      throw new Error('Hang on a moment before pledging again.');
    }
    // Drop exact duplicates from the same person.
    if (priorByEmail.some((r) => r.description.toLowerCase() === description.toLowerCase())) {
      throw new Error('Looks like you already shared that one.');
    }

    await ctx.db.insert('slopPledges', {
      name,
      email,
      description,
      createdAt: now,
      approved: 0, // pending manual review
    });
  },
});

// Public list — newest first, WITHOUT emails, only approved pledges.
export const list = query({
  args: {},
  handler: async (ctx) => {
    const rows = await ctx.db
      .query('slopPledges')
      .withIndex('by_approved', (q) => q.eq('approved', 1))
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
