/**
 * Fixture "chain": one run that moves over all three sites.
 *
 * `durable_plan_trip` starts on the local site and is paused after every loop
 * iteration. The user resumes it on `cloud`, then on `cloud2`. There it makes
 * its remote call. The caller is on `cloud2`, so the remote pool places the
 * child on `cloud`. The user pauses the parent during the wait and resumes it
 * on `local`, two lanes away, while the child still runs on the lane in
 * between. The result reaches `cloud2`, which forwards it along `migrated_to`
 * to `local`, where the parent completes.
 */

import { FixtureBuilder, type Fixture } from "./builder";
import { loopState, pauseStats, planDay, remoteChild, remoteWaitState, resumeSegment, TRIP_VALUE } from "./scenes";
import { lineOf, TRIP_BAML_FILE } from "./trip.baml";

export const CHAIN_PARENT = "r-h4c9tz";
export const CHAIN_CHILD = "r-b6y1wf";

export function buildChainFixture(): Fixture {
  const b = new FixtureBuilder();
  const fn = "durable_plan_trip";
  const parent = CHAIN_PARENT;
  const child = CHAIN_CHILD;
  const callId = `${parent}-c1`;
  const file = TRIP_BAML_FILE;

  // Segment 1 on `local`: the first loop iteration, then a pause.
  b.createRun(0, "local", parent, fn, { city: "Lisbon" });
  b.setStatus(36, "local", parent, "running", { pid: 41733 });
  b.worker(38, "local", parent, { type: "hello", mode: "start", function: fn, durable: true });
  b.worker(39, "local", parent, { type: "thread_started", thread: 1, parent_thread: null });
  planDay(b, 44, "local", parent, fn, 1);
  b.setStatus(800, "local", parent, "pausing");
  b.snapshot(826, "local", parent, 1, pauseStats(26, 1), loopState(fn, 1, 720));

  // "Resume on cloud". Segment 2: the second iteration, then a pause.
  b.migrate(2000, "local", "cloud", parent);
  resumeSegment(b, 2071, "cloud", parent, fn, 52406);
  planDay(b, 2800, "cloud", parent, fn, 2);
  b.setStatus(3400, "cloud", parent, "pausing");
  b.snapshot(3429, "cloud", parent, 2, pauseStats(29, 2), loopState(fn, 2, 904));

  // "Resume on cloud2". Segment 3: the third iteration and the remote call.
  b.migrate(4600, "cloud", "cloud2", parent);
  resumeSegment(b, 4671, "cloud2", parent, fn, 63118);
  planDay(b, 5580, "cloud2", parent, fn, 3);
  const callAt = 7084;
  b.worker(callAt, "cloud2", parent, { type: "position", thread: 1, function: fn, file, line: lineOf(fn, "remote_fetch_weather(city)"), reason: "remote_call", op: null });
  b.worker(callAt + 1, "cloud2", parent, { type: "remote_call", call_id: callId, thread: 1, function: "remote_fetch_weather", args: { city: "Lisbon" } });
  // The caller is on `cloud2`, so the first other site of the pool hosts the child: `cloud`.
  const returnedAt = remoteChild(b, callAt + 9, { site: "cloud2", run: parent, callId }, { site: "cloud", run: child, pid: 52431 });
  b.worker(callAt + 16, "cloud2", parent, { type: "log", stream: "stdout", text: `waiting for remote run ${child} on site cloud`, thread: 1 });

  // The user pauses the parent during the wait and resumes it on `local`.
  b.setStatus(7700, "cloud2", parent, "pausing");
  b.snapshot(7727, "cloud2", parent, 3, pauseStats(27, 3), remoteWaitState(fn, `${callId} remote_fetch_weather on cloud (${child})`));
  b.migrate(8600, "cloud2", "local", parent);
  resumeSegment(b, 8671, "local", parent, fn, 41802);

  // The child reports to `cloud2`, which forwards the result along `migrated_to` to `local`.
  b.siteEvent(returnedAt, { type: "remote_returned", site: "local", run: parent, call_id: callId, child_site: "cloud", child_run: child, ok: true });
  b.update(returnedAt, "local", parent, { waiting_on: [] });
  b.worker(returnedAt + 2, "local", parent, { type: "remote_result_received", call_id: callId, thread: 1 });
  b.worker(returnedAt + 3, "local", parent, { type: "position", thread: 1, function: fn, file, line: lineOf(fn, "TripPlan {"), reason: "early_yield", op: null });
  b.worker(returnedAt + 20, "local", parent, { type: "log", stream: "stdout", text: "trip plan ready", thread: 1 });
  b.worker(returnedAt + 24, "local", parent, { type: "thread_ended", thread: 1 });
  b.worker(returnedAt + 25, "local", parent, { type: "completed", value: TRIP_VALUE });
  b.exit(returnedAt + 28, "local", parent, 0, "completed", { result: TRIP_VALUE });

  return b.build("chain", "One run moves local, cloud, cloud2, and back to local");
}
