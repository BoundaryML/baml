#!/usr/bin/env node
// Mock worker for the durable functions proof of concept.
//
// It accepts the command line of contract section 2.1, writes the events of
// sections 2.3 and 7.1 to stdout (one JSON object per line), reads the commands
// of section 2.4 from stdin, and uses the exit codes of section 2.2. It does not
// run BAML. It simulates the functions of program/baml_src/trip.baml with
// timers, and its "snapshot" is a JSON file that holds the simulated state.
//
// Mock-only knobs (environment variables):
//   MOCK_SPEED=<n>          divide every simulated sleep by n (default 1)
//   MOCK_AUTO_SNAPSHOT=0    do not write automatic snapshots (default: a
//                           durable run writes one after every loop iteration
//                           and reports it with a `snapshot` event)
//   MOCK_BLOCKED=<n>        report n `blocked` snapshot attempts before a pause
//                           succeeds (also settable per run with the argument
//                           "mock_blocked": n)
// Mock-only run arguments:
//   "mock_remote_sleep_ms": n   the remote child of this run sleeps n real
//                           milliseconds (not divided by MOCK_SPEED) instead of
//                           3000 simulated ones. The parent passes it on to the
//                           child as "mock_sleep_ms".
//   "mock_nap_ms": n        durable_fan_out sleeps n simulated milliseconds
//                           instead of 12000.
//   "mock_exit_delay_ms": n  a run that suspends itself exits n milliseconds after
//                            its `paused` event and reads no command in between
//   "mock_fail_after_ms": n the run fails after n simulated milliseconds, while
//                           its remote children may still run.
//   "mock_no_call_site": true  every `remote_call` and `remote_cancel` of this
//                           run reports `file: null` and `line: null`, as a real
//                           worker does when no user frame can be attributed.
//   "mock_cancel_cause": "<cause>"  every `remote_cancel` of this run reports
//                           this cause instead of the one the mock classifies.
//                           `"unknown"` is what an engine reports when it cannot
//                           tell what fired.
// Mock-only behavior: remote_fetch_weather fails for the city "Atlantis".
//
// Contract section 10.1: `remote_call` and `thread_started` carry the call site
// and the `spawn` site, and `remote_cancel` carries the call site of the
// cancelled call plus `cause`. The mock maps its cancellations to the four
// cause values: a `race` loser and an input that `baml.future.all` drops after
// an error are `future_cancel` (both cancel the input's own future), and
// `with_timeout` is `token`. The run argument "mock_cancel_cause" overrides all
// of them. `remote_call` also carries `caller`, the function that made the call
// (contract section 10.3).
//
// Contract section 9.2: the flags --sleep-suspend-ms, --program-store, and
// --program-hash, `hello.program_hash`, `paused.wake`, `remote_cancel`, and
// `resumed.stats.program_source`. The functions of program/baml_src/quotes.baml
// run on several simulated threads. The "program" of the mock is the text of
// the project's .baml files, and its hash is the SHA-256 of that payload.
//   MOCK_RUNTIME_BUILD=<s>  the runtime build that the mock writes into store
//                           entries and expects in them (default mock-worker/1)
//
// The worker does not know which site hosts it (contract section 8). The site
// server of the caller places a remote call on a site of the remote pool.

import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import readline from "node:readline";

const T0 = performance.now();
const TRIP_FILE = "baml_src/trip.baml";
const QUOTES_FILE = "baml_src/quotes.baml";
const RUNTIME_BUILD = process.env.MOCK_RUNTIME_BUILD ?? "mock-worker/1";
const PROGRAM_FORMAT_VERSION = 1;
const SPEED = Math.max(0.001, Number(process.env.MOCK_SPEED ?? "1") || 1);
const AUTO_SNAPSHOT = (process.env.MOCK_AUTO_SNAPSHOT ?? "1") !== "0";

// Line numbers in program/baml_src/trip.baml.
const LINES = {
  remote_fetch_weather: { print: 15, sleep: 16, ret: 17 },
  durable_plan_trip: { print: 24, sleep: 25, push: 26, remote: 29, ret: 30 },
  plan_trip: { print: 37, sleep: 38, push: 39, remote: 42, ret: 43 },
  durable_plan_trip_parallel: {
    start_print: 48, spawn: 49, print: 52, sleep: 53, push: 54,
    await_print: 57, await: 58, ret: 59,
  },
  // Line numbers in program/baml_src/quotes.baml.
  remote_get_quote: { print: 121, sleep: 130, fail: 132, ret: 147 },
  durable_fan_out: { print: 158, spawns: [159, 160, 161, 162], nap_print: 163, sleep: 164, await_print: 165, await: 166, ret: 179 },
  durable_race: { print: 189, spawns: [190, 191, 192], await: 193, after_print: 194, ret: 195 },
  durable_settled: { print: 199, spawns: [203, 204, 205], await: 206, after_print: 230, ret: 236 },
  durable_deadline: { print: 240, await: 241, remote: 242, after_print: 247, ret: 248 },
  durable_nap: { print: 252, sleep: 253, after_print: 254, ret: 255 },
};
const QUOTE_FUNCTIONS = new Set(["remote_get_quote", "durable_fan_out", "durable_race", "durable_settled", "durable_deadline", "durable_nap"]);
const sourceFile = (fn) => (QUOTE_FUNCTIONS.has(fn) ? QUOTES_FILE : TRIP_FILE);

// ---------------------------------------------------------------- arguments

function usage(message) {
  process.stderr.write(`mock-worker: ${message}\n`);
  process.stderr.write(
    "usage: mock-worker.mjs --project <dir> --run <run-id> --segment <n> --snapshot-dir <dir>\n" +
    "         ( --start <function> --json-args '<json>' | --resume <snapshot-path> )\n" +
    "         [--remote-result '<json>']... [--sleep-suspend-ms <n>]\n" +
    "         [--program-store <dir>] [--program-hash <hex>]\n" +
    "       --project is optional with --program-hash, and with --resume when --program-store is given\n");
  process.exit(2);
}

function parseArgs(argv) {
  const out = { remoteResults: [] };
  for (let i = 0; i < argv.length; i++) {
    const flag = argv[i];
    const value = () => {
      if (i + 1 >= argv.length) usage(`missing value for ${flag}`);
      return argv[++i];
    };
    switch (flag) {
      case "--project": out.project = value(); break;
      case "--run": out.run = value(); break;
      case "--segment": out.segment = Number(value()); break;
      case "--snapshot-dir": out.snapshotDir = value(); break;
      case "--start": out.start = value(); break;
      case "--json-args": out.jsonArgs = value(); break;
      case "--resume": out.resume = value(); break;
      case "--remote-result": out.remoteResults.push(value()); break;
      case "--sleep-suspend-ms": out.sleepSuspendMs = Number(value()); break;
      case "--program-store": out.programStore = value(); break;
      case "--program-hash": out.programHash = value(); break;
      default: usage(`unknown argument ${flag}`);
    }
  }
  if (!out.run || !/^[a-z0-9-]+$/.test(out.run)) usage("--run must match [a-z0-9-]+");
  if (!Number.isInteger(out.segment) || out.segment < 1) usage("--segment must be a positive integer");
  if (!out.snapshotDir) usage("--snapshot-dir is required");
  if (!!out.start === !!out.resume) usage("exactly one of --start and --resume is required");
  if (out.sleepSuspendMs === undefined) out.sleepSuspendMs = 5000;
  if (!Number.isFinite(out.sleepSuspendMs) || out.sleepSuspendMs < 0) usage("--sleep-suspend-ms must be a number that is not negative");
  if (out.programHash !== undefined && !/^[0-9a-f]{64}$/.test(out.programHash)) usage("--program-hash must be 64 lowercase hexadecimal characters");
  if (out.programHash !== undefined && !out.start) usage("--program-hash is only meaningful with --start");
  if (out.programHash !== undefined && !out.programStore) usage("--program-hash requires --program-store");
  const projectOptional = out.programHash !== undefined || (!!out.resume && !!out.programStore);
  if (!out.project && !projectOptional) usage("--project is required");
  return out;
}

const opts = parseArgs(process.argv.slice(2));

// ------------------------------------------------------------------ output

function emit(type, fields = {}, done = undefined) {
  const event = { v: 1, type, ts: Date.now(), run: opts.run, segment: opts.segment, pid: process.pid, ...fields };
  process.stdout.write(JSON.stringify(event) + "\n", done);
}

function exitAfterFlush(code) {
  // The callback of an empty write runs after every earlier write was flushed.
  process.stdout.write("", () => process.exit(code));
}

function diag(text) {
  process.stderr.write(`[mock-worker ${opts.run}#${opts.segment}] ${text}\n`);
}

const round = (x) => Math.round(x * 100) / 100;

// ------------------------------------------------------------------- state

/** The whole simulated program state. It is what a snapshot stores. */
let state = null;

const PAUSE = Symbol("pause");
let pauseRequested = false;
let pauseRequestedAt = 0;
let interrupt = null;               // rejects the wait that the main thread is in
const lastPosition = new Map();     // thread -> "file:line"
const resultWaiters = new Map();    // call_id -> resolve function

function newState(fn, args) {
  return {
    mock_snapshot: 1,
    function: fn,
    args,
    durable: fn.includes("durable"),
    call_counter: 0,
    pc: "begin",
    sleep_remaining_ms: null,
    vars: { city: args.city, ideas: [], day: 1, weather: null },
    // call_id -> { thread, function, args, result: null | {value} | {error}, received }
    calls: {},
    spawned: null,                  // { thread, call_id, ended } for the parallel variant
    blocked_left: Number(args.mock_blocked ?? process.env.MOCK_BLOCKED ?? 0) || 0,
    // The functions of quotes.baml:
    threads: {},                    // thread id -> { parent, name, line, ended }
    next_thread: 2,
    sleeps: {},                     // name -> { deadline_ts, thread } (absolute, so it survives a suspend)
    order: [],                      // call ids in input order of the combinator
    collector: null,                // thread of baml.future.all / race / all_settled
    suspend_blocked: false,         // a self-suspend was answered with `blocked` once
    program_hash: null,
  };
}

function position(thread, fn, line, reason, op) {
  const file = sourceFile(state.function);
  const key = `${file}:${line}`;
  if (lastPosition.get(thread) === key) return;
  lastPosition.set(thread, key);
  emit("position", { thread, function: fn, file, line, reason, op: op ?? null });
}

function log(text, thread = 1) {
  emit("log", { stream: "stdout", text, thread });
}

/** The arguments of the remote call that `state.function` makes. */
function remoteArgs() {
  const out = { city: state.vars.city };
  const ms = Number(state.args.mock_remote_sleep_ms);
  if (Number.isFinite(ms) && ms > 0) out.mock_sleep_ms = ms;
  return out;
}

/** An interruptible sleep. The remaining time survives a pause. */
function sleep(ms) {
  const total = state.sleep_remaining_ms ?? ms;
  const started = performance.now();
  state.sleep_remaining_ms = total;
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      interrupt = null;
      state.sleep_remaining_ms = null;
      resolve();
    }, total / SPEED);
    interrupt = () => {
      clearTimeout(timer);
      interrupt = null;
      const elapsed = (performance.now() - started) * SPEED;
      state.sleep_remaining_ms = Math.max(0, Math.round(total - elapsed));
      reject(PAUSE);
    };
    if (pauseRequested) interrupt();
  });
}

/** Waits until the result of `callId` is known. Interruptible by a pause. */
function waitForResult(callId) {
  const call = state.calls[callId];
  if (call.result) return Promise.resolve(call.result);
  return new Promise((resolve, reject) => {
    resultWaiters.set(callId, (result) => {
      interrupt = null;
      resolve(result);
    });
    interrupt = () => {
      interrupt = null;
      resultWaiters.delete(callId);
      reject(PAUSE);
    };
    if (pauseRequested) interrupt();
  });
}

function deliverResult(msg) {
  const callId = msg.call_id;
  const result = "error" in msg && msg.error != null ? { error: String(msg.error) } : { value: msg.value ?? null };
  const call = state.calls[callId];
  if (call && call.cancelled) return; // contract section 9.2: a result for a cancelled call is ignored
  if (!call) {
    // Contract section 7.1: a result for a call id that this worker does not
    // wait on is ignored. After a recovery from an older snapshot the call is
    // reached again, `remote_call` is emitted with the same id, and the site
    // server answers from its stored result.
    diag(`ignoring a result for the unknown call ${callId}`);
    return;
  }
  if (call.result) return;          // duplicate delivery, ignored as well
  call.result = result;
  emit("remote_result_received", { call_id: callId, thread: call.thread });
  if (state.spawned && state.spawned.call_id === callId && !state.spawned.ended) {
    state.spawned.ended = true;
    emit("thread_ended", { thread: call.thread });
  }
  if (call.thread !== 1 && state.threads?.[call.thread]) endThread(call.thread);
  wakeParked();
  const waiter = resultWaiters.get(callId);
  if (waiter) {
    resultWaiters.delete(callId);
    waiter(result);
  }
}

/**
 * Contract section 10.1: the call site that a `remote_call` reports. The
 * mock-only run argument `"mock_no_call_site": true` makes every call report
 * `null`, which is what a real worker does when it cannot attribute the call
 * to a user frame. The app then falls back to the thread's last `position`.
 */
function callSite(line) {
  if (state.args?.mock_no_call_site === true) return { file: null, line: null, caller: null };
  // Contract section 10.3: `caller` is the function the call is written in. A
  // real worker reports the innermost user frame, which for a `spawn { }` body
  // is the function that contains the `spawn`.
  return { file: sourceFile(state.function), line, caller: state.function };
}

/** Reaches a `remote_` call on `thread`. Returns the call id. */
function remoteCall(thread, fn, line, args) {
  state.call_counter += 1;
  const callId = `${opts.run}-c${state.call_counter}`;
  position(thread, state.function, line, "remote_call", null);
  const site = callSite(line);
  state.calls[callId] = { thread, function: fn, args, result: null, file: site.file, line: site.line, caller: site.caller };
  emit("remote_call", { call_id: callId, thread, function: fn, args, file: site.file, line: site.line, caller: site.caller });
  log(`waiting for remote run of ${fn} (${callId}) on a remote site`, thread);
  return callId;
}

// --------------------------------------------------------------- snapshots

function nextSnapshotNumber() {
  let max = 0;
  try {
    for (const name of fs.readdirSync(opts.snapshotDir)) {
      const m = /^snap-(\d+)\.bamlsnap$/.exec(name);
      if (m) max = Math.max(max, Number(m[1]));
    }
  } catch { /* the directory is created below */ }
  return max + 1;
}

function dumpValue(value, depth = 0) {
  if (value === null || value === undefined) return { kind: "null", preview: "null" };
  if (typeof value === "boolean") return { kind: "bool", preview: String(value) };
  if (typeof value === "number") {
    return { kind: Number.isInteger(value) ? "int" : "float", preview: String(value) };
  }
  if (typeof value === "string") return { kind: "string", preview: JSON.stringify(value) };
  if (depth >= 4) return { kind: "omitted", preview: "…" };
  if (Array.isArray(value)) {
    return {
      kind: "array",
      preview: `[${value.length} item${value.length === 1 ? "" : "s"}]`,
      children: value.slice(0, 50).map((v, i) => ({ key: String(i), value: dumpValue(v, depth + 1) })),
    };
  }
  return {
    kind: "map",
    preview: `{${Object.keys(value).length} entries}`,
    children: Object.entries(value).slice(0, 50).map(([k, v]) => ({ key: k, value: dumpValue(v, depth + 1) })),
  };
}

function currentLine() {
  const L = LINES[state.function];
  switch (state.pc) {
    case "loop_print": return L.print;
    case "loop_sleep": return L.sleep;
    case "remote": case "remote_wait": return L.remote;
    case "await_print": return L.await_print;
    case "await": return L.await;
    case "print": return L.print;
    case "sleep": return L.sleep;
    default: return L.print ?? L.ret;
  }
}

function buildStateDump() {
  if (QUOTE_FUNCTIONS.has(state.function)) return buildQuotesStateDump();
  const fn = state.function;
  const v = state.vars;
  const pendingCall = Object.entries(state.calls).find(([, c]) => !c.result);
  let parked;
  if (state.pc === "loop_sleep" || state.pc === "sleep") {
    parked = { kind: "sysop", detail: `baml.sys.sleep (${state.sleep_remaining_ms ?? 0} ms remaining)` };
  } else if (state.pc === "remote_wait" && pendingCall) {
    parked = { kind: "remote_call", detail: `${pendingCall[1].function} ${pendingCall[0]}` };
  } else if (state.pc === "await") {
    parked = { kind: "await", detail: "future of spawn at line " + LINES[fn].spawn };
  } else {
    parked = { kind: "runnable", detail: "" };
  }
  const locals = [{ name: "city", type: "string", value: dumpValue(v.city) }];
  if (fn !== "remote_fetch_weather") {
    locals.push({ name: "ideas", type: "string[]", value: dumpValue(v.ideas) });
    if (state.spawned) {
      locals.push({
        name: "weather_future", type: "Future<string>",
        value: { kind: "opaque", preview: state.spawned.ended ? "Future(resolved)" : "Future(pending)" },
      });
    }
    locals.push({ name: "day", type: "int", value: dumpValue(v.day) });
    if (v.weather !== null) locals.push({ name: "weather", type: "string", value: dumpValue(v.weather) });
  }
  const threads = [{
    thread: 1, name: fn, parked,
    frames: [{ function: fn, file: TRIP_FILE, line: currentLine(), locals }],
  }];
  if (state.spawned && !state.spawned.ended) {
    const call = state.calls[state.spawned.call_id];
    threads.push({
      thread: state.spawned.thread,
      name: `${fn}.<spawn>`,
      parked: { kind: "remote_call", detail: `${call.function} ${state.spawned.call_id}` },
      frames: [{
        function: `${fn}.<spawn>`, file: TRIP_FILE, line: LINES[fn].spawn,
        locals: [{ name: "city", type: "string", value: dumpValue(v.city) }],
      }],
    });
  }
  const strings = 1 + v.ideas.length + (v.weather ? 1 : 0);
  const stringBytes = [v.city, ...v.ideas, v.weather ?? ""].reduce((n, s) => n + 24 + String(s).length, 0);
  const by_kind = {
    string: { count: strings, bytes: stringBytes },
    array: { count: 1, bytes: 32 + 8 * v.ideas.length },
  };
  if (state.spawned) by_kind.future = { count: 1, bytes: 48 };
  const objects = Object.values(by_kind).reduce((n, k) => n + k.count, 0);
  const bytes = Object.values(by_kind).reduce((n, k) => n + k.bytes, 0);
  return {
    run: opts.run, segment: opts.segment, created_ts: Date.now(),
    threads, heap: { objects, bytes, by_kind },
  };
}

/** Writes snap-<n>.bamlsnap and snap-<n>.json. Returns the paths and PauseStats. */
function writeSnapshot(pauseLatencyMs, blockedAttempts) {
  fs.mkdirSync(opts.snapshotDir, { recursive: true });
  const n = nextSnapshotNumber();
  const snapshotPath = path.join(opts.snapshotDir, `snap-${n}.bamlsnap`);
  const statePath = path.join(opts.snapshotDir, `snap-${n}.json`);
  const t0 = performance.now();
  const dump = buildStateDump();
  const t1 = performance.now();
  const raw = JSON.stringify({ ...state, saved_by: { run: opts.run, segment: opts.segment, pid: process.pid } });
  const t2 = performance.now();
  // The mock does not compress. The "compressed" size is the size on disk.
  const t3 = performance.now();
  fs.writeFileSync(snapshotPath, raw);
  fs.writeFileSync(statePath, JSON.stringify(dump, null, 2));
  const t4 = performance.now();
  const stats = {
    pause_latency_ms: pauseLatencyMs === null ? null : round(pauseLatencyMs),
    walk_ms: round(t1 - t0), encode_ms: round(t2 - t1), compress_ms: round(t3 - t2), write_ms: round(t4 - t3),
    objects: dump.heap.objects, raw_bytes: raw.length, compressed_bytes: raw.length,
    program_bytes: null, blocked_attempts: blockedAttempts,
  };
  return { n, snapshot_path: snapshotPath, state_path: statePath, stats };
}

function autoSnapshot() {
  if (!AUTO_SNAPSHOT || !state.durable) return;
  const snap = writeSnapshot(null, 0);
  emit("snapshot", { snapshot_path: snap.snapshot_path, state_path: snap.state_path, stats: snap.stats, automatic: true });
}

async function doPause() {
  let blocked = 0;
  while (state.blocked_left > 0) {
    state.blocked_left -= 1;
    blocked += 1;
    emit("blocked", {
      reason: "value is not serializable: open HTTP response body (mock)",
      path: [state.function, "local http_response", "baml.http.Response", "_body"],
    });
    await new Promise((r) => setTimeout(r, 250 / SPEED));
  }
  const snap = writeSnapshot(performance.now() - pauseRequestedAt, blocked);
  emit("paused", { snapshot_path: snap.snapshot_path, state_path: snap.state_path, stats: snap.stats, wake: null });
  exitAfterFlush(75);
}

/** Contract section 9.2: the run suspends itself for a long sleep. */
function doSuspend() {
  const sleepEntry = earliestSleep();
  const snap = writeSnapshot(null, 0);
  const remaining = Math.max(0, sleepEntry.deadline_ts - Date.now());
  emit("paused", {
    snapshot_path: snap.snapshot_path, state_path: snap.state_path, stats: snap.stats,
    wake: { reason: "sleep", remaining_ms: Math.round(remaining), at_ts: Date.now() + Math.round(remaining) },
  });
  // The real worker needs some tens of milliseconds between `paused` and its
  // exit, and reads no command in that time. `mock_exit_delay_ms` widens the
  // window so that a test can send a command into it.
  leaving = true;
  const delay = Number(state.args.mock_exit_delay_ms);
  if (Number.isFinite(delay) && delay > 0) setTimeout(() => exitAfterFlush(75), delay);
  else exitAfterFlush(75);
}

// ------------------------------------------------ threads, sleeps, and waits
// The functions of quotes.baml run on several simulated threads. All of their
// state is in `state`, so a snapshot at any wait resumes correctly.

const SUSPEND = Symbol("suspend");
const parkedChecks = new Set();     // re-evaluated when a result arrives or a timer fires

function wakeParked() {
  for (const evaluate of [...parkedChecks]) evaluate();
}

function startThread(parent, name, line) {
  const thread = state.next_thread;
  state.next_thread += 1;
  const file = sourceFile(state.function);
  state.threads[thread] = { parent, name, line, file, ended: false };
  // Contract section 10.1: the `spawn` site in the parent thread.
  emit("thread_started", { thread, parent_thread: parent, file, line });
  return thread;
}

function endThread(thread) {
  const t = state.threads[thread];
  if (!t || t.ended) return;
  t.ended = true;
  for (const [name, entry] of Object.entries(state.sleeps)) {
    if (entry.thread === thread) delete state.sleeps[name];
  }
  emit("thread_ended", { thread });
}

/** Starts a sleep of `ms` simulated milliseconds on `thread`. The deadline is absolute. */
function startSleep(name, ms, thread = 1) {
  if (!state.sleeps[name]) state.sleeps[name] = { deadline_ts: Date.now() + ms / SPEED, thread };
}

const sleepDone = (name) => !state.sleeps[name] || Date.now() >= state.sleeps[name].deadline_ts;

/** The pending sleep with the earliest deadline, or null. `mock_fail` is a test knob and not a sleep of the program. */
function earliestSleep() {
  let best = null;
  for (const [name, entry] of Object.entries(state.sleeps)) {
    if (name === "mock_fail") continue;
    if (!best || entry.deadline_ts < best.deadline_ts) best = { name, ...entry };
  }
  return best;
}

/**
 * Parks the main flow until `check()` returns a value. Every live thread of
 * the mock is parked in a wait that can be re-issued while the main flow is
 * here, so this is also the place of the self-suspend rule (contract 9.2).
 */
function park(check) {
  return new Promise((resolve, reject) => {
    const timers = [];
    let settled = false;
    const cleanup = () => {
      settled = true;
      parkedChecks.delete(evaluate);
      for (const t of timers) clearTimeout(t);
      interrupt = null;
    };
    const evaluate = () => {
      if (settled) return;
      if (state.sleeps.mock_fail && sleepDone("mock_fail")) {
        cleanup();
        reject(new MockFailure("mock failure requested by mock_fail_after_ms"));
        return;
      }
      const value = check();
      if (value) {
        cleanup();
        resolve(value);
      }
    };
    parkedChecks.add(evaluate);
    interrupt = () => {
      cleanup();
      reject(PAUSE);
    };
    for (const entry of Object.values(state.sleeps)) {
      timers.push(setTimeout(evaluate, Math.max(0, entry.deadline_ts - Date.now()) + 1));
    }
    if (pauseRequested) return interrupt();
    evaluate();
    if (settled) return;
    //# Self-suspend: a durable run, every thread parked, the earliest sleep at least --sleep-suspend-ms away
    const next = earliestSleep();
    const remainingSimulated = next ? (next.deadline_ts - Date.now()) * SPEED : 0;
    if (state.durable && opts.sleepSuspendMs > 0 && next && remainingSimulated >= opts.sleepSuspendMs && !state.suspend_blocked) {
      if (state.blocked_left > 0) {
        // A blocked snapshot: the threads keep waiting in process, and the
        // worker reports the reason once.
        state.blocked_left = 0;
        state.suspend_blocked = true;
        emit("blocked", {
          reason: "value is not serializable: open HTTP response body (mock)",
          path: [state.function, "local http_response", "baml.http.Response", "_body"],
        });
        return;
      }
      cleanup();
      reject(SUSPEND);
    }
  });
}

class MockFailure extends Error {}

/** Starts `fn` as a remote call on its own thread. Returns the call id. */
function spawnRemote(fn, line, args, name) {
  const thread = startThread(1, name, line);
  position(thread, `${state.function}.<spawn>`, line, "remote_call", null);
  state.call_counter += 1;
  const callId = `${opts.run}-c${state.call_counter}`;
  const site = callSite(line);
  state.calls[callId] = { thread, function: fn, args, result: null, cancelled: false, name, file: site.file, line: site.line, caller: site.caller };
  state.order.push(callId);
  emit("remote_call", { call_id: callId, thread, function: fn, args, file: site.file, line: site.line, caller: site.caller });
  return callId;
}

/**
 * Contract sections 9.2 and 10.1: the thread that waited on the call was
 * cancelled. `cause` is one of `future_cancel`, `token`, `parent`, and
 * `unknown`, and `file`/`line` repeat the call site that was recorded when the
 * call was made.
 */
function cancelCall(callId, cause) {
  const call = state.calls[callId];
  if (!call || call.result || call.cancelled) return;
  call.cancelled = true;
  // The mock-only run argument "mock_cancel_cause" overrides the classification,
  // so that a scene can exercise the `unknown` case of an engine that cannot
  // tell what fired.
  const override = state.args?.mock_cancel_cause;
  emit("remote_cancel", {
    call_id: callId,
    thread: call.thread,
    file: call.file ?? null,
    line: call.line ?? null,
    cause: typeof override === "string" && override.length > 0 ? override : (cause ?? "unknown"),
  });
  endThread(call.thread);
}



// ------------------------------------------------------- quotes.baml values

const KINDS = ["Flight", "Hotel", "Car", "Tour"];
const NIGHTLY = { Flight: 420, Hotel: 135, Car: 48, Tour: 75 };
const VENDORS = { Flight: "Skyways", Hotel: "Casa Azul", Car: "Rodas", Tour: "Seven Hills Walks" };

function quoteRequest(city, kind, delayMs, options = {}) {
  return {
    city, kind, nights: 3, delay_ms: delayMs,
    traveler: { name: "Ada", loyalty_tier: 2 },
    options: { currency: "EUR", ...options },
  };
}

function quoteFor(request) {
  const amount = request.kind === "Flight" ? NIGHTLY.Flight : NIGHTLY[request.kind] * request.nights;
  const tier = request.traveler?.loyalty_tier;
  return {
    request,
    vendor: VENDORS[request.kind],
    price: { amount, currency: request.options?.currency ?? "USD" },
    tags: [request.kind.toLowerCase(), String(request.city).toLowerCase()],
    extras: { insurance: 12, late_checkout: 30 },
    note: tier === null || tier === undefined ? null : `loyalty tier ${tier} applied for ${request.traveler.name}`,
  };
}

/** The text that a parent sees for a failed remote call, as the real worker builds it. */
const remoteError = (fn, error) => `remote call to \`user.${fn}\` failed: ${error}`;

// ------------------------------------------------------ quotes.baml programs

function mockFailKnob() {
  const ms = Number(state.args.mock_fail_after_ms);
  if (Number.isFinite(ms) && ms > 0) startSleep("mock_fail", ms);
}

/** Re-announces the threads of a resumed run. Thread 1 is announced by main(). */
function announceLiveThreads() {
  for (const [id, t] of Object.entries(state.threads)) {
    // The `spawn` site travels in the snapshot, so a restored thread reports it again.
    if (!t.ended) emit("thread_started", { thread: Number(id), parent_thread: t.parent, file: t.file ?? null, line: t.line ?? null });
  }
}

async function runGetQuote() {
  const fn = state.function;
  const L = LINES[fn];
  const request = state.args.request;
  if (state.pc === "begin") {
    position(1, fn, L.print, "sysop", "baml.io.println");
    log(`[remote] ${request.kind} quote for ${request.city} (${request.delay_ms} ms)`);
    startSleep("delay", request.delay_ms);
    state.pc = "sleep";
  }
  if (state.pc === "sleep") {
    position(1, fn, L.sleep, "sysop", "baml.sys.sleep");
    await park(() => sleepDone("delay"));
    delete state.sleeps.delay;
    state.pc = "after";
  }
  if (request.options?.simulate === "unavailable") {
    return fail(`user.QuoteUnavailable {kind: ${request.kind}, city: ${JSON.stringify(request.city)}, reason: "no ${request.kind} vendor answers in ${request.city}"}`, L.fail);
  }
  finish(quoteFor(request));
}

async function runNap() {
  const fn = state.function;
  const L = LINES[fn];
  const seconds = state.args.seconds;
  if (state.pc === "begin") {
    position(1, fn, L.print, "sysop", "baml.io.println");
    log(`going to sleep for ${seconds} seconds`);
    startSleep("nap", seconds * 1000);
    state.pc = "sleep";
  }
  if (state.pc === "sleep") {
    position(1, fn, L.sleep, "sysop", "baml.sys.sleep");
    await park(() => sleepDone("nap"));
    delete state.sleeps.nap;
    state.pc = "after";
  }
  position(1, fn, L.after_print, "sysop", "baml.io.println");
  log("woke up");
  finish(`slept ${seconds} seconds`);
}

/** The settled calls in input order: `{ callId, call }`. */
const settledCalls = () => state.order.map((callId) => ({ callId, call: state.calls[callId] })).filter((c) => c.call.result);

async function runFanOut() {
  const fn = state.function;
  const L = LINES[fn];
  const city = state.args.city;
  if (state.pc === "begin") {
    mockFailKnob();
    position(1, fn, L.print, "sysop", "baml.io.println");
    log(`asking four vendors for ${city}`);
    const delays = [3000, 5000, 2000, 4000];
    const names = ["flight", "hotel", "car", "tour"];
    KINDS.forEach((kind, i) => {
      const options = kind === state.args.mock_unavailable ? { simulate: "unavailable" } : {};
      spawnRemote("remote_get_quote", L.spawns[i], { request: quoteRequest(city, kind, delays[i], options) }, names[i]);
    });
    position(1, fn, L.nap_print, "sysop", "baml.io.println");
    const napMs = Number(state.args.mock_nap_ms) > 0 ? Number(state.args.mock_nap_ms) : 12000;
    log(`sleeping ${Math.round(napMs / 1000)} seconds while the vendors work`);
    startSleep("nap", napMs);
    state.pc = "sleep";
  }
  if (state.pc === "sleep") {
    position(1, fn, L.sleep, "sysop", "baml.sys.sleep");
    await park(() => sleepDone("nap"));
    delete state.sleeps.nap;
    state.pc = "await_print";
  }
  if (state.pc === "await_print") {
    position(1, fn, L.await_print, "sysop", "baml.io.println");
    log("awake again, collecting the quotes");
    state.collector = startThread(1, "baml.future.all", L.await);
    state.pc = "await";
  }
  if (state.pc === "await") {
    position(1, fn, L.await, "await", null);
    // baml.future.all awaits its inputs in input order. The first error that
    // it observes cancels the inputs that are still pending.
    const outcome = await park(() => {
      const values = [];
      for (const callId of state.order) {
        const result = state.calls[callId].result;
        if (!result) return null;
        if (result.error != null) return { error: result.error };
        values.push(result.value);
      }
      return { values };
    });
    if (outcome.error != null) {
      // `baml.future.all` cancels the inputs that are still pending after the
      // first error: its catch arm runs `futures.map((g) -> { g.cancel() })`.
      // The inputs were spawned by this function, not by the helper thread
      // that `all` runs in, so the engine classifies the cancellation as the
      // cancellation of each input's own future, not as a cancelled parent.
      for (const callId of state.order) cancelCall(callId, "future_cancel");
      endThread(state.collector);
      return fail(remoteError("remote_get_quote", outcome.error), L.await);
    }
    endThread(state.collector);
    const quotes = outcome.values;
    let cheapest = null;
    const vendors = {};
    let total = 0;
    for (const q of quotes) {
      total += q.price.amount;
      vendors[q.request.kind] = q.vendor;
      if (cheapest === null || q.price.amount < cheapest.price.amount) cheapest = q;
    }
    finish({ city, quotes, total: { amount: total, currency: "EUR" }, vendors, cheapest });
  }
}

async function runRace() {
  const fn = state.function;
  const L = LINES[fn];
  const city = state.args.city;
  if (state.pc === "begin") {
    mockFailKnob();
    position(1, fn, L.print, "sysop", "baml.io.println");
    log(`racing three hotel vendors for ${city}`);
    const delays = [2000, 6000, 9000];
    const names = ["fast", "medium", "slow"];
    delays.forEach((delay, i) => spawnRemote("remote_get_quote", L.spawns[i], { request: quoteRequest(city, "Hotel", delay) }, names[i]));
    state.collector = startThread(1, "baml.future.race", L.await);
    state.pc = "await";
  }
  if (state.pc === "await") {
    position(1, fn, L.await, "await", null);
    const winner = await park(() => settledCalls()[0] ?? null);
    //# The race is decided: cancel the losers
    for (const callId of state.order) {
      // A `race` loser: the future that the thread settles was cancelled.
      if (callId !== winner.callId) cancelCall(callId, "future_cancel");
    }
    endThread(state.collector);
    if (winner.call.result.error != null) return fail(remoteError("remote_get_quote", winner.call.result.error), L.await);
    state.vars.winner = winner.call.result.value;
    state.pc = "after";
  }
  position(1, fn, L.after_print, "sysop", "baml.io.println");
  log(`the winner answered after ${state.vars.winner.request.delay_ms} ms`);
  finish(state.vars.winner);
}

async function runSettled() {
  const fn = state.function;
  const L = LINES[fn];
  const city = state.args.city;
  const kinds = ["Flight", "Car", "Tour"];
  if (state.pc === "begin") {
    position(1, fn, L.print, "sysop", "baml.io.println");
    log(`asking three vendors for ${city}, one of them will refuse`);
    const requests = [
      quoteRequest(city, "Flight", 2000),
      quoteRequest(city, "Car", 3000, { simulate: "unavailable" }),
      quoteRequest(city, "Tour", 4000),
    ];
    const names = ["flight", "car", "tour"];
    requests.forEach((request, i) => spawnRemote("remote_get_quote", L.spawns[i], { request }, names[i]));
    state.collector = startThread(1, "baml.future.all_settled", L.await);
    state.pc = "await";
  }
  if (state.pc === "await") {
    position(1, fn, L.await, "await", null);
    // all_settled waits for every input and cancels none.
    await park(() => (settledCalls().length === state.order.length ? true : null));
    endThread(state.collector);
    state.pc = "after";
  }
  const report = { city, succeeded: [], failed: [] };
  state.order.forEach((callId, i) => {
    const result = state.calls[callId].result;
    if (result.error != null) report.failed.push({ kind: kinds[i], error: remoteError("remote_get_quote", result.error) });
    else report.succeeded.push(result.value);
  });
  position(1, fn, L.after_print, "sysop", "baml.io.println");
  log(`${report.succeeded.length} quotes, ${report.failed.length} refused`);
  finish(report);
}

async function runDeadline() {
  const fn = state.function;
  const L = LINES[fn];
  const city = state.args.city;
  if (state.pc === "begin") {
    position(1, fn, L.print, "sysop", "baml.io.println");
    log(`asking a slow vendor for ${city} with a 2 second deadline`);
    // with_timeout runs the body on one thread and the deadline on another.
    spawnRemote("remote_get_quote", L.remote, { request: quoteRequest(city, "Tour", 6000) }, "work");
    const deadline = startThread(1, "with_timeout.<deadline>", L.await);
    position(deadline, "baml.future.with_timeout", L.await, "sysop", "baml.sys.sleep");
    startSleep("deadline", 2000, deadline);
    state.vars.deadline_thread = deadline;
    state.pc = "await";
  }
  if (state.pc === "await") {
    position(1, fn, L.await, "await", null);
    const callId = state.order[0];
    const outcome = await park(() => {
      const result = state.calls[callId].result;
      if (result) return { result };
      return sleepDone("deadline") ? { timeout: true } : null;
    });
    if (outcome.timeout) {
      //# The deadline passed: the body's thread is cancelled, and so is its remote call
      // `with_timeout` fires a cancel token that is linked to the body thread.
      cancelCall(callId, "token");
      endThread(state.vars.deadline_thread);
      state.vars.answer = `no tour quote for ${city}: operation timed out after 2000ms`;
    } else {
      endThread(state.vars.deadline_thread);
      if (outcome.result.error != null) return fail(remoteError("remote_get_quote", outcome.result.error), L.remote);
      state.vars.answer = outcome.result.value.vendor;
    }
    state.pc = "after";
  }
  position(1, fn, L.after_print, "sysop", "baml.io.println");
  log(state.vars.answer);
  finish(state.vars.answer);
}

// ----------------------------------------------- quotes.baml state dump

const CLASS_FIELDS = {
  Quote: { request: "QuoteRequest", price: "Money" },
  QuoteRequest: { kind: "QuoteKind", traveler: "Traveler" },
  TripReport: { total: "Money", cheapest: "Quote" },
};

/** A DumpValue for a value of the class `className` (contract section 2.5). */
function dumpInstance(value, className, depth = 0) {
  if (value === null || value === undefined) return { kind: "null", preview: "null" };
  if (className === "QuoteKind") return { kind: "instance", class: "QuoteKind", preview: `QuoteKind.${value}` };
  if (depth >= 5) return { kind: "omitted", preview: "…" };
  const fields = CLASS_FIELDS[className] ?? {};
  return {
    kind: "instance", class: className, preview: `${className} {…}`,
    children: Object.entries(value).map(([key, v]) => ({
      key, value: fields[key] ? dumpInstance(v, fields[key], depth + 1) : dumpValue(v, depth + 1),
    })),
  };
}

function dumpFuture(call) {
  if (call.cancelled) return { kind: "opaque", preview: "Future(cancelled)" };
  if (!call.result) return { kind: "opaque", preview: "Future(pending)" };
  if (call.result.error != null) {
    return { kind: "opaque", preview: "Future(error)", children: [{ key: "error", value: dumpValue(call.result.error) }] };
  }
  return { kind: "opaque", preview: "Future(ready)", children: [{ key: "value", value: dumpInstance(call.result.value, "Quote") }] };
}

function buildQuotesStateDump() {
  const fn = state.function;
  const L = LINES[fn];
  const file = QUOTES_FILE;
  const lineOf = { begin: L.print, sleep: L.sleep ?? L.print, await_print: L.await_print ?? L.print, await: L.await ?? L.print, after: L.after_print ?? L.ret };
  const next = earliestSleep();
  let parked = { kind: "runnable", detail: "" };
  if (state.pc === "sleep" && next) parked = { kind: "sysop", detail: `baml.sys.sleep (deadline ${new Date(next.deadline_ts).toISOString()})` };
  else if (state.pc === "await") parked = { kind: "await", detail: state.collector ? `future of thread ${state.collector}` : "future" };
  const locals = [];
  for (const [name, value] of Object.entries(state.args)) {
    if (name.startsWith("mock_")) continue;
    locals.push(name === "request"
      ? { name, type: "QuoteRequest", value: dumpInstance(value, "QuoteRequest") }
      : { name, type: typeof value === "number" ? "int" : "string", value: dumpValue(value) });
  }
  for (const callId of state.order) {
    const call = state.calls[callId];
    locals.push({ name: call.name, type: "Future<Quote>", value: dumpFuture(call) });
  }
  const threads = [{ thread: 1, name: fn, parked, frames: [{ function: fn, file, line: lineOf[state.pc] ?? L.print, locals }] }];
  for (const [id, t] of Object.entries(state.threads)) {
    if (t.ended) continue;
    const thread = Number(id);
    const callEntry = Object.entries(state.calls).find(([, c]) => c.thread === thread);
    const sleepEntry = Object.values(state.sleeps).find((entry) => entry.thread === thread);
    let threadParked = { kind: "await", detail: "the input futures" };
    const threadLocals = [];
    if (callEntry) {
      threadParked = { kind: "remote_call", detail: `${callEntry[1].function} ${callEntry[0]}` };
      threadLocals.push({ name: "request", type: "QuoteRequest", value: dumpInstance(callEntry[1].args.request, "QuoteRequest") });
    } else if (sleepEntry) {
      threadParked = { kind: "sysop", detail: `baml.sys.sleep (deadline ${new Date(sleepEntry.deadline_ts).toISOString()})` };
    }
    threads.push({ thread, name: t.name, parked: threadParked, frames: [{ function: `${fn}.<spawn>`, file, line: t.line, locals: threadLocals }] });
  }
  const calls = Object.values(state.calls);
  const by_kind = {
    string: { count: 4 + calls.length * 6, bytes: 64 + calls.length * 180 },
    instance: { count: calls.length * 3 + calls.filter((c) => c.result?.value).length * 5, bytes: calls.length * 220 },
    map: { count: calls.length * 2, bytes: calls.length * 96 },
    future: { count: calls.length + (state.collector ? 1 : 0), bytes: 48 * (calls.length + 1) },
  };
  const objects = Object.values(by_kind).reduce((n, k) => n + k.count, 0);
  const bytes = Object.values(by_kind).reduce((n, k) => n + k.bytes, 0);
  return { run: opts.run, segment: opts.segment, created_ts: Date.now(), threads, heap: { objects, bytes, by_kind } };
}

// ------------------------------------------------------------ program store
// Contract section 9.5. The "program" of the mock is a JSON payload with the
// text of the project's .baml files. Its hash is the SHA-256 of the payload.
// An entry is `<store>/<first two hex chars>/<hash>.bamlprog`:
//   "BAMLPROG" | format_version u32 LE | build_len u32 LE | runtime_build | payload_len u64 LE | payload
// This is the entry layout of the Rust crate bex_program_store.

const sha256 = (bytes) => crypto.createHash("sha256").update(bytes).digest("hex");

function compileProject(projectDir) {
  const files = [];
  for (const dir of [path.join(projectDir, "baml_src"), projectDir]) {
    let names = [];
    try { names = fs.readdirSync(dir).filter((n) => n.endsWith(".baml")).sort(); } catch { continue; }
    for (const name of names) files.push({ name: path.relative(projectDir, path.join(dir, name)), text: fs.readFileSync(path.join(dir, name), "utf8") });
    if (files.length > 0) break;
  }
  if (files.length === 0) throw new Error(`no .baml files in ${projectDir}`);
  const payload = Buffer.from(JSON.stringify({ mock_program: 1, files }));
  return { payload, hash: sha256(payload) };
}

const storePath = (hash) => path.join(opts.programStore, hash.slice(0, 2), `${hash}.bamlprog`);

/** The payload of the store entry for `hash`, or null. A reader verifies the hash and the runtime build. */
function readStore(hash) {
  let bytes;
  try { bytes = fs.readFileSync(storePath(hash)); } catch { return null; }
  if (bytes.length < 16 || bytes.subarray(0, 8).toString("latin1") !== "BAMLPROG") return null;
  const buildLen = bytes.readUInt32LE(12);
  if (24 + buildLen > bytes.length) return null;
  const build = bytes.subarray(16, 16 + buildLen).toString("utf8");
  const payload = bytes.subarray(24 + buildLen);
  if (bytes.readBigUInt64LE(16 + buildLen) !== BigInt(payload.length)) return null;
  if (build !== RUNTIME_BUILD) {
    diag(`the store entry for ${hash} was built by ${build}, not by ${RUNTIME_BUILD}; treating it as missing`);
    return null;
  }
  if (sha256(payload) !== hash) {
    diag(`the store entry for ${hash} does not match its hash; treating it as missing`);
    return null;
  }
  return payload;
}

/** Writes to a temporary file in the same directory and renames it. */
function writeStore(hash, payload) {
  const file = storePath(hash);
  fs.mkdirSync(path.dirname(file), { recursive: true });
  const build = Buffer.from(RUNTIME_BUILD, "utf8");
  const header = Buffer.alloc(16);
  header.write("BAMLPROG", 0, "latin1");
  header.writeUInt32LE(PROGRAM_FORMAT_VERSION, 8);
  header.writeUInt32LE(build.length, 12);
  const payloadLen = Buffer.alloc(8);
  payloadLen.writeBigUInt64LE(BigInt(payload.length), 0);
  const tmp = `${file}.tmp-${process.pid}`;
  fs.writeFileSync(tmp, Buffer.concat([header, build, payloadLen, payload]));
  fs.renameSync(tmp, file);
}

/**
 * Loads the program of this segment. `wanted` is the hash from --program-hash
 * or from the snapshot, or null for a start from --project.
 * Returns { hash, payload, source: "store" | "compile" } or { error }.
 */
function loadProgram(wanted) {
  if (wanted && opts.programStore) {
    const payload = readStore(wanted);
    if (payload) return { hash: wanted, payload, source: "store" };
  }
  if (!opts.project) return { error: `program ${wanted} is not in the store` };
  let compiled;
  try { compiled = compileProject(opts.project); } catch (err) { return { error: `cannot compile ${opts.project}: ${err.message}` }; }
  if (wanted && compiled.hash !== wanted) {
    return { error: `program ${wanted} is not in the store, and ${opts.project} compiles to the different program ${compiled.hash}` };
  }
  if (opts.programStore && !readStore(compiled.hash)) writeStore(compiled.hash, compiled.payload);
  return { hash: compiled.hash, payload: compiled.payload, source: "compile" };
}

/** True when the program text declares the top-level function `name`. */
function programHasFunction(payload, name) {
  try {
    const program = JSON.parse(payload.toString("utf8"));
    return program.files.some((f) => new RegExp(`^function ${name}\\(`, "m").test(f.text));
  } catch { return false; }
}

// ---------------------------------------------------------------- programs

function finish(value) {
  state.pc = "done";
  emit("thread_ended", { thread: 1 });
  emit("completed", { value });
  exitAfterFlush(0);
}

function fail(error, line) {
  emit("failed", { error, stack: [{ function: state.function, file: sourceFile(state.function), line }] });
  exitAfterFlush(1);
}

async function planLoop(L) {
  const v = state.vars;
  const fn = state.function;
  while (v.day < 4) {
    if (state.pc === "loop_print") {
      position(1, fn, L.print, "sysop", "baml.io.println");
      log(`planning day ${v.day}`);
      state.pc = "loop_sleep";
    }
    position(1, fn, L.sleep, "sysop", "baml.sys.sleep");
    await sleep(1500);
    v.ideas.push(`day ${v.day} in ${v.city}`);
    v.day += 1;
    state.pc = "loop_print";
    autoSnapshot();
  }
}

async function runPlanTrip() {
  const fn = state.function;
  const L = LINES[fn];
  const v = state.vars;
  if (state.pc === "begin") state.pc = "loop_print";
  if (state.pc === "loop_print" || state.pc === "loop_sleep") {
    await planLoop(L);
    state.pc = "remote";
  }
  if (state.pc === "remote") {
    remoteCall(1, "remote_fetch_weather", L.remote, remoteArgs());
    state.pc = "remote_wait";
  }
  if (state.pc === "remote_wait") {
    position(1, fn, L.remote, "remote_call", null);
    const callId = Object.keys(state.calls)[0];
    const result = await waitForResult(callId);
    if (result.error != null) return fail(`remote call remote_fetch_weather failed: ${result.error}`, L.remote);
    v.weather = result.value;
  }
  finish({ city: v.city, ideas: v.ideas, weather: v.weather });
}

async function runPlanTripParallel() {
  const fn = state.function;
  const L = LINES[fn];
  const v = state.vars;
  if (state.pc === "begin") {
    position(1, fn, L.start_print, "sysop", "baml.io.println");
    log("starting the weather lookup in the background");
    state.spawned = { thread: 2, call_id: null, ended: false };
    emit("thread_started", { thread: 2, parent_thread: 1, file: sourceFile(fn), line: L.spawn });
    state.spawned.call_id = remoteCall(2, "remote_fetch_weather", L.spawn, remoteArgs());
    state.pc = "loop_print";
  }
  if (state.pc === "loop_print" || state.pc === "loop_sleep") {
    await planLoop(L);
    state.pc = "await_print";
  }
  if (state.pc === "await_print") {
    position(1, fn, L.await_print, "sysop", "baml.io.println");
    log("waiting for the weather");
    state.pc = "await";
  }
  if (state.pc === "await") {
    position(1, fn, L.await, "await", null);
    const result = await waitForResult(state.spawned.call_id);
    if (result.error != null) return fail(`remote call remote_fetch_weather failed: ${result.error}`, L.await);
    v.weather = result.value;
  }
  finish({ city: v.city, ideas: v.ideas, weather: v.weather });
}

async function runFetchWeather() {
  const fn = state.function;
  const L = LINES[fn];
  const v = state.vars;
  if (state.pc === "begin") state.pc = "print";
  if (state.pc === "print") {
    position(1, fn, L.print, "sysop", "baml.io.println");
    log(`[remote] looking up weather for ${v.city}`);
    state.pc = "sleep";
  }
  if (state.pc === "sleep") {
    position(1, fn, L.sleep, "sysop", "baml.sys.sleep");
    // `mock_sleep_ms` is real time, so it is scaled up by SPEED before the
    // sleep divides by SPEED again.
    const real = Number(state.args.mock_sleep_ms);
    await sleep(Number.isFinite(real) && real > 0 ? real * SPEED : 3000);
  }
  if (String(v.city).toLowerCase() === "atlantis") {
    return fail("baml.errors.Io: no weather station in Atlantis (mock failure)", L.ret);
  }
  finish(`sunny in ${v.city}`);
}

const PROGRAMS = {
  durable_plan_trip: runPlanTrip,
  plan_trip: runPlanTrip,
  durable_plan_trip_parallel: runPlanTripParallel,
  remote_fetch_weather: runFetchWeather,
  remote_get_quote: runGetQuote,
  durable_fan_out: runFanOut,
  durable_race: runRace,
  durable_settled: runSettled,
  durable_deadline: runDeadline,
  durable_nap: runNap,
};

/** Returns a message when the arguments of `fn` do not fit its parameters. */
function checkArguments(fn, args) {
  if (fn === "remote_get_quote") {
    const r = args.request;
    const ok = r && typeof r === "object" && typeof r.city === "string" && KINDS.includes(r.kind)
      && Number.isInteger(r.nights) && Number.isInteger(r.delay_ms) && r.traveler && typeof r.traveler.name === "string"
      && r.options && typeof r.options === "object";
    return ok ? null : "missing or malformed argument: request (QuoteRequest)";
  }
  if (fn === "durable_nap") return Number.isInteger(args.seconds) && args.seconds >= 0 ? null : "missing argument: seconds (int)";
  return typeof args.city === "string" ? null : "missing argument: city (string)";
}

// ---------------------------------------------------------------- commands

/** True once the worker has reported `paused`: it takes no command any more. */
let leaving = false;

function handleCommand(line) {
  const text = line.trim();
  if (!text || leaving) return;
  let msg;
  try {
    msg = JSON.parse(text);
  } catch {
    diag(`ignoring a command that is not JSON: ${text}`);
    return;
  }
  switch (msg.type) {
    case "pause":
      if (!state) return;
      if (!state.durable) {
        emit("blocked", { reason: `function ${state.function} is not durable`, path: [] });
        return;
      }
      if (pauseRequested) return;
      pauseRequested = true;
      pauseRequestedAt = performance.now();
      emit("pausing", { waiting_on: describeParked() });
      if (interrupt) interrupt();
      break;
    case "cancel":
      cancel("cancelled");
      break;
    case "remote_result":
      if (state && typeof msg.call_id === "string") deliverResult(msg);
      break;
    default:
      diag(`ignoring unknown command type ${JSON.stringify(msg.type)}`);
  }
}

/** Contract section 7.1: `cancelled` is the last event before exit code 130. */
let cancelling = false;
function cancel(why) {
  if (cancelling) return;
  cancelling = true;
  diag(why);
  emit("cancelled");
  exitAfterFlush(130);
}

function describeParked() {
  const out = [];
  if (state.pc === "loop_sleep" || (state.pc === "sleep" && !QUOTE_FUNCTIONS.has(state.function))) out.push("baml.sys.sleep (parked, can be re-issued)");
  for (const name of Object.keys(state.sleeps ?? {})) {
    if (name !== "mock_fail") out.push(`baml.sys.sleep ${name} (parked, can be re-issued)`);
  }
  for (const [callId, call] of Object.entries(state.calls)) {
    if (!call.result && !call.cancelled) out.push(`remote call ${call.function} ${callId} (parked)`);
  }
  return out;
}

// -------------------------------------------------------------------- main

async function main() {
  const rl = readline.createInterface({ input: process.stdin });
  rl.on("line", handleCommand);
  rl.on("close", () => {
    // End of input on stdin means cancel (contract section 7.1): the site
    // server is gone, and the worker must not linger as an orphan.
    cancel("stdin closed; cancelled");
  });
  // The reader of stdout may be gone as well.
  process.stdout.on("error", () => process.exit(130));

  let resumeStats = null;
  if (opts.resume) {
    const tRead = performance.now();
    let loaded;
    try {
      loaded = JSON.parse(fs.readFileSync(opts.resume, "utf8"));
    } catch (err) {
      emit("hello", { mode: "resume", function: "", durable: false });
      emit("failed", { error: `cannot read snapshot ${opts.resume}: ${err.message}`, stack: [] });
      return exitAfterFlush(1);
    }
    if (loaded.mock_snapshot !== 1 || !PROGRAMS[loaded.function]) {
      emit("hello", { mode: "resume", function: String(loaded.function ?? ""), durable: false });
      emit("failed", { error: `${opts.resume} is not a mock snapshot`, stack: [] });
      return exitAfterFlush(1);
    }
    delete loaded.saved_by;
    state = { threads: {}, next_thread: 2, sleeps: {}, order: [], collector: null, suspend_blocked: false, program_hash: null, ...loaded };
    const decodeMs = performance.now() - tRead;
    resumeStats = {
      process_start_ms: round(Math.max(0, process.uptime() * 1000 - (performance.now() - T0))),
      program_load_ms: null,
      decode_ms: round(decodeMs),
      first_exec_ms: null,
      program_source: null,
    };
  } else {
    let args;
    try {
      args = JSON.parse(opts.jsonArgs ?? "{}");
    } catch (err) {
      emit("hello", { mode: "start", function: opts.start, durable: opts.start.includes("durable") });
      emit("failed", { error: `--json-args is not JSON: ${err.message}`, stack: [] });
      return exitAfterFlush(1);
    }
    state = newState(opts.start, args ?? {});
  }

  //# Load the program: from the store by hash, or by compiling --project (contract section 9.5)
  const tLoad = performance.now();
  const wanted = opts.resume ? state.program_hash : (opts.programHash ?? null);
  const loadedProgram = loadProgram(wanted);
  const hello = {
    mode: opts.resume ? "resume" : "start", function: state.function, durable: state.durable,
    program_hash: loadedProgram.hash ?? wanted ?? "",
    // The site server learns the build of its workers from `hello`, and
    // refuses a fetched program that another build wrote.
    runtime_build: RUNTIME_BUILD,
    // Mock only: lets verify.mjs check the environment that the site server passes
    // on, and where the program of this segment came from.
    mock_env: { allow_direct: process.env.BAML_CLI_ALLOW_DIRECT ?? null, marker: process.env.VERIFY_MARKER ?? null },
    mock_program_source: loadedProgram.source ?? null,
    mock_project: opts.project ?? null,
  };
  emit("hello", hello);
  if (loadedProgram.error) {
    emit("failed", { error: loadedProgram.error, stack: [] });
    return exitAfterFlush(1);
  }
  state.program_hash = loadedProgram.hash;
  if (resumeStats) {
    resumeStats.program_load_ms = round(performance.now() - tLoad);
    resumeStats.program_source = loadedProgram.source;
  }

  const program = PROGRAMS[state.function];
  if (!program || !programHasFunction(loadedProgram.payload, state.function)) {
    emit("failed", { error: `function ${state.function} not found in the program ${loadedProgram.hash}`, stack: [] });
    return exitAfterFlush(1);
  }
  const badArguments = checkArguments(state.function, state.args);
  if (badArguments) {
    emit("failed", { error: badArguments, stack: [] });
    return exitAfterFlush(1);
  }

  if (resumeStats) {
    resumeStats.first_exec_ms = round(performance.now() - T0);
    emit("resumed", { stats: resumeStats });
  }
  // The root thread of a segment has no `spawn` site (contract section 10.1).
  emit("thread_started", { thread: 1, parent_thread: null, file: null, line: null });
  if (opts.resume && state.spawned && !state.spawned.ended) {
    const spawnLine = LINES[state.function]?.spawn ?? null;
    emit("thread_started", {
      thread: state.spawned.thread,
      parent_thread: 1,
      file: spawnLine === null ? null : sourceFile(state.function),
      line: spawnLine,
    });
  }
  if (opts.resume) announceLiveThreads();
  const startResults = [];
  for (const text of opts.remoteResults) {
    try {
      startResults.push(JSON.parse(text));
    } catch (err) {
      diag(`ignoring --remote-result that is not JSON: ${err.message}`);
    }
  }
  if (opts.resume) {
    //# A restored wait is not announced again: tell the supervisor what this process waits on
    const offered = new Set(startResults.map((r) => r?.call_id));
    for (const [callId, call] of Object.entries(state.calls ?? {})) {
      if (call.result || call.cancelled) continue;
      emit("remote_wait", { call_id: callId, thread: call.thread, function: call.function, has_result: offered.has(callId) });
    }
  }
  // Results that arrived while the run had no process are taken in the order
  // of their arrival (`ts`), like the real worker's ordered replay. The sort is
  // stable, so results without `ts` keep the order of the arguments.
  startResults.sort((a, b) => (Number(a?.ts) || 0) - (Number(b?.ts) || 0));
  for (const result of startResults) deliverResult(result);

  try {
    await program();
  } catch (err) {
    if (err === PAUSE) return doPause();
    if (err === SUSPEND) return doSuspend();
    if (err instanceof MockFailure) return fail(err.message, LINES[state.function].await ?? LINES[state.function].print);
    emit("failed", { error: `mock worker bug: ${err?.stack ?? err}`, stack: [] });
    exitAfterFlush(1);
  }
}

main();
