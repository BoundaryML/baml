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
  // A crashed BugVerify (open -> verifying) previously stranded the issue in
  // `verifying` forever; requeue it for another first-pass verify.
  ["issues", "status", "verifying", "open", "by_lease"],
  // The stale-issue re-verify loop (reverify -> reverifying), same story.
  ["issues", "status", "reverifying", "reverify", "by_lease"],
  ["issues", "linearSyncStatus", "syncing", "dirty", "by_linear_sync"],
  ["bamlBuilds", "status", "building", "queued", "by_lease"],
  // A crashed CohortCompare requeues its cohort for another compare attempt
  // (the pending -> queued fan-in is owned by the Python reconciler, not the reaper).
  ["cohorts", "status", "comparing", "queued", "by_lease"],
  ["changelogEntries", "status", "generating", "queued", "by_lease"],
  // Dedup claims comments alongside its trophy batch; a crashed batch requeues.
  ["transcriptComments", "status", "deduping", "queued", "by_lease"],
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

// At most this many issues are queued for re-verification per sweep, so a new
// nightly never floods bug-verify with the whole backlog at once.
const REVERIFY_PER_SWEEP = 5;

async function requeueReverifyImpl(ctx: MutationCtx): Promise<number> {
  // Newest ready baml build = the version to re-check against.
  const builds = await ctx.db
    .query("bamlBuilds" as any)
    .withIndex("by_status_created" as any, (q: any) => q.eq("status", "ready"))
    .order("desc")
    .take(1);
  const newest = (builds as any[])[0];
  if (!newest?.sha) return 0;

  // Candidates: resting `confirmed` issues not yet verified against this build.
  // Only `confirmed` is swept — approved/tocursor/prprep/needs_human are owned by
  // humans or the fix pipeline, and sweeping `open` would race first-pass verify.
  const rows = await ctx.db
    .query("issues" as any)
    .withIndex("by_status_created" as any, (q: any) => q.eq("status", "confirmed"))
    .take(500);
  const candidates = (rows as any[])
    .filter((r) => r.verifyBamlVersion !== newest.sha)
    .sort((a, b) => (a.verifiedAt ?? 0) - (b.verifiedAt ?? 0))
    .slice(0, REVERIFY_PER_SWEEP);

  const now = Date.now();
  for (const row of candidates) {
    await ctx.db.patch(row._id, { status: "reverify", updatedAt: now });
  }
  return candidates.length;
}

/**
 * Cron entry point that re-queues stale `confirmed` issues for re-verification
 * whenever a newer ready baml build exists (capped at REVERIFY_PER_SWEEP per
 * run, oldest-verified first). The ReverifyProcessor claims the `reverify`
 * status and auto-closes issues whose repro no longer fails.
 *
 * @returns The number of issues queued for re-verification.
 */
export const requeueReverify = internalMutation({
  args: {},
  handler: (ctx) => requeueReverifyImpl(ctx),
});

/**
 * Public wrapper around the re-verify sweep for ops/testing.
 *
 * @returns The number of issues queued for re-verification.
 */
export const requeueReverifyNow = mutation({
  args: {},
  handler: (ctx) => requeueReverifyImpl(ctx),
});

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
