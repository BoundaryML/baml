import { query, mutation } from "./_generated/server";
import { v } from "convex/values";
import * as lib from "./lib";

const T = "changelogEntries" as const;

/**
 * Fetch one changelog entry by id.
 *
 * @param id - Entry document id to look up.
 * @returns The entry document, or null if it doesn't exist.
 */
export const get = query({
  args: { id: v.string() },
  handler: (ctx, a) => lib.getDoc(ctx, T, a.id),
});

/**
 * List changelog entries newest-first, optionally filtered by an index field/value.
 *
 * @param field - Optional index field to filter on.
 * @param value - Optional value the field must equal.
 * @param index - Optional index name to scan (defaults to "by_status_created").
 * @param limit - Maximum number of entries to return (defaults to 100).
 * @returns Up to `limit` entry documents ordered newest-first.
 */
export const list = query({
  args: { field: v.optional(v.string()), value: v.optional(v.string()), index: v.optional(v.string()), limit: v.optional(v.number()) },
  handler: (ctx, a) => lib.listDocs(ctx, T, a),
});

/**
 * Count entries in a claimable state for queue-depth gauges.
 *
 * @param field - Field that holds the claimable state (e.g. "status").
 * @param value - The claimable value to match (e.g. "queued").
 * @param index - Index name to scan for matching entries.
 * @returns The number of claimable entries, capped at 1000.
 */
export const countClaimable = query({
  args: { field: v.string(), value: v.string(), index: v.string() },
  handler: (ctx, a) => lib.countClaimable(ctx, T, a),
});

/**
 * Insert a new changelog entry row.
 *
 * @param doc - Field values for the new entry; merged over the queue defaults.
 * @returns The id of the inserted entry.
 */
export const create = mutation({
  args: { doc: v.any() },
  handler: (ctx, a) => lib.createDoc(ctx, T, a.doc),
});

/**
 * Patch fields on a changelog entry.
 *
 * @param id - Entry document id to patch.
 * @param patch - Partial field values to merge into the entry.
 * @returns The patched entry document.
 */
export const update = mutation({
  args: { id: v.string(), patch: v.any() },
  handler: (ctx, a) => lib.updateDoc(ctx, T, a.id, a.patch),
});

/**
 * Delete a changelog entry.
 *
 * @param id - Entry document id to delete.
 * @returns Nothing.
 */
export const remove = mutation({
  args: { id: v.string() },
  handler: (ctx, a) => lib.removeDoc(ctx, T, a.id),
});

/**
 * Atomically claim the oldest queued entry for a worker.
 *
 * @param index - Index to scan for claimable entries.
 * @param field - Field that holds the claimable state (e.g. "status").
 * @param value - The claimable value to match (e.g. "queued").
 * @param claimedValue - Value to flip the entry to on claim (e.g. "generating").
 * @param workerId - Identifier stamped as the claimer.
 * @param leaseMs - Lease duration in milliseconds before the entry can be reaped.
 * @returns The claimed entry, or null if the queue is empty.
 */
export const claim = mutation({
  args: { index: v.string(), field: v.string(), value: v.string(), claimedValue: v.string(), workerId: v.string(), leaseMs: v.number() },
  handler: (ctx, a) => lib.claimDoc(ctx, T, a),
});

/**
 * Move an entry to a new status and release its claim.
 *
 * @param id - Entry document id to transition.
 * @param field - Status field to set (defaults to "status").
 * @param to - New status value to write.
 * @param patch - Optional extra field values to merge into the entry.
 * @param releaseClaim - Whether to clear the claim/lease; defaults to true unless explicitly false.
 * @returns The transitioned entry document.
 */
export const transition = mutation({
  args: { id: v.string(), field: v.optional(v.string()), to: v.string(), patch: v.optional(v.any()), releaseClaim: v.optional(v.boolean()) },
  handler: (ctx, a) => lib.transitionDoc(ctx, T, a),
});

/**
 * Extend a claimed entry's lease so a live worker isn't reaped.
 *
 * @param id - Entry document id whose lease to extend.
 * @param leaseMs - Milliseconds from now to set the new lease expiry.
 * @returns Nothing.
 */
export const heartbeat = mutation({
  args: { id: v.string(), leaseMs: v.number() },
  handler: (ctx, a) => lib.heartbeatDoc(ctx, T, a),
});
