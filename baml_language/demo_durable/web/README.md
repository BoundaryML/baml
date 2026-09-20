# Durable functions demo: web app

The React app of the durable functions proof of concept (contract sections
3.5, 7.7, and 8.4 in `documents/durable-poc-contracts.md`). TypeScript, Vite, React, `strict`
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

## Fixture mode

The app runs without site servers when the URL names a fixture:

| URL | Scene |
|---|---|
| `/?fixture=central` | The parent on `local` calls `remote_fetch_weather` on `cloud`. The parent is paused while the child runs. The child completes. The parent resumes in a new process and completes. |
| `/?fixture=pool` | The central scene with three sites. The parent starts on `local` and is paused in its loop. The user resumes it on `cloud`. It calls `remote_fetch_weather` there, and the remote pool places the child on `cloud2`. The result returns to `cloud`, and the run completes. |
| `/?fixture=chain` | One run moves over all sites: `local`, `cloud`, `cloud2`, and back to `local`. On `cloud2` it calls `remote_fetch_weather`, which runs on `cloud`. The last move connects two lanes that are not adjacent while the child's bar is on the lane in between. The result follows the run to `local`. |
| `/?fixture=fork` | The worker process of a durable run is ended during day 3. Only the `worker_exit` event has the time of the loss. The run has automatic snapshots (`snapshot` events), so it becomes `paused`. The user forks it at snapshot #1 (`forked`), resumes the source, which completes through the remote call, resumes the fork, and cancels the fork (`cancelled`). |
| `/?fixture=spawn` | `spawn { remote_fetch_weather() }`: the spawned thread crosses to `cloud` while the main thread continues. A pause request is blocked twice. The run is then resumed on `cloud` (migration). |

`&speed=8` sets the playback rate (default 4). `&speed=instant` dispatches the
whole fixture at once. A fixture is a list of SSE messages in the form that
the site servers produce. It is played into the same reducer that the live
streams feed, against a virtual clock, so the recorded timestamps keep their
meaning. Commands return an error in fixture mode.

## Checks

```bash
pnpm build            # tsc --noEmit (strict) and vite build
pnpm test             # vitest: reducer, HTTP API client, timeline layout, timeline geometry
pnpm check:headless   # needs `pnpm dev` in another terminal
LIVE=1 pnpm check:headless   # also needs the site servers (../scripts/dev.sh)
```

`check:headless` loads the five fixtures in headless Chromium in light and dark
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
`OUT_DIR=<dir>` writes screenshots. The script is TypeScript
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
`durable_plan_trip_parallel`, whose future blocks the snapshot until the run
completes. In the paused state it also checks that the state tree names the
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
| `src/components/` | Header, runs list and controls, source view, state tree, log lanes, and the resizable split. |
| `src/fixtures/` | The fixture builder, the five fixtures, the building blocks that `pool` and `chain` share (`scenes.ts`), and a copy of the demo program for the source view. |

## Behavior notes

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
