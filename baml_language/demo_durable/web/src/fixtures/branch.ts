/**
 * Fixture "branch": one snapshot, two runs.
 *
 * `durable_plan_trip` is paused on the local site during day 2. The user forks
 * the run at that snapshot, resumes the source, and resumes the fork. Both
 * continue from the same state in processes of their own, each makes its own
 * remote call (the pool places one child on cloud and the other on cloud2),
 * and both complete with the same result.
 */

import { FixtureBuilder, type Fixture } from "./builder";
import { loopState, pauseStats, planDay, remoteChild, resumeSegment, TRIP_VALUE } from "./scenes";
import { lineOf, TRIP_BAML_FILE } from "./trip.baml";

export const BRANCH_SOURCE = "r-br6s1h";
export const BRANCH_FORK = "r-bf2y9q";
export const BRANCH_CHILDREN = ["r-bc8u4e", "r-bd3i7o"] as const;

export function buildBranchFixture(): Fixture {
  const b = new FixtureBuilder();
  const fn = "durable_plan_trip";
  const source = BRANCH_SOURCE;
  const fork = BRANCH_FORK;
  const file = TRIP_BAML_FILE;

  b.createRun(0, "local", source, fn, { city: "Lisbon" });
  b.setStatus(36, "local", source, "running", { pid: 42_210 });
  b.worker(38, "local", source, { type: "hello", mode: "start", function: fn, durable: true });
  b.worker(39, "local", source, { type: "thread_started", thread: 1, parent_thread: null });
  planDay(b, 42, "local", source, fn, 1);
  planDay(b, 1550, "local", source, fn, 2);

  // The user pauses the run during day 2.
  const pauseAt = 2200;
  b.setStatus(pauseAt, "local", source, "pausing");
  b.snapshot(pauseAt + 12, "local", source, 1, pauseStats(12, 2), loopState(fn, 2, 850));

  // The user forks the paused run. The fork owns a copy of the snapshot.
  b.fork(3600, "local", source, fork, 1);

  /** A resumed run finishes day 2, plans day 3, calls the remote function, and completes. */
  const finish = (at: number, run: string, pid: number, child: { site: "cloud" | "cloud2"; run: string; pid: number }): void => {
    const callId = `${run}-c1`;
    b.setStatus(at, "local", run, "starting", { segment: 2 });
    resumeSegment(b, at + 44, "local", run, fn, pid);
    planDay(b, at + 900, "local", run, fn, 3);
    const callAt = at + 2410;
    b.worker(callAt, "local", run, { type: "position", thread: 1, function: fn, file, line: lineOf(fn, "remote_fetch_weather(city)"), reason: "remote_call", op: null });
    b.worker(callAt + 1, "local", run, { type: "remote_call", call_id: callId, thread: 1, function: "remote_fetch_weather", args: { city: "Lisbon" } });
    const returnedAt = remoteChild(b, callAt + 9, { site: "local", run, callId }, child);
    b.siteEvent(returnedAt, { type: "remote_returned", site: "local", run, call_id: callId, child_site: child.site, child_run: child.run, ok: true });
    b.update(returnedAt, "local", run, { waiting_on: [] });
    b.worker(returnedAt + 3, "local", run, { type: "remote_result_received", call_id: callId, thread: 1 });
    b.worker(returnedAt + 8, "local", run, { type: "thread_ended", thread: 1 });
    b.worker(returnedAt + 9, "local", run, { type: "completed", value: TRIP_VALUE });
    b.exit(returnedAt + 12, "local", run, 0, "completed", { result: TRIP_VALUE });
  };
  finish(4800, source, 42_266, { site: "cloud", run: BRANCH_CHILDREN[0], pid: 52_810 });
  finish(5900, fork, 42_301, { site: "cloud2", run: BRANCH_CHILDREN[1], pid: 61_344 });

  return b.build("branch", "One snapshot, two runs: a fork that completes next to its source", { roles: { root: { site: "local", id: source } } });
}
