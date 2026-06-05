// Generic, table-agnostic CRUD + claimable-queue primitives.
// Convex can't take a fully dynamic table name in a typed way, so each
// table module (tasks.ts, trophies.ts, issues.ts, bamlBuilds.ts) wraps
// these handlers with `query`/`mutation` and passes its table literal.
//
// The one operation that MUST be transactional is `claim`: Convex
// mutations are serializable with OCC, so among N racers exactly one
// flips a given row out of the claimable value - no double-processing.

import { QueryCtx, MutationCtx } from "./_generated/server";

type Table = "tasks" | "trophies" | "issues" | "bamlBuilds" | "workers";

// ---------- queries ----------

/**
 * Fetch a single row by id.
 *
 * @param ctx - Convex query context.
 * @param table - Table to read from.
 * @param id - Document id to look up.
 * @returns The document, or null if it doesn't exist.
 */
export async function getDoc(ctx: QueryCtx, table: Table, id: string) {
  return await ctx.db.get(id as any);
}

/**
 * List rows newest-first, optionally filtered to a single index field/value.
 *
 * When `field` and `value` are both given, scans the named index (defaulting to
 * "by_status_created"); otherwise returns the most recent rows table-wide.
 *
 * @param ctx - Convex query context.
 * @param table - Table to read from.
 * @param opts - Query options.
 * @param opts.field - Optional index field to filter on.
 * @param opts.value - Optional value the field must equal.
 * @param opts.index - Optional index name to scan (defaults to "by_status_created").
 * @param opts.limit - Maximum number of rows to return (defaults to 100).
 * @returns Up to `limit` documents ordered newest-first.
 */
export async function listDocs(
  ctx: QueryCtx,
  table: Table,
  opts: { field?: string; value?: string; index?: string; limit?: number },
) {
  const limit = opts.limit ?? 100;
  if (opts.field && opts.value !== undefined) {
    const index = opts.index ?? "by_status_created";
    return await ctx.db
      .query(table as any)
      .withIndex(index as any, (q: any) => q.eq(opts.field, opts.value))
      .order("desc")
      .take(limit);
  }
  return await ctx.db.query(table as any).order("desc").take(limit);
}

/**
 * Count rows currently in a claimable state for queue-depth gauges.
 *
 * Scans the given index and counts matching rows, capped at 1000.
 *
 * @param ctx - Convex query context.
 * @param table - Table to read from.
 * @param opts - Query options.
 * @param opts.field - Field that holds the claimable state (e.g. "status").
 * @param opts.value - The claimable value to match (e.g. "queued").
 * @param opts.index - Index name to scan for matching rows.
 * @returns The number of claimable rows, capped at 1000.
 */
export async function countClaimable(
  ctx: QueryCtx,
  table: Table,
  opts: { field: string; value: string; index: string },
) {
  const rows = await ctx.db
    .query(table as any)
    .withIndex(opts.index as any, (q: any) => q.eq(opts.field, opts.value))
    .take(1000);
  return rows.length;
}

// ---------- mutations ----------

// Convex treats `v.optional(T)` as "absent or T" - a literal null is NOT
// valid. The Python client serializes None -> null, so we strip null keys
// (no schema field legitimately stores null) to mean "field absent".
function stripNulls<T extends Record<string, any>>(obj: T): T {
  for (const k of Object.keys(obj)) {
    if (obj[k] === null) delete obj[k];
  }
  return obj;
}

/**
 * Insert a row with default attempts and timestamps.
 *
 * Null keys are stripped (Convex treats them as absent), and a taskEvent is
 * logged when inserting into the tasks table.
 *
 * @param ctx - Convex mutation context.
 * @param table - Table to insert into.
 * @param doc - Field values for the new row; merged over the defaults.
 * @returns The id of the inserted document.
 */
export async function createDoc(ctx: MutationCtx, table: Table, doc: any) {
  const now = Date.now();
  const full = stripNulls({
    attempts: 0,
    createdAt: now,
    updatedAt: now,
    ...doc,
  });
  const id = await ctx.db.insert(table as any, full);
  if (table === "tasks") {
    await ctx.db.insert("taskEvents", {
      taskId: id,
      eventType: full.status ?? "created",
    });
  }
  return id;
}

/**
 * Patch a row and bump its updatedAt timestamp.
 *
 * Null keys in the patch are stripped (Convex treats them as absent).
 *
 * @param ctx - Convex mutation context.
 * @param table - Table the row lives in.
 * @param id - Document id to patch.
 * @param patch - Partial field values to merge into the row.
 * @returns The patched document.
 */
export async function updateDoc(
  ctx: MutationCtx,
  table: Table,
  id: string,
  patch: any,
) {
  await ctx.db.patch(id as any, stripNulls({ ...patch, updatedAt: Date.now() }));
  return await ctx.db.get(id as any);
}

/**
 * Delete a row by id.
 *
 * @param ctx - Convex mutation context.
 * @param table - Table the row lives in.
 * @param id - Document id to delete.
 * @returns Nothing.
 */
export async function removeDoc(ctx: MutationCtx, table: Table, id: string) {
  await ctx.db.delete(id as any);
}

/**
 * Atomically claim the oldest claimable row for a table.
 *
 * Flips `field` from `value` to `claimedValue` and stamps an owner + lease in one
 * OCC mutation, so exactly one of N racing workers wins. Logs a taskEvent for the
 * tasks table.
 *
 * @param ctx - Convex mutation context.
 * @param table - Table to claim a row from.
 * @param args - Claim parameters.
 * @param args.index - Index to scan for claimable rows.
 * @param args.field - Field that holds the claimable state (e.g. "status").
 * @param args.value - The claimable value to match (e.g. "queued").
 * @param args.claimedValue - Value to flip the row to on claim (e.g. "running").
 * @param args.workerId - Identifier stamped as the claimer.
 * @param args.leaseMs - Lease duration in milliseconds before the row can be reaped.
 * @returns The claimed document, or null if the queue is empty.
 */
export async function claimDoc(
  ctx: MutationCtx,
  table: Table,
  args: {
    index: string;
    field: string;
    value: string;
    claimedValue: string;
    workerId: string;
    leaseMs: number;
  },
) {
  const next = await ctx.db
    .query(table as any)
    .withIndex(args.index as any, (q: any) => q.eq(args.field, args.value))
    .order("asc")
    .first();
  if (!next) return null;
  const now = Date.now();
  const patch: any = {
    [args.field]: args.claimedValue,
    claimedBy: args.workerId,
    claimedAt: now,
    leaseExpiresAt: now + args.leaseMs,
    attempts: (next.attempts ?? 0) + 1,
    updatedAt: now,
  };
  await ctx.db.patch(next._id, patch);
  if (table === "tasks") {
    await ctx.db.insert("taskEvents", {
      taskId: next._id,
      eventType: args.claimedValue,
      details: { workerId: args.workerId },
    });
  }
  return await ctx.db.get(next._id);
}

/**
 * Move a row to a new status and, unless told otherwise, release its claim.
 *
 * Sets the status `field` to `to`, applies any extra patch, and clears the
 * claim/lease so the worker hands ownership back. State-machine edge validity is
 * enforced upstream in the Python service, not here. Logs a taskEvent for the
 * tasks table.
 *
 * @param ctx - Convex mutation context.
 * @param table - Table the row lives in.
 * @param args - Transition parameters.
 * @param args.id - Document id to transition.
 * @param args.field - Status field to set (defaults to "status").
 * @param args.to - New status value to write.
 * @param args.patch - Optional extra field values to merge into the row.
 * @param args.releaseClaim - Whether to clear the claim/lease; defaults to true unless explicitly false.
 * @returns The transitioned document.
 */
export async function transitionDoc(
  ctx: MutationCtx,
  table: Table,
  args: { id: string; field?: string; to: string; patch?: any; releaseClaim?: boolean },
) {
  const field = args.field ?? "status";
  const patch: any = stripNulls({ [field]: args.to, updatedAt: Date.now(), ...(args.patch ?? {}) });
  if (args.releaseClaim !== false) {
    patch.claimedBy = undefined;
    patch.claimedAt = undefined;
    patch.leaseExpiresAt = undefined;
  }
  await ctx.db.patch(args.id as any, patch);
  if (table === "tasks") {
    await ctx.db.insert("taskEvents", { taskId: args.id as any, eventType: args.to });
  }
  return await ctx.db.get(args.id as any);
}

/**
 * Extend a claimed row's lease so a live worker isn't reaped.
 *
 * Pushes `leaseExpiresAt` out by `leaseMs` from now, keeping the claim alive.
 *
 * @param ctx - Convex mutation context.
 * @param table - Table the row lives in.
 * @param args - Heartbeat parameters.
 * @param args.id - Document id whose lease to extend.
 * @param args.leaseMs - Milliseconds from now to set the new lease expiry.
 * @returns Nothing.
 */
export async function heartbeatDoc(
  ctx: MutationCtx,
  table: Table,
  args: { id: string; leaseMs: number },
) {
  await ctx.db.patch(args.id as any, {
    leaseExpiresAt: Date.now() + args.leaseMs,
    updatedAt: Date.now(),
  });
}
