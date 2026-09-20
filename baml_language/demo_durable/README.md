# Durable functions demo

This directory holds the proof of concept for durable BAML functions. The
design is in `documents/durable-functions-design.md`, and the API agreement
between the components is in `documents/durable-poc-contracts.md`.

| Path | Content |
|---|---|
| `server/` | The site server, a BAML program. Three instances run side by side: `local` on port 8787, `cloud` on port 8788, and `cloud2` on port 8789. |
| `program/` | The demo program that the workers run (`baml_src/trip.baml` and `baml_src/quotes.baml`). |
| `scripts/mock-worker.mjs` | A mock worker (Node 22, no dependencies) that speaks the worker protocol and simulates the demo program. |
| `scripts/dev.sh` | Starts the three site servers and stops them on exit. Passes `CHAOS`, `SLEEP_SUSPEND_MS`, and `WORKER_LEGACY` through and gives each site its own program store. |
| `scripts/verify.mjs` | An end-to-end check of the three site servers with the mock worker. |
| `scripts/verify-real.mjs` | An end-to-end check of the three site servers with the real worker (`baml-cli worker`). It also prints the measured pause and resume numbers. Scene 8 checks the call sites and the cancellation causes of contract section 10 against the lines of `program/baml_src/`, which it reads rather than repeating. |
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
   run calls `remote_fetch_weather`, which starts on `cloud` or on `cloud2`
   (see "Sites and placement of remote calls" below).
2. Press Pause while the child run is active. The local worker writes
   `snap-<n>.bamlsnap` and exits with code 75. The state panel shows `city`,
   `ideas`, `day`, and `weather` as they are at line 29. The child run keeps
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
   appears. The run pauses with two threads. The state panel shows the root
   thread in its `sleep`, the spawned thread in its remote call, and the local
   `weather_future` as a pending future. "Resume on cloud2" moves both threads:
   the restored thread waits for the same call, and the child's result follows
   the run to `cloud2`. A worker from before contract section 9 cannot write
   the future. It answers with `blocked`, and the status stays `pausing` until
   the run completes.

Every object on the timeline explains itself (contract section 10.3). Select an
arrow, a bar, a gap, or a marker: the panel on the right names what made it
happen in one sentence and offers the source location as a link. Following the
link opens that file in the source view and highlights the line, in its own
color and with the tag `▸ why`; the button next to the file name clears it.
When the worker reported no location, the panel falls back to the last position
of the thread and says that the location is approximate, and when it cannot
attribute a location at all it says so instead of guessing.

What the panel says. The sentences below are the ones the app printed for a
live `durable_race`, `durable_deadline`, `durable_fan_out`, `durable_nap`, and
`durable_plan_trip` run with the real worker on the demo program. A location
that the worker reported is exact; one that comes from the last `position` of
the thread is marked `· approximate` under the link.

| Object | Sentence | Location |
|---|---|---|
| call arrow | `durable_race called remote_get_quote at quotes.baml:192, placed on cloud2.` | exact, the call site the worker reported; the rows add the call id, the callee, the calling function, the calling thread, the child run, and the arguments |
| return arrow, parent running | `remote_get_quote succeeded on cloud2, and thread 3 of the parent took the result at quotes.baml:190.` | approximate: `remote_result_received` carries no line, so the last position of that thread is used |
| return arrow, no process | `remote_get_quote succeeded on cloud. The result was delivered while the run had no process, taken at resume.` | none: `No source location: The parent had no process when the result arrived, so no line of the program took it.` |
| cancel arrow, race loser | `remote_get_quote was cancelled because the future was cancelled (a race loser, or Future.cancel). The call was made at quotes.baml:192.` | exact, the call site of the cancelled call (`cause: future_cancel`) |
| cancel arrow, deadline | `remote_get_quote was cancelled because a cancel token fired (the deadline of with_timeout, or a token of the program). The call was made at quotes.baml:242.` | exact, the line of the call inside the `with_timeout` closure (`cause: token`) |
| cancel arrow, cause not reported | `remote_get_quote was cancelled, and the worker could not say what fired: the run itself may have been cancelled, or the thread held no link the engine can read. The call was made at quotes.baml:191.` | exact, the call site (`cause: unknown`, which the panel never turns into a claim) |
| migration arrow | `A "Resume on cloud" command moved r-022ga9 from local to cloud. Its snapshot travelled with it, and it was taken at quotes.baml:253.` | approximate, the last line the run reported on the site it left |
| fork arrow | `r-drmuea was forked from snapshot #3 of r-e269fw, which was taken at trip.baml:29.` | approximate, the last line the source reported before that snapshot |
| sleeping gap | `The run suspended itself for a sleep at quotes.baml:164 and wakes at 11:20:16.598.` | approximate: a `position` event is the last sleep the segment reported, not necessarily the one whose deadline the gap ends at |
| snapshot marker | `No pause was requested. The worker writes this snapshot on its own, and the process keeps running. Thread 2 stood at quotes.baml:193.` | the top user frame of the state dump, which belongs to one thread of it, not to the run |
| segment bar | `The process started the function and it ran the function to its end. It ran from quotes.baml:189 to quotes.baml:194.` | approximate, the last position of the segment |
| thread sub-bar | `Thread 6 was spawned by thread 2 at quotes.baml:193.` | exact, the `spawn` site in the parent thread |
| wait band | `durable_race called remote_get_quote at quotes.baml:190. Thread 3 waited here for 2.62 s.` | exact, the call site |

A cancel arrow of a run that was paused and resumed carries the same location:
the process that reports the cancellation is not the one that made the call, and
the call site travels in the snapshot.

The scenario gallery. The "Scenarios" button in the header of the app opens
seven cards. "Run live" starts the scenario on the site servers and shows a
guide bar with the next step. The guide follows the event stream, so a hint
appears at the moment it applies. With the Autoplay switch on, the app
highlights each button for 1.3 seconds and presses it. "Play recording" plays
the same scenario from a fixture without servers. All seven ran to their end
with the real worker and autoplay (`LIVE=1 LIVE_ONLY=guide pnpm
check:headless`, see `web/README.md`).

| Card | Function | What to watch |
|---|---|---|
| Pause here, resume there | `durable_plan_trip` | Pause inside the loop on `local`, "Resume on cloud". The loop continues on `cloud` at the same line, and the remote call of the moved run goes to `cloud2`. |
| Fan-out with a durable sleep | `durable_fan_out` | Nothing to press. Four children start, two on `cloud` and two on `cloud2`. The parent suspends itself within a few milliseconds of its last remote call: the status is `sleeping`, no process exists, and the timeline shows the sleeping gap with a countdown. The state panel shows five threads and four futures. The four `Quote` objects are stored on the sleeping run. The timer wakes it 12 seconds later, a new process takes all four results, and the four thread lines continue across the gap. |
| Race and cancellation | `durable_race` | Pause during the race (five threads, pending futures), "Resume on cloud2". The winner's result was stored while no process existed and travels with the run. `race` settles on `cloud2` and cancels the two losers on their sites: two cancel connectors and two striped bars. Click a cancel arrow: the panel says that the future was cancelled and links to the line of the call. |
| Deadline | `durable_deadline` | Pause (the state panel shows the cancel token, the work thread in its remote call, and the deadline thread in its sleep), "Resume here". The deadline kept counting during the pause. The token fires, the 6 second child is cancelled on its site, and the result is built from the `Timeout` error. Click the cancel arrow: the panel names the cancel token and links to line 242, the call inside the `with_timeout` closure. |
| All settled, one failure | `durable_settled` | Pause after the refusal, about 3.3 seconds in. The state panel shows the three futures in three states: resolved with a `Quote`, failed with the vendor's error, and pending. After the resume the report holds two quotes and the typed failure of the `Car` vendor. Nothing is cancelled. |
| Kill and recover | `durable_plan_trip`, then `plan_trip` | "Kill process" after an automatic snapshot: the durable run is `paused` and completes after "Resume here". The plain run is `lost`. |
| Fork | `durable_plan_trip` | Pause, Fork, and resume both runs. Each of them makes its own remote call and completes. |

The same scenes by hand (they need a worker that implements contract section
9, or the mock worker):

6. Start `durable_nap` with `seconds: 8`. The worker writes a snapshot and
   exits, the status is `sleeping`, and the run record shows `wake_at`. No
   process exists until the site server resumes the run at that time. "Resume
   here" before the deadline starts a worker that suspends again for the rest
   of the sleep. Cancel ends the run without a process.
7. Start `durable_fan_out`. Four `remote_get_quote` children start, two on
   `cloud` and two on `cloud2`, and the parent goes to sleep for 12 seconds.
   The children run by program hash from a program that their site fetched
   from `local`; the cloud sites never compile it. The children finish while
   the parent has no process. Their results, whole `Quote` objects, are stored
   on the sleeping run and are all delivered when it wakes up. "Resume here"
   while it sleeps delivers the results that are stored so far, and the run
   suspends again. Stop and restart `dev.sh` while the run sleeps: the timer is
   rebuilt from the run store, and a run whose deadline has passed resumes at
   once.
8. Start `durable_race`. The fastest of three vendors wins after 2 seconds, and
   the two losers are cancelled on their sites (`remote_cancelled`, exit code
   130 of the child workers). Press Pause right after the start, wait until
   all three children have finished, and resume: the 2 second vendor still
   wins, because the stored results are replayed in the order in which they
   arrived. `durable_deadline` shows the same for a timeout, and
   `durable_settled` shows `all_settled` with one child that fails.
9. Start `durable_race` and press Cancel (or "Kill process") on the parent. The
   site server cancels the three children. Fork a sleeping `durable_fan_out`
   and cancel the fork: the children keep running, because they belong to the
   source.
10. Start the servers with `SLEEP_SUSPEND_MS=600000`, start `durable_fan_out`,
    and press "Kill process" after the second hollow snapshot marker. The run
    is `paused` at an automatic snapshot that holds its threads and futures.
    "Resume here" waits only for the calls that were outstanding in that
    snapshot and gets their results from the run record.

With the mock worker (no Rust build needed):

```bash
cd baml_language/demo_durable
./scripts/dev.sh                 # site servers on 8787 (local), 8788 (cloud), and 8789 (cloud2)
(cd web && pnpm install && pnpm dev)   # the app on http://localhost:5173
```

`CLEAN=1 ./scripts/dev.sh` deletes the three run stores and the three program
stores of this instance first. Ctrl-C stops the three servers. A worker exits when its site server
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

The program stores follow the same rule: `server/.baml/programs<RUNS_TAG>-<site>`.

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
the target when its worker reports `remote_call` (contract section 9.3):

1. It takes `REMOTE_POOL` in order and removes its own site. The sites that
   remain are the eligible sites.
2. It keeps one counter per site server and picks the eligible sites in
   round-robin order.

| The caller runs on | The remote calls run on |
|---|---|
| `local` | `cloud`, `cloud2`, `cloud`, `cloud2`, ... |
| `cloud` | `cloud2` |
| `cloud2` | `cloud` |

The site is picked on the thread that reads the worker's events, in the order
in which the worker made the calls, so the four calls of `durable_fan_out`
alternate between `cloud` and `cloud2`. The counter belongs to the site server,
not to a run, and it starts at zero when the server starts. A scene that needs
the site of a child takes it from the `remote_dispatched` event.

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
| `PROGRAMS_DIR` | the program store of this site, relative to `server/` (contract section 9.5) | `.baml/programs-<SITE>` |
| `SLEEP_SUSPEND_MS` | passed to every worker as `--sleep-suspend-ms` | not set, so the worker's default of 5000 applies |
| `CHAOS` | JSON object with the test knobs of contract section 9.3 | not set |
| `WORKER_LEGACY` | `1`: the worker predates contract section 9. It gets `--project` always and none of `--program-store`, `--program-hash`, and `--sleep-suspend-ms`. | `0` |

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
| `PROGRAMS_DIR_BASE` | prefix of the three program stores. Each site gets `<PROGRAMS_DIR_BASE>-<site>`, so two sites never share a store. | `.baml/programs<RUNS_TAG>` |
| `CLEAN=1` | delete this instance's three run stores and three program stores before the start | off |

`CHAOS`, `SLEEP_SUSPEND_MS`, and `WORKER_LEGACY` reach the three servers
through the environment as they are. `PROGRAMS_DIR` is set per site by the
script.

### Switching to the real worker

The real worker is the hidden `worker` subcommand of `baml-cli`. Build it and
point `WORKER_CMD` at the binary with an absolute path:

```bash
cd baml_language && cargo build -p baml_cli
WORKER_CMD='["/abs/path/to/baml_language/target/debug/baml-cli","worker"]' ./demo_durable/scripts/dev.sh
```

The site server appends the contract arguments to that prefix:

```
[--project <PROGRAM_DIR>] --run <run-id> --segment <n> --snapshot-dir <RUNS_DIR>/<run-id>
--program-store <PROGRAMS_DIR> [--sleep-suspend-ms <SLEEP_SUSPEND_MS>] [--program-hash <hash>]
( --start <function> --json-args '<json>' | --resume <snapshot-path> )
[--remote-result '<json>']...
```

`--project` is left out, and a start gets `--program-hash`, for a run that came
from another site with a known program hash: a remote child and an imported
run. With `WORKER_LEGACY=1` the second line is left out and `--project` is
always passed, which is what a worker from before contract section 9 accepts.

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

## Durable sleep, remote cancellation, and the program store

Contract section 9 specifies this part. The site server code is in
`server/baml_src/worker.baml` (sleep, timers, cancellation) and
`server/baml_src/programs.baml` (program store).

### Sleeping runs

A durable run suspends itself for a long `sleep` (worker flag
`--sleep-suspend-ms`, default 5000). The worker writes a snapshot exactly as for
a pause, reports `paused` with `wake: {reason: "sleep", remaining_ms, at_ts}`,
and exits with code 75.

- The site server owns the deadline. At the receipt of the `paused` event it
  sets `wake_at` to its own clock plus `wake.remaining_ms`. `at_ts` of the
  worker is not used, so clock skew between a worker machine and the server has
  no effect. A `remaining_ms` with a fraction is rounded up.
- At the exit of the worker the status becomes `sleeping`, the server emits
  `sleep_scheduled {run, wake_at}`, and a timer thread waits until `wake_at`.
  `meta.json` holds `status: "sleeping"` and `wake_at`.
- The timer resumes the run through the normal resume path and emits
  `woken {run, reason: "timer"}`. `POST resume` on a sleeping run works too
  (`reason: "manual"`). The new worker sleeps the remaining time in process, or
  suspends again when the remaining time is still at or above the threshold.
  The deadline is absolute, so it does not move.
- The change from `sleeping` or `paused` to `starting` is a compare-and-set
  (`begin_resume`) under the state lock of the server (see "The state lock"
  below). Of two resume requests, or of a request and a timer, exactly one
  starts a worker. The others get 409 or do nothing. A cancel without a process
  and a move to another site use the same lock, so a run is resumed, cancelled,
  or moved, and never two of them. A timer also checks that it is still the
  current timer of the run (`app.timers`), so the timer of an earlier suspend
  never wakes a run that sleeps again.
- At startup the server rebuilds the timers from the run store
  (`rebuild_timers`). A run whose `wake_at` has passed resumes at once with
  `woken {reason: "restart"}`. For the others `sleep_scheduled` is emitted
  again with the unchanged `wake_at`. A run that was live when the server
  stopped, and whose record already carries `wake_at`, is loaded as `sleeping`.
- `POST cancel` on a `sleeping` or `paused` run sets `cancelled` without a
  process, records the optional `{reason}` of the request as the error, emits
  `run_cancelled {run, was, reason}`, and cancels the outstanding remote
  children. `kill` and `pause` of a sleeping run return 409.
- `resume {site}` works on a sleeping run. The destination starts a worker,
  which suspends again, and the destination computes its own `wake_at`. When
  the import is refused, the run is `sleeping` again and gets a new timer.

### Remote cancellation

- On a worker `remote_cancel {call_id}` event the server adds the call to
  `cancelled_calls` of the run (at once, on the thread that reads the worker's
  events), removes the `waiting_on` entry, emits
  `remote_cancelled {run, call_id, child_site, child_run}`, and posts
  `cancel` to the child's site with the body `{reason, parent}`.
- The child's site cancels a child with a worker through the `cancel` command
  (exit code 130), and a `paused` or `sleeping` child without a process. A
  `migrated` record passes the request on to `migrated_to`.
- A run that ends as `cancelled`, `failed`, or `lost` cancels every child in
  `waiting_on` in the same way. This also happens for a run that is cancelled
  without a process, for a worker that could not be started, and at the start
  of the server for runs that became `lost` while it was down.
- The cancelled child still posts a `remote_result` with an error, as every
  child that ends does. The parent's site discards a result for a call in
  `cancelled_calls` and emits `remote_result_discarded {run, call_id, ok}` once.
  The answer is `{ok: true}`, so the child does not retry.
- A `remote_cancel` can arrive while the dispatch of the call is in flight,
  for example when the dispatch is slower than a `with_timeout` deadline. The
  call is planned on the reader thread (`plan_remote_call`), so the cancel
  always finds the dispatch registered. When the other site answers, the child
  is not added to `waiting_on` and is cancelled right away.
- A worker that recovers from a snapshot that predates a cancellation reaches
  the call again and emits `remote_call` with the same `call_id`. The server
  takes the call out of `cancelled_calls` and starts a second child. A
  `remote_result` carries `child_run`, and a result from a run that is not the
  one in `waiting_on` is discarded, so the late answer of the first child cannot
  settle the second attempt.
- A fork starts with an empty `cancelled_calls`.
- `POST cancel` and `POST pause` can be accepted after the worker has reported
  `paused` for a self-suspend and before its process has exited. The real
  worker needs about 60 ms for that, and it reads no command in that time. The
  server records the request before it writes the command, and the exit handler
  finishes it: the run becomes `cancelled` and its children are cancelled, or
  it becomes `paused` and gets no wake timer. `cancel` on a run that is
  `starting` without a process, because its program is being fetched, is
  honoured before the worker starts.

### Forks and remote children

A fork waits on the same child runs as its source. A child belongs to the run
that dispatched it:

- The `waiting_on` entries that a fork copies are marked `inherited`. A run
  never cancels a child through an inherited entry.
- A run does not cancel a child while another run of the site still has a
  `waiting_on` entry for the same child: a fork, its source, or the record of a
  fork that moved away and still gets the result forwarded. Instead of
  `remote_cancelled` it emits
  `remote_released {run, call_id, child_site, child_run, inherited, shared_with}`.
  Cancelling a fork therefore leaves the children of the source alone, and
  cancelling the source leaves the children that its fork waits on.
- The run record keeps `calls`, the function and arguments of every remote call
  that a worker of the run announced. A fork copies the list.
- A resumed worker reports every remote wait of its restored threads with
  `remote_wait {call_id, thread, function, has_result}`, because it does not
  announce such a call again. The server settles the call from what it knows:
  a result stored on the run; a result stored on a run that shares the call id,
  which is copied; a child that still runs for such a run, to which the run
  attaches itself (`remote_attached`); a dispatch in flight, which is looked at
  again when it has returned; and otherwise a new dispatch from `calls`. A
  fork that was taken before its source's dispatches had returned, or from a
  snapshot that is not the latest one, completes this way.

### Results in arrival order

`remote_results[].ts` is the time at which a result arrived. The server passes
it with every `--remote-result` and `remote_result`. A resumed worker takes the
results it got at its start one at a time in that order, merged with the sleeps
whose deadline passed while the run had no process, and lets the run become
quiescent between two of them. A `race` that is paused after its dispatches and
resumed after all children have finished returns the child that finished
first, and its other children are cancelled, as in a run that was never paused.

Call ids do not depend on the scheduler: `<run>-c<k>` for the k-th call of the
root thread, and `<run>-c<path>-<k>` for a spawned thread, where the path is
the thread's place in the spawn tree (`0.2` is the third spawn of the root). A
recovery from an older snapshot makes some calls again, and every call gets the
id it had before, so the stored result of the same request answers it.

### Program store

A program is identified by its hash, the SHA-256 of the Borsh-encoded
`Program`. `hello.program_hash` sets `program_hash` on the run record. Each
site has its own content-addressed store, `PROGRAMS_DIR`, with the layout
`<store>/<first two hex chars>/<hash>.bamlprog`. Every worker gets
`--program-store`, and the workers write the entries.

The site server reads and writes entries with this file layout:

```
magic           8 bytes   "BAMLPROG"
format_version  u32 LE
build_len       u32 LE
runtime_build   build_len bytes, UTF-8
payload_len     u64 LE
program         payload_len bytes, the rest of the file: the bytes that the hash covers
```

This is the entry layout of the Rust crate `bex_program_store`, which the
worker uses. The crate also keeps build marker files next to an entry. The
server does not write markers, because a worker accepts an entry whose header
names its own runtime build.

The server accepts only this layout in format version 1, with a runtime build
of 1 to 256 bytes on one line. The worker refuses every other entry, so an
entry in another layout counts as missing on the server too. The server builds
the header of a fetched entry itself from the `runtime_build` that the sending
site reported. Header bytes from another site are not trusted, because only the
program bytes are covered by the hash. The server writes an entry without fsync.
A torn entry fails its hash check and is fetched again.

- `GET /api/programs/:hash` returns `{hash, runtime_build, program_base64,
  format_version}` or 404. `program_base64` holds the bytes that the hash
  covers. `runtime_build` is the build of the site's own workers when their
  marker exists next to the entry, and the build in the header otherwise. A
  value that is not 64 lowercase hexadecimal characters returns 400.
- `POST /api/remote/runs` and `POST /api/runs/import` carry `program_hash` and
  `from_site`. Before the receiving site starts a worker it looks for the hash
  in its own store. On a miss it fetches the program from `from_site`, checks
  that the SHA-256 of the received bytes is the requested hash, writes the entry
  to a temporary file in the same directory (`.tmp-<hash>.<random>.partial`,
  removed on every failure path), and renames it. It emits
  `program_fetched {hash, from_site, bytes, ms}` on its SSE stream. Two requests
  for the same program at the same time cause one fetch.
- `POST /api/remote/runs` answers as soon as the run record exists, and the
  fetch happens before the worker starts. A fetch can take longer than the 10 s
  that the caller waits for the dispatch. A failed fetch, a hash mismatch, or a
  program of another runtime build ends the child as `failed` with the reason,
  nothing is stored, no worker starts, and the parent gets the reason as the
  error of the call. `POST /api/runs/import` fetches before it answers, and the
  source waits 45 s for it. It reads the program hash from the snapshot header
  and refuses a request that names another hash with 400.
- A site learns the runtime build of its workers from `hello.runtime_build`. It
  refuses a fetched program of another build and names both builds, because no
  worker of the site would run it.
- The memory of which hashes the store holds is trusted only while the entry
  file exists. Before every start and resume of a run that executes by hash,
  the site makes sure that the entry is there and fetches it again from
  `origin.site` or `parent.site` when it is gone. A resume that cannot get the
  program leaves the run `paused` with `error` set, so its snapshot is not
  spent on a worker that must fail.
- A run that came from another site with a program hash (a remote child, an
  imported run) is started without `--project`, and a remote child gets
  `--program-hash`. The cloud sites therefore run a program that they never
  compiled, and the function of such a run need not exist in `PROGRAM_DIR` of
  the site. A request without `program_hash` (an older site, or a legacy worker
  on the caller's site) is handled as before, with `--project`.

### The state lock

Every HTTP request, every reader of a worker's output, and every timer of the
site server is its own BAML thread. BAML threads run in parallel and are
preempted between any two instructions. A check that is followed by a change is
therefore not atomic: in a test program, eight threads that each read a field
and then set it all saw the old value in 30 of 30 rounds. The scene with five
resume requests at once started two workers from one snapshot before the lock
existed.

The stdlib has no mutex. A `baml.spawn.TaskGroup` with the limit 1 admits one
task at a time in FIFO order, and `locked(app, body)` in `app.baml` runs a body
as a task of that group (0 of 30 rounds went wrong in the same test program). A
body performs no sys-op and never calls `locked` again. The server takes the
lock for:

- the status changes that compete: resume, cancel without a process, move to
  another site, pause;
- placement and the bookkeeping of a remote call: the round-robin counter, the
  registration of a dispatch, `cancelled_calls`, the `waiting_on` entry after a
  dispatch, the cascade to the children;
- the bookkeeping of a remote result: duplicate detection, discard, the stored
  result, the `waiting_on` entry, the forward to a fork that migrated;
- the reservation of a run id by an import, the run table, and the claim of a
  program fetch;
- the writer of a `meta.json` file, and the queue and the drain thread of an
  SSE subscriber. `init` is built and queued under the lock, so a change of a
  run is part of `init` or reaches the client as a `run` event.

Events, files, HTTP requests, and the worker's stdin are handled after the lock
is released.

### CHAOS

The environment variable `CHAOS` is a JSON object. Each delay is applied by
the site that sends the named request, before it sends it.

| Knob | Effect |
|---|---|
| `dispatch_delay_ms` | before `POST /api/remote/runs` |
| `result_delay_ms` | before a child's site posts `remote_result` |
| `cancel_delay_ms` | before `POST cancel` for a remote child |
| `import_delay_ms` | before `POST /api/runs/import` |
| `program_fetch_delay_ms` | before `GET /api/programs/:hash` |
| `duplicate_results` | every `remote_result` is posted twice |
| `reorder_results` | one result at a time is held until the next result of that site was sent, so the second overtakes the first |
| `reorder_hold_ms` | extension: the longest hold when no other result follows. Default 4000. |

`GET /api/info` returns the knobs in `chaos`, the store in `programs_dir`, and
the threshold in `sleep_suspend_ms`.

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
- **Contract section 9 extensions.** Run fields: `cancelled_calls`. SSE events:
  `run_cancelled {run, was, reason}`, `remote_result_discarded {run, call_id,
  ok, child_run?}`, and `program_fetched {hash, from_site, bytes, ms}` (the last
  one belongs to no run and is not written to an `events.jsonl`). `sleep_scheduled`,
  `woken`, `remote_cancelled`, `run_cancelled`, and `remote_result_discarded`
  are written to `events.jsonl`. Request fields: `child_run` on `remote_result`,
  `{reason, parent}` on `cancel`. Response fields: `format_version` on
  `GET /api/programs/:hash`. Configuration: `WORKER_LEGACY` and
  `CHAOS.reorder_hold_ms`.
- **Review of phase 3 (contract section 9.7).** Run fields: `calls` and
  `waiting_on[].inherited`. SSE events: `remote_released` and `remote_attached`.
  Worker events that the server acts on: `remote_wait` and
  `hello.runtime_build`. `--remote-result` and the `remote_result` command
  carry `ts`.
- **Contract section 10.** The site server forwards the new worker fields
  unchanged, because it forwards every worker event as it is. It records the
  call site of each remote call in the run record: `waiting_on[].file`,
  `waiting_on[].line`, `calls[].file`, and `calls[].line`. The values come from
  the `remote_call` event, and a re-dispatch from `calls` (a restored wait that
  this site has to place again) carries them along, so the cause of a call
  survives a resume, a fork, a migration, and a page reload. A record that an
  older server wrote is loaded with `null` in the four fields.

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
| `MOCK_RUNTIME_BUILD=<s>` | the runtime build that the mock writes into program store entries and expects in them (default `mock-worker/1`) |

The run argument `"mock_no_call_site": true` makes every `remote_call` and
`remote_cancel` of the run report `file: null` and `line: null`, which is what
a real worker does when no user frame can be attributed. The run argument
`"mock_cancel_cause": "<cause>"` overrides the `cause` that the mock would
classify, so that a scene can exercise `unknown`.

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

The mock implements contract section 9.2. It accepts `--sleep-suspend-ms`,
`--program-store`, and `--program-hash`, and `--project` is optional with
`--program-hash` and with `--resume` plus `--program-store`.

- **Threads.** The functions of `quotes.baml` run on several simulated threads.
  Each spawned remote call has its own thread (`thread_started`, `remote_call`,
  `thread_ended`), the combinators `all`, `race`, and `all_settled` have a
  collector thread, and `with_timeout` has a body thread and a deadline thread.
  A snapshot holds the calls, their results, the cancelled calls, the threads,
  and the absolute deadlines of the sleeps, so a pause or a suspend at any wait
  resumes correctly. A resumed worker announces its live threads again.
- **Self-suspend.** When the main flow parks, every thread of the mock is in a
  wait that can be re-issued. A durable run then suspends itself if its
  earliest sleep is at least `--sleep-suspend-ms` away: it writes a snapshot,
  emits `paused` with `wake`, and exits with code 75. The threshold is compared
  with simulated time, so `MOCK_SPEED` does not change which sleeps suspend,
  and `wake.remaining_ms` is real time. With `mock_blocked` the suspend is
  answered with one `blocked` event, and the run sleeps in process. A sleep
  whose deadline has passed at the resume completes at once.
- **Cancellation.** `race` cancels the losers, `all` cancels the pending inputs
  after the first error in input order, and `with_timeout` cancels its body.
  Each cancelled call produces `remote_cancel {call_id, thread, file, line,
  cause}` and `thread_ended`, and a later result for it is ignored. The `cause`
  is `future_cancel` for a `race` loser and for an input that `all` drops (both
  cancel the input's own future; `all` runs `futures.map((g) -> { g.cancel() })`
  in its catch arm, and the inputs were spawned by the calling function, not by
  the helper thread that `all` runs in), and `token` for `with_timeout`, and
  `file`/`line` repeat the call site that was recorded when the call was made
  (contract section 10.1).
- **Call sites.** `remote_call` carries the `file` and `line` of the call and
  `caller`, the function the call is written in, and `thread_started` carries
  the `spawn` site in the parent thread. The root thread of a segment carries
  `null` for both. A restored thread reports the `spawn` site again, because it
  travels in the snapshot.
- **Program store.** The "program" of the mock is a JSON payload with the text
  of the project's `.baml` files, and its hash is the SHA-256 of that payload.
  A start from `--project` compiles, stores the entry, and reports the hash in
  `hello.program_hash`. A start with `--program-hash` and every resume load the
  payload from the store. The function must be declared in the loaded payload,
  so a run by hash really depends on the store. A missing entry without
  `--project` fails with `program <hash> is not in the store`. An entry with
  another runtime build, or with bytes that do not have the hash, counts as
  missing. `resumed.stats.program_source` is `store` or `compile`. The
  mock-only `hello` fields `mock_program_source` and `mock_project` let
  `verify.mjs` check where the program came from and that no `--project` was
  passed.
- **Values.** `remote_get_quote` takes a `QuoteRequest` object and returns a
  `Quote` object with the values that `quotes.baml` computes. The mock checks
  the shape of the argument. An enum value is its name as a JSON string.

Mock-only run arguments for these functions: `"mock_nap_ms": n` (the sleep of
`durable_fan_out` in simulated milliseconds), `"mock_unavailable": "<kind>"`
(that vendor of `durable_fan_out` refuses), and `"mock_fail_after_ms": n` (the
run fails after `n` simulated milliseconds while its children still run).

`remote_fetch_weather` fails in the mock for the city `Atlantis`. This
exercises `failed`, the error form of `remote_result`, and a failing parent. A
non-durable function answers `pause` with a `blocked` event and keeps running.
Position events point at the real lines of `program/baml_src/trip.baml` and
`program/baml_src/quotes.baml`. Update the `LINES` table in the mock when one of
those files changes.

## Demo program

```bash
cd program
baml run durable_plan_trip -- --city Lisbon
baml run plan_trip -- --city Lisbon
baml run durable_plan_trip_parallel -- --city Lisbon
baml run remote_fetch_weather -- --city Lisbon
```

Under a plain `baml run` the `durable` and `remote_` markers have no effect.

`baml_src/quotes.baml` holds the functions of contract section 9.4. Their
arguments and results are classes with an enum, a nested class, an optional
field, arrays, and maps.

| Function | Content |
|---|---|
| `remote_get_quote(request: QuoteRequest) -> Quote` | a class in and a class out. It sleeps `request.delay_ms` and throws the typed error `QuoteUnavailable` when `request.options` holds `"simulate": "unavailable"`. |
| `durable_fan_out(city) -> TripReport` | spawns four `remote_get_quote` calls, sleeps 12 seconds (the run suspends itself), awaits `baml.future.all`, and returns the four quotes, their total, a map of vendors, and the cheapest quote. |
| `durable_race(city) -> Quote` | `baml.future.race` over three calls of 2, 6, and 9 seconds. The two losers are cancelled. |
| `durable_settled(city) -> SettledReport` | `baml.future.all_settled` over three calls of which the second throws. Under the worker the failure of a remote child arrives as `baml.errors.Io` with the text of the child's error, so the function handles both error types. |
| `durable_deadline(city) -> string` | `baml.future.with_timeout` of 2 seconds around a call of 6 seconds. The result is a message built from the `Timeout` error. |
| `durable_nap(seconds: int) -> string` | one `println`, one `sleep`, one `println`. |

```bash
baml run durable_fan_out -- --city Lisbon
baml run durable_race -- --city Lisbon
baml run durable_settled -- --city Lisbon
baml run durable_deadline -- --city Lisbon
baml run durable_nap -- --seconds 2
baml run remote_get_quote -- --json-args '{"request":{"city":"Porto","kind":"Car","nights":2,"delay_ms":100,"traveler":{"name":"Bo","loyalty_tier":null},"options":{}}}'
```

The helper functions are static functions of the class `Catalog`. `GET
/api/info` lists the top-level functions of the program, so only the functions
that a run can start appear in the app.

## Verification

### Mock worker

```bash
node scripts/verify.mjs              # about 8 minutes, 404 checks
ONLY=phase3 node scripts/verify.mjs  # the scenes of contract section 9 only (not timed again after the review scenes were added)
ONLY=chaos node scripts/verify.mjs   # the CHAOS scenes only: about 2 minutes 30 seconds, 74 checks
MOCK_SPEED=3 node scripts/verify.mjs # faster simulated sleeps (the CHAOS scenes always run at speed 1)
PORT_BASE=28787 RUNS_TAG=-b node scripts/verify.mjs   # other ports and run store names
```

The script starts its own three site servers on `PORT_BASE`, `PORT_BASE + 1`,
and `PORT_BASE + 2`. Its default `PORT_BASE` is 18787, not the 8787 of
`dev.sh`, its run stores are `server/.baml/verify<RUNS_TAG>-<site>`, and its
program stores are `server/.baml/verify-programs<RUNS_TAG>-<site>`, so it can
run while the demo is in use. Before it deletes or starts anything it
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
- A parent on `local` calls into one site of the pool. The scenes take that
  site from `remote_dispatched.child_site`, because placement is round-robin.
  No child appears on `local` or on the other pool site.
- A parent that migrated to `cloud` makes its remote call from there. The child
  runs on `cloud2`, the result returns to `cloud`, and the run completes. The
  local record carries `migrated_to: "cloud"`.
- A migration chain `local` -> `cloud` -> `cloud2` while the child still runs
  on its pool site. The run is paused on `cloud2` and has no process when the child
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

The scenes for contract section 9 (`ONLY=phase3` runs them without the scenes
of sections 3 to 8, and `ONLY=chaos` runs the CHAOS scenes alone). Each scene
is wrapped, so a wait that times out counts as one failed check and the next
scenes still run. The servers run with `SLEEP_SUSPEND_MS=2500`, so the loop
sleeps of `trip.baml` and the 2 second deadline stay in process and a nap of 3
seconds suspends the run.

- **Program fetch and `all_settled`.** The first remote calls of the instance.
  `cloud` and `cloud2` each fetch the program from `local` once, although two
  children reach one site at the same time. The stored entries are
  byte-identical on the three sites, `GET /api/programs/:hash` serves bytes
  with that hash, the children run with `--program-hash` and without
  `--project`, and a second use fetches nothing. `durable_settled` returns two
  whole `Quote` objects and one failure, the failing child has the typed error,
  the class-valued argument reached the child intact, and no child is cancelled.
- **Nap.** `sleeping`, `wake` on the `paused` event, a server-owned `wake_at`,
  exit code 75, `sleep_scheduled`, `meta.json`, 409 for `kill` and `pause`, one
  `woken {reason: "timer"}` that is not early and not late, segment 2 in a new
  process with `program_source: "store"`, and the output of an uninterrupted
  run.
- **Manual resume.** Far from the deadline the new worker suspends again, the
  deadline does not move, and the timer of the first suspend starts nothing.
  Close to the deadline the worker sleeps the rest in process, and the timer
  that fires later does nothing.
- **Pause with four threads, and a fork of a sleeping run.** `durable_race` is
  paused after its three dispatches. The StateDump shows five threads, the
  winner's result is stored while no process exists, and no child is cancelled
  in that time. Segment 2 takes the stored result, returns the winner, and
  cancels the two children that segment 1 dispatched. A fork of a sleeping run
  is `paused` without `wake_at`, suspends for the same deadline on its own
  after a resume, and completes.
- **The worker's store rules, on the mock alone.** Compile and store, start by
  hash without `--project`, `program <hash> is not in the store`, an entry of
  another runtime build, an entry with wrong bytes, the repair with
  `--project`, and a `--project` that compiles to another hash.
- **Compare-and-set.** Five resume requests at once get one 200 and four 409.
  The process table shows at most one worker of the run, and every segment has
  one `hello`. Three more runs get resume requests a few milliseconds before,
  at, and after `wake_at`: one `woken` event, one worker for segment 2, and no
  segment 3.
- **Cancel without a process.** A sleeping run, a paused run, and a sleeping
  remote child become `cancelled`. The timer of the cancelled run fires into
  nothing. A second cancel and a resume return 409.
- **Fan-out.** Round-robin placement in call order (`c1` and `c3` on one pool
  site, `c2` and `c4` on the other), five threads and class instances in the
  StateDump, children that run while the parent has no process, four whole
  `Quote` objects stored on the sleeping run, all four delivered once in
  segment 2, and a `TripReport` that equals the one of `baml run` (quotes in
  input order, total, the map of vendors, the cheapest quote with its nested
  request).
- **Race.** The winner's `Quote`, `remote_cancel` and `remote_cancelled` once
  per loser, exit code 130 of the losers on their sites, one
  `remote_result_discarded` per loser, only the winner's result on the record,
  and `result_delivered` on every child.
- **Deadline.** The message from the `Timeout` error, no suspend, the child
  cancelled, and its late result discarded.
- **`baml.future.all` with a failing child.** The parent fails with the typed
  error, and the child that still ran is cancelled.
- **A sleeping run moves to another site.** The destination suspends again and
  computes its own `wake_at` for the same deadline, the program comes from its
  store, and the timer on the origin starts nothing.
- **Cascade.** Three parents end as `failed`, `cancelled`, and `lost` with
  three children each. The server cancels all of them without a `remote_cancel`
  from the worker, and their late answers are discarded. The cancel of a
  sleeping parent cancels its four children.
- **Restart while runs sleep.** The local server stops while two runs sleep.
  The run whose deadline passed resumes at once (`woken {reason: "restart"}`).
  The other gets `sleep_scheduled` again with the unchanged `wake_at` and wakes
  by its timer.
- **Program store through the scripted site.** The dispatch of a remote call
  is answered with 200 as soon as the run exists. A program whose bytes do not
  have the requested hash ends the child as `failed` with the reason, nothing
  is stored, no worker starts, and the parent's site gets the reason as the
  error of the call. The same holds for a program that the sender does not
  serve, for a program of another runtime build (the error names both builds),
  and for a program without a usable runtime build. A `program_hash` that is
  not 64 hex characters is refused with 400. A peer that serves the right
  bytes with garbage in `header_base64` gets an entry with the documented
  header, and a worker runs it. A program with the right hash is fetched,
  stored, and run: `cloud2` executes a program that exists nowhere as source
  on its disk. No temporary file is left in the shard.
- **Late commands.** `durable_nap` with `mock_exit_delay_ms: 700` keeps its
  process for 700 ms after `paused`. A `cancel` in that window is accepted and
  ends the run as `cancelled`: it never sleeps, wakes, or completes. A `pause`
  in that window leaves the run `paused` without a wake timer, and it resumes
  by hand.
- **A store entry that disappears.** A remote child that runs by hash sleeps on
  `cloud`, and its store entry is deleted. The timer resume fetches the program
  again from the parent's site, and the run completes from the store.
- **Shared children.** A sleeping fan-out is forked. Cancelling the fork
  cancels no child (`remote_released` four times, no `remote_cancelled`), and
  the source completes with four quotes. In the mirror image the source is
  cancelled, its fork still waits, no child is cancelled, and the resumed fork
  completes with four quotes. A fan-out without a fork cancels its four
  children as before.
- **Restored waits.** A resumed fan-out reports four `remote_wait` events with
  `has_result: true`, and takes the stored results in the order of their `ts`.
- **CHAOS.** All three servers restart with `dispatch_delay_ms: 600`,
  `result_delay_ms: 500`, `cancel_delay_ms: 700`, `import_delay_ms: 500`,
  `program_fetch_delay_ms: 400`, `duplicate_results`, and `reorder_results`,
  and with empty program stores on `cloud` and `cloud2`. The scenes program
  fetch and `all_settled`, fan-out, nap, race, deadline, the move of a sleeping
  run, and cascade run again, and no call returns twice on any run. Two checks
  are wider under CHAOS, because cancellation is a request: the cancel reaches
  the 6 second loser of the race, and the 2 second child of a parent that ends,
  about one second before that child is done, and one retried HTTP request uses
  that margin up. Such a child may complete, and its result is discarded like
  that of a cancelled child. The slower children must be cancelled. The CHAOS
  scenes always run the workers with `MOCK_SPEED=1`, because the delays are
  real time. Then `local` restarts with `dispatch_delay_ms: 3500`, which
  is longer than the deadline of `durable_deadline`: the run completes with the
  `Timeout` message before its child exists, and the child that appears later
  is cancelled right away and never enters `waiting_on`. A fan-out under that
  slow dispatch suspends before any child exists and still completes correctly.
  A fork that is taken at that moment lists no child and no result. After its
  source has completed, the resumed fork reports four `remote_wait` events and
  completes with the copied results and no new children. A fork that resumes
  while the children of its source still run attaches to them
  (`remote_attached`), and both runs complete with the same report from four
  children. At the end `cloud` and `cloud2` restart with empty program stores
  and `program_fetch_delay_ms: 12000`, which is longer than the 10 s that a
  dispatch request waits: `plan_trip` still completes with one child and one
  fetch.

The scene for contract section 10 ("call sites") checks that a `remote_call`
carries the line of `program/baml_src/quotes.baml` that made the call, that a
`thread_started` carries the `spawn` site and the root thread of a segment
carries none, and that the site server records both in `waiting_on` and in
`calls`. It reads the same values back from `GET /api/runs/:id/events`, which
is what a page reload reads, and from `meta.json` after a pause, so the call
site survives a resume. It also checks that `remote_call` names the calling
function in `caller` and that the record keeps it. It checks the `cause` of
every cancellation: a `race` loser is `future_cancel`, `with_timeout` is
`token`, and the inputs that `baml.future.all` drops after an error are
`future_cancel` too, because `all` cancels their futures rather than a parent
thread (the engine test `bex_engine::durable_cause::an_input_that_all_drops_after_an_error_reports_a_cancelled_future`
pins that against the real runtime). A run started with the
mock-only arguments `mock_no_call_site` and `mock_cancel_cause` shows that a
worker which cannot attribute a call writes `null` (never a guess) and that a
cancellation it cannot classify is reported as `unknown`.

The last scene of sections 3 to 8 covers the routing cases that need a request in flight. The
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
node scripts/verify-real.mjs                 # 248 checks in 6 to 11 minutes; 151 checks in about 3 minutes with a worker from before contract section 9
LEGACY=1 node scripts/verify-real.mjs        # skip the section 9 scenes, and start the servers with WORKER_LEGACY=1
USE_RUNNING=1 node scripts/verify-real.mjs   # against the three servers that dev.sh started with the real WORKER_CMD (PORT_BASE defaults to 8787 here)
ONLY=b,c node scripts/verify-real.mjs        # selected scenes
CHAOS_ALL='{"dispatch_delay_ms":600,"result_delay_ms":500,"cancel_delay_ms":700,"import_delay_ms":500,"program_fetch_delay_ms":400,"duplicate_results":true,"reorder_results":true,"reorder_hold_ms":1200}' \
  ONLY=x,a,b,d,g,h,j,m,q,r,v,w,1,2,3,4,5,6,7 node scripts/verify-real.mjs   # the same scenes with a slow controller on every site
RECORD=/tmp/events.jsonl KEEP=1 node scripts/verify-real.mjs   # keep the SSE events and the run stores
```

`WORKER_BIN` selects another binary (default `../target/debug/baml-cli`). The
script starts its own three site servers on `PORT_BASE` (default 18787, so it
does not collide with a demo on 8787) with run stores in
`server/.baml/verify-real<RUNS_TAG>-<site>` and program stores in
`server/.baml/verify-real-programs<RUNS_TAG>-<site>`, and it prints the measurements of
the next section at the end. It checks its ports and claims its run stores in
the same way as `verify.mjs`, and it stops its servers on `SIGINT`, `SIGTERM`,
and `SIGHUP`. Its check for leftover workers looks only at processes whose
`--snapshot-dir` lies in its own run stores. With `USE_RUNNING=1` the run
stores belong to a demo that may be in use, so the check is skipped and no run
store is deleted. The scenes:

| Scene | Content |
|---|---|
| a | `durable_plan_trip` completes. The remote child runs on a site of the pool. Automatic snapshots are recorded. |
| b | The central scene. The parent is paused during the remote wait. The worker process is gone, and the StateDump shows `city`, `day`, and `ideas`. The child completes while no parent process exists, and its result is stored. The resume starts a new pid, and the run completes with the correct `TripPlan`. The remote call is announced and dispatched once. |
| c | A pause inside the loop. The thread is parked in `sleep` with an absolute deadline. After the resume every `println` line appears exactly once. |
| d | Migration. The run is paused on `local` and resumed with `{"site":"cloud"}`. The snapshot bytes and the StateDump are identical on both sites. The remote call of the migrated run is placed on `cloud2`, and the result returns to `cloud`. The local record carries `migrated_to: "cloud"`. |
| e | Fork of a paused run. The source and the fork complete in separate processes, and each makes its own remote call. |
| f | Kill. `plan_trip` becomes `lost`. `durable_plan_trip` becomes `paused` from its latest automatic snapshot and completes after a resume. A durable run without a snapshot becomes `lost`. |
| g | `durable_plan_trip_parallel`. The run pauses with two threads: the root in its loop and the spawned thread parked in its remote call. No `blocked` event is emitted. The StateDump lists both threads with the parent of the spawned one, and the local `weather_future` as a pending future. After the resume both threads are reported with their original ids, the remote call is dispatched once, every `println` line appears once, and the run completes. A worker from before contract section 9 answers this pause with `blocked`, and the scene then checks that behavior. |
| h | Kill during the remote wait. The latest automatic snapshot predates the call, so the resumed worker repeats `remote_call` with the same `call_id`. The server does not dispatch it again and answers from the stored result. |
| i | Cancel: the `cancelled` event, then exit code 130. |
| j | Two pauses in one run (loop, then remote wait). The result arrives on stdin of segment 3. |
| k | A pause request that arrives while the worker still loads the program. The run pauses at its first clean point. |
| m | Migration chain `local` -> `cloud` -> `cloud2` during the remote wait. Each import is paused again right away (the pause request waits while the worker loads the program). The run is paused on `cloud2` without a process when the child completes on its pool site. The result is forwarded `local` -> `cloud` -> `cloud2` and stored, and the run completes in segment 4. The two hops took 2.8 s in one run, and the child needs about 4.3 s, so the scene depends on that margin. While the run is paused on `cloud`, an import of the same id into `cloud` returns 409 and changes nothing. At the end a `remote_result` whose `via` names a site of the chain is answered 502 and reaches no record, and one without `via` still follows the chain. |
| n | A run on `cloud2` calls into `cloud`. Nothing of it touches `local`. |
| o | A resume with an unknown site name, or with `site: 5`, returns 400, also on a completed run (a known name returns 409 there). A remote call whose `parent.site` is not in `SITES` returns 400. A resume with the run's own site name resumes in place. |
| l | The local site server stops and restarts during a durable run. The worker exits, the run is `paused` after the restart, and it completes after a resume. Skipped with `USE_RUNNING=1`. |
| p | `cloud2` restarts with `REMOTE_POOL=["cloud2"]`. `plan_trip` and `durable_plan_trip` on it fail with `baml.errors.Io {message: "remote call to `user.remote_fetch_weather` failed: no remote site available"}`, and the `failed` event carries the line of the remote call. Skipped with `USE_RUNNING=1`. |

The scenes of contract section 9 run when `baml-cli worker --help` lists
`--program-store`. With an older binary, or with `LEGACY=1`, the script starts
the servers with `WORKER_LEGACY=1`, skips these scenes, and says so.

| Scene | Content |
|---|---|
| x | Runs first. Two `plan_trip` parents reach both pool sites. `cloud` and `cloud2` fetch the program once each, the entries are byte-identical, the command line of a child worker has `--program-hash` and `--program-store` and no `--project`, `GET /api/programs/:hash` serves bytes with the hash and a `runtime_build` that is not empty (the server parsed the header), and a second use fetches nothing. |
| q | `durable_nap` of 7 seconds: `sleeping`, `wake`, `wake_at`, exit code 75, one `woken {reason: "timer"}` at `wake_at`, segment 2 with `program_source: "store"`, each `println` line once, and a sleep that was not cut short. |
| r | Five resume requests at once on a sleeping run (one 200, four 409, one worker for segment 2, which suspends again), completion in segment 3, and cancel of a sleeping run. |
| s | `durable_fan_out`: two children per pool site, class-valued arguments, four results stored while no process exists, the `TripReport` of an uninterrupted run, and each call dispatched, returned, and received once. Every child reports `program_source: "store"` and no compile time in its `hello`, carries the parent's `program_hash`, and ran from an entry that its site fetched from `local`. |
| t | `durable_race`: the winner's `Quote`, the losers cancelled on their sites, late results discarded. |
| u | `durable_deadline`: the `Timeout` message, the child cancelled. |
| v | `durable_settled`: two quotes, one failure with the text of the remote error, no cancellation. |
| w | A cancelled parent and a cancelled sleeping parent cancel their children. |
| 1 | `durable_race` is paused after its three dispatches, and all three children finish while it is paused. The run and four forks of it are resumed: all five executions return the 2 second vendor, take only that result, and abandon the other two calls. |
| 2 | A sleeping `durable_fan_out` is resumed by hand after two results have arrived. Segment 2 reports four `remote_wait` events and takes the stored results before it suspends again, and the run completes with four quotes in input order, each result received once. |
| 3 | Four `durable_nap` runs get a `cancel` as soon as their `paused` event appears, which is before the worker process has exited. Every cancel is accepted, and every run ends as `cancelled` without a wake timer. |
| 4 | A sleeping fan-out is forked and the fork is cancelled: no child is cancelled, and the source completes with four quotes. A second fork is resumed next to its sleeping source: both complete with the same report from the same four children. |
| 5 | `durable_plan_trip_parallel` is paused while its spawned remote call is outstanding (two threads, one `waiting_on` entry) and resumed with `{"site":"cloud2"}`. `cloud2` fetches the program and runs segment 2 from its store without a compile. The restored thread reports `remote_wait` under the original call id `<run>-c0.0-1`, and no second `remote_call` is made. Both threads are announced with their original ids, the parent first. The child's result follows the run to `cloud2` (one dispatch, one child, one `remote_returned`), and the run completes there with every `println` line once. |
| 7 | Resume timings. `durable_plan_trip` is paused and resumed twice. The first resume loads the program from the store (`program_source: "store"`, `program_compile_ms` null). Before the second resume the scene deletes the store entry, so the worker compiles `--project`, checks the hash, and stores the program again (`program_source: "compile"`). The run completes in segment 3. |
| 6 | Runs after scene p. `local` restarts with `SLEEP_SUSPEND_MS=600000`, so the 12 second sleep of `durable_fan_out` stays in its process. The first automatic snapshot holds five threads. The scene waits for a result that the worker takes after the second automatic snapshot and then sends `kill`. The run is `paused` at that snapshot, the other children finish while no process exists, and the resume reports a `remote_wait` for every call that was outstanding in the snapshot. That includes the call whose result segment 1 had already taken and acknowledged: the server delivers it again. The run completes with the `TripReport` of an uninterrupted run, nothing is dispatched twice, and no `println` line from before the snapshot is repeated. Skipped with `USE_RUNNING=1`. |
| y | The local server restarts while two runs sleep (`restart` and `timer`). Skipped with `USE_RUNNING=1`. |
| z | Scenes s, t, and u under CHAOS with empty program stores on the cloud sites. Skipped with `USE_RUNNING=1`. |

These scenes compare results with `isDeepStrictEqual` against the values that
`quotes.baml` computes. The worker writes an enum value as its name
(`"Hotel"`) in `remote_call.args` and in `completed.value`, and the `failed`
event of a child that throws `QuoteUnavailable` carries the rendered value,
including its `reason` text.

One run with the section 9 worker (debug build) passed all 248 checks: the 16
scenes above, scenes x q r s t u v w 1 2 3 4 5 6 7 y z, and the invariants.

`CHAOS_ALL` gives every site server of the run the named `CHAOS` object, so
that any scene can be repeated with a slow controller. Scene z always runs
scenes s, t, and u that way. One run with the object above and
`ONLY=x,a,b,d,g,h,j,m,q,r,v,w,1,2,3,4,5,6,7` passed all 157 checks: dispatches
600 ms late, results 500 ms late, sent twice and out of order, cancels 700 ms
late, imports 500 ms late, and program fetches 400 ms late. Scenes that assert
exact timing (c, f, k, t, u outside scene z) are not part of that list.

The web app has its own checks (`web/README.md`). `LIVE=1 pnpm check:headless`
drives the app in a headless browser against the site servers. With the real
worker it passed all 890 checks: the fixture scenes, the live scenes `pool`,
`chain`, `skip`, and `central`, and all seven scenarios of the gallery with
autoplay (`LIVE_GUIDE=migrate,fanout,race,deadline,settled,recover,fork`). The
`central` scene pauses `durable_plan_trip_parallel` and accepts both answers:
`paused` from a section 9 worker, and `blocked` from an older one. The seven
scenarios also passed against site servers that ran with the `CHAOS` object
above.

## Release build against debug

Build the worker with `cargo build --release -p baml_cli` for a demo. The
figures below were measured on Apple M5 Max, macOS 26.6, with the three site
servers and the web app running, so they carry the load of the machine they
were taken on (load average 26 during the run). Read them as a comparison
between the two builds rather than as a floor for the runtime.

| Operation | debug | release |
|---|---|---|
| Start a run through the site server, which compiles the project | about 2100 ms | 502 ms |
| Start a worker from the program store with no project directory | 99.9 ms | 32.9 ms |
| Resume request to a live process, measured at the site server | — | 10.3 ms |
| `program_load_ms` of a resume, from the store | about 1200 ms | 87 to 130 ms, median 100 ms over 8 resumes |
| `decode_ms` of a resume, the snapshot itself | 0.49 ms | 0.09 ms |
| Pause latency and snapshot size, `durable_plan_trip` | — | 0.07 ms, 565 bytes |
| Binary size | 581 MB | 64.8 MB |

The `release` profile of this workspace sets `opt-level = "s"`. A `fasttest`
build (`opt-level = 2`, thin LTO) was measured against it on the same
machine and gave the same `program_load_ms`, 81 ms median for both, with a
binary of 260 MB. The release profile is therefore the better choice: the
same speed in a quarter of the size, which also shortens a cold start.

What is left in a resume is the engine build, not the snapshot. Decoding the
snapshot is under a tenth of a millisecond, while building an engine from the
program is the rest of `program_load_ms`. Neither a compiler flag nor a
faster snapshot format moves that number. The two changes that would are a
worker pool that keeps built engines in memory keyed by program hash, and a
shared stdlib image per runtime build, since a store entry is about 99.9%
stdlib. Both are described in the design document and neither is built.

## Measurements with the real worker

Debug build of `baml-cli` (`cargo build -p baml_cli`, unoptimized), Apple M5
Max, macOS 26.6, demo programs `program/baml_src/trip.baml` and `quotes.baml`,
all site servers on the same machine. The numbers come from one
`verify-real.mjs` run with the section 9 worker (248 checks, 0 failures). Every
resume of that run loaded the program from the program store, except the one
resume of scene 7 whose store entry the scene had deleted.

PauseStats (`paused` event):

| Field | Scene b: paused in the remote wait | Scene c: paused in the loop (`sleep`) |
|---|---|---|
| `pause_latency_ms` | 0.066 | 0.158 |
| `walk_ms` | 0.062 | 0.286 |
| `encode_ms` | 0.033 | 0.117 |
| `compress_ms` | 0.045 | 0.131 |
| `write_ms` (both files, atomic rename) | 0.370 | 0.513 |
| `objects` | 7 | 5 |
| `raw_bytes` (state section) | 753 | 619 |
| `compressed_bytes` (state section, zstd) | 452 | 411 |
| `file_bytes` (`snap-<n>.bamlsnap`) | 596 | 555 |
| `program_bytes` (not embedded in the file) | 2,735,827 | 2,735,827 |
| `blocked_attempts` | 0 | 0 |
| `threads` | 1 | 1 |

Snapshots of runs with several threads, from the same run:

| Field | Scene g: two threads, pending future | Scene s: `durable_fan_out` suspends itself, five threads | Scene 6: automatic snapshot of the fan-out, five threads | Scene q: `durable_nap` suspends itself |
|---|---|---|---|---|
| `pause_latency_ms` | 0.076 | 0.068 | null (`park_ms` 0.146) | 0.025 |
| `walk_ms` | 0.109 | 0.110 | 0.169 | 0.060 |
| `encode_ms` | 0.057 | 0.065 | 0.097 | 0.034 |
| `compress_ms` | 0.068 | 0.081 | 0.110 | 0.051 |
| `write_ms` | 0.421 | 0.401 | 0.847 | 0.285 |
| `objects` | 6 | 10 | 10 | 1 |
| `raw_bytes` | 1047 | 2376 | 2376 | 523 |
| `compressed_bytes` | 575 | 679 | 686 | 350 |
| `file_bytes` | 719 | 823 | 830 | 494 |
| `threads` | 2 | 5 | 5 | 1 |

The five threads of the fan-out, their four pending futures, and the
class-valued requests fit in 823 bytes. The same run under CHAOS (scene z)
wrote 814 bytes.

Self-suspend (scenes s and q). The worker decides by itself, so there is no
request to measure from:

| Interval | Scene s: fan-out, five threads | Scene q: `durable_nap` |
|---|---|---|
| last `remote_call` event (scene s) or `hello` (scene q) to the `paused` event | 2 ms | 6 ms |
| `paused` event to `worker_exit` (process gone, both pipes drained) | 62 ms | 273 ms |
| snapshot file | 823 bytes | 494 bytes |

The parent writes its snapshot about 2 ms after its last `remote_call`, which
is before the site server has placed the children: the four
`remote_dispatched` events carry timestamps 40 to 70 ms after the `paused`
event.

ResumeStats (`resumed` event):

| Field | Scene b | Scene c | Scene 7, from the store | Scene 7, store entry deleted |
|---|---|---|---|---|
| `process_start_ms` | null (not measured) | null | null | null |
| `program_source` | `store` | `store` | `store` | `compile` |
| `program_load_ms` (read and verify the store entry, decode, build the engine; or compile) | 128.8 | 128.6 | 130.7 | 1294.4 |
| `program_store_read_ms` | 12.7 | 12.5 | 11.7 | 0.015 (miss) |
| `program_decode_ms` | 38.5 | 38.3 | 37.8 | null |
| `program_compile_ms` | null | null | null | 1179.0 |
| `program_store_put_ms` | null | null | null | 32.1 |
| `program_engine_ms` | 77.5 | 77.7 | 81.2 | 82.7 |
| `decode_ms` (read, decompress, allocate, restore) | 0.446 | 0.440 | 0.475 | 0.497 |
| `first_exec_ms` (worker entry to the first `exec()`) | 130.0 | 129.8 | 132.0 | 1295.7 |

A resume from the store is ten times faster than a resume that compiles
(131 ms against 1294 ms for the program load). Scene 7 measures both on one
run: it deletes the run's store entry before the second resume, the worker
compiles `--project`, checks that the hash is the one in the snapshot header,
and stores the program again.

Wall time, measured by `verify-real.mjs` from outside:

| Interval | Scene b | Scene c |
|---|---|---|
| `POST pause` to the `paused` event | 8 ms | 10 ms |
| `POST pause` to status `paused` (process exited, both pipes drained) | 71 ms | 65 ms |
| `POST resume` to the HTTP response | 17 ms | 12 ms |
| `POST resume` to the first event of the new segment (`hello`, by worker timestamp) | 15 ms | 15 ms |
| `POST resume` to the same event on the SSE stream | 17 ms | 16 ms |
| `POST resume` to `resumed` and the first program event | 145 ms | 145 ms |

From the wake timer to the run (the timer fires at `wake_at`, the site server
starts a worker, the worker loads the program from the store and restores the
snapshot):

| Interval | Scene q: `durable_nap` | Scene s: fan-out with four stored results |
|---|---|---|
| `wake_at` to the `woken` event | 1 ms | not recorded |
| `woken` to `hello` of the new process | 20 ms | 13 ms |
| `woken` to `resumed` and the first program event | 165 ms | 141 ms |
| `woken` to `completed` (scene s: four results replayed in arrival order, `baml.future.all`, the report) | not recorded | 146 ms |

A resume on another site has the same cost once that site holds the program.
In scene 5 the run with two threads moved from `local` to `cloud2`: the
HTTP response came after 14 ms, `hello` on `cloud2` after 10 ms by worker
timestamp, and `resumed` after 138 ms, with `program_load_ms` 126.1 from the
store of `cloud2`. The recovery of scene 6 (kill -9, five threads in the first
automatic snapshot) resumed with `program_load_ms` 131.8 and `decode_ms` 0.72.

Over the scenes of that run, the resumes that loaded the program from the
store had `program_load_ms` between 126 and 144 and `decode_ms` between 0.43 and
0.72. An earlier run on the same machine, with less load, measured 91 to 99 ms
for the same step, of which the engine build was 40 ms instead of 78 ms. A start
from `--project` still compiles: `POST /api/runs` to `hello` is 1351 ms,
because `hello` carries the program hash and is therefore emitted after the
compile. The first fetch of the 2.7 MB program from another site, with
verification and the write to the store, took 42 ms on `cloud` and 34 ms on
`cloud2`. The snapshot work is below 1 ms in total, apart from the file write.
An optimized build loads a program from the store in about 23 ms, of which
18 ms are the engine build (`crates/bex_program_store`, crate documentation).
The release build was not measured end to end.

## Known limitations

Found or confirmed by the end-to-end runs with the real worker:

- A value that cannot be serialized still blocks a snapshot: an open file or
  socket, a host closure, a future that ended with an internal engine error,
  and the continuation frames of the runtime compiler in `baml.reflect`. The
  pause request is answered with `blocked` and is retried at later yields. A
  blocked self-suspend is reported once, and the run sleeps in its process.
  Runs with several threads, futures in every state, cancel tokens, task
  groups, and the array combinators pause and resume (scene g and the section 9
  scenes).
- A resumed run replays what completed while it had no process one item at a
  time and releases the next item when the run is quiescent. A thread that is
  inside an operation that cannot be re-issued (an HTTP call, for example)
  holds the replay until that operation ends, so a long external call delays
  the delivery of recorded results.
- The StateDump lists the threads in the order in which they parked, so the
  root thread is often the last entry. A thread that was inside `await` is
  recorded as `parked.kind: "runnable"`, because the resumed process executes
  the await again. The dump does not name the future that such a thread waits
  on. The web app sorts the threads and explains `runnable` in a tooltip.
- The worker reports every live thread with `thread_ended` right before its
  `paused` event and announces the restored threads again after the resume. A
  reader that wants to draw one thread across a pause has to join the two by
  thread id, as the web app does.
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
- A start from `--project` compiles (about 1.25 s in the debug build), and
  `hello` is emitted after that compile. Every resume and every start with
  `--program-hash` loads the program from the program store (about 95 ms in the
  debug build). The snapshot does not embed the program. A paused run keeps
  working after an edit of `program/`, because its program stays in the store
  under its hash. A resume refuses a snapshot when the runtime build differs.
  The runtime build is the package version plus the git sha when `BAML_GIT_SHA`
  is set, so two development builds with different layouts share one name.
- The program store is never collected. `bex_program_store` offers `entries()`
  and `remove()` for a collector that decides from the run stores which hashes
  are still needed.
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

Site server and mock worker, contract section 9:

- A run suspends only for a `sleep`, and only its timer or a request wakes it.
  A run that waits on remote results without a long sleep keeps its worker
  process. The design document describes the later phase ("Waits on remote
  results"): the parent suspends, and the arrival of a `remote_result` wakes
  it. The site server already stores results for a run without a process and
  passes them at the resume, so that phase needs a wake rule and no new storage.
- A run that ends as `completed` does not cancel children that it still waits
  on. Contract section 9.3 names `cancelled`, `failed`, and `lost`.
- Cancellation is a request. A child that finishes before the cancel reaches
  its site completes, and its effects have happened. Its result is discarded.
- The placement counter lives in memory and starts at zero when a site server
  starts.
- The file layout of a program store entry is fixed by contract section 9.5
  and documented in `server/baml_src/programs.baml`. The server accepts
  nothing else.
- A child run belongs to the run that dispatched it. A fork and its source on
  one site share the children correctly (section "Forks and remote children").
  A fork that moved to another site and dispatches a call of its own there
  owns that child, but the origin site does not know about it. A production
  system would keep the waiters of a child on the child's own site.
- A result that arrives while `launch` starts the worker of a resume is
  written to the worker's stdin right after the process is registered. No
  scene forces that window, which is shorter than a millisecond.
- The state lock is one lock per site server. A body under it is short, but
  every SSE frame takes it once per subscriber.
- The mock worker writes no automatic snapshots for the functions of
  `quotes.baml`. A killed `durable_race` is therefore `lost`, which the cascade
  scene uses.

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
