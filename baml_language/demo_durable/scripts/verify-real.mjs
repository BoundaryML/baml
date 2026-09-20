#!/usr/bin/env node
// End-to-end check of the three site servers with the REAL worker (`baml-cli worker`).
//
//   cd baml_language && cargo build -p baml_cli
//   node demo_durable/scripts/verify-real.mjs
//
//   WORKER_BIN=/abs/path/baml-cli   worker binary (default: ../target/debug/baml-cli)
//   PORT_BASE=<n>                   first port: local on n, cloud on n + 1, cloud2 on n + 2.
//                                   Default: 18787, or 8787 with USE_RUNNING=1 (the default of dev.sh)
//   RUNS_TAG=<tag>                  added to the run store names of the servers that this script starts
//   USE_RUNNING=1                   use the three site servers that already run on PORT_BASE
//                                   (started with the real WORKER_CMD, for example by dev.sh)
//   KEEP=1                          keep the run stores for inspection
//   RECORD=<file>                   write every SSE event of the three sites to <file> (JSON lines)
//   ONLY=a,b,c                      run only the named scenes (a b c d e f g h i j k m n o l p, the review
//                                   scenes 1 2 3 4 of contract section 9.7, the end-to-end
//                                   scenes 5 6 7, and the contract section 9 scenes
//                                   x q r s t u v w y z)
//   CHAOS_ALL=<json>                every site server of this run gets this CHAOS object (contract
//                                   section 9.3), so that any scene can be repeated with a slow
//                                   controller, for example
//                                   CHAOS_ALL='{"dispatch_delay_ms":600,"result_delay_ms":500,"duplicate_results":true}' ONLY=5,6,1,2,4
//                                   Checks that assert exact timing or exact site-to-site event
//                                   counts may fail under it; the results of the runs must not.
//   LEGACY=1                        the worker predates contract section 9. The servers run with
//                                   WORKER_LEGACY=1, and the section 9 scenes are skipped. This is also
//                                   chosen when `baml-cli worker --help` does not list --program-store.
//
// Without USE_RUNNING the script starts its own site servers with run stores in
// server/.baml/verify-real<RUNS_TAG>-<site> and stops them at the end. Its
// default ports are not the ports of dev.sh, so it does not collide with a demo
// that is running, and it refuses to start when one of its ports is in use.
// `verify.mjs` is the same kind of check for the mock worker; this file only
// asserts what the real worker does.
//
// Scenes:
//   a  durable_plan_trip completes (remote child on a site of the pool)
//   b  the central scene: pause during the remote wait, the child finishes while
//      no parent process exists, resume in a new process
//   c  pause inside the loop, resume, no println line lost or duplicated
//   d  migration: pause on local, resume on cloud; the remote call of the migrated
//      run is placed on cloud2, and the result returns to cloud
//   e  fork of a paused run: source and fork complete independently
//   f  kill -9: plan_trip is lost, durable_plan_trip recovers from an automatic snapshot
//   g  durable_plan_trip_parallel: a pause with two threads and a pending future, resume
//      (a worker that predates section 9 answers the pause with `blocked`)
//   h  kill during the remote wait, resume with the stored result
//   i  cancel (exit code 130)
//   j  two pauses in one run; the remote result arrives on stdin of the third segment
//   k  a pause request that arrives while the worker still loads the program
//   m  migration chain local -> cloud -> cloud2: the remote result arrives while the
//      run has no process and is forwarded along migrated_to
//   n  a run on cloud2 calls into cloud
//   o  unknown and malformed site names are refused with 400: `resume {site}` before the
//      status of the run is looked at, and `parent.site` of a remote call
//   l  the local site server restarts during a durable run (skipped with USE_RUNNING=1)
//   p  cloud2 restarts with REMOTE_POOL=["cloud2"]: the remote call fails with
//      `no remote site available` (skipped with USE_RUNNING=1)
//
// Scenes of contract section 9. They need a worker with --program-store,
// --program-hash, --sleep-suspend-ms, `paused.wake`, and `remote_cancel`:
//   x  program store: cloud and cloud2 fetch the program on first use, verify it,
//      and run remote children by hash without --project (runs first)
//   q  durable_nap: sleeping, the timer resumes the run, the run completes
//   r  manual resume of a sleeping run, five resumes at once, cancel while sleeping
//   s  durable_fan_out: round-robin placement, class-valued results stored while
//      the parent sleeps, all delivered at the resume
//   t  durable_race: the losers are cancelled on their sites, late results are discarded
//   u  durable_deadline: with_timeout cancels the remote call
//   v  durable_settled: all_settled with one failing child
//   w  a parent that is cancelled cancels its children
//   5  durable_plan_trip_parallel is paused while its spawned remote call is
//      outstanding and resumes on cloud2, which never compiled the program
//   6  kill -9 of a fan-out whose parent stays in process (local restarts with
//      SLEEP_SUSPEND_MS=600000): recovery from an automatic snapshot with five
//      threads (skipped with USE_RUNNING=1)
//   7  resume timings: the program from the store against a compile
//   y  the local site server restarts while runs sleep (skipped with USE_RUNNING=1)
//   z  scenes s, t, u again under CHAOS (skipped with USE_RUNNING=1)

import { spawn, spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import fs from "node:fs";
import net from "node:net";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { isDeepStrictEqual } from "node:util";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const SERVER_DIR = path.join(ROOT, "server");
const WORKER_BIN = path.resolve(process.env.WORKER_BIN ?? path.join(ROOT, "..", "target", "debug", "baml-cli"));
const USE_RUNNING = process.env.USE_RUNNING === "1";
const ONLY = process.env.ONLY ? new Set(process.env.ONLY.split(",")) : null;
const PORT_BASE = Number(process.env.PORT_BASE ?? (USE_RUNNING ? "8787" : "18787"));
const RUNS_TAG = process.env.RUNS_TAG ?? "";
if (!Number.isInteger(PORT_BASE) || PORT_BASE < 1 || PORT_BASE > 65533) throw new Error(`PORT_BASE is not a port number: ${process.env.PORT_BASE}`);
if (!/^[A-Za-z0-9_-]*$/.test(RUNS_TAG)) throw new Error(`RUNS_TAG may contain only letters, digits, '-' and '_': ${RUNS_TAG}`);
const SITE_NAMES = ["local", "cloud", "cloud2"];
// True when the worker binary implements contract section 9.
function workerHasPhase3() {
  if (process.env.LEGACY === "1") return false;
  const help = spawnSync(WORKER_BIN, ["worker", "--help"], { env: { ...process.env, BAML_CLI_ALLOW_DIRECT: "1" }, encoding: "utf8" });
  return `${help.stdout ?? ""}${help.stderr ?? ""}`.includes("--program-store");
}
let PHASE3 = false;
const SITES = Object.fromEntries(SITE_NAMES.map((name, i) => [name, {
  port: PORT_BASE + i, base: `http://127.0.0.1:${PORT_BASE + i}`, runs: `.baml/verify-real${RUNS_TAG}-${name}`,
  programs: `.baml/verify-real-programs${RUNS_TAG}-${name}`,
}]));
// The registry that every site server receives (contract section 8.1).
const REGISTRY = Object.fromEntries(SITE_NAMES.map((name) => [name, SITES[name].base]));
const url = (site, p) => `${SITES[site].base}${p}`;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

let passed = 0;
const failures = [];
function check(name, ok, detail = "") {
  if (ok) {
    passed += 1;
    console.log(`  ok    ${name}`);
  } else {
    failures.push(name);
    console.log(`  FAIL  ${name} ${detail}`);
  }
}
function section(title) {
  console.log(`\n== ${title}`);
}
const measurements = [];
function measure(scene, what, value) {
  measurements.push({ scene, what, value });
}

// ------------------------------------------------------------------ servers

const servers = {};

function portInUse(port) {
  return new Promise((resolve) => {
    const socket = net.connect({ host: "127.0.0.1", port });
    socket.once("connect", () => { socket.destroy(); resolve(true); });
    socket.once("error", () => resolve(false));
  });
}

// `extraEnv` overrides the environment of one server, for example REMOTE_POOL.
async function startServer(site, extraEnv = {}) {
  const cfg = SITES[site];
  if (stopping) throw new Error("this run is stopping");
  if (await portInUse(cfg.port)) throw new Error(`port ${cfg.port} (site ${site}) is already in use; set PORT_BASE to a free range`);
  const { PEER_URL: _peer, REMOTE_POOL: _pool, CHAOS: _chaos, SLEEP_SUSPEND_MS: _suspend, WORKER_LEGACY: _legacy, ...inherited } = process.env;
  const child = spawn("baml", ["run", "main", "--log", "warn"], {
    cwd: SERVER_DIR,
    env: {
      ...inherited,
      SITE: site, PORT: String(cfg.port), SITES: JSON.stringify(REGISTRY), RUNS_DIR: cfg.runs,
      PROGRAMS_DIR: cfg.programs,
      // A worker that predates contract section 9 rejects the new flags.
      ...(PHASE3 ? {} : { WORKER_LEGACY: "1" }),
      ...(process.env.CHAOS_ALL ? { CHAOS: process.env.CHAOS_ALL } : {}),
      WORKER_CMD: JSON.stringify([WORKER_BIN, "worker"]),
      BAML_AGENT_SKILL_CHECK: process.env.BAML_AGENT_SKILL_CHECK ?? "off",
      ...extraEnv,
    },
    stdio: ["ignore", "pipe", "pipe"],
  });
  child.stdout.on("data", (d) => process.stdout.write(`    [${site}] ${d}`));
  child.stderr.on("data", (d) => process.stdout.write(`    [${site}] ${d}`));
  servers[site] = child;
  for (let i = 0; i < 100; i++) {
    try {
      const res = await fetch(url(site, "/api/info"));
      if (res.ok) return;
    } catch { /* not up yet */ }
    await sleep(100);
  }
  throw new Error(`site ${site} did not start`);
}

async function stopServer(site) {
  const child = servers[site];
  if (!child) return;
  delete servers[site];
  const exited = new Promise((r) => child.once("exit", r));
  child.kill("SIGTERM");
  await Promise.race([exited, sleep(3000)]);
  if (child.exitCode === null && child.signalCode === null) child.kill("SIGKILL");
}

// ------------------------------------------------------------- run stores

// Two instances must not share a run store. Each store is claimed with the file
// <store>/.instance.lock, which holds the pid of this process.
let storesClaimed = false;
const lockPath = (site) => path.join(SERVER_DIR, SITES[site].runs, ".instance.lock");

function storeOwner(site) {
  let pid = NaN;
  try { pid = Number(fs.readFileSync(lockPath(site), "utf8").trim()); } catch { return null; }
  if (!Number.isInteger(pid) || pid <= 0 || pid === process.pid) return null;
  try { process.kill(pid, 0); return pid; } catch { return null; }
}

// Checks every port and every run store before anything is deleted or started.
async function claimStores() {
  for (const site of SITE_NAMES) {
    if (await portInUse(SITES[site].port)) throw new Error(`port ${SITES[site].port} (site ${site}) is already in use; set PORT_BASE to a free range`);
  }
  for (const site of SITE_NAMES) {
    const owner = storeOwner(site);
    if (owner !== null) throw new Error(`run store ${path.join(SERVER_DIR, SITES[site].runs)} is used by pid ${owner}; set RUNS_TAG to give this run its own stores`);
  }
  for (const site of SITE_NAMES) {
    const dir = path.join(SERVER_DIR, SITES[site].runs);
    fs.rmSync(dir, { recursive: true, force: true });
    fs.rmSync(path.join(SERVER_DIR, SITES[site].programs), { recursive: true, force: true });
    fs.mkdirSync(dir, { recursive: true });
    fs.writeFileSync(lockPath(site), `${process.pid}\n`);
  }
  storesClaimed = true;
}

function releaseStores(remove) {
  if (!storesClaimed) return;
  for (const site of SITE_NAMES) {
    const dir = path.join(SERVER_DIR, SITES[site].runs);
    if (remove) {
      fs.rmSync(dir, { recursive: true, force: true });
      fs.rmSync(path.join(SERVER_DIR, SITES[site].programs), { recursive: true, force: true });
    } else fs.rmSync(lockPath(site), { force: true });
  }
}

// A terminated run stops its servers. Workers exit when their server does.
let stopping = false;
for (const signal of ["SIGINT", "SIGTERM", "SIGHUP"]) {
  process.on(signal, async () => {
    if (stopping) return;
    stopping = true;
    console.log(`\n${signal}: stopping the site servers of this run`);
    await Promise.all(Object.keys(servers).map(stopServer));
    releaseStores(false);
    process.exit(130);
  });
}

// --------------------------------------------------------------------- http

async function api(site, method, p, body) {
  const res = await fetch(url(site, p), {
    method,
    headers: body === undefined ? {} : { "content-type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const text = await res.text();
  let json = null;
  try { json = text.length ? JSON.parse(text) : null; } catch { /* keep null */ }
  return { status: res.status, headers: res.headers, json, text };
}
const get = (site, p) => api(site, "GET", p);
const post = (site, p, body = {}) => api(site, "POST", p, body);

async function waitFor(what, fn, timeoutMs = 40000) {
  const started = Date.now();
  while (Date.now() - started < timeoutMs) {
    const value = await fn();
    if (value) return value;
    await sleep(25);
  }
  throw new Error(`timed out waiting for ${what}`);
}

async function waitStatus(site, id, statuses, timeoutMs = 40000) {
  const want = Array.isArray(statuses) ? statuses : [statuses];
  return waitFor(`${site}/${id} to be ${want.join("|")}`, async () => {
    const r = await get(site, `/api/runs/${id}`);
    return r.json && want.includes(r.json.status) ? r.json : null;
  }, timeoutMs);
}

function pidAlive(pid) {
  try { process.kill(pid, 0); return true; } catch { return false; }
}

// ---------------------------------------------------------------------- sse

const record = process.env.RECORD ? fs.createWriteStream(process.env.RECORD) : null;

class Stream {
  constructor(site) {
    this.site = site;
    this.events = [];
    this.controller = new AbortController();
  }
  async open() {
    const res = await fetch(url(this.site, "/api/events"), { signal: this.controller.signal });
    this.reader = res.body.getReader();
    this.pump();
    await waitFor(`first SSE message of ${this.site}`, () => this.events.length > 0, 5000);
  }
  async pump() {
    const decoder = new TextDecoder();
    let buffer = "";
    try {
      while (true) {
        const { value, done } = await this.reader.read();
        if (done) break;
        buffer += decoder.decode(value, { stream: true });
        let idx;
        while ((idx = buffer.indexOf("\n\n")) >= 0) {
          const block = buffer.slice(0, idx);
          buffer = buffer.slice(idx + 2);
          if (!block.startsWith("data: ")) continue;
          const ev = JSON.parse(block.slice(6));
          Object.defineProperty(ev, "rx", { value: Date.now(), enumerable: false });
          this.events.push(ev);
          if (record && ev.type !== "init" && ev.type !== "run") record.write(`${JSON.stringify(ev)}\n`);
        }
      }
    } catch { /* aborted */ }
  }
  close() { this.controller.abort(); }
  find(pred) { return this.events.find(pred); }
  all(pred) { return this.events.filter(pred); }
  waitEvent(what, pred, timeoutMs = 40000) {
    return waitFor(`SSE ${this.site}: ${what}`, () => this.events.find(pred), timeoutMs);
  }
}

// ------------------------------------------------------------------ helpers

// The worker processes of this instance: processes whose --snapshot-dir lies in
// one of the run stores of the three sites. Workers of another instance of the
// demo on the same machine are not listed.
let runStores = [];
function leftoverWorkers() {
  const out = spawnSync("ps", ["-axo", "pid=,command="], { encoding: "utf8" }).stdout;
  return out.split("\n").filter((line) => runStores.some((dir) => line.includes(`--snapshot-dir ${dir}/`))).map((line) => line.trim().slice(0, 160)).join("\n");
}

// Placement is round-robin (contract section 9.3), so a scene takes the site of
// a child from the `remote_dispatched` event and reads that site's stream.
const STREAMS = {};
const POOL = ["cloud", "cloud2"];
const inPool = (site, caller) => POOL.includes(site) && site !== caller;
async function poolChildren(pred) {
  const lists = await Promise.all(SITE_NAMES.map((site) => get(site, "/api/runs")));
  return lists.flatMap((r) => r.json).filter((r) => r.parent && pred(r));
}

const TRIP = (city) => ({ city, ideas: [1, 2, 3].map((d) => `day ${d} in ${city}`), weather: `sunny in ${city}` });
const sameJson = (a, b) => JSON.stringify(a) === JSON.stringify(b);
const local_ = (dump, name) => dump.threads[0].frames.flatMap((f) => f.locals).find((l) => l.name === name);
const logs = (stream, id) => stream.all((e) => e.type === "log" && e.run === id && e.stream === "stdout").map((e) => e.text);
const fmt = (n) => (typeof n === "number" ? Number(n.toFixed(3)) : n);

function checkSnapshotFiles(name, snap) {
  let dump = null;
  try { dump = JSON.parse(fs.readFileSync(snap.state_path, "utf8")); } catch { /* checked below */ }
  const size = fs.existsSync(snap.snapshot_path) ? fs.statSync(snap.snapshot_path).size : -1;
  check(`${name}: snap-${snap.n}.bamlsnap and snap-${snap.n}.json exist, sizes match the stats`,
    size > 0 && size === snap.bytes && size === snap.stats.file_bytes && dump !== null
    && snap.snapshot_path.endsWith(`/snap-${snap.n}.bamlsnap`) && snap.state_path.endsWith(`/snap-${snap.n}.json`),
    `size ${size}, bytes ${snap.bytes}, file_bytes ${snap.stats?.file_bytes}`);
  const magic = size > 0 ? fs.readFileSync(snap.snapshot_path).subarray(0, 8).toString("latin1") : "";
  check(`${name}: the snapshot is a binary file, not JSON`, size > 0 && !magic.startsWith("{"), JSON.stringify(magic));
  return dump;
}

function pauseStatsComplete(stats) {
  return ["pause_latency_ms", "walk_ms", "encode_ms", "compress_ms", "write_ms", "objects", "raw_bytes", "compressed_bytes", "program_bytes", "blocked_attempts"]
    .every((k) => typeof stats[k] === "number");
}

function recordPause(scene, stats) {
  measure(scene, "PauseStats", Object.fromEntries(Object.entries(stats).map(([k, v]) => [k, fmt(v)])));
}

// Resume and measure the wall time from the resume request to the first event
// of the new segment.
async function resumeAndMeasure(scene, site, stream, id, body = {}, onSite = site, onStream = stream) {
  const before = (await get(site, `/api/runs/${id}`)).json;
  const t0 = Date.now();
  const res = await post(site, `/api/runs/${id}/resume`, body);
  const tResponse = Date.now();
  const segment = before.segment + 1;
  const first = await onStream.waitEvent(`first event of segment ${segment}`, (e) => e.run === id && e.segment === segment && typeof e.pid === "number");
  const resumed = await onStream.waitEvent("resumed", (e) => e.type === "resumed" && e.run === id && e.segment === segment);
  const firstAfter = await onStream.waitEvent("first program event after resumed",
    (e) => e.run === id && e.segment === segment && ["position", "log", "remote_result_received", "completed"].includes(e.type));
  measure(scene, "ResumeStats", Object.fromEntries(Object.entries(resumed.stats).map(([k, v]) => [k, fmt(v)])));
  measure(scene, "resume request -> first event of the new segment", {
    first_event: first.type,
    http_response_ms: tResponse - t0,
    by_worker_ts_ms: first.ts - t0,
    by_sse_arrival_ms: first.rx - t0,
    resumed_event_ms: resumed.ts - t0,
    first_program_event: firstAfter.type,
    first_program_event_ms: firstAfter.ts - t0,
  });
  return { res, first, resumed, segment, onSite };
}

// ------------------------------------------------------------------- scenes

async function sceneA(local, cloud) {
  section("a. durable_plan_trip completes, the remote child runs on cloud");
  const t0 = Date.now();
  const started = await post("local", "/api/runs", { function: "durable_plan_trip", args: { city: "Lisbon" } });
  const id = started.json.id;
  check("POST /api/runs returns a starting durable run", started.status === 200 && started.json.status === "starting" && started.json.durable === true, started.text);
  const hello = await local.waitEvent("hello", (e) => e.type === "hello" && e.run === id);
  check("hello: mode start, real function name, durable", hello.mode === "start" && hello.function === "durable_plan_trip" && hello.durable === true && hello.v === 1 && hello.pid > 0);
  measure("a", "start request -> hello / first program event", {
    hello_ms: hello.ts - t0,
    first_position_ms: (await local.waitEvent("position", (e) => e.type === "position" && e.run === id)).ts - t0,
  });
  const pos = local.find((e) => e.type === "position" && e.run === id);
  check("position carries a project-relative file and a real line", pos.file === "baml_src/trip.baml" && pos.function === "durable_plan_trip" && [24, 25].includes(pos.line), JSON.stringify(pos));
  const src = await get("local", `/api/source?file=${encodeURIComponent(pos.file)}`);
  check("GET /api/source accepts position.file, and the line is the println or the sleep",
    src.status === 200 && /println|sleep/.test(src.json.text.split("\n")[pos.line - 1]), `status ${src.status}`);
  const dispatched = await local.waitEvent("remote_dispatched", (e) => e.type === "remote_dispatched" && e.run === id);
  const cs = dispatched.child_site;
  check("remote call dispatched to a site of the pool with call id <run>-c1", inPool(cs, "local") && dispatched.call_id === `${id}-c1` && dispatched.function === "remote_fetch_weather");
  const call = local.find((e) => e.type === "remote_call" && e.run === id);
  check("remote_call event carries the args by parameter name", call && sameJson(call.args, { city: "Lisbon" }), JSON.stringify(call));
  const child = await waitStatus(cs, dispatched.child_run, "completed");
  check("child ran on the pool site, non-durable, with parent set", child.site === cs && child.result === "sunny in Lisbon" && child.durable === false && child.parent.run === id);
  check("the child's println reached the stream of its site", !!STREAMS[cs].find((e) => e.type === "log" && e.run === child.id && e.text === "[remote] looking up weather for Lisbon"));
  const done = await waitStatus("local", id, "completed");
  check("the run completed with the correct TripPlan", sameJson(done.result, TRIP("Lisbon")), JSON.stringify(done.result));
  check("one process for the whole run", done.segment === 1 && local.all((e) => e.type === "hello" && e.run === id).length === 1);
  check("println lines in order, once each", sameJson(logs(local, id), ["planning day 1", "planning day 2", "planning day 3"]), JSON.stringify(logs(local, id)));
  check("remote_result_received and completed were forwarded",
    !!local.find((e) => e.type === "remote_result_received" && e.run === id && e.call_id === `${id}-c1`)
    && !!local.find((e) => e.type === "completed" && e.run === id && sameJson(e.value, TRIP("Lisbon"))));
  const exit = await local.waitEvent("worker_exit", (e) => e.type === "worker_exit" && e.run === id);
  check("worker_exit: exit code 0", exit.exit_code === 0 && exit.status === "completed");
  const auto = done.snapshots.filter((s) => s.automatic);
  check("automatic snapshots were recorded (durable run, default interval 2000 ms)", auto.length >= 1 && auto.every((s) => s.stats.pause_latency_ms === null && s.segment === 1),
    JSON.stringify(done.snapshots.map((s) => [s.n, s.automatic])));
  const events = (await get("local", `/api/runs/${id}/events`)).json;
  const types = new Set(events.map((e) => e.type));
  check("events.jsonl has the worker and server events and no run/init messages",
    ["hello", "thread_started", "position", "log", "snapshot", "remote_call", "remote_dispatched", "remote_returned", "remote_result_received", "thread_ended", "completed", "worker_exit"].every((t) => types.has(t))
    && !types.has("run") && !types.has("init"), [...types].join(","));
  check("no worker_stderr noise (BAML_CLI_ALLOW_DIRECT reached the worker)", !events.some((e) => e.type === "log" && e.stream === "worker_stderr"),
    JSON.stringify(events.filter((e) => e.stream === "worker_stderr").map((e) => e.text)));
}

async function sceneB(local, cloud) {
  section("b. THE CENTRAL SCENE: pause during the remote wait, the child finishes without a parent process, resume");
  const id = (await post("local", "/api/runs", { function: "durable_plan_trip", args: { city: "Lisbon" } })).json.id;
  const dispatched = await local.waitEvent("remote_dispatched", (e) => e.type === "remote_dispatched" && e.run === id);
  const childId = dispatched.child_run;
  const running = (await get("local", `/api/runs/${id}`)).json;
  const pid1 = running.pid;
  check("the parent waits on the child", running.status === "running" && pidAlive(pid1) && running.waiting_on.length === 1 && running.waiting_on[0].child_run === childId);

  const tPause = Date.now();
  const pausing = await post("local", `/api/runs/${id}/pause`);
  check("POST pause returns pausing", pausing.status === 200 && pausing.json.status === "pausing", pausing.text);
  const paused = await waitStatus("local", id, "paused");
  const tPaused = Date.now();
  const childDuring = (await get(dispatched.child_site, `/api/runs/${childId}`)).json;
  check("the child is still running on its site while the parent is paused", childDuring.status === "running" || childDuring.status === "starting", childDuring.status);
  check("the parent has no pid, and its worker process is gone", paused.pid === null && !pidAlive(pid1));
  const exit1 = local.find((e) => e.type === "worker_exit" && e.run === id && e.segment === 1);
  check("worker_exit of segment 1: exit code 75, status paused", exit1 && exit1.exit_code === 75 && exit1.status === "paused", JSON.stringify(exit1));
  const pausedEv = local.find((e) => e.type === "paused" && e.run === id);
  const snap = paused.snapshots.at(-1);
  check("paused event and snapshot record agree; PauseStats complete", pausedEv && snap.automatic === false && snap.snapshot_path === pausedEv.snapshot_path
    && pauseStatsComplete(pausedEv.stats), JSON.stringify(pausedEv?.stats));
  recordPause("b", pausedEv.stats);
  measure("b", "pause request -> status paused (process exited, pipes drained)", { ms: tPaused - tPause, paused_event_ms: pausedEv.ts - tPause });
  const dump = checkSnapshotFiles("b", snap);
  const state = await get("local", `/api/runs/${id}/snapshots/${snap.n}/state`);
  check("GET snapshot state serves the file on disk", state.status === 200 && sameJson(state.json, dump));
  const d = state.json;
  const top = d.threads[0].frames[0];
  check("StateDump: one thread parked in remote_call, top frame durable_plan_trip at line 29 of baml_src/trip.baml",
    d.run === id && d.segment === 1 && d.threads.length === 1 && d.threads[0].parked.kind === "remote_call"
    && top.function === "durable_plan_trip" && top.file === "baml_src/trip.baml" && top.line === 29, JSON.stringify({ parked: d.threads[0].parked, top: { ...top, locals: undefined } }));
  let detail = null;
  try { detail = JSON.parse(d.threads[0].parked.detail); } catch { /* checked below */ }
  check("StateDump: parked.detail names the call id and the callee", detail && detail.call_id === `${id}-c1` && /remote_fetch_weather/.test(detail.function), d.threads[0].parked.detail);
  const ideas = local_(d, "ideas"), day = local_(d, "day"), city = local_(d, "city");
  check("StateDump shows the real locals: city \"Lisbon\", day 4, ideas with three strings",
    city?.value.kind === "string" && city.value.preview.includes("Lisbon") && day?.value.kind === "int" && day.value.preview === "4"
    && ideas?.value.kind === "array" && ideas.value.children.length === 3 && ideas.value.children[2].value.preview.includes("day 3 in Lisbon"),
    JSON.stringify({ city: city?.value, day: day?.value, ideas: ideas?.value }));
  check("StateDump heap totals", d.heap.objects > 0 && d.heap.bytes > 0 && Object.keys(d.heap.by_kind).length > 0, JSON.stringify(d.heap));

  const childDone = await waitStatus(dispatched.child_site, childId, "completed");
  check("the child completed on its site while no parent process existed", childDone.result === "sunny in Lisbon" && !pidAlive(pid1)
    && (await get("local", `/api/runs/${id}`)).json.pid === null);
  await local.waitEvent("remote_returned", (e) => e.type === "remote_returned" && e.run === id && e.ok === true);
  const stored = (await get("local", `/api/runs/${id}`)).json;
  check("the result is stored on the paused parent (unacknowledged)", stored.status === "paused" && stored.waiting_on.length === 0
    && stored.remote_results.length === 1 && stored.remote_results[0].value === "sunny in Lisbon" && stored.remote_results[0].acked === false, JSON.stringify(stored.remote_results));

  const { res, first, resumed } = await resumeAndMeasure("b", "local", local, id);
  check("POST resume starts segment 2", res.status === 200 && res.json.segment === 2 && res.json.status === "starting", res.text);
  const hello2 = await local.waitEvent("hello 2", (e) => e.type === "hello" && e.run === id && e.segment === 2);
  check("a new pid appears: hello of segment 2 (mode resume, real function name)", hello2.mode === "resume" && hello2.pid !== pid1 && hello2.function === "durable_plan_trip"
    && hello2.durable === true && first.type === "hello", JSON.stringify(hello2));
  check("resumed event with ResumeStats", typeof resumed.stats.program_load_ms === "number" && typeof resumed.stats.decode_ms === "number" && typeof resumed.stats.first_exec_ms === "number", JSON.stringify(resumed.stats));
  const done = await waitStatus("local", id, "completed");
  check("the run completed with the correct TripPlan", sameJson(done.result, TRIP("Lisbon")), JSON.stringify(done.result));
  check("the stored result was taken by segment 2 and acknowledged",
    !!local.find((e) => e.type === "remote_result_received" && e.run === id && e.segment === 2 && e.call_id === `${id}-c1`) && done.remote_results[0].acked === true);
  check("the remote call was announced and dispatched once", local.all((e) => e.type === "remote_call" && e.run === id).length === 1
    && local.all((e) => e.type === "remote_dispatched" && e.run === id).length === 1
    && (await poolChildren((r) => r.parent.run === id)).length === 1);
  check("println lines across both segments: once each, none in segment 2", sameJson(logs(local, id), ["planning day 1", "planning day 2", "planning day 3"])
    && local.all((e) => e.type === "log" && e.run === id && e.segment === 2 && e.stream === "stdout").length === 0, JSON.stringify(logs(local, id)));
  const pos2 = local.find((e) => e.type === "position" && e.run === id && e.segment === 2);
  check("segment 2 reports the restored position (line 29, remote_call)", pos2 && pos2.line === 29 && pos2.reason === "remote_call", JSON.stringify(pos2));
  const exits = local.all((e) => e.type === "worker_exit" && e.run === id);
  check("two worker_exit events: 75 then 0", exits.length === 2 && exits[0].exit_code === 75 && exits[1].exit_code === 0);
}

async function sceneC(local) {
  section("c. pause inside the loop (before the remote call), resume, completes");
  const id = (await post("local", "/api/runs", { function: "durable_plan_trip", args: { city: "Porto" } })).json.id;
  await local.waitEvent("planning day 2", (e) => e.type === "log" && e.run === id && e.text === "planning day 2");
  await sleep(300);
  const pid1 = (await get("local", `/api/runs/${id}`)).json.pid;
  const tPause = Date.now();
  await post("local", `/api/runs/${id}/pause`);
  const paused = await waitStatus("local", id, "paused");
  measure("c", "pause request -> status paused (process exited, pipes drained)", { ms: Date.now() - tPause });
  check("paused, process gone", paused.pid === null && !pidAlive(pid1));
  const pausedEv = local.find((e) => e.type === "paused" && e.run === id);
  check("PauseStats complete", pausedEv && pauseStatsComplete(pausedEv.stats), JSON.stringify(pausedEv?.stats));
  recordPause("c", pausedEv.stats);
  measure("c", "paused event after the pause request", { ms: pausedEv.ts - tPause });
  check("the pause did not wait for the 1500 ms sleep to end", pausedEv.ts - tPause < 1000, `${pausedEv.ts - tPause} ms`);
  const snap = paused.snapshots.at(-1);
  const d = checkSnapshotFiles("c", snap);
  const top = d.threads[0].frames[0];
  check("StateDump: parked in sleep at line 25, day 2, ideas has one entry",
    d.threads[0].parked.kind === "sleep" && top.line === 25 && top.file === "baml_src/trip.baml"
    && local_(d, "day")?.value.preview === "2" && local_(d, "ideas")?.value.children.length === 1
    && local_(d, "ideas").value.children[0].value.preview.includes("day 1 in Porto"), JSON.stringify({ parked: d.threads[0].parked, line: top.line, day: local_(d, "day")?.value }));
  let detail = null;
  try { detail = JSON.parse(d.threads[0].parked.detail); } catch { /* checked below */ }
  check("the sleep is stored with an absolute deadline", detail && typeof detail.deadline_unix_ms === "number" && detail.deadline_unix_ms > pausedEv.ts - 1500
    && detail.deadline_unix_ms <= pausedEv.ts + 1500, d.threads[0].parked.detail);

  const { first } = await resumeAndMeasure("c", "local", local, id);
  check("segment 2 has a new pid", first.pid !== pid1);
  const done = await waitStatus("local", id, "completed");
  check("the run completed with the correct TripPlan", sameJson(done.result, TRIP("Porto")), JSON.stringify(done.result));
  const lines = local.all((e) => e.type === "log" && e.run === id && e.stream === "stdout").map((e) => `${e.segment}:${e.text}`);
  check("no println line duplicated or lost across the pause", sameJson(lines, ["1:planning day 1", "1:planning day 2", "2:planning day 3"]), JSON.stringify(lines));
  check("the remote call happened in segment 2 with call id <run>-c1", local.find((e) => e.type === "remote_call" && e.run === id)?.segment === 2
    && local.find((e) => e.type === "remote_call" && e.run === id)?.call_id === `${id}-c1`);
}

async function sceneD(local, cloud, cloud2) {
  section("d. migration: pause on local, resume with {site: cloud}");
  const id = (await post("local", "/api/runs", { function: "durable_plan_trip", args: { city: "Faro" } })).json.id;
  await local.waitEvent("planning day 1", (e) => e.type === "log" && e.run === id && e.text === "planning day 1");
  await sleep(200);
  await post("local", `/api/runs/${id}/pause`);
  const paused = await waitStatus("local", id, "paused");
  const snap = paused.snapshots.at(-1);
  const state = (await get("local", `/api/runs/${id}/snapshots/${snap.n}/state`)).json;
  const { res } = await resumeAndMeasure("d", "local", local, id, { site: "cloud" }, "cloud", cloud);
  check("resume {site: cloud} marks the local record migrated", res.status === 200 && res.json.status === "migrated", res.text);
  check("migrated_out on local, migrated_in on cloud",
    !!(await local.waitEvent("migrated_out", (e) => e.type === "migrated_out" && e.run === id && e.to_site === "cloud"))
    && !!(await cloud.waitEvent("migrated_in", (e) => e.type === "migrated_in" && e.run === id && e.from_site === "local")));
  const hello2 = await cloud.waitEvent("hello on cloud", (e) => e.type === "hello" && e.run === id && e.segment === 2);
  check("segment 2 runs on cloud in resume mode", hello2.site === "cloud" && hello2.mode === "resume" && hello2.function === "durable_plan_trip");
  const onCloud = (await get("cloud", `/api/runs/${id}`)).json;
  const cloudState = await get("cloud", `/api/runs/${id}/snapshots/${snap.n}/state`);
  check("cloud has the run with origin local, the snapshot under the same n, and the same StateDump",
    onCloud.origin?.site === "local" && onCloud.snapshots[0].n === snap.n && fs.existsSync(onCloud.snapshots[0].snapshot_path)
    && onCloud.snapshots[0].snapshot_path !== snap.snapshot_path && cloudState.status === 200 && sameJson(cloudState.json, state));
  check("the snapshot bytes on cloud equal the bytes on local", Buffer.compare(fs.readFileSync(onCloud.snapshots[0].snapshot_path), fs.readFileSync(snap.snapshot_path)) === 0);
  const dispatched = await cloud.waitEvent("remote_dispatched from cloud", (e) => e.type === "remote_dispatched" && e.run === id);
  check("placement: the remote call of the run that migrated to cloud is placed on cloud2", dispatched.child_site === "cloud2" && dispatched.call_id === `${id}-c1`, JSON.stringify(dispatched));
  const child = await waitStatus("cloud2", dispatched.child_run, "completed");
  check("the child ran on cloud2 and names cloud as the site to call back", child.site === "cloud2" && child.result === "sunny in Faro" && child.parent.site === "cloud"
    && !!cloud2.find((e) => e.type === "log" && e.run === child.id && e.text === "[remote] looking up weather for Faro"));
  const returned = await cloud.waitEvent("remote_returned on cloud", (e) => e.type === "remote_returned" && e.run === id);
  check("the result returns to cloud, not to local", returned.ok === true && returned.child_site === "cloud2" && !local.find((e) => e.type === "remote_returned" && e.run === id));
  const done = await waitStatus("cloud", id, "completed");
  check("the run completed on cloud with the correct TripPlan", sameJson(done.result, TRIP("Faro")), JSON.stringify(done.result));
  const lines = [...local.all((e) => e.type === "log" && e.run === id && e.stream === "stdout").map((e) => `local:${e.text}`),
    ...cloud.all((e) => e.type === "log" && e.run === id && e.stream === "stdout").map((e) => `cloud:${e.text}`)];
  check("println lines: day 1 on local, days 2 and 3 on cloud", sameJson(lines, ["local:planning day 1", "cloud:planning day 2", "cloud:planning day 3"]), JSON.stringify(lines));
  check("snapshots written on cloud continue the numbering", done.snapshots.every((s, i) => i === 0 || s.n === done.snapshots[i - 1].n + 1)
    && done.snapshots.every((s) => s.snapshot_path.endsWith(`/snap-${s.n}.bamlsnap`)), JSON.stringify(done.snapshots.map((s) => s.n)));
  const still = (await get("local", `/api/runs/${id}`)).json;
  check("the local record stays migrated and names cloud in migrated_to", still.status === "migrated" && still.pid === null && still.migrated_to === "cloud" && done.migrated_to === null,
    JSON.stringify({ local: still.migrated_to, cloud: done.migrated_to }));
}

async function sceneE(local) {
  section("e. fork a paused run: the source and the fork complete independently");
  const id = (await post("local", "/api/runs", { function: "durable_plan_trip", args: { city: "Evora" } })).json.id;
  await local.waitEvent("planning day 2", (e) => e.type === "log" && e.run === id && e.text === "planning day 2");
  await sleep(200);
  await post("local", `/api/runs/${id}/pause`);
  const paused = await waitStatus("local", id, "paused");
  const forked = await post("local", `/api/runs/${id}/fork`);
  const fid = forked.json.id;
  check("POST fork returns a new paused run with its own copy of the snapshot", forked.status === 200 && fid !== id && forked.json.status === "paused"
    && forked.json.forked_from.run === id && forked.json.forked_from.n === paused.snapshots.at(-1).n
    && fs.existsSync(forked.json.snapshots[0].snapshot_path) && forked.json.snapshots[0].snapshot_path.includes(fid), forked.text);
  await post("local", `/api/runs/${fid}/resume`);
  await post("local", `/api/runs/${id}/resume`);
  const [a, b] = await Promise.all([waitStatus("local", id, ["completed", "failed", "lost"]), waitStatus("local", fid, ["completed", "failed", "lost"])]);
  check("the source completed", a.status === "completed" && sameJson(a.result, TRIP("Evora")), `${a.status} ${a.error ?? ""} ${JSON.stringify(a.result)}`);
  check("the fork completed", b.status === "completed" && sameJson(b.result, TRIP("Evora")), `${b.status} ${b.error ?? ""} ${JSON.stringify(b.result)}`);
  const helloFork = local.find((e) => e.type === "hello" && e.run === fid);
  const helloSrc = local.find((e) => e.type === "hello" && e.run === id && e.segment === 2);
  check("they ran in different processes", helloFork && helloSrc && helloFork.pid !== helloSrc.pid);
  const forkLines = logs(local, fid), srcLines = logs(local, id);
  check("each of them printed day 3 itself", sameJson(forkLines, ["planning day 3"]) && sameJson(srcLines, ["planning day 1", "planning day 2", "planning day 3"]),
    JSON.stringify({ forkLines, srcLines }));
  const callSrc = local.find((e) => e.type === "remote_call" && e.run === id);
  const callFork = local.find((e) => e.type === "remote_call" && e.run === fid);
  const children = await poolChildren((r) => r.parent.run === id || r.parent.run === fid);
  check("each of them made its own remote call (two child runs on the pool sites)", callSrc && callFork && children.length === 2
    && children.some((r) => r.parent.run === id) && children.some((r) => r.parent.run === fid),
    JSON.stringify({ src: callSrc?.call_id, fork: callFork?.call_id, children: children.map((r) => r.parent) }));
}

async function sceneF(local) {
  section("f. kill -9: plan_trip is lost, durable_plan_trip recovers from an automatic snapshot");
  const plain = (await post("local", "/api/runs", { function: "plan_trip", args: { city: "Sintra" } })).json.id;
  const durable = (await post("local", "/api/runs", { function: "durable_plan_trip", args: { city: "Sintra" } })).json.id;
  const snapEv = await local.waitEvent("first automatic snapshot", (e) => e.type === "snapshot" && e.run === durable);
  check("automatic snapshots are implemented: `snapshot` event with automatic: true and pause_latency_ms null",
    snapEv.automatic === true && snapEv.stats.pause_latency_ms === null && typeof snapEv.stats.walk_ms === "number", JSON.stringify(snapEv.stats));
  await local.waitEvent("plain run day 2", (e) => e.type === "log" && e.run === plain && e.text === "planning day 2");
  check("a non-durable run takes no automatic snapshot", !local.find((e) => e.type === "snapshot" && e.run === plain));
  const pauseRefused = await post("local", `/api/runs/${plain}/pause`);
  check("pause of a non-durable run is 409", pauseRefused.status === 409);
  const dpid = (await get("local", `/api/runs/${durable}`)).json.pid;
  const ppid = (await get("local", `/api/runs/${plain}`)).json.pid;
  const k2 = await post("local", `/api/runs/${plain}/kill`);
  check("kill of plan_trip: lost", k2.status === 200 && k2.json.status === "lost" && k2.json.pid === null && !!k2.json.error, k2.text);
  const k1 = await post("local", `/api/runs/${durable}/kill`);
  check("kill of durable_plan_trip with an automatic snapshot: paused", k1.status === 200 && k1.json.status === "paused" && k1.json.pid === null, k1.text);
  check("both processes are gone", !pidAlive(dpid) && !pidAlive(ppid));
  const exitEv = local.find((e) => e.type === "worker_exit" && e.run === durable);
  check("worker_exit reports the signal", exitEv && exitEv.exit_code === -1 && exitEv.signal !== null && exitEv.status === "paused", JSON.stringify(exitEv));
  const snap = k1.json.snapshots.at(-1);
  const d = checkSnapshotFiles("f", snap);
  const dayAtSnap = Number(local_(d, "day")?.value.preview);
  const linesBefore = logs(local, durable);
  await resumeAndMeasure("f", "local", local, durable);
  const done = await waitStatus("local", durable, "completed");
  check("the killed durable run resumes from its latest automatic snapshot and completes with the correct TripPlan",
    done.segment === 2 && sameJson(done.result, TRIP("Sintra")), JSON.stringify(done.result));
  const lines2 = local.all((e) => e.type === "log" && e.run === durable && e.segment === 2 && e.stream === "stdout").map((e) => e.text);
  const expected2 = [1, 2, 3].filter((n) => n > dayAtSnap).map((n) => `planning day ${n}`);
  check(`segment 2 prints exactly the days after the snapshot (snapshot at day ${dayAtSnap})`, sameJson(lines2, expected2), JSON.stringify({ linesBefore, lines2 }));
  measure("f", "lines repeated after the recovery (at-least-once between snapshot and kill)", { repeated: lines2.filter((l) => linesBefore.includes(l)) });

  const fresh = (await post("local", "/api/runs", { function: "durable_plan_trip", args: { city: "Tomar" } })).json.id;
  await local.waitEvent("hello", (e) => e.type === "hello" && e.run === fresh);
  const k4 = await post("local", `/api/runs/${fresh}/kill`);
  check("kill of a durable run that has no snapshot yet: lost", k4.json.status === "lost", k4.text);
}

async function sceneG(local, cloud) {
  if (!PHASE3) return sceneGLegacy(local);
  section("g. durable_plan_trip_parallel: a pause with two threads and a pending future, resume, the run completes");
  const id = (await post("local", "/api/runs", { function: "durable_plan_trip_parallel", args: { city: "Aveiro" } })).json.id;
  const started = await local.waitEvent("thread_started for the spawn", (e) => e.type === "thread_started" && e.run === id && e.parent_thread !== null);
  const call = await local.waitEvent("remote_call", (e) => e.type === "remote_call" && e.run === id);
  check("remote_call comes from the spawned thread", call.thread === started.thread && sameJson(call.args, { city: "Aveiro" }), JSON.stringify({ call: call.thread, started: started.thread }));
  await local.waitEvent("remote_dispatched", (e) => e.type === "remote_dispatched" && e.run === id);
  const pausing = await post("local", `/api/runs/${id}/pause`);
  check("POST pause returns pausing", pausing.status === 200 && pausing.json.status === "pausing", pausing.text);
  const paused = await waitStatus("local", id, ["paused", "completed", "failed", "lost"]);
  check("the run pauses although it holds a pending future and a second thread", paused.status === "paused" && paused.snapshots.length >= 1 && paused.blocked === null,
    JSON.stringify({ status: paused.status, blocked: paused.blocked, snapshots: paused.snapshots.length }));
  check("no `blocked` event answered the pause request", local.all((e) => e.type === "blocked" && e.run === id).length === 0);
  const snap = paused.snapshots[paused.snapshots.length - 1];
  const dump = (await get("local", `/api/runs/${id}/snapshots/${snap.n}/state`)).json;
  const spawned = dump.threads?.find((t) => t.thread === started.thread);
  check("the state dump lists both threads, and the spawned one is parked in its remote call with the root as parent", dump.threads?.length === 2 && spawned
    && spawned.parent_thread === dump.threads.find((t) => t !== spawned).thread && spawned.parked.kind === "remote_call", JSON.stringify(dump.threads?.map((t) => ({ thread: t.thread, parent: t.parent_thread, parked: t.parked }))));
  const futureLocal = dump.threads?.flatMap((t) => t.frames).flatMap((f) => f.locals).find((l) => l.name === "weather_future");
  check("the local weather_future is a pending future in the dump", futureLocal?.value.kind === "future" && futureLocal.value.preview.includes("pending"), JSON.stringify(futureLocal));
  const linesBefore = logs(local, id).length;
  const res = await post("local", `/api/runs/${id}/resume`);
  check("POST resume starts segment 2", res.status === 200 && res.json.segment === 2, res.text);
  const done = await waitStatus("local", id, ["completed", "failed", "lost"]);
  check("the run completes with the correct TripPlan", done.status === "completed" && sameJson(done.result, TRIP("Aveiro")), `${done.status} ${JSON.stringify(done.result)} ${done.error ?? ""}`);
  check("println lines complete and in order across the pause", sameJson(logs(local, id), ["starting the weather lookup in the background", "planning day 1", "planning day 2", "planning day 3", "waiting for the weather"]), JSON.stringify(logs(local, id)));
  check("the remote call was dispatched once", local.all((e) => e.type === "remote_dispatched" && e.run === id).length === 1);
  const restarted = local.all((e) => e.type === "thread_started" && e.run === id && e.segment === 2);
  check("segment 2 reports both threads with their original ids and parents", restarted.length === 2 && restarted.some((e) => e.thread === started.thread && e.parent_thread === started.parent_thread),
    JSON.stringify(restarted.map((e) => ({ thread: e.thread, parent: e.parent_thread }))));
  measure("g", "pause with two threads", { lines_before_pause: linesBefore, stats: snap.stats });
}

async function sceneGLegacy(local) {
  section("g. durable_plan_trip_parallel: a pause is answered with `blocked`, the run still completes");
  const id = (await post("local", "/api/runs", { function: "durable_plan_trip_parallel", args: { city: "Aveiro" } })).json.id;
  const started = await local.waitEvent("thread_started for the spawn", (e) => e.type === "thread_started" && e.run === id && e.parent_thread !== null);
  const call = await local.waitEvent("remote_call", (e) => e.type === "remote_call" && e.run === id);
  check("remote_call comes from the spawned thread", call.thread === started.thread && sameJson(call.args, { city: "Aveiro" }), JSON.stringify({ call: call.thread, started: started.thread }));
  await local.waitEvent("remote_dispatched", (e) => e.type === "remote_dispatched" && e.run === id);
  const pausing = await post("local", `/api/runs/${id}/pause`);
  check("POST pause returns pausing", pausing.status === 200 && pausing.json.status === "pausing", pausing.text);
  const blocked = await local.waitEvent("blocked", (e) => e.type === "blocked" && e.run === id, 10000).catch(() => null);
  check("the worker answers with a `blocked` event that has a reason and a root-to-value path", blocked && typeof blocked.reason === "string" && blocked.reason.length > 0
    && Array.isArray(blocked.path) && blocked.path.length > 0, JSON.stringify(blocked));
  if (blocked) {
    check("the path names the frame and the local that holds the future", blocked.path.some((p) => p === "frame durable_plan_trip_parallel") && blocked.path.some((p) => p.includes("weather_future"))
      && blocked.path.every((p) => !p.includes("0x") && !p.includes("user.")), JSON.stringify(blocked.path));
    measure("g", "blocked event", { reason: blocked.reason, path: blocked.path });
  }
  const during = await waitFor("blocked on the run record", async () => {
    const r = (await get("local", `/api/runs/${id}`)).json;
    return r.blocked || r.status !== "pausing" ? r : null;
  }, 10000).catch(() => null);
  check("the run record exposes the blocked reason while the status is pausing", during && during.status === "pausing" && during.blocked
    && during.blocked.reason === blocked?.reason && during.blocked.attempts >= 1 && during.pid !== null, JSON.stringify(during && { status: during.status, blocked: during.blocked }));
  const runMsg = await local.waitEvent("run message with blocked", (e) => e.type === "run" && e.run.id === id && e.run.status === "pausing" && e.run.blocked, 5000).catch(() => null);
  check("a `run` SSE message carried the blocked reason", !!runMsg);
  const done = await waitStatus("local", id, ["completed", "failed", "lost", "paused"]);
  check("the run still completes with the correct TripPlan", done.status === "completed" && sameJson(done.result, TRIP("Aveiro")), `${done.status} ${JSON.stringify(done.result)} ${done.error ?? ""}`);
  check("blocked is cleared, no snapshot was written, one segment", done.blocked === null && done.snapshots.length === 0 && done.segment === 1,
    JSON.stringify({ blocked: done.blocked, snapshots: done.snapshots.length }));
  check("println lines complete and in order", sameJson(logs(local, id), ["starting the weather lookup in the background", "planning day 1", "planning day 2", "planning day 3", "waiting for the weather"]), JSON.stringify(logs(local, id)));
  check("thread_ended for the spawned thread", !!local.find((e) => e.type === "thread_ended" && e.run === id && e.thread === started.thread));
  measure("g", "blocked events for one pause request", { count: local.all((e) => e.type === "blocked" && e.run === id).length });
}

async function sceneH(local, cloud) {
  section("h. kill during the remote wait, the child finishes, resume with the stored result");
  const id = (await post("local", "/api/runs", { function: "durable_plan_trip", args: { city: "Lagos" } })).json.id;
  const dispatched = await local.waitEvent("remote_dispatched", (e) => e.type === "remote_dispatched" && e.run === id);
  await sleep(300);
  const killed = await post("local", `/api/runs/${id}/kill`);
  check("killed while waiting: paused, every snapshot is automatic", killed.json.status === "paused" && killed.json.snapshots.length > 0 && killed.json.snapshots.every((s) => s.automatic), killed.text.slice(0, 200));
  const d = JSON.parse(fs.readFileSync(killed.json.snapshots.at(-1).state_path, "utf8"));
  const parkedKind = d.threads[0].parked.kind;
  const dayAtSnap = Number(local_(d, "day")?.value.preview);
  await waitStatus(dispatched.child_site, dispatched.child_run, "completed");
  await waitFor("stored result", async () => (await get("local", `/api/runs/${id}`)).json.remote_results.length === 1);
  await post("local", `/api/runs/${id}/resume`);
  const done = await waitStatus("local", id, ["completed", "failed", "lost"]);
  check("the resumed run completes with the stored result", done.status === "completed" && sameJson(done.result, TRIP("Lagos")) && done.remote_results[0].acked === true,
    `${done.status} ${done.error ?? ""} ${JSON.stringify(done.result)}`);
  const calls = local.all((e) => e.type === "remote_call" && e.run === id);
  check("the call id is stable, and the call was dispatched once", calls.every((c) => c.call_id === `${id}-c1`)
    && local.all((e) => e.type === "remote_dispatched" && e.run === id).length === 1
    && (await poolChildren((r) => r.parent.run === id)).length === 1, JSON.stringify(calls.map((c) => [c.segment, c.call_id])));
  check("remote_result_received once, in segment 2", local.all((e) => e.type === "remote_result_received" && e.run === id).length === 1
    && local.find((e) => e.type === "remote_result_received" && e.run === id).segment === 2);
  measure("h", "latest automatic snapshot at the kill", { parked: parkedKind, day: dayAtSnap, remote_call_events: calls.length,
    segment_2_lines: local.all((e) => e.type === "log" && e.run === id && e.segment === 2 && e.stream === "stdout").map((e) => e.text) });
}

async function sceneI(local) {
  section("i. cancel");
  const id = (await post("local", "/api/runs", { function: "durable_plan_trip", args: { city: "Beja" } })).json.id;
  await local.waitEvent("first log", (e) => e.type === "log" && e.run === id);
  const r = await post("local", `/api/runs/${id}/cancel`);
  check("POST cancel returns the run", r.status === 200 && r.json.id === id);
  const done = await waitStatus("local", id, "cancelled");
  const exitEv = await local.waitEvent("worker_exit", (e) => e.type === "worker_exit" && e.run === id);
  const cancelledEv = local.find((e) => e.type === "cancelled" && e.run === id);
  check("the run is cancelled: exit code 130, `cancelled` precedes worker_exit", done.pid === null && exitEv.exit_code === 130 && exitEv.status === "cancelled"
    && !!cancelledEv && local.events.indexOf(cancelledEv) < local.events.indexOf(exitEv), JSON.stringify(exitEv));
}

async function sceneJ(local, cloud) {
  section("j. two pauses: inside the loop, then during the remote wait; the result arrives on stdin of segment 3");
  const id = (await post("local", "/api/runs", { function: "durable_plan_trip", args: { city: "Braga" } })).json.id;
  await local.waitEvent("planning day 1", (e) => e.type === "log" && e.run === id && e.text === "planning day 1");
  await post("local", `/api/runs/${id}/pause`);
  const first = await waitStatus("local", id, "paused");
  await post("local", `/api/runs/${id}/resume`);
  const dispatched = await local.waitEvent("remote_dispatched", (e) => e.type === "remote_dispatched" && e.run === id);
  await post("local", `/api/runs/${id}/pause`);
  const second = await waitStatus("local", id, "paused");
  check("the second pause happened in segment 2, with a higher snapshot number", second.segment === 2 && second.snapshots.at(-1).segment === 2
    && second.snapshots.at(-1).n > first.snapshots.at(-1).n && second.snapshots.at(-1).automatic === false, JSON.stringify(second.snapshots.map((s) => [s.n, s.segment, s.automatic])));
  await post("local", `/api/runs/${id}/resume`);
  const resumed = await local.waitEvent("resumed 3", (e) => e.type === "resumed" && e.run === id && e.segment === 3);
  const childStatus = (await get(dispatched.child_site, `/api/runs/${dispatched.child_run}`)).json.status;
  check("segment 3 is up while the child still runs", childStatus === "running", childStatus);
  const received = await local.waitEvent("remote_result_received", (e) => e.type === "remote_result_received" && e.run === id);
  check("the result reached segment 3 on stdin, after `resumed`", received.segment === 3 && received.ts > resumed.ts + 500 && received.call_id === `${id}-c1`,
    `${received.ts - resumed.ts} ms after resumed`);
  const done = await waitStatus("local", id, "completed");
  check("the run completed in segment 3 with the correct TripPlan", done.segment === 3 && sameJson(done.result, TRIP("Braga")), JSON.stringify(done.result));
  const lines = local.all((e) => e.type === "log" && e.run === id && e.stream === "stdout").map((e) => `${e.segment}:${e.text}`);
  check("println lines once each across three segments", sameJson(lines, ["1:planning day 1", "2:planning day 2", "2:planning day 3"]), JSON.stringify(lines));
  check("one remote_call event, one dispatch", local.all((e) => e.type === "remote_call" && e.run === id).length === 1
    && local.all((e) => e.type === "remote_dispatched" && e.run === id).length === 1);
  const exits = local.all((e) => e.type === "worker_exit" && e.run === id).map((e) => e.exit_code);
  check("worker exits: 75, 75, 0", sameJson(exits, [75, 75, 0]), JSON.stringify(exits));
}

async function sceneK(local) {
  section("k. a pause request that arrives while the worker still loads the program");
  const id = (await post("local", "/api/runs", { function: "durable_plan_trip", args: { city: "Guarda" } })).json.id;
  await local.waitEvent("hello", (e) => e.type === "hello" && e.run === id);
  const tPause = Date.now();
  const pausing = await post("local", `/api/runs/${id}/pause`);
  check("POST pause right after hello returns pausing", pausing.status === 200 && pausing.json.status === "pausing", pausing.text);
  const paused = await waitStatus("local", id, "paused");
  const pausedEv = local.find((e) => e.type === "paused" && e.run === id);
  const started = local.find((e) => e.type === "thread_started" && e.run === id);
  check("the run pauses at its first clean point (within 200 ms of the thread start)", pausedEv && started && pausedEv.ts - started.ts < 200, `${pausedEv?.ts - started?.ts} ms`);
  measure("k", "pause requested during program load", { request_to_paused_event_ms: pausedEv.ts - tPause, pause_latency_ms: fmt(pausedEv.stats.pause_latency_ms),
    thread_start_to_paused_ms: pausedEv.ts - started.ts });
  const d = JSON.parse(fs.readFileSync(paused.snapshots.at(-1).state_path, "utf8"));
  check("StateDump: day 1, no ideas yet", local_(d, "day")?.value.preview === "1" && (local_(d, "ideas")?.value.children ?? []).length === 0, JSON.stringify(local_(d, "day")?.value));
  await post("local", `/api/runs/${id}/resume`);
  const done = await waitStatus("local", id, "completed");
  check("the run completed with the correct TripPlan", sameJson(done.result, TRIP("Guarda")), JSON.stringify(done.result));
  check("println lines once each", sameJson(logs(local, id), ["planning day 1", "planning day 2", "planning day 3"]), JSON.stringify(logs(local, id)));
}

async function sceneM(local, cloud, cloud2) {
  section("m. migration chain local -> cloud -> cloud2: the late remote result follows migrated_to to a run without a process");
  const id = (await post("local", "/api/runs", { function: "durable_plan_trip", args: { city: "Chaves" } })).json.id;
  const dispatched = await local.waitEvent("remote_dispatched", (e) => e.type === "remote_dispatched" && e.run === id);
  const t0 = Date.now();
  check("the child of the local parent runs on a site of the pool", inPool(dispatched.child_site, "local"), dispatched.child_site);
  await post("local", `/api/runs/${id}/pause`);
  await waitStatus("local", id, "paused");
  const hop1 = await post("local", `/api/runs/${id}/resume`, { site: "cloud" });
  check("first hop: local -> cloud", hop1.status === 200 && hop1.json.status === "migrated" && hop1.json.migrated_to === "cloud", hop1.text);
  // The pause request waits on stdin while the worker loads the program.
  const pause2 = await post("cloud", `/api/runs/${id}/pause`);
  check("the run accepts a pause on cloud right after the import", pause2.status === 200, pause2.text);
  const onCloud = await waitStatus("cloud", id, "paused");
  // An import must not replace a record that is not `migrated` (contract 7.4).
  const ownSnap = onCloud.snapshots.at(-1);
  const clash = await post("cloud", "/api/runs/import", {
    run: { ...onCloud, site: "local", segment: 99 },
    snapshot_base64: fs.readFileSync(ownSnap.snapshot_path).toString("base64"),
    state: JSON.parse(fs.readFileSync(ownSnap.state_path, "utf8")),
  });
  const afterClash = (await get("cloud", `/api/runs/${id}`)).json;
  check("import of a run that is paused here is 409, and the run is unchanged", clash.status === 409 && afterClash.status === "paused"
    && afterClash.segment === onCloud.segment && afterClash.updated_ts === onCloud.updated_ts && afterClash.pid === null, `${clash.status} ${clash.text}`);
  const hop2 = await post("cloud", `/api/runs/${id}/resume`, { site: "cloud2" });
  check("second hop: cloud -> cloud2", hop2.status === 200 && hop2.json.status === "migrated" && hop2.json.migrated_to === "cloud2" && onCloud.segment === 2, hop2.text);
  await post("cloud2", `/api/runs/${id}/pause`);
  const onCloud2 = await waitStatus("cloud2", id, "paused");
  const childNow = (await get(dispatched.child_site, `/api/runs/${dispatched.child_run}`)).json;
  measure("m", "remote_dispatched -> paused on cloud2 after two hops (the child needs about 4.3 s)", { ms: Date.now() - t0, child_status: childNow.status });
  check("the run is paused on cloud2 (segment 3, no process) while the child still runs on cloud", onCloud2.pid === null && onCloud2.segment === 3
    && onCloud2.origin.site === "cloud" && onCloud2.remote_results.length === 0 && childNow.status === "running",
    JSON.stringify({ segment: onCloud2.segment, results: onCloud2.remote_results.length, child: childNow.status }));
  const snapBytes = ["cloud", "cloud2"].map((site, i) => fs.readFileSync((i === 0 ? onCloud : onCloud2).snapshots.at(-1).snapshot_path));
  check("each site wrote its own snapshot of the run", snapBytes.every((b) => b.length > 0) && onCloud2.snapshots.at(-1).n > onCloud.snapshots.at(-1).n,
    JSON.stringify([onCloud.snapshots.map((s) => s.n), onCloud2.snapshots.map((s) => s.n)]));

  await waitStatus(dispatched.child_site, dispatched.child_run, "completed");
  const stored = await waitFor("stored result on cloud2", async () => {
    const r = (await get("cloud2", `/api/runs/${id}`)).json;
    return r.remote_results.length === 1 ? r : null;
  });
  check("the result was forwarded local -> cloud -> cloud2 and is stored on the paused run", stored.status === "paused" && stored.pid === null
    && stored.remote_results[0].value === "sunny in Chaves" && stored.remote_results[0].acked === false, JSON.stringify(stored.remote_results));
  const returned = await cloud2.waitEvent("remote_returned on cloud2", (e) => e.type === "remote_returned" && e.run === id);
  check("remote_returned is emitted by cloud2 only", returned.ok === true && returned.child_run === dispatched.child_run
    && !local.find((e) => e.type === "remote_returned" && e.run === id) && !cloud.find((e) => e.type === "remote_returned" && e.run === id));
  await waitFor("result_delivered on the child", async () => (await get(dispatched.child_site, `/api/runs/${dispatched.child_run}`)).json.result_delivered);
  const left = [(await get("local", `/api/runs/${id}`)).json, (await get("cloud", `/api/runs/${id}`)).json];
  check("the records that stayed behind form the chain local -> cloud -> cloud2", left[0].status === "migrated" && left[0].migrated_to === "cloud"
    && left[1].status === "migrated" && left[1].migrated_to === "cloud2", JSON.stringify(left.map((r) => [r.site, r.status, r.migrated_to])));

  await resumeAndMeasure("m", "cloud2", cloud2, id);
  const done = await waitStatus("cloud2", id, ["completed", "failed", "lost"]);
  check("the run completes on cloud2 with the correct TripPlan", done.status === "completed" && sameJson(done.result, TRIP("Chaves")) && done.segment === 4
    && done.remote_results[0].acked === true, `${done.status} ${done.error ?? ""} ${JSON.stringify(done.result)}`);
  const all = [...local.events, ...cloud.events, ...cloud2.events];
  const hellos = all.filter((e) => e.type === "hello" && e.run === id).map((e) => `${e.segment}:${e.site}`).sort();
  check("segments ran on local, cloud, cloud2, cloud2", sameJson(hellos, ["1:local", "2:cloud", "3:cloud2", "4:cloud2"]), JSON.stringify(hellos));
  check("one remote_call event, one dispatch, one child", all.filter((e) => e.type === "remote_call" && e.run === id).length === 1
    && all.filter((e) => e.type === "remote_dispatched" && e.run === id).length === 1
    && (await poolChildren((r) => r.parent.run === id)).length === 1);
  // Forwarded results carry `via`. A result that already passed a site of the
  // chain is refused with 502 and is not sent around again.
  const looped = await Promise.all([["local"], ["cloud"], ["cloud2"], ["a", "b", "c"]].map((via) =>
    post("local", `/api/runs/${id}/remote_result`, { call_id: `${id}-c9${via.length}${via[0]}`, value: "loop", via })));
  const end = (await get("cloud2", `/api/runs/${id}`)).json;
  check("a forwarded result whose via names a site of the chain, or as many sites as the registry, is answered 502 and reaches no record",
    looped.every((r) => r.status === 502) && !end.remote_results.some((r) => r.value === "loop"), looped.map((r) => r.status).join(" "));
  const straight = await post("local", `/api/runs/${id}/remote_result`, { call_id: `${id}-c99`, value: "late" });
  const end2 = (await get("cloud2", `/api/runs/${id}`)).json;
  check("without via the result still follows local -> cloud -> cloud2", straight.status === 200 && straight.json.forwarded_to === "cloud"
    && end2.remote_results.some((r) => r.call_id === `${id}-c99`), straight.text);
  const lines = all.filter((e) => e.type === "log" && e.run === id && e.stream === "stdout").map((e) => `${e.site}:${e.text}`);
  check("println lines once each, all on local", sameJson(lines, ["local:planning day 1", "local:planning day 2", "local:planning day 3"]), JSON.stringify(lines));
}

async function sceneN(local, cloud, cloud2) {
  section("n. a run on cloud2 calls into cloud");
  const started = await post("cloud2", "/api/runs", { function: "durable_plan_trip", args: { city: "Nazare" } });
  const id = started.json.id;
  check("POST /api/runs on cloud2 starts a run there", started.status === 200 && started.json.site === "cloud2", started.text);
  const dispatched = await cloud2.waitEvent("remote_dispatched", (e) => e.type === "remote_dispatched" && e.run === id);
  check("placement: remote_dispatched on cloud2 names cloud", dispatched.child_site === "cloud" && dispatched.site === "cloud2", JSON.stringify(dispatched));
  const child = await waitStatus("cloud", dispatched.child_run, "completed");
  check("the child ran on cloud with parent.site cloud2", child.site === "cloud" && child.parent.site === "cloud2" && child.parent.run === id);
  const done = await waitStatus("cloud2", id, "completed");
  check("the parent on cloud2 completes with the correct TripPlan", sameJson(done.result, TRIP("Nazare")), JSON.stringify(done.result));
  check("nothing of this run touched local", !local.find((e) => e.run === id || e.run === child.id));
}

async function sceneO(local) {
  section("o. unknown and malformed site names are refused with 400");
  const id = (await post("local", "/api/runs", { function: "durable_plan_trip", args: { city: "Tomar" } })).json.id;
  await local.waitEvent("planning day 1", (e) => e.type === "log" && e.run === id && e.text === "planning day 1");
  await post("local", `/api/runs/${id}/pause`);
  await waitStatus("local", id, "paused");
  const bad = await post("local", `/api/runs/${id}/resume`, { site: "mars" });
  const after = (await get("local", `/api/runs/${id}`)).json;
  check("resume {site: mars} is 400 {error}, and the run stays paused", bad.status === 400 && typeof bad.json.error === "string" && after.status === "paused" && after.migrated_to === null, bad.text);
  const notAName = await post("local", `/api/runs/${id}/resume`, { site: 5 });
  check("resume {site: 5} is 400, and the run is not resumed here", notAName.status === 400 && (await get("local", `/api/runs/${id}`)).json.status === "paused", notAName.text);
  const runsBefore = (await get("cloud", "/api/runs")).json.length;
  const badParent = await post("cloud", "/api/remote/runs", { function: "remote_fetch_weather", args: { city: "X" }, parent: { site: "mars", run: "r-x", call_id: "r-x-c1" } });
  check("a remote call whose parent.site is not in SITES is 400, and no worker starts", badParent.status === 400
    && (await get("cloud", "/api/runs")).json.length === runsBefore, badParent.text);
  const own = await post("local", `/api/runs/${id}/resume`, { site: "local" });
  check("resume with the run's own site name resumes here", own.status === 200 && own.json.site === "local" && own.json.segment === 2, own.text);
  const done = await waitStatus("local", id, "completed");
  check("the run completes", sameJson(done.result, TRIP("Tomar")), JSON.stringify(done.result));
  const late = await post("local", `/api/runs/${id}/resume`, { site: "mars" });
  const lateKnown = await post("local", `/api/runs/${id}/resume`, { site: "cloud" });
  check("on a completed run an unknown site is 400 and a known site is 409", late.status === 400 && lateKnown.status === 409, `${late.status} ${lateKnown.status}`);
}

// Needs servers that this script owns, because it restarts cloud2 with another pool.
async function sceneP() {
  section("p. REMOTE_POOL with only the caller's site: `no remote site available`");
  await stopServer("cloud2");
  await startServer("cloud2", { REMOTE_POOL: JSON.stringify(["cloud2"]) });
  const info = (await get("cloud2", "/api/info")).json;
  check("info.remote_pool shows the configured pool", sameJson(info.remote_pool, ["cloud2"]), JSON.stringify(info.remote_pool));
  const stream = new Stream("cloud2");
  await stream.open();
  const before = (await Promise.all(SITE_NAMES.map((site) => get(site, "/api/runs")))).map((r) => r.json.length);
  const plain = (await post("cloud2", "/api/runs", { function: "plan_trip", args: { city: "Sagres" } })).json.id;
  const durable = (await post("cloud2", "/api/runs", { function: "durable_plan_trip", args: { city: "Sagres" } })).json.id;
  const [a, b] = await Promise.all([waitStatus("cloud2", plain, ["failed", "completed", "lost"]), waitStatus("cloud2", durable, ["failed", "completed", "lost"])]);
  check("plan_trip fails instead of hanging: the user code sees a throw that names the cause", a.status === "failed" && String(a.error).includes("no remote site available"), `${a.status} ${a.error}`);
  check("durable_plan_trip fails the same way", b.status === "failed" && String(b.error).includes("no remote site available"), `${b.status} ${b.error}`);
  measure("p", "error of the failed run", { error: a.error });
  const returned = stream.find((e) => e.type === "remote_returned" && e.run === plain);
  check("the error reached the worker as a remote_result (remote_returned ok false, remote_result_received, exit code 1)", returned && returned.ok === false
    && !!stream.find((e) => e.type === "remote_result_received" && e.run === plain)
    && stream.find((e) => e.type === "worker_exit" && e.run === plain)?.exit_code === 1
    && a.remote_results.length === 1 && a.remote_results[0].error === "no remote site available", JSON.stringify(a.remote_results));
  const failedEv = stream.find((e) => e.type === "failed" && e.run === plain);
  check("the `failed` event carries a stack that points at the remote call (line 42 of plan_trip)", failedEv && Array.isArray(failedEv.stack)
    && failedEv.stack.some((f) => f.function === "plan_trip" && f.line === 42), JSON.stringify(failedEv?.stack));
  const after = (await Promise.all(SITE_NAMES.map((site) => get(site, "/api/runs")))).map((r) => r.json.length);
  check("no remote_dispatched event, and no child run on any site", !stream.find((e) => e.type === "remote_dispatched")
    && after[0] === before[0] && after[1] === before[1] && after[2] === before[2] + 2, JSON.stringify({ before, after }));
  stream.close();
}

// Needs servers that this script owns, because it stops and restarts the local one.
async function sceneL() {
  section("l. the local site server stops and restarts while a durable run is in its loop");
  const id = (await post("local", "/api/runs", { function: "durable_plan_trip", args: { city: "Coimbra" } })).json.id;
  const withSnap = await waitFor("automatic snapshot", async () => {
    const r = (await get("local", `/api/runs/${id}`)).json;
    return r.snapshots.length > 0 ? r : null;
  });
  const pid = withSnap.pid;
  await stopServer("local");
  const gone = await waitFor("worker to exit after the server stopped", () => !pidAlive(pid), 5000).catch(() => false);
  check("the worker exits when its site server stops (stdin end of input means cancel)", gone === true, `pid ${pid} still alive`);
  if (!gone) { try { process.kill(pid, "SIGKILL"); } catch { /* already gone */ } }
  await startServer("local");
  const after = (await get("local", `/api/runs/${id}`)).json;
  check("after the restart the durable run is paused with its snapshot", after.status === "paused" && after.pid === null && after.snapshots.length >= 1, `${after.status} ${after.snapshots.length}`);
  await post("local", `/api/runs/${id}/resume`);
  const done = await waitStatus("local", id, "completed");
  check("it resumes from the latest automatic snapshot and completes with the correct TripPlan", sameJson(done.result, TRIP("Coimbra")), JSON.stringify(done.result));
}

// --------------------------------------------------------------------- main

// ------------------------------------------------- contract section 9 scenes
// These scenes assert what the contract says and what program/baml_src/quotes.baml
// computes. They run when the worker binary lists --program-store in its help.

const RATES = { Flight: 420, Hotel: 135, Car: 48, Tour: 75 };
const VENDORS = { Flight: "Skyways", Hotel: "Casa Azul", Car: "Rodas", Tour: "Seven Hills Walks" };
function expectedRequest(city, kind, delayMs, options = {}) {
  return { city, kind, nights: 3, delay_ms: delayMs, traveler: { name: "Ada", loyalty_tier: 2 }, options: { currency: "EUR", ...options } };
}
function expectedQuote(city, kind, delayMs) {
  return {
    request: expectedRequest(city, kind, delayMs),
    vendor: VENDORS[kind],
    price: { amount: kind === "Flight" ? RATES.Flight : RATES[kind] * 3, currency: "EUR" },
    tags: [kind.toLowerCase(), city.toLowerCase()],
    extras: { insurance: 12, late_checkout: 30 },
    note: "loyalty tier 2 applied for Ada",
  };
}
const allEvents = () => SITE_NAMES.flatMap((site) => STREAMS[site]?.events ?? []);
const eventsOf = (id, type) => allEvents().filter((e) => e.type === type && e.run === id);
const countBy = (list, key) => list.reduce((m, e) => m.set(e[key], (m.get(e[key]) ?? 0) + 1), new Map());
const eachOnce = (list, key, wanted) => { const c = countBy(list, key); return wanted.every((k) => c.get(k) === 1) && c.size === wanted.length; };
const childrenOf = (parentId) => poolChildren((r) => r.parent.run === parentId);
const isLive = (status) => ["starting", "running", "pausing"].includes(status);
const stdoutOf = (id) => eventsOf(id, "log").filter((e) => e.stream === "stdout").map((e) => e.text);
/** The command lines of the worker processes of a run that exist right now. */
function workerCommands(id) {
  const out = spawnSync("ps", ["-axo", "pid=,command="], { encoding: "utf8" }).stdout;
  return out.split("\n").filter((l) => l.includes(` --run ${id} `) && l.includes(" worker "));
}
const storeFile = (site, hash) => path.join(SERVER_DIR, SITES[site].programs, hash.slice(0, 2), `${hash}.bamlprog`);

async function sceneX() {
  section("x. program store: fetch on first use, verify, run remote children by hash without --project");
  const fetchedBefore = allEvents().filter((e) => e.type === "program_fetched").length;
  // Two parents, so that round-robin placement reaches both pool sites.
  const ids = [];
  for (const city of ["Lisbon", "Porto"]) ids.push((await post("local", "/api/runs", { function: "plan_trip", args: { city } })).json.id);
  const hello = await STREAMS.local.waitEvent("hello", (e) => e.type === "hello" && e.run === ids[0]);
  const hash = hello.program_hash;
  check("hello carries program_hash (64 hex characters), and local's store holds the compiled program", /^[0-9a-f]{64}$/.test(hash ?? "")
    && (await waitFor("the store entry", () => fs.existsSync(storeFile("local", hash)), 20000)), String(hash));
  const dispatches = await Promise.all(ids.map((id) => STREAMS.local.waitEvent("remote_dispatched", (e) => e.type === "remote_dispatched" && e.run === id)));
  check("the two calls went to the two pool sites in round-robin order", dispatches[0].child_site !== dispatches[1].child_site
    && dispatches.every((d) => inPool(d.child_site, "local")), JSON.stringify(dispatches.map((d) => d.child_site)));
  // The child's command line, while it runs.
  const commands = await waitFor("a child worker process", () => { const c = workerCommands(dispatches[0].child_run); return c.length > 0 ? c : null; }, 10000).catch(() => []);
  check("the child worker was started with --program-hash and --program-store, and without --project", commands.length > 0
    && commands[0].includes(`--program-hash ${hash}`) && commands[0].includes("--program-store ") && !commands[0].includes("--project "), commands[0]?.slice(0, 300));
  const done = await Promise.all(ids.map((id) => waitStatus("local", id, "completed")));
  check("both parents complete with the results of children that ran by hash", sameJson(done[0].result, TRIP("Lisbon")) && sameJson(done[1].result, TRIP("Porto")));
  const fetched = allEvents().filter((e) => e.type === "program_fetched").slice(fetchedBefore);
  check("cloud and cloud2 each fetched the program once, from local", fetched.length === 2 && eachOnce(fetched, "site", ["cloud", "cloud2"])
    && fetched.every((e) => e.hash === hash && e.from_site === "local"), JSON.stringify(fetched.map((e) => [e.site, e.from_site, e.bytes, e.ms])));
  measure("x", "program fetch (site to site, verify, store)", Object.fromEntries(fetched.map((e) => [e.site, { bytes: e.bytes, ms: e.ms }])));
  const bytes = SITE_NAMES.map((site) => fs.readFileSync(storeFile(site, hash)));
  check("the fetched entries are byte-identical to the entry that local's worker wrote", bytes[1].equals(bytes[0]) && bytes[2].equals(bytes[0]));
  const served = await get("local", `/api/programs/${hash}`);
  const payload = Buffer.from(served.json?.program_base64 ?? "", "base64");
  check("GET /api/programs/:hash returns {hash, runtime_build, program_base64}, and the SHA-256 of the bytes is the hash", served.status === 200 && served.json.hash === hash
    && typeof served.json.runtime_build === "string" && sha256(payload) === hash, `${served.status} runtime_build ${served.json?.runtime_build} header ${served.json?.header_base64?.length}`);
  check("the server read the entry with the documented header layout (runtime_build is not empty)", (served.json?.runtime_build ?? "").length > 0, JSON.stringify(served.json?.runtime_build));
  const kids = (await Promise.all(ids.map(childrenOf))).flat();
  check("the child records carry the parent's program_hash", kids.length === 2 && kids.every((k) => k.program_hash === hash));
  const again = (await post("local", "/api/runs", { function: "plan_trip", args: { city: "Faro" } })).json.id;
  await waitStatus("local", again, "completed");
  check("second use: nothing is fetched again", allEvents().filter((e) => e.type === "program_fetched").length === fetchedBefore + 2);
  const unknown = await get("cloud", `/api/programs/${"0".repeat(64)}`);
  check("GET /api/programs/:hash for an unknown program is 404", unknown.status === 404);
}
const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");

async function sceneQ() {
  section("q. durable sleep: durable_nap suspends itself, the timer resumes it");
  const t0 = Date.now();
  const id = (await post("local", "/api/runs", { function: "durable_nap", args: { seconds: 7 } })).json.id;
  const sleeping = await waitStatus("local", id, "sleeping");
  const pausedEv = eventsOf(id, "paused")[0];
  const pid1 = eventsOf(id, "hello")[0].pid;
  check("paused carries wake {reason: sleep, remaining_ms, at_ts}, and the process is gone", pausedEv?.wake?.reason === "sleep" && pausedEv.wake.remaining_ms > 0
    && pausedEv.wake.remaining_ms <= 7000 && typeof pausedEv.wake.at_ts === "number" && sleeping.pid === null && !pidAlive(pid1), JSON.stringify(pausedEv?.wake));
  check("status sleeping with wake_at = receipt time plus remaining_ms", typeof sleeping.wake_at === "number" && Math.abs(sleeping.wake_at - (t0 + 7000)) < 3000, `${sleeping.wake_at - t0} ms after the start`);
  const exit1 = await STREAMS.local.waitEvent("worker_exit", (e) => e.type === "worker_exit" && e.run === id);
  const scheduled = await STREAMS.local.waitEvent("sleep_scheduled", (e) => e.type === "sleep_scheduled" && e.run === id);
  check("worker_exit: exit code 75, status sleeping; sleep_scheduled carries wake_at", exit1.exit_code === 75 && exit1.status === "sleeping" && scheduled.wake_at === sleeping.wake_at);
  recordPause("q", sleeping.snapshots.at(-1).stats);
  const kill = await post("local", `/api/runs/${id}/kill`);
  check("kill of a sleeping run is 409", kill.status === 409);
  const done = await waitStatus("local", id, "completed");
  const woken = eventsOf(id, "woken");
  check("one woken {reason: timer} at wake_at", woken.length === 1 && woken[0].reason === "timer" && woken[0].ts >= sleeping.wake_at && woken[0].ts - sleeping.wake_at < 1500, JSON.stringify(woken));
  const resumed = eventsOf(id, "resumed")[0];
  check("segment 2 ran in a new process with the program from the store", done.segment === 2 && eachOnce(eventsOf(id, "hello"), "segment", [1, 2])
    && eventsOf(id, "hello")[1].pid !== pid1 && resumed?.stats.program_source === "store", JSON.stringify(resumed?.stats));
  measure("q", "ResumeStats of the timer resume (program from the store)", resumed?.stats);
  const hello2 = eventsOf(id, "hello")[1];
  const firstProgram = allEvents().find((e) => e.run === id && e.segment === 2 && ["position", "log", "completed"].includes(e.type));
  measure("q", "timer wake -> events of the new process (ms after wake_at)", { woken_event: woken[0]?.ts - sleeping.wake_at, hello: hello2?.ts - sleeping.wake_at,
    resumed: resumed?.ts - sleeping.wake_at, first_program_event: firstProgram?.type, first_program_event_ms: firstProgram?.ts - sleeping.wake_at });
  const started1 = eventsOf(id, "hello")[0];
  measure("q", "self-suspend: hello -> paused -> process exit (ms)", { hello_to_paused: pausedEv.ts - started1.ts, paused_to_exit: exit1.ts - pausedEv.ts, snapshot_bytes: sleeping.snapshots.at(-1).bytes });
  check("the result and the output are those of an uninterrupted run", done.result === "slept 7 seconds"
    && sameJson(stdoutOf(id), ["going to sleep for 7 seconds", "woke up"]), JSON.stringify(stdoutOf(id)));
  check("the sleep was not cut short", eventsOf(id, "completed")[0].ts - t0 >= 6900, `${eventsOf(id, "completed")[0].ts - t0} ms`);
}

async function sceneR() {
  section("r. sleeping runs: manual resume, five resumes at once, cancel");
  const id = (await post("local", "/api/runs", { function: "durable_nap", args: { seconds: 14 } })).json.id;
  const first = await waitStatus("local", id, "sleeping");
  const answers = await Promise.all([1, 2, 3, 4, 5].map(() => post("local", `/api/runs/${id}/resume`)));
  check("five resume requests at once: one 200, four 409", sameJson(answers.map((r) => r.status).sort(), [200, 409, 409, 409, 409]), JSON.stringify(answers.map((r) => r.status)));
  const second = await waitFor("the second suspend", async () => {
    const r = (await get("local", `/api/runs/${id}`)).json;
    return r.status === "sleeping" && r.segment === 2 ? r : null;
  });
  check("one worker started for segment 2, and it suspended again for the same deadline", eventsOf(id, "hello").filter((e) => e.segment === 2).length === 1
    && Math.abs(second.wake_at - first.wake_at) < 1500 && eventsOf(id, "woken")[0]?.reason === "manual", `${second.wake_at - first.wake_at} ms`);
  const done = await waitStatus("local", id, "completed");
  check("the run completes in segment 3, and every segment started once", done.result === "slept 14 seconds" && eachOnce(eventsOf(id, "hello"), "segment", [1, 2, 3])
    && eventsOf(id, "completed").length === 1, JSON.stringify(eventsOf(id, "hello").map((e) => e.segment)));

  const victim = (await post("local", "/api/runs", { function: "durable_nap", args: { seconds: 6 } })).json.id;
  const asleep = await waitStatus("local", victim, "sleeping");
  const cancelled = await post("local", `/api/runs/${victim}/cancel`);
  check("cancel of a sleeping run: cancelled without a process", cancelled.status === 200 && cancelled.json.status === "cancelled" && cancelled.json.wake_at === null);
  await sleep(Math.max(0, asleep.wake_at + 800 - Date.now()));
  check("its timer fires into nothing", (await get("local", `/api/runs/${victim}`)).json.status === "cancelled" && eventsOf(victim, "hello").length === 1 && eventsOf(victim, "woken").length === 0);
}

// The first remote call of the n-th thread that the root spawned (contract
// section 9.7): the id names the spawn path, not the order in which the
// scheduler ran the threads.
const spawnCall = (id, n) => `${id}-c0.${n - 1}-1`;

async function fanOut(label, city, timeoutMs = 60000) {
  const id = (await post("local", "/api/runs", { function: "durable_fan_out", args: { city } })).json.id;
  const calls = [1, 2, 3, 4].map((n) => spawnCall(id, n));
  const delays = [3000, 5000, 2000, 4000];
  const kinds = ["Flight", "Hotel", "Car", "Tour"];
  const sleeping = await waitStatus("local", id, "sleeping");
  check(`${label}the parent suspended itself while four threads wait on remote calls: no process`, sleeping.pid === null && workerCommands(id).length === 0
    && eventsOf(id, "thread_started").filter((e) => e.segment === 1).length >= 5, `${eventsOf(id, "thread_started").length} threads`);
  recordPause(`${label}s`, sleeping.snapshots.at(-1).stats);
  {
    const pausedEv = eventsOf(id, "paused")[0], lastCall = eventsOf(id, "remote_call").at(-1), exitEv = await STREAMS.local.waitEvent("worker_exit", (e) => e.type === "worker_exit" && e.run === id);
    measure(`${label}s`, "self-suspend of the fan-out with five threads (ms)", { last_remote_call_to_paused: pausedEv.ts - lastCall.ts, paused_to_exit: exitEv.ts - pausedEv.ts,
      snapshot_bytes: sleeping.snapshots.at(-1).bytes, threads: sleeping.snapshots.at(-1).stats.threads });
  }
  const state = (await get("local", `/api/runs/${id}/snapshots/${sleeping.snapshots.at(-1).n}/state`)).json;
  check(`${label}the StateDump lists several threads`, state.threads.length >= 5, `${state.threads.length}`);
  await waitFor("four dispatches", () => eventsOf(id, "remote_dispatched").length === 4, 30000);
  const site = Object.fromEntries(eventsOf(id, "remote_dispatched").map((e) => [e.call_id, e.child_site]));
  const perSite = countBy(eventsOf(id, "remote_dispatched"), "child_site");
  check(`${label}round-robin placement: two children on cloud and two on cloud2`, perSite.get("cloud") === 2 && perSite.get("cloud2") === 2, JSON.stringify(site));
  const kids = await childrenOf(id);
  check(`${label}each child got its class-valued argument (enum, nested class, optional, map)`, kids.length === 4 && kids.every((k) => {
    const kind = k.args.request?.kind;
    return kinds.includes(kind) && isDeepStrictEqual(k.args.request, expectedRequest(city, kind, delays[kinds.indexOf(kind)]));
  }), JSON.stringify(kids.map((k) => k.args)).slice(0, 300));
  const parentHash = eventsOf(id, "hello")[0]?.program_hash;
  const childHellos = await waitFor("hello of the four children", () => { const h = kids.map((k) => eventsOf(k.id, "hello")[0]); return h.every(Boolean) ? h : null; }, 20000).catch(() => kids.map((k) => eventsOf(k.id, "hello")[0]));
  check(`${label}every child ran the parent's program by hash from its site's store, which the site fetched from local: no compile, no project`,
    /^[0-9a-f]{64}$/.test(parentHash ?? "") && childHellos.every((h) => h && h.program_hash === parentHash && h.program_source === "store" && h.program_compile_ms === null)
    && kids.every((k) => k.program_hash === parentHash && fs.existsSync(storeFile(k.site, parentHash)))
    && ["cloud", "cloud2"].every((site) => allEvents().some((e) => e.type === "program_fetched" && e.site === site && e.hash === parentHash && e.from_site === "local")),
    JSON.stringify(childHellos.map((h) => [h?.site, h?.program_source, h?.program_compile_ms])));
  const stored = await waitFor("four stored results on the sleeping parent", async () => {
    const r = (await get("local", `/api/runs/${id}`)).json;
    return r.remote_results.length === 4 ? r : null;
  }, 40000);
  check(`${label}all four results were stored while no process existed`, stored.status === "sleeping" && stored.pid === null
    && stored.remote_results.every((r) => r.acked === false && r.error === null) && eventsOf(id, "remote_result_received").length === 0, stored.status);
  const done = await waitStatus("local", id, "completed", timeoutMs);
  const quotes = kinds.map((kind, i) => expectedQuote(city, kind, delays[i]));
  const expected = { city, quotes, total: { amount: 1194, currency: "EUR" }, vendors: VENDORS, cheapest: quotes[2] };
  check(`${label}the TripReport is that of an uninterrupted run: nested classes, enums, maps, and the optional field`, isDeepStrictEqual(done.result, expected), JSON.stringify(done.result).slice(0, 400));
  check(`${label}exactly once: dispatched, returned, and received once per call, in segment 2; four children`, eachOnce(eventsOf(id, "remote_dispatched"), "call_id", calls)
    && eachOnce(eventsOf(id, "remote_returned"), "call_id", calls) && eachOnce(eventsOf(id, "remote_result_received"), "call_id", calls)
    && eventsOf(id, "remote_result_received").every((e) => e.segment === 2) && (await childrenOf(id)).length === 4 && eventsOf(id, "completed").length === 1);
  {
    const woken = eventsOf(id, "woken")[0], hello2 = eventsOf(id, "hello")[1], resumed = eventsOf(id, "resumed")[0], completed = eventsOf(id, "completed")[0];
    measure(`${label}s`, "timer wake of the fan-out -> new process (ms after the woken event)", { hello: hello2?.ts - woken?.ts, resumed: resumed?.ts - woken?.ts, completed: completed?.ts - woken?.ts,
      program_source: resumed?.stats.program_source, program_load_ms: fmt(resumed?.stats.program_load_ms), decode_ms: fmt(resumed?.stats.decode_ms) });
  }
  check(`${label}the println lines appear once each, in order`, sameJson(stdoutOf(id), [`asking four vendors for ${city}`, "sleeping 12 seconds while the vendors work", "awake again, collecting the quotes"]), JSON.stringify(stdoutOf(id)));
  return done;
}
const sceneS = () => { section("s. durable_fan_out: fan-out with a durable sleep"); return fanOut("", "Lisbon"); };

async function race(label, city) {
  const id = (await post("local", "/api/runs", { function: "durable_race", args: { city } })).json.id;
  const calls = [1, 2, 3].map((n) => spawnCall(id, n));
  const done = await waitStatus("local", id, "completed", 60000);
  check(`${label}the run returns the Quote of the fastest vendor`, isDeepStrictEqual(done.result, expectedQuote(city, "Hotel", 2000)), JSON.stringify(done.result).slice(0, 300));
  const kids = await waitFor("the losers to end", async () => {
    const list = await childrenOf(id);
    return list.length === 3 && list.every((k) => !isLive(k.status)) ? list : null;
  }, 40000);
  const losers = [calls[1], calls[2]];
  // Cancellation is a request. Under CHAOS the cancel reaches the 6 second
  // loser with a small margin, so that loser may complete. Its result is
  // discarded all the same. The 9 second loser is always cancelled.
  const status = Object.fromEntries(kids.map((k) => [k.parent.call_id, k.status]));
  check(`${label}remote_cancel from the worker and remote_cancelled from the server for both losers, which end as cancelled`, eachOnce(eventsOf(id, "remote_cancel"), "call_id", losers)
    && eachOnce(eventsOf(id, "remote_cancelled"), "call_id", losers) && status[calls[0]] === "completed" && status[calls[2]] === "cancelled"
    && (status[calls[1]] === "cancelled" || (label.startsWith("CHAOS") && status[calls[1]] === "completed")), JSON.stringify(status));
  await waitFor("two discarded results", () => eventsOf(id, "remote_result_discarded").length >= 2, 30000);
  const after = (await get("local", `/api/runs/${id}`)).json;
  check(`${label}the late results were discarded: only the winner's result is on the record`, after.remote_results.length === 1 && after.waiting_on.length === 0
    && eachOnce(eventsOf(id, "remote_dispatched"), "call_id", calls) && eventsOf(id, "remote_returned").length === 1, JSON.stringify(after.remote_results.map((r) => r.call_id)));
}
const sceneT = () => { section("t. durable_race: the losers are cancelled on their sites"); return race("", "Porto"); };

async function deadline(label, city) {
  const id = (await post("local", "/api/runs", { function: "durable_deadline", args: { city } })).json.id;
  const done = await waitStatus("local", id, "completed", 60000);
  check(`${label}the function returns a message built from the Timeout error`, done.result === `no tour quote for ${city}: operation timed out after 2000ms`, JSON.stringify(done.result));
  const kids = await waitFor("the child to be cancelled", async () => {
    const list = await childrenOf(id);
    return list.length === 1 && list[0].status === "cancelled" ? list : null;
  }, 30000);
  check(`${label}the child was cancelled on its site`, kids[0].status === "cancelled" && eventsOf(id, "remote_cancel").length === 1 && eventsOf(id, "remote_cancelled").length === 1);
}
const sceneU = () => { section("u. durable_deadline: with_timeout cancels the remote call"); return deadline("", "Faro"); };

async function sceneV() {
  section("v. durable_settled: all_settled with one failing child");
  const id = (await post("local", "/api/runs", { function: "durable_settled", args: { city: "Lisbon" } })).json.id;
  const done = await waitStatus("local", id, "completed", 60000);
  check("two quotes and one failure with its kind and the text of the remote error", isDeepStrictEqual(done.result.succeeded, [expectedQuote("Lisbon", "Flight", 2000), expectedQuote("Lisbon", "Tour", 4000)])
    && done.result.failed.length === 1 && done.result.failed[0].kind === "Car" && done.result.failed[0].error.includes("no Car vendor answers in Lisbon"), JSON.stringify(done.result).slice(0, 400));
  const kids = await childrenOf(id);
  check("the Car child failed, the others completed, none was cancelled", kids.length === 3 && kids.filter((k) => k.status === "failed").length === 1
    && kids.filter((k) => k.status === "completed").length === 2 && eventsOf(id, "remote_cancel").length === 0, JSON.stringify(kids.map((k) => k.status)));
}

async function sceneW() {
  section("w. a parent that is cancelled or lost cancels its children");
  const cancelling = (await post("local", "/api/runs", { function: "durable_race", args: { city: "Evora" } })).json.id;
  await waitFor("three dispatches", () => eventsOf(cancelling, "remote_dispatched").length === 3, 30000);
  await post("local", `/api/runs/${cancelling}/cancel`);
  await waitStatus("local", cancelling, "cancelled");
  const kids = await waitFor("the children to be cancelled", async () => {
    const list = await childrenOf(cancelling);
    return list.length === 3 && list.every((k) => k.status === "cancelled") ? list : null;
  }, 30000);
  check("all three children of the cancelled parent were cancelled by the server", kids.length === 3
    && eachOnce(eventsOf(cancelling, "remote_cancelled"), "call_id", [1, 2, 3].map((n) => spawnCall(cancelling, n))));
  const asleep = (await post("local", "/api/runs", { function: "durable_fan_out", args: { city: "Guarda" } })).json.id;
  await waitStatus("local", asleep, "sleeping");
  await waitFor("four dispatches", () => eventsOf(asleep, "remote_dispatched").length === 4, 30000);
  const c = await post("local", `/api/runs/${asleep}/cancel`);
  const kids2 = await waitFor("the children of the sleeping parent to end", async () => {
    const list = await childrenOf(asleep);
    return list.length === 4 && list.every((k) => !isLive(k.status)) ? list : null;
  }, 30000);
  check("cancel of a sleeping parent: cancelled without a process, and its outstanding children are cancelled", c.json.status === "cancelled"
    && kids2.filter((k) => k.status === "cancelled").length >= 1 && eventsOf(asleep, "remote_cancelled").length >= 1, JSON.stringify(kids2.map((k) => k.status)));
}

// ------------------------------------------------ scenes of the phase 3 review

async function sceneRaceReplay() {
  section("1. a race whose results all arrive while the run is paused returns the first arrival, every time");
  const city = "Braga";
  const id = (await post("local", "/api/runs", { function: "durable_race", args: { city } })).json.id;
  await waitFor("three dispatches", () => eventsOf(id, "remote_dispatched").length === 3, 30000);
  await post("local", `/api/runs/${id}/pause`);
  const paused = await waitStatus("local", id, "paused", 30000);
  check("the race is paused with five threads", paused.snapshots.at(-1).stats.threads === 5, JSON.stringify(paused.snapshots.at(-1).stats.threads));
  const stored = await waitFor("all three results on the paused run", async () => {
    const r = (await get("local", `/api/runs/${id}`)).json;
    return r.remote_results.length === 3 ? r : null;
  }, 40000);
  const byArrival = [...stored.remote_results].sort((a, b) => a.ts - b.ts).map((r) => r.call_id);
  check("the results arrived in the order 2 s, 6 s, 9 s", isDeepStrictEqual(byArrival, [1, 2, 3].map((n) => spawnCall(id, n))), JSON.stringify(byArrival));
  // Four forks and the run itself: five executions from the same snapshot.
  const forks = [];
  for (let i = 0; i < 4; i += 1) forks.push((await post("local", `/api/runs/${id}/fork`)).json.id);
  const winners = [];
  for (const run of [...forks, id]) {
    await post("local", `/api/runs/${run}/resume`);
    const done = await waitStatus("local", run, "completed", 60000);
    winners.push(done.result?.request?.delay_ms);
    const received = eventsOf(run, "remote_result_received");
    check(`run ${run}: only the first arrival is taken, and the two later results are abandoned`, received.length === 1 && received[0].call_id === spawnCall(id, 1)
      && eachOnce(eventsOf(run, "remote_cancel"), "call_id", [spawnCall(id, 2), spawnCall(id, 3)]), JSON.stringify(received.map((e) => e.call_id)));
  }
  check("all five executions return the 2 s vendor", winners.every((ms) => ms === 2000), JSON.stringify(winners));
}

async function sceneEarlyResume() {
  section("2. a manual resume long before the sleep deadline delivers the stored results");
  const id = (await post("local", "/api/runs", { function: "durable_fan_out", args: { city: "Viseu" } })).json.id;
  const calls = [1, 2, 3, 4].map((n) => spawnCall(id, n));
  await waitStatus("local", id, "sleeping", 30000);
  await waitFor("two stored results", async () => (await get("local", `/api/runs/${id}`)).json.remote_results.length >= 2, 30000);
  const before = (await get("local", `/api/runs/${id}`)).json;
  const offered = before.remote_results.map((r) => r.call_id);
  const resumed = await post("local", `/api/runs/${id}/resume`);
  await waitFor("the early segment to take the stored results", () => offered.every((c) => eventsOf(id, "remote_result_received").some((e) => e.call_id === c && e.segment === 2)), 30000);
  check("the early resume delivered every stored result in segment 2 before the run could suspend again", resumed.status === 200
    && eventsOf(id, "remote_wait").filter((e) => e.segment === 2).length === 4, JSON.stringify(offered));
  const done = await waitStatus("local", id, "completed", 60000);
  check("the run completes with four quotes in input order, each result received once", done.result.quotes.length === 4
    && eachOnce(eventsOf(id, "remote_result_received"), "call_id", calls) && isDeepStrictEqual(done.result.quotes.map((q) => q.request.kind), ["Flight", "Hotel", "Car", "Tour"]),
    JSON.stringify(done.result.quotes.map((q) => q.request.kind)));
}

async function sceneLateCancel() {
  section("3. a cancel between the worker's `paused` event and its exit");
  const outcomes = [];
  const ids = [];
  for (let i = 0; i < 4; i += 1) {
    const id = (await post("local", "/api/runs", { function: "durable_nap", args: { seconds: 7 } })).json.id;
    ids.push(id);
    await STREAMS.local.waitEvent("paused", (e) => e.type === "paused" && e.run === id, 30000);
    const cancel = await post("local", `/api/runs/${id}/cancel`);
    const ended = await waitStatus("local", id, ["cancelled"], 20000);
    outcomes.push([cancel.status, ended.status, eventsOf(id, "woken").length]);
  }
  await sleep(7500);
  check("every cancel that was accepted took effect: no run sleeps, wakes, or completes", outcomes.every(([status, ended, woken]) => status === 200 && ended === "cancelled" && woken === 0)
    && ids.every((id) => eventsOf(id, "completed").length === 0 && eventsOf(id, "sleep_scheduled").length === 0), JSON.stringify(outcomes));
}

async function sceneSharedChildren() {
  section("4. a fork and its source share their remote children");
  const id = (await post("local", "/api/runs", { function: "durable_fan_out", args: { city: "Tomar" } })).json.id;
  await waitStatus("local", id, "sleeping", 30000);
  await waitFor("four dispatches", () => eventsOf(id, "remote_dispatched").length === 4, 30000);
  const fork = (await post("local", `/api/runs/${id}/fork`)).json;
  const cancelled = await post("local", `/api/runs/${fork.id}/cancel`);
  await sleep(1000);
  const kids = await childrenOf(id);
  check("cancelling the fork cancels no child of the source", cancelled.status === 200 && kids.length === 4 && kids.every((k) => k.status !== "cancelled")
    && eventsOf(fork.id, "remote_cancelled").length === 0, JSON.stringify(kids.map((k) => k.status)));
  const done = await waitStatus("local", id, "completed", 60000);
  check("the source completes with four quotes", done.result.quotes.length === 4 && done.result.total.amount === 1194, JSON.stringify(done.result).slice(0, 160));
  // A fork of the sleeping source that is resumed at once attaches to the
  // children or takes the copied results, and completes with the same report.
  const id2 = (await post("local", "/api/runs", { function: "durable_fan_out", args: { city: "Tomar" } })).json.id;
  await waitStatus("local", id2, "sleeping", 30000);
  const early = (await post("local", `/api/runs/${id2}/fork`)).json;
  await post("local", `/api/runs/${early.id}/resume`);
  const [forkDone, sourceDone] = await Promise.all([waitStatus("local", early.id, "completed", 90000), waitStatus("local", id2, "completed", 90000)]);
  check("a fork that runs next to its source completes with the same report from the same four children", isDeepStrictEqual(forkDone.result, sourceDone.result)
    && (await childrenOf(id2)).length === 4 && (await childrenOf(early.id)).length === 0, JSON.stringify(forkDone.result).slice(0, 160));
}

// ------------------------------------------------ end-to-end scenes of phase 3

async function sceneParallelMigrate() {
  section("5. durable_plan_trip_parallel: pause while the spawned remote call is outstanding, resume on cloud2");
  const local = STREAMS.local, dest = STREAMS.cloud2;
  const city = "Coimbra";
  const id = (await post("local", "/api/runs", { function: "durable_plan_trip_parallel", args: { city } })).json.id;
  const started = await local.waitEvent("thread_started for the spawn", (e) => e.type === "thread_started" && e.run === id && e.parent_thread !== null);
  const dispatched = await local.waitEvent("remote_dispatched", (e) => e.type === "remote_dispatched" && e.run === id);
  await post("local", `/api/runs/${id}/pause`);
  const paused = await waitStatus("local", id, ["paused", "completed", "failed", "lost"]);
  const snap = paused.snapshots.at(-1);
  check("paused with two threads while the run waits on its child", paused.status === "paused" && snap?.stats.threads === 2 && paused.waiting_on.length === 1
    && paused.waiting_on[0].call_id === spawnCall(id, 1) && paused.remote_results.length === 0, JSON.stringify({ status: paused.status, threads: snap?.stats.threads, waiting_on: paused.waiting_on }));
  recordPause("5", snap.stats);
  const { res } = await resumeAndMeasure("5", "local", local, id, { site: "cloud2" }, "cloud2", dest);
  check("resume {site: cloud2} marks the local record migrated", res.status === 200 && res.json.status === "migrated" && res.json.migrated_to === "cloud2", res.text);
  const hello2 = await dest.waitEvent("hello on cloud2", (e) => e.type === "hello" && e.run === id && e.segment === 2);
  const resumed = await dest.waitEvent("resumed on cloud2", (e) => e.type === "resumed" && e.run === id && e.segment === 2);
  check("segment 2 runs on cloud2 from the program store, without a compile", hello2.site === "cloud2" && hello2.mode === "resume" && resumed.stats.program_source === "store"
    && resumed.stats.program_compile_ms === null && fs.existsSync(storeFile("cloud2", hello2.program_hash)), JSON.stringify(resumed.stats));
  const wait = await dest.waitEvent("remote_wait on cloud2", (e) => e.type === "remote_wait" && e.run === id && e.segment === 2);
  check("the restored spawned thread reports its remote wait under the original call id, and no second remote_call is made", wait.call_id === spawnCall(id, 1)
    && wait.thread === started.thread && wait.function === "remote_fetch_weather" && !dest.find((e) => e.type === "remote_call" && e.run === id), JSON.stringify(wait));
  const restarted = dest.all((e) => e.type === "thread_started" && e.run === id && e.segment === 2);
  check("cloud2 announces both threads with their original ids, the parent first", restarted.length === 2 && restarted[0].parent_thread === null
    && restarted[1].thread === started.thread && restarted[1].parent_thread === started.parent_thread, JSON.stringify(restarted.map((e) => [e.thread, e.parent_thread])));
  const done = await waitStatus("cloud2", id, ["completed", "failed", "lost"], 60000);
  check("the run completes on cloud2 with the correct TripPlan", done.status === "completed" && sameJson(done.result, TRIP(city)), `${done.status} ${done.error ?? ""} ${JSON.stringify(done.result)}`);
  const returned = await dest.waitEvent("remote_returned on cloud2", (e) => e.type === "remote_returned" && e.run === id);
  check("the child's result followed the run to cloud2: one dispatch, one child, one remote_returned (on cloud2), received once", returned.ok === true && returned.child_run === dispatched.child_run
    && eventsOf(id, "remote_dispatched").length === 1 && eventsOf(id, "remote_returned").length === 1 && (await childrenOf(id)).length === 1
    && eachOnce(eventsOf(id, "remote_result_received"), "call_id", [spawnCall(id, 1)]), JSON.stringify(eventsOf(id, "remote_returned").map((e) => e.site)));
  const lines = allEvents().filter((e) => e.type === "log" && e.run === id && e.stream === "stdout").sort((a, b) => a.ts - b.ts).map((e) => e.text);
  check("println lines once each and in order across the two sites", sameJson(lines, ["starting the weather lookup in the background", "planning day 1", "planning day 2", "planning day 3", "waiting for the weather"]), JSON.stringify(lines));
}

async function sceneResumeCost() {
  section("7. resume timings: the program from the store against a compile");
  const local = STREAMS.local;
  const id = (await post("local", "/api/runs", { function: "durable_plan_trip", args: { city: "Beja" } })).json.id;
  const hello = await local.waitEvent("hello", (e) => e.type === "hello" && e.run === id);
  await local.waitEvent("planning day 1", (e) => e.type === "log" && e.run === id && e.text === "planning day 1");
  await post("local", `/api/runs/${id}/pause`);
  await waitStatus("local", id, "paused");
  const fromStore = await resumeAndMeasure("7 store", "local", local, id);
  await post("local", `/api/runs/${id}/pause`);
  await waitStatus("local", id, "paused");
  // Without its store entry the worker compiles --project, checks the hash, and stores the program again.
  const entry = storeFile("local", hello.program_hash);
  fs.rmSync(entry);
  const compiled = await resumeAndMeasure("7 compile", "local", local, id);
  const done = await waitStatus("local", id, ["completed", "failed", "lost"], 60000);
  const a = fromStore.resumed.stats, b = compiled.resumed.stats;
  check("a resume with the entry in the store does not compile; without the entry it compiles, and the hash still matches", a.program_source === "store" && a.program_compile_ms === null
    && b.program_source === "compile" && typeof b.program_compile_ms === "number" && eventsOf(id, "hello").every((e) => e.program_hash === hello.program_hash),
    JSON.stringify({ store: a, compile: b }));
  check("the compile put the entry back, and the run completes with the correct TripPlan", fs.existsSync(entry) && done.status === "completed" && sameJson(done.result, TRIP("Beja")) && done.segment === 3,
    `${done.status} ${done.error ?? ""}`);
  check("the resume from the store loads the program faster than the compile", a.program_load_ms < b.program_load_ms, `${a.program_load_ms} ms against ${b.program_load_ms} ms`);
  measure("7", "program_load_ms of a resume", { store: fmt(a.program_load_ms), compile: fmt(b.program_load_ms) });
}

async function sceneKillFanOut() {
  section("6. kill -9 of a fan-out whose parent stays in process: recovery from an automatic snapshot with five threads");
  // With this threshold the 12 second sleep does not suspend the run, so a process exists that can be killed.
  STREAMS.local?.close();
  await stopServer("local");
  await startServer("local", { SLEEP_SUSPEND_MS: "600000" });
  const local = new Stream("local");
  await local.open();
  STREAMS.local = local;
  const city = "Lisbon";
  const id = (await post("local", "/api/runs", { function: "durable_fan_out", args: { city } })).json.id;
  const calls = [1, 2, 3, 4].map((n) => spawnCall(id, n));
  await waitFor("four dispatches", () => eventsOf(id, "remote_dispatched").length === 4, 30000);
  // Automatic snapshots are taken every 2 s. The first one sees all five threads. At the second one,
  // about 4 s after the start, some vendors have answered and their threads have ended.
  const autos = () => eventsOf(id, "snapshot").filter((e) => e.automatic === true);
  const firstEv = await waitFor("the first automatic snapshot", () => autos()[0] ?? null, 30000);
  check("the first automatic snapshot holds the five threads of the fan-out", firstEv.stats.threads === 5 && firstEv.stats.pause_latency_ms === null, JSON.stringify(firstEv.stats));
  measure("6", "automatic snapshot with five threads", Object.fromEntries(Object.entries(firstEv.stats).map(([k, v]) => [k, fmt(v)])));
  const snapEv = await waitFor("the second automatic snapshot", () => autos()[1] ?? null, 30000);
  const snapN = Number(/snap-(\d+)\.bamlsnap$/.exec(snapEv.snapshot_path)?.[1]);
  // Kill after a result that the worker took after that snapshot. The snapshot does not
  // hold it, so the recovery has to get that result again although segment 1 acknowledged it.
  const lateEv = await waitFor("a result taken after the second automatic snapshot", () => eventsOf(id, "remote_result_received").find((e) => e.ts > snapEv.ts) ?? null, 10000);
  const receivedBefore = eventsOf(id, "remote_result_received").map((e) => e.call_id);
  const before = (await get("local", `/api/runs/${id}`)).json;
  const killed = await post("local", `/api/runs/${id}/kill`);
  check("kill -9 while the parent sleeps in process: paused at the latest automatic snapshot, the process is gone", before.status === "running" && killed.status === 200 && killed.json.status === "paused"
    && killed.json.pid === null && !pidAlive(before.pid) && killed.json.snapshots.at(-1).n >= snapN,
    JSON.stringify({ before: before.status, status: killed.json?.status, pid: killed.json?.pid, alive: pidAlive(before.pid), n: killed.json?.snapshots?.map((x) => x.n), snapN }));
  const lastSnap = killed.json.snapshots.at(-1);
  // Without CHAOS two vendors have answered by then. With a slow controller fewer have.
  check("the snapshot that the recovery uses holds the threads that had not ended: a vendor that answered is done", lastSnap.stats.threads >= 1 && lastSnap.stats.threads <= 5
    && (process.env.CHAOS_ALL || lastSnap.stats.threads < 5) && lastSnap.stats.threads === 5 - eventsOf(id, "thread_ended").filter((e) => e.ts <= lastSnap.ts).length, JSON.stringify({ threads: lastSnap.stats.threads, received_before_kill: receivedBefore }));
  const dump = (await get("local", `/api/runs/${id}/snapshots/${lastSnap.n}/state`)).json;
  const values = dump.threads.flatMap((t) => t.frames).flatMap((f) => f.locals).map((l) => l.value);
  check("the StateDump of that snapshot lists its threads and holds the futures of the fan-out", dump.threads.length === lastSnap.stats.threads
    && JSON.stringify(values).includes('"kind":"future"'), JSON.stringify(dump.threads.map((t) => t.parked?.kind)));
  // The children keep running while no parent process exists.
  await waitFor("four results on the record of the killed run", async () => (await get("local", `/api/runs/${id}`)).json.remote_results.length === 4, 40000);
  const { resumed } = await resumeAndMeasure("6", "local", local, id);
  const waits = eventsOf(id, "remote_wait").filter((e) => e.segment === 2);
  const done = await waitStatus("local", id, ["completed", "failed", "lost", "sleeping"], 60000);
  const kinds = ["Flight", "Hotel", "Car", "Tour"], delays = [3000, 5000, 2000, 4000];
  const quotes = kinds.map((kind, i) => expectedQuote(city, kind, delays[i]));
  check("the recovered run completes with the TripReport of an uninterrupted run", done.status === "completed"
    && isDeepStrictEqual(done.result, { city, quotes, total: { amount: 1194, currency: "EUR" }, vendors: VENDORS, cheapest: quotes[2] }), `${done.status} ${done.error ?? ""} ${JSON.stringify(done.result).slice(0, 300)}`);
  const received2 = eventsOf(id, "remote_result_received").filter((e) => e.segment === 2).map((e) => e.call_id);
  check("segment 2 waits only for the calls that were outstanding in the snapshot and takes each of them once, from the stored results: no new dispatch, four children",
    waits.length === received2.length && waits.every((w) => received2.filter((c) => c === w.call_id).length === 1) && waits.length >= 1
    && eachOnce(eventsOf(id, "remote_dispatched"), "call_id", calls) && (await childrenOf(id)).length === 4 && resumed.stats.program_source === "store",
    JSON.stringify({ receivedBefore, waits: waits.map((w) => [w.call_id, w.has_result]), received2 }));
  check("a result that segment 1 took after the snapshot is delivered again to segment 2 (at least once between snapshot and kill)",
    lastSnap.n === snapN && waits.some((w) => w.call_id === lateEv.call_id) && received2.includes(lateEv.call_id), JSON.stringify({ late: lateEv.call_id, lastSnap: lastSnap.n, snapN, waits: waits.map((w) => w.call_id) }));
  measure("6", "calls received before the kill, and calls that segment 2 waited for", { received_before_kill: receivedBefore.length, waited_in_segment_2: waits.length });
  const lines = stdoutOf(id);
  check("the recovery repeats no println line from before the snapshot", sameJson(lines, [`asking four vendors for ${city}`, "sleeping 12 seconds while the vendors work", "awake again, collecting the quotes"]), JSON.stringify(lines));
  // Put local back as the other scenes expect it.
  local.close();
  await stopServer("local");
  await startServer("local");
  STREAMS.local = new Stream("local");
  await STREAMS.local.open();
}

async function sceneY() {
  section("y. the local site server restarts while runs sleep");
  const later = (await post("local", "/api/runs", { function: "durable_nap", args: { seconds: 16 } })).json.id;
  const overdue = (await post("local", "/api/runs", { function: "durable_nap", args: { seconds: 6 } })).json.id;
  const [laterRun, overdueRun] = await Promise.all([waitStatus("local", later, "sleeping"), waitStatus("local", overdue, "sleeping")]);
  await stopServer("local");
  await sleep(Math.max(0, overdueRun.wake_at + 500 - Date.now()));
  await startServer("local");
  const overdueDone = await waitStatus("local", overdue, "completed");
  const overdueEvents = (await get("local", `/api/runs/${overdue}/events`)).json;
  check("the overdue run resumed at once: woken {reason: restart}", overdueDone.result === "slept 6 seconds"
    && sameJson(overdueEvents.filter((e) => e.type === "woken").map((e) => e.reason), ["restart"]));
  const laterDone = await waitStatus("local", later, "completed");
  const laterEvents = (await get("local", `/api/runs/${later}/events`)).json;
  const woken = laterEvents.filter((e) => e.type === "woken");
  check("the timer of the other run was rebuilt: woken {reason: timer} at the original wake_at", laterDone.result === "slept 16 seconds" && woken.length === 1
    && woken[0].reason === "timer" && woken[0].ts >= laterRun.wake_at && woken[0].ts - laterRun.wake_at < 2000, JSON.stringify(woken));
}

async function sceneZ() {
  section("z. CHAOS: slow dispatch, results, cancel, and program fetch; duplicated and reordered results");
  const chaos = { dispatch_delay_ms: 600, result_delay_ms: 500, cancel_delay_ms: 700, import_delay_ms: 500, program_fetch_delay_ms: 400, duplicate_results: true, reorder_results: true,
    // A held result waits at most this long. With the other delays the cancel still
    // reaches the 6 second loser of the race before that loser is done.
    reorder_hold_ms: 1200,
  };
  for (const site of SITE_NAMES) STREAMS[site]?.close();
  await Promise.all(SITE_NAMES.map(stopServer));
  for (const site of ["cloud", "cloud2"]) fs.rmSync(path.join(SERVER_DIR, SITES[site].programs), { recursive: true, force: true });
  await Promise.all(SITE_NAMES.map((site) => startServer(site, { CHAOS: JSON.stringify(chaos) })));
  for (const site of SITE_NAMES) {
    STREAMS[site] = new Stream(site);
    await STREAMS[site].open();
  }
  await fanOut("CHAOS: ", "Lisbon", 90000);
  await Promise.all([race("CHAOS: ", "Porto"), deadline("CHAOS: ", "Faro")]);
  const returned = allEvents().filter((e) => e.type === "remote_returned");
  check("CHAOS: although every result was sent twice, no call returned twice", [...countBy(returned, "call_id").values()].every((n) => n === 1));
  check("CHAOS: both cloud sites fetched the program again after their stores were emptied", eachOnce(allEvents().filter((e) => e.type === "program_fetched"), "site", ["cloud", "cloud2"]));
}

async function main() {
  if (!fs.existsSync(WORKER_BIN)) throw new Error(`worker binary not found: ${WORKER_BIN} (cargo build -p baml_cli)`);
  // The first execution of a freshly linked binary can take seconds on macOS.
  // Run it once so that this cost does not land in the first scene.
  const warm = Date.now();
  spawnSync(WORKER_BIN, ["--version"], { env: { ...process.env, BAML_CLI_ALLOW_DIRECT: "1" }, stdio: "ignore" });
  measure("-", "first execution of the worker binary (`--version`)", { ms: Date.now() - warm });

  PHASE3 = workerHasPhase3();
  if (!USE_RUNNING) {
    await claimStores();
    await Promise.all(SITE_NAMES.map((site) => startServer(site)));
  }
  const infos = Object.fromEntries(await Promise.all(SITE_NAMES.map(async (site) => [site, (await get(site, "/api/info")).json])));
  const info = infos.local;
  runStores = SITE_NAMES.map((site) => infos[site].runs_dir);
  section("setup");
  check("the three site servers use the real worker", SITE_NAMES.every((site) => infos[site].site === site
    && infos[site].worker_cmd[0] === WORKER_BIN && infos[site].worker_cmd[1] === "worker"), JSON.stringify(info.worker_cmd));
  check("every site reports the same registry (local, cloud, cloud2) and the remote pool [cloud, cloud2]",
    SITE_NAMES.every((site) => sameJson(infos[site].sites.map((s) => s.name), SITE_NAMES) && sameJson(infos[site].sites, info.sites)
      && sameJson(infos[site].remote_pool, ["cloud", "cloud2"])) && sameJson(info.sites.map((s) => s.url), SITE_NAMES.map((site) => SITES[site].base))
    && !("peer_url" in info) && !("peer_site" in info), JSON.stringify({ sites: info.sites, remote_pool: info.remote_pool }));

  const local = new Stream("local");
  const cloud = new Stream("cloud");
  const cloud2 = new Stream("cloud2");
  await Promise.all([local.open(), cloud.open(), cloud2.open()]);
  Object.assign(STREAMS, { local, cloud, cloud2 });

  // Scene x looks at the first remote calls of the instance, so it runs first.
  if (PHASE3 && !USE_RUNNING && (!ONLY || ONLY.has("x"))) {
    try {
      await sceneX();
    } catch (err) {
      check("scene x ran to its end", false, String(err.message ?? err));
    }
  }
  if (!PHASE3) console.log("\nthe worker predates contract section 9: scenes x q r s t u v w y z are skipped, and the servers run with WORKER_LEGACY=1");
  const phase3Scenes = PHASE3 ? { q: sceneQ, r: sceneR, s: sceneS, t: sceneT, u: sceneU, v: sceneV, w: sceneW, 1: sceneRaceReplay, 2: sceneEarlyResume, 3: sceneLateCancel, 4: sceneSharedChildren, 5: sceneParallelMigrate, 7: sceneResumeCost } : {};

  const scenes = { a: sceneA, b: sceneB, c: sceneC, d: sceneD, e: sceneE, f: sceneF, g: sceneG, h: sceneH, i: sceneI, j: sceneJ, k: sceneK, m: sceneM, n: sceneN, o: sceneO, ...phase3Scenes };
  for (const [name, fn] of Object.entries(scenes)) {
    if (ONLY && !ONLY.has(name)) continue;
    try {
      await fn(local, cloud, cloud2);
    } catch (err) {
      check(`scene ${name} ran to its end`, false, String(err.message ?? err));
    }
  }

  section("invariants");
  const all = [...local.events, ...cloud.events, ...cloud2.events].filter((e) => e.type !== "init" && e.type !== "run");
  check("every SSE event has type, ts, and the site of its stream", [local, cloud, cloud2].every((stream) => stream.events.every((e) => typeof e.type === "string" && typeof e.ts === "number" && e.site === stream.site)));
  const nonJson = all.filter((e) => e.type === "log" && e.stream === "worker_stdout");
  check("worker stdout carried only JSON lines", nonJson.length === 0, JSON.stringify(nonJson.slice(0, 3)));
  const stderrLines = all.filter((e) => e.type === "log" && e.stream === "worker_stderr").map((e) => e.text);
  measure("-", "worker_stderr lines over all scenes", { count: stderrLines.length, distinct: [...new Set(stderrLines.map((t) => t.replace(/r-[a-z0-9]+/g, "r-…")))].slice(0, 8) });
  await sleep(300);
  // With USE_RUNNING the run stores belong to a demo that someone may be using,
  // so a worker on them is not a leftover of this script.
  if (!USE_RUNNING) {
    const left = leftoverWorkers();
    check("no worker process of this instance is left behind", left === "", left);
  }
  local.close();
  cloud.close();
  cloud2.close();

  if (!USE_RUNNING && (!ONLY || ONLY.has("l"))) {
    try {
      await sceneL();
    } catch (err) {
      check("scene l ran to its end", false, String(err.message ?? err));
    }
    const leftAfter = leftoverWorkers();
    check("no worker process of this instance is left behind after the restart scene", leftAfter === "", leftAfter);
  }
  if (!USE_RUNNING && (!ONLY || ONLY.has("p"))) {
    try {
      await sceneP();
    } catch (err) {
      check("scene p ran to its end", false, String(err.message ?? err));
    }
  }
  for (const [name, fn] of Object.entries(PHASE3 && !USE_RUNNING ? { 6: sceneKillFanOut, y: sceneY, z: sceneZ } : {})) {
    if (ONLY && !ONLY.has(name)) continue;
    try {
      await fn();
    } catch (err) {
      check(`scene ${name} ran to its end`, false, String(err.message ?? err));
    }
    const left = leftoverWorkers();
    check(`no worker process of this instance is left behind after scene ${name}`, left === "", left);
  }
  for (const site of SITE_NAMES) STREAMS[site]?.close();
}

let exitCode = 0;
try {
  await main();
} catch (err) {
  console.log(`\nERROR ${err.stack ?? err}`);
  exitCode = 1;
} finally {
  await Promise.all(Object.keys(servers).map(stopServer));
  await sleep(500);
  // Stores that this run did not claim (USE_RUNNING, or a refused start) are left alone.
  releaseStores(process.env.KEEP !== "1");
  if (record) record.end();
}

console.log("\n== measurements (debug build unless WORKER_BIN says otherwise)");
for (const m of measurements) console.log(`  [${m.scene}] ${m.what}: ${JSON.stringify(m.value)}`);
console.log(`\n${passed} passed, ${failures.length} failed`);
for (const f of failures) console.log(`  failed: ${f}`);
process.exit(exitCode || (failures.length ? 1 : 0));
