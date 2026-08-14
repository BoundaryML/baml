import { v } from 'convex/values';
import { mutation } from './_generated/server';

// /bamlcode feedback intake. Only `feedback` is required; name + email optional.
// Mirrors the sanitising/rate-limit approach used by convex/slopPledges.ts.

const MAX_NAME = 120;
const MAX_EMAIL = 254;
const MAX_FEEDBACK = 4000;

const RATE_WINDOW_MS = 60 * 60 * 1000; // 1 hour
const RATE_MAX_PER_WINDOW = 20; // submissions per email (or anon) per window
const COOLDOWN_MS = 5 * 1000; // min gap between submissions

// Control characters to strip. Built from escaped strings so no literal control
// bytes appear in the source. CTRL_KEEP_NL preserves newlines and tabs.
const CTRL_KEEP_NL = new RegExp(
  '[\\u0000-\\u0008\\u000B\\u000C\\u000E-\\u001F\\u007F]',
  'g',
);
const CTRL_ALL = new RegExp('[\\u0000-\\u001F\\u007F]', 'g');

// Strip HTML and control characters, then trim. `keepNewlines` preserves line
// breaks in the feedback body; names/emails collapse to a single line.
function clean(input: string, keepNewlines = false): string {
  const noHtml = input.replace(/<[^>]*>/g, '');
  if (keepNewlines) {
    return noHtml
      .replace(CTRL_KEEP_NL, '')
      .replace(/[ \t]+/g, ' ')
      .replace(/\n{3,}/g, '\n\n')
      .trim();
  }
  return noHtml.replace(CTRL_ALL, ' ').replace(/\s+/g, ' ').trim();
}

export const submit = mutation({
  args: {
    feedback: v.string(),
    name: v.optional(v.string()),
    email: v.optional(v.string()),
    slug: v.optional(v.string()),
    // Hidden honeypot: real users never fill it; bots often do.
    website: v.optional(v.string()),
  },
  handler: async (ctx, args) => {
    if (args.website && args.website.length > 0) return { ok: true }; // bot

    const feedback = clean(args.feedback, true).slice(0, MAX_FEEDBACK);
    if (feedback.length === 0) {
      throw new Error('Feedback is required.');
    }
    const name = args.name ? clean(args.name).slice(0, MAX_NAME) : undefined;
    const email = args.email
      ? clean(args.email).toLowerCase().slice(0, MAX_EMAIL)
      : undefined;

    // Light rate limit keyed on email (or "anon" when omitted).
    const key = email && email.length > 0 ? email : 'anon';
    const since = Date.now() - RATE_WINDOW_MS;
    const recent = await ctx.db
      .query('bamlcodeFeedback')
      .withIndex('by_created', (q) => q.gt('createdAt', since))
      .collect();
    const mine = recent.filter((r) => (r.email ?? 'anon') === key);
    if (mine.length >= RATE_MAX_PER_WINDOW) {
      throw new Error('Too many submissions. Please try again later.');
    }
    const last = mine.reduce((m, r) => Math.max(m, r.createdAt), 0);
    if (last && Date.now() - last < COOLDOWN_MS) {
      throw new Error('Please wait a moment before submitting again.');
    }

    await ctx.db.insert('bamlcodeFeedback', {
      feedback,
      name: name && name.length > 0 ? name : undefined,
      email: email && email.length > 0 ? email : undefined,
      slug: args.slug,
      createdAt: Date.now(),
    });
    return { ok: true };
  },
});
