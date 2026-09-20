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
// Mock-only behavior: remote_fetch_weather fails for the city "Atlantis".
//
// The worker does not know which site hosts it (contract section 8). The site
// server of the caller places a remote call on a site of the remote pool.

import fs from "node:fs";
import path from "node:path";
import readline from "node:readline";

const T0 = performance.now();
const SOURCE_FILE = "baml_src/trip.baml";
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
};

// ---------------------------------------------------------------- arguments

function usage(message) {
  process.stderr.write(`mock-worker: ${message}\n`);
  process.stderr.write(
    "usage: mock-worker.mjs --project <dir> --run <run-id> --segment <n> --snapshot-dir <dir>\n" +
    "         ( --start <function> --json-args '<json>' | --resume <snapshot-path> )\n" +
    "         [--remote-result '<json>']...\n");
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
      default: usage(`unknown argument ${flag}`);
    }
  }
  if (!out.run || !/^[a-z0-9-]+$/.test(out.run)) usage("--run must match [a-z0-9-]+");
  if (!Number.isInteger(out.segment) || out.segment < 1) usage("--segment must be a positive integer");
  if (!out.snapshotDir) usage("--snapshot-dir is required");
  if (!out.project) usage("--project is required");
  if (!!out.start === !!out.resume) usage("exactly one of --start and --resume is required");
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
  };
}

function position(thread, fn, line, reason, op) {
  const key = `${SOURCE_FILE}:${line}`;
  if (lastPosition.get(thread) === key) return;
  lastPosition.set(thread, key);
  emit("position", { thread, function: fn, file: SOURCE_FILE, line, reason, op: op ?? null });
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
  const waiter = resultWaiters.get(callId);
  if (waiter) {
    resultWaiters.delete(callId);
    waiter(result);
  }
}

/** Reaches a `remote_` call on `thread`. Returns the call id. */
function remoteCall(thread, fn, line, args) {
  state.call_counter += 1;
  const callId = `${opts.run}-c${state.call_counter}`;
  position(thread, state.function, line, "remote_call", null);
  state.calls[callId] = { thread, function: fn, args, result: null };
  emit("remote_call", { call_id: callId, thread, function: fn, args });
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
    frames: [{ function: fn, file: SOURCE_FILE, line: currentLine(), locals }],
  }];
  if (state.spawned && !state.spawned.ended) {
    const call = state.calls[state.spawned.call_id];
    threads.push({
      thread: state.spawned.thread,
      name: `${fn}.<spawn>`,
      parked: { kind: "remote_call", detail: `${call.function} ${state.spawned.call_id}` },
      frames: [{
        function: `${fn}.<spawn>`, file: SOURCE_FILE, line: LINES[fn].spawn,
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
  emit("paused", { snapshot_path: snap.snapshot_path, state_path: snap.state_path, stats: snap.stats });
  exitAfterFlush(75);
}

// ---------------------------------------------------------------- programs

function finish(value) {
  state.pc = "done";
  emit("thread_ended", { thread: 1 });
  emit("completed", { value });
  exitAfterFlush(0);
}

function fail(error, line) {
  emit("failed", { error, stack: [{ function: state.function, file: SOURCE_FILE, line }] });
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
    emit("thread_started", { thread: 2, parent_thread: 1 });
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
};

// ---------------------------------------------------------------- commands

function handleCommand(line) {
  const text = line.trim();
  if (!text) return;
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
  if (state.pc === "loop_sleep" || state.pc === "sleep") out.push("baml.sys.sleep (parked, can be re-issued)");
  for (const [callId, call] of Object.entries(state.calls)) {
    if (!call.result) out.push(`remote call ${call.function} ${callId} (parked)`);
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
    state = loaded;
    const decodeMs = performance.now() - tRead;
    resumeStats = {
      process_start_ms: round(Math.max(0, process.uptime() * 1000 - (performance.now() - T0))),
      program_load_ms: round(tRead - T0),
      decode_ms: round(decodeMs),
      first_exec_ms: null,
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

  emit("hello", {
    mode: opts.resume ? "resume" : "start", function: state.function, durable: state.durable,
    // Mock only: lets verify.mjs check the environment that the site server passes on.
    mock_env: { allow_direct: process.env.BAML_CLI_ALLOW_DIRECT ?? null, marker: process.env.VERIFY_MARKER ?? null },
  });

  const program = PROGRAMS[state.function];
  if (!program) {
    emit("failed", { error: `function ${state.function} not found in ${opts.project}`, stack: [] });
    return exitAfterFlush(1);
  }
  if (typeof state.vars.city !== "string") {
    emit("failed", { error: `missing argument: city (string)`, stack: [] });
    return exitAfterFlush(1);
  }

  if (resumeStats) {
    resumeStats.first_exec_ms = round(performance.now() - T0);
    emit("resumed", { stats: resumeStats });
  }
  emit("thread_started", { thread: 1, parent_thread: null });
  if (opts.resume && state.spawned && !state.spawned.ended) {
    emit("thread_started", { thread: state.spawned.thread, parent_thread: 1 });
  }
  for (const text of opts.remoteResults) {
    try {
      deliverResult(JSON.parse(text));
    } catch (err) {
      diag(`ignoring --remote-result that is not JSON: ${err.message}`);
    }
  }

  try {
    await program();
  } catch (err) {
    if (err === PAUSE) return doPause();
    emit("failed", { error: `mock worker bug: ${err?.stack ?? err}`, stack: [] });
    exitAfterFlush(1);
  }
}

main();
