import { query, mutation } from "./_generated/server";
import { v } from "convex/values";
import * as lib from "./lib";

const T = "issues" as const;

/**
 * Fetch one issue by id.
 *
 * @param id - Issue document id to look up.
 * @returns The issue document, or null if it doesn't exist.
 */
export const get = query({
  args: { id: v.string() },
  handler: (ctx, a) => lib.getDoc(ctx, T, a.id),
});

/**
 * List issues newest-first, optionally filtered by an index field/value.
 *
 * @param field - Optional index field to filter on.
 * @param value - Optional value the field must equal.
 * @param index - Optional index name to scan (defaults to "by_status_created").
 * @param limit - Maximum number of issues to return (defaults to 100).
 * @returns Up to `limit` issue documents ordered newest-first.
 */
export const list = query({
  args: { field: v.optional(v.string()), value: v.optional(v.string()), index: v.optional(v.string()), limit: v.optional(v.number()) },
  handler: (ctx, a) => lib.listDocs(ctx, T, a),
});

/**
 * Count issues in a claimable state for queue-depth gauges.
 *
 * @param field - Field that holds the claimable state (e.g. "status").
 * @param value - The claimable value to match (e.g. "approved").
 * @param index - Index name to scan for matching issues.
 * @returns The number of claimable issues, capped at 1000.
 */
export const countClaimable = query({
  args: { field: v.string(), value: v.string(), index: v.string() },
  handler: (ctx, a) => lib.countClaimable(ctx, T, a),
});

/**
 * Insert a new issue row.
 *
 * @param doc - Field values for the new issue; merged over the queue defaults.
 * @returns The id of the inserted issue.
 */
export const create = mutation({
  args: { doc: v.any() },
  handler: (ctx, a) => lib.createDoc(ctx, T, a.doc),
});

/**
 * Patch fields on an issue.
 *
 * @param id - Issue document id to patch.
 * @param patch - Partial field values to merge into the issue.
 * @returns The patched issue document.
 */
export const update = mutation({
  args: { id: v.string(), patch: v.any() },
  handler: (ctx, a) => lib.updateDoc(ctx, T, a.id, a.patch),
});

/**
 * Delete an issue.
 *
 * @param id - Issue document id to delete.
 * @returns Nothing.
 */
export const remove = mutation({
  args: { id: v.string() },
  handler: (ctx, a) => lib.removeDoc(ctx, T, a.id),
});

/**
 * Atomically claim the oldest queued issue for a worker.
 *
 * Flips `field` from `value` to `claimedValue` and stamps an owner + lease in one
 * OCC mutation, so exactly one of N racing workers wins.
 *
 * @param index - Index to scan for claimable issues.
 * @param field - Field that holds the claimable state (e.g. "status").
 * @param value - The claimable value to match (e.g. "approved").
 * @param claimedValue - Value to flip the issue to on claim (e.g. "fixing").
 * @param workerId - Identifier stamped as the claimer.
 * @param leaseMs - Lease duration in milliseconds before the issue can be reaped.
 * @returns The claimed issue, or null if the queue is empty.
 */
export const claim = mutation({
  args: { index: v.string(), field: v.string(), value: v.string(), claimedValue: v.string(), workerId: v.string(), leaseMs: v.number() },
  handler: (ctx, a) => lib.claimDoc(ctx, T, a),
});

/**
 * Move an issue to a new status and release its claim.
 *
 * @param id - Issue document id to transition.
 * @param field - Status field to set (defaults to "status").
 * @param to - New status value to write.
 * @param patch - Optional extra field values to merge into the issue.
 * @param releaseClaim - Whether to clear the claim/lease; defaults to true unless explicitly false.
 * @returns The transitioned issue document.
 */
export const transition = mutation({
  args: { id: v.string(), field: v.optional(v.string()), to: v.string(), patch: v.optional(v.any()), releaseClaim: v.optional(v.boolean()) },
  handler: (ctx, a) => lib.transitionDoc(ctx, T, a),
});

/**
 * Extend a claimed issue's lease so a live worker isn't reaped.
 *
 * @param id - Issue document id whose lease to extend.
 * @param leaseMs - Milliseconds from now to set the new lease expiry.
 * @returns Nothing.
 */
export const heartbeat = mutation({
  args: { id: v.string(), leaseMs: v.number() },
  handler: (ctx, a) => lib.heartbeatDoc(ctx, T, a),
});
