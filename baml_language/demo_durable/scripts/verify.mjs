#!/usr/bin/env node
// End-to-end check of the three site servers with the mock worker.
//
//   node scripts/verify.mjs            (from demo_durable/)
//   MOCK_SPEED=4 node scripts/verify.mjs      faster simulated sleeps
//   KEEP=1 node scripts/verify.mjs            keep the run stores for inspection
//   PORT_BASE=<n>                             first port (default 18787)
//   RUNS_TAG=<tag>                            added to the run store names
//   ONLY=phase3 | chaos                       phase3: skip the scenes of contract sections 3 to 8
//                                             chaos: only the CHAOS scenes
//
// The script starts its own site servers: local on PORT_BASE, cloud on
// PORT_BASE + 1, and cloud2 on PORT_BASE + 2, with run stores in
// server/.baml/verify<RUNS_TAG>-<site>. The default PORT_BASE is 18787, not the
// 8787 of dev.sh, so the script never collides with a demo that is running. It
// refuses to start when one of its ports is in use. It runs every scenario,
// restarts the local server once and the cloud2 server once, and stops
// everything that it started at the end.

import { execFileSync, spawn } from "node:child_process";
import crypto from "node:crypto";
import fs from "node:fs";
import http from "node:http";
import net from "node:net";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { isDeepStrictEqual } from "node:util";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const SERVER_DIR = path.join(ROOT, "server");
const PORT_BASE = Number(process.env.PORT_BASE ?? "18787");
const RUNS_TAG = process.env.RUNS_TAG ?? "";
const ONLY = process.env.ONLY ?? "";
if (!["", "phase3", "chaos"].includes(ONLY)) throw new Error(`ONLY must be phase3 or chaos: ${ONLY}`);
if (!Number.isInteger(PORT_BASE) || PORT_BASE < 1 || PORT_BASE > 65533) throw new Error(`PORT_BASE is not a port number: ${process.env.PORT_BASE}`);
if (!/^[A-Za-z0-9_-]*$/.test(RUNS_TAG)) throw new Error(`RUNS_TAG may contain only letters, digits, '-' and '_': ${RUNS_TAG}`);
const SITE_NAMES = ["local", "cloud", "cloud2"];
const SITES = Object.fromEntries(SITE_NAMES.map((name, i) => [name, {
  port: PORT_BASE + i, base: `http://127.0.0.1:${PORT_BASE + i}`, runs: `.baml/verify${RUNS_TAG}-${name}`,
  programs: `.baml/verify-programs${RUNS_TAG}-${name}`,
}]));
// Passed to every worker as --sleep-suspend-ms. The loop sleeps of trip.baml
// (1500 ms) and the deadline of durable_deadline (2000 ms) stay in process, and
// a durable_nap of 3 seconds or more suspends the run. The mock compares the
// threshold with simulated time, so MOCK_SPEED does not change which sleeps
// suspend.
const SLEEP_SUSPEND_MS = 2500;
const MOCK_SPEED = Math.max(0.001, Number(process.env.MOCK_SPEED ?? "1") || 1);
// The speed of the workers that the servers start. The CHAOS scenes set it to 1.
let speed = MOCK_SPEED;
// A simulated duration of the mock in real milliseconds.
const real = (ms) => ms / speed;
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
  const { PORT: _port, REMOTE_POOL: _pool, CHAOS: _chaos, WORKER_LEGACY: _legacy, ...inherited } = process.env;
  const child = spawn("baml", ["run", "main", "--log", "warn"], {
    cwd: SERVER_DIR,
    env: {
      ...inherited,
      SITE: site, SITES: JSON.stringify(REGISTRY), RUNS_DIR: cfg.runs,
      PROGRAMS_DIR: cfg.programs, SLEEP_SUSPEND_MS: String(SLEEP_SUSPEND_MS),
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
  check("info.functions lists the demo functions that a run can start, and no helper",
    names === "durable_deadline,durable_fan_out,durable_nap,durable_plan_trip,durable_plan_trip_parallel,durable_race,durable_settled,plan_trip,remote_fetch_weather,remote_get_quote", names);
  const rgq = info.json.functions.find((f) => f.name === "remote_get_quote");
  check("a class-typed parameter is listed with its type", rgq.remote && rgq.params.length === 1 && rgq.params[0].name === "request" && rgq.params[0].type === "QuoteRequest");
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
  const quotes = (await get("local", "/api/source?file=baml_src/quotes.baml")).json.text.split("\n");
  check("mock positions of quotes.baml point at real lines", quotes[120].includes("println") && quotes[129].includes("sleep") && quotes[131].includes("throw QuoteUnavailable")
    && [158, 159, 160, 161].every((i) => quotes[i].includes("spawn { remote_get_quote(")) && quotes[163].includes("sleep") && quotes[165].includes("baml.future.all(")
    && quotes[192].includes("baml.future.race(") && quotes[205].includes("baml.future.all_settled(") && quotes[240].includes("with_timeout") && quotes[241].includes("remote_get_quote(")
    && quotes[252].includes("sleep") && quotes[253].includes("woke up"));
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

// The SSE streams of the three sites, by site name. Placement is round-robin
// (contract section 9.3), so a scene takes the site of a child from the
// `remote_dispatched` event and reads that site's stream.
const STREAMS = {};
const inPool = (site, caller) => DEFAULT_POOL.includes(site) && site !== caller;

async function centralScene(local) {
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
  const cs = dispatched.child_site;
  check("remote_dispatched event", inPool(cs, "local") && dispatched.function === "remote_fetch_weather"
    && dispatched.call_id === `${id}-c1` && dispatched.site === "local" && typeof dispatched.ts === "number");
  const childId = dispatched.child_run;
  const parentNow = (await get("local", `/api/runs/${id}`)).json;
  check("parent.waiting_on records the child", parentNow.waiting_on.length === 1 && parentNow.waiting_on[0].child_run === childId
    && parentNow.waiting_on[0].function === "remote_fetch_weather" && parentNow.waiting_on[0].child_site === cs);
  const child = (await get(cs, `/api/runs/${childId}`)).json;
  check("child run on the pool site has parent set", child.site === cs && child.parent.site === "local"
    && child.parent.run === id && child.parent.call_id === `${id}-c1` && child.durable === false);

  // Pause while the child is still running.
  const pausing = await post("local", `/api/runs/${id}/pause`);
  check("POST pause returns status pausing", pausing.status === 200 && pausing.json.status === "pausing");
  const paused = await waitStatus("local", id, "paused");
  const childDuringPause = (await get(cs, `/api/runs/${childId}`)).json;
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
  const childDone = await waitStatus(cs, childId, "completed");
  check("child completed on its site", childDone.result === "sunny in Lisbon");
  await waitFor("child result delivered", async () => (await get(cs, `/api/runs/${childId}`)).json.result_delivered);
  const returned = await local.waitEvent("remote_returned", (e) => e.type === "remote_returned" && e.run === id);
  check("remote_returned event", returned.ok === true && returned.child_run === childId && returned.child_site === cs && returned.call_id === `${id}-c1`);
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
  const childrenOn = {};
  for (const site of SITE_NAMES) childrenOn[site] = (await get(site, "/api/runs")).json.filter((r) => r.parent && r.parent.run === id).length;
  check("placement: a parent on local calls into one site of the pool, and no child ran on local or on the other pool site",
    childrenOn.local === 0 && childrenOn[cs] === 1 && childrenOn.cloud + childrenOn.cloud2 === 1, JSON.stringify(childrenOn));
  check("the stream of the child's site carried the child's events",
    !!STREAMS[cs].find((e) => e.type === "log" && e.run === childId && e.text.includes("looking up weather") && e.site === cs)
    && !!STREAMS[cs].find((e) => e.type === "run" && e.run.id === childId && e.run.status === "completed"));
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
  const cs = dispatched.child_site;
  check("the child of the local parent runs on a site of the pool", inPool(cs, "local"), cs);
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
  const childNow = (await get(cs, `/api/runs/${dispatched.child_run}`)).json;
  check("the run is paused on cloud2 (segment 3, no process) while the child still runs on its site", onCloud2.pid === null && onCloud2.segment === 3
    && onCloud2.origin.site === "cloud" && onCloud2.remote_results.length === 0 && onCloud2.waiting_on.length === 1 && childNow.status === "running",
    JSON.stringify({ segment: onCloud2.segment, results: onCloud2.remote_results.length, child: childNow.status }));

  // The child posts its result to local (parent.site). Local forwards it to
  // cloud, and cloud forwards it to cloud2, where the run has no process.
  await waitStatus(cs, dispatched.child_run, "completed");
  const stored = await waitFor("stored result on cloud2", async () => {
    const r = (await get("cloud2", `/api/runs/${id}`)).json;
    return r.remote_results.length === 1 ? r : null;
  });
  check("the result was forwarded along migrated_to and is stored on the paused run on cloud2", stored.status === "paused" && stored.pid === null
    && stored.remote_results[0].value === "sunny in Chaves" && stored.remote_results[0].acked === false && stored.waiting_on.length === 0);
  const returned = await cloud2.waitEvent("remote_returned on cloud2", (e) => e.type === "remote_returned" && e.run === id);
  check("remote_returned is emitted by cloud2 only, with the child's site", returned.ok === true && returned.child_site === cs && returned.child_run === dispatched.child_run
    && !local.find((e) => e.type === "remote_returned" && e.run === id) && !cloud.find((e) => e.type === "remote_returned" && e.run === id));
  await waitFor("result_delivered on the child", async () => (await get(cs, `/api/runs/${dispatched.child_run}`)).json.result_delivered);
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
  return { id, child: waiting.child_run, childSite: waiting.child_site };
}

async function forkAndMigration() {
  section("fork while waiting on a remote call, then migration: the late result reaches both sites");
  // The fork migrates, the source stays.
  const a = await pausedWhileWaiting("Viseu");
  const forkA = (await post("local", `/api/runs/${a.id}/fork`)).json;
  check("a fork of the latest snapshot waits on the same call", forkA.waiting_on.length === 1 && forkA.waiting_on[0].call_id === `${a.id}-c1`);
  const movedA = await post("local", `/api/runs/${forkA.id}/resume`, { site: "cloud2" });
  const childA = (await get(a.childSite, `/api/runs/${a.child}`)).json.status;
  check("the fork migrated to cloud2 while the child still runs on its site", movedA.json.status === "migrated" && movedA.json.migrated_to === "cloud2"
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
  await waitStatus(dispatched.child_site, dispatched.child_run, "completed");
  await waitFor("stored result", async () => (await get("local", `/api/runs/${id}`)).json.remote_results.length === 1);
  await post("local", `/api/runs/${id}/resume`);
  const done = await waitStatus("local", id, "completed");
  check("the resumed run completes with the stored result", done.result.weather === "sunny in Lagos" && done.remote_results[0].acked === true);
  const calls = local.events.filter((e) => e.type === "remote_call" && e.run === id);
  check("the worker ignored the unknown --remote-result and repeated remote_call with the same call_id",
    calls.length === 2 && calls[0].call_id === calls[1].call_id && calls[1].segment === 2
    && !!local.find((e) => e.type === "log" && e.run === id && e.segment === 2 && e.stream === "worker_stderr" && e.text.includes("ignoring a result for the unknown call")));
  const children = (await Promise.all(DEFAULT_POOL.map((site) => get(site, "/api/runs")))).flatMap((r) => r.json).filter((r) => r.parent && r.parent.call_id === `${id}-c1`);
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
  await waitStatus(dispatched.child_site, dispatched.child_run, "completed");
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
  const child = await waitStatus(dispatched.child_site, dispatched.child_run, "failed");
  check("child failed on its site", typeof child.error === "string" && child.error.includes("Atlantis"));
  check("failed event with a stack was forwarded", !!STREAMS[dispatched.child_site].find((e) => e.type === "failed" && e.run === child.id && e.stack.length === 1));
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
  const child = await waitStatus(childOf.child_site, childOf.child_run, "completed");
  check("the child completed while the parent's site was down", child.result === "sunny in Guarda");
  const parent = await waitFor("stored result after the redelivery", async () => {
    const r = (await get("local", `/api/runs/${waiting}`)).json;
    return r.remote_results.length === 1 ? r : null;
  }, 20000);
  check("cloud redelivered the result after local came back", parent.status === "paused" && parent.remote_results[0].value === "sunny in Guarda");
  await waitFor("result_delivered on the child", async () => (await get(childOf.child_site, `/api/runs/${childOf.child_run}`)).json.result_delivered);
  await post("local", `/api/runs/${waiting}/resume`);
  const finished2 = await waitStatus("local", waiting, "completed");
  check("the parent resumes after the restart and completes with the stored result", finished2.result.weather === "sunny in Guarda");
  stream.close();
}

// ------------------------------------------------- contract section 9 scenes
//
// Every scene takes a label that is added to its check names, so the same
// scene can run a second time under CHAOS. The scenes read the SSE streams
// from STREAMS, which main() replaces after it restarts the servers.

const RATES = { Flight: 420, Hotel: 135, Car: 48, Tour: 75 };
const VENDORS = { Flight: "Skyways", Hotel: "Casa Azul", Car: "Rodas", Tour: "Seven Hills Walks" };

// The values that program/baml_src/quotes.baml builds. A result that crossed
// two site servers, a run store, and a resume must still equal them.
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

/** The worker processes of a run that exist right now, from the process table. */
function workerPids(id) {
  const out = execFileSync("ps", ["-axo", "pid=,command="], { encoding: "utf8" });
  return out.split("\n").filter((l) => l.includes("mock-worker.mjs") && l.includes(` --run ${id} `)).map((l) => Number(l.trim().split(/\s+/)[0]));
}

/** The children of `parentId` on every site, by call id. */
async function childrenOf(parentId) {
  const lists = await Promise.all(SITE_NAMES.map((site) => get(site, "/api/runs")));
  return lists.flatMap((r) => r.json).filter((r) => r.parent && r.parent.run === parentId);
}

async function programRoutes() {
  section("program store: routes");
  const unknown = await get("cloud", `/api/programs/${"0".repeat(64)}`);
  check("GET /api/programs/:hash for a program that the site does not hold is 404 {error}", unknown.status === 404 && typeof unknown.json.error === "string", unknown.text);
  for (const bad of ["abc", "Z".repeat(64), `..%2F${"0".repeat(61)}`]) {
    const r = await get("cloud", `/api/programs/${bad}`);
    check(`GET /api/programs/${bad.slice(0, 12)}… is refused: a hash is 64 lowercase hex characters`, r.status === 400 || r.status === 404, `status ${r.status}`);
  }
  const info = (await get("cloud", "/api/info")).json;
  check("info names the program store of the site, the suspend threshold, and the CHAOS knobs", info.programs_dir.endsWith(SITES.cloud.programs.replace(/^\.\//, ""))
    && info.sleep_suspend_ms === String(SLEEP_SUSPEND_MS) && typeof info.chaos === "object", JSON.stringify({ p: info.programs_dir, s: info.sleep_suspend_ms }));
}

/** Runs the mock worker directly, with stdin held open, and returns its exit code and events. */
function runMock(args, env = {}) {
  return new Promise((resolve) => {
    const child = spawn("node", [path.join(ROOT, "scripts", "mock-worker.mjs"), ...args], { env: { ...process.env, ...env }, stdio: ["pipe", "pipe", "ignore"] });
    let out = "";
    child.stdout.on("data", (d) => { out += d; });
    child.on("exit", (code) => resolve({ code, events: out.split("\n").filter(Boolean).map((l) => JSON.parse(l)) }));
  });
}

// The rules of contract section 9.5 that the worker owns, checked on the mock
// without a site server.
async function mockStoreRules() {
  section("program store: the worker's rules, on the mock worker alone");
  const dir = fs.mkdtempSync(path.join(SERVER_DIR, ".baml", "verify-mock-store-"));
  try {
    const store = path.join(dir, "store");
    const common = (run) => ["--run", run, "--segment", "1", "--snapshot-dir", path.join(dir, run), "--program-store", store, "--sleep-suspend-ms", "0"];
    const nap = ["--start", "durable_nap", "--json-args", JSON.stringify({ seconds: 0 })];
    const compiled = await runMock(["--project", path.join(ROOT, "program"), ...common("r-a"), ...nap]);
    const hash = compiled.events[0].program_hash;
    const entry = path.join(store, hash.slice(0, 2), `${hash}.bamlprog`);
    check("a start from --project compiles, stores the program, and reports program_hash in hello", compiled.code === 0 && /^[0-9a-f]{64}$/.test(hash)
      && compiled.events[0].mock_program_source === "compile" && fs.existsSync(entry) && fs.readdirSync(path.dirname(entry)).length === 1);
    const byHash = await runMock([...common("r-b"), "--program-hash", hash, ...nap]);
    check("a start with --program-hash and without --project loads the program from the store", byHash.code === 0 && byHash.events[0].mock_program_source === "store"
      && byHash.events.at(-1).value === "slept 0 seconds");
    const absent = "f".repeat(64);
    const missing = await runMock([...common("r-c"), "--program-hash", absent, ...nap]);
    check("neither a store entry nor --project: failed with `program <hash> is not in the store`, exit code 1", missing.code === 1
      && missing.events.at(-1).type === "failed" && missing.events.at(-1).error === `program ${absent} is not in the store`, JSON.stringify(missing.events.at(-1)));
    const otherBuild = await runMock([...common("r-d"), "--program-hash", hash, ...nap], { MOCK_RUNTIME_BUILD: "mock-worker/2" });
    check("an entry that another runtime build wrote is not used: the worker treats it as missing", otherBuild.code === 1
      && otherBuild.events.at(-1).error === `program ${hash} is not in the store`, JSON.stringify(otherBuild.events.at(-1)));
    const bytes = fs.readFileSync(entry);
    bytes[bytes.length - 1] ^= 0xff;
    fs.writeFileSync(entry, bytes);
    const corrupt = await runMock([...common("r-e"), "--program-hash", hash, ...nap]);
    check("an entry whose bytes do not have the hash counts as missing", corrupt.code === 1 && corrupt.events.at(-1).error === `program ${hash} is not in the store`);
    const repaired = await runMock(["--project", path.join(ROOT, "program"), ...common("r-f"), "--program-hash", hash, ...nap]);
    check("with --project the worker compiles, checks that the hash matches, and stores the program again", repaired.code === 0
      && repaired.events[0].mock_program_source === "compile" && (await runMock([...common("r-g"), "--program-hash", hash, ...nap])).events[0].mock_program_source === "store");
    const wrongProject = await runMock(["--project", path.join(ROOT, "server"), ...common("r-h"), "--program-hash", absent, ...nap]);
    check("a --project that compiles to another hash is refused", wrongProject.code === 1 && wrongProject.events.at(-1).error.includes("different program"), JSON.stringify(wrongProject.events.at(-1)));
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

const storeFile = (site, hash) => path.join(SERVER_DIR, SITES[site].programs, hash.slice(0, 2), `${hash}.bamlprog`);

// The first remote calls of a fresh instance. cloud and cloud2 have empty
// program stores, so each fetches the program from local before its first
// worker starts. The scene uses durable_settled: three calls, one of them fails.
async function programFetchAndSettled(label) {
  section(`${label}program fetch on first use, and all_settled with one failing child`);
  const fetchedBefore = allEvents().filter((e) => e.type === "program_fetched").length;
  const id = (await post("local", "/api/runs", { function: "durable_settled", args: { city: "Lisbon" } })).json.id;
  const hello = await STREAMS.local.waitEvent("hello", (e) => e.type === "hello" && e.run === id);
  const hash = hello.program_hash;
  check(`${label}hello carries program_hash, and the run record stores it`, /^[0-9a-f]{64}$/.test(hash)
    && (await waitFor("program_hash on the record", async () => (await get("local", `/api/runs/${id}`)).json.program_hash)) === hash, hash);
  check(`${label}the worker got --program-store: the compiled program is in local's store`, hello.mock_program_source === "compile" && fs.existsSync(storeFile("local", hash)));
  const done = await waitStatus("local", id, "completed", 40000);
  const kids = await childrenOf(id);
  check(`${label}three children, placed on cloud, cloud2, cloud in call order or the mirror image`, kids.length === 3 && (() => {
    const site = Object.fromEntries(kids.map((k) => [k.parent.call_id, k.site]));
    return inPool(site[`${id}-c1`], "local") && site[`${id}-c1`] !== site[`${id}-c2`] && site[`${id}-c1`] === site[`${id}-c3`];
  })(), JSON.stringify(kids.map((k) => [k.parent.call_id, k.site])));
  const fetched = allEvents().filter((e) => e.type === "program_fetched").slice(fetchedBefore);
  check(`${label}cloud and cloud2 each fetched the program once from local, although two children reached one site at the same time`,
    fetched.length === 2 && eachOnce(fetched, "site", ["cloud", "cloud2"]) && fetched.every((e) => e.hash === hash && e.from_site === "local" && e.bytes > 0),
    JSON.stringify(fetched.map((e) => [e.site, e.from_site])));
  const bytes = SITE_NAMES.map((site) => fs.readFileSync(storeFile(site, hash)));
  check(`${label}the fetched store entries are byte-identical to the entry that local's worker wrote`, bytes[1].equals(bytes[0]) && bytes[2].equals(bytes[0]));
  const served = await get("local", `/api/programs/${hash}`);
  const payload = Buffer.from(served.json?.program_base64 ?? "", "base64");
  check(`${label}GET /api/programs/:hash returns {hash, runtime_build, program_base64}, and the bytes have the hash`, served.status === 200 && served.json.hash === hash
    && served.json.runtime_build === "mock-worker/1" && crypto.createHash("sha256").update(payload).digest("hex") === hash, served.text.slice(0, 120));
  const kidHellos = kids.map((k) => STREAMS[k.site].find((e) => e.type === "hello" && e.run === k.id));
  check(`${label}the children ran by hash: --program-hash from the store, without --project`, kidHellos.every((h) => h && h.program_hash === hash
    && h.mock_program_source === "store" && h.mock_project === null) && kids.every((k) => k.program_hash === hash), JSON.stringify(kidHellos.map((h) => [h?.mock_program_source, h?.mock_project])));

  // all_settled: every child settles, none is cancelled.
  const failedKid = kids.find((k) => k.parent.call_id === `${id}-c2`);
  check(`${label}the Car child failed with the typed error, the other two completed, and none was cancelled`, failedKid.status === "failed"
    && failedKid.error.includes("QuoteUnavailable") && failedKid.error.includes("no Car vendor answers in Lisbon")
    && kids.filter((k) => k.status === "completed").length === 2 && eventsOf(id, "remote_cancel").length === 0 && eventsOf(id, "remote_cancelled").length === 0,
    JSON.stringify(kids.map((k) => k.status)));
  check(`${label}the class-valued argument reached the child intact (enum, nested class, optional, map)`,
    isDeepStrictEqual(failedKid.args.request, expectedRequest("Lisbon", "Car", 3000, { simulate: "unavailable" })), JSON.stringify(failedKid.args));
  check(`${label}SettledReport: two quotes with nested values, one failure with its kind and the remote error`,
    isDeepStrictEqual(done.result.succeeded, [expectedQuote("Lisbon", "Flight", 2000), expectedQuote("Lisbon", "Tour", 4000)])
    && done.result.failed.length === 1 && done.result.failed[0].kind === "Car" && done.result.failed[0].error.includes("no Car vendor answers in Lisbon")
    && done.result.city === "Lisbon", JSON.stringify(done.result).slice(0, 300));
  check(`${label}each call was dispatched once and returned once`, eachOnce(eventsOf(id, "remote_dispatched"), "call_id", [1, 2, 3].map((n) => `${id}-c${n}`))
    && eachOnce(eventsOf(id, "remote_returned"), "call_id", [1, 2, 3].map((n) => `${id}-c${n}`))
    && eachOnce(eventsOf(id, "remote_result_received"), "call_id", [1, 2, 3].map((n) => `${id}-c${n}`)));

  // Second use: both sites hold the program now.
  const again = (await post("local", "/api/runs", { function: "plan_trip", args: { city: "Porto" } })).json.id;
  await waitStatus("local", again, "completed");
  check(`${label}second use: the program is served from the site's own store, and nothing is fetched again`,
    allEvents().filter((e) => e.type === "program_fetched").length === fetchedBefore + 2);
  return hash;
}

async function napScene(label) {
  section(`${label}durable sleep: nap -> sleeping -> timer resume -> completed`);
  const t0 = Date.now();
  const id = (await post("local", "/api/runs", { function: "durable_nap", args: { seconds: 3 } })).json.id;
  const sleeping = await waitStatus("local", id, "sleeping");
  const pausedEv = eventsOf(id, "paused")[0];
  const pid1 = eventsOf(id, "hello")[0].pid;
  check(`${label}the worker suspended itself: paused carries wake {reason: sleep, remaining_ms, at_ts}, and the process is gone`, pausedEv && pausedEv.wake.reason === "sleep"
    && pausedEv.wake.remaining_ms > 0 && pausedEv.wake.remaining_ms <= real(3000) + 5 && typeof pausedEv.wake.at_ts === "number" && sleeping.pid === null && !pidAlive(pid1), JSON.stringify(pausedEv?.wake));
  check(`${label}status sleeping with a server-owned wake_at: receipt time plus remaining_ms`, typeof sleeping.wake_at === "number"
    && Math.abs(sleeping.wake_at - (t0 + real(3000))) < 1200 && sleeping.wake_at >= pausedEv.ts + pausedEv.wake.remaining_ms - 5, `${sleeping.wake_at - t0} ms after the start`);
  const exit1 = await STREAMS.local.waitEvent("worker_exit", (e) => e.type === "worker_exit" && e.run === id);
  const scheduled = await STREAMS.local.waitEvent("sleep_scheduled", (e) => e.type === "sleep_scheduled" && e.run === id);
  check(`${label}worker_exit has exit code 75 and status sleeping, and sleep_scheduled carries the same wake_at`, exit1.exit_code === 75 && exit1.status === "sleeping"
    && scheduled.wake_at === sleeping.wake_at && scheduled.site === "local");
  check(`${label}the suspend wrote a snapshot like a pause does`, sleeping.snapshots.length === 1 && sleeping.snapshots[0].automatic === false
    && fs.existsSync(sleeping.snapshots[0].snapshot_path) && sleeping.snapshots[0].stats.pause_latency_ms === null);
  const meta = JSON.parse(fs.readFileSync(path.join(SERVER_DIR, SITES.local.runs, id, "meta.json"), "utf8"));
  check(`${label}meta.json holds status sleeping and wake_at, so a restarted server can rebuild the timer`, meta.status === "sleeping" && meta.wake_at === sleeping.wake_at);
  const kill = await post("local", `/api/runs/${id}/kill`);
  const pause = await post("local", `/api/runs/${id}/pause`);
  check(`${label}kill and pause of a sleeping run are 409`, kill.status === 409 && pause.status === 409, `${kill.status} ${pause.status}`);
  const done = await waitStatus("local", id, "completed");
  const woken = eventsOf(id, "woken");
  check(`${label}the timer resumed the run at wake_at: one woken {reason: timer}, not early and not late`, woken.length === 1 && woken[0].reason === "timer"
    && woken[0].ts >= sleeping.wake_at && woken[0].ts - sleeping.wake_at < 1500, `${woken[0]?.ts - sleeping.wake_at} ms after wake_at`);
  const hellos = eventsOf(id, "hello");
  check(`${label}segment 2 ran in a new process from the snapshot, with the program from the store`, done.segment === 2 && hellos.length === 2 && hellos[1].mode === "resume"
    && hellos[1].pid !== pid1 && eventsOf(id, "resumed")[0].stats.program_source === "store" && done.wake_at === null);
  const texts = eventsOf(id, "log").filter((e) => e.stream === "stdout").map((e) => e.text);
  check(`${label}the result and the output are those of an uninterrupted run`, done.result === "slept 3 seconds"
    && JSON.stringify(texts) === JSON.stringify(["going to sleep for 3 seconds", "woke up"]), JSON.stringify(texts));
  const stored = (await get("local", `/api/runs/${id}/events`)).json.map((e) => e.type);
  check(`${label}sleep_scheduled and woken are in events.jsonl`, stored.includes("sleep_scheduled") && stored.includes("woken"));
}

async function manualResumeScene(label) {
  section(`${label}durable sleep: manual resume before the deadline`);
  // Far from the deadline: the new worker sees at least the threshold and suspends again.
  const far = (await post("local", "/api/runs", { function: "durable_nap", args: { seconds: 7 } })).json.id;
  const first = await waitStatus("local", far, "sleeping");
  const resumed = await post("local", `/api/runs/${far}/resume`);
  check(`${label}POST resume works on a sleeping run`, resumed.status === 200 && resumed.json.segment === 2 && resumed.json.status === "starting" && resumed.json.wake_at === null, resumed.text);
  const second = await waitFor("the second suspend", async () => {
    const r = (await get("local", `/api/runs/${far}`)).json;
    return r.status === "sleeping" && r.segment === 2 ? r : null;
  });
  await waitFor("the second sleep_scheduled event", () => eventsOf(far, "sleep_scheduled").length === 2);
  check(`${label}the resumed worker suspends again for the rest of the sleep: the deadline did not move`, Math.abs(second.wake_at - first.wake_at) < 400
    && second.snapshots.length === 2 && eventsOf(far, "sleep_scheduled").length === 2, `${second.wake_at - first.wake_at} ms`);
  check(`${label}woken {reason: manual} for the request`, eventsOf(far, "woken").length === 1 && eventsOf(far, "woken")[0].reason === "manual");
  const farDone = await waitStatus("local", far, "completed");
  const wokenFar = eventsOf(far, "woken");
  check(`${label}the timer of the second suspend completes the run in segment 3, and the timer of the first suspend started nothing`, farDone.segment === 3
    && farDone.result === "slept 7 seconds" && wokenFar.length === 2 && wokenFar[1].reason === "timer" && eachOnce(eventsOf(far, "hello"), "segment", [1, 2, 3]),
    JSON.stringify(eventsOf(far, "hello").map((e) => e.segment)));

  // Close to the deadline: the worker sleeps the rest in process.
  const near = (await post("local", "/api/runs", { function: "durable_nap", args: { seconds: 3 } })).json.id;
  const nearSleeping = await waitStatus("local", near, "sleeping");
  await sleep(real(1000));
  await post("local", `/api/runs/${near}/resume`);
  const nearDone = await waitStatus("local", near, "completed");
  const completedEv = eventsOf(near, "completed")[0];
  check(`${label}a resume with less than the threshold left sleeps the rest in process and completes at the deadline`, nearDone.segment === 2
    && eventsOf(near, "paused").length === 1 && completedEv.ts >= nearSleeping.wake_at - 300, `${completedEv.ts - nearSleeping.wake_at} ms`);
  await sleep(Math.max(0, nearSleeping.wake_at + 600 - Date.now()));
  check(`${label}the timer that fires after the manual resume does nothing`, eventsOf(near, "hello").length === 2 && eventsOf(near, "woken").length === 1
    && (await get("local", `/api/runs/${near}`)).json.segment === 2);
}

async function resumeRaceScene(label) {
  section(`${label}resume is a compare-and-set: double resume, and timer against manual resume`);
  const id = (await post("local", "/api/runs", { function: "durable_nap", args: { seconds: 6 } })).json.id;
  await waitStatus("local", id, "sleeping");
  const answers = await Promise.all([1, 2, 3, 4, 5].map(() => post("local", `/api/runs/${id}/resume`)));
  const codes = answers.map((r) => r.status).sort();
  check(`${label}five resume requests at once: one 200, four 409`, JSON.stringify(codes) === JSON.stringify([200, 409, 409, 409, 409]), JSON.stringify(codes));
  await STREAMS.local.waitEvent("hello of segment 2", (e) => e.type === "hello" && e.run === id && e.segment === 2);
  const pids = workerPids(id);
  check(`${label}exactly one worker process exists for the run`, pids.length <= 1 && eventsOf(id, "hello").filter((e) => e.segment === 2).length === 1, JSON.stringify(pids));
  const done = await waitStatus("local", id, "completed");
  check(`${label}every segment started once, and the run completed once`, eachOnce(eventsOf(id, "hello"), "segment", [1, 2, 3]) && done.segment === 3
    && eventsOf(id, "completed").length === 1 && new Set(eventsOf(id, "hello").map((e) => e.pid)).size === 3, JSON.stringify(eventsOf(id, "hello").map((e) => e.segment)));

  // Timer against request: requests are sent around wake_at, three runs at once.
  const ids = [];
  for (let i = 0; i < 3; i++) ids.push((await post("local", "/api/runs", { function: "durable_nap", args: { seconds: 3 } })).json.id);
  const sleeping = await Promise.all(ids.map((r) => waitStatus("local", r, "sleeping")));
  await Promise.all(sleeping.map(async (run, i) => {
    const offsets = [-12 + 4 * i, -2 + 2 * i, 3 + 2 * i];
    await Promise.all(offsets.map(async (offset) => {
      await sleep(Math.max(0, run.wake_at + offset - Date.now()));
      return post("local", `/api/runs/${run.id}/resume`);
    }));
  }));
  const finished = await Promise.all(ids.map((r) => waitStatus("local", r, "completed")));
  await sleep(300);
  check(`${label}a manual resume at the moment of the timer: one woken event, one worker for segment 2, and no segment 3`, finished.every((run) => run.segment === 2
    && run.result === "slept 3 seconds" && eventsOf(run.id, "woken").length === 1 && eachOnce(eventsOf(run.id, "hello"), "segment", [1, 2])
    && eventsOf(run.id, "completed").length === 1 && eventsOf(run.id, "worker_exit").length === 2),
    JSON.stringify(finished.map((run) => [run.segment, eventsOf(run.id, "woken").map((e) => e.reason), eventsOf(run.id, "hello").length])));
  check(`${label}no worker of these runs is left`, [id, ...ids].every((r) => workerPids(r).length === 0));
}

async function cancelSleepingScene(label) {
  section(`${label}cancel of a run without a process: sleeping and paused`);
  const id = (await post("local", "/api/runs", { function: "durable_nap", args: { seconds: 4 } })).json.id;
  const sleeping = await waitStatus("local", id, "sleeping");
  const cancelled = await post("local", `/api/runs/${id}/cancel`);
  check(`${label}POST cancel on a sleeping run sets cancelled without a process`, cancelled.status === 200 && cancelled.json.status === "cancelled"
    && cancelled.json.pid === null && cancelled.json.wake_at === null, cancelled.text);
  const ev = await STREAMS.local.waitEvent("run_cancelled", (e) => e.type === "run_cancelled" && e.run === id);
  check(`${label}run_cancelled {was: sleeping} is announced and stored`, ev.was === "sleeping" && (await get("local", `/api/runs/${id}/events`)).json.some((e) => e.type === "run_cancelled"));
  const again = await post("local", `/api/runs/${id}/cancel`);
  const resume = await post("local", `/api/runs/${id}/resume`);
  check(`${label}a second cancel and a resume of the cancelled run are 409`, again.status === 409 && resume.status === 409, `${again.status} ${resume.status}`);
  await sleep(Math.max(0, sleeping.wake_at + 700 - Date.now()));
  const after = (await get("local", `/api/runs/${id}`)).json;
  check(`${label}the timer of the cancelled run fires into nothing: no woken event, no second worker`, after.status === "cancelled" && after.segment === 1
    && eventsOf(id, "woken").length === 0 && eventsOf(id, "hello").length === 1);

  const paused = (await post("local", "/api/runs", { function: "durable_plan_trip", args: { city: "Silves" } })).json.id;
  await STREAMS.local.waitEvent("first log", (e) => e.type === "log" && e.run === paused);
  await post("local", `/api/runs/${paused}/pause`);
  await waitStatus("local", paused, "paused");
  const c2 = await post("local", `/api/runs/${paused}/cancel`);
  check(`${label}POST cancel on a paused run sets cancelled without a process`, c2.status === 200 && c2.json.status === "cancelled"
    && !!STREAMS.local.find((e) => e.type === "run_cancelled" && e.run === paused && e.was === "paused"), c2.text);

  // A remote child that sleeps is cancelled without a process too.
  const child = (await post("cloud", "/api/remote/runs", { function: "durable_nap", args: { seconds: 30 }, parent: { site: "local", run: "r-ghost", call_id: "r-ghost-c7" } })).json.id;
  await waitStatus("cloud", child, "sleeping");
  const c3 = await post("cloud", `/api/runs/${child}/cancel`, { reason: "the caller cancelled the remote call" });
  check(`${label}a sleeping remote child is cancelled without a process, and the reason is recorded`, c3.status === 200 && c3.json.status === "cancelled"
    && c3.json.error === "the caller cancelled the remote call" && workerPids(child).length === 0, c3.text);
}

async function fanOutScene(label, slow = false) {
  section(`${label}fan-out: four class-valued remote calls, round-robin placement, results stored while the parent sleeps`);
  const city = "Lisbon";
  const id = (await post("local", "/api/runs", { function: "durable_fan_out", args: { city, mock_nap_ms: slow ? 13000 : 8000 } })).json.id;
  const calls = [1, 2, 3, 4].map((n) => `${id}-c${n}`);
  const sleeping = await waitStatus("local", id, "sleeping");
  const pid1 = eventsOf(id, "hello")[0].pid;
  check(`${label}the parent suspended itself with four threads waiting on remote calls: no process`, sleeping.pid === null && !pidAlive(pid1) && workerPids(id).length === 0
    && eventsOf(id, "thread_started").filter((e) => e.segment === 1).length === 5);
  const state = (await get("local", `/api/runs/${id}/snapshots/${sleeping.snapshots.at(-1).n}/state`)).json;
  const main = state.threads[0];
  const flight = main.frames[0].locals.find((l) => l.name === "flight");
  check(`${label}the StateDump shows five threads, the futures, and class instances with nested values`, state.threads.length === 5
    && state.threads.slice(1).every((t) => t.parked.kind === "remote_call" && t.frames[0].locals[0].value.kind === "instance" && t.frames[0].locals[0].value.class === "QuoteRequest")
    && main.parked.kind === "sysop" && flight && flight.type === "Future<Quote>"
    && state.threads[1].frames[0].locals[0].value.children.some((c) => c.key === "traveler" && c.value.kind === "instance" && c.value.class === "Traveler"), JSON.stringify(main.parked));

  await waitFor("four dispatches", () => eventsOf(id, "remote_dispatched").length === 4, 20000);
  const site = Object.fromEntries(eventsOf(id, "remote_dispatched").map((e) => [e.call_id, e.child_site]));
  check(`${label}round-robin placement in call order: the calls alternate between cloud and cloud2`, inPool(site[calls[0]], "local") && site[calls[0]] !== site[calls[1]]
    && site[calls[0]] === site[calls[2]] && site[calls[1]] === site[calls[3]], JSON.stringify(site));
  const kids = await childrenOf(id);
  check(`${label}four children, two on each pool site, none on local, each with the class-valued argument`, kids.length === 4
    && kids.filter((k) => k.site === "cloud").length === 2 && kids.filter((k) => k.site === "cloud2").length === 2
    && ["Flight", "Hotel", "Car", "Tour"].every((kind, i) => {
      const k = kids.find((c) => c.parent.call_id === calls[i]);
      return k && isDeepStrictEqual(k.args.request, expectedRequest(city, kind, [3000, 5000, 2000, 4000][i]));
    }), JSON.stringify(kids.map((k) => [k.site, k.args.request?.kind])));
  check(`${label}children ran while the parent had no process`, kids.some((k) => is_live(k.status)) || slow, JSON.stringify(kids.map((k) => k.status)));

  const stored = await waitFor("four stored results on the sleeping parent", async () => {
    const r = (await get("local", `/api/runs/${id}`)).json;
    return r.remote_results.length === 4 ? r : null;
  }, 30000);
  check(`${label}all four results were stored while no process existed: status sleeping, not acknowledged, waiting_on empty`, stored.status === "sleeping"
    && stored.pid === null && stored.remote_results.every((r) => r.acked === false && r.error === null) && stored.waiting_on.length === 0
    && eventsOf(id, "remote_result_received").length === 0, stored.status);
  check(`${label}the stored values are whole Quote objects`, ["Flight", "Hotel", "Car", "Tour"].every((kind, i) =>
    isDeepStrictEqual(stored.remote_results.find((r) => r.call_id === calls[i]).value, expectedQuote(city, kind, [3000, 5000, 2000, 4000][i]))));

  const done = await waitStatus("local", id, "completed", 40000);
  const received = eventsOf(id, "remote_result_received");
  check(`${label}at the resume all four results were delivered, each once, in segment 2`, eachOnce(received, "call_id", calls) && received.every((e) => e.segment === 2)
    && done.remote_results.every((r) => r.acked) && done.segment === 2 && eventsOf(id, "woken")[0]?.reason === "timer");
  const quotes = ["Flight", "Hotel", "Car", "Tour"].map((kind, i) => expectedQuote(city, kind, [3000, 5000, 2000, 4000][i]));
  check(`${label}TripReport: quotes in input order, total, the vendors map, and the cheapest quote with its nested request`, isDeepStrictEqual(done.result, {
    city, quotes, total: { amount: 1194, currency: "EUR" },
    vendors: { Flight: "Skyways", Hotel: "Casa Azul", Car: "Rodas", Tour: "Seven Hills Walks" },
    cheapest: quotes[2],
  }), JSON.stringify(done.result).slice(0, 200));
  check(`${label}exactly once: each call dispatched once, returned once, one child per call, one completed event`, eachOnce(eventsOf(id, "remote_dispatched"), "call_id", calls)
    && eachOnce(eventsOf(id, "remote_returned"), "call_id", calls) && (await childrenOf(id)).length === 4 && eventsOf(id, "completed").length === 1
    && eventsOf(id, "remote_cancelled").length === 0 && eventsOf(id, "remote_result_discarded").length === 0);
  await waitFor("result_delivered on every child", async () => (await childrenOf(id)).every((k) => k.result_delivered));
  check(`${label}every child knows that its result was delivered, so none retries`, true);
}
const is_live = (status) => ["starting", "running", "pausing"].includes(status);

async function raceScene(label) {
  section(`${label}race: the winner settles the run, the losers are cancelled on their sites, late results are discarded`);
  const city = "Porto";
  const id = (await post("local", "/api/runs", { function: "durable_race", args: { city } })).json.id;
  const calls = [1, 2, 3].map((n) => `${id}-c${n}`);
  const done = await waitStatus("local", id, "completed", 40000);
  check(`${label}the run returns the Quote of the fastest vendor with its nested values`, isDeepStrictEqual(done.result, expectedQuote(city, "Hotel", 2000)), JSON.stringify(done.result).slice(0, 200));
  const workerCancels = eventsOf(id, "remote_cancel");
  check(`${label}the worker reported remote_cancel for both losers`, eachOnce(workerCancels, "call_id", [calls[1], calls[2]]) && workerCancels.every((e) => typeof e.thread === "number"));
  await waitFor("two remote_cancelled events", () => eventsOf(id, "remote_cancelled").length === 2, 20000);
  const serverCancels = eventsOf(id, "remote_cancelled");
  const kids = await waitFor("the losers to end", async () => {
    const list = await childrenOf(id);
    return list.length === 3 && list.every((k) => !is_live(k.status)) ? list : null;
  }, 30000);
  check(`${label}remote_cancelled names each loser's site and run, once`, eachOnce(serverCancels, "call_id", [calls[1], calls[2]])
    && serverCancels.every((e) => { const k = kids.find((c) => c.parent.call_id === e.call_id); return k && k.id === e.child_run && k.site === e.child_site; }));
  const winner = kids.find((k) => k.parent.call_id === calls[0]);
  // Cancellation is a request. Under CHAOS the cancel reaches the 6 second
  // loser about 1.5 seconds before it is done, and one retried HTTP request
  // uses that margin up. That loser may then complete, and its result is
  // discarded like the result of a cancelled child. The 9 second loser is
  // always cancelled.
  const ended = (k) => k.status === "cancelled" && STREAMS[k.site].find((e) => e.type === "worker_exit" && e.run === k.id)?.exit_code === 130;
  const medium = kids.find((k) => k.parent.call_id === calls[1]);
  const slowest = kids.find((k) => k.parent.call_id === calls[2]);
  check(`${label}the losers ended as cancelled on their sites (exit code 130), and the winner completed`, winner.status === "completed"
    && ended(slowest) && (ended(medium) || (label.startsWith("CHAOS") && medium.status === "completed")),
    JSON.stringify({
      kids: kids.map((k) => [k.parent.call_id, k.site, k.status, k.updated_ts - done.created_ts]),
      returned: eventsOf(id, "remote_returned").map((e) => e.ts - done.created_ts),
      cancel: eventsOf(id, "remote_cancel").map((e) => e.ts - done.created_ts),
      cancelled: eventsOf(id, "remote_cancelled").map((e) => e.ts - done.created_ts),
    }));
  await waitFor("two discarded results", () => eventsOf(id, "remote_result_discarded").length >= 2, 20000);
  await waitFor("result_delivered on every child", async () => (await childrenOf(id)).every((k) => k.result_delivered), 20000);
  await sleep(400);
  const after = (await get("local", `/api/runs/${id}`)).json;
  check(`${label}the late results of the cancelled children were discarded, each once: only the winner's result is on the record`,
    eachOnce(eventsOf(id, "remote_result_discarded"), "call_id", [calls[1], calls[2]]) && after.remote_results.length === 1 && after.remote_results[0].call_id === calls[0]
    && after.waiting_on.length === 0 && isDeepStrictEqual([...after.cancelled_calls].sort(), [calls[1], calls[2]]), JSON.stringify(after.remote_results.map((r) => r.call_id)));
  check(`${label}exactly once: three dispatches, one returned result, one acknowledged result, one completed event`, eachOnce(eventsOf(id, "remote_dispatched"), "call_id", calls)
    && eachOnce(eventsOf(id, "remote_returned"), "call_id", [calls[0]]) && eachOnce(eventsOf(id, "remote_result_received"), "call_id", [calls[0]])
    && eventsOf(id, "completed").length === 1 && kids.every((k) => workerPids(k.id).length === 0));
}

// A pause request while several threads wait on remote calls, and a fork of a
// sleeping run.
async function pauseRaceScene(label) {
  section(`${label}pause with four threads: the race is decided after the resume, from a stored result`);
  const id = (await post("local", "/api/runs", { function: "durable_race", args: { city: "Viseu" } })).json.id;
  const calls = [1, 2, 3].map((n) => `${id}-c${n}`);
  await waitFor("three dispatches", () => eventsOf(id, "remote_dispatched").length === 3, 20000);
  await post("local", `/api/runs/${id}/pause`);
  const paused = await waitStatus("local", id, "paused");
  const state = (await get("local", `/api/runs/${id}/snapshots/${paused.snapshots.at(-1).n}/state`)).json;
  check(`${label}the run paused with five threads: three remote calls, the collector of race, and the main thread`, state.threads.length === 5
    && state.threads.filter((t) => t.parked.kind === "remote_call").length === 3 && paused.wake_at === null && paused.pid === null, `${state.threads.length} threads`);
  await waitFor("the winner's result on the paused run", async () => (await get("local", `/api/runs/${id}`)).json.remote_results.length === 1, 20000);
  check(`${label}no child is cancelled while the parent is paused: the race is not decided without a process`, eventsOf(id, "remote_cancelled").length === 0
    && (await childrenOf(id)).filter((k) => is_live(k.status)).length === 2);
  await post("local", `/api/runs/${id}/resume`);
  const done = await waitStatus("local", id, "completed", 40000);
  check(`${label}segment 2 takes the stored result, returns the winner, and cancels the two losers`, isDeepStrictEqual(done.result, expectedQuote("Viseu", "Hotel", 2000))
    && eachOnce(eventsOf(id, "remote_cancel"), "call_id", [calls[1], calls[2]]) && eventsOf(id, "remote_cancel").every((e) => e.segment === 2)
    && eachOnce(eventsOf(id, "thread_started").filter((e) => e.segment === 2), "thread", [1, 2, 3, 4, 5]), JSON.stringify(eventsOf(id, "remote_cancel").map((e) => [e.segment, e.call_id])));
  await waitFor("the losers to be cancelled", async () => (await childrenOf(id)).filter((k) => k.status === "cancelled").length === 2, 20000);
  check(`${label}the children that were dispatched by segment 1 are cancelled by segment 2`, eachOnce(eventsOf(id, "remote_cancelled"), "call_id", [calls[1], calls[2]])
    && eachOnce(eventsOf(id, "remote_dispatched"), "call_id", calls));

  const napId = (await post("local", "/api/runs", { function: "durable_nap", args: { seconds: 5 } })).json.id;
  const sleeping = await waitStatus("local", napId, "sleeping");
  const fork = await post("local", `/api/runs/${napId}/fork`);
  check(`${label}a fork of a sleeping run is paused and has no wake_at`, fork.status === 200 && fork.json.status === "paused" && fork.json.wake_at === null
    && fork.json.program_hash === sleeping.program_hash, fork.text.slice(0, 200));
  await post("local", `/api/runs/${fork.json.id}/resume`);
  const [srcDone, forkDone] = await Promise.all([waitStatus("local", napId, "completed"), waitStatus("local", fork.json.id, "completed")]);
  check(`${label}the fork suspends for the same deadline on its own, and both runs complete`, srcDone.result === "slept 5 seconds" && forkDone.result === "slept 5 seconds"
    && eventsOf(fork.json.id, "sleep_scheduled").length === 1 && Math.abs(eventsOf(fork.json.id, "sleep_scheduled")[0].wake_at - sleeping.wake_at) < 600);
}

async function deadlineScene(label) {
  section(`${label}deadline: with_timeout cancels the remote call`);
  const id = (await post("local", "/api/runs", { function: "durable_deadline", args: { city: "Faro" } })).json.id;
  const done = await waitStatus("local", id, "completed", 40000);
  check(`${label}the function returns a message built from the Timeout error`, done.result === "no tour quote for Faro: operation timed out after 2000ms", JSON.stringify(done.result));
  check(`${label}the 2 second deadline stayed in process: the run did not suspend`, done.segment === 1 && eventsOf(id, "paused").length === 0);
  await waitFor("remote_cancelled", () => eventsOf(id, "remote_cancelled").length === 1, 20000);
  const kids = await waitFor("the child to be cancelled", async () => {
    const list = await childrenOf(id);
    return list.length === 1 && list[0].status === "cancelled" ? list : null;
  }, 20000);
  await waitFor("the discarded result", () => eventsOf(id, "remote_result_discarded").length === 1, 20000);
  const after = (await get("local", `/api/runs/${id}`)).json;
  check(`${label}the child was cancelled on its site, and its late result was discarded`, kids[0].status === "cancelled" && eventsOf(id, "remote_cancel").length === 1
    && after.remote_results.length === 0 && after.waiting_on.length === 0 && eventsOf(id, "remote_returned").length === 0);
}

async function cascadeScene(label) {
  section(`${label}a parent that ends as failed, cancelled, or lost cancels its children`);
  const failing = (await post("local", "/api/runs", { function: "durable_race", args: { city: "Braga", mock_fail_after_ms: 300 } })).json.id;
  const cancelling = (await post("local", "/api/runs", { function: "durable_race", args: { city: "Evora" } })).json.id;
  const losing = (await post("local", "/api/runs", { function: "durable_race", args: { city: "Tomar" } })).json.id;
  await Promise.all([failing, cancelling, losing].map((id) => waitFor(`three dispatches of ${id}`, () => eventsOf(id, "remote_dispatched").length === 3, 20000)));
  await post("local", `/api/runs/${cancelling}/cancel`);
  await post("local", `/api/runs/${losing}/kill`);
  const ends = await Promise.all([waitStatus("local", failing, "failed"), waitStatus("local", cancelling, "cancelled"), waitStatus("local", losing, "lost")]);
  check(`${label}the three parents ended as failed, cancelled, and lost`, ends[0].error.includes("mock failure") && ends[1].status === "cancelled" && ends[2].status === "lost");
  for (const [id, how] of [[failing, "failed"], [cancelling, "cancelled"], [losing, "lost"]]) {
    const kids = await waitFor(`the children of the ${how} parent to end`, async () => {
      const list = await childrenOf(id);
      return list.length === 3 && list.every((k) => !is_live(k.status)) ? list : null;
    }, 25000);
    const calls = [1, 2, 3].map((n) => `${id}-c${n}`);
    const record = (await get("local", `/api/runs/${id}`)).json;
    // Under CHAOS the cancel reaches the 2 second child about a second before
    // it is done. One retried HTTP request uses that margin up, the child
    // completes, and its result is discarded. The other two are always cancelled.
    const statusOf = (n) => kids.find((k) => k.parent.call_id === calls[n]).status;
    check(`${label}${how} parent: the server cancelled all three children without a remote_cancel from the worker`, kids.length === 3
      && statusOf(1) === "cancelled" && statusOf(2) === "cancelled" && (statusOf(0) === "cancelled" || (label.startsWith("CHAOS") && statusOf(0) === "completed"))
      && eachOnce(eventsOf(id, "remote_cancelled"), "call_id", calls) && eventsOf(id, "remote_cancel").length === 0
      && record.waiting_on.length === 0 && isDeepStrictEqual([...record.cancelled_calls].sort(), calls), JSON.stringify(kids.map((k) => k.status)));
    await waitFor(`discarded results of the ${how} parent`, () => eventsOf(id, "remote_result_discarded").length === 3, 25000);
    check(`${label}${how} parent: the children's late answers were discarded, and nothing was stored`, (await get("local", `/api/runs/${id}`)).json.remote_results.length === 0
      && kids.every((k) => workerPids(k.id).length === 0));
  }

  // A sleeping parent has no process. Its cancel still reaches the children.
  const asleep = (await post("local", "/api/runs", { function: "durable_fan_out", args: { city: "Guarda", mock_nap_ms: 20000 } })).json.id;
  await waitStatus("local", asleep, "sleeping");
  await waitFor("four dispatches", () => eventsOf(asleep, "remote_dispatched").length === 4, 20000);
  const c = await post("local", `/api/runs/${asleep}/cancel`);
  const kids = await waitFor("the children of the sleeping parent to be cancelled", async () => {
    const list = await childrenOf(asleep);
    return list.length === 4 && list.every((k) => k.status === "cancelled") ? list : null;
  }, 25000);
  check(`${label}cancel of a sleeping parent cancels its four outstanding children`, c.status === 200 && c.json.status === "cancelled" && kids.length === 4
    && eachOnce(eventsOf(asleep, "remote_cancelled"), "call_id", [1, 2, 3, 4].map((n) => `${asleep}-c${n}`)));
}

// A parent of a remote call that fails inside baml.future.all: the first error
// in input order cancels the inputs that are still pending.
async function fanOutFailureScene(label) {
  section(`${label}baml.future.all with a failing child: the remaining children are cancelled`);
  const id = (await post("local", "/api/runs", { function: "durable_fan_out", args: { city: "Beja", mock_nap_ms: 2400, mock_unavailable: "Flight" } })).json.id;
  const failed = await waitStatus("local", id, "failed", 40000);
  check(`${label}the parent fails with the child's typed error`, failed.error.includes("no Flight vendor answers in Beja"), failed.error);
  const kids = await waitFor("the pending children to be cancelled", async () => {
    const list = await childrenOf(id);
    return list.length === 4 && list.every((k) => !is_live(k.status)) ? list : null;
  }, 25000);
  const hotel = kids.find((k) => k.args.request.kind === "Hotel");
  check(`${label}the Hotel child (5 s) was still running and was cancelled`, hotel.status === "cancelled" && kids.find((k) => k.args.request.kind === "Flight").status === "failed",
    JSON.stringify(kids.map((k) => [k.args.request.kind, k.status])));
}

// A sleeping run migrates. The import starts a worker on the other site, which
// suspends again, and the timer of that site completes the run.
async function sleepingMigrationScene(label) {
  section(`${label}a sleeping run moves to another site and sleeps on there`);
  const id = (await post("local", "/api/runs", { function: "durable_nap", args: { seconds: 7 } })).json.id;
  const here = await waitStatus("local", id, "sleeping");
  const moved = await post("local", `/api/runs/${id}/resume`, { site: "cloud2" });
  check(`${label}resume {site} of a sleeping run migrates it`, moved.status === 200 && moved.json.status === "migrated" && moved.json.wake_at === null, moved.text);
  const there = await waitFor("the run to sleep on cloud2", async () => {
    const r = await get("cloud2", `/api/runs/${id}`);
    return r.json && r.json.status === "sleeping" ? r.json : null;
  });
  check(`${label}cloud2 computed its own wake_at for the same deadline, and the program came from its store`, Math.abs(there.wake_at - here.wake_at) < 600
    && there.program_hash === here.program_hash && STREAMS.cloud2.find((e) => e.type === "hello" && e.run === id)?.mock_project === null, `${there.wake_at - here.wake_at} ms`);
  const done = await waitStatus("cloud2", id, "completed");
  await sleep(Math.max(0, here.wake_at + 500 - Date.now()));
  check(`${label}the run completes on cloud2, and the timer on local started nothing`, done.result === "slept 7 seconds" && done.segment === 3
    && (await get("local", `/api/runs/${id}`)).json.status === "migrated" && STREAMS.local.events.filter((e) => e.type === "hello" && e.run === id).length === 1);
}

async function restartWhileSleepingScene() {
  section("server restart while runs sleep: timers are rebuilt, and an overdue run resumes at once");
  const later = (await post("local", "/api/runs", { function: "durable_nap", args: { seconds: 9 } })).json.id;
  const overdue = (await post("local", "/api/runs", { function: "durable_nap", args: { seconds: 3 } })).json.id;
  const [laterRun, overdueRun] = await Promise.all([waitStatus("local", later, "sleeping"), waitStatus("local", overdue, "sleeping")]);
  await stopServer("local");
  await sleep(Math.max(0, overdueRun.wake_at + 400 - Date.now()));
  const restartedAt = Date.now();
  await startServer("local");
  const stream = new Stream("local");
  await stream.open();
  STREAMS.local = stream;
  const overdueDone = await waitStatus("local", overdue, "completed");
  const overdueEvents = (await get("local", `/api/runs/${overdue}/events`)).json;
  const wokenOverdue = overdueEvents.filter((e) => e.type === "woken");
  check("the overdue run resumed at once after the restart: woken {reason: restart}", overdueDone.result === "slept 3 seconds" && wokenOverdue.length === 1
    && wokenOverdue[0].reason === "restart" && wokenOverdue[0].ts - restartedAt < 4000, JSON.stringify(wokenOverdue));
  const reloaded = (await get("local", `/api/runs/${later}`)).json;
  check("the other run is still sleeping with the same wake_at", (reloaded.status === "sleeping" && reloaded.wake_at === laterRun.wake_at) || reloaded.status !== "sleeping", `${reloaded.status}`);
  const laterDone = await waitStatus("local", later, "completed");
  const laterEvents = (await get("local", `/api/runs/${later}/events`)).json;
  const wokenLater = laterEvents.filter((e) => e.type === "woken");
  check("its timer was rebuilt from the run store: sleep_scheduled again with the same wake_at, then woken {reason: timer} at the deadline", laterDone.result === "slept 9 seconds"
    && laterEvents.filter((e) => e.type === "sleep_scheduled").length === 2 && laterEvents.filter((e) => e.type === "sleep_scheduled").every((e) => e.wake_at === laterRun.wake_at)
    && wokenLater.length === 1 && wokenLater[0].reason === "timer" && wokenLater[0].ts >= laterRun.wake_at && wokenLater[0].ts - laterRun.wake_at < 1500,
    JSON.stringify(wokenLater));
  check("each of the two runs had exactly two segments", eachOnce(overdueEvents.filter((e) => e.type === "hello"), "segment", [1, 2])
    && eachOnce(laterEvents.filter((e) => e.type === "hello"), "segment", [1, 2]));
}

// The knobs of contract section 9.3. Every site-to-site request of the scenes
// below is slow, and every result is sent twice and out of order.
// The delays are real time, and the scenes depend on how they relate to the
// sleeps of the program (the cancel must reach the 6 second loser of the race
// before it is done). The CHAOS scenes therefore run the workers with
// MOCK_SPEED=1, whatever the speed of the other scenes is.
const CHAOS = {
  dispatch_delay_ms: 600, result_delay_ms: 500, cancel_delay_ms: 700, import_delay_ms: 500, program_fetch_delay_ms: 400,
  // A held result waits at most this long for another result to overtake it.
  // With the other delays the winner of durable_race still reaches its parent,
  // and the cancel still reaches the 6 second loser, before that loser is done.
  duplicate_results: true, reorder_results: true, reorder_hold_ms: 1200,
};

// Runs one scene. A scene that throws (a wait that timed out) counts as one
// failed check, and the scenes after it still run.
// ------------------------------------------------ scenes of the phase 3 review

/** A cancel and a pause that arrive after the worker reported `paused` and before its process has exited. */
async function lateCommandScene() {
  section("a cancel and a pause that arrive between the worker's `paused` event and its exit");
  const nap = (seconds) => post("local", "/api/runs", { function: "durable_nap", args: { seconds, mock_exit_delay_ms: 700 } });
  const id = (await nap(3)).json.id;
  await STREAMS.local.waitEvent("paused", (e) => e.type === "paused" && e.run === id && e.wake);
  const cancel = await post("local", `/api/runs/${id}/cancel`, { reason: "cancelled in the window" });
  const record = (await get("local", `/api/runs/${id}`)).json;
  check("the cancel is accepted while the process of the suspended worker still exists", cancel.status === 200 && ["running", "starting"].includes(record.status) && record.pid !== null, `${cancel.status} ${record.status}`);
  const ended = await waitStatus("local", id, "cancelled", 15000);
  await sleep(3200);
  const after = (await get("local", `/api/runs/${id}`)).json;
  check("the run ends as cancelled with the reason; it never sleeps, never wakes, and never completes", ended.error === "cancelled in the window" && after.status === "cancelled"
    && after.segment === 1 && after.wake_at === null && eventsOf(id, "woken").length === 0 && eventsOf(id, "sleep_scheduled").length === 0
    && eventsOf(id, "run_cancelled").length === 1 && eventsOf(id, "completed").length === 0, JSON.stringify([after.status, after.segment]));

  const id2 = (await nap(3)).json.id;
  await STREAMS.local.waitEvent("paused", (e) => e.type === "paused" && e.run === id2 && e.wake);
  const pause = await post("local", `/api/runs/${id2}/pause`);
  const paused = await waitStatus("local", id2, "paused", 15000);
  await sleep(3200);
  const still = (await get("local", `/api/runs/${id2}`)).json;
  check("a pause in the same window leaves the run paused: no timer resumes a run that the user paused", pause.status === 200 && paused.wake_at === null
    && still.status === "paused" && still.segment === 1 && eventsOf(id2, "woken").length === 0, `${pause.status} ${still.status}`);
  const resumed = await post("local", `/api/runs/${id2}/resume`);
  const done = await waitStatus("local", id2, "completed", 20000);
  check("the paused run resumes by hand and completes", resumed.status === 200 && done.result === "slept 3 seconds", JSON.stringify(done.result));
}

/** A fork and its source wait on the same child runs. Ending one must not cancel the children of the other. */
async function sharedChildrenScene() {
  section("a fork and its source share their remote children");
  const city = "Lisbon";
  const start = async () => {
    const id = (await post("local", "/api/runs", { function: "durable_fan_out", args: { city, mock_nap_ms: 9000 } })).json.id;
    await waitStatus("local", id, "sleeping");
    await waitFor("four dispatches", () => eventsOf(id, "remote_dispatched").length === 4, 20000);
    return id;
  };
  // Cancel the fork: the source completes with its four quotes.
  const source = await start();
  const fork = (await post("local", `/api/runs/${source}/fork`)).json;
  check("the fork of a sleeping fan-out inherits the waits of its source", fork.waiting_on.length === 4 && fork.waiting_on.every((w) => w.inherited === true), JSON.stringify(fork.waiting_on));
  const cancelFork = await post("local", `/api/runs/${fork.id}/cancel`);
  await sleep(800);
  const kids = await childrenOf(source);
  check("cancelling the fork cancels no child of the source", cancelFork.status === 200 && kids.length === 4 && kids.every((k) => k.status !== "cancelled")
    && eventsOf(fork.id, "remote_cancelled").length === 0 && eventsOf(fork.id, "remote_released").length === 4, JSON.stringify(kids.map((k) => k.status)));
  const done = await waitStatus("local", source, "completed", 40000);
  check("the source completes with four quotes", done.result.quotes.length === 4 && done.result.total.amount === 1194, JSON.stringify(done.result).slice(0, 120));

  // The mirror image: cancel the source, and the fork completes.
  const source2 = await start();
  const fork2 = (await post("local", `/api/runs/${source2}/fork`)).json;
  const cancelSource = await post("local", `/api/runs/${source2}/cancel`);
  await sleep(800);
  const kids2 = await childrenOf(source2);
  check("cancelling the source while its fork still waits cancels no child", cancelSource.status === 200 && kids2.every((k) => k.status !== "cancelled")
    && eventsOf(source2, "remote_released").length === 4 && eventsOf(source2, "remote_released").every((e) => e.shared_with === fork2.id), JSON.stringify(kids2.map((k) => k.status)));
  await waitFor("four stored results on the fork", async () => (await get("local", `/api/runs/${fork2.id}`)).json.remote_results.length === 4, 30000);
  const resumeFork = await post("local", `/api/runs/${fork2.id}/resume`);
  const forkDone = await waitStatus("local", fork2.id, "completed", 40000);
  check("the fork completes with four quotes after its source was cancelled", resumeFork.status === 200 && forkDone.result.quotes.length === 4 && forkDone.result.total.amount === 1194);
  // The last waiter does cancel: a fan-out without a fork.
  const lonely = await start();
  await post("local", `/api/runs/${lonely}/cancel`);
  await waitFor("the children of a run without a fork are cancelled", async () => (await childrenOf(lonely)).every((k) => k.status === "cancelled"), 20000);
  check("a run that is the only waiter cancels its children as before", eventsOf(lonely, "remote_cancelled").length === 4 && eventsOf(lonely, "remote_released").length === 0);
}

/** A by-hash run whose store entry disappears while it sleeps: the entry is fetched again before the resume. */
async function lostEntryScene() {
  section("a store entry that disappears while a by-hash run sleeps");
  const probe = (await post("local", "/api/runs", { function: "durable_nap", args: { seconds: 0 } })).json.id;
  const hash = (await waitStatus("local", probe, "completed")).program_hash;
  const child = (await post("cloud", "/api/remote/runs", {
    function: "durable_nap", args: { seconds: 4 }, parent: { site: "local", run: "r-ghost", call_id: "r-ghost-c9" }, program_hash: hash, from_site: "local",
  })).json.id;
  await waitStatus("cloud", child, "sleeping", 20000);
  const fetchedBefore = allEvents().filter((e) => e.type === "program_fetched" && e.site === "cloud").length;
  fs.rmSync(storeFile("cloud", hash));
  const done = await waitStatus("cloud", child, "completed", 30000);
  const fetched = allEvents().filter((e) => e.type === "program_fetched" && e.site === "cloud").length - fetchedBefore;
  const hellos = eventsOf(child, "hello");
  check("the timer resume fetched the program again from the parent's site, and the run completed from the store", done.result === "slept 4 seconds" && fetched === 1
    && fs.existsSync(storeFile("cloud", hash)) && hellos.length === 2 && hellos[1].mock_program_source === "store" && hellos[1].mock_project === null, `${fetched} ${JSON.stringify(done.result)}`);
}

/** A resumed worker reports its restored remote waits, and results carry their arrival time. */
async function restoredWaitScene() {
  section("remote_wait of a resumed worker, and results in arrival order");
  const id = (await post("local", "/api/runs", { function: "durable_fan_out", args: { city: "Lisbon", mock_nap_ms: 8000 } })).json.id;
  const done = await waitStatus("local", id, "completed", 60000);
  const waits = eventsOf(id, "remote_wait");
  const received = eventsOf(id, "remote_result_received").map((e) => e.call_id);
  const byArrival = [...done.remote_results].sort((a, b) => a.ts - b.ts).map((r) => r.call_id);
  check("the resumed worker names the four calls it waits on, and says that it holds their results", waits.length === 4 && waits.every((w) => w.segment === 2 && w.has_result === true
    && w.function === "remote_get_quote"), JSON.stringify(waits.map((w) => [w.call_id, w.has_result])));
  check("the results that arrived without a process are taken in the order of their arrival", isDeepStrictEqual(received, byArrival), `${received} / ${byArrival}`);
}

// ---------------------------------------------------------------------------
// Contract section 10: the call site of every remote call
// ---------------------------------------------------------------------------

/** The 1-based line of the first line of `file` in `program/baml_src` that contains `needle`. */
function programLine(file, needle) {
  const text = fs.readFileSync(path.join(ROOT, "program", file), "utf8");
  const index = text.split("\n").findIndex((line) => line.includes(needle));
  if (index === -1) throw new Error(`program/${file} has no line that contains ${needle}`);
  return index + 1;
}

const QUOTES = "baml_src/quotes.baml";

/**
 * Contract section 10.1 and 10.2: the worker reports the call site of a remote
 * call, the `spawn` site of a thread, and the call site plus the `cause` of a
 * cancellation. The site server forwards the fields unchanged and records the
 * call site in `waiting_on` and in `calls`, so the cause survives a resume and
 * a page reload.
 */
async function callSiteScene() {
  section("call sites: remote_call, thread_started, and remote_cancel carry file, line, and cause");
  const city = "Braga";
  const spawnLines = [
    programLine(QUOTES, "let fast = spawn"),
    programLine(QUOTES, "let medium = spawn"),
    programLine(QUOTES, "let slow = spawn"),
  ];
  const id = (await post("local", "/api/runs", { function: "durable_race", args: { city } })).json.id;
  const calls = [1, 2, 3].map((n) => `${id}-c${n}`);

  // While the three children run, the record of the parent names every call site.
  await waitFor("three dispatches", () => eventsOf(id, "remote_dispatched").length === 3, 20000);
  await waitFor("three waiting_on entries", async () => (await get("local", `/api/runs/${id}`)).json.waiting_on.length === 3, 20000);
  const waiting = (await get("local", `/api/runs/${id}`)).json;
  const siteOf = (list) => list.map((e) => [e.call_id, e.file, e.line]).sort();
  const want = calls.map((callId, i) => [callId, QUOTES, spawnLines[i]]).sort();
  check("waiting_on records the call site of each entry", isDeepStrictEqual(siteOf(waiting.waiting_on), want), JSON.stringify(siteOf(waiting.waiting_on)));
  check("calls records the call site of each entry", isDeepStrictEqual(siteOf(waiting.calls), want), JSON.stringify(siteOf(waiting.calls)));

  const spawned = eventsOf(id, "thread_started");
  check("remote_call carries the call site, unchanged from the worker", isDeepStrictEqual(siteOf(eventsOf(id, "remote_call")), want), JSON.stringify(siteOf(eventsOf(id, "remote_call"))));
  check("thread_started carries the spawn site, and the root thread of a segment carries none",
    spawned.filter((e) => e.parent_thread === null).every((e) => e.file === null && e.line === null)
    && spawnLines.every((line) => spawned.some((e) => e.parent_thread === 1 && e.file === QUOTES && e.line === line)),
    JSON.stringify(spawned.map((e) => [e.thread, e.parent_thread, e.file, e.line])));

  // The race cancels the two losers: `future_cancel`, with the call site of the cancelled call.
  const done = await waitStatus("local", id, "completed", 40000);
  const cancels = eventsOf(id, "remote_cancel");
  check("remote_cancel names the cause future_cancel and repeats the call site of the cancelled call",
    eachOnce(cancels, "call_id", [calls[1], calls[2]]) && cancels.every((e) => e.cause === "future_cancel")
    && isDeepStrictEqual(siteOf(cancels), [[calls[1], QUOTES, spawnLines[1]], [calls[2], QUOTES, spawnLines[2]]].sort()),
    JSON.stringify(cancels.map((e) => [e.call_id, e.cause, e.file, e.line])));
  check("the completed run keeps every call site in `calls`", isDeepStrictEqual(siteOf(done.calls), want), JSON.stringify(siteOf(done.calls)));

  // Contract section 10.3: the panel names "the calling function". The only
  // evidence is what the worker reported, so it travels with the event and
  // with the record. `function` is the callee; `caller` is the function the
  // call is written in.
  const callers = eventsOf(id, "remote_call").map((e) => e.caller);
  check("remote_call names the calling function", isDeepStrictEqual(callers, ["durable_race", "durable_race", "durable_race"]), JSON.stringify(callers));
  check("waiting_on and calls keep the calling function, so it survives a resume and a reload",
    done.calls.every((c) => c.caller === "durable_race") && done.calls.every((c) => c.function === "remote_get_quote"),
    JSON.stringify(done.calls.map((c) => [c.call_id, c.function, c.caller])));

  // The same list comes back from events.jsonl, which is what a page reload reads.
  const history = (await get("local", `/api/runs/${id}/events`)).json;
  check("events.jsonl keeps file, line, and cause, so a page reload sees them",
    isDeepStrictEqual(siteOf(history.filter((e) => e.type === "remote_call")), want)
    && history.filter((e) => e.type === "remote_cancel").every((e) => e.cause === "future_cancel" && e.file === QUOTES)
    && history.some((e) => e.type === "thread_started" && e.file === QUOTES && e.line === spawnLines[0])
    && history.filter((e) => e.type === "remote_call").every((e) => e.caller === "durable_race"),
    JSON.stringify(history.filter((e) => e.type === "remote_cancel").map((e) => [e.call_id, e.cause, e.file, e.line])));

  // A resume: the call site is read back from `meta.json` and travels with the run.
  // The call is inside the closure that `with_timeout` runs, one line below it.
  const timeout = programLine(QUOTES, "QuoteKind.Tour, 6000");
  const deadlineId = (await post("local", "/api/runs", { function: "durable_deadline", args: { city } })).json.id;
  await waitFor("the deadline run to dispatch its call", () => eventsOf(deadlineId, "remote_dispatched").length === 1, 20000);
  await post("local", `/api/runs/${deadlineId}/pause`);
  await waitStatus("local", deadlineId, "paused", 20000);
  const parked = (await get("local", `/api/runs/${deadlineId}`)).json;
  check("a paused run keeps the call site in waiting_on", isDeepStrictEqual(siteOf(parked.waiting_on), [[`${deadlineId}-c1`, QUOTES, timeout]]),
    JSON.stringify(siteOf(parked.waiting_on)));
  await post("local", `/api/runs/${deadlineId}/resume`, {});
  const deadlineDone = await waitStatus("local", deadlineId, "completed", 40000);
  const timeoutCancel = eventsOf(deadlineId, "remote_cancel");
  check("with_timeout reports the cause token, and the call site survived the resume",
    timeoutCancel.length === 1 && timeoutCancel[0].cause === "token" && timeoutCancel[0].file === QUOTES && timeoutCancel[0].line === timeout
    && deadlineDone.calls.every((c) => c.file === QUOTES && c.line === timeout),
    JSON.stringify(timeoutCancel.map((e) => [e.call_id, e.cause, e.file, e.line])));
  // Section 10.3: the resumed process announces `remote_wait`, not `remote_call`,
  // so the calling function of that call exists only on the record.
  check("the calling function survived the resume on the run record",
    deadlineDone.calls.every((c) => c.caller === "durable_deadline")
    && deadlineDone.calls.every((c) => c.function === "remote_get_quote"),
    JSON.stringify(deadlineDone.calls.map((c) => [c.call_id, c.function, c.caller])));

  // `baml.future.all` drops the inputs that are still pending after an error by
  // cancelling their futures (`futures.map((g) -> { g.cancel() })`). The inputs
  // were spawned by the calling function, not by the helper thread that `all`
  // runs in, so the engine classifies this as `future_cancel`, not `parent`.
  // The engine test bex_engine::durable_cause
  // `an_input_that_all_drops_after_an_error_reports_a_cancelled_future` pins
  // that behavior against the real runtime.
  const failId = (await post("local", "/api/runs", { function: "durable_fan_out", args: { city, mock_nap_ms: 500, mock_unavailable: "Flight" } })).json.id;
  await waitStatus("local", failId, "failed", 60000);
  await waitFor("the failed fan-out to cancel its pending inputs", () => eventsOf(failId, "remote_cancel").length > 0, 20000);
  const failCancels = eventsOf(failId, "remote_cancel");
  check("the inputs that baml.future.all drops after an error report the cause future_cancel, with their call site",
    failCancels.every((e) => e.cause === "future_cancel" && e.file === QUOTES && typeof e.line === "number"),
    JSON.stringify(failCancels.map((e) => [e.call_id, e.cause, e.file, e.line])));

  // A worker that cannot attribute the call writes null, and the site server
  // stores null. A worker that cannot classify a cancellation writes `unknown`.
  const blindId = (await post("local", "/api/runs", {
    function: "durable_race",
    args: { city, mock_no_call_site: true, mock_cancel_cause: "unknown" },
  })).json.id;
  await waitFor("the blind run to dispatch", () => eventsOf(blindId, "remote_dispatched").length === 3, 20000);
  const blind = (await get("local", `/api/runs/${blindId}`)).json;
  check("a call without a location is recorded as null, not as a guess",
    blind.waiting_on.every((w) => w.file === null && w.line === null) && blind.calls.every((c) => c.file === null && c.line === null)
    && eventsOf(blindId, "remote_call").every((e) => e.file === null && e.line === null),
    JSON.stringify(blind.waiting_on.map((w) => [w.call_id, w.file, w.line])));
  await waitStatus("local", blindId, "completed", 40000);
  const unknown = eventsOf(blindId, "remote_cancel");
  check("a cancellation that the worker cannot classify reports the cause unknown and no location",
    unknown.length === 2 && unknown.every((e) => e.cause === "unknown" && e.file === null && e.line === null),
    JSON.stringify(unknown.map((e) => [e.call_id, e.cause, e.file, e.line])));
}

/**
 * `DELETE /api/runs` empties one site: the workers end, the sleep timers are
 * retired so that nothing is resumed afterwards, and the run store is deleted.
 * The program store is kept, so the next start still skips the compile.
 */
async function clearRunsScene() {
  const started = await api("local", "POST", "/api/runs", { function: "durable_nap", args: { seconds: 30 } });
  const napId = started.json?.id;
  await waitStatus("local", napId, ["sleeping"]);

  const before = await api("local", "GET", "/api/runs");
  check("there are runs to clear", (before.json ?? []).length > 0, `${(before.json ?? []).length}`);

  const cleared = await api("local", "DELETE", "/api/runs");
  check("DELETE /api/runs answers 200", cleared.status === 200, `${cleared.status}`);
  check("it reports how many it removed", (cleared.json?.cleared ?? 0) > 0, JSON.stringify(cleared.json));

  const after = await api("local", "GET", "/api/runs");
  check("the site holds no run afterwards", (after.json ?? []).length === 0, `${(after.json ?? []).length}`);
  const gone = await api("local", "GET", `/api/runs/${napId}`);
  check("a cleared run is a 404", gone.status === 404, `${gone.status}`);

  // The nap was sleeping, so a timer of its suspend was pending. A retired
  // timer must not bring the run back.
  await new Promise((resolve) => setTimeout(resolve, 1500));
  const still = await api("local", "GET", "/api/runs");
  check("the sleep timer of a cleared run does not resurrect it", (still.json ?? []).length === 0, `${(still.json ?? []).length}`);

  const fresh = await api("local", "POST", "/api/runs", { function: "durable_plan_trip", args: { city: "Faro" } });
  check("a run started after the clear works", fresh.status === 200, `${fresh.status}`);
  const done = await waitStatus("local", fresh.json?.id, ["completed", "failed"]);
  check("and it completes", done?.status === "completed", `${done?.status}`);
  for (const site of SITE_NAMES) await api(site, "DELETE", "/api/runs");
}

async function scene(name, fn) {
  try {
    await fn();
  } catch (err) {
    check(`${name} ran to its end`, false, String(err.message ?? err));
  }
}

async function restartAll(extraEnv, wipePrograms) {
  for (const site of SITE_NAMES) STREAMS[site]?.close();
  await Promise.all(SITE_NAMES.map(stopServer));
  for (const site of wipePrograms) fs.rmSync(path.join(SERVER_DIR, SITES[site].programs), { recursive: true, force: true });
  await Promise.all(SITE_NAMES.map((site) => startServer(site, extraEnv)));
  for (const site of SITE_NAMES) {
    STREAMS[site] = new Stream(site);
    await STREAMS[site].open();
  }
}

async function chaosScenes() {
  section("CHAOS: the same scenes with slow dispatch, slow results, slow cancel, slow import, slow program fetch, duplicated and reordered results");
  speed = 1;
  await restartAll({ CHAOS: JSON.stringify(CHAOS), MOCK_SPEED: "1" }, ["cloud", "cloud2"]);
  const info = (await get("cloud", "/api/info")).json;
  check("CHAOS: info shows the knobs", info.chaos.dispatch_delay_ms === CHAOS.dispatch_delay_ms && info.chaos.duplicate_results === true && info.chaos.reorder_results === true, JSON.stringify(info.chaos));
  const L = "CHAOS: ";
  await scene("CHAOS: program fetch and all_settled", () => programFetchAndSettled(L));
  await scene("CHAOS: fan-out", () => fanOutScene(L, true));
  await Promise.all([scene("CHAOS: nap", () => napScene(L)), scene("CHAOS: race", () => raceScene(L)), scene("CHAOS: deadline", () => deadlineScene(L)),
    scene("CHAOS: migration of a sleeping run", () => sleepingMigrationScene(L))]);
  await scene("CHAOS: cascade", () => cascadeScene(L));
  const dup = allEvents().filter((e) => e.type === "remote_returned");
  check("CHAOS: although every result was sent twice, no call returned twice on any run", [...countBy(dup, "call_id").values()].every((n) => n === 1), JSON.stringify([...countBy(dup, "call_id")].filter(([, n]) => n > 1)));

  // A controller that is slower than the program: the dispatch takes longer
  // than the deadline of the call, so remote_cancel arrives while the dispatch
  // is in flight.
  section("CHAOS: a dispatch that is slower than the deadline of the call");
  STREAMS.local.close();
  await stopServer("local");
  await startServer("local", { CHAOS: JSON.stringify({ dispatch_delay_ms: 3500 }), MOCK_SPEED: "1" });
  STREAMS.local = new Stream("local");
  await STREAMS.local.open();
  const id = (await post("local", "/api/runs", { function: "durable_deadline", args: { city: "Sines" } })).json.id;
  const done = await waitStatus("local", id, "completed", 40000);
  const atCompletion = eventsOf(id, "remote_dispatched").length;
  check("the run completes with the Timeout message before the child even exists", done.result === "no tour quote for Sines: operation timed out after 2000ms" && atCompletion === 0, `${atCompletion}`);
  const dispatched = await STREAMS.local.waitEvent("remote_dispatched", (e) => e.type === "remote_dispatched" && e.run === id, 20000);
  const cancelledEv = await STREAMS.local.waitEvent("remote_cancelled", (e) => e.type === "remote_cancelled" && e.run === id, 20000);
  const kid = await waitStatus(dispatched.child_site, dispatched.child_run, "cancelled", 20000);
  const record = (await get("local", `/api/runs/${id}`)).json;
  check("the child that appeared after the cancellation is cancelled right away and never recorded in waiting_on", kid.status === "cancelled" && cancelledEv.child_run === dispatched.child_run
    && STREAMS.local.events.indexOf(cancelledEv) > STREAMS.local.events.indexOf(dispatched) && record.waiting_on.length === 0 && record.remote_results.length === 0,
    JSON.stringify(record.waiting_on));
  // The same slow controller under a fan-out: the parent suspends before any
  // child exists, and the run still completes correctly.
  await scene("CHAOS, slow dispatch: fan-out", () => fanOutScene("CHAOS, slow dispatch: ", true));
  await scene("CHAOS, slow dispatch: fork before the dispatches return", forkDuringDispatchScene);
  await scene("CHAOS: a program fetch that is slower than the dispatch request", slowFetchScene);
}

/** local still has `dispatch_delay_ms: 3500`: the run sleeps before any child exists, and a fork taken then knows no child. */
async function forkDuringDispatchScene() {
  section("CHAOS, slow dispatch: a fork taken before the dispatches returned");
  const id = (await post("local", "/api/runs", { function: "durable_fan_out", args: { city: "Lisbon", mock_nap_ms: 13000 } })).json.id;
  await waitStatus("local", id, "sleeping");
  const fork = (await post("local", `/api/runs/${id}/fork`)).json;
  check("the fork was taken while no dispatch had returned: its record lists no child and no result", fork.waiting_on.length === 0 && fork.remote_results.length === 0
    && fork.calls.length === 4, JSON.stringify([fork.waiting_on.length, fork.calls.length]));
  const done = await waitStatus("local", id, "completed", 60000);
  const resumed = await post("local", `/api/runs/${fork.id}/resume`);
  const forkDone = await waitStatus("local", fork.id, "completed", 60000);
  check("the resumed fork reports its four restored waits and completes with the quotes of its source, without new children", resumed.status === 200
    && eventsOf(fork.id, "remote_wait").length === 4 && eventsOf(fork.id, "remote_wait").every((w) => w.has_result === false)
    && isDeepStrictEqual(forkDone.result, done.result) && (await childrenOf(fork.id)).length === 0 && (await childrenOf(id)).length === 4,
    JSON.stringify(forkDone.result).slice(0, 120));

  // A fork that resumes while the children of its source still run attaches
  // itself to them.
  const id2 = (await post("local", "/api/runs", { function: "durable_fan_out", args: { city: "Lisbon", mock_nap_ms: 13000 } })).json.id;
  await waitStatus("local", id2, "sleeping");
  const early = (await post("local", `/api/runs/${id2}/fork`)).json;
  await waitFor("four dispatches of the source", () => eventsOf(id2, "remote_dispatched").length === 4, 30000);
  await post("local", `/api/runs/${early.id}/resume`);
  const attached = await waitFor("the fork attaches to the running children", () => eventsOf(early.id, "remote_attached").length + eventsOf(early.id, "remote_returned").length >= 4 ? true : null, 30000);
  const earlyDone = await waitStatus("local", early.id, "completed", 60000);
  const sourceDone = await waitStatus("local", id2, "completed", 60000);
  check("a fork that resumes while the children run attaches to them: both runs complete with the same quotes from four children", attached === true
    && isDeepStrictEqual(earlyDone.result, sourceDone.result) && (await childrenOf(id2)).length === 4 && (await childrenOf(early.id)).length === 0);
}

/** The first use of a program on a site takes longer than the 10 s that a dispatch request waits. */
async function slowFetchScene() {
  section("CHAOS: a program fetch of 12 s on first use");
  await restartAll({ CHAOS: JSON.stringify({ program_fetch_delay_ms: 12000 }), MOCK_SPEED: "1" }, ["cloud", "cloud2"]);
  const started = Date.now();
  const id = (await post("local", "/api/runs", { function: "plan_trip", args: { city: "Tomar" } })).json.id;
  const done = await waitStatus("local", id, "completed", 90000);
  const kids = await childrenOf(id);
  const fetched = allEvents().filter((e) => e.type === "program_fetched" && e.at_verify >= started);
  check("the parent completes although the child's site needed 12 s to fetch the program: one child, one fetch, no failed call", done.status === "completed"
    && kids.length === 1 && kids[0].status === "completed" && eventsOf(id, "remote_dispatched").length === 1
    && allEvents().filter((e) => e.type === "program_fetched" && e.site === kids[0].site && e.ts >= started).length === 1, JSON.stringify([kids.length, fetched.length, done.error]));
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
    this.programs = new Map();         // program hash -> the body of GET /api/programs/:hash
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
    const program = /^\/api\/programs\/([0-9a-f]+)$/.exec(req.url);
    if (program) {
      const served = this.programs.get(program[1]);
      return served ? reply(200, served) : reply(404, { error: "the scripted site does not hold this program" });
    }
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

    // 6. Program store (contract section 9.5). `slow` sends remote calls for a
    //    program that cloud2 never saw. cloud2 fetches it from `slow`, verifies
    //    the hash, stores it, and runs it without a project directory.
    const sha = (bytes) => crypto.createHash("sha256").update(bytes).digest("hex");
    const foreign = Buffer.from(JSON.stringify({ mock_program: 1, files: [{ name: "baml_src/elsewhere.baml", text: "function remote_fetch_weather(city: string) -> string {\n    \"written on another machine\"\n}\n" }] }));
    const goodHash = sha(foreign);
    const wrongHash = sha(Buffer.from("these are not the bytes that were promised"));
    const absentHash = sha(Buffer.from("a program that no site holds"));
    slow.programs.set(goodHash, { hash: goodHash, runtime_build: "mock-worker/1", program_base64: foreign.toString("base64") });
    slow.programs.set(wrongHash, { hash: wrongHash, runtime_build: "mock-worker/1", program_base64: foreign.toString("base64") });
    const remoteRun = (hash, call) => post("cloud2", "/api/remote/runs", {
      function: "remote_fetch_weather", args: { city: "Pinhel" }, parent: { site: "slow", run: "r-far", call_id: `r-far-c${call}` }, program_hash: hash, from_site: "slow",
    });
    const countRuns = async () => (await get("cloud2", "/api/runs")).json.length;
    const runsBefore = await countRuns();
    // The dispatch answers at once, and the program is fetched before the
    // worker starts. A fetch that fails ends the child as `failed`, and the
    // parent's site gets the reason as the result of the call.
    const failedChild = async (res, call) => {
      const run = await waitStatus("cloud2", res.json.id, "failed", 20000);
      const told = await waitFor("the failure reaches the parent's site", () => slow.seen((r) => r.path === "/api/runs/r-far/remote_result" && r.body?.call_id === `r-far-c${call}`)[0], 20000);
      return { run, told };
    };
    const mismatch = await remoteRun(wrongHash, 1);
    const mismatchEnd = mismatch.status === 200 ? await failedChild(mismatch, 1) : null;
    check("a fetched program whose bytes do not have the requested hash is rejected: the child fails with the reason, nothing is stored, no worker starts, and the parent is told",
      mismatch.status === 200 && mismatchEnd.run.error.includes(wrongHash) && String(mismatchEnd.told.body.error ?? "").includes(wrongHash)
      && !fs.existsSync(storeFile("cloud2", wrongHash)) && (await get("cloud2", `/api/runs/${mismatch.json.id}/events`)).json.every((e) => e.type !== "hello")
      && slow.seen((r) => r.path === `/api/programs/${wrongHash}`).length === 1, `${mismatch.status} ${mismatch.text}`);
    const absent = await remoteRun(absentHash, 2);
    const absentEnd = absent.status === 200 ? await failedChild(absent, 2) : null;
    check("a program that the sending site does not serve: the child fails with the reason, and no worker starts", absent.status === 200
      && absentEnd.run.error.includes("did not serve the program") && String(absentEnd.told.body.error ?? "").includes(absentHash), `${absent.status} ${absent.text}`);
    const notAHash = await remoteRun("../../etc/passwd", 3);
    check("a program_hash that is not 64 hex characters is refused with 400 before anything is fetched or recorded", notAHash.status === 400 && (await countRuns()) === runsBefore + 2
      && slow.seen((r) => r.path.startsWith("/api/programs/") && r.path.includes("passwd")).length === 0, `${notAHash.status} ${notAHash.text}`);

    // A peer's header bytes are not trusted: only the program bytes are covered
    // by the hash. The entry gets a header that cloud2 builds itself.
    const garbage = Buffer.from(JSON.stringify({ mock_program: 1, files: [{ name: "baml_src/garbage.baml", text: "function remote_fetch_weather(city: string) -> string {\n    \"served with a garbage header\"\n}\n" }] }));
    const garbageHash = sha(garbage);
    slow.programs.set(garbageHash, { hash: garbageHash, runtime_build: "mock-worker/1", program_base64: garbage.toString("base64"), header_base64: Buffer.from("XXXX").toString("base64"), format_version: 7 });
    const withGarbage = await remoteRun(garbageHash, 6);
    const garbageDone = await waitStatus("cloud2", withGarbage.json.id, "completed", 20000);
    const garbageEntry = fs.readFileSync(storeFile("cloud2", garbageHash));
    check("a peer that serves the right bytes with a garbage header: the stored entry has the documented layout and a worker runs it", garbageDone.result === "sunny in Pinhel"
      && garbageEntry.subarray(0, 8).toString("latin1") === "BAMLPROG" && garbageEntry.readUInt32LE(8) === 1
      && garbageEntry.subarray(16, 16 + garbageEntry.readUInt32LE(12)).toString("utf8") === "mock-worker/1"
      && sha(garbageEntry.subarray(24 + garbageEntry.readUInt32LE(12))) === garbageHash);
    // cloud2 knows the build of its workers by now (from `hello`). A program of
    // another build would be refused by every worker here, so the site refuses
    // it and says why.
    const evil = Buffer.from(JSON.stringify({ mock_program: 1, files: [{ name: "baml_src/evil.baml", text: "function remote_fetch_weather(city: string) -> string {\n    \"another build\"\n}\n" }] }));
    const evilHash = sha(evil);
    slow.programs.set(evilHash, { hash: evilHash, runtime_build: "evil/1", program_base64: evil.toString("base64") });
    const otherBuild = await remoteRun(evilHash, 7);
    const otherBuildEnd = otherBuild.status === 200 ? await failedChild(otherBuild, 7) : null;
    check("a program of another runtime build is refused with both build names, and nothing is stored", otherBuild.status === 200
      && otherBuildEnd.run.error.includes("evil/1") && otherBuildEnd.run.error.includes("mock-worker/1") && !fs.existsSync(storeFile("cloud2", evilHash)), otherBuildEnd?.run.error);
    const noBuild = Buffer.from(JSON.stringify({ mock_program: 1, files: [] }));
    slow.programs.set(sha(noBuild), { hash: sha(noBuild), runtime_build: "", program_base64: noBuild.toString("base64") });
    const unnamed = await remoteRun(sha(noBuild), 8);
    const unnamedEnd = unnamed.status === 200 ? await failedChild(unnamed, 8) : null;
    check("a program without a usable runtime build is not stored", unnamed.status === 200 && unnamedEnd.run.error.includes("without a usable runtime build")
      && !fs.existsSync(storeFile("cloud2", sha(noBuild))), unnamedEnd?.run.error);

    const accepted = await remoteRun(goodHash, 4);
    check("a program with the right hash: the run is accepted with that program_hash", accepted.status === 200 && accepted.json.program_hash === goodHash, `${accepted.status} ${accepted.text}`);
    await waitFor("the fetched entry", () => fs.existsSync(storeFile("cloud2", goodHash)), 20000);
    const entry = fs.readFileSync(storeFile("cloud2", goodHash));
    check("the stored entry has the header of the documented layout and the program bytes after it", entry.subarray(0, 8).toString("latin1") === "BAMLPROG"
      && entry.readUInt32LE(8) === 1 && entry.subarray(16, 16 + entry.readUInt32LE(12)).toString("utf8") === "mock-worker/1"
      && Number(entry.readBigUInt64LE(16 + entry.readUInt32LE(12))) === entry.length - 24 - entry.readUInt32LE(12)
      && sha(entry.subarray(24 + entry.readUInt32LE(12))) === goodHash);
    check("no temporary file is left in the shard of a stored entry, and none after a refused one", fs.readdirSync(path.dirname(storeFile("cloud2", goodHash))).every((name) => !name.includes(".tmp-")));
    stream = new Stream("cloud2");
    await stream.open();
    const foreignDone = await waitStatus("cloud2", accepted.json.id, "completed");
    const foreignHello = (await get("cloud2", `/api/runs/${accepted.json.id}/events`)).json.find((e) => e.type === "hello");
    check("cloud2 ran a program that it never compiled: the worker loaded it from the store by hash, without --project", foreignDone.result === "sunny in Pinhel"
      && foreignHello.program_hash === goodHash && foreignHello.mock_program_source === "store" && foreignHello.mock_project === null, JSON.stringify(foreignHello));
    const secondUse = await remoteRun(goodHash, 5);
    check("second use: the program is not fetched again", secondUse.status === 200 && slow.seen((r) => r.path === `/api/programs/${goodHash}`).length === 1);
    await waitStatus("cloud2", secondUse.json.id, "completed");
    stream.close();
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
  Object.assign(STREAMS, { local, cloud, cloud2 });
  section("SSE");
  check("content-type is text/event-stream", local.contentType === "text/event-stream");
  check("init is the first message on both streams", local.events[0].type === "init" && local.events[0].site === "local"
    && Array.isArray(local.events[0].runs) && cloud.events[0].type === "init" && cloud.events[0].site === "cloud"
    && cloud2.events[0].type === "init" && cloud2.events[0].site === "cloud2");

  if (ONLY === "chaos") {
    await scene("CHAOS", chaosScenes);
    for (const site of SITE_NAMES) STREAMS[site]?.close();
    return;
  }
  await routesAndErrors();
  await programRoutes();
  await scene("program store rules of the worker", mockStoreRules);
  // The first remote calls of this instance: cloud and cloud2 fetch the program.
  await scene("program fetch and all_settled", () => programFetchAndSettled(""));
  if (ONLY !== "phase3") await earlierScenes(local, cloud, cloud2);

  // Contract section 9. Scenes that make no placement check run side by side.
  await Promise.all([scene("nap", () => napScene("")), scene("manual resume", () => manualResumeScene("")), scene("resume race", () => resumeRaceScene("")),
    scene("cancel without a process", () => cancelSleepingScene("")), scene("pause with four threads", () => pauseRaceScene(""))]);
  await scene("fan-out", () => fanOutScene(""));
  await Promise.all([scene("race", () => raceScene("")), scene("deadline", () => deadlineScene("")), scene("fan-out failure", () => fanOutFailureScene("")),
    scene("migration of a sleeping run", () => sleepingMigrationScene(""))]);
  await scene("cascade", () => cascadeScene(""));
  await Promise.all([scene("late commands", lateCommandScene), scene("lost store entry", lostEntryScene)]);
  await scene("shared children", sharedChildrenScene);
  await scene("restored waits", restoredWaitScene);
  await scene("call sites", callSiteScene);
  await invariants(local, cloud, cloud2, t0);

  if (ONLY !== "phase3") await restartScenario();
  await scene("restart while sleeping", restartWhileSleepingScene);
  cloud.close();
  cloud2.close();
  if (ONLY !== "phase3") await noRemoteSite();
  await routingScene();
  await scene("clear runs", clearRunsScene);
  await scene("CHAOS", chaosScenes);
  for (const site of SITE_NAMES) STREAMS[site]?.close();
}

// The scenes of contract sections 3 to 8.
async function earlierScenes(local, cloud, cloud2) {
  await centralScene(local);
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
}

async function invariants(local, cloud, cloud2, t0) {
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
  const wanted = ["hello", "log", "position", "thread_started", "thread_ended", "remote_call", "remote_result_received", "paused", "resumed", "completed", "failed",
    "init", "run", "remote_dispatched", "remote_returned", "migrated_out", "migrated_in",
    // Contract section 7. Some of them only occur in the scenes of sections 3 to 8.
    ...(ONLY === "phase3" ? ["cancelled", "worker_exit"] : ["cancelled", "snapshot", "worker_exit", "forked", "pausing", "blocked"]),
    // Contract section 9, and the server events that this implementation adds.
    "sleep_scheduled", "woken", "remote_cancel", "remote_cancelled", "remote_result_discarded", "run_cancelled", "program_fetched"];
  check("every contract event type was seen on the streams", wanted.every((t) => seen.has(t)), wanted.filter((t) => !seen.has(t)).join(","));
  local.close();
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
