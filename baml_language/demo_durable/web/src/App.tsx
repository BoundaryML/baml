import { useCallback, useEffect, useMemo, useReducer, useRef, useState } from "react";
import { httpApi, type Api } from "./api";
import { Header } from "./components/Header";
import { LogLanes } from "./components/LogLanes";
import { RunsPanel } from "./components/RunsPanel";
import { SourceView, type SourceFocus } from "./components/SourceView";
import { Split } from "./components/Split";
import { StateTree } from "./components/StateTree";
import { isFixtureName, loadFixture, type FixtureName } from "./fixtures";
import type { JsonObject, Run, Site } from "./protocol";
import { startFixtureSession, startLiveSession } from "./session";
import { connectionOf, initialState, reducer, runKey, runTree, splitRunKey, type RunKey } from "./state";
import { Timeline, type SnapshotRef } from "./timeline/Timeline";

interface Mode {
  fixture: FixtureName | null;
  speed: number;
}

/** `?fixture=<name>` plays a fixture (see `fixtures/index.ts`). `&speed=8` or `&speed=instant` sets the playback rate. */
function readMode(): Mode {
  const params = new URLSearchParams(window.location.search);
  const fixture = params.get("fixture");
  const speedParam = params.get("speed");
  const speed = speedParam === "instant" ? Number.POSITIVE_INFINITY : Number(speedParam ?? 4);
  return {
    fixture: isFixtureName(fixture) ? fixture : null,
    speed: Number.isFinite(speed) && speed > 0 ? speed : speedParam === "instant" ? Number.POSITIVE_INFINITY : 4,
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
  useEffect(() => {
    setSelectedSnapshot(null);
    setFocus(null);
  }, [selected]);
  // A frame focus from the state tree ends when the run moves on.
  const selectedStatus = selectedRun?.status;
  const selectedSegment = selectedRun?.segment;
  useEffect(() => setFocus(null), [selectedStatus, selectedSegment]);

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

  const onRunResponse = useCallback((run: Run, requestedOn: Site, selectIt: boolean): void => {
    dispatch({ type: "run_response", site: requestedOn, run, ts: Date.now() });
    if (selectIt) dispatch({ type: "select", key: runKey(run.site ?? requestedOn, run.id) });
    // "Resume on <site>" leaves a `migrated` record behind. Follow the run to
    // the site that now hosts it, so that Pause and Kill act on the live run.
    // The record on that site arrives on its event stream a moment later.
    else if (run.status === "migrated" && run.migrated_to) dispatch({ type: "select", key: runKey(run.migrated_to, run.id) });
  }, []);

  return (
    <div className="app" data-testid="app" data-mode={api.mode}>
      <Header
        sites={state.sites}
        connections={state.connections}
        fixture={fixture ? { name: fixture.name, title: fixture.title, speed: mode.speed, onReplay: () => setReplay((n) => n + 1) } : null}
        onStart={onStart}
      />
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
                      onSelect={select} onRunResponse={onRunResponse} />
                  ),
                },
                {
                  id: "timeline", defaultSize: "68", minSize: "30",
                  content: (
                    <Timeline key={selected ?? "none"} sites={state.sites} runs={treeRuns} events={state.events} selectedRun={selected}
                      selectedSnapshot={selectedSnapshot} onSelectSnapshot={setSelectedSnapshot} />
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
                  content: <SourceView api={api} run={selectedRun} events={selected ? state.events[selected] : undefined} focus={focus} />,
                },
                {
                  id: "state", defaultSize: "21", minSize: "12",
                  content: (
                    <StateTree api={api} run={selectedRun} selectedSnapshot={selectedSnapshot} onSelectSnapshot={setSelectedSnapshot}
                      onFocusSource={setFocus} />
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
