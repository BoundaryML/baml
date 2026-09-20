# Durable functions demo

This directory holds the proof of concept for durable BAML functions. The
design is in `documents/durable-functions-design.md`, and the API agreement
between the components is in `documents/durable-poc-contracts.md`.

| Path | Content |
|---|---|
| `server/` | The site server, a BAML program. Three instances run side by side: `local` on port 8787, `cloud` on port 8788, and `cloud2` on port 8789. |
| `program/` | The demo program that the workers run (`baml_src/trip.baml`). |
| `scripts/mock-worker.mjs` | A mock worker (Node 22, no dependencies) that speaks the worker protocol and simulates the demo program. |
| `scripts/dev.sh` | Starts the three site servers and stops them on exit. |
| `scripts/verify.mjs` | An end-to-end check of the three site servers with the mock worker. |
| `scripts/verify-real.mjs` | An end-to-end check of the three site servers with the real worker (`baml-cli worker`). It also prints the measured pause and resume numbers. |
| `web/` | The React app (TypeScript, Vite, pnpm). |

## Requirements

- The `baml` CLI, toolchain 0.19 or newer (`baml --version`).
- Node 22 for the mock worker and `verify.mjs`.
- pnpm for the web app.

## Run the demo

With the real worker (pause, snapshot, and resume are done by the BEX runtime):

```bash
cd baml_language
cargo build -p baml_cli                       # builds target/debug/baml-cli
cd demo_durable
WORKER_CMD="[\"$PWD/../target/debug/baml-cli\",\"worker\"]" CLEAN=1 ./scripts/dev.sh
(cd web && pnpm install && pnpm dev)          # second terminal; the app on http://localhost:5173
```

Ports 8787, 8788, and 8789 must be free. On macOS the first execution of a freshly
linked `baml-cli` takes several seconds (8 s was measured once), so the first
run after a build starts late. Run `BAML_CLI_ALLOW_DIRECT=1
../target/debug/baml-cli --version` once to get that out of the way.

The scenes to show in the app:

1. Pick `durable_plan_trip` and press Start. After three loop iterations the
   run calls `remote_fetch_weather`, which starts on `cloud` (see "Sites and
   placement of remote calls" below).
2. Press Pause while the cloud run is active. The local worker writes
   `snap-<n>.bamlsnap` and exits with code 75. The state panel shows `city`,
   `ideas`, `day`, and `weather` as they are at line 29. The cloud run keeps
   logging and completes. Its result is stored on the paused run.
3. Press "Resume here". A new process with a new pid starts from the snapshot,
   takes the stored result, and completes. "Resume on cloud" or "Resume on
   cloud2" moves the snapshot to that site instead. A run that is paused
   inside its loop and resumed on `cloud` makes its remote call from there, so
   the child runs on `cloud2`.
4. Start `durable_plan_trip` again and press "Kill process" after the first
   hollow snapshot marker. The run becomes `paused` and can be resumed. A
   killed `plan_trip` run is `lost`.
5. Start `durable_plan_trip_parallel` and press Pause once the spawned thread
   appears. The status stays `pausing`, and the reason
   ("the run holds a future ...") appears under it. The run still completes.

With the mock worker (no Rust build needed):

```bash
cd baml_language/demo_durable
./scripts/dev.sh                 # site servers on 8787 (local), 8788 (cloud), and 8789 (cloud2)
(cd web && pnpm install && pnpm dev)   # the app on http://localhost:5173
```

`CLEAN=1 ./scripts/dev.sh` deletes the three run stores of this instance
first. Ctrl-C stops the three servers. A worker exits when its site server
closes the worker's stdin, so no process is left behind.

### A second instance next to the first one

`PORT_BASE` sets the first port, and the three sites use `PORT_BASE`,
`PORT_BASE + 1`, and `PORT_BASE + 2`. `RUNS_TAG` is added to the names of the
run stores (`server/.baml/runs<RUNS_TAG>-<site>`). With both variables a second
instance runs next to the first one and shares nothing with it:

```bash
PORT_BASE=18787 RUNS_TAG=-test CLEAN=1 ./scripts/dev.sh
# run stores: server/.baml/runs-test-local, runs-test-cloud, runs-test-cloud2
(cd web && LOCAL_SITE_URL=http://127.0.0.1:18787 CLOUD_SITE_URL=http://127.0.0.1:18788 \
  CLOUD2_SITE_URL=http://127.0.0.1:18789 WEB_PORT=15173 pnpm dev)
```

The cleanup of `dev.sh` is scoped to its own instance. It ends the three
servers that it started by pid. It ends a worker only when the worker's
`--snapshot-dir` lies in one of this instance's three run stores. It ends a
listener on one of its three ports only when that process is a server that
this script started. `CLEAN=1` deletes only this instance's run stores.
`dev.sh` refuses to start when one of its ports is in use, and it touches no
process in that case.

Two instances must not share a run store. Both would write the same
`meta.json` files, `CLEAN=1` of one would delete the runs of the other, and the
cleanup of one would end the workers of the other. `dev.sh` prevents this in
two ways:

- When `RUNS_TAG` is not set and `PORT_BASE` is not 8787, the tag defaults to
  `-p<PORT_BASE>`. `PORT_BASE=18787 ./scripts/dev.sh` therefore uses
  `runs-p18787-<site>` and not the run stores of the default instance. An
  explicit `RUNS_TAG`, also an empty one, is used as given.
- The script claims each run store with the file `<store>/.instance.lock`,
  which holds its pid, and removes the file when it stops. It refuses to start
  when a run store is claimed by a `dev.sh` process that is alive, or when a
  worker process already runs on the store (this also detects an instance that
  was started by a `dev.sh` without lock files). Both checks run before
  `CLEAN=1` deletes anything and before the cleanup trap is installed. The
  site server ignores the lock file when it loads the run store.

### One site server by hand

```bash
cd baml_language/demo_durable/server
SITE=local  RUNS_DIR=.baml/runs-local  baml run main
SITE=cloud  RUNS_DIR=.baml/runs-cloud  baml run main    # second terminal
SITE=cloud2 RUNS_DIR=.baml/runs-cloud2 baml run main    # third terminal
```

Each server takes its port from its own entry in `SITES`, so the default
registry needs no `PORT`.

Inside a coding agent session the `baml` CLI refuses to run until the BAML
agent skill is installed. `dev.sh`, `verify.mjs`, and `verify-real.mjs` set
`BAML_AGENT_SKILL_CHECK=off` for the servers they start. Set the variable
yourself when you start a server by hand from such a session.

### The central scene with curl

```bash
L=http://127.0.0.1:8787; C=http://127.0.0.1:8788; C2=http://127.0.0.1:8789
curl -sN $L/api/events &          # event stream of the local site
curl -sN $C/api/events &          # event stream of the cloud site
curl -sN $C2/api/events &         # event stream of the cloud2 site

# Start the parent on local. After three loop iterations it calls
# remote_fetch_weather, which runs on cloud.
curl -s -X POST $L/api/runs -d '{"function":"durable_plan_trip","args":{"city":"Lisbon"}}'
# => {"id":"r-7k2m9x", ...}

# Wait for the `remote_dispatched` event, then pause the parent. Its worker
# writes a snapshot and exits (status `paused`, `pid: null`).
curl -s -X POST $L/api/runs/r-7k2m9x/pause

# The child keeps running on cloud. When it completes, cloud posts the result
# to local, which stores it in the parent's meta.json (`remote_results`).
curl -s $L/api/runs/r-7k2m9x

# Resume: a new worker process starts from the snapshot, receives the stored
# result with --remote-result, and completes.
curl -s -X POST $L/api/runs/r-7k2m9x/resume
# Or migrate: the snapshot moves to the named site and the run continues there.
# Any site of the registry is accepted. An unknown name returns 400.
curl -s -X POST $L/api/runs/r-7k2m9x/resume -d '{"site":"cloud"}'
# The local record is now `migrated` with `migrated_to: "cloud"`. A paused run
# on cloud can move on in the same way:
curl -s -X POST $C/api/runs/r-7k2m9x/resume -d '{"site":"cloud2"}'
```

## Sites and placement of remote calls

Contract section 8 specifies this part. Every site server receives the same
registry in `SITES`, a JSON object that maps each site name to its base URL.
`REMOTE_POOL` is a JSON array of the site names that may host `remote_` calls.
The default pool is `["cloud","cloud2"]`. `local` is not in the pool, so a
remote call never runs on the local site.

A `remote_` call always runs as its own run in its own worker process, on a
site of the pool that is not the caller's site. The caller's site server picks
the target when its worker reports `remote_call`:

1. It takes `REMOTE_POOL` in order and removes its own site.
2. It picks the first site that remains.

| The caller runs on | The remote call runs on |
|---|---|
| `local` | `cloud` |
| `cloud` | `cloud2` |
| `cloud2` | `cloud` |

When the pool has no other site, the call fails with the error
`no remote site available`. The site server delivers the error to the worker
as a `remote_result`, so the user code sees a throw
(`baml.errors.Io` with the message "remote call to ... failed: no remote site
available") and the run ends as `failed`. It does not hang. A pool member that
has no entry in `SITES` is ignored with a warning at startup.

Callbacks use the registry too. A child run stores `parent.site`, and its site
server posts the result to the URL of that site from `SITES`. A run that
migrates leaves a record with status `migrated` and `migrated_to: <site>`
behind. A `remote_result` that reaches such a record is forwarded to the site
in `migrated_to`. The next site does the same, so a result follows a chain
such as `local` -> `cloud` -> `cloud2` until it reaches the site that holds
the run. The HTTP answer of each hop is `{ok: true, forwarded_to: <site>}`. A
`migrated` record that an older server wrote without `migrated_to`, or whose
`migrated_to` names no site of the registry, is answered by trying every other
site of the registry.

A forwarded `remote_result` body carries the field `via`, the list of the sites
that the result already passed. Each site adds its own name before it forwards.
A site holds one record per run, so a chain never visits a site twice. A site
that finds its own name in `via`, or as many names as the registry has sites,
answers 502 and does not forward. The fallback list skips the sites in `via`.
This bounds the two cases in which two `migrated` records name each other: a
run that returns to a site while the import is still in flight, and a
`migrated_to` that names a site that was removed from `SITES`. The sender of
the result treats 502 like an unreachable site and tries again after 1.5 s.

`resume {site}` sets `status: "migrated"` and `migrated_to` on the record before
it reads the snapshot, and it restores `paused` and `migrated_to: null` when the
other site refuses the import. A result that arrives during the transfer is
therefore forwarded to the destination, which answers 404 until the import is
done, and the sender retries. A second `resume` request during the transfer
finds a run that is not `paused` and gets 409. `POST /api/runs/import` reserves
the run id before it writes files, and a second import of the same id gets 409
during that time.

`resume {site}` is refused with 409 while the dispatch of a remote call of the
run is in flight (the `POST /api/remote/runs` to the pool site has not
answered). The child is recorded in `waiting_on` when that request returns. If
the run left earlier, the record that travels would not name the child, and the
new site would dispatch the same `call_id` a second time.

`POST /api/remote/runs` returns 400 when `parent.site` has no entry in `SITES`,
because the result of such a child could never be delivered. The caller's site
turns the refusal into a `remote_result` with an error, so the user code sees a
failed call.

## Environment variables of the site server

| Variable | Meaning | Default |
|---|---|---|
| `SITE` | the name of this site, one of the names in `SITES` | `local` |
| `SITES` | the site registry: a JSON object that maps each site name (`[a-z0-9]+`) to its base URL. Every site server gets the same value. | `{"local":"http://127.0.0.1:8787","cloud":"http://127.0.0.1:8788","cloud2":"http://127.0.0.1:8789"}` |
| `REMOTE_POOL` | JSON array of the site names that may host `remote_` calls, in the order of preference | `["cloud","cloud2"]` |
| `PORT` | listen port on 127.0.0.1 | the port of this site's own entry in `SITES` |
| `WORKER_CMD` | JSON array, the worker command prefix | `["node","scripts/mock-worker.mjs"]` |
| `WORKER_CWD` | working directory of worker processes, relative to `server/` | `..` (this directory, so the default `WORKER_CMD` resolves) |
| `PROGRAM_DIR` | BAML project that workers run, relative to `server/` | `../program` |
| `RUNS_DIR` | run store root, relative to `server/` | `.baml/runs` |

The server passes absolute paths to the worker (`--project`,
`--snapshot-dir`, `--resume`), so the worker's working directory only matters
for a relative `WORKER_CMD`.

A worker process gets the environment of its site server plus
`BAML_CLI_ALLOW_DIRECT=1`. Without that variable `baml-cli worker` prints a
"using the internal BAML toolchain binary directly" warning on stderr at every
start, and the server forwards it as a `worker_stderr` log line.
`baml.sys.ProcessOptions.env` replaces the whole environment of a child
process, and the stdlib has no call that lists the variables. The server
therefore reads its own environment once at startup with `env -0` and passes
the full map. When that fails it logs a warning, and workers inherit the
environment unchanged.

`PEER_URL` is no longer read. `GET /api/info` returns the registry as
`sites: [{name, url}]` in the order of `SITES`, and the pool as `remote_pool`.
It no longer returns `peer_url` and `peer_site`.

`dev.sh` passes `WORKER_CMD`, `WORKER_CWD`, `PROGRAM_DIR`, and `REMOTE_POOL`
through to the three servers. It builds `SITES` from `PORT_BASE` and sets
`SITE`, `PORT`, and `RUNS_DIR` itself. `BAML_LOG` sets the log level of the
servers (default `warn`).

| Variable of `dev.sh` | Meaning | Default |
|---|---|---|
| `PORT_BASE` | first port. `local`, `cloud`, and `cloud2` use `PORT_BASE`, `PORT_BASE + 1`, and `PORT_BASE + 2`. | `8787` |
| `RUNS_TAG` | added to the run store names: `server/.baml/runs<RUNS_TAG>-<site>` | empty for `PORT_BASE=8787`, `-p<PORT_BASE>` otherwise |
| `CLEAN=1` | delete this instance's three run stores before the start | off |

### Switching to the real worker

The real worker is the hidden `worker` subcommand of `baml-cli`. Build it and
point `WORKER_CMD` at the binary with an absolute path:

```bash
cd baml_language && cargo build -p baml_cli
WORKER_CMD='["/abs/path/to/baml_language/target/debug/baml-cli","worker"]' ./demo_durable/scripts/dev.sh
```

The site server appends the contract arguments to that prefix:

```
--project <PROGRAM_DIR> --run <run-id> --segment <n> --snapshot-dir <RUNS_DIR>/<run-id>
( --start <function> --json-args '<json>' | --resume <snapshot-path> )
[--remote-result '<json>']...
```

What the site server expects from a worker (contract section 7.1):

- A `remote_result` command or a `--remote-result` argument can name a call id
  that the worker does not wait on, or whose result it already has. The worker
  must ignore it. This happens after a recovery from an older snapshot and for
  forks, where the server cannot know which results the snapshot contains.
- A worker that reaches a `remote_call` again after a resume (same `call_id`)
  is safe. The server does not dispatch the call a second time. It answers
  from the stored result or keeps waiting for the child that already runs.
- End of input on stdin means cancel: the worker emits `cancelled` and exits
  with code 130. `dev.sh` also ends workers that are left on its own three run
  stores when it stops.
- Snapshot files are named `snap-<n>` with `n` = highest existing number in
  `--snapshot-dir` plus 1. The server takes `snapshots[].n` from the file name
  when that keeps `n` increasing, and counts on from the previous `n`
  otherwise. It reads `snapshot_path` and `state_path` from the event.
- One pause request may be answered with `blocked` events and later with
  `paused`. While only `blocked` events arrive, the status stays `pausing` and
  the run record carries the latest reason in `blocked`
  (`{reason, path, ts, attempts}`). The web app shows it under the status.
  Kill and cancel work in that state.
- `GET /api/source?file=` accepts the `file` of a `position` event as it is:
  relative to `PROGRAM_DIR`, or absolute inside `PROGRAM_DIR`.

## Run store

`<RUNS_DIR>/<run-id>/` holds `meta.json` (the run record, rewritten after
every change), `events.jsonl` (worker events with `site` added, plus the
server's own events about the run), and the snapshot files that the worker
wrote (`snap-<n>.bamlsnap`, `snap-<n>.json`).

At startup the server reloads every run. A run that had a worker process when
the server stopped becomes `paused` if it is durable and has a snapshot, and
`lost` otherwise. A child run whose result did not reach its parent's site is
delivered again.

## Behavior beyond sections 1 to 6 of the contract

Contract section 7 ("Accepted extensions") now specifies the items below, and
every component may rely on them.

- **Extra run fields.** `remote_results` (results that reached this site, with
  an `acked` flag that turns true at `remote_result_received`), `forked_from`
  (`{run, n}`), `result_delivered` (child runs), `blocked` (the latest
  `blocked` answer while the status is `pausing`), and `segment` and
  `automatic` on each snapshot entry.
- **Extra SSE events.** `worker_exit {run, segment, pid, exit_code, signal,
  status}` after every worker exit, and `forked {run, from_run, n}`. Both are
  also written to `events.jsonl`, as are `remote_dispatched`,
  `remote_returned`, `migrated_out`, and `migrated_in`. `run` events are not
  written to `events.jsonl`.
- **`snapshot` worker event.** The mock worker writes an automatic snapshot
  after every loop iteration of a durable function and reports it with a
  `snapshot` event that has the fields of `paused` plus `automatic: true`. The
  server records it in `snapshots` without changing the status. This is what
  lets a killed durable run in segment 1 become `paused`. A worker that never
  sends `snapshot` events is fine. `MOCK_AUTO_SNAPSHOT=0` turns it off.
- **Status changes at process exit.** `paused`, `completed`, `failed`, and
  `cancelled` are set when the worker process has exited and both of its pipes
  are drained, not when the matching event arrives. `pid` is null from then on.
- **Pause of a non-durable run** is refused with 409, because the run could
  never leave `pausing`. `pause`, `resume`, `kill`, and `cancel` return 409
  when the run is in the wrong state.
- **`resume {site}`** checks the site name before the state of the run. A value
  that is not a string, or a name that is not in `SITES`, returns 400 for a run
  in any status. A missing `site`, JSON `null`, and the empty string mean the
  site that holds the run.
- **Kill** waits up to three seconds for the exit, so the response already
  carries `paused` or `lost`.
- **Child runs that end as `lost` or `cancelled`** also post a `remote_result`
  with an error, so a parent never waits forever.
- **Migration** sends only the latest snapshot. The imported record lists that
  one snapshot with paths on the new site. An import of an id that already
  exists is accepted only if the existing record is `migrated` (a run that
  comes back). The run id in an import must match `[a-z0-9-]+`. A
  `remote_result` that reaches a site where the run is `migrated` is forwarded
  to the site in `migrated_to` (contract section 8.3).
- **Fork** copies `remote_results` (unacknowledged) into the fork, and copies
  `waiting_on` when the fork is from the latest snapshot. A result that arrives
  later for the source run is also delivered to such forks, including a fork
  that migrated to another site (the result is forwarded to its `migrated_to`)
  and a fork that stayed when the source migrated. A fork has `parent: null`, so it never posts a
  second result to a parent.
- **A result for a fork that migrated** is stored in `remote_results` of the
  record that stayed behind, and the `waiting_on` entry of that record is
  removed only after the fork's site accepted the result. The forward retries
  every 1.5 s, up to 20 times, while the fork's site is unreachable, answers
  5xx, or answers 404 (its import may still be in progress). When every attempt
  fails, the entry stays. The next delivery of the result forwards again, and
  so does the next start of the server, which forwards every result that is
  stored on a `migrated` record whose `waiting_on` still names the call.
- **`GET /api/info`** lists functions by scanning the program's `.baml` files
  for top-level `function` headers. It also returns `worker_cmd`, `runs_dir`,
  `sites`, and `remote_pool`.

## Mock worker

```bash
node scripts/mock-worker.mjs --project program --run r-demo --segment 1 \
  --snapshot-dir /tmp/r-demo --start durable_plan_trip --json-args '{"city":"Lisbon"}'
```

Type `{"type":"pause"}` on its stdin to get a snapshot and exit code 75, then
start it again with `--resume /tmp/r-demo/snap-<n>.bamlsnap`. The snapshot is a
JSON file with the simulated state (loop counter, collected ideas, remaining
sleep time, pending remote calls, the call id counter). The state dump next to
it follows contract section 2.5.

| Variable | Effect |
|---|---|
| `MOCK_SPEED=<n>` | divides every simulated sleep by `n` |
| `MOCK_AUTO_SNAPSHOT=0` | no automatic snapshots |
| `MOCK_BLOCKED=<n>` | `n` `blocked` events before a pause succeeds. The run argument `"mock_blocked": n` does the same for one run. |

The run argument `"mock_remote_sleep_ms": n` makes the remote child of that
run sleep `n` real milliseconds, which `MOCK_SPEED` does not divide.
`verify.mjs` uses it to keep a child alive while its parent migrates twice.
The mock does not know which site hosts it. The site server places its remote
calls.

The mock follows contract section 7.1: it emits `cancelled` before exit code
130, treats end of input on stdin as cancel, names its files `snap-<n>` with
the next free number, and ignores a result for a call id that it does not wait
on. Its `hello` event carries a mock-only field `mock_env`, which `verify.mjs`
uses to check the environment that the server passes to workers.

`remote_fetch_weather` fails in the mock for the city `Atlantis`. This
exercises `failed`, the error form of `remote_result`, and a failing parent. A
non-durable function answers `pause` with a `blocked` event and keeps running.
Position events point at the real lines of `program/baml_src/trip.baml`. Update
the `LINES` table in the mock when that file changes.

## Demo program

```bash
cd program
baml run durable_plan_trip -- --city Lisbon
baml run plan_trip -- --city Lisbon
baml run durable_plan_trip_parallel -- --city Lisbon
baml run remote_fetch_weather -- --city Lisbon
```

Under a plain `baml run` the `durable` and `remote_` markers have no effect.

## Verification

### Mock worker

```bash
node scripts/verify.mjs              # about 2 minutes 45 seconds, 200 checks
MOCK_SPEED=4 node scripts/verify.mjs # faster simulated sleeps
PORT_BASE=28787 RUNS_TAG=-b node scripts/verify.mjs   # other ports and run store names
```

The script starts its own three site servers on `PORT_BASE`, `PORT_BASE + 1`,
and `PORT_BASE + 2`. Its default `PORT_BASE` is 18787, not the 8787 of
`dev.sh`, and its run stores are `server/.baml/verify<RUNS_TAG>-<site>`, so it
can run while the demo is in use. Before it deletes or starts anything it
checks its three ports one after the other and then its three run stores. It
refuses to start when a port is in use, or when a run store is claimed by
another live process (`<store>/.instance.lock` holds the pid of the script that
uses the store). A refused start deletes nothing. At the end the script stops
only the servers that it started and deletes only the run stores that it
claimed. `SIGINT`, `SIGTERM`, and `SIGHUP` stop the three servers before the
script exits, and the run stores are kept in that case.

It checks every route of contract section 3.3, the SSE stream, the central
scene, migration, fork, kill, cancel, the parallel variant, a failing remote
call, and a server restart. It also checks the accepted extensions of contract
section 7: the worker environment, `position.file` and absolute paths on
`GET /api/source`, the contents of `events.jsonl`, `snapshot` events against
`snapshots[].automatic` and `.segment`, snapshot numbers against file names,
the imported StateDump on the destination of a migration, the `forked` and
`cancelled` events, a pause that is answered with `blocked` events only, and a
recovery from a snapshot that predates a remote call.

The scenes for contract section 8:

- `/api/info` returns `sites` in registry order and `remote_pool` on all three
  sites, and no `peer_url`. The servers run without `PORT`, so the port comes
  from `SITES`. `PEER_URL` is set to an unusable address to show that it is
  not read.
- A parent on `local` calls into `cloud`. No child appears on `cloud2` or on
  `local`.
- A parent that migrated to `cloud` makes its remote call from there. The child
  runs on `cloud2`, the result returns to `cloud`, and the run completes. The
  local record carries `migrated_to: "cloud"`.
- A migration chain `local` -> `cloud` -> `cloud2` while the child on `cloud`
  still runs. The run is paused on `cloud2` and has no process when the child
  completes. The result goes to `local`, is forwarded to `cloud` and then to
  `cloud2`, and is stored there. The run completes after a resume. Each hop
  answers `{ok, forwarded_to}`. A run also returns to a site that holds its
  `migrated` record (`cloud2` -> `local` -> `cloud2`).
- A run on `cloud2` calls into `cloud`.
- A resume with an unknown site name, or with a `site` that is not a string,
  returns 400, and the run stays `paused`. A finished run and a `migrated`
  record also answer 400 for an unknown name, and 409 for a known one.
- `POST /api/remote/runs` with a `parent.site` outside `SITES` returns 400 and
  starts no run.
- `POST /api/runs/import` of a run that is paused on the receiving site returns
  409, and the run is unchanged.
- A fork migrates to `cloud2` and still receives the late result of its
  source's call (section 7 with named sites).
- `cloud2` restarts with `REMOTE_POOL=["cloud2"]`. A `plan_trip` run and a
  `durable_plan_trip` run on it fail with `no remote site available`. The
  worker acknowledged the error result, no `remote_dispatched` event appears,
  and no child run exists on any site.

The last scene covers the routing cases that need a request in flight. The
script itself plays a fourth site named `slow`: an HTTP server on a port that
the system assigns, which answers the site-to-site routes with delays and
statuses that the scene chooses. `cloud2` restarts with a registry that also
names `slow` and with `REMOTE_POOL=["slow"]`. The scene checks:

- While the dispatch of a remote call is in flight, `resume {site}` returns
  409. After the dispatch the run moves to `local`, the call is not dispatched
  again, and the result reaches the run through `cloud2`.
- While an import is in flight, the record is `migrated` and already names the
  destination in `migrated_to`. A second resume, to another site or in place,
  returns 409. A result that arrives goes to the destination only, carries
  `via: ["cloud2"]`, and is answered 502. A refused import answers 502, and the
  record is `paused` again with `migrated_to: null`.
- Two `migrated` records that name each other (`slow` forwards back to
  `cloud2`): the result passes each site once and is answered 502. A `via` that
  names the receiving site, or as many sites as the registry has, is answered
  502 without a forward.
- Two forks migrate to `slow`. The first forward to fork A is answered 404, the
  `waiting_on` entry and the stored result stay on the record, the retry
  succeeds, and the entry is removed. A repeated delivery does not forward
  again. The site of fork B answers 503 until `cloud2` restarts. After the
  restart the stored result is forwarded, and the entry is removed.
- A record whose `migrated_to` names no site of the registry: the fallback
  skips the sites in `via`, asks every other site once, answers 502 when none
  holds the run, and answers `forwarded_to` with the site that accepted.

### Real worker

```bash
(cd .. && cargo build -p baml_cli)
node scripts/verify-real.mjs                 # about 2 minutes 30 seconds, 151 checks
USE_RUNNING=1 node scripts/verify-real.mjs   # against the three servers that dev.sh started with the real WORKER_CMD (PORT_BASE defaults to 8787 here)
ONLY=b,c node scripts/verify-real.mjs        # selected scenes
RECORD=/tmp/events.jsonl KEEP=1 node scripts/verify-real.mjs   # keep the SSE events and the run stores
```

`WORKER_BIN` selects another binary (default `../target/debug/baml-cli`). The
script starts its own three site servers on `PORT_BASE` (default 18787, so it
does not collide with a demo on 8787) with run stores in
`server/.baml/verify-real<RUNS_TAG>-<site>`, and it prints the measurements of
the next section at the end. It checks its ports and claims its run stores in
the same way as `verify.mjs`, and it stops its servers on `SIGINT`, `SIGTERM`,
and `SIGHUP`. Its check for leftover workers looks only at processes whose
`--snapshot-dir` lies in its own run stores. With `USE_RUNNING=1` the run
stores belong to a demo that may be in use, so the check is skipped and no run
store is deleted. The scenes:

| Scene | Content |
|---|---|
| a | `durable_plan_trip` completes. The remote child runs on `cloud`. Automatic snapshots are recorded. |
| b | The central scene. The parent is paused during the remote wait. The worker process is gone, and the StateDump shows `city`, `day`, and `ideas`. The child completes while no parent process exists, and its result is stored. The resume starts a new pid, and the run completes with the correct `TripPlan`. The remote call is announced and dispatched once. |
| c | A pause inside the loop. The thread is parked in `sleep` with an absolute deadline. After the resume every `println` line appears exactly once. |
| d | Migration. The run is paused on `local` and resumed with `{"site":"cloud"}`. The snapshot bytes and the StateDump are identical on both sites. The remote call of the migrated run is placed on `cloud2`, and the result returns to `cloud`. The local record carries `migrated_to: "cloud"`. |
| e | Fork of a paused run. The source and the fork complete in separate processes, and each makes its own remote call. |
| f | Kill. `plan_trip` becomes `lost`. `durable_plan_trip` becomes `paused` from its latest automatic snapshot and completes after a resume. A durable run without a snapshot becomes `lost`. |
| g | `durable_plan_trip_parallel`. The pause request is answered with `blocked` and the path `thread 2 > frame durable_plan_trip_parallel > local weather_future > future #0 (pending)`. The run record and the `run` SSE messages carry the reason. The run completes, and no snapshot is written. |
| h | Kill during the remote wait. The latest automatic snapshot predates the call, so the resumed worker repeats `remote_call` with the same `call_id`. The server does not dispatch it again and answers from the stored result. |
| i | Cancel: the `cancelled` event, then exit code 130. |
| j | Two pauses in one run (loop, then remote wait). The result arrives on stdin of segment 3. |
| k | A pause request that arrives while the worker still loads the program. The run pauses at its first clean point. |
| m | Migration chain `local` -> `cloud` -> `cloud2` during the remote wait. Each import is paused again right away (the pause request waits while the worker loads the program). The run is paused on `cloud2` without a process when the child on `cloud` completes. The result is forwarded `local` -> `cloud` -> `cloud2` and stored, and the run completes in segment 4. The two hops took 2.8 s in one run, and the child needs about 4.3 s, so the scene depends on that margin. While the run is paused on `cloud`, an import of the same id into `cloud` returns 409 and changes nothing. At the end a `remote_result` whose `via` names a site of the chain is answered 502 and reaches no record, and one without `via` still follows the chain. |
| n | A run on `cloud2` calls into `cloud`. Nothing of it touches `local`. |
| o | A resume with an unknown site name, or with `site: 5`, returns 400, also on a completed run (a known name returns 409 there). A remote call whose `parent.site` is not in `SITES` returns 400. A resume with the run's own site name resumes in place. |
| l | The local site server stops and restarts during a durable run. The worker exits, the run is `paused` after the restart, and it completes after a resume. Skipped with `USE_RUNNING=1`. |
| p | `cloud2` restarts with `REMOTE_POOL=["cloud2"]`. `plan_trip` and `durable_plan_trip` on it fail with `baml.errors.Io {message: "remote call to `user.remote_fetch_weather` failed: no remote site available"}`, and the `failed` event carries the line of the remote call. Skipped with `USE_RUNNING=1`. |

The web app has its own checks (`web/README.md`). `LIVE=1 pnpm check:headless`
detects the worker behind the site servers from `/api/info` and runs the
`blocked` scene with `durable_plan_trip_parallel` for the real worker.

## Measurements with the real worker

Debug build of `baml-cli` (`cargo build -p baml_cli`, unoptimized), Apple M5
Max, macOS 26.6, demo program `program/baml_src/trip.baml`, all site servers
on the same machine. The numbers come from one `verify-real.mjs` run. In four
more runs every PauseStats time stayed below 1 ms, `decode_ms` stayed between
0.34 and 0.41, and `program_load_ms` stayed between 1210 and 1440.

PauseStats (`paused` event):

| Field | Scene b: paused in the remote wait | Scene c: paused in the loop (`sleep`) |
|---|---|---|
| `pause_latency_ms` | 0.060 | 0.087 |
| `walk_ms` | 0.052 | 0.147 |
| `encode_ms` | 0.029 | 0.069 |
| `compress_ms` | 0.045 | 0.097 |
| `write_ms` (both files, atomic rename) | 0.356 | 0.626 |
| `objects` | 7 | 5 |
| `raw_bytes` (state section) | 686 | 552 |
| `compressed_bytes` (state section, zstd) | 421 | 376 |
| `file_bytes` (`snap-<n>.bamlsnap`) | 565 | 520 |
| `program_bytes` (not embedded in the file) | 2,692,913 | 2,692,913 |
| `blocked_attempts` | 0 | 0 |
| `threads` | 1 | 1 |

ResumeStats (`resumed` event):

| Field | Scene b | Scene c |
|---|---|---|
| `process_start_ms` | null (not measured) | null |
| `program_load_ms` (compile `--project`, build the engine) | 1327.9 | 1230.6 |
| `decode_ms` (read, decompress, allocate, restore) | 0.373 | 0.342 |
| `first_exec_ms` (worker entry to the first `exec()`) | 1329.1 | 1231.7 |

Wall time, measured by `verify-real.mjs` from outside:

| Interval | Scene b | Scene c |
|---|---|---|
| `POST pause` to the `paused` event | 8 ms | 9 ms |
| `POST pause` to status `paused` (process exited, both pipes drained) | 66 ms | 62 ms |
| `POST resume` to the HTTP response | 16 ms | 12 ms |
| `POST resume` to the first event of the new segment (`hello`, by worker timestamp) | 16 ms | 16 ms |
| `POST resume` to the same event on the SSE stream | 17 ms | 16 ms |
| `POST resume` to `resumed` and the first program event | 1345 ms | 1247 ms |

Over all scenes of that run: 10 resumes had `program_load_ms` between 1227 and
1439 (median 1251) and `decode_ms` between 0.34 and 0.41. 27 automatic
snapshots took 0.04 to 0.13 ms to park the run (`park_ms`) and 0.39 to 2.67 ms
to write, and their files were 526 to 568 bytes. A start takes the same
program load: `POST /api/runs` to `hello` is 14 ms, and to the first `position`
event 1259 ms. Program load dominates the resume. The snapshot work is below
1 ms in total.

## Known limitations

Found or confirmed by the end-to-end runs with the real worker:

- A run with several threads cannot be paused. A local that holds a future,
  pending or ready, blocks every snapshot until it leaves scope.
  `durable_plan_trip_parallel` therefore never pauses. The pause request is
  answered with `blocked`, the status stays `pausing` until the run completes,
  and no event ends the request. Automatic snapshots of such a run are skipped.
- A recovery from an automatic snapshot repeats the effects between that
  snapshot and the loss of the process (at-least-once). In the demo program
  this can repeat a `println` line. The remote call is not repeated, because the
  `call_id` is stable and the server does not dispatch a `call_id` twice.
- A fork has its own run id, and its first remote call gets the `call_id`
  `<fork-id>-c1`. A fork of a run that was paused before the call makes its own
  remote call, so two child runs exist (scene e).
- `pause_latency_ms` counts from the moment the worker handles the `pause`
  command. A command that arrives while the worker loads the program waits on
  stdin for up to 1.3 s, and that time is not part of the number (scene k:
  1232 ms wall time, `pause_latency_ms` 0.06).
- `process_start_ms` is always null. `first_exec_ms` starts at the worker's
  entry point, not at the start of the OS process.
- The worker compiles `--project` at every start and resume (about 1.25 s in
  the debug build). The snapshot does not embed the program. A resume refuses a
  snapshot when the program hash or the runtime build (package version and git
  sha) differs, so an edit of `program/` invalidates paused runs.
- The site server refuses `POST pause` for a non-durable run (409), although
  the worker can pause any run.
- `parked.detail` in the StateDump is the raw resume payload. For a remote call
  it names the function as `user.remote_fetch_weather`, while frames and events
  use the display name `remote_fetch_weather`.
- Migration transfers only the latest snapshot. Earlier snapshot numbers return
  404 on the destination, and `events.jsonl` starts empty there. The web app
  joins both files by run id.
- `baml-cli worker` before this verification stayed alive as an orphan when its
  site server died, because a write to the closed stderr panicked before the
  run was cancelled. `diagnostic()` in `worker_command.rs` now ignores write
  errors. Scene l checks the behavior. No Rust test covers it.

Reported by the Rust integration and not re-checked by these runs:

- Globals are not part of a snapshot. A resumed engine runs `$init` again.
- The profiler is off for resumed segments.
- The position of an `await` yield carries a wrong line (compiler line table).
  In the demo program the future is ready when the `await` runs, so no `await`
  position event appears.
- A `remote_result` that reaches the worker's stdin after the snapshot is
  committed and before the process exits is lost in the worker. The site server
  covers this: it keeps a result until `remote_result_received` and passes every
  unacknowledged result as `--remote-result` at the next resume.
- `park_requested` is one flag that the collector, the snapshot coordinator,
  and the cancel path share. A cleared request is set again within 20 ms, so
  the effect is a delay.
