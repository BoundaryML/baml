/**
 * Fixture "pool": the central scene with three sites (contract section 8).
 *
 * The parent `durable_plan_trip` starts on the local site. The user pauses it
 * inside its loop and resumes it on the cloud site ("Resume on cloud"). The
 * parent continues there and calls `remote_fetch_weather`. The caller is on
 * `cloud`, so the remote pool places the child on `cloud2`. The result returns
 * to `cloud`, where the parent completes.
 */

import { FixtureBuilder, type Fixture } from "./builder";
import { loopState, pauseStats, planDay, remoteChild, resumeSegment, TRIP_VALUE } from "./scenes";
import { lineOf, TRIP_BAML_FILE } from "./trip.baml";

export const POOL_PARENT = "r-m2v7hc";
export const POOL_CHILD = "r-q8s3nd";

export function buildPoolFixture(): Fixture {
  const b = new FixtureBuilder();
  const fn = "durable_plan_trip";
  const parent = POOL_PARENT;
  const child = POOL_CHILD;
  const callId = `${parent}-c1`;
  const file = TRIP_BAML_FILE;

  // Segment 1 on the local site: two loop iterations.
  b.createRun(0, "local", parent, fn, { city: "Lisbon" });
  b.setStatus(37, "local", parent, "running", { pid: 41644 });
  b.worker(39, "local", parent, { type: "hello", mode: "start", function: fn, durable: true });
  b.worker(40, "local", parent, { type: "thread_started", thread: 1, parent_thread: null });
  planDay(b, 44, "local", parent, fn, 1);
  planDay(b, 1548, "local", parent, fn, 2);

  // The user pauses the parent while it sleeps in the second iteration.
  const pauseAt = 2300;
  b.setStatus(pauseAt, "local", parent, "pausing");
  b.snapshot(pauseAt + 28, "local", parent, 1, pauseStats(28, 2), loopState(fn, 2, 722));

  // "Resume on cloud": the snapshot moves, and segment 2 starts on the cloud site.
  const moveAt = 4200;
  b.migrate(moveAt, "local", "cloud", parent);
  resumeSegment(b, moveAt + 71, "cloud", parent, fn, 52318);
  planDay(b, moveAt + 800, "cloud", parent, fn, 3);

  // The remote call. The caller is on `cloud`, so the pool places the child on `cloud2`.
  const callAt = moveAt + 800 + 1504;
  b.worker(callAt, "cloud", parent, { type: "position", thread: 1, function: fn, file, line: lineOf(fn, "remote_fetch_weather(city)"), reason: "remote_call", op: null });
  b.worker(callAt + 1, "cloud", parent, { type: "remote_call", call_id: callId, thread: 1, function: "remote_fetch_weather", args: { city: "Lisbon" } });
  const returnedAt = remoteChild(b, callAt + 9, { site: "cloud", run: parent, callId }, { site: "cloud2", run: child, pid: 63027 });
  b.worker(callAt + 16, "cloud", parent, { type: "log", stream: "stdout", text: `waiting for remote run ${child} on site cloud2`, thread: 1 });

  // The result returns to `cloud`, and the parent completes there.
  b.siteEvent(returnedAt, { type: "remote_returned", site: "cloud", run: parent, call_id: callId, child_site: "cloud2", child_run: child, ok: true });
  b.update(returnedAt, "cloud", parent, { waiting_on: [] });
  b.worker(returnedAt + 2, "cloud", parent, { type: "remote_result_received", call_id: callId, thread: 1 });
  b.worker(returnedAt + 3, "cloud", parent, { type: "position", thread: 1, function: fn, file, line: lineOf(fn, "TripPlan {"), reason: "early_yield", op: null });
  b.worker(returnedAt + 20, "cloud", parent, { type: "log", stream: "stdout", text: "trip plan ready", thread: 1 });
  b.worker(returnedAt + 24, "cloud", parent, { type: "thread_ended", thread: 1 });
  b.worker(returnedAt + 25, "cloud", parent, { type: "completed", value: TRIP_VALUE });
  b.exit(returnedAt + 28, "cloud", parent, 0, "completed", { result: TRIP_VALUE });

  return b.build("pool", "Pause on local, resume on cloud, remote call into cloud2");
}
