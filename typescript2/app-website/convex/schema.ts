import { defineSchema, defineTable } from 'convex/server';
import { v } from 'convex/values';

export default defineSchema({
  // Sheep Council registry — one row per form submission.
  councilSubmissions: defineTable({
    address: v.string(),
    createdAt: v.number(),
    discord: v.string(),
    email: v.string(),
  }),
});
