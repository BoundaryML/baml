/**
 * Fixture "fork": a lost process, automatic snapshots, a fork, and a cancel.
 *
 * `durable_plan_trip` runs on the local site and writes an automatic snapshot
 * after every loop iteration (`snapshot` events). The user ends the worker
 * process during day 3. The process writes no terminal event, so only the
 * `worker_exit` event of the site server gives the time of the loss. The run
 * is durable and has a snapshot, so it becomes `paused`.
 *
 * The user forks the run at snapshot #1 (`forked` event), resumes the source,
 * which repeats day 3 and completes through the remote call, resumes the fork,
 * and cancels the fork (`cancelled` event, exit code 130).
 */

import type { DumpValue, PauseStats, StateDump } from "../protocol";
import { FixtureBuilder, type Fixture } from "./builder";
import { lineOf, TRIP_BAML_FILE } from "./trip.baml";

export const FORK_SOURCE = "r-k9d2fw";
export const FORK_RUN = "r-f3q8zt";
export const FORK_CHILD = "r-h6v1mb";

const str = (value: string): DumpValue => ({ kind: "string", preview: JSON.stringify(value) });

export function buildForkFixture(): Fixture {
  const b = new FixtureBuilder();
  const fn = "durable_plan_trip";
  const childFn = "remote_fetch_weather";
  const source = FORK_SOURCE;
  const fork = FORK_RUN;
  const child = FORK_CHILD;
  const callId = `${source}-c1`;
  const file = TRIP_BAML_FILE;
  const printLine = lineOf(fn, "baml.io.println");
  const sleepLine = lineOf(fn, "baml.sys.sleep");
  const callLine = lineOf(fn, "remote_fetch_weather(city)");
  const returnLine = lineOf(fn, "TripPlan {");

  const planDay = (at: number, run: string, day: number): void => {
    b.worker(at, "local", run, { type: "position", thread: 1, function: fn, file, line: printLine, reason: "sysop", op: "baml.io.println" });
    b.worker(at + 1, "local", run, { type: "log", stream: "stdout", text: `planning day ${day}`, thread: 1 });
    b.worker(at + 2, "local", run, { type: "position", thread: 1, function: fn, file, line: sleepLine, reason: "sysop", op: "baml.sys.sleep" });
  };
  const autoStats = (objects: number, raw: number, compressed: number): PauseStats => ({
    pause_latency_ms: null,
    walk_ms: 0.38,
    encode_ms: 0.74,
    compress_ms: 1.6,
    write_ms: 2.4,
    objects,
    raw_bytes: raw,
    compressed_bytes: compressed,
    program_bytes: 18233,
    blocked_attempts: 0,
  });
  const stateAfter = (days: number): Omit<StateDump, "run" | "segment" | "created_ts"> => ({
    threads: [
      {
        thread: 1,
        name: "main",
        parked: { kind: "runnable", detail: "" },
        frames: [
          {
            function: fn,
            file,
            line: printLine,
            locals: [
              { name: "city", type: "string", value: str("Lisbon") },
              {
                name: "ideas",
                type: "string[]",
                value: {
                  kind: "array",
                  preview: `[${days} item${days === 1 ? "" : "s"}]`,
                  children: Array.from({ length: days }, (_, index) => ({ key: String(index), value: str(`day ${index + 1} in Lisbon`) })),
                },
              },
              { name: "day", type: "int", value: { kind: "int", preview: String(days + 1) } },
              { name: "weather", type: "string", value: { kind: "omitted", preview: "<not assigned yet>" } },
            ],
          },
        ],
      },
    ],
    heap: {
      objects: 21 + 2 * days,
      bytes: 5200 + 400 * days,
      by_kind: {
        string: { count: 14 + days, bytes: 2600 + 300 * days },
        array: { count: 2, bytes: 1100 + 100 * days },
        closure: { count: 3, bytes: 960 },
        map: { count: 2 + days, bytes: 540 },
      },
    },
  });

  // Segment 1 of the source. An automatic snapshot follows every loop iteration.
  b.createRun(0, "local", source, fn, { city: "Lisbon" });
  b.setStatus(36, "local", source, "running", { pid: 41810 });
  b.worker(38, "local", source, { type: "hello", mode: "start", function: fn, durable: true });
  b.worker(39, "local", source, { type: "thread_started", thread: 1, parent_thread: null });
  planDay(42, source, 1);
  b.autoSnapshot(1548, "local", source, 1, autoStats(23, 5600, 2610), stateAfter(1));
  planDay(1550, source, 2);
  b.autoSnapshot(3056, "local", source, 2, autoStats(25, 6000, 2790), stateAfter(2));
  planDay(3058, source, 3);

  // The user ends the process. No terminal event exists, only `worker_exit`.
  b.exit(3700, "local", source, -1, "paused", {}, "SIGKILL");

  // The user forks the run at its first snapshot.
  b.fork(5200, "local", source, fork, 1);

  // The source resumes from snapshot #2, repeats day 3, and makes the remote call.
  b.setStatus(6400, "local", source, "starting", { segment: 2 });
  b.setStatus(6446, "local", source, "running", { pid: 41902 });
  b.worker(6448, "local", source, { type: "hello", mode: "resume", function: fn, durable: true });
  b.worker(6452, "local", source, { type: "resumed", stats: { process_start_ms: 21.9, program_load_ms: 24.2, decode_ms: 1.6, first_exec_ms: 48.8 } });
  b.worker(6452, "local", source, { type: "thread_started", thread: 1, parent_thread: null });
  planDay(6454, source, 3);
  b.autoSnapshot(7960, "local", source, 3, autoStats(27, 6400, 2960), stateAfter(3));
  b.worker(7962, "local", source, { type: "position", thread: 1, function: fn, file, line: callLine, reason: "remote_call", op: null });
  b.worker(7963, "local", source, { type: "remote_call", call_id: callId, thread: 1, function: childFn, args: { city: "Lisbon" } });
  b.createRun(7971, "cloud", child, childFn, { city: "Lisbon" }, { parent: { site: "local", run: source, call_id: callId } });
  b.siteEvent(7976, { type: "remote_dispatched", site: "local", run: source, call_id: callId, child_site: "cloud", child_run: child, function: childFn });
  const callAtLine = b.callSite(source, callId);
  // Section 10.3: the record keeps the calling function too.
  const callFrom = b.caller(source, callId);
  b.update(7976, "local", source, {
    waiting_on: [{ call_id: callId, child_site: "cloud", child_run: child, function: childFn, inherited: false, ...callAtLine, caller: callFrom }],
    calls: [{ call_id: callId, function: childFn, args: { city: "Lisbon" }, ...callAtLine, caller: callFrom }],
  });
  b.setStatus(8018, "cloud", child, "running", { pid: 52231 });
  b.worker(8020, "cloud", child, { type: "hello", mode: "start", function: childFn, durable: false });
  b.worker(8021, "cloud", child, { type: "thread_started", thread: 1, parent_thread: null });
  b.worker(8023, "cloud", child, { type: "position", thread: 1, function: childFn, file, line: lineOf(childFn, "baml.io.println"), reason: "sysop", op: "baml.io.println" });
  b.worker(8024, "cloud", child, { type: "log", stream: "stdout", text: "[remote] looking up weather for Lisbon", thread: 1 });
  b.worker(8025, "cloud", child, { type: "position", thread: 1, function: childFn, file, line: lineOf(childFn, "baml.sys.sleep"), reason: "sysop", op: "baml.sys.sleep" });

  // The fork resumes from its copy of snapshot #1 and plans day 2 again.
  b.setStatus(8600, "local", fork, "starting", { segment: 2 });
  b.setStatus(8646, "local", fork, "running", { pid: 41955 });
  b.worker(8648, "local", fork, { type: "hello", mode: "resume", function: fn, durable: true });
  b.worker(8652, "local", fork, { type: "resumed", stats: { process_start_ms: 22.3, program_load_ms: 24.8, decode_ms: 1.5, first_exec_ms: 49.5 } });
  b.worker(8652, "local", fork, { type: "thread_started", thread: 1, parent_thread: null });
  planDay(8654, fork, 2);

  // The user cancels the fork.
  b.worker(9504, "local", fork, { type: "cancelled" });
  b.exit(9506, "local", fork, 130, "cancelled");

  // The child completes, and the source receives the result in its running process.
  b.worker(11_030, "cloud", child, { type: "thread_ended", thread: 1 });
  b.worker(11_031, "cloud", child, { type: "completed", value: "sunny in Lisbon" });
  b.exit(11_034, "cloud", child, 0, "completed", { result: "sunny in Lisbon" });
  b.siteEvent(11_041, { type: "remote_returned", site: "local", run: source, call_id: callId, child_site: "cloud", child_run: child, ok: true });
  b.update(11_041, "local", source, { waiting_on: [] });
  b.worker(11_043, "local", source, { type: "remote_result_received", call_id: callId, thread: 1 });
  b.worker(11_044, "local", source, { type: "position", thread: 1, function: fn, file, line: returnLine, reason: "early_yield", op: null });
  const value = {
    city: "Lisbon",
    ideas: ["day 1 in Lisbon", "day 2 in Lisbon", "day 3 in Lisbon"],
    weather: "sunny in Lisbon",
  };
  b.worker(11_049, "local", source, { type: "thread_ended", thread: 1 });
  b.worker(11_050, "local", source, { type: "completed", value });
  b.exit(11_053, "local", source, 0, "completed", { result: value });

  return b.build("fork", "A lost process, automatic snapshots, a fork, and a cancel");
}
