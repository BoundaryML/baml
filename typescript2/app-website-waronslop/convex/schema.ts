import { defineSchema, defineTable } from 'convex/server';
import { v } from 'convex/values';

export default defineSchema({
  // War-on-slop pledges: how someone fights slop with slop.
  submissions: defineTable({
    name: v.string(),
    email: v.string(), // collected, never exposed publicly
    description: v.string(),
    createdAt: v.number(),
  }).index('by_created', ['createdAt']),
});
