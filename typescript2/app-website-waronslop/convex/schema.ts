import { defineSchema, defineTable } from 'convex/server';
import { v } from 'convex/values';

export default defineSchema({
  // War-on-slop pledges: how someone fights slop with slop.
  submissions: defineTable({
    name: v.string(),
    email: v.string(), // collected, never exposed publicly
    description: v.string(),
    createdAt: v.number(),
    // Moderation gate: undefined/0 = pending review, 1 = approved & shown on the
    // site. Flip to 1 manually in the Convex dashboard to publish a pledge.
    approved: v.optional(v.number()),
  })
    .index('by_created', ['createdAt'])
    .index('by_email', ['email'])
    .index('by_approved', ['approved', 'createdAt']),
});
