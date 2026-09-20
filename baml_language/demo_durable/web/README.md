# Durable functions demo: web app

The React app of the durable functions proof of concept (contract sections
3.5, 7.7, 8.4, and 9.6 in `documents/durable-poc-contracts.md`). TypeScript, Vite, React, `strict`
mode, pnpm. The timeline is drawn with SVG. There is no UI kit and no charting
library.

## Run

```bash
pnpm install
pnpm dev            # http://127.0.0.1:5173 (also reachable as http://localhost:5173)
```

The dev server proxies `/local/*` to `http://127.0.0.1:8787/*`, `/cloud/*` to
`http://127.0.0.1:8788/*`, and `/cloud2/*` to `http://127.0.0.1:8789/*` with
the prefix removed. Start the three site servers with `../scripts/dev.sh`. The
app sends commands with `fetch`.

`LOCAL_SITE_URL`, `CLOUD_SITE_URL`, `CLOUD2_SITE_URL`, and `WEB_PORT` override
the proxy targets and the port. They exist for a second instance of the demo
and for tests against stand-in servers. The defaults are the values that the
contract fixes. A second instance next to a running demo looks like this:

```bash
PORT_BASE=18787 RUNS_TAG=-test ../scripts/dev.sh
LOCAL_SITE_URL=http://127.0.0.1:18787 CLOUD_SITE_URL=http://127.0.0.1:18788 \
  CLOUD2_SITE_URL=http://127.0.0.1:18789 WEB_PORT=15173 pnpm dev
```

## Sites

The app does not fix the set of sites (contract section 8). At startup it reads
`sites` from `GET /local/api/info` and uses that list, in registry order, for
the connections, the timeline lanes, the log lanes, the site picker, and the
"Resume on <site>" buttons. When the request fails or does not answer within
2.5 seconds, the app uses `local`, `cloud`, and `cloud2`. It reads the registry
again when the connection to `local` opens, so a `local` server that starts
after the app still names the sites. A server that predates section 8 answers
with `peer_site` instead of `sites`. The app then uses the two sites that the
answer names.

The app opens one `EventSource` per site. A site that is down shows as
"disconnected, retrying" in the header, and its lanes stay in place. The other
sites are not affected. A run on a site that is not in the registry still gets
a timeline lane, after the lanes of the registry.

The dev server can only proxy the three default site names. A registry with
another name needs a matching entry in `vite.config.ts`.

## Scenarios

The "Scenarios" button in the header opens the scenario gallery. A scenario
starts one function of the demo program and guides the user through it. Each
card has a title, a paragraph that says what happens, the function and the
arguments that the scenario starts, its steps, and a small picture of the
timeline that the scenario produces. The picture is drawn from the recording
of the scenario.

| Scenario | Starts | What the user does | Recording |
|---|---|---|---|
| Pause here, resume there | `durable_plan_trip` | Pause, Resume on cloud | `pool` |
| Fan-out with a durable sleep | `durable_fan_out` | nothing: the run suspends itself, and the timer wakes it | `fanout` |
| Race and cancellation | `durable_race` | optional: Pause during the race, Resume on cloud2 | `race` |
| Deadline | `durable_deadline` | optional: Pause, Resume here | `deadline` |
| All settled, one failure | `durable_settled` | optional: Pause, inspect the futures, Resume here | `settled` |
| Kill and recover | `durable_plan_trip`, then `plan_trip` | Kill, Resume here, Start `plan_trip`, Kill | `recover` |
| Fork a run | `durable_plan_trip` | Pause, Fork, Resume here on the source, Resume here on the fork | `branch` |

"Run live" starts the function on the site servers and opens the guide, a bar
under the header. The guide lists the steps of the scenario and shows the hint
of the current step. The hints follow the event stream. "Click Pause now: three
children are running on cloud and cloud2" appears when three children run, and
it names the sites on which they run. While a run sleeps, the hint counts the
children that have finished and counts down to the wake time. A step whose
action is possible highlights its button in the runs panel, and the guide
selects the run that the button acts on. "Play recording" plays the scenario
from its fixture, without servers.

The Autoplay switch performs the hinted actions. It highlights the button for
1.3 seconds and then presses it. The press goes through the button's own code
path, so an error of the command appears where it appears for a click. While a
recording plays, autoplay sends nothing, and the highlight marks the moment at
which the recorded user acted. The switch is remembered in `localStorage`, and
`?autoplay=1` or `?autoplay=0` sets it. `?scenarios=1` opens the gallery at
startup.

A step is done when its condition holds for the event history, so a step never
becomes undone. A step whose moment has passed, because a later step is already
done, counts as skipped. The guide therefore stays in step with a user who acts
early, late, or not at all. The optional pauses of the race, the deadline, and
the all settled scenario work this way: a run that settles without the pause
skips it.

## Fixture mode

The app runs without site servers when the URL names a fixture:

| URL | Scene |
|---|---|
| `/?fixture=central` | The parent on `local` calls `remote_fetch_weather` on `cloud`. The parent is paused while the child runs. The child completes. The parent resumes in a new process and completes. |
| `/?fixture=pool` | The central scene with three sites. The parent starts on `local` and is paused in its loop. The user resumes it on `cloud`. It calls `remote_fetch_weather` there, and the remote pool places the child on `cloud2`. The result returns to `cloud`, and the run completes. |
| `/?fixture=chain` | One run moves over all sites: `local`, `cloud`, `cloud2`, and back to `local`. On `cloud2` it calls `remote_fetch_weather`, which runs on `cloud`. The last move connects two lanes that are not adjacent while the child's bar is on the lane in between. The result follows the run to `local`. |
| `/?fixture=fork` | The worker process of a durable run is ended during day 3. Only the `worker_exit` event has the time of the loss. The run has automatic snapshots (`snapshot` events), so it becomes `paused`. The user forks it at snapshot #1 (`forked`), resumes the source, which completes through the remote call, resumes the fork, and cancels the fork (`cancelled`). |
| `/?fixture=spawn` | `spawn { remote_fetch_weather() }`: the spawned thread crosses to `cloud` while the main thread continues. A pause request is blocked twice. The run is then resumed on `cloud` (migration). |
| `/?fixture=fanout` | `durable_fan_out` spawns four threads that call `remote_get_quote`. The pool alternates between `cloud` and `cloud2`. The run reaches `sleep(12 s)` and suspends itself (`paused` with `wake`, status `sleeping`, `sleep_scheduled`). The children finish while no parent process exists. The timer wakes the run (`woken`), the new process gets the four results at startup, and `baml.future.all` returns. The resumed worker announces only its root thread. |
| `/?fixture=race` | `baml.future.race` over three remote calls of 2, 6, and 9 seconds. The user pauses the run during the race (five threads in the snapshot) and resumes it on `cloud2`. The stored result of the fastest child travels with the run. `race` settles and cancels the two losers (`remote_cancel`, `remote_cancelled`), and their processes end as `cancelled`. The resumed worker announces every restored thread. |
| `/?fixture=deadline` | `baml.future.with_timeout` of 2 seconds around a call of 6 seconds. The user pauses the run after 0.9 seconds: three threads, two pending futures, and a cancel token are in the snapshot. After the resume the deadline passes, the work thread is cancelled in its remote wait, and the child on `cloud` is cancelled. |
| `/?fixture=settled` | `baml.future.all_settled` over three remote calls. One child fails with a typed error. The user pauses the run while the third child is out, so the snapshot holds a resolved future with a nested class instance, a failed future with its error, and a pending future. The third result arrives while no process exists. |
| `/?fixture=recover` | The durable run is killed during day 3 and becomes `paused`, because it has automatic snapshots. It is resumed and completes. `plan_trip`, the same body without the marker, is killed too and becomes `lost`. |
| `/?fixture=branch` | The run is paused during day 2 and forked. The source and the fork are resumed, each makes its own remote call (one child on `cloud`, one on `cloud2`), and both complete with the same result. |

The fixtures `pool`, `fanout`, `race`, `deadline`, `settled`, `recover`, and
`branch` are the recordings of the seven scenarios. Each names the runs that
its guide follows (`roles`), so the guide plays along with the recording.
`src/fixtures/quotes.baml.ts` is a copy of `program/baml_src/quotes.baml`, and
the fixtures look their line numbers up in it.
`src/fixtures/recorded/fanout-mock.json` is not a fixture of the app. It holds
the run records and the event files that the three site servers wrote for one
`durable_fan_out` run with the mock worker. `src/timeline/recorded.test.ts`
loads it the way a page reload does and checks the timeline against it.

`&speed=8` sets the playback rate (default 4). `&speed=instant` dispatches the
whole fixture at once. A fixture is a list of SSE messages in the form that
the site servers produce. It is played into the same reducer that the live
streams feed, against a virtual clock, so the recorded timestamps keep their
meaning. Commands return an error in fixture mode.

## Checks

```bash
pnpm build            # tsc --noEmit (strict) and vite build
pnpm test             # vitest: reducer, HTTP API client, timeline layout and geometry, state values, scenario guide
pnpm check:headless   # needs `pnpm dev` in another terminal
LIVE=1 pnpm check:headless   # also needs the site servers (../scripts/dev.sh)
```

`check:headless` loads the eleven fixtures in headless Chromium in light and dark
mode, fails on any console error or page error, and asserts the number of
segments, gaps, thread bars, connectors, and markers that the timeline drew.
For every fixture it checks that the header, the legend, the timeline, and the
log area show the three sites in registry order, that the three site colors
differ, and that the runs list, the timeline, and the log lanes use the same
color for a site. For `pool` and `chain` it also checks the sites that each
connector joins. At 1440x900 it checks that the three log lanes stay readable.
It checks the "Resume on <site>" buttons for a run on `local` and for a run on
`cloud`, and it checks that a log layout saved by the two-site app is not
applied to the three lanes and that a resized three-lane layout survives a
reload. `WEB_PORT` selects the dev server when `BASE_URL` is not set.
It uses the Chromium build from the Playwright browser cache
(`pnpm dlx playwright install chromium-headless-shell` when it is missing).
It also exercises the zoom buttons, ctrl+wheel zoom, drag pan, and Fit, the
fork marker and the link from a fork to its source, and the `blocked` reason
that appears under the status while a pause request stays `pausing`.
The scenes of contract section 9.6 check the fan-out while it sleeps (the label
`sleeping until <time>`, a countdown that runs, a band that reaches past the
now line to the wake time, the `sleeping` status with its wake time, the
commands of a sleeping run, and five threads with four pending futures in the
state tree), the wake (closed gap, wake marker with its detail panel), thread
bars that keep their line across the sleep, `program_source` in the resume
timings, the program hash, the grouped runs list and its fold, the style of
cancelled bars, cancel connectors and cancel markers with their detail panels,
bar labels that do not overlap, futures in three states with their values,
nested instances, enums, maps, a cancel token, the scenario gallery (seven
cards, the autoplay switch and its persistence, the links to the recordings),
and the guide (the highlighted button with and without autoplay, the selection
of the second run of the recovery scenario, Exit). `SCENES=phase3` or
`SCENES=base` runs one group of the fixture scenes.
`OUT_DIR=<dir>` writes screenshots, among them `fanout-asleep-<scheme>.png`,
`race-guide-pause.png`, `race-guide-resume-autoplay.png`,
`settled-state-tree-<scheme>.png`, `deadline-state-tree-<scheme>.png`,
`gallery-<scheme>.png`, and `recover-guide-kill-plain.png`.
`node scripts/screenshot.ts "<path>" out.png [--dark] [--size WxH] [--wait ms]`
takes one screenshot of any URL of the app. The script is TypeScript
(`scripts/headless-check.ts`). Node runs it directly, which needs Node 22.18 or
later, and `pnpm build` type checks it.

`LIVE=1` adds four scenes against the real site servers, with the mock worker
or with the real worker (`baml-cli worker`; the script reads `worker_cmd` from
`/local/api/info`). Three of them move a run between sites and drive the app
through its buttons:

| Scene | What it does |
|---|---|
| `pool` | The central scene of contract section 8. It starts `durable_plan_trip` on `local`, pauses it inside the loop, and clicks "Resume on cloud". It checks the connectors `migration local > cloud`, `call cloud > cloud2`, and `return cloud2 > cloud`, a segment bar on each lane, the `migrated_to` field of the record on `local`, the child run on `cloud2` with `parent.site` equal to `cloud`, and the `TripPlan` of the run that completed on `cloud`. It then reloads the page and checks the connectors again. |
| `chain` | One run moves `local`, `cloud`, `cloud2`. It is paused in the loop on `local` and again on `cloud`. On `cloud2` it calls `remote_fetch_weather`, which the pool places on `cloud`. The scene checks two migration connectors, the records along the chain, and the `TripPlan` of segment 3. |
| `skip` | The run is paused on `local` and resumed on `cloud2`, so the migration connector crosses the `cloud` lane. The remote child runs on `cloud`. This scene uses dark mode. |
| `central` | The scene described below. |

`LIVE_ONLY=guide` runs the scenario guide against the site servers. It opens
the gallery, starts a scenario with "Run live", turns autoplay on, records the
buttons that autoplay highlights, and waits until the guide is finished.
`LIVE_GUIDE=migrate,fanout,race,deadline,settled,recover,fork` names the
scenarios. The default is `migrate`, which needs nothing of contract section 9
and checks the result, the records, and the connectors. The other scenarios
need site servers with a worker of contract section 9, mock or real. Each of
them checks the buttons that autoplay pressed, the run records on the site
servers, the state tree that was shown while the run had no process, and the
drawn timeline:

| Scenario | Checks |
|---|---|
| `fanout` | No button is pressed. The run completes in segment 2 with four quotes in input order and a total of 1194. Two children ran on each cloud site. While the run slept, the state tree showed five threads and their futures. The timeline has one sleeping gap, one wake marker, four calls, four returns, and four thread links across the gap. |
| `race` | Pause, then Resume on cloud2. The race settles on `cloud2` with the 2 second vendor, `local` keeps a `migrated` record, and the two losers are `cancelled`. The paused state tree showed five threads and at least three pending futures. The timeline has one migration, two cancel connectors, and two cancelled bars. |
| `deadline` | Pause, then Resume here. The result is the `Timeout` message, and the child is `cancelled`. The paused state tree showed the cancel token and three threads. The timeline has one cancel connector, one cancelled bar, and two thread links. |
| `settled` | Pause, then Resume here. The result has two quotes and the `Car` failure with the text of the remote error. No child is cancelled. The paused state tree showed a pending future next to a settled one. |
| `recover` | Kill, Resume here, start of the plain run, Kill. The durable run completes in segment 2 with the `TripPlan`, and the plain run is `lost`. |
| `fork` | Pause, Fork, Resume here for both runs. The source and the fork complete with the `TripPlan`, and the timeline has the fork marker, the fork connector, and two remote calls. |

A screenshot of the suspended state is written as
`live-guide-<scenario>-suspended.png` next to `live-guide-<scenario>.png`.
When the site servers run with `CHAOS` (the script reads `chaos` from
`/local/api/info`), the checks that depend on the timing of a cancel or of the
first result accept the slower outcome: the 6 second loser of the race may
complete, and every future of `settled` may still be pending at the pause. All
seven scenarios passed with the real worker, with and without `CHAOS`.

`LIVE_ONLY=pool,chain` runs only the named live scenes and skips the fixture
scenes. The three scenes above use a 1440x900 window and write
`live-pool-paused.png`, `live-pool-child-running.png`, `live-pool.png`,
`live-chain-child-running.png`, `live-chain.png`, and `live-skip.png` into
`OUT_DIR`.

The `central` scene runs the parent on `local` and the child on `cloud`: start
`durable_plan_trip`, pause it while the remote child runs, reload
the page in the paused state, and resume. It then kills a durable run after an
automatic snapshot and checks that the end of its bar survives a reload, forks
that run, resumes and cancels the fork, pauses a run whose worker answers with
`blocked` events, and kills a non-durable run. The `blocked` scene uses the
`mock_blocked` argument with the mock worker. With the real worker it pauses
`durable_plan_trip_parallel` while the spawned remote call is outstanding and
looks at the answer. A worker of contract section 9 pauses: the scene checks
that the state tree shows both threads (`sleep` and `remote_call`) and the
pending `weather_future`, resumes the run, and checks that the spawned thread
continues across the gap on one sub-row with one call and one return. A worker
from before section 9 answers with `blocked` until the run completes, and the
scene checks the reason and the marker as before. After "Resume on cloud" the
app selects the record on the destination site, and the `pool` scene checks
that. In the paused state the `central` scene also checks that the state tree names the
locals `city`, `ideas`, and `day`. It starts runs on the servers
behind the proxy. `BASE_URL` selects another dev
server.

## Source layout

| Path | Content |
|---|---|
| `src/protocol.ts` | Every protocol type of contract sections 2.3, 2.5, 3.2, 3.3, and 3.4, and the tolerant parser for SSE messages. |
| `src/state.ts` | The single reducer and its selectors (run tree, latest position, valid commands). |
| `src/session.ts` | The two event sources: live `EventSource` connections (one per site of the registry), and fixture playback. It also loads the site registry. |
| `src/api.ts` | HTTP commands and queries. |
| `src/timeline/layout.ts` | Run records and events to timeline geometry in time units: segments, gaps, thread bars, waits, connectors, markers. No React. |
| `src/timeline/geometry.ts` | Time scale, lane and row positions, label and marker placement, and the route of a connector between lanes. No React. |
| `src/timeline/Timeline.tsx` | The SVG timeline and its detail pane. |
| `src/stateValue.ts` | Reads the shape of a state dump value: a future with its state, a cancel token, a task group, an enum variant, a map, an array, an instance. One-line summaries. No React. |
| `src/scenarios/` | The scenario guide: `engine.ts` (what a scenario is, the view of its runs, the progress; no React), `catalog.ts` (the seven scenarios and their hints), `useCoach.ts` (selection of the target run, the highlight, autoplay). |
| `src/components/` | Header, scenario gallery and guide bar, runs list and controls, source view, state tree, log lanes, and the resizable split. |
| `src/fixtures/` | The fixture builder, the eleven fixtures, the building blocks that the trip fixtures share (`scenes.ts`) and that the quote fixtures share (`quotes.ts`), copies of the two program files for the source view (`programs.ts` finds a line by its text), and one recording of real site servers (`recorded/`). |

## Behavior notes

Contract section 9:

- A run that suspends itself for a sleep has the status `sleeping`. The timeline
  draws the interval as a band in the site color, not as the dashed line of a
  pause. While the run sleeps, the band reaches from the end of the process to
  `wake_at`, which is in the future, and the now line moves through it. The
  label is `sleeping until <time> · <countdown>`. The countdown is computed
  from the app's clock, so it also runs in a recording. The gap closes at the
  first event of the next segment, and the label becomes `slept <duration>`.
  The wake time comes from `sleep_scheduled`, from `wake_at` of the run record
  when the event list is not loaded, or from `paused.ts + wake.remaining_ms`.
  The `woken` event is a marker with a detail panel (reason, timer error, time
  without a process). A self-suspension draws no pause request marker.
- A sleeping run can be resumed and cancelled, and it cannot be paused or
  killed. A paused run can be cancelled too (section 9.3). The controls follow
  this. The runs list and the controls show `wakes <time> · in <countdown>`.
- A `remote_cancel` event ends the wait on its call, which is marked as
  cancelled, and places a cancel marker on the parent's row. The cancel
  connector leaves the parent at that time, from the sub-bar of the cancelled
  thread, and arrives where the child's process ended. While the child still
  runs, the connector is drawn as pending, up to now. Without a worker event,
  the site server's `remote_cancelled` gives the time, which is the case for a
  run that ended and whose site cancelled its children. A child that had
  settled before the cancel gets no connector. A cancelled call has no return
  connector. A bar that ended as `cancelled` has diagonal stripes.
- Spawned threads that are parked when a run is suspended continue in the next
  segment under the same thread id. The layout visits the segments of a run in
  order, over every site that held the run. A thread that was open at the end
  of a suspended segment gets a bar in the next segment: from its
  `thread_started` event there, or, when the worker announces only new
  threads, from the `resumed` event. Both bars share one sub-row, and a dotted
  link joins them through the gap. No link is drawn between lanes. The root
  thread of a resumed segment is the root thread of the segment before it, also
  when the worker announces every restored thread without a parent. A killed
  segment hands no threads on, because the run continues from an older
  snapshot. The real worker reports every live thread with `thread_ended` at
  the time of its `paused` event, because the process ends, and it announces
  the restored threads again after the resume. A thread that ended within 5 ms
  of the `paused` event therefore continues when the next segment announces
  its id again. A thread that ended earlier does not.
- The state tree lists the root thread first and the spawned threads after it
  by id. The real worker writes the threads in the order in which they parked.
  It records a thread that was inside `await` as `runnable`, because the
  resumed process executes the await again. The mock worker and the fixtures
  write `await`. The tooltip of `parked: runnable` says so.
- Runs that overlap in time in one lane get rows of their own, so a fan-out of
  four shows two rows on `cloud` and two on `cloud2`. A run keeps its row when
  later runs arrive. Every bar label sits in the band above its own bar.
- The runs list puts remote children under their parent, in call order, one
  level deeper. A parent with children has a fold button with their number. A
  child whose parent is not listed is a top-level row. The children of a parent
  that migrated follow the record on the site that made the call.
- The program hash of a run is shown in short form in the runs list, next to
  the status in the controls, and in the detail panel of a segment. The resume
  timings name `program_source`: `store (no compile)` or `compile`.
- The state tree reads the forms of the snapshot core: a future is the kind
  `future` with the preview `#<id> (<state>)` and one child `value` or `error`,
  a cancel token is the kind `cancel_token`, a task group is the kind
  `task_group`, and an enum variant is an opaque value named `Enum.Variant`.
  A future shows its id, its state as a pill, and a one-line summary of its
  value or error. A thread names the future that it settles (`settles #<id>`).
  The tree also accepts a future that is an instance of `Future` or an opaque
  value, because a reader must tolerate a kind that it does not know. A class
  instance, an array, and a map show a one-line summary. A nested value that
  does not fit is named (`QuoteRequest {…}`) and not cut in the middle, except
  that an error keeps its message. Map entries are listed under quoted keys.
  The state tree shows the latest snapshot of a `sleeping` run without a click,
  as it does for a `paused` run. With more than three threads, only the first
  thread is open.

Earlier sections:

- `init` replaces the runs of its site. `run` upserts a record. Every other
  known event is appended to the event list of its run. An event of an unknown
  type is dropped, and unknown fields are kept and ignored.
- A segment bar ends at the `worker_exit` event of its process (contract
  section 7.2). The event is part of `events.jsonl`, so the end of a killed
  segment is the same after a page reload. The end kind comes from the terminal
  worker event (`paused`, `completed`, `failed`, `cancelled`) or, without one,
  from `worker_exit.status`.
- The reducer records each change of a run's status or segment as a `ui_status`
  entry in the run's event list. The worker emits nothing when a pause is
  requested, so the timeline takes that time from these entries. After a page
  reload it falls back to `pausing`, to the first `blocked`, or to
  `pause_latency_ms`. For a site server that sends no `worker_exit`, the end of
  a segment falls back to these entries and to `updated_ts`.
- A wait on a remote call that continues in a resumed segment starts at the
  `resumed` event of that segment. The real worker loads the program for about
  a second between `hello` and `resumed`.
- An automatic snapshot is a hollow marker. It comes from the `snapshot` event,
  or from `snapshots[].automatic` and `.segment` of the run record when the
  event list is not loaded.
- While the status is `pausing`, the latest `blocked` reason is shown under the
  status in the controls and in the run's row. It comes from the `blocked`
  field of the run record, or from the `blocked` events of the segment.
- Selecting a run loads `GET /api/runs/:id/events` for every run of its tree
  and merges the result with the live events. The merge drops duplicates. The
  history is loaded again after a reconnect.
- A run tree is the selected run, the same run id on every other site (a
  migration or a chain of migrations), its forks, the run it was forked from, and, transitively, the
  remote children of all of them. The tree of a child does not include its
  parent. The runs list links a child to its parent and a fork to its source.
  The timeline draws a fork as a marker at `forked.ts`, a gap without a process
  until the fork's first segment, and a dotted connector from the copied
  snapshot on the source's row.
- `EventSource` reconnects by itself after a dropped connection. It gives up
  after an HTTP error status, which is what the dev proxy answers while a site
  server is down. The app then opens a new `EventSource` after two seconds. The
  delay doubles with every failed attempt in a row, up to eight seconds, and
  starts at two seconds again after a successful connection.
- The timeline has one lane per site in registry order. A connector between
  adjacent lanes is one S-curve. A connector between lanes that are not
  adjacent, for example a migration from `cloud2` to `local`, crosses the lanes
  in between as a straight vertical line and travels horizontally only in the
  free band between two lanes. The vertical is at the target's time, so the
  arrow arrives straight. When a bar of a lane in between is under that
  position and none is under the source's position, the vertical moves to the
  source's time. A run that moves over several sites gets one migration
  connector and one gap per move. The gap ends at the `migrated_out` event of
  the site that the run left.
- The controls show "Resume here" and one "Resume on <site>" button for every
  other site of the registry. The command sends `{ "site": "<site>" }` to
  `POST /api/runs/:id/resume` on the site that holds the run.
- The log area has one lane per site inside a resizable group. The pane ids of
  that group name the sites (`logpane-<site>`), and the saved layout is stored
  per set of pane ids. A layout that was saved for another set of lanes is
  therefore never applied. In a lane narrower than 430 px, which is the normal
  case with three sites, the time, the run id, and the stream form the first
  row of a line, and the text starts on the next row. The pid is hidden there.
  A long line that the app wrote itself is cut after three rows, and its full
  text is in the tooltip. Program output is never cut.
- Color encodes the site (blue for `local`, orange for `cloud`, violet for
  `cloud2`, gray for any other site) and the status of a run. The CSS variables
  are `--site-local`, `--site-cloud`, `--site-cloud2`, and `--site-other`, each
  with a light and a dark value. An element with a `data-site` attribute sets
  `--site` for its subtree, and every rule that colors by site reads `--site`.
  A status always has a glyph and a label next to its color.
