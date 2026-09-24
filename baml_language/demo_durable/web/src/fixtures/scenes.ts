/**
 * Building blocks that the three-site fixtures share: state dumps of the trip
 * planner, one loop iteration, the start of a resumed segment, and a remote
 * child from its creation to its result.
 */

import type { DumpValue, PauseStats, Site, StateDump } from "../protocol";
import type { FixtureBuilder } from "./builder";
import { lineOf, TRIP_BAML_FILE } from "./trip.baml";

const str = (value: string): DumpValue => ({ kind: "string", preview: JSON.stringify(value) });

/** The state of `durable_plan_trip` while it sleeps in the loop, after `days` iterations. */
export function loopState(fn: string, days: number, remainingMs: number): Omit<StateDump, "run" | "segment" | "created_ts"> {
  return {
    threads: [
      {
        thread: 1,
        name: "main",
        parked: { kind: "sleep", detail: `baml.sys.sleep, ${remainingMs} ms remain` },
        frames: [
          {
            function: fn,
            file: TRIP_BAML_FILE,
            line: lineOf(fn, "baml.sys.sleep"),
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
              { name: "day", type: "int", value: { kind: "int", preview: String(days) } },
              { name: "weather", type: "string", value: { kind: "omitted", preview: "<not assigned yet>" } },
            ],
          },
        ],
      },
    ],
    heap: {
      objects: 19 + 4 * days,
      bytes: 5200 + 400 * days,
      by_kind: {
        string: { count: 13 + 3 * days, bytes: 2600 + 260 * days },
        array: { count: 2, bytes: 1024 + 96 * days },
        instance: { count: 1, bytes: 412 },
        closure: { count: 3, bytes: 960 },
        map: { count: days, bytes: 204 + 44 * days },
      },
    },
  };
}

/** The state of `durable_plan_trip` while it waits for the result of its remote call. */
export function remoteWaitState(fn: string, detail: string): Omit<StateDump, "run" | "segment" | "created_ts"> {
  const state = loopState(fn, 3, 0);
  const thread = state.threads[0];
  const frame = thread?.frames[0];
  if (!thread || !frame) return state;
  return {
    ...state,
    threads: [
      {
        ...thread,
        parked: { kind: "remote_call", detail },
        frames: [
          {
            ...frame,
            line: lineOf(fn, "remote_fetch_weather(city)"),
            locals: frame.locals.map((local) => (local.name === "day" ? { ...local, value: { kind: "int", preview: "4" } } : local)),
          },
        ],
      },
    ],
  };
}

export function pauseStats(latencyMs: number, days: number): PauseStats {
  const raw = 5200 + 400 * days;
  return {
    pause_latency_ms: latencyMs,
    walk_ms: 0.38 + 0.02 * days,
    encode_ms: 0.8 + 0.03 * days,
    compress_ms: 1.8 + 0.05 * days,
    write_ms: 2.9,
    objects: 19 + 4 * days,
    raw_bytes: raw,
    compressed_bytes: Math.round(raw * 0.46),
    program_bytes: 18233,
    blocked_attempts: 0,
  };
}

/** One loop iteration of the trip planner: the `println`, its log line, and the `sleep`. */
export function planDay(b: FixtureBuilder, at: number, site: Site, run: string, fn: string, day: number): void {
  const file = TRIP_BAML_FILE;
  b.worker(at, site, run, { type: "position", thread: 1, function: fn, file, line: lineOf(fn, "baml.io.println"), reason: "sysop", op: "baml.io.println" });
  b.worker(at + 1, site, run, { type: "log", stream: "stdout", text: `planning day ${day}`, thread: 1 });
  b.worker(at + 2, site, run, { type: "position", thread: 1, function: fn, file, line: lineOf(fn, "baml.sys.sleep"), reason: "sysop", op: "baml.sys.sleep" });
}

/** The first events of a resumed segment: `hello`, `resumed`, and the root thread. */
export function resumeSegment(b: FixtureBuilder, at: number, site: Site, run: string, fn: string, pid: number): void {
  b.setStatus(at, site, run, "running", { pid });
  b.worker(at + 2, site, run, { type: "hello", mode: "resume", function: fn, durable: true });
  b.worker(at + 6, site, run, {
    type: "resumed",
    stats: { process_start_ms: 22.1, program_load_ms: 24.6, decode_ms: 1.8, first_exec_ms: 50.3 },
  });
  b.worker(at + 6, site, run, { type: "thread_started", thread: 1, parent_thread: null });
}

/** A remote child from its creation to the delivery of its result. Returns the time of `remote_returned`. */
export function remoteChild(
  b: FixtureBuilder,
  at: number,
  parent: { site: Site; run: string; callId: string },
  child: { site: Site; run: string; pid: number },
): number {
  const fn = "remote_fetch_weather";
  const file = TRIP_BAML_FILE;
  b.createRun(at, child.site, child.run, fn, { city: "Lisbon" }, { parent: { site: parent.site, run: parent.run, call_id: parent.callId } });
  b.siteEvent(at + 5, { type: "remote_dispatched", site: parent.site, run: parent.run, call_id: parent.callId, child_site: child.site, child_run: child.run, function: fn });
  const callAtLine = b.callSite(parent.run, parent.callId);
  b.update(at + 5, parent.site, parent.run, {
    waiting_on: [{ call_id: parent.callId, child_site: child.site, child_run: child.run, function: fn, inherited: false, ...callAtLine }],
  });
  b.setStatus(at + 43, child.site, child.run, "running", { pid: child.pid });
  b.worker(at + 45, child.site, child.run, { type: "hello", mode: "start", function: fn, durable: false });
  b.worker(at + 46, child.site, child.run, { type: "thread_started", thread: 1, parent_thread: null });
  b.worker(at + 48, child.site, child.run, { type: "position", thread: 1, function: fn, file, line: lineOf(fn, "baml.io.println"), reason: "sysop", op: "baml.io.println" });
  b.worker(at + 49, child.site, child.run, { type: "log", stream: "stdout", text: "[remote] looking up weather for Lisbon", thread: 1 });
  b.worker(at + 50, child.site, child.run, { type: "position", thread: 1, function: fn, file, line: lineOf(fn, "baml.sys.sleep"), reason: "sysop", op: "baml.sys.sleep" });
  const done = at + 3055;
  b.worker(done, child.site, child.run, { type: "completed", value: "sunny in Lisbon" });
  b.exit(done + 3, child.site, child.run, 0, "completed", { result: "sunny in Lisbon" });
  // The site that holds the parent accepted the result.
  b.update(done + 11, child.site, child.run, { result_delivered: true });
  return done + 11;
}

export const TRIP_VALUE = {
  city: "Lisbon",
  ideas: ["day 1 in Lisbon", "day 2 in Lisbon", "day 3 in Lisbon"],
  weather: "sunny in Lisbon",
};

