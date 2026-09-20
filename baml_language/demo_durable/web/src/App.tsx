import { useCallback, useEffect, useMemo, useReducer, useRef, useState } from "react";
import { httpApi, type Api } from "./api";
import { CoachBar } from "./components/CoachBar";
import { Header } from "./components/Header";
import { LogLanes } from "./components/LogLanes";
import { RunsPanel } from "./components/RunsPanel";
import { ScenarioGallery } from "./components/ScenarioGallery";
import { SourceView, type SourceFocus } from "./components/SourceView";
import { Split } from "./components/Split";
import { StateTree } from "./components/StateTree";
import { isFixtureName, loadFixture, type FixtureName } from "./fixtures";
import { REGISTRY_SITE, type JsonObject, type Run, type Site, type StateDump } from "./protocol";
import { scenarioOfFixture } from "./scenarios/catalog";
import type { RunStart, Scenario } from "./scenarios/engine";
import { useCoach, type ScenarioSession } from "./scenarios/useCoach";
import { startFixtureSession, startLiveSession } from "./session";
import { connectionOf, initialState, reducer, runKey, runTree, splitRunKey, type RunKey } from "./state";
import { Timeline, type SnapshotRef } from "./timeline/Timeline";

interface Mode {
  fixture: FixtureName | null;
  speed: number;
  /** `?scenarios=1` opens the scenario gallery at startup. */
  gallery: boolean;
  /** `?autoplay=1` or `?autoplay=0`. `null` leaves the stored preference. */
  autoplay: boolean | null;
}

/** The snapshot whose state dump the state tree holds. */
type SnapshotState = { site: Site; run: string; n: number; dump: StateDump };

const AUTOPLAY_KEY = "durable-demo:autoplay";

/** The stored autoplay preference. Storage can be unavailable, and the switch then starts off. */
function storedAutoplay(): boolean {
  try {
    return window.localStorage.getItem(AUTOPLAY_KEY) === "1";
  } catch {
    return false;
  }
}

/** `?fixture=<name>` plays a fixture (see `fixtures/index.ts`). `&speed=8` or `&speed=instant` sets the playback rate. */
function readMode(): Mode {
  const params = new URLSearchParams(window.location.search);
  const fixture = params.get("fixture");
  const speedParam = params.get("speed");
  const speed = speedParam === "instant" ? Number.POSITIVE_INFINITY : Number(speedParam ?? 4);
  const autoplay = params.get("autoplay");
  return {
    fixture: isFixtureName(fixture) ? fixture : null,
    speed: Number.isFinite(speed) && speed > 0 ? speed : speedParam === "instant" ? Number.POSITIVE_INFINITY : 4,
    gallery: params.get("scenarios") === "1",
    autoplay: autoplay === null ? null : autoplay === "1",
  };
}

export function App() {
  const mode = useMemo(readMode, []);
  const [state, dispatch] = useReducer(reducer, undefined, initialState);
  const [api, setApi] = useState<Api>(httpApi);
  const [replay, setReplay] = useState(0);
  const fixture = useMemo(() => (mode.fixture ? loadFixture(mode.fixture) : null), [mode.fixture]);

  useEffect(() => {
    dispatch({ type: "reset" });
    const session = fixture ? startFixtureSession(fixture, dispatch, mode.speed) : startLiveSession(dispatch);
    setApi(session.api);
    return () => session.stop();
  }, [fixture, mode.speed, replay]);

  const selected = state.selected;
  const tree = useMemo(
    () => runTree({ runs: state.runs, events: state.events }, selected),
    [state.runs, state.events, selected],
  );
  const treeId = tree.join(",");
  // `runs` and `tree` keep their identity while the membership of the tree and its records do not change.
  const stableTree = useMemo(() => tree, [treeId]); // eslint-disable-line react-hooks/exhaustive-deps
  const treeRuns = useMemo(
    () => stableTree.flatMap((key) => (state.runs[key] ? [state.runs[key] as Run] : [])),
    [stableTree, state.runs],
  );
  const allRuns = useMemo(() => Object.values(state.runs), [state.runs]);
  const selectedRun = selected ? (state.runs[selected] ?? null) : null;

  const [selectedSnapshot, setSelectedSnapshot] = useState<SnapshotRef | null>(null);
  const [focus, setFocus] = useState<SourceFocus | null>(null);
  // The dump that the state tree holds. The timeline names the top user frame
  // of a snapshot marker from it (contract section 10.3).
  const [snapshotState, setSnapshotState] = useState<SnapshotState | null>(null);
  useEffect(() => {
    setSelectedSnapshot(null);
    setFocus(null);
  }, [selected]);
  // A frame focus from the state tree names a line of a snapshot that the run
  // has left behind, so it ends when the run moves on. A location that the
  // user followed from the timeline stays until it is cleared.
  const selectedStatus = selectedRun?.status;
  const selectedSegment = selectedRun?.segment;
  useEffect(() => setFocus((current) => (current?.origin === "cause" ? current : null)), [selectedStatus, selectedSegment]);
  const clearFocus = useCallback(() => setFocus(null), []);

  // Backfill the history of every run of the selected tree, once per connection generation.
  const backfilled = useRef(new Set<string>());
  useEffect(() => {
    backfilled.current.clear();
  }, [api]);
  // One entry per site, so that a reconnect of any site runs the effect again.
  const generations = Object.entries(state.connections)
    .map(([site, connection]) => `${site}:${connection.generation}`)
    .join(",");
  useEffect(() => {
    for (const key of stableTree) {
      const { site, id } = splitRunKey(key);
      if (state.runs[key] === undefined) continue;
      const generation = connectionOf(state, site).generation;
      const token = `${key}@${generation}`;
      if (backfilled.current.has(token)) continue;
      backfilled.current.add(token);
      api
        .runEvents(site, id)
        .then((events) => dispatch({ type: "backfill", site, run: id, events }))
        .catch(() => backfilled.current.delete(token));
    }
    // `state.runs` is read for membership only. The tree id covers its changes.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [api, stableTree, generations]);

  const select = useCallback((key: RunKey | null) => dispatch({ type: "select", key }), []);

  const onStart = useCallback(
    async (site: Site, fn: string, args: JsonObject): Promise<void> => {
      const run = await api.startRun(site, { function: fn, args });
      dispatch({ type: "run_response", site, run, ts: Date.now() });
      dispatch({ type: "select", key: runKey(run.site ?? site, run.id) });
    },
    [api],
  );

  // The scenario gallery and its guide (contract section 9.6).
  const [galleryOpen, setGalleryOpen] = useState(mode.gallery);
  const [autoplay, setAutoplayState] = useState<boolean>(() => mode.autoplay ?? storedAutoplay());
  const setAutoplay = useCallback((on: boolean): void => {
    setAutoplayState(on);
    try {
      window.localStorage.setItem(AUTOPLAY_KEY, on ? "1" : "0");
    } catch {
      // The preference is a convenience. Without storage it lasts for the page.
    }
  }, []);
  const [liveSession, setLiveSession] = useState<ScenarioSession | null>(null);
  const [guideClosed, setGuideClosed] = useState(false);
  // A recording that plays a scenario brings its guide along.
  const fixtureSession = useMemo<ScenarioSession | null>(() => {
    const scenario = fixture ? scenarioOfFixture(fixture.name) : null;
    return scenario && fixture?.roles?.root ? { scenario, roles: fixture.roles } : null;
  }, [fixture]);
  const session = guideClosed ? null : (fixture ? fixtureSession : liveSession);

  const runScenario = useCallback(
    async (scenario: Scenario): Promise<void> => {
      const { site, fn, args } = scenario.start;
      const run = await api.startRun(site, { function: fn, args });
      const runSite = run.site ?? site;
      dispatch({ type: "run_response", site, run, ts: Date.now() });
      dispatch({ type: "select", key: runKey(runSite, run.id) });
      setLiveSession({ scenario, roles: { root: { site: runSite, id: run.id } } });
      setGuideClosed(false);
      setGalleryOpen(false);
    },
    [api],
  );
  const [coachError, setCoachError] = useState<string | null>(null);
  const startRole = useCallback(
    (role: "plain", start: RunStart): void => {
      setCoachError(null);
      api
        .startRun(start.site, { function: start.fn, args: start.args })
        .then((run) => {
          const runSite = run.site ?? start.site;
          dispatch({ type: "run_response", site: start.site, run, ts: Date.now() });
          dispatch({ type: "select", key: runKey(runSite, run.id) });
          setLiveSession((current) => (current ? { ...current, roles: { ...current.roles, [role]: { site: runSite, id: run.id } } } : current));
        })
        .catch((cause: unknown) => setCoachError(cause instanceof Error ? cause.message : String(cause)));
    },
    [api],
  );
  const coach = useCoach({ session, state, autoplay, live: api.mode === "live", select, startRole });

  const guided = session !== null;
  // Clearing is per site, so every site is asked. The local state is emptied
  // once they have all answered: the SSE streams send nothing when a run
  // disappears, so there is no event to wait for.
  const onClearAll = useCallback(async (): Promise<void> => {
    const sites = state.sites.map((entry) => entry.name);
    const results = await Promise.allSettled(sites.map((site) => api.clearRuns(site)));
    dispatch({ type: "cleared" });
    const failed = results.flatMap((result, index) =>
      result.status === "rejected" ? [sites[index] ?? "?"] : [],
    );
    if (failed.length > 0) throw new Error(`could not clear ${failed.join(", ")}`);
  }, [api, state.sites]);

  const onRunResponse = useCallback((run: Run, requestedOn: Site, selectIt: boolean): void => {
    dispatch({ type: "run_response", site: requestedOn, run, ts: Date.now() });
    // A new fork is selected, except while a scenario guides the user: its next step names the run to act on.
    if (selectIt && !guided) dispatch({ type: "select", key: runKey(run.site ?? requestedOn, run.id) });
    else if (selectIt) return;
    // "Resume on <site>" leaves a `migrated` record behind. Follow the run to
    // the site that now hosts it, so that Pause and Kill act on the live run.
    // The record on that site arrives on its event stream a moment later.
    else if (run.status === "migrated" && run.migrated_to) dispatch({ type: "select", key: runKey(run.migrated_to, run.id) });
  }, [guided]);

  return (
    <div className="app" data-testid="app" data-mode={api.mode}>
      <div className="top">
        <Header
          sites={state.sites}
          connections={state.connections}
          fixture={fixture ? { name: fixture.name, title: fixture.title, speed: mode.speed, onReplay: () => setReplay((n) => n + 1) } : null}
          onStart={onStart}
          onOpenScenarios={() => setGalleryOpen(true)}
        />
        {session && coach && (
          <CoachBar session={session} coach={coach} autoplay={autoplay} live={api.mode === "live"} onAutoplay={setAutoplay}
            onStartRole={() => { if (coach.start) startRole(coach.start.action.as, coach.start.action.start); }}
            onRestart={() => (fixture ? setReplay((n) => n + 1) : void runScenario(session.scenario).catch((cause: unknown) => setCoachError(cause instanceof Error ? cause.message : String(cause))))}
            onExit={() => setGuideClosed(true)} onOpenGallery={() => setGalleryOpen(true)} />
        )}
        {coachError && <div className="error-line" role="alert" data-testid="coach-error">{coachError}</div>}
      </div>
      {galleryOpen && (
        <ScenarioGallery live={api.mode === "live"} connected={connectionOf(state, REGISTRY_SITE).status === "open"} active={session?.scenario.id ?? null}
          autoplay={autoplay} onAutoplay={setAutoplay} onRun={runScenario} onClose={() => setGalleryOpen(false)} />
      )}
      <div className="rows">
        <Split id="rows" orientation="vertical" panes={[
          {
            id: "top", defaultSize: "42", minSize: "20",
            content: (
              <Split id="top" orientation="horizontal" panes={[
                {
                  id: "runs", defaultSize: "32", minSize: "16",
                  content: (
                    <RunsPanel sites={state.sites} runs={allRuns} events={state.events} selected={selected} tree={stableTree} selectedSnapshot={selectedSnapshot} api={api}
                      onSelect={select} onRunResponse={onRunResponse} onClearAll={onClearAll} coach={coach?.target ?? null} />
                  ),
                },
                {
                  id: "timeline", defaultSize: "68", minSize: "30",
                  content: (
                    <Timeline key={selected ?? "none"} sites={state.sites} runs={treeRuns} events={state.events} selectedRun={selected}
                      selectedSnapshot={selectedSnapshot} onSelectSnapshot={setSelectedSnapshot} snapshotState={snapshotState}
                      onOpenSource={setFocus} />
                  ),
                },
              ]} />
            ),
          },
          {
            id: "bottom", defaultSize: "58", minSize: "20",
            content: (
              <Split id="bottom" orientation="horizontal" panes={[
                {
                  id: "source", defaultSize: "24", minSize: "14",
                  content: (
                    <SourceView api={api} run={selectedRun} events={selected ? state.events[selected] : undefined} focus={focus}
                      onClearFocus={clearFocus} />
                  ),
                },
                {
                  id: "state", defaultSize: "21", minSize: "12",
                  content: (
                    <StateTree api={api} run={selectedRun} selectedSnapshot={selectedSnapshot} onSelectSnapshot={setSelectedSnapshot}
                      onFocusSource={setFocus} onStateLoaded={setSnapshotState} />
                  ),
                },
                {
                  id: "logs", defaultSize: "55", minSize: "20",
                  content: <LogLanes sites={state.sites} events={state.events} tree={stableTree} selected={selected} />,
                },
              ]} />
            ),
          },
        ]} />
      </div>
    </div>
  );
}
