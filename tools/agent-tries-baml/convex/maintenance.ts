// Lease reaper: requeue (or fail) rows whose claim lease expired, so a
// crashed processor never strands work. Called by convex/crons.ts.

import { internalMutation, mutation, MutationCtx } from "./_generated/server";

const MAX_ATTEMPTS = 4;

// (table, statusField, claimedValue, requeueValue, index)
const RULES: Array<[string, string, string, string, string]> = [
  ["tasks", "status", "running", "queued", "by_lease"],
  ["trophies", "status", "deduping", "queued", "by_lease"],
  // FixDispatch claims approved -> dispatching, then transitions to tocursor on a
  // successful launch (releasing the claim). A crashed dispatch is requeued to
  // approved. tocursor/prprep are owned by the tracker sweep (no claim/lease).
  ["issues", "status", "dispatching", "approved", "by_lease"],
  ["issues", "status", "redrafting", "redraft", "by_lease"],
  ["issues", "notionSyncStatus", "syncing", "dirty", "by_notion_sync"],
  ["bamlBuilds", "status", "building", "queued", "by_lease"],
  // A crashed CohortCompare requeues its cohort for another compare attempt
  // (the pending -> queued fan-in is owned by the Python reconciler, not the reaper).
  ["cohorts", "status", "comparing", "queued", "by_lease"],
  ["changelogEntries", "status", "generating", "queued", "by_lease"],
];

async function reapImpl(ctx: MutationCtx): Promise<number> {
  const now = Date.now();
  let reaped = 0;
  for (const [table, field, claimed, requeue, index] of RULES) {
    const rows = await ctx.db
      .query(table as any)
      .withIndex(index as any, (q: any) => q.eq(field, claimed))
      .take(200);
    for (const row of rows as any[]) {
      if ((row.leaseExpiresAt ?? Infinity) >= now) continue;
      const toFail = field === "status" && (row.attempts ?? 0) >= MAX_ATTEMPTS;
      await ctx.db.patch(row._id, {
        [field]: toFail ? "failed" : requeue,
        lastError: toFail ? "lease expired; max attempts" : "lease expired; requeued",
        claimedBy: undefined,
        claimedAt: undefined,
        leaseExpiresAt: undefined,
        updatedAt: now,
      });
      reaped++;
    }
  }
  return reaped;
}

/**
 * Cron entry point that sweeps every queue rule for expired leases.
 *
 * For each rule, requeues rows whose lease expired or fails them once attempts
 * reach MAX_ATTEMPTS, so a crashed processor never strands work. Scheduled by
 * convex/crons.ts every couple of minutes.
 *
 * @returns The number of rows reaped across all rules.
 */
export const reap = internalMutation({ args: {}, handler: (ctx) => reapImpl(ctx) });

/**
 * Public wrapper around the reaper for ops/testing ("force a reap now").
 *
 * Runs the same sweep as the cron, but callable on demand from the dashboard.
 *
 * @returns The number of rows reaped across all rules.
 */
export const reapNow = mutation({ args: {}, handler: (ctx) => reapImpl(ctx) });

/**
 * Cron entry point that deletes worker presence rows whose heartbeat is
 * older than 10 minutes, so dead machines age out of the dashboard roster.
 * Scheduled by convex/crons.ts.
 *
 * @returns The number of presence rows deleted.
 */
export const sweepStaleWorkers = internalMutation({
  args: {},
  handler: async (ctx) => {
    const cutoff = Date.now() - 10 * 60 * 1000;
    const rows = await ctx.db.query("workers" as any).take(500);
    let removed = 0;
    for (const row of rows as any[]) {
      if ((row.lastHeartbeat ?? 0) < cutoff) {
        await ctx.db.delete(row._id);
        removed++;
      }
    }
    return removed;
  },
});
