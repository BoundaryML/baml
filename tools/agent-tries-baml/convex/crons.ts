import { cronJobs } from "convex/server";
import { internal } from "./_generated/api";

/**
 * Scheduled jobs for the bench backend.
 *
 * Runs the lease reaper ("reap stale leases") every 2 minutes via
 * internal.maintenance.reap, requeueing or failing work stranded by crashed
 * processors so no claimed row stays locked forever.
 */
const crons = cronJobs();

// Requeue work stranded by crashed processors.
crons.interval("reap stale leases", { minutes: 2 }, internal.maintenance.reap);

// Age dead machines out of the presence roster (UI greys them long before).
crons.interval("sweep stale workers", { minutes: 10 }, internal.maintenance.sweepStaleWorkers);

export default crons;
