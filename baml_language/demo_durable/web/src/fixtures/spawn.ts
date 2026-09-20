/**
 * Fixture "spawn": `spawn { remote_fetch_weather() }` plus a migration.
 *
 * `durable_plan_trip_parallel` starts on the local site. A spawned thread
 * makes the remote call and waits, while the main thread continues its loop.
 * A pause request is blocked twice, because the pending future of the spawned
 * thread cannot be written yet. After the child returns and the spawned thread
 * ends, the snapshot succeeds. The user resumes the run on the cloud site
 * ("Resume on cloud"), where it completes.
 */

import type { DumpValue, PauseStats, Site } from "../protocol";
import { FixtureBuilder, type Fixture } from "./builder";
import { lineOf, TRIP_BAML_FILE } from "./trip.baml";

export const SPAWN_PARENT = "r-p3n8qd";
export const SPAWN_CHILD = "r-w5t1ke";

const str = (value: string): DumpValue => ({ kind: "string", preview: JSON.stringify(value) });

export function buildSpawnFixture(): Fixture {
  const b = new FixtureBuilder();
  const fn = "durable_plan_trip_parallel";
  const childFn = "remote_fetch_weather";
  const parent = SPAWN_PARENT;
  const child = SPAWN_CHILD;
  const callId = `${parent}-c1`;
  const file = TRIP_BAML_FILE;
  const spawnLine = lineOf(fn, "spawn {");
  const printLine = lineOf(fn, "baml.io.println");
  const sleepLine = lineOf(fn, "baml.sys.sleep");
  const awaitLine = lineOf(fn, "await forecast");
  const returnLine = lineOf(fn, "TripPlan {");

  b.createRun(0, "local", parent, fn, { city: "Lisbon" });
  b.setStatus(33, "local", parent, "running", { pid: 41502 });
  b.worker(35, "local", parent, { type: "hello", mode: "start", function: fn, durable: true });
  b.worker(36, "local", parent, { type: "thread_started", thread: 1, parent_thread: null });
  b.worker(39, "local", parent, { type: "position", thread: 1, function: fn, file, line: spawnLine, reason: "early_yield", op: null });

  // The spawned thread leaves the parent and crosses to the cloud lane.
  b.worker(58, "local", parent, { type: "thread_started", thread: 2, parent_thread: 1 });
  b.worker(60, "local", parent, { type: "position", thread: 2, function: fn, file, line: spawnLine, reason: "remote_call", op: null });
  b.worker(61, "local", parent, { type: "remote_call", call_id: callId, thread: 2, function: childFn, args: { city: "Lisbon" } });
  // The main thread continues its loop in the meantime.
  const planDay = (at: number, site: Site, day: number): void => {
    b.worker(at, site, parent, { type: "position", thread: 1, function: fn, file, line: printLine, reason: "sysop", op: "baml.io.println" });
    b.worker(at + 1, site, parent, { type: "log", stream: "stdout", text: `planning day ${day}`, thread: 1 });
    b.worker(at + 2, site, parent, { type: "position", thread: 1, function: fn, file, line: sleepLine, reason: "sysop", op: "baml.sys.sleep" });
  };
  planDay(64, "local", 1);

  b.createRun(70, "cloud", child, childFn, { city: "Lisbon" }, { parent: { site: "local", run: parent, call_id: callId } });
  b.siteEvent(76, { type: "remote_dispatched", site: "local", run: parent, call_id: callId, child_site: "cloud", child_run: child, function: childFn });
  b.update(76, "local", parent, { waiting_on: [{ call_id: callId, child_site: "cloud", child_run: child, function: childFn }] });

  b.setStatus(118, "cloud", child, "running", { pid: 52110 });
  b.worker(120, "cloud", child, { type: "hello", mode: "start", function: childFn, durable: false });
  b.worker(121, "cloud", child, { type: "thread_started", thread: 1, parent_thread: null });
  b.worker(123, "cloud", child, { type: "position", thread: 1, function: childFn, file, line: lineOf(childFn, "baml.io.println"), reason: "sysop", op: "baml.io.println" });
  b.worker(124, "cloud", child, { type: "log", stream: "stdout", text: "[remote] looking up weather for Lisbon", thread: 1 });
  b.worker(125, "cloud", child, { type: "position", thread: 1, function: childFn, file, line: lineOf(childFn, "baml.sys.sleep"), reason: "sysop", op: "baml.sys.sleep" });

  planDay(1568, "local", 2);

  // The pause request. The pending future of thread 2 blocks two attempts.
  const pauseAt = 1900;
  b.setStatus(pauseAt, "local", parent, "pausing");
  b.worker(pauseAt + 4, "local", parent, { type: "pausing", waiting_on: [`remote call ${callId} (thread 2)`] });
  const blockedPath = ["thread 1", `frame ${fn}`, "local forecast", "Future<string>"];
  b.worker(pauseAt + 6, "local", parent, { type: "blocked", reason: "a pending future cannot be written to a snapshot yet", path: blockedPath });
  planDay(3072, "local", 3);
  b.worker(3076, "local", parent, { type: "blocked", reason: "a pending future cannot be written to a snapshot yet", path: blockedPath });

  // The child completes, and the spawned thread ends.
  b.worker(3131, "cloud", child, { type: "completed", value: "sunny in Lisbon" });
  b.exit(3134, "cloud", child, 0, "completed", { result: "sunny in Lisbon" });
  b.siteEvent(3145, { type: "remote_returned", site: "local", run: parent, call_id: callId, child_site: "cloud", child_run: child, ok: true });
  b.update(3145, "local", parent, { waiting_on: [] });
  b.worker(3148, "local", parent, { type: "remote_result_received", call_id: callId, thread: 2 });
  b.worker(3150, "local", parent, { type: "thread_ended", thread: 2 });

  // The third attempt succeeds while thread 1 sleeps.
  const stats: PauseStats = {
    pause_latency_ms: 1262,
    walk_ms: 0.51,
    encode_ms: 1.02,
    compress_ms: 2.2,
    write_ms: 2.7,
    objects: 34,
    raw_bytes: 7180,
    compressed_bytes: 3204,
    program_bytes: 18233,
    blocked_attempts: 2,
  };
  b.snapshot(3162, "local", parent, 1, stats, {
    threads: [
      {
        thread: 1,
        name: "main",
        parked: { kind: "sleep", detail: "baml.sys.sleep, 1410 ms remain" },
        frames: [
          {
            function: fn,
            file,
            line: sleepLine,
            locals: [
              { name: "city", type: "string", value: str("Lisbon") },
              {
                name: "forecast",
                type: "Future<string>",
                value: {
                  kind: "instance",
                  class: "Future",
                  preview: "Future { resolved }",
                  children: [
                    { key: "state", value: str("resolved") },
                    { key: "value", value: str("sunny in Lisbon") },
                  ],
                },
              },
              {
                name: "ideas",
                type: "string[]",
                value: {
                  kind: "array",
                  preview: "[2 items]",
                  children: [1, 2].map((day, index) => ({ key: String(index), value: str(`day ${day} in Lisbon`) })),
                },
              },
              { name: "day", type: "int", value: { kind: "int", preview: "3" } },
            ],
          },
        ],
      },
    ],
    heap: {
      objects: 34,
      bytes: 7180,
      by_kind: {
        string: { count: 22, bytes: 3620 },
        array: { count: 2, bytes: 1152 },
        instance: { count: 2, bytes: 824 },
        closure: { count: 5, bytes: 1180 },
        map: { count: 3, bytes: 404 },
      },
    },
  });

  // The user resumes the run on another site ("Resume on cloud").
  const migrateAt = 5200;
  b.migrate(migrateAt, "local", "cloud", parent);
  b.setStatus(migrateAt + 71, "cloud", parent, "running", { pid: 52140 });
  b.worker(migrateAt + 73, "cloud", parent, { type: "hello", mode: "resume", function: fn, durable: true });
  b.worker(migrateAt + 77, "cloud", parent, {
    type: "resumed",
    stats: { process_start_ms: 22.8, program_load_ms: 25.3, decode_ms: 2.1, first_exec_ms: 51.6 },
  });
  b.worker(migrateAt + 77, "cloud", parent, { type: "thread_started", thread: 1, parent_thread: null });
  planDay(migrateAt + 80, "cloud", 4);
  const doneAt = migrateAt + 80 + 1504;
  b.worker(doneAt, "cloud", parent, { type: "position", thread: 1, function: fn, file, line: awaitLine, reason: "await", op: null });
  b.worker(doneAt + 2, "cloud", parent, { type: "position", thread: 1, function: fn, file, line: returnLine, reason: "early_yield", op: null });
  const value = {
    city: "Lisbon",
    ideas: [1, 2, 3, 4].map((day) => `day ${day} in Lisbon`),
    weather: "sunny in Lisbon",
  };
  b.worker(doneAt + 5, "cloud", parent, { type: "thread_ended", thread: 1 });
  b.worker(doneAt + 6, "cloud", parent, { type: "completed", value });
  b.exit(doneAt + 9, "cloud", parent, 0, "completed", { result: value });

  return b.build("spawn", "spawn { remote_f() } and a migration");
}
