import { query, mutation } from "./_generated/server";
import { v } from "convex/values";
import * as lib from "./lib";

const T = "tasks" as const;

/**
 * Fetch one task by id.
 *
 * @param id - Task document id to look up.
 * @returns The task document, or null if it doesn't exist.
 */
export const get = query({
  args: { id: v.string() },
  handler: (ctx, a) => lib.getDoc(ctx, T, a.id),
});

/**
 * List tasks newest-first, optionally filtered by an index field/value.
 *
 * @param field - Optional index field to filter on.
 * @param value - Optional value the field must equal.
 * @param index - Optional index name to scan (defaults to "by_status_created").
 * @param limit - Maximum number of tasks to return (defaults to 100).
 * @returns Up to `limit` task documents ordered newest-first.
 */
export const list = query({
  args: { field: v.optional(v.string()), value: v.optional(v.string()), index: v.optional(v.string()), limit: v.optional(v.number()) },
  handler: (ctx, a) => lib.listDocs(ctx, T, a),
});

/**
 * Count tasks in a claimable state for queue-depth gauges.
 *
 * @param field - Field that holds the claimable state (e.g. "status").
 * @param value - The claimable value to match (e.g. "queued").
 * @param index - Index name to scan for matching tasks.
 * @returns The number of claimable tasks, capped at 1000.
 */
export const countClaimable = query({
  args: { field: v.string(), value: v.string(), index: v.string() },
  handler: (ctx, a) => lib.countClaimable(ctx, T, a),
});

/**
 * Insert a new task row.
 *
 * @param doc - Field values for the new task; merged over the queue defaults.
 * @returns The id of the inserted task.
 */
export const create = mutation({
  args: { doc: v.any() },
  handler: (ctx, a) => lib.createDoc(ctx, T, a.doc),
});

/**
 * Patch fields on a task.
 *
 * @param id - Task document id to patch.
 * @param patch - Partial field values to merge into the task.
 * @returns The patched task document.
 */
export const update = mutation({
  args: { id: v.string(), patch: v.any() },
  handler: (ctx, a) => lib.updateDoc(ctx, T, a.id, a.patch),
});

/**
 * Delete a task.
 *
 * @param id - Task document id to delete.
 * @returns Nothing.
 */
export const remove = mutation({
  args: { id: v.string() },
  handler: (ctx, a) => lib.removeDoc(ctx, T, a.id),
});

/**
 * Atomically claim the oldest queued task for a worker.
 *
 * Flips `field` from `value` to `claimedValue` and stamps an owner + lease in one
 * OCC mutation, so exactly one of N racing workers wins.
 *
 * @param index - Index to scan for claimable tasks.
 * @param field - Field that holds the claimable state (e.g. "status").
 * @param value - The claimable value to match (e.g. "queued").
 * @param claimedValue - Value to flip the task to on claim (e.g. "running").
 * @param workerId - Identifier stamped as the claimer.
 * @param leaseMs - Lease duration in milliseconds before the task can be reaped.
 * @returns The claimed task, or null if the queue is empty.
 */
export const claim = mutation({
  args: { index: v.string(), field: v.string(), value: v.string(), claimedValue: v.string(), workerId: v.string(), leaseMs: v.number() },
  handler: (ctx, a) => lib.claimDoc(ctx, T, a),
});

/**
 * Move a task to a new status and release its claim.
 *
 * @param id - Task document id to transition.
 * @param field - Status field to set (defaults to "status").
 * @param to - New status value to write.
 * @param patch - Optional extra field values to merge into the task.
 * @param releaseClaim - Whether to clear the claim/lease; defaults to true unless explicitly false.
 * @returns The transitioned task document.
 */
export const transition = mutation({
  args: { id: v.string(), field: v.optional(v.string()), to: v.string(), patch: v.optional(v.any()), releaseClaim: v.optional(v.boolean()) },
  handler: (ctx, a) => lib.transitionDoc(ctx, T, a),
});

/**
 * Extend a claimed task's lease so a live worker isn't reaped.
 *
 * @param id - Task document id whose lease to extend.
 * @param leaseMs - Milliseconds from now to set the new lease expiry.
 * @returns Nothing.
 */
export const heartbeat = mutation({
  args: { id: v.string(), leaseMs: v.number() },
  handler: (ctx, a) => lib.heartbeatDoc(ctx, T, a),
});
