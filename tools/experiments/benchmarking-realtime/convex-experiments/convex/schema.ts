import { defineSchema, defineTable } from "convex/server";
import { v } from "convex/values";

export default defineSchema({
  // One row per user turn per agent lane (arena) or per benchmark case (compare).
  turns: defineTable({
    source: v.string(), // "arena" | "compare"
    suite: v.optional(v.string()), // compare suite name
    caseId: v.optional(v.string()),
    agent: v.string(), // "baml" | "native"
    utterance: v.string(),
    toolCalls: v.array(
      v.object({
        name: v.string(),
        args: v.any(),
        tool: v.string(),
        data: v.any(),
        say: v.string(),
      }),
    ),
    finalText: v.string(),
    ms: v.union(v.number(), v.null()),
    pass: v.optional(v.boolean()),
    detail: v.optional(v.string()),
    realtimeModel: v.string(),
    thinkerModel: v.optional(v.string()),
    at: v.number(),
  }).index("by_source", ["source", "at"]),

  // One row per builder model in the adherence-by-model benchmark
  // (tools/experiments/benchmark-models). Seeded by modelBenchmarking.seed.
  model_benchmarking: defineTable({
    model: v.string(),
    provider: v.string(),
    adherence: v.number(),
    commission: v.number(),
    omission: v.number(),
    chunks: v.number(),
    pairs: v.number(),
    slop: v.number(),
    compileErrors: v.number(), // 0 = compiles clean
    spendUsd: v.optional(v.number()),
    summary: v.string(), // per-model writeup shown on the site
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
    at: v.number(),
  }).index("by_adherence", ["adherence"]),
});
