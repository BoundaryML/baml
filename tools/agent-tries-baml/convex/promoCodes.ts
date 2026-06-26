import { query, mutation } from "./_generated/server";
import { v } from "convex/values";
import * as lib from "./lib";

const T = "promoCodes" as const;

/**
 * Fetch one promo code row by id.
 *
 * @param id - Code document id to look up.
 * @returns The code document, or null if it doesn't exist.
 */
export const get = query({
  args: { id: v.string() },
  handler: (ctx, a) => lib.getDoc(ctx, T, a.id),
});

/**
 * List promo codes, optionally filtered by an index field/value.
 *
 * @param field - Optional index field to filter on.
 * @param value - Optional value the field must equal.
 * @param index - Optional index name to scan.
 * @param limit - Maximum number of rows to return (defaults to 100).
 * @returns Up to `limit` code documents ordered newest-first.
 */
export const list = query({
  args: { field: v.optional(v.string()), value: v.optional(v.string()), index: v.optional(v.string()), limit: v.optional(v.number()) },
  handler: (ctx, a) => lib.listDocs(ctx, T, a),
});

/**
 * Count codes by status (e.g. how many are still unused).
 *
 * @param field - Field that holds the state (e.g. "status").
 * @param value - The value to match (e.g. "unused").
 * @param index - Index name to scan for matching rows.
 * @returns The number of matching codes, capped at 1000.
 */
export const countClaimable = query({
  args: { field: v.string(), value: v.string(), index: v.string() },
  handler: (ctx, a) => lib.countClaimable(ctx, T, a),
});

/**
 * Insert a new promo code row (used by the one-off SQLite migration).
 *
 * Custom handler (not lib.createDoc): promoCodes has no queueFields, so the
 * generic create's `attempts: 0` default would fail schema validation.
 *
 * @param doc - Field values for the new row.
 * @returns The id of the inserted row.
 */
export const create = mutation({
  args: { doc: v.any() },
  handler: async (ctx, a) => {
    const now = Date.now();
    const doc: Record<string, unknown> = { createdAt: now, updatedAt: now, ...a.doc };
    for (const k of Object.keys(doc)) {
      if (doc[k] === null) delete doc[k]; // None -> absent, like lib.stripNulls
    }
    return await ctx.db.insert("promoCodes", doc as any);
  },
});

/**
 * Patch fields on a promo code row.
 *
 * @param id - Code document id to patch.
 * @param patch - Partial field values to merge into the row.
 * @returns The patched document.
 */
export const update = mutation({
  args: { id: v.string(), patch: v.any() },
  handler: (ctx, a) => lib.updateDoc(ctx, T, a.id, a.patch),
});

/**
 * Atomically claim the next unused promo code (lowest position).
 *
 * One OCC mutation: among N racing claimers exactly one flips a given row
 * to "used", so a code is never issued twice. Mirrors the old t-shirts bot's
 * SQLite claim_next_code.
 *
 * @param claimedBy - Display name of the requesting Slack user.
 * @param claimedByUserId - Slack user id of the requester.
 * @param notes - Free-text audit note (the mention text).
 * @returns The claimed code string, or null when inventory is exhausted.
 */
export const claimNext = mutation({
  args: {
    claimedBy: v.string(),
    claimedByUserId: v.string(),
    notes: v.optional(v.string()),
  },
  handler: async (ctx, a) => {
    const row = await ctx.db
      .query("promoCodes")
      .withIndex("by_status_position", (q) => q.eq("status", "unused"))
      .order("asc")
      .first();
    if (!row) return null;
    const now = Date.now();
    await ctx.db.patch(row._id, {
      status: "used",
      claimedBy: a.claimedBy,
      claimedByUserId: a.claimedByUserId,
      notes: a.notes,
      claimedAt: now,
      updatedAt: now,
    });
    return row.code;
  },
});
