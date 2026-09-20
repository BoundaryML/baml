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
//   ONLY=a,b,c                      run only the named scenes (a b c d e f g h i j k m n o l p)
//
// Without USE_RUNNING the script starts its own site servers with run stores in
// server/.baml/verify-real<RUNS_TAG>-<site> and stops them at the end. Its
// default ports are not the ports of dev.sh, so it does not collide with a demo
// that is running, and it refuses to start when one of its ports is in use.
// `verify.mjs` is the same kind of check for the mock worker; this file only
// asserts what the real worker does.
//
// Scenes:
//   a  durable_plan_trip completes (remote child on cloud)
//   b  the central scene: pause during the remote wait, the child finishes while
//      no parent process exists, resume in a new process
//   c  pause inside the loop, resume, no println line lost or duplicated
//   d  migration: pause on local, resume on cloud; the remote call of the migrated
//      run is placed on cloud2, and the result returns to cloud
//   e  fork of a paused run: source and fork complete independently
//   f  kill -9: plan_trip is lost, durable_plan_trip recovers from an automatic snapshot
//   g  durable_plan_trip_parallel: the pause is answered with `blocked`, the run completes
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

import { spawn, spawnSync } from "node:child_process";
import fs from "node:fs";
import net from "node:net";
import path from "node:path";
import { fileURLToPath } from "node:url";

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
const SITES = Object.fromEntries(SITE_NAMES.map((name, i) => [name, {
  port: PORT_BASE + i, base: `http://127.0.0.1:${PORT_BASE + i}`, runs: `.baml/verify-real${RUNS_TAG}-${name}`,
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
  const { PEER_URL: _peer, REMOTE_POOL: _pool, ...inherited } = process.env;
  const child = spawn("baml", ["run", "main", "--log", "warn"], {
    cwd: SERVER_DIR,
    env: {
      ...inherited,
      SITE: site, PORT: String(cfg.port), SITES: JSON.stringify(REGISTRY), RUNS_DIR: cfg.runs,
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
    fs.mkdirSync(dir, { recursive: true });
    fs.writeFileSync(lockPath(site), `${process.pid}\n`);
  }
  storesClaimed = true;
}

function releaseStores(remove) {
  if (!storesClaimed) return;
  for (const site of SITE_NAMES) {
    const dir = path.join(SERVER_DIR, SITES[site].runs);
    if (remove) fs.rmSync(dir, { recursive: true, force: true });
    else fs.rmSync(lockPath(site), { force: true });
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
  check("remote call dispatched to cloud with call id <run>-c1", dispatched.child_site === "cloud" && dispatched.call_id === `${id}-c1` && dispatched.function === "remote_fetch_weather");
  const call = local.find((e) => e.type === "remote_call" && e.run === id);
  check("remote_call event carries the args by parameter name", call && sameJson(call.args, { city: "Lisbon" }), JSON.stringify(call));
  const child = await waitStatus("cloud", dispatched.child_run, "completed");
  check("child ran on cloud, non-durable, with parent set", child.result === "sunny in Lisbon" && child.durable === false && child.parent.run === id);
  check("the child's println reached the cloud stream", !!cloud.find((e) => e.type === "log" && e.run === child.id && e.text === "[remote] looking up weather for Lisbon"));
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
  const childDuring = (await get("cloud", `/api/runs/${childId}`)).json;
  check("the child is still running on cloud while the parent is paused", childDuring.status === "running" || childDuring.status === "starting", childDuring.status);
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

  const childDone = await waitStatus("cloud", childId, "completed");
  check("the child completed on cloud while no parent process existed", childDone.result === "sunny in Lisbon" && !pidAlive(pid1)
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
    && (await get("cloud", "/api/runs")).json.filter((r) => r.parent && r.parent.run === id).length === 1);
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
  const children = (await get("cloud", "/api/runs")).json.filter((r) => r.parent && (r.parent.run === id || r.parent.run === fid));
  check("each of them made its own remote call (two child runs on cloud)", callSrc && callFork && children.length === 2
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
  await waitStatus("cloud", dispatched.child_run, "completed");
  await waitFor("stored result", async () => (await get("local", `/api/runs/${id}`)).json.remote_results.length === 1);
  await post("local", `/api/runs/${id}/resume`);
  const done = await waitStatus("local", id, ["completed", "failed", "lost"]);
  check("the resumed run completes with the stored result", done.status === "completed" && sameJson(done.result, TRIP("Lagos")) && done.remote_results[0].acked === true,
    `${done.status} ${done.error ?? ""} ${JSON.stringify(done.result)}`);
  const calls = local.all((e) => e.type === "remote_call" && e.run === id);
  check("the call id is stable, and the call was dispatched once", calls.every((c) => c.call_id === `${id}-c1`)
    && local.all((e) => e.type === "remote_dispatched" && e.run === id).length === 1
    && (await get("cloud", "/api/runs")).json.filter((r) => r.parent && r.parent.run === id).length === 1, JSON.stringify(calls.map((c) => [c.segment, c.call_id])));
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
  const childStatus = (await get("cloud", `/api/runs/${dispatched.child_run}`)).json.status;
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
  check("the child of the local parent runs on cloud", dispatched.child_site === "cloud");
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
  const childNow = (await get("cloud", `/api/runs/${dispatched.child_run}`)).json;
  measure("m", "remote_dispatched -> paused on cloud2 after two hops (the child needs about 4.3 s)", { ms: Date.now() - t0, child_status: childNow.status });
  check("the run is paused on cloud2 (segment 3, no process) while the child still runs on cloud", onCloud2.pid === null && onCloud2.segment === 3
    && onCloud2.origin.site === "cloud" && onCloud2.remote_results.length === 0 && childNow.status === "running",
    JSON.stringify({ segment: onCloud2.segment, results: onCloud2.remote_results.length, child: childNow.status }));
  const snapBytes = ["cloud", "cloud2"].map((site, i) => fs.readFileSync((i === 0 ? onCloud : onCloud2).snapshots.at(-1).snapshot_path));
  check("each site wrote its own snapshot of the run", snapBytes.every((b) => b.length > 0) && onCloud2.snapshots.at(-1).n > onCloud.snapshots.at(-1).n,
    JSON.stringify([onCloud.snapshots.map((s) => s.n), onCloud2.snapshots.map((s) => s.n)]));

  await waitStatus("cloud", dispatched.child_run, "completed");
  const stored = await waitFor("stored result on cloud2", async () => {
    const r = (await get("cloud2", `/api/runs/${id}`)).json;
    return r.remote_results.length === 1 ? r : null;
  });
  check("the result was forwarded local -> cloud -> cloud2 and is stored on the paused run", stored.status === "paused" && stored.pid === null
    && stored.remote_results[0].value === "sunny in Chaves" && stored.remote_results[0].acked === false, JSON.stringify(stored.remote_results));
  const returned = await cloud2.waitEvent("remote_returned on cloud2", (e) => e.type === "remote_returned" && e.run === id);
  check("remote_returned is emitted by cloud2 only", returned.ok === true && returned.child_run === dispatched.child_run
    && !local.find((e) => e.type === "remote_returned" && e.run === id) && !cloud.find((e) => e.type === "remote_returned" && e.run === id));
  await waitFor("result_delivered on the child", async () => (await get("cloud", `/api/runs/${dispatched.child_run}`)).json.result_delivered);
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
    && (await get("cloud", "/api/runs")).json.filter((r) => r.parent && r.parent.run === id).length === 1);
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

async function main() {
  if (!fs.existsSync(WORKER_BIN)) throw new Error(`worker binary not found: ${WORKER_BIN} (cargo build -p baml_cli)`);
  // The first execution of a freshly linked binary can take seconds on macOS.
  // Run it once so that this cost does not land in the first scene.
  const warm = Date.now();
  spawnSync(WORKER_BIN, ["--version"], { env: { ...process.env, BAML_CLI_ALLOW_DIRECT: "1" }, stdio: "ignore" });
  measure("-", "first execution of the worker binary (`--version`)", { ms: Date.now() - warm });

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

  const scenes = { a: sceneA, b: sceneB, c: sceneC, d: sceneD, e: sceneE, f: sceneF, g: sceneG, h: sceneH, i: sceneI, j: sceneJ, k: sceneK, m: sceneM, n: sceneN, o: sceneO };
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
