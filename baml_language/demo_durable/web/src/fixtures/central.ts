/**
 * Fixture "central": the central scene of the demo.
 *
 * The parent `durable_plan_trip` starts on the local site and calls
 * `remote_fetch_weather` on the cloud site. The user pauses the parent while
 * the child runs, and the parent's process ends. The child completes, and the
 * local site stores the result. The user resumes the parent in a new process,
 * which receives the stored result at startup and completes.
 */

import type { DumpValue, PauseStats } from "../protocol";
import { FixtureBuilder, type Fixture } from "./builder";
import { lineOf, TRIP_BAML_FILE } from "./trip.baml";

export const CENTRAL_PARENT = "r-7k2m9x";
export const CENTRAL_CHILD = "r-c4x8p2";

const str = (value: string): DumpValue => ({ kind: "string", preview: JSON.stringify(value) });

export function buildCentralFixture(): Fixture {
  const b = new FixtureBuilder();
  const fn = "durable_plan_trip";
  const parent = CENTRAL_PARENT;
  const child = CENTRAL_CHILD;
  const callId = `${parent}-c1`;
  const file = TRIP_BAML_FILE;
  const printLine = lineOf(fn, "baml.io.println");
  const sleepLine = lineOf(fn, "baml.sys.sleep");
  const callLine = lineOf(fn, "remote_fetch_weather(city)");
  const returnLine = lineOf(fn, "TripPlan {");

  // Segment 1 of the parent on the local site.
  b.createRun(0, "local", parent, fn, { city: "Lisbon" });
  b.setStatus(38, "local", parent, "running", { pid: 41201 });
  b.worker(40, "local", parent, { type: "hello", mode: "start", function: fn, durable: true });
  b.worker(41, "local", parent, { type: "thread_started", thread: 1, parent_thread: null });
  let t = 44;
  for (let day = 1; day <= 3; day++) {
    b.worker(t, "local", parent, { type: "position", thread: 1, function: fn, file, line: printLine, reason: "sysop", op: "baml.io.println" });
    b.worker(t + 1, "local", parent, { type: "log", stream: "stdout", text: `planning day ${day}`, thread: 1 });
    b.worker(t + 2, "local", parent, { type: "position", thread: 1, function: fn, file, line: sleepLine, reason: "sysop", op: "baml.sys.sleep" });
    t += 1504;
  }

  // The remote call. `t` is 4556 here.
  b.worker(t, "local", parent, { type: "position", thread: 1, function: fn, file, line: callLine, reason: "remote_call", op: null });
  b.worker(t + 1, "local", parent, { type: "remote_call", call_id: callId, thread: 1, function: "remote_fetch_weather", args: { city: "Lisbon" } });
  b.createRun(t + 9, "cloud", child, "remote_fetch_weather", { city: "Lisbon" }, { parent: { site: "local", run: parent, call_id: callId } });
  b.siteEvent(t + 14, { type: "remote_dispatched", site: "local", run: parent, call_id: callId, child_site: "cloud", child_run: child, function: "remote_fetch_weather" });
  // Section 10.2: the run record keeps the call site of the entry and of the call.
  const callAtLine = b.callSite(parent, callId);
  // Section 10.3: the record keeps the calling function too.
  const callFrom = b.caller(parent, callId);
  b.update(t + 14, "local", parent, {
    waiting_on: [{ call_id: callId, child_site: "cloud", child_run: child, function: "remote_fetch_weather", inherited: false, ...callAtLine, caller: callFrom }],
    calls: [{ call_id: callId, function: "remote_fetch_weather", args: { city: "Lisbon" }, ...callAtLine, caller: callFrom }],
  });
  b.worker(t + 16, "local", parent, { type: "log", stream: "stdout", text: `waiting for remote run ${child} on site cloud`, thread: 1 });

  // The child on the cloud site.
  const childFn = "remote_fetch_weather";
  b.setStatus(t + 52, "cloud", child, "running", { pid: 52077 });
  b.worker(t + 54, "cloud", child, { type: "hello", mode: "start", function: childFn, durable: false });
  b.worker(t + 55, "cloud", child, { type: "thread_started", thread: 1, parent_thread: null });
  b.worker(t + 57, "cloud", child, { type: "position", thread: 1, function: childFn, file, line: lineOf(childFn, "baml.io.println"), reason: "sysop", op: "baml.io.println" });
  b.worker(t + 58, "cloud", child, { type: "log", stream: "stdout", text: "[remote] looking up weather for Lisbon", thread: 1 });
  b.worker(t + 59, "cloud", child, { type: "position", thread: 1, function: childFn, file, line: lineOf(childFn, "baml.sys.sleep"), reason: "sysop", op: "baml.sys.sleep" });

  // The user pauses the parent while the child runs.
  const pauseAt = 5400;
  b.setStatus(pauseAt, "local", parent, "pausing");
  const stats: PauseStats = {
    pause_latency_ms: 31,
    walk_ms: 0.42,
    encode_ms: 0.88,
    compress_ms: 1.95,
    write_ms: 3.1,
    objects: 27,
    raw_bytes: 6412,
    compressed_bytes: 2948,
    program_bytes: 18233,
    blocked_attempts: 0,
  };
  b.snapshot(pauseAt + 31, "local", parent, 1, stats, {
    threads: [
      {
        thread: 1,
        name: "main",
        parked: { kind: "remote_call", detail: `${callId} remote_fetch_weather on cloud (${child})` },
        frames: [
          {
            function: fn,
            file,
            line: callLine,
            locals: [
              { name: "city", type: "string", value: str("Lisbon") },
              {
                name: "ideas",
                type: "string[]",
                value: {
                  kind: "array",
                  preview: "[3 items]",
                  children: [1, 2, 3].map((day, index) => ({ key: String(index), value: str(`day ${day} in Lisbon`) })),
                },
              },
              { name: "day", type: "int", value: { kind: "int", preview: "4" } },
              { name: "weather", type: "string", value: { kind: "omitted", preview: "<not assigned yet>" } },
            ],
          },
        ],
      },
    ],
    heap: {
      objects: 27,
      bytes: 6412,
      by_kind: {
        string: { count: 19, bytes: 3384 },
        array: { count: 2, bytes: 1216 },
        instance: { count: 1, bytes: 412 },
        closure: { count: 3, bytes: 960 },
        map: { count: 2, bytes: 440 },
      },
    },
  });

  // The cloud lane continues. The child completes while the parent has no process.
  const childDone = t + 3064;
  b.worker(childDone, "cloud", child, { type: "completed", value: "sunny in Lisbon" });
  b.exit(childDone + 3, "cloud", child, 0, "completed", { result: "sunny in Lisbon" });
  b.siteEvent(childDone + 11, { type: "remote_returned", site: "local", run: parent, call_id: callId, child_site: "cloud", child_run: child, ok: true });
  b.update(childDone + 11, "local", parent, { waiting_on: [] });

  // The user resumes the parent in a new process.
  const resumeAt = 9800;
  b.setStatus(resumeAt, "local", parent, "starting", { segment: 2 });
  b.setStatus(resumeAt + 46, "local", parent, "running", { pid: 41377 });
  b.worker(resumeAt + 48, "local", parent, { type: "hello", mode: "resume", function: fn, durable: true });
  b.worker(resumeAt + 52, "local", parent, {
    type: "resumed",
    stats: { process_start_ms: 21.4, program_load_ms: 24.9, decode_ms: 1.7, first_exec_ms: 49.2 },
  });
  b.worker(resumeAt + 52, "local", parent, { type: "thread_started", thread: 1, parent_thread: null });
  b.worker(resumeAt + 53, "local", parent, { type: "remote_result_received", call_id: callId, thread: 1 });
  b.worker(resumeAt + 54, "local", parent, { type: "position", thread: 1, function: fn, file, line: returnLine, reason: "early_yield", op: null });
  const value = {
    city: "Lisbon",
    ideas: ["day 1 in Lisbon", "day 2 in Lisbon", "day 3 in Lisbon"],
    weather: "sunny in Lisbon",
  };
  b.worker(resumeAt + 420, "local", parent, { type: "log", stream: "stdout", text: "trip plan ready", thread: 1 });
  b.worker(resumeAt + 424, "local", parent, { type: "thread_ended", thread: 1 });
  b.worker(resumeAt + 425, "local", parent, { type: "completed", value });
  b.exit(resumeAt + 428, "local", parent, 0, "completed", { result: value });

  return b.build("central", "Pause the parent while the remote child runs");
}
