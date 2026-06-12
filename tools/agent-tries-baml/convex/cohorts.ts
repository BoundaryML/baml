import { query, mutation } from "./_generated/server";
import { v } from "convex/values";
import * as lib from "./lib";

const T = "cohorts" as const;

/**
 * Fetch one cohort by id.
 *
 * @param id - Cohort document id to look up.
 * @returns The cohort document, or null if it doesn't exist.
 */
export const get = query({
  args: { id: v.string() },
  handler: (ctx, a) => lib.getDoc(ctx, T, a.id),
});

/**
 * List cohorts newest-first, optionally filtered by an index field/value.
 *
 * @param field - Optional index field to filter on.
 * @param value - Optional value the field must equal.
 * @param index - Optional index name to scan (defaults to "by_status_created").
 * @param limit - Maximum number of cohorts to return (defaults to 100).
 * @returns Up to `limit` cohort documents ordered newest-first.
 */
export const list = query({
  args: { field: v.optional(v.string()), value: v.optional(v.string()), index: v.optional(v.string()), limit: v.optional(v.number()) },
  handler: (ctx, a) => lib.listDocs(ctx, T, a),
});

/**
 * Count cohorts in a claimable state for queue-depth gauges.
 *
 * @param field - Field that holds the claimable state (e.g. "status").
 * @param value - The claimable value to match (e.g. "queued").
 * @param index - Index name to scan for matching cohorts.
 * @returns The number of claimable cohorts, capped at 1000.
 */
export const countClaimable = query({
  args: { field: v.string(), value: v.string(), index: v.string() },
  handler: (ctx, a) => lib.countClaimable(ctx, T, a),
});

/**
 * Insert a new cohort row.
 *
 * @param doc - Field values for the new cohort; merged over the queue defaults.
 * @returns The id of the inserted cohort.
 */
export const create = mutation({
  args: { doc: v.any() },
  handler: (ctx, a) => lib.createDoc(ctx, T, a.doc),
});

/**
 * Patch fields on a cohort.
 *
 * @param id - Cohort document id to patch.
 * @param patch - Partial field values to merge into the cohort.
 * @returns The patched cohort document.
 */
export const update = mutation({
  args: { id: v.string(), patch: v.any() },
  handler: (ctx, a) => lib.updateDoc(ctx, T, a.id, a.patch),
});

/**
 * Delete a cohort.
 *
 * @param id - Cohort document id to delete.
 * @returns Nothing.
 */
export const remove = mutation({
  args: { id: v.string() },
  handler: (ctx, a) => lib.removeDoc(ctx, T, a.id),
});

/**
 * Atomically claim the oldest queued cohort for the compare processor.
 *
 * Flips `field` from `value` to `claimedValue` and stamps an owner + lease in one
 * OCC mutation, so exactly one of N racing processors wins.
 *
 * @param index - Index to scan for claimable cohorts.
 * @param field - Field that holds the claimable state (e.g. "status").
 * @param value - The claimable value to match (e.g. "queued").
 * @param claimedValue - Value to flip the cohort to on claim (e.g. "comparing").
 * @param workerId - Identifier stamped as the claimer.
 * @param leaseMs - Lease duration in milliseconds before the cohort can be reaped.
 * @returns The claimed cohort, or null if the queue is empty.
 */
export const claim = mutation({
  args: { index: v.string(), field: v.string(), value: v.string(), claimedValue: v.string(), workerId: v.string(), leaseMs: v.number() },
  handler: (ctx, a) => lib.claimDoc(ctx, T, a),
});

/**
 * Move a cohort to a new status and release its claim.
 *
 * @param id - Cohort document id to transition.
 * @param field - Status field to set (defaults to "status").
 * @param to - New status value to write.
 * @param patch - Optional extra field values to merge into the cohort.
 * @param releaseClaim - Whether to clear the claim/lease; defaults to true unless explicitly false.
 * @returns The transitioned cohort document.
 */
export const transition = mutation({
  args: { id: v.string(), field: v.optional(v.string()), to: v.string(), patch: v.optional(v.any()), releaseClaim: v.optional(v.boolean()) },
  handler: (ctx, a) => lib.transitionDoc(ctx, T, a),
});

/**
 * Extend a claimed cohort's lease so a live compare processor isn't reaped.
 *
 * @param id - Cohort document id whose lease to extend.
 * @param leaseMs - Milliseconds from now to set the new lease expiry.
 * @returns Nothing.
 */
export const heartbeat = mutation({
  args: { id: v.string(), leaseMs: v.number() },
  handler: (ctx, a) => lib.heartbeatDoc(ctx, T, a),
});
