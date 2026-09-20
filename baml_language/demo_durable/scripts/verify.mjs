#!/usr/bin/env node
// End-to-end check of the three site servers with the mock worker.
//
//   node scripts/verify.mjs            (from demo_durable/)
//   MOCK_SPEED=4 node scripts/verify.mjs      faster simulated sleeps
//   KEEP=1 node scripts/verify.mjs            keep the run stores for inspection
//   PORT_BASE=<n>                             first port (default 18787)
//   RUNS_TAG=<tag>                            added to the run store names
//
// The script starts its own site servers: local on PORT_BASE, cloud on
// PORT_BASE + 1, and cloud2 on PORT_BASE + 2, with run stores in
// server/.baml/verify<RUNS_TAG>-<site>. The default PORT_BASE is 18787, not the
// 8787 of dev.sh, so the script never collides with a demo that is running. It
// refuses to start when one of its ports is in use. It runs every scenario,
// restarts the local server once and the cloud2 server once, and stops
// everything that it started at the end.

import { spawn } from "node:child_process";
import fs from "node:fs";
import http from "node:http";
import net from "node:net";
import path from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const SERVER_DIR = path.join(ROOT, "server");
const PORT_BASE = Number(process.env.PORT_BASE ?? "18787");
const RUNS_TAG = process.env.RUNS_TAG ?? "";
if (!Number.isInteger(PORT_BASE) || PORT_BASE < 1 || PORT_BASE > 65533) throw new Error(`PORT_BASE is not a port number: ${process.env.PORT_BASE}`);
if (!/^[A-Za-z0-9_-]*$/.test(RUNS_TAG)) throw new Error(`RUNS_TAG may contain only letters, digits, '-' and '_': ${RUNS_TAG}`);
const SITE_NAMES = ["local", "cloud", "cloud2"];
const SITES = Object.fromEntries(SITE_NAMES.map((name, i) => [name, {
  port: PORT_BASE + i, base: `http://127.0.0.1:${PORT_BASE + i}`, runs: `.baml/verify${RUNS_TAG}-${name}`,
}]));
// The registry that every site server receives (contract section 8.1).
const REGISTRY = Object.fromEntries(SITE_NAMES.map((name) => [name, SITES[name].base]));
const REGISTRY_LIST = SITE_NAMES.map((name) => ({ name, url: SITES[name].base }));
const DEFAULT_POOL = ["cloud", "cloud2"];
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
  // PORT is not set: the server takes its port from its own entry in SITES.
  // PEER_URL is set to an unusable value to show that it is no longer read.
  const { PORT: _port, REMOTE_POOL: _pool, ...inherited } = process.env;
  const child = spawn("baml", ["run", "main", "--log", "warn"], {
    cwd: SERVER_DIR,
    env: {
      ...inherited,
      SITE: site, SITES: JSON.stringify(REGISTRY), RUNS_DIR: cfg.runs,
      PEER_URL: "http://127.0.0.1:9",
      VERIFY_MARKER: `marker-${site}`,
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

async function waitFor(what, fn, timeoutMs = 30000) {
  const started = Date.now();
  while (Date.now() - started < timeoutMs) {
    const value = await fn();
    if (value) return value;
    await sleep(50);
  }
  throw new Error(`timed out waiting for ${what}`);
}

async function waitStatus(site, id, statuses, timeoutMs = 30000) {
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

class Stream {
  constructor(site) {
    this.site = site;
    this.events = [];
    this.comments = [];
    this.controller = new AbortController();
  }
  async open() {
    const res = await fetch(url(this.site, "/api/events"), { signal: this.controller.signal });
    this.contentType = res.headers.get("content-type");
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
          if (block.startsWith(":")) this.comments.push({ text: block, at: Date.now() });
          else if (block.startsWith("data: ")) this.events.push(JSON.parse(block.slice(6)));
        }
      }
    } catch { /* aborted */ }
  }
  close() { this.controller.abort(); }
  find(pred) { return this.events.find(pred); }
  waitEvent(what, pred, timeoutMs = 30000) {
    return waitFor(`SSE ${this.site}: ${what}`, () => this.events.find(pred), timeoutMs);
  }
}

// ---------------------------------------------------------------- scenarios

async function routesAndErrors() {
  section("info, source, CORS, errors");
  const info = await get("local", "/api/info");
  check("GET /api/info", info.status === 200 && info.json.site === "local");
  check("info.sites is the registry in its order, info.remote_pool is the default pool (contract 8.3)",
    JSON.stringify(info.json.sites) === JSON.stringify(REGISTRY_LIST) && JSON.stringify(info.json.remote_pool) === JSON.stringify(DEFAULT_POOL),
    JSON.stringify({ sites: info.json.sites, remote_pool: info.json.remote_pool }));
  check("info no longer has peer_url and peer_site", !("peer_url" in info.json) && !("peer_site" in info.json));
  const names = info.json.functions.map((f) => f.name).sort().join(",");
  check("info.functions lists the four demo functions",
    names === "durable_plan_trip,durable_plan_trip_parallel,plan_trip,remote_fetch_weather", names);
  const dpt = info.json.functions.find((f) => f.name === "durable_plan_trip");
  const rfw = info.json.functions.find((f) => f.name === "remote_fetch_weather");
  check("durable/remote flags and params", dpt.durable && !dpt.remote && rfw.remote && !rfw.durable
    && dpt.params.length === 1 && dpt.params[0].name === "city" && dpt.params[0].type === "string");
  check("access-control-allow-origin: *", info.headers.get("access-control-allow-origin") === "*");
  check("info extensions: worker_cmd, runs_dir", Array.isArray(info.json.worker_cmd)
    && info.json.worker_cmd.length > 0 && info.json.runs_dir.endsWith(SITES.local.runs.replace(/^\.\//, "")), JSON.stringify(info.json));
  for (const site of ["cloud", "cloud2"]) {
    const other = await get(site, "/api/info");
    check(`${site} info: same registry, and PORT came from the site's own entry in SITES`, other.status === 200 && other.json.site === site
      && JSON.stringify(other.json.sites) === JSON.stringify(REGISTRY_LIST) && JSON.stringify(other.json.remote_pool) === JSON.stringify(DEFAULT_POOL)
      && other.json.runs_dir.endsWith(SITES[site].runs.replace(/^\.\//, "")), JSON.stringify(other.json));
  }

  const options = await api("local", "OPTIONS", "/api/runs");
  check("OPTIONS returns 204 with CORS", options.status === 204 && options.headers.get("access-control-allow-origin") === "*");

  const src = await get("local", "/api/source?file=baml_src/trip.baml");
  check("GET /api/source", src.status === 200 && src.json.file === "baml_src/trip.baml" && src.json.text.includes("function durable_plan_trip"));
  const srcEnc = await get("local", "/api/source?file=baml_src%2Ftrip.baml");
  check("GET /api/source with a percent-encoded path", srcEnc.status === 200);
  const lines = src.json.text.split("\n");
  check("mock positions point at real lines",
    lines[23].includes("println") && lines[24].includes("sleep") && lines[28].includes("remote_fetch_weather(city)")
    && lines[48].includes("spawn") && lines[57].includes("await") && lines[14].includes("println") && lines[15].includes("sleep"));
  // A worker reports an absolute path when it cannot make it relative. The
  // value of `position.file` is accepted as it is.
  const absolute = `${info.json.program_dir}/baml_src/trip.baml`;
  const srcAbs = await get("local", `/api/source?file=${encodeURIComponent(absolute)}`);
  check("GET /api/source accepts an absolute path inside PROGRAM_DIR as-is", srcAbs.status === 200 && srcAbs.json.file === absolute
    && srcAbs.json.text === src.json.text, `status ${srcAbs.status}`);
  const srcOutside = await get("local", `/api/source?file=${encodeURIComponent(path.join(SERVER_DIR, "baml.toml"))}`);
  check("source rejects an absolute path outside PROGRAM_DIR", srcOutside.status === 400, `status ${srcOutside.status}`);
  for (const bad of ["../server/baml.toml", "/etc/passwd", "baml_src/../../server/baml.toml", "baml_src/%2E%2E/%2E%2E/README.md"]) {
    const r = await get("local", `/api/source?file=${bad}`);
    check(`source rejects ${bad}`, r.status === 400 && typeof r.json.error === "string", `status ${r.status}`);
  }
  const missing = await get("local", "/api/source?file=baml_src/nope.baml");
  check("source 404 for a missing file", missing.status === 404 && !!missing.json.error);

  const noRun = await get("local", "/api/runs/r-nope");
  check("GET unknown run is 404 {error}", noRun.status === 404 && typeof noRun.json.error === "string");
  const noRoute = await get("local", "/api/nothing");
  check("unknown route is 404 {error}", noRoute.status === 404 && typeof noRoute.json.error === "string");
  const badFn = await post("local", "/api/runs", { function: "does_not_exist", args: {} });
  check("POST /api/runs with an unknown function is 400", badFn.status === 400 && !!badFn.json.error);
  const badArgs = await post("local", "/api/runs", { function: "plan_trip", args: [1] });
  check("POST /api/runs with non-object args is 400", badArgs.status === 400);
  const badRemote = await post("cloud", "/api/remote/runs", { function: "remote_fetch_weather", args: { city: "X" } });
  check("POST /api/remote/runs without parent is 400", badRemote.status === 400);
  const runsBefore = (await get("cloud", "/api/runs")).json.length;
  const badParent = await post("cloud", "/api/remote/runs", { function: "remote_fetch_weather", args: { city: "X" }, parent: { site: "mars", run: "r-x", call_id: "r-x-c1" } });
  check("POST /api/remote/runs with a parent.site outside SITES is 400, and no run starts", badParent.status === 400 && badParent.json.error.includes("mars")
    && (await get("cloud", "/api/runs")).json.length === runsBefore, badParent.text);
}

async function centralScene(local, cloud) {
  section("central scene: pause the parent while the remote child runs, resume after it finished");
  const started = await post("local", "/api/runs", { function: "durable_plan_trip", args: { city: "Lisbon" } });
  const id = started.json.id;
  check("POST /api/runs returns a starting run", started.status === 200 && /^[a-z0-9-]+$/.test(id)
    && started.json.status === "starting" && started.json.segment === 1 && started.json.durable === true
    && started.json.function === "durable_plan_trip" && started.json.site === "local", started.text);

  const hello = await local.waitEvent("hello", (e) => e.type === "hello" && e.run === id);
  check("worker hello is forwarded with site", hello.site === "local" && hello.mode === "start" && hello.v === 1 && hello.pid > 0);
  check("workers get BAML_CLI_ALLOW_DIRECT=1 and keep the server's environment",
    hello.mock_env?.allow_direct === "1" && hello.mock_env?.marker === "marker-local", JSON.stringify(hello.mock_env));
  const posEv = await local.waitEvent("position", (e) => e.type === "position" && e.run === id);
  const posSrc = await get("local", `/api/source?file=${encodeURIComponent(posEv.file)}`);
  check("position.file is accepted as-is by GET /api/source", posSrc.status === 200 && posSrc.json.file === posEv.file
    && posSrc.json.text.split("\n").length >= posEv.line, `${posEv.file} -> ${posSrc.status}`);
  const running = await waitStatus("local", id, "running");
  const pid1 = running.pid;
  check("run is running with the worker pid", pid1 === hello.pid && pidAlive(pid1));

  const dispatched = await local.waitEvent("remote_dispatched", (e) => e.type === "remote_dispatched" && e.run === id);
  check("remote_dispatched event", dispatched.child_site === "cloud" && dispatched.function === "remote_fetch_weather"
    && dispatched.call_id === `${id}-c1` && dispatched.site === "local" && typeof dispatched.ts === "number");
  const childId = dispatched.child_run;
  const parentNow = (await get("local", `/api/runs/${id}`)).json;
  check("parent.waiting_on records the child", parentNow.waiting_on.length === 1 && parentNow.waiting_on[0].child_run === childId
    && parentNow.waiting_on[0].function === "remote_fetch_weather" && parentNow.waiting_on[0].child_site === "cloud");
  const child = (await get("cloud", `/api/runs/${childId}`)).json;
  check("child run on cloud has parent set", child.site === "cloud" && child.parent.site === "local"
    && child.parent.run === id && child.parent.call_id === `${id}-c1` && child.durable === false);

  // Pause while the child is still running.
  const pausing = await post("local", `/api/runs/${id}/pause`);
  check("POST pause returns status pausing", pausing.status === 200 && pausing.json.status === "pausing");
  const paused = await waitStatus("local", id, "paused");
  const childDuringPause = (await get("cloud", `/api/runs/${childId}`)).json;
  check("child is still running when the parent is paused", childDuringPause.status === "running", childDuringPause.status);
  check("paused parent has no pid, and its process is gone", paused.pid === null && !pidAlive(pid1));
  const lastSnap = paused.snapshots[paused.snapshots.length - 1];
  check("snapshot recorded with files and stats", lastSnap && fs.existsSync(lastSnap.snapshot_path)
    && fs.existsSync(lastSnap.state_path) && lastSnap.bytes > 0 && lastSnap.n === paused.snapshots.length
    && typeof lastSnap.stats.pause_latency_ms === "number" && lastSnap.automatic === false);
  check("pausing and paused worker events were forwarded",
    !!local.find((e) => e.type === "pausing" && e.run === id && Array.isArray(e.waiting_on))
    && !!local.find((e) => e.type === "paused" && e.run === id && e.snapshot_path === lastSnap.snapshot_path));
  const exit1 = local.find((e) => e.type === "worker_exit" && e.run === id && e.segment === 1);
  check("worker_exit reports exit code 75", exit1 && exit1.exit_code === 75 && exit1.status === "paused");

  const state = await get("local", `/api/runs/${id}/snapshots/${lastSnap.n}/state`);
  check("GET snapshot state returns a StateDump", state.status === 200 && state.json.run === id
    && state.json.threads[0].parked.kind === "remote_call" && state.json.threads[0].frames[0].line === 29
    && state.json.threads[0].frames[0].locals.some((l) => l.name === "ideas" && l.value.kind === "array" && l.value.children.length === 3)
    && state.json.heap.objects > 0, state.text.slice(0, 200));
  const noState = await get("local", `/api/runs/${id}/snapshots/99/state`);
  check("snapshot state 404 for an unknown n", noState.status === 404);

  // The child finishes while the parent has no process.
  const childDone = await waitStatus("cloud", childId, "completed");
  check("child completed on cloud", childDone.result === "sunny in Lisbon");
  await waitFor("child result delivered", async () => (await get("cloud", `/api/runs/${childId}`)).json.result_delivered);
  const returned = await local.waitEvent("remote_returned", (e) => e.type === "remote_returned" && e.run === id);
  check("remote_returned event", returned.ok === true && returned.child_run === childId && returned.child_site === "cloud" && returned.call_id === `${id}-c1`);
  const stored = (await get("local", `/api/runs/${id}`)).json;
  check("result is stored while the parent is paused", stored.status === "paused" && stored.waiting_on.length === 0
    && stored.remote_results.length === 1 && stored.remote_results[0].value === "sunny in Lisbon" && stored.remote_results[0].acked === false);
  const meta = JSON.parse(fs.readFileSync(path.join(SERVER_DIR, SITES.local.runs, id, "meta.json"), "utf8"));
  check("meta.json on disk holds the stored result", meta.status === "paused" && meta.remote_results[0].call_id === `${id}-c1` && meta.function === "durable_plan_trip");

  // Resume in a new process.
  const resumed = await post("local", `/api/runs/${id}/resume`);
  check("POST resume starts segment 2", resumed.status === 200 && resumed.json.segment === 2 && resumed.json.status === "starting");
  const done = await waitStatus("local", id, "completed");
  check("parent completed with the remote result", done.result.weather === "sunny in Lisbon" && done.result.ideas.length === 3 && done.result.city === "Lisbon");
  const hello2 = local.find((e) => e.type === "hello" && e.run === id && e.segment === 2);
  check("segment 2 ran in a different process", hello2 && hello2.mode === "resume" && hello2.pid !== pid1);
  check("resumed and remote_result_received were forwarded",
    !!local.find((e) => e.type === "resumed" && e.run === id && typeof e.stats.decode_ms === "number")
    && !!local.find((e) => e.type === "remote_result_received" && e.run === id && e.segment === 2));
  check("stored result is acknowledged", done.remote_results[0].acked === true);

  const events = await get("local", `/api/runs/${id}/events`);
  const types = new Set(events.json.map((e) => e.type));
  check("GET /api/runs/:id/events returns events.jsonl",
    events.status === 200 && ["hello", "log", "position", "remote_call", "remote_dispatched", "paused", "remote_returned", "resumed", "completed", "thread_started", "thread_ended"].every((t) => types.has(t))
    && events.json.every((e) => e.site === "local" && typeof e.ts === "number"), [...types].join(","));
  check("events.jsonl holds worker_exit and the other server events, but no run messages",
    types.has("worker_exit") && !types.has("run") && !types.has("init")
    && events.json.filter((e) => e.type === "worker_exit").length === 2, [...types].join(","));
  check("automatic snapshots: `snapshot` events match snapshots[].automatic and .segment", (() => {
    const evs = events.json.filter((e) => e.type === "snapshot");
    const auto = done.snapshots.filter((s) => s.automatic);
    return evs.length > 0 && evs.length === auto.length && evs.every((e) => e.automatic === true && typeof e.stats === "object"
      && auto.some((s) => s.snapshot_path === e.snapshot_path && s.segment === e.segment));
  })());
  check("snapshots[].n matches the worker's snap-<n> file names", done.snapshots.every((s) => s.snapshot_path.endsWith(`/snap-${s.n}.bamlsnap`)),
    JSON.stringify(done.snapshots.map((s) => [s.n, path.basename(s.snapshot_path)])));
  const onCloud2 = (await get("cloud2", "/api/runs")).json.filter((r) => r.parent && r.parent.run === id);
  const onLocal = (await get("local", "/api/runs")).json.filter((r) => r.parent && r.parent.run === id);
  check("placement: a parent on local calls into cloud, and no child ran on cloud2 or on local", onCloud2.length === 0 && onLocal.length === 0
    && (await get("cloud", "/api/runs")).json.filter((r) => r.parent && r.parent.run === id).length === 1);
  check("cloud stream carried the child's events",
    !!cloud.find((e) => e.type === "log" && e.run === childId && e.text.includes("looking up weather") && e.site === "cloud")
    && !!cloud.find((e) => e.type === "run" && e.run.id === childId && e.run.status === "completed"));
  check("run events follow every change", local.events.filter((e) => e.type === "run" && e.run.id === id).length >= 8);
  return id;
}

async function liveDelivery() {
  section("remote result delivered to a running worker");
  const id = (await post("local", "/api/runs", { function: "plan_trip", args: { city: "Porto" } })).json.id;
  const done = await waitStatus("local", id, "completed");
  check("non-durable run completes through the remote call", done.result.weather === "sunny in Porto" && done.durable === false
    && done.snapshots.length === 0 && done.waiting_on.length === 0);
  const pauseRefused = await post("local", `/api/runs/${id}/pause`);
  check("pause of a finished run is 409", pauseRefused.status === 409);
  const resumeRefused = await post("local", `/api/runs/${id}/resume`);
  check("resume of a finished run is 409", resumeRefused.status === 409);
  const resumeBadSite = await post("local", `/api/runs/${id}/resume`, { site: "mars" });
  check("resume of a finished run with an unknown site is 400: the site name is checked before the status", resumeBadSite.status === 400
    && resumeBadSite.json.error.includes("mars"), resumeBadSite.text);
}

async function migration(local, cloud, cloud2) {
  section("migration: pause on local, resume with {site: cloud}");
  const id = (await post("local", "/api/runs", { function: "durable_plan_trip", args: { city: "Faro" } })).json.id;
  await local.waitEvent("first log", (e) => e.type === "log" && e.run === id);
  await post("local", `/api/runs/${id}/pause`);
  const paused = await waitStatus("local", id, "paused");
  const state = (await get("local", `/api/runs/${id}/snapshots/${paused.snapshots.at(-1).n}/state`)).json;
  check("paused inside the loop (parked in sleep)", state.threads[0].parked.kind === "sysop");

  const bad = await post("local", `/api/runs/${id}/resume`, { site: "mars" });
  check("resume with an unknown site is 400 {error}, and the run stays paused", bad.status === 400 && typeof bad.json.error === "string"
    && (await get("local", `/api/runs/${id}`)).json.status === "paused", bad.text);
  for (const notAName of [5, true, ["cloud"], { name: "cloud" }]) {
    const r = await post("local", `/api/runs/${id}/resume`, { site: notAName });
    check(`resume with site ${JSON.stringify(notAName)} is 400, and the run is not resumed here`, r.status === 400 && typeof r.json.error === "string"
      && (await get("local", `/api/runs/${id}`)).json.status === "paused", r.text);
  }
  const moved = await post("local", `/api/runs/${id}/resume`, { site: "cloud" });
  check("resume {site: cloud} marks the run migrated", moved.status === 200 && moved.json.status === "migrated", moved.text);
  check("migrated_out event on local", !!(await local.waitEvent("migrated_out", (e) => e.type === "migrated_out" && e.run === id && e.to_site === "cloud")));
  check("migrated_in event on cloud", !!(await cloud.waitEvent("migrated_in", (e) => e.type === "migrated_in" && e.run === id && e.from_site === "local")));
  const onCloud = (await get("cloud", `/api/runs/${id}`)).json;
  check("cloud has the run with the same id, origin, and the next segment", onCloud.site === "cloud" && onCloud.origin.site === "local"
    && onCloud.origin.run === id && onCloud.segment === 2 && onCloud.snapshots.length === 1
    && onCloud.snapshots[0].snapshot_path.includes(`/${SITES.cloud.runs.replace(/^\.baml\//, "")}/`) && fs.existsSync(onCloud.snapshots[0].snapshot_path));
  const cloudState = await get("cloud", `/api/runs/${id}/snapshots/${onCloud.snapshots[0].n}/state`);
  check("state dump travelled with the snapshot", cloudState.status === 200 && cloudState.json.run === id);
  check("the destination serves the imported StateDump under the same n", onCloud.snapshots[0].n === paused.snapshots.at(-1).n
    && JSON.stringify(cloudState.json) === JSON.stringify(state), `n ${onCloud.snapshots[0].n} vs ${paused.snapshots.at(-1).n}`);
  const dispatched = await cloud.waitEvent("remote_dispatched from cloud", (e) => e.type === "remote_dispatched" && e.run === id);
  check("placement: the remote call of a parent that migrated to cloud runs on cloud2", dispatched.child_site === "cloud2" && dispatched.site === "cloud");
  const child = await waitStatus("cloud2", dispatched.child_run, "completed");
  check("the child on cloud2 names cloud as the site to call back", child.site === "cloud2" && child.parent.site === "cloud" && child.parent.run === id
    && !!cloud2.find((e) => e.type === "log" && e.run === child.id && e.site === "cloud2"));
  const returned = await cloud.waitEvent("remote_returned on cloud", (e) => e.type === "remote_returned" && e.run === id);
  check("the result returns to cloud", returned.ok === true && returned.child_site === "cloud2" && returned.child_run === child.id && returned.site === "cloud"
    && !local.find((e) => e.type === "remote_returned" && e.run === id));
  const done = await waitStatus("cloud", id, "completed");
  check("migrated run completed on cloud", done.result.weather === "sunny in Faro" && done.result.ideas.length === 3);
  await waitFor("result_delivered on the cloud2 child", async () => (await get("cloud2", `/api/runs/${child.id}`)).json.result_delivered);
  const cloudStates = await Promise.all(done.snapshots.map((s) => get("cloud", `/api/runs/${id}/snapshots/${s.n}/state`)));
  check("snapshots written on the destination continue the numbering, and each has a state dump",
    done.snapshots.length > 1 && done.snapshots.every((s, i) => i === 0 || s.n === done.snapshots[i - 1].n + 1)
    && done.snapshots.every((s) => s.snapshot_path.endsWith(`/snap-${s.n}.bamlsnap`))
    && cloudStates.every((r) => r.status === 200 && r.json.run === id), JSON.stringify(done.snapshots.map((s) => s.n)));
  const still = (await get("local", `/api/runs/${id}`)).json;
  check("the local record stays migrated and names the new site in migrated_to", still.status === "migrated" && still.pid === null && still.migrated_to === "cloud"
    && done.migrated_to === null, JSON.stringify({ local: still.migrated_to, cloud: done.migrated_to }));
  const again = await post("local", `/api/runs/${id}/resume`);
  check("a migrated run cannot be resumed on the old site", again.status === 409);
  const againBad = await post("local", `/api/runs/${id}/resume`, { site: "mars" });
  const againOther = await post("local", `/api/runs/${id}/resume`, { site: "cloud2" });
  check("on a migrated record an unknown site is 400 and a known site is 409", againBad.status === 400 && againOther.status === 409, `${againBad.status} ${againOther.status}`);
}

async function migrationWhileWaiting(local, cloud) {
  section("migration while waiting on a remote call: the result follows the run");
  const id = (await post("local", "/api/runs", { function: "durable_plan_trip", args: { city: "Braga" } })).json.id;
  const dispatched = await local.waitEvent("remote_dispatched", (e) => e.type === "remote_dispatched" && e.run === id);
  await post("local", `/api/runs/${id}/pause`);
  await waitStatus("local", id, "paused");
  const moved = await post("local", `/api/runs/${id}/resume`, { site: "cloud" });
  check("migrated while the child still runs", moved.json.status === "migrated");
  // The child (on cloud) posts its result to local, which forwards it to cloud.
  const done = await waitStatus("cloud", id, "completed");
  check("run completed on cloud with the forwarded result", done.result.weather === "sunny in Braga", JSON.stringify(done.result));
  check("remote_returned was emitted by cloud", !!cloud.find((e) => e.type === "remote_returned" && e.run === id && e.child_run === dispatched.child_run));
}

async function migrationChain(local, cloud, cloud2) {
  section("migration chain local -> cloud -> cloud2: a late remote result follows migrated_to to a run without a process");
  // The child sleeps 4 real seconds (mock-only argument), so both hops finish before it does.
  const id = (await post("local", "/api/runs", { function: "durable_plan_trip", args: { city: "Chaves", mock_remote_sleep_ms: 4000 } })).json.id;
  const dispatched = await local.waitEvent("remote_dispatched", (e) => e.type === "remote_dispatched" && e.run === id);
  check("the child of the local parent runs on cloud", dispatched.child_site === "cloud");
  await post("local", `/api/runs/${id}/pause`);
  await waitStatus("local", id, "paused");
  const hop1 = await post("local", `/api/runs/${id}/resume`, { site: "cloud" });
  check("first hop: local -> cloud", hop1.status === 200 && hop1.json.status === "migrated" && hop1.json.migrated_to === "cloud", hop1.text);
  const pause2 = await post("cloud", `/api/runs/${id}/pause`);
  check("the run can be paused on cloud right after the import", pause2.status === 200, pause2.text);
  const onCloud = await waitStatus("cloud", id, "paused");
  // An import must not replace a record that is not `migrated` (contract 7.4).
  const ownSnap = onCloud.snapshots.at(-1);
  const clash = await post("cloud", "/api/runs/import", {
    run: { ...onCloud, site: "local", segment: 99 },
    snapshot_base64: fs.readFileSync(ownSnap.snapshot_path).toString("base64"),
    state: JSON.parse(fs.readFileSync(ownSnap.state_path, "utf8")),
  });
  const afterClash = (await get("cloud", `/api/runs/${id}`)).json;
  check("import of a run that exists here with a status other than migrated is 409, and the run is unchanged", clash.status === 409 && typeof clash.json.error === "string"
    && afterClash.status === "paused" && afterClash.segment === onCloud.segment && afterClash.pid === null
    && afterClash.updated_ts === onCloud.updated_ts && afterClash.site === "cloud", `${clash.status} ${clash.text}`);
  const hop2 = await post("cloud", `/api/runs/${id}/resume`, { site: "cloud2" });
  check("second hop: cloud -> cloud2", hop2.status === 200 && hop2.json.status === "migrated" && hop2.json.migrated_to === "cloud2" && onCloud.segment === 2, hop2.text);
  check("migrated_out and migrated_in carry the site names of the second hop",
    !!(await cloud.waitEvent("migrated_out", (e) => e.type === "migrated_out" && e.run === id && e.to_site === "cloud2"))
    && !!(await cloud2.waitEvent("migrated_in", (e) => e.type === "migrated_in" && e.run === id && e.from_site === "cloud")));
  await post("cloud2", `/api/runs/${id}/pause`);
  const onCloud2 = await waitStatus("cloud2", id, "paused");
  const childNow = (await get("cloud", `/api/runs/${dispatched.child_run}`)).json;
  check("the run is paused on cloud2 (segment 3, no process) while the child still runs on cloud", onCloud2.pid === null && onCloud2.segment === 3
    && onCloud2.origin.site === "cloud" && onCloud2.remote_results.length === 0 && onCloud2.waiting_on.length === 1 && childNow.status === "running",
    JSON.stringify({ segment: onCloud2.segment, results: onCloud2.remote_results.length, child: childNow.status }));

  // The child posts its result to local (parent.site). Local forwards it to
  // cloud, and cloud forwards it to cloud2, where the run has no process.
  await waitStatus("cloud", dispatched.child_run, "completed");
  const stored = await waitFor("stored result on cloud2", async () => {
    const r = (await get("cloud2", `/api/runs/${id}`)).json;
    return r.remote_results.length === 1 ? r : null;
  });
  check("the result was forwarded along migrated_to and is stored on the paused run on cloud2", stored.status === "paused" && stored.pid === null
    && stored.remote_results[0].value === "sunny in Chaves" && stored.remote_results[0].acked === false && stored.waiting_on.length === 0);
  const returned = await cloud2.waitEvent("remote_returned on cloud2", (e) => e.type === "remote_returned" && e.run === id);
  check("remote_returned is emitted by cloud2 only, with the child's site", returned.ok === true && returned.child_site === "cloud" && returned.child_run === dispatched.child_run
    && !local.find((e) => e.type === "remote_returned" && e.run === id) && !cloud.find((e) => e.type === "remote_returned" && e.run === id));
  await waitFor("result_delivered on the child", async () => (await get("cloud", `/api/runs/${dispatched.child_run}`)).json.result_delivered);
  const left = [(await get("local", `/api/runs/${id}`)).json, (await get("cloud", `/api/runs/${id}`)).json];
  check("the records that stayed behind form the chain local -> cloud -> cloud2", left[0].status === "migrated" && left[0].migrated_to === "cloud"
    && left[1].status === "migrated" && left[1].migrated_to === "cloud2" && left[1].origin.site === "local"
    && left.every((r) => r.remote_results.length === 0), JSON.stringify(left.map((r) => [r.site, r.status, r.migrated_to])));

  await post("cloud2", `/api/runs/${id}/resume`);
  const done = await waitStatus("cloud2", id, "completed");
  check("the run completes on cloud2 with the forwarded result", done.result.weather === "sunny in Chaves" && done.result.ideas.length === 3
    && done.segment === 4 && done.remote_results[0].acked === true, JSON.stringify(done.result));
  const hellos = [...local.events, ...cloud.events, ...cloud2.events].filter((e) => e.type === "hello" && e.run === id).map((e) => `${e.segment}:${e.site}`).sort();
  check("segments ran on local, cloud, cloud2, cloud2", JSON.stringify(hellos) === JSON.stringify(["1:local", "2:cloud", "3:cloud2", "4:cloud2"]), JSON.stringify(hellos));
  check("the remote call was dispatched once", [...local.events, ...cloud.events, ...cloud2.events].filter((e) => e.type === "remote_dispatched" && e.run === id).length === 1);

  // The HTTP answer of each hop names the next site.
  const viaLocal = await post("local", `/api/runs/${id}/remote_result`, { call_id: `${id}-c99`, value: "late" });
  const viaCloud = await post("cloud", `/api/runs/${id}/remote_result`, { call_id: `${id}-c98`, value: "late" });
  const end = (await get("cloud2", `/api/runs/${id}`)).json;
  check("POST remote_result on a migrated record answers {ok, forwarded_to: <migrated_to>} and the result reaches the last site",
    viaLocal.status === 200 && viaLocal.json.forwarded_to === "cloud" && viaCloud.status === 200 && viaCloud.json.forwarded_to === "cloud2"
    && end.remote_results.some((r) => r.call_id === `${id}-c99`) && end.remote_results.some((r) => r.call_id === `${id}-c98`), `${viaLocal.text} ${viaCloud.text}`);

  // A run that comes back to a site it left: cloud2 -> local replaces the migrated record on local.
  const back = (await post("cloud2", "/api/runs", { function: "durable_plan_trip", args: { city: "Mafra" } })).json.id;
  await cloud2.waitEvent("first log", (e) => e.type === "log" && e.run === back);
  await post("cloud2", `/api/runs/${back}/pause`);
  await waitStatus("cloud2", back, "paused");
  await post("cloud2", `/api/runs/${back}/resume`, { site: "local" });
  await post("local", `/api/runs/${back}/pause`);
  await waitStatus("local", back, "paused");
  const home = await post("local", `/api/runs/${back}/resume`, { site: "cloud2" });
  check("a run returns to a site that holds its migrated record (cloud2 -> local -> cloud2)", home.status === 200 && home.json.migrated_to === "cloud2", home.text);
  const backDone = await waitStatus("cloud2", back, "completed");
  check("it completes there, and its remote call went to cloud", backDone.result.weather === "sunny in Mafra" && backDone.migrated_to === null && backDone.origin.site === "local"
    && !!cloud2.find((e) => e.type === "remote_dispatched" && e.run === back && e.child_site === "cloud"));
}

async function cloud2CallsCloud(cloud, cloud2) {
  section("placement: a run on cloud2 calls into cloud");
  const started = await post("cloud2", "/api/runs", { function: "plan_trip", args: { city: "Nazare" } });
  const id = started.json.id;
  check("POST /api/runs on cloud2 starts a run there", started.status === 200 && started.json.site === "cloud2");
  const dispatched = await cloud2.waitEvent("remote_dispatched", (e) => e.type === "remote_dispatched" && e.run === id);
  check("remote_dispatched on cloud2 names cloud", dispatched.child_site === "cloud" && dispatched.site === "cloud2");
  const child = await waitStatus("cloud", dispatched.child_run, "completed");
  check("the child ran on cloud with parent.site cloud2", child.site === "cloud" && child.parent.site === "cloud2" && child.parent.run === id);
  const done = await waitStatus("cloud2", id, "completed");
  check("the parent on cloud2 completes with the result from cloud", done.result.weather === "sunny in Nazare" && done.waiting_on.length === 0);
  await waitFor("result_delivered on the child", async () => (await get("cloud", `/api/runs/${child.id}`)).json.result_delivered);
  check("remote_returned on cloud2", !!cloud2.find((e) => e.type === "remote_returned" && e.run === id && e.ok === true && e.child_site === "cloud"));
}

// Restarts cloud2 with a pool that holds only cloud2 itself.
async function noRemoteSite() {
  section("REMOTE_POOL with only the caller's site: the remote call fails with `no remote site available`");
  await stopServer("cloud2");
  await startServer("cloud2", { REMOTE_POOL: JSON.stringify(["cloud2"]) });
  const info = (await get("cloud2", "/api/info")).json;
  check("info.remote_pool shows the configured pool", JSON.stringify(info.remote_pool) === JSON.stringify(["cloud2"]), JSON.stringify(info.remote_pool));
  const stream = new Stream("cloud2");
  await stream.open();
  const before = (await Promise.all(SITE_NAMES.map((site) => get(site, "/api/runs")))).map((r) => r.json.length);
  const plain = (await post("cloud2", "/api/runs", { function: "plan_trip", args: { city: "Sagres" } })).json.id;
  const durable = (await post("cloud2", "/api/runs", { function: "durable_plan_trip", args: { city: "Sagres" } })).json.id;
  const [a, b] = await Promise.all([waitStatus("cloud2", plain, ["failed", "completed", "lost"]), waitStatus("cloud2", durable, ["failed", "completed", "lost"])]);
  check("the non-durable caller fails instead of hanging, and the error names the cause", a.status === "failed" && a.error.includes("no remote site available"), `${a.status} ${a.error}`);
  check("the durable caller fails the same way", b.status === "failed" && b.error.includes("no remote site available"), `${b.status} ${b.error}`);
  const returned = stream.find((e) => e.type === "remote_returned" && e.run === plain);
  check("the error reached the worker as a remote_result: remote_returned has ok false, and the worker acknowledged it", returned && returned.ok === false
    && !!stream.find((e) => e.type === "remote_result_received" && e.run === plain)
    && a.remote_results.length === 1 && a.remote_results[0].error === "no remote site available" && a.remote_results[0].acked === true, JSON.stringify(a.remote_results));
  const after = (await Promise.all(SITE_NAMES.map((site) => get(site, "/api/runs")))).map((r) => r.json.length);
  check("no remote_dispatched event, and no child run on any site", !stream.find((e) => e.type === "remote_dispatched")
    && after[0] === before[0] && after[1] === before[1] && after[2] === before[2] + 2, JSON.stringify({ before, after }));
  stream.close();
}

// A run that is paused while it waits on its remote call, with the child still running.
async function pausedWhileWaiting(city) {
  const id = (await post("local", "/api/runs", { function: "durable_plan_trip", args: { city } })).json.id;
  const waiting = await waitFor(`remote call of ${id}`, async () => (await get("local", `/api/runs/${id}`)).json.waiting_on[0]);
  await post("local", `/api/runs/${id}/pause`);
  await waitStatus("local", id, "paused");
  return { id, child: waiting.child_run };
}

async function forkAndMigration() {
  section("fork while waiting on a remote call, then migration: the late result reaches both sites");
  // The fork migrates, the source stays.
  const a = await pausedWhileWaiting("Viseu");
  const forkA = (await post("local", `/api/runs/${a.id}/fork`)).json;
  check("a fork of the latest snapshot waits on the same call", forkA.waiting_on.length === 1 && forkA.waiting_on[0].call_id === `${a.id}-c1`);
  const movedA = await post("local", `/api/runs/${forkA.id}/resume`, { site: "cloud2" });
  const childA = (await get("cloud", `/api/runs/${a.child}`)).json.status;
  check("the fork migrated to cloud2 while the child still runs on cloud", movedA.json.status === "migrated" && movedA.json.migrated_to === "cloud2"
    && childA === "running", `${movedA.json.status} ${movedA.json.migrated_to} ${childA}`);
  const doneA = await waitStatus("cloud2", forkA.id, "completed");
  check("the migrated fork receives the late result on cloud2 (local forwards it to migrated_to)", doneA.result.weather === "sunny in Viseu"
    && doneA.origin.site === "local" && doneA.forked_from.run === a.id);
  const srcA = await waitFor("stored result of the source", async () => {
    const r = (await get("local", `/api/runs/${a.id}`)).json;
    return r.remote_results.length === 1 ? r : null;
  });
  check("the source on local stores the same result", srcA.status === "paused" && srcA.remote_results[0].value === "sunny in Viseu");

  // The source migrates, the fork stays.
  const b = await pausedWhileWaiting("Tavira");
  const forkB = (await post("local", `/api/runs/${b.id}/fork`)).json;
  const movedB = await post("local", `/api/runs/${b.id}/resume`, { site: "cloud" });
  check("the source migrated while the child still runs", movedB.json.status === "migrated");
  const doneB = await waitStatus("cloud", b.id, "completed");
  check("the migrated source completes on cloud", doneB.result.weather === "sunny in Tavira");
  const storedB = await waitFor("stored result of the fork", async () => {
    const r = (await get("local", `/api/runs/${forkB.id}`)).json;
    return r.remote_results.length === 1 ? r : null;
  });
  check("the fork that stayed on local stores the late result", storedB.status === "paused" && storedB.waiting_on.length === 0);
  await post("local", `/api/runs/${forkB.id}/resume`);
  const forkDone = await waitStatus("local", forkB.id, "completed");
  check("and completes after a resume", forkDone.result.weather === "sunny in Tavira");
}

async function forkScenario(local) {
  section("fork");
  const id = (await post("local", "/api/runs", { function: "durable_plan_trip", args: { city: "Evora" } })).json.id;
  await local.waitEvent("second day", (e) => e.type === "log" && e.run === id && e.text === "planning day 2");
  await post("local", `/api/runs/${id}/pause`);
  const paused = await waitStatus("local", id, "paused");
  const forked = await post("local", `/api/runs/${id}/fork`);
  const fid = forked.json.id;
  check("POST fork returns a new paused run", forked.status === 200 && fid !== id && forked.json.status === "paused"
    && forked.json.snapshots.length === 1 && forked.json.forked_from.run === id && forked.json.function === "durable_plan_trip"
    && fs.existsSync(forked.json.snapshots[0].snapshot_path) && forked.json.snapshots[0].snapshot_path.includes(fid));
  const forkedEv = await local.waitEvent("forked", (e) => e.type === "forked" && e.run === fid);
  const forkEvents = (await get("local", `/api/runs/${fid}/events`)).json;
  check("forked event on the stream and in the fork's events.jsonl", forkedEv.from_run === id && forkedEv.n === forked.json.forked_from.n
    && forkedEv.site === "local" && forkEvents.some((e) => e.type === "forked" && e.from_run === id));
  const forkOld = await post("local", `/api/runs/${id}/fork`, { n: 1 });
  check("fork {n: 1} copies the first snapshot", forkOld.status === 200 && forkOld.json.snapshots[0].n === 1 && forkOld.json.forked_from.n === 1);
  const forkBad = await post("local", `/api/runs/${id}/fork`, { n: 42 });
  check("fork of an unknown snapshot is 404", forkBad.status === 404);
  const fstate = await get("local", `/api/runs/${fid}/snapshots/${forked.json.snapshots[0].n}/state`);
  check("fork has its own state dump", fstate.status === 200);
  await post("local", `/api/runs/${id}/resume`);
  await post("local", `/api/runs/${fid}/resume`);
  await post("local", `/api/runs/${forkOld.json.id}/resume`);
  const [a, b, c] = await Promise.all([waitStatus("local", id, "completed"), waitStatus("local", fid, "completed"), waitStatus("local", forkOld.json.id, "completed")]);
  check("source and both forks complete independently", a.result.ideas.length === 3 && b.result.ideas.length === 3 && c.result.ideas.length === 3
    && b.result.weather === "sunny in Evora" && c.result.weather === "sunny in Evora");
  check("forks ran in their own processes", new Set(local.events.filter((e) => e.type === "hello" && [id, fid, forkOld.json.id].includes(e.run)).map((e) => e.pid)).size === 4);
  const list = (await get("local", "/api/runs")).json;
  check("GET /api/runs is newest first", list.length >= 3 && list.every((r, i) => i === 0 || list[i - 1].created_ts >= r.created_ts)
    && list.findIndex((r) => r.id === fid) < list.findIndex((r) => r.id === id));
  return paused;
}

async function killScenarios(local) {
  section("kill: durable run with a snapshot becomes paused, other runs become lost");
  const durable = (await post("local", "/api/runs", { function: "durable_plan_trip", args: { city: "Sintra" } })).json.id;
  const plain = (await post("local", "/api/runs", { function: "plan_trip", args: { city: "Sintra" } })).json.id;
  await local.waitEvent("first automatic snapshot", (e) => e.type === "snapshot" && e.run === durable);
  await local.waitEvent("plain run day 2", (e) => e.type === "log" && e.run === plain && e.text === "planning day 2");
  const dpid = (await get("local", `/api/runs/${durable}`)).json.pid;
  const ppid = (await get("local", `/api/runs/${plain}`)).json.pid;
  const pauseRefused = await post("local", `/api/runs/${plain}/pause`);
  check("pause of a non-durable run is 409", pauseRefused.status === 409 && !!pauseRefused.json.error);

  const k1 = await post("local", `/api/runs/${durable}/kill`);
  check("kill of a durable run with a snapshot returns paused", k1.status === 200 && k1.json.status === "paused" && k1.json.pid === null, k1.text);
  const k2 = await post("local", `/api/runs/${plain}/kill`);
  check("kill of a non-durable run returns lost", k2.status === 200 && k2.json.status === "lost" && k2.json.pid === null && !!k2.json.error, k2.text);
  check("both processes are gone", !pidAlive(dpid) && !pidAlive(ppid));
  const exitEv = local.find((e) => e.type === "worker_exit" && e.run === plain);
  check("worker_exit reports the signal", exitEv && exitEv.exit_code !== 0 && exitEv.signal !== null, JSON.stringify(exitEv));
  const k3 = await post("local", `/api/runs/${plain}/kill`);
  check("kill without a process is 409", k3.status === 409);

  await post("local", `/api/runs/${durable}/resume`);
  const done = await waitStatus("local", durable, "completed");
  check("killed durable run resumes from its latest snapshot and completes", done.result.ideas.length === 3 && done.segment === 2);

  const fresh = (await post("local", "/api/runs", { function: "durable_plan_trip", args: { city: "Tomar" } })).json.id;
  await waitStatus("local", fresh, "running");
  const k4 = await post("local", `/api/runs/${fresh}/kill`);
  check("kill of a durable run without a snapshot returns lost", k4.json.status === "lost", k4.text);
}

async function recoveryWithStoredResult(local, cloud) {
  section("recovery from a snapshot that predates a remote call: the stored result answers the repeated call");
  const id = (await post("local", "/api/runs", { function: "durable_plan_trip", args: { city: "Lagos" } })).json.id;
  const dispatched = await local.waitEvent("remote_dispatched", (e) => e.type === "remote_dispatched" && e.run === id);
  // The latest automatic snapshot was written before the call.
  const killed = await post("local", `/api/runs/${id}/kill`);
  check("killed while waiting on the remote call: paused", killed.json.status === "paused" && killed.json.snapshots.every((s) => s.automatic));
  await waitStatus("cloud", dispatched.child_run, "completed");
  await waitFor("stored result", async () => (await get("local", `/api/runs/${id}`)).json.remote_results.length === 1);
  await post("local", `/api/runs/${id}/resume`);
  const done = await waitStatus("local", id, "completed");
  check("the resumed run completes with the stored result", done.result.weather === "sunny in Lagos" && done.remote_results[0].acked === true);
  const calls = local.events.filter((e) => e.type === "remote_call" && e.run === id);
  check("the worker ignored the unknown --remote-result and repeated remote_call with the same call_id",
    calls.length === 2 && calls[0].call_id === calls[1].call_id && calls[1].segment === 2
    && !!local.find((e) => e.type === "log" && e.run === id && e.segment === 2 && e.stream === "worker_stderr" && e.text.includes("ignoring a result for the unknown call")));
  const children = (await get("cloud", "/api/runs")).json.filter((r) => r.parent && r.parent.call_id === `${id}-c1`);
  check("the call was dispatched once", children.length === 1
    && local.events.filter((e) => e.type === "remote_dispatched" && e.run === id).length === 1);
  check("remote_result_received once, in segment 2", local.events.filter((e) => e.type === "remote_result_received" && e.run === id).length === 1);
}

async function cancelScenario(local) {
  section("cancel");
  const id = (await post("local", "/api/runs", { function: "durable_plan_trip", args: { city: "Beja" } })).json.id;
  await waitStatus("local", id, "running");
  const r = await post("local", `/api/runs/${id}/cancel`);
  check("POST cancel returns the run", r.status === 200 && r.json.id === id);
  const done = await waitStatus("local", id, "cancelled");
  const exitEv = await local.waitEvent("worker_exit", (e) => e.type === "worker_exit" && e.run === id);
  check("run is cancelled (exit code 130)", done.pid === null && exitEv.exit_code === 130 && exitEv.status === "cancelled");
  const cancelledEv = local.find((e) => e.type === "cancelled" && e.run === id);
  check("the worker's cancelled event is forwarded before worker_exit", !!cancelledEv && cancelledEv.site === "local"
    && local.events.indexOf(cancelledEv) < local.events.indexOf(exitEv));
  check("stderr of the worker is forwarded as a log event",
    !!local.find((e) => e.type === "log" && e.run === id && e.stream === "worker_stderr" && e.text.includes("cancelled")));
}

async function blockedScenario(local) {
  section("pause answered only with blocked events: the status stays pausing and the record keeps the reason");
  const id = (await post("local", "/api/runs", { function: "durable_plan_trip", args: { city: "Elvas", mock_blocked: 8 } })).json.id;
  await local.waitEvent("first log", (e) => e.type === "log" && e.run === id);
  const pausing = await post("local", `/api/runs/${id}/pause`);
  check("pause returns pausing with blocked: null", pausing.json.status === "pausing" && pausing.json.blocked === null);
  const during = await waitFor("two blocked attempts on the record", async () => {
    const r = (await get("local", `/api/runs/${id}`)).json;
    return r.blocked && r.blocked.attempts >= 2 ? r : null;
  });
  check("the record stays pausing and carries the latest blocked reason", during.status === "pausing" && during.pid !== null
    && typeof during.blocked.reason === "string" && during.blocked.reason.length > 0 && Array.isArray(during.blocked.path)
    && during.blocked.path.length > 0 && typeof during.blocked.ts === "number", JSON.stringify(during.blocked));
  const again = await post("local", `/api/runs/${id}/pause`);
  check("a second pause while pausing is 409", again.status === 409);
  const paused = await waitStatus("local", id, "paused");
  check("blocked and later paused for one pause request: the run is paused and blocked is cleared", paused.blocked === null
    && paused.snapshots.at(-1).stats.blocked_attempts === 8 && paused.snapshots.at(-1).automatic === false
    && local.events.filter((e) => e.type === "blocked" && e.run === id).length === 8);
  const runEvents = local.events.filter((e) => e.type === "run" && e.run.id === id && e.run.status === "pausing" && e.run.blocked);
  check("run events carry the blocked reason while pausing", runEvents.length >= 2 && runEvents.at(-1).run.blocked.attempts === 8);
  await post("local", `/api/runs/${id}/resume`);
  const done = await waitStatus("local", id, "completed");
  check("the run completes after the resume", done.result.ideas.length === 3 && done.blocked === null);
}

async function parallelScenario(local, cloud) {
  section("durable_plan_trip_parallel: spawn, pause with a pending remote call, resume");
  const id = (await post("local", "/api/runs", { function: "durable_plan_trip_parallel", args: { city: "Aveiro", mock_blocked: 2 } })).json.id;
  const started = await local.waitEvent("thread_started for the spawn", (e) => e.type === "thread_started" && e.run === id && e.parent_thread !== null);
  const call = await local.waitEvent("remote_call on the spawned thread", (e) => e.type === "remote_call" && e.run === id);
  check("remote_call comes from the spawned thread", call.thread === started.thread && call.args.city === "Aveiro");
  const dispatched = await local.waitEvent("remote_dispatched", (e) => e.type === "remote_dispatched" && e.run === id);
  await post("local", `/api/runs/${id}/pause`);
  const paused = await waitStatus("local", id, "paused");
  const blocked = local.events.filter((e) => e.type === "blocked" && e.run === id);
  check("blocked events are forwarded before the pause succeeds", blocked.length === 2 && blocked[0].path.length > 0 && typeof blocked[0].reason === "string"
    && paused.snapshots.at(-1).stats.blocked_attempts === 2);
  const state = (await get("local", `/api/runs/${id}/snapshots/${paused.snapshots.at(-1).n}/state`)).json;
  check("state dump shows two threads", state.threads.length === 2 && state.threads[1].parked.kind === "remote_call");
  await waitStatus("cloud", dispatched.child_run, "completed");
  await local.waitEvent("remote_returned", (e) => e.type === "remote_returned" && e.run === id);
  await post("local", `/api/runs/${id}/resume`);
  const done = await waitStatus("local", id, "completed");
  check("parallel run completes after resume", done.result.weather === "sunny in Aveiro" && done.result.ideas.length === 3);
  check("thread_ended for the spawned thread", !!local.find((e) => e.type === "thread_ended" && e.run === id && e.thread === started.thread));
}

async function failureScenario(local, cloud) {
  section("failure: the remote child fails, the parent fails");
  const id = (await post("local", "/api/runs", { function: "plan_trip", args: { city: "Atlantis" } })).json.id;
  const dispatched = await local.waitEvent("remote_dispatched", (e) => e.type === "remote_dispatched" && e.run === id);
  const child = await waitStatus("cloud", dispatched.child_run, "failed");
  check("child failed on cloud", typeof child.error === "string" && child.error.includes("Atlantis"));
  check("failed event with a stack was forwarded", !!cloud.find((e) => e.type === "failed" && e.run === child.id && e.stack.length === 1));
  const returned = await local.waitEvent("remote_returned", (e) => e.type === "remote_returned" && e.run === id);
  check("remote_returned has ok: false", returned.ok === false);
  const parent = await waitStatus("local", id, "failed");
  check("parent failed with the remote error", parent.error.includes("Atlantis") && parent.result === null);
}

async function peerRoutes() {
  section("peer-to-peer routes called directly");
  const r = await post("cloud", "/api/remote/runs", { function: "remote_fetch_weather", args: { city: "Nowhere" }, parent: { site: "local", run: "r-ghost", call_id: "r-ghost-c1" } });
  check("POST /api/remote/runs starts a child run", r.status === 200 && r.json.parent.run === "r-ghost" && r.json.status === "starting");
  const rr = await post("local", "/api/runs/r-ghost/remote_result", { call_id: "r-ghost-c1", value: 1 });
  check("remote_result for an unknown run is 404", rr.status === 404);
  const noCall = await post("cloud", `/api/runs/${r.json.id}/remote_result`, {});
  check("remote_result without call_id is 400", noCall.status === 400);
  const ok = await post("cloud", `/api/runs/${r.json.id}/remote_result`, { call_id: "x-c9", error: "boom" });
  check("remote_result returns {ok: true}", ok.status === 200 && ok.json.ok === true);
  const badImport = await post("cloud", "/api/runs/import", { run: { id: "zzz" }, snapshot_base64: "", state: null });
  check("import of a malformed run is 400", badImport.status === 400);
  const record = (await get("cloud", `/api/runs/${r.json.id}`)).json;
  const escaping = await post("cloud", "/api/runs/import", {
    run: { ...record, id: "../../escaped", status: "paused", snapshots: [{ n: 1, snapshot_path: "/x", state_path: "/y", bytes: 1, ts: 1, stats: null }] },
    snapshot_base64: Buffer.from("{}").toString("base64"), state: null,
  });
  check("import of a run id outside [a-z0-9-]+ is 400", escaping.status === 400 && !fs.existsSync(path.join(SERVER_DIR, "escaped")), `status ${escaping.status}`);
  await post("cloud", `/api/runs/${r.json.id}/kill`);
}

async function restartScenario() {
  section("server restart: runs are reloaded from RUNS_DIR");
  const durable = (await post("local", "/api/runs", { function: "durable_plan_trip", args: { city: "Coimbra" } })).json.id;
  const plain = (await post("local", "/api/runs", { function: "plan_trip", args: { city: "Coimbra" } })).json.id;
  await waitFor("automatic snapshot", async () => (await get("local", `/api/runs/${durable}`)).json.snapshots.length > 0);
  const before = (await get("local", "/api/runs")).json;
  const pids = [(await get("local", `/api/runs/${durable}`)).json.pid, (await get("local", `/api/runs/${plain}`)).json.pid];
  // A third run: its parent is paused on local while its child still runs on cloud.
  const waiting = (await post("local", "/api/runs", { function: "durable_plan_trip", args: { city: "Guarda" } })).json.id;
  const childOf = await waitFor("remote call of the third run", async () => (await get("local", `/api/runs/${waiting}`)).json.waiting_on[0]);
  await post("local", `/api/runs/${waiting}/pause`);
  await waitStatus("local", waiting, "paused");
  await stopServer("local");
  await waitFor("workers to exit after the server stopped", () => pids.every((p) => !pidAlive(p)), 5000);
  check("workers exit when their site server stops", true);
  await startServer("local");
  const after = (await get("local", "/api/runs")).json;
  check("all runs are back, in the same order", after.length === before.length + 1 && before.every((r) => after.some((a) => a.id === r.id))
    && after.every((r, i) => i === 0 || after[i - 1].created_ts >= r.created_ts));
  const d = after.find((r) => r.id === durable);
  const p = after.find((r) => r.id === plain);
  check("a durable run that was running is paused after the restart", d.status === "paused" && d.pid === null, d.status);
  check("a non-durable run that was running is lost after the restart", p.status === "lost" && p.pid === null && !!p.error, p.status);
  const finished = after.filter((r) => r.status === "completed");
  check("finished runs keep their results", finished.length > 0 && finished.every((r) => r.result !== null));
  const stream = new Stream("local");
  await stream.open();
  check("init after the restart lists the reloaded runs", stream.events[0].type === "init" && stream.events[0].runs.length === after.length);
  await post("local", `/api/runs/${durable}/resume`);
  const done = await waitStatus("local", durable, "completed");
  check("the reloaded durable run resumes and completes", done.result.weather === "sunny in Coimbra" && done.result.ideas.length === 3);
  // The child finished while local was down. Cloud retries the delivery.
  const child = await waitStatus("cloud", childOf.child_run, "completed");
  check("the child completed while the parent's site was down", child.result === "sunny in Guarda");
  const parent = await waitFor("stored result after the redelivery", async () => {
    const r = (await get("local", `/api/runs/${waiting}`)).json;
    return r.remote_results.length === 1 ? r : null;
  }, 20000);
  check("cloud redelivered the result after local came back", parent.status === "paused" && parent.remote_results[0].value === "sunny in Guarda");
  await waitFor("result_delivered on the child", async () => (await get("cloud", `/api/runs/${childOf.child_run}`)).json.result_delivered);
  await post("local", `/api/runs/${waiting}/resume`);
  const finished2 = await waitStatus("local", waiting, "completed");
  check("the parent resumes after the restart and completes with the stored result", finished2.result.weather === "sunny in Guarda");
  stream.close();
}

// ------------------------------------------------------------- scripted site

// A site that this script plays itself. It listens on a port that the system
// assigns, and it is known only to the cloud2 server that `routingScene`
// starts. It answers the site-to-site routes with delays and statuses that a
// scene chooses, so that the scene can look at a site server while a dispatch,
// an import, or a forward is in flight.

class ScriptedSite {
  constructor() {
    this.requests = [];          // { path, body, at, status }
    this.dispatchDelayMs = 0;
    this.importDelayMs = 0;
    this.importStatus = 200;
    this.resultStatuses = new Map();   // run id -> statuses for the next requests
    this.resultDefault = new Map();    // run id -> status once the list is empty
    this.bounceTo = new Map();         // run id -> base URL: play a `migrated` record that names that site
    this.children = 0;
  }
  async start() {
    this.server = http.createServer((req, res) => this.handle(req, res));
    await new Promise((resolve) => this.server.listen(0, "127.0.0.1", resolve));
    this.base = `http://127.0.0.1:${this.server.address().port}`;
  }
  stop() {
    this.server.closeAllConnections?.();
    return new Promise((resolve) => this.server.close(resolve));
  }
  seen(pred) { return this.requests.filter(pred); }
  async handle(req, res) {
    let text = "";
    for await (const chunk of req) text += chunk;
    let body = null;
    try { body = text.length ? JSON.parse(text) : null; } catch { /* keep null */ }
    const entry = { path: req.url, body, at: Date.now(), status: null };
    this.requests.push(entry);
    const reply = (status, json) => {
      entry.status = status;
      res.writeHead(status, { "content-type": "application/json" });
      res.end(JSON.stringify(json));
    };
    const result = /^\/api\/runs\/([a-z0-9-]+)\/remote_result$/.exec(req.url);
    if (req.url === "/api/remote/runs") {
      await sleep(this.dispatchDelayMs);
      this.children += 1;
      return reply(200, { id: `r-scripted${this.children}`, site: "slow", status: "running" });
    }
    if (req.url === "/api/runs/import") {
      await sleep(this.importDelayMs);
      if (this.importStatus !== 200) return reply(this.importStatus, { error: "the scripted site refuses this import" });
      return reply(200, { ...body.run, site: "slow", status: "starting" });
    }
    if (result) {
      const id = result[1];
      const target = this.bounceTo.get(id);
      if (target) {
        // Forward like a site whose record of the run is `migrated`.
        const via = [...(body.via ?? []), "slow"];
        const back = await fetch(`${target}/api/runs/${id}/remote_result`, {
          method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ ...body, via }),
        });
        entry.bounceStatus = back.status;
        return back.ok ? reply(200, { ok: true, forwarded_to: "cloud2" }) : reply(502, { error: "forwarding failed" });
      }
      const list = this.resultStatuses.get(id) ?? [];
      const status = list.length > 0 ? list.shift() : (this.resultDefault.get(id) ?? 200);
      return status === 200 ? reply(200, { ok: true }) : reply(status, { error: `scripted status ${status}` });
    }
    return reply(req.url === "/api/info" ? 200 : 404, { site: "slow" });
  }
}

// The routing cases that need a request in flight. cloud2 is restarted with a
// registry that also names the scripted site `slow`, and with REMOTE_POOL
// ["slow"], so its remote calls and the migrations of this scene go there.
async function routingScene() {
  section("routing while a dispatch, an import, or a forward is in flight (scripted site `slow`)");
  const slow = new ScriptedSite();
  await slow.start();
  const env = { SITES: JSON.stringify({ ...REGISTRY, slow: slow.base }), REMOTE_POOL: JSON.stringify(["slow"]) };
  try {
    await stopServer("cloud2");
    await startServer("cloud2", env);
    let stream = new Stream("cloud2");
    await stream.open();
    const local = new Stream("local");
    await local.open();

    // 1. A dispatch that is in flight blocks the move to another site.
    slow.dispatchDelayMs = 2500;
    const id = (await post("cloud2", "/api/runs", { function: "durable_plan_trip", args: { city: "Tondela" } })).json.id;
    const call = await stream.waitEvent("remote_call", (e) => e.type === "remote_call" && e.run === id);
    await post("cloud2", `/api/runs/${id}/pause`);
    const pausedEarly = await waitStatus("cloud2", id, "paused");
    const early = await post("cloud2", `/api/runs/${id}/resume`, { site: "local" });
    const inFlight = !stream.find((e) => e.type === "remote_dispatched" && e.run === id);
    check("resume {site} is 409 while the dispatch of a remote call of the run is in flight, and the run stays paused", inFlight && pausedEarly.waiting_on.length === 0
      && early.status === 409 && early.json.error.includes("dispatch") && (await get("cloud2", `/api/runs/${id}`)).json.status === "paused", `in flight ${inFlight}, ${early.status} ${early.text}`);
    const dispatched = await stream.waitEvent("remote_dispatched", (e) => e.type === "remote_dispatched" && e.run === id);
    check("the child is recorded on the paused record once the other site answered", dispatched.child_site === "slow"
      && (await get("cloud2", `/api/runs/${id}`)).json.waiting_on.some((w) => w.call_id === call.call_id));

    // 2. While an import is in flight the record names its destination, a
    //    second resume is refused, and a result is not sent around in a loop.
    slow.importDelayMs = 2500;
    slow.importStatus = 500;
    const refusedMove = post("cloud2", `/api/runs/${id}/resume`, { site: "slow" });
    await waitFor("the import request at the scripted site", () => slow.seen((r) => r.path === "/api/runs/import").length === 1);
    const during = (await get("cloud2", `/api/runs/${id}`)).json;
    check("during the transfer the record is migrated and already names the destination in migrated_to", during.status === "migrated" && during.migrated_to === "slow", `${during.status} ${during.migrated_to}`);
    const second = await Promise.all([post("cloud2", `/api/runs/${id}/resume`, { site: "local" }), post("cloud2", `/api/runs/${id}/resume`)]);
    check("a second resume during the transfer is 409, to another site and in place", second[0].status === 409 && second[1].status === 409
      && !local.find((e) => e.type === "migrated_in" && e.run === id), `${second[0].status} ${second[1].status}`);
    slow.resultStatuses.set(id, [404]);
    const t0 = Date.now();
    const duringResult = await post("cloud2", `/api/runs/${id}/remote_result`, { call_id: `${id}-c77`, value: "early" });
    const sent = slow.seen((r) => r.path === `/api/runs/${id}/remote_result`);
    check("a result that arrives during the transfer goes to the destination only, with via, and is answered 502 so that the sender retries",
      duringResult.status === 502 && sent.length === 1 && JSON.stringify(sent[0].body.via) === JSON.stringify(["cloud2"]) && sent[0].body.value === "early"
      && Date.now() - t0 < 2000, `${duringResult.status} ${JSON.stringify(sent.map((r) => r.body))}`);
    const refused = await refusedMove;
    const back = (await get("cloud2", `/api/runs/${id}`)).json;
    check("a refused import answers 502, and the record is paused again without migrated_to", refused.status === 502 && back.status === "paused" && back.migrated_to === null
      && back.waiting_on.length === 1, `${refused.status} ${back.status} ${back.migrated_to}`);

    // 3. The run moves to local for real. The call is not dispatched again, and
    //    the result follows the run.
    slow.importDelayMs = 0;
    slow.importStatus = 200;
    const moved = await post("cloud2", `/api/runs/${id}/resume`, { site: "local" });
    check("the run moves to local after the dispatch finished", moved.status === 200 && moved.json.migrated_to === "local", moved.text);
    await local.waitEvent("the next segment on local", (e) => e.type === "hello" && e.run === id && e.mode === "resume");
    const delivered = await post("cloud2", `/api/runs/${id}/remote_result`, { call_id: call.call_id, value: "sunny in Tondela" });
    const done = await waitStatus("local", id, "completed");
    check("the call was dispatched once, and its result reaches the run on local through cloud2", delivered.status === 200 && delivered.json.forwarded_to === "local"
      && done.result.weather === "sunny in Tondela" && slow.seen((r) => r.path === "/api/remote/runs" && r.body.parent.call_id === call.call_id).length === 1
      && !local.find((e) => e.type === "remote_dispatched" && e.run === id), JSON.stringify(done.result));

    // 4. Two forks wait on the call of their source and migrate to `slow`.
    slow.dispatchDelayMs = 0;
    const src = (await post("cloud2", "/api/runs", { function: "durable_plan_trip", args: { city: "Lamego" } })).json.id;
    const waiting = await waitFor(`remote call of ${src}`, async () => (await get("cloud2", `/api/runs/${src}`)).json.waiting_on[0]);
    await post("cloud2", `/api/runs/${src}/pause`);
    await waitStatus("cloud2", src, "paused");
    const forkA = (await post("cloud2", `/api/runs/${src}/fork`)).json.id;
    const forkB = (await post("cloud2", `/api/runs/${src}/fork`)).json.id;
    const movedForks = await Promise.all([forkA, forkB].map((f) => post("cloud2", `/api/runs/${f}/resume`, { site: "slow" })));
    check("both forks migrated to the scripted site", movedForks.every((r) => r.status === 200 && r.json.status === "migrated" && r.json.migrated_to === "slow"));

    // Loop guard: `slow` plays a migrated record of fork A that names cloud2.
    slow.bounceTo.set(forkA, SITES.cloud2.base);
    const loop = await post("cloud2", `/api/runs/${forkA}/remote_result`, { call_id: `${forkA}-c88`, value: "loop" });
    const hops = slow.seen((r) => r.path === `/api/runs/${forkA}/remote_result` && r.body.call_id === `${forkA}-c88`);
    check("two migrated records that name each other: the result passes each site once and is answered 502", loop.status === 502 && hops.length === 1
      && hops[0].bounceStatus === 502 && JSON.stringify(hops[0].body.via) === JSON.stringify(["cloud2"]), `${loop.status} ${JSON.stringify(hops.map((h) => [h.body.via, h.bounceStatus]))}`);
    slow.bounceTo.delete(forkA);
    const before = slow.requests.length;
    const tooLong = await post("cloud2", `/api/runs/${forkA}/remote_result`, { call_id: `${forkA}-c89`, value: "x", via: ["a", "b", "c", "d"] });
    const visited = await post("cloud2", `/api/runs/${forkA}/remote_result`, { call_id: `${forkA}-c90`, value: "x", via: ["cloud2"] });
    check("a result whose via already names this site, or every site of the registry, is answered 502 without a forward", tooLong.status === 502 && visited.status === 502
      && slow.requests.length === before, `${tooLong.status} ${visited.status}`);

    // The source receives the result. Fork A's site answers 404 first (its
    // import is not done), fork B's site answers 503 until cloud2 restarts.
    slow.resultStatuses.set(forkA, [404]);
    slow.resultDefault.set(forkB, 503);
    const toSource = await post("cloud2", `/api/runs/${src}/remote_result`, { call_id: waiting.call_id, value: "sunny in Lamego" });
    check("the source accepts the result", toSource.status === 200 && toSource.json.ok === true && !("forwarded_to" in toSource.json));
    const forResult = (f) => slow.seen((r) => r.path === `/api/runs/${f}/remote_result` && r.body.call_id === waiting.call_id);
    await waitFor("the first forward to fork A", () => forResult(forkA).length >= 1 && forResult(forkA)[0].status === 404);
    const keptA = (await get("cloud2", `/api/runs/${forkA}`)).json;
    check("after a 404 from the fork's site the record that stayed behind still waits on the call and holds the result", keptA.waiting_on.length === 1
      && keptA.remote_results.some((r) => r.call_id === waiting.call_id && r.value === "sunny in Lamego"), JSON.stringify(keptA.waiting_on));
    const clearedA = await waitFor("the forward to fork A to succeed", async () => {
      const r = (await get("cloud2", `/api/runs/${forkA}`)).json;
      return r.waiting_on.length === 0 ? r : null;
    }, 10000);
    check("the forward is retried after the 404, and the waiting_on entry is removed once the fork's site accepted the result", forResult(forkA).length === 2
      && forResult(forkA)[1].status === 200 && forResult(forkA)[1].body.value === "sunny in Lamego"
      && JSON.stringify(forResult(forkA)[1].body.via) === JSON.stringify(["cloud2"]) && clearedA.status === "migrated", JSON.stringify(forResult(forkA).map((r) => r.status)));
    const again = await post("cloud2", `/api/runs/${src}/remote_result`, { call_id: waiting.call_id, value: "sunny in Lamego" });
    await sleep(300);
    check("a repeated delivery does not forward to fork A again", again.status === 200 && forResult(forkA).length === 2);

    await waitFor("two failed forwards to fork B", () => forResult(forkB).filter((r) => r.status === 503).length >= 2, 10000);
    const keptB = (await get("cloud2", `/api/runs/${forkB}`)).json;
    check("while the fork's site answers 503 the entry of fork B stays", keptB.waiting_on.length === 1 && keptB.status === "migrated");

    // A record whose migrated_to names no site of the registry. It is written
    // into the run store and loaded at the restart.
    const store = path.join(SERVER_DIR, SITES.cloud2.runs);
    const crafted = { ...keptB, id: "r-nowhere", migrated_to: "mars", waiting_on: [], remote_results: [] };
    fs.mkdirSync(path.join(store, "r-nowhere"), { recursive: true });
    fs.writeFileSync(path.join(store, "r-nowhere", "meta.json"), JSON.stringify(crafted, null, 2));

    // 5. Restart: the forward to fork B is taken up again from meta.json.
    stream.close();
    await stopServer("cloud2");
    const attemptsBefore = forResult(forkB).length;
    slow.resultDefault.delete(forkB);
    await startServer("cloud2", env);
    const clearedB = await waitFor("the forward to fork B after the restart", async () => {
      const r = (await get("cloud2", `/api/runs/${forkB}`)).json;
      return r.waiting_on.length === 0 ? r : null;
    }, 10000);
    check("after a restart the stored result is forwarded to fork B, and its waiting_on entry is removed", clearedB.status === "migrated"
      && forResult(forkB).length === attemptsBefore + 1 && forResult(forkB).at(-1).status === 200, `${attemptsBefore} -> ${forResult(forkB).length}`);

    // Fallback for an unusable migrated_to: every other site that the result
    // did not pass yet is asked once.
    slow.resultDefault.set("r-nowhere", 404);
    const viaSlow = await post("cloud2", "/api/runs/r-nowhere/remote_result", { call_id: "r-nowhere-c1", value: 1, via: ["slow"] });
    const asked1 = slow.seen((r) => r.path === "/api/runs/r-nowhere/remote_result").length;
    const viaNone = await post("cloud2", "/api/runs/r-nowhere/remote_result", { call_id: "r-nowhere-c2", value: 2 });
    const asked2 = slow.seen((r) => r.path === "/api/runs/r-nowhere/remote_result");
    check("migrated_to outside the registry: the fallback skips the sites in via, asks the others once, and answers 502 when none holds the run",
      viaSlow.status === 502 && asked1 === 0 && viaNone.status === 502 && asked2.length === 1
      && JSON.stringify(asked2[0].body.via) === JSON.stringify(["cloud2"]), `${viaSlow.status} ${asked1} ${viaNone.status} ${asked2.length}`);
    slow.resultDefault.set("r-nowhere", 200);
    const found = await post("cloud2", "/api/runs/r-nowhere/remote_result", { call_id: "r-nowhere-c3", value: 3 });
    check("the fallback answers with the site that accepted the result", found.status === 200 && found.json.forwarded_to === "slow", found.text);
    local.close();
  } finally {
    await slow.stop();
  }
}

// --------------------------------------------------------------------- main

async function main() {
  await claimStores();
  await Promise.all(SITE_NAMES.map((site) => startServer(site)));
  const t0 = Date.now();

  const local = new Stream("local");
  const cloud = new Stream("cloud");
  const cloud2 = new Stream("cloud2");
  await Promise.all([local.open(), cloud.open(), cloud2.open()]);
  section("SSE");
  check("content-type is text/event-stream", local.contentType === "text/event-stream");
  check("init is the first message on both streams", local.events[0].type === "init" && local.events[0].site === "local"
    && Array.isArray(local.events[0].runs) && cloud.events[0].type === "init" && cloud.events[0].site === "cloud"
    && cloud2.events[0].type === "init" && cloud2.events[0].site === "cloud2");

  await routesAndErrors();
  await centralScene(local, cloud);
  await liveDelivery();
  await migration(local, cloud, cloud2);
  await migrationWhileWaiting(local, cloud);
  await migrationChain(local, cloud, cloud2);
  await cloud2CallsCloud(cloud, cloud2);
  await forkScenario(local);
  await forkAndMigration();
  await killScenarios(local);
  await recoveryWithStoredResult(local, cloud);
  await cancelScenario(local);
  await blockedScenario(local);
  await parallelScenario(local, cloud);
  await failureScenario(local, cloud);
  await peerRoutes();

  section("SSE invariants");
  const elapsed = Date.now() - t0;
  if (elapsed < 16000) await sleep(16000 - elapsed);
  let gapOk = local.comments.length >= 1 && local.comments.every((c) => c.text === ": keepalive");
  let last = t0;
  for (const c of local.comments) { if (c.at - last > 15500) gapOk = false; last = c.at; }
  check(`keepalive comments at least every 15 s (${local.comments.length} seen)`, gapOk);
  check("every SSE event has type, ts, and the site of its stream", [local, cloud, cloud2].every((stream) => stream.events.every((e) => typeof e.type === "string" && typeof e.ts === "number" && e.site === stream.site)));
  const late = new Stream("local");
  await late.open();
  const runsNow = (await get("local", "/api/runs")).json;
  check("a new connection gets init with all runs, newest first", late.events[0].type === "init" && late.events[0].runs.length === runsNow.length
    && late.events[0].runs[0].id === runsNow[0].id);
  late.close();
  const seen = new Set([...local.events, ...cloud.events, ...cloud2.events].map((e) => e.type));
  const wanted = ["hello", "log", "position", "thread_started", "thread_ended", "remote_call", "remote_result_received", "pausing", "blocked", "paused", "resumed", "completed", "failed",
    "init", "run", "remote_dispatched", "remote_returned", "migrated_out", "migrated_in",
    // Contract section 7.
    "cancelled", "snapshot", "worker_exit", "forked"];
  check("every contract event type was seen on the streams", wanted.every((t) => seen.has(t)), wanted.filter((t) => !seen.has(t)).join(","));
  local.close();

  await restartScenario();
  cloud.close();
  cloud2.close();
  await noRemoteSite();
  await routingScene();
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
  // Stores that this run did not claim (a refused start) are left alone.
  releaseStores(process.env.KEEP !== "1");
}
console.log(`\n${passed} passed, ${failures.length} failed`);
for (const f of failures) console.log(`  failed: ${f}`);
process.exit(exitCode || (failures.length ? 1 : 0));
