/**
 * Fixture "recover": the same kill, with and without durability.
 *
 * `durable_plan_trip` runs on the local site and writes an automatic snapshot
 * after every loop iteration. The user ends its process during day 3. The run
 * is durable and has a snapshot, so it becomes `paused`. The user resumes it:
 * a new process repeats day 3 from snapshot #2, makes the remote call, and
 * completes. The user then starts `plan_trip`, the same body without the
 * marker, and ends its process too. That run has nothing to continue from and
 * becomes `lost`.
 */

import type { PauseStats } from "../protocol";
import { FixtureBuilder, type Fixture } from "./builder";
import { loopState, planDay, remoteChild, resumeSegment, TRIP_VALUE } from "./scenes";
import { lineOf, TRIP_BAML_FILE } from "./trip.baml";

export const RECOVER_DURABLE = "r-rv2d8k";
export const RECOVER_CHILD = "r-rw6c1n";
export const RECOVER_PLAIN = "r-rp4n7t";

const autoStats = (days: number): PauseStats => ({
  pause_latency_ms: null,
  walk_ms: 0.38,
  encode_ms: 0.74,
  compress_ms: 1.6,
  write_ms: 2.4,
  objects: 21 + 2 * days,
  raw_bytes: 5200 + 400 * days,
  compressed_bytes: Math.round((5200 + 400 * days) * 0.46),
  program_bytes: 18_233,
  blocked_attempts: 0,
});

export function buildRecoverFixture(): Fixture {
  const b = new FixtureBuilder();
  const fn = "durable_plan_trip";
  const plainFn = "plan_trip";
  const run = RECOVER_DURABLE;
  const plain = RECOVER_PLAIN;
  const callId = `${run}-c1`;
  const file = TRIP_BAML_FILE;

  // The durable run. An automatic snapshot follows every loop iteration.
  b.createRun(0, "local", run, fn, { city: "Lisbon" });
  b.setStatus(36, "local", run, "running", { pid: 42_010 });
  b.worker(38, "local", run, { type: "hello", mode: "start", function: fn, durable: true });
  b.worker(39, "local", run, { type: "thread_started", thread: 1, parent_thread: null });
  planDay(b, 42, "local", run, fn, 1);
  b.autoSnapshot(1548, "local", run, 1, autoStats(1), loopState(fn, 1, 1500));
  planDay(b, 1550, "local", run, fn, 2);
  b.autoSnapshot(3056, "local", run, 2, autoStats(2), loopState(fn, 2, 1500));
  planDay(b, 3058, "local", run, fn, 3);

  // The user ends the process. It writes nothing on its way out.
  b.exit(3700, "local", run, -1, "paused", {}, "SIGKILL");

  // The user resumes the run. Day 3 is planned again, from snapshot #2.
  const resumeAt = 5400;
  b.setStatus(resumeAt, "local", run, "starting", { segment: 2 });
  resumeSegment(b, resumeAt + 44, "local", run, fn, 42_066);
  planDay(b, resumeAt + 54, "local", run, fn, 3);
  b.autoSnapshot(resumeAt + 1560, "local", run, 3, autoStats(3), loopState(fn, 3, 1500));
  const callAt = resumeAt + 1562;
  b.worker(callAt, "local", run, { type: "position", thread: 1, function: fn, file, line: lineOf(fn, "remote_fetch_weather(city)"), reason: "remote_call", op: null });
  b.worker(callAt + 1, "local", run, { type: "remote_call", call_id: callId, thread: 1, function: "remote_fetch_weather", args: { city: "Lisbon" } });
  const returnedAt = remoteChild(b, callAt + 9, { site: "local", run, callId }, { site: "cloud", run: RECOVER_CHILD, pid: 52_720 });
  b.siteEvent(returnedAt, { type: "remote_returned", site: "local", run, call_id: callId, child_site: "cloud", child_run: RECOVER_CHILD, ok: true });
  b.update(returnedAt, "local", run, { waiting_on: [] });
  b.worker(returnedAt + 3, "local", run, { type: "remote_result_received", call_id: callId, thread: 1 });
  b.worker(returnedAt + 5, "local", run, { type: "position", thread: 1, function: fn, file, line: lineOf(fn, "TripPlan {"), reason: "early_yield", op: null });
  b.worker(returnedAt + 8, "local", run, { type: "thread_ended", thread: 1 });
  b.worker(returnedAt + 9, "local", run, { type: "completed", value: TRIP_VALUE });
  b.exit(returnedAt + 12, "local", run, 0, "completed", { result: TRIP_VALUE });

  // The same body without the durable marker: no snapshots, so a kill loses the run.
  const plainAt = returnedAt + 900;
  b.createRun(plainAt, "local", plain, plainFn, { city: "Lisbon" });
  b.setStatus(plainAt + 36, "local", plain, "running", { pid: 42_131 });
  b.worker(plainAt + 38, "local", plain, { type: "hello", mode: "start", function: plainFn, durable: false });
  b.worker(plainAt + 39, "local", plain, { type: "thread_started", thread: 1, parent_thread: null });
  planDay(b, plainAt + 42, "local", plain, plainFn, 1);
  planDay(b, plainAt + 1550, "local", plain, plainFn, 2);
  b.exit(plainAt + 2300, "local", plain, -1, "lost", { error: "worker process was lost (signal SIGKILL)" }, "SIGKILL");

  return b.build("recover", "The same kill, with and without durability", {
    roles: { root: { site: "local", id: run }, plain: { site: "local", id: plain } },
  });
}
