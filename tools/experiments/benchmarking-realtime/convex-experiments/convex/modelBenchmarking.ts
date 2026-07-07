import { mutation, query } from "./_generated/server";
import { v } from "convex/values";

const row = v.object({
  model: v.string(),
  provider: v.string(),
  adherence: v.number(),
  commission: v.number(),
  omission: v.number(),
  chunks: v.number(),
  pairs: v.number(),
  slop: v.number(),
  compileErrors: v.number(),
  spendUsd: v.optional(v.number()),
  summary: v.string(),
  slopFindings: v.array(
    v.object({
      chunkId: v.string(),
      cardId: v.string(),
      grade: v.number(),
      evidence: v.string(),
    }),
  ),
  omissions: v.array(
    v.object({
      cardId: v.string(),
      description: v.string(),
    }),
  ),
});

// All benchmark rows, best adherence first. Public — the experiments site
// reads this straight from the browser.
export const list = query({
  args: {},
  handler: async (ctx) => {
    const rows = await ctx.db.query("model_benchmarking").collect();
    return rows.sort((a, b) => b.adherence - a.adherence);
  },
});

// Replace the whole table with a fresh result set (idempotent reseeding after
// a new experiment run). Only touches model_benchmarking.
export const seed = mutation({
  args: { rows: v.array(row) },
  handler: async (ctx, { rows }) => {
    const existing = await ctx.db.query("model_benchmarking").collect();
    for (const doc of existing) {
      await ctx.db.delete(doc._id);
    }
    const at = Date.now();
    for (const r of rows) {
      await ctx.db.insert("model_benchmarking", { ...r, at });
    }
    return rows.length;
  },
});
