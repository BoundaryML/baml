import { query, mutation } from "./_generated/server";
import { v } from "convex/values";
import * as lib from "./lib";

const T = "transcriptComments" as const;

/**
 * Fetch one transcript comment by id.
 *
 * @param id - Comment document id to look up.
 * @returns The comment document, or null if it doesn't exist.
 */
export const get = query({
  args: { id: v.string() },
  handler: (ctx, a) => lib.getDoc(ctx, T, a.id),
});

/**
 * List transcript comments, optionally filtered by an index field/value
 * (the dashboard lists by trophyId via the by_trophy index).
 *
 * @param field - Optional index field to filter on.
 * @param value - Optional value the field must equal.
 * @param index - Optional index name to scan (defaults to "by_status_created").
 * @param limit - Maximum number of comments to return (defaults to 100).
 * @returns Up to `limit` comment documents ordered newest-first.
 */
export const list = query({
  args: { field: v.optional(v.string()), value: v.optional(v.string()), index: v.optional(v.string()), limit: v.optional(v.number()) },
  handler: (ctx, a) => lib.listDocs(ctx, T, a),
});

/**
 * Count comments in a claimable state for queue-depth gauges.
 *
 * @param field - Field that holds the claimable state (e.g. "status").
 * @param value - The claimable value to match (e.g. "queued").
 * @param index - Index name to scan for matching comments.
 * @returns The number of claimable comments, capped at 1000.
 */
export const countClaimable = query({
  args: { field: v.string(), value: v.string(), index: v.string() },
  handler: (ctx, a) => lib.countClaimable(ctx, T, a),
});

/**
 * Insert a new transcript comment — the ONLY password-gated create in the
 * schema. The dashboard's Convex websocket client is unauthenticated, so this
 * mutation carries a shared password checked against the deployment env var
 * ATB_COMMENTS_PASSWORD. An unset env var locks the endpoint (fails closed).
 *
 * @param doc - Field values for the new comment (trophyId, author, body, ...).
 * @param password - The shared comments password.
 * @returns The id of the inserted comment.
 */
export const create = mutation({
  args: { doc: v.any(), password: v.string() },
  handler: (ctx, a) => {
    const expected = process.env.ATB_COMMENTS_PASSWORD;
    if (!expected || a.password !== expected) {
      throw new Error("invalid comments password");
    }
    const doc = {
      ...a.doc,
      author: String(a.doc?.author ?? "").slice(0, 80) || "anonymous",
      body: String(a.doc?.body ?? "").slice(0, 4000),
      status: "queued",
    };
    if (!doc.trophyId || !doc.body.trim()) {
      throw new Error("trophyId and body required");
    }
    return lib.createDoc(ctx, T, doc);
  },
});

/**
 * Patch fields on a comment.
 *
 * @param id - Comment document id to patch.
 * @param patch - Partial field values to merge into the comment.
 * @returns The patched comment document.
 */
export const update = mutation({
  args: { id: v.string(), patch: v.any() },
  handler: (ctx, a) => lib.updateDoc(ctx, T, a.id, a.patch),
});

/**
 * Delete a comment.
 *
 * @param id - Comment document id to remove.
 */
export const remove = mutation({
  args: { id: v.string() },
  handler: (ctx, a) => lib.removeDoc(ctx, T, a.id),
});

/**
 * Claim the oldest claimable comment for a worker (dedup's batch gather).
 *
 * @returns The claimed comment document, or null when none are claimable.
 */
export const claim = mutation({
  args: { index: v.string(), field: v.string(), value: v.string(), claimedValue: v.string(), workerId: v.string(), leaseMs: v.number() },
  handler: (ctx, a) => lib.claimDoc(ctx, T, a),
});

/**
 * Transition a comment to a new status (dedup marks the batch done).
 */
export const transition = mutation({
  args: { id: v.string(), to: v.string(), field: v.optional(v.string()), patch: v.optional(v.any()), releaseClaim: v.optional(v.boolean()) },
  handler: (ctx, a) => lib.transitionDoc(ctx, T, a),
});

/**
 * Extend the lease on a claimed comment.
 */
export const heartbeat = mutation({
  args: { id: v.string(), leaseMs: v.number() },
  handler: (ctx, a) => lib.heartbeatDoc(ctx, T, a),
});
