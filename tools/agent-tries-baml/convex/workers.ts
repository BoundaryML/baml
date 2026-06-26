// Presence-only table: every long-lived processor heartbeats here so the
// dashboard can show a live roster of agents. NOT a claimable queue — the
// workers table has no queueFields, so it must never be wired into the
// generic queue verbs (claim/transition would 500).

import { query, mutation } from "./_generated/server";
import { v } from "convex/values";
import * as lib from "./lib";

const T = "workers" as const;

/**
 * Fetch one worker presence row by id.
 *
 * @param id - Worker document id to look up.
 * @returns The worker document, or null if it doesn't exist.
 */
export const get = query({
  args: { id: v.string() },
  handler: (ctx, a) => lib.getDoc(ctx, T, a.id),
});

/**
 * List worker presence rows, optionally filtered by role.
 *
 * @param role - Optional role to filter on (scans by_role_status).
 * @param limit - Maximum number of rows to return (defaults to 100).
 * @returns Up to `limit` worker documents.
 */
export const list = query({
  args: { role: v.optional(v.string()), limit: v.optional(v.number()) },
  handler: async (ctx, a) => {
    const limit = a.limit ?? 100;
    if (a.role) {
      return await ctx.db
        .query("workers")
        .withIndex("by_role_status", (q) => q.eq("role", a.role!))
        .take(limit);
    }
    return await ctx.db.query("workers").order("desc").take(limit);
  },
});

/**
 * Upsert a worker's presence heartbeat, keyed by workerId.
 *
 * Patches the existing row when the worker is known, inserts otherwise.
 * Always stamps lastHeartbeat to now.
 *
 * @param workerId - Stable processor identity (role-host-pid-hex).
 * @param role - Processor role (baml_worker, baml_dedup, changelog_worker, ...).
 * @param status - "idle" or "busy".
 * @param currentItemId - The row the worker is processing, when busy.
 * @returns The upserted document id.
 */
export const upsert = mutation({
  args: {
    workerId: v.string(),
    role: v.string(),
    status: v.string(),
    currentItemId: v.optional(v.string()),
  },
  handler: async (ctx, a) => {
    const now = Date.now();
    const existing = await ctx.db
      .query("workers")
      .withIndex("by_worker", (q) => q.eq("workerId", a.workerId))
      .first();
    if (existing) {
      await ctx.db.patch(existing._id, {
        role: a.role,
        status: a.status,
        currentItemId: a.status === "busy" ? a.currentItemId : undefined,
        lastHeartbeat: now,
      });
      return existing._id;
    }
    return await ctx.db.insert("workers", {
      workerId: a.workerId,
      role: a.role,
      status: a.status,
      currentItemId: a.status === "busy" ? a.currentItemId : undefined,
      lastHeartbeat: now,
    });
  },
});
