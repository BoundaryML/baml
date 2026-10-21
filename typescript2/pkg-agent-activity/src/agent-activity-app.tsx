// Agent Activity: what an AI agent is doing to a project, as it does it.
//
// Served by `baml agent watch` at `/agent-activity`, which streams every file
// change in the watched directory. With `?mock=<scenario>` the page replays a
// recorded session instead, so the UI can be developed without a server.

import { useCallback, useEffect, useMemo, useState } from 'react';
import { IdleRecap } from './components/idle-recap';
import { MenuBar, VIEWS, type ViewId } from './components/menu-bar';
import { FileRail } from './components/rails';
import { ScenarioBar } from './components/scenario-bar';
import { Timeline } from './components/timeline';
import { ActivityModel } from './engine/model';
import { Player } from './engine/player';
import { type ActivityRuntime, RuntimeProvider } from './runtime';
import { defaultLiveUrl, LiveSource } from './sources/live';
import { MOCK_SCENARIOS, MockSource } from './sources/mock';
import type { ActivitySource, ConnectionStatus } from './sources/source';
import './styles.css';
import { ContractsView } from './views/contracts-view';
import { FeedView } from './views/feed-view';
import { FlowView } from './views/flow-view';
import { GraphView } from './views/graph-view';
import { StageView } from './views/stage-view';
import { TilesView } from './views/tiles-view';

function readParams() {
  const params = new URLSearchParams(window.location.search);
  const view = params.get('view');
  return {
    mock: params.get('mock'),
    view: (VIEWS.some((v) => v.id === view) ? view : 'stage') as ViewId,
  };
}

function readPref(key: string, fallback: boolean): boolean {
  try {
    const v = window.localStorage.getItem(`agent-activity.${key}`);
    return v === null ? fallback : v === 'true';
  } catch {
    return fallback;
  }
}

function writePref(key: string, value: boolean) {
  try {
    window.localStorage.setItem(`agent-activity.${key}`, String(value));
  } catch {
    // Storage can be unavailable (private windows, sandboxed webviews).
  }
}

function writeParam(key: string, value: string | null) {
  const url = new URL(window.location.href);
  if (value === null) url.searchParams.delete(key);
  else url.searchParams.set(key, value);
  window.history.replaceState(null, '', url);
}

export function AgentActivityApp() {
  const initial = useMemo(readParams, []);
  const [view, setView] = useState<ViewId>(initial.view);
  const [scenario, setScenario] = useState<string | null>(
    initial.mock === null
      ? null
      : (MOCK_SCENARIOS.find((s) => s.id === initial.mock)?.id ??
          MOCK_SCENARIOS[0].id),
  );
  const [runtime, setRuntime] = useState<Pick<
    ActivityRuntime,
    'model' | 'player' | 'source'
  > | null>(null);
  const [status, setStatus] = useState<ConnectionStatus>('connecting');
  const [focusRequest, setFocusRequest] =
    useState<ActivityRuntime['focusRequest']>(null);
  // Bumped to replay the current scenario from the start.
  const [run, setRun] = useState(0);
  // Whether the live server has answered: unknown until the first attempt settles.
  const [reachable, setReachable] = useState<boolean | null>(null);
  const [captions, setCaptions] = useState(() => readPref('captions', true));
  const [filesOpen, setFilesOpen] = useState(() =>
    readPref('files-open', true),
  );
  const toggleFiles = useCallback(() => {
    setFilesOpen((open) => {
      writePref('files-open', !open);
      return !open;
    });
  }, []);

  // One model, player, and source per scenario (or one for the live server).
  useEffect(() => {
    let cancelled = false;
    let source: ActivitySource | null = null;
    let player: Player | null = null;
    const model = new ActivityModel();
    const boot = async () => {
      if (scenario === null) {
        source = new LiveSource(defaultLiveUrl());
      } else {
        const info =
          MOCK_SCENARIOS.find((s) => s.id === scenario) ?? MOCK_SCENARIOS[0];
        source = new MockSource(await info.load());
      }
      if (cancelled) return;
      player = new Player(model);
      setRuntime({ model, player, source });
      setReachable(null);
      source.start(
        (message) => model.apply(message),
        (next) => {
          setStatus(next);
          // Only a server that never answered counts as missing; a dropped
          // connection keeps the session on screen while it reconnects.
          if (next === 'open') setReachable(true);
          else if (next === 'closed') setReachable((r) => r ?? false);
        },
      );
    };
    void boot();
    return () => {
      cancelled = true;
      source?.stop();
      player?.dispose();
      setRuntime(null);
    };
  }, [scenario, run]);

  const chooseScenario = useCallback((id: string | null) => {
    setScenario(id);
    writeParam('mock', id);
  }, []);

  const focusFile = useCallback((path: string) => {
    setFocusRequest((prev) => ({ nonce: (prev?.nonce ?? 0) + 1, path }));
  }, []);

  const changeView = useCallback((next: ViewId) => {
    setView(next);
    writeParam('view', next);
  }, []);

  // Keyboard: 1–5 switch views, space pauses, arrows step through history.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!runtime || e.metaKey || e.ctrlKey || e.altKey) return;
      const target = e.target as HTMLElement;
      if (target.closest('input, select, textarea')) return;
      const n = Number(e.key);
      if (n >= 1 && n <= VIEWS.length) {
        changeView(VIEWS[n - 1].id);
      } else if (e.key === ' ') {
        e.preventDefault();
        runtime.player.setPaused(!runtime.player.getSnapshot().paused);
      } else if (e.key === 'ArrowLeft') {
        runtime.player.seek(runtime.player.getSnapshot().played - 1);
      } else if (e.key === 'ArrowRight') {
        runtime.player.seek(runtime.player.getSnapshot().played + 1);
      } else if (e.key.toLowerCase() === 'l') {
        runtime.player.follow();
      } else if (e.key.toLowerCase() === 'b') {
        toggleFiles();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [runtime, changeView, toggleFiles]);

  useEffect(() => {
    document.title = 'Agent activity';
  }, []);

  if (!runtime) return <div className="aa-root" />;

  const value: ActivityRuntime = {
    ...runtime,
    captions,
    focusFile,
    focusRequest,
    status,
  };
  if (import.meta.env?.DEV)
    (window as { __agentActivity?: ActivityRuntime }).__agentActivity = value;
  const noServer = scenario === null && reachable === false;

  return (
    <RuntimeProvider value={value}>
      <div className="aa-root" data-picker={scenario !== null || noServer}>
        <MenuBar
          captions={captions}
          filesOpen={filesOpen}
          onCaptions={(on) => {
            setCaptions(on);
            writePref('captions', on);
          }}
          onToggleFiles={toggleFiles}
          onView={changeView}
          view={view}
        />
        {(scenario !== null || noServer) && (
          <ScenarioBar
            onRestart={() => setRun((n) => n + 1)}
            onScenario={chooseScenario}
            scenario={scenario}
          />
        )}
        <div className="aa-body" data-files={filesOpen ? 'open' : 'closed'}>
          {filesOpen && <FileRail />}
          <main className="aa-main">
            {noServer ? (
              <NoServer onScenario={chooseScenario} />
            ) : (
              <>
                {view === 'stage' && <StageView />}
                {view === 'tiles' && <TilesView />}
                {view === 'flow' && <FlowView />}
                {view === 'graph' && <GraphView />}
                {view === 'contracts' && <ContractsView />}
                {view === 'feed' && <FeedView />}
                {view === 'stage' && <IdleRecap />}
              </>
            )}
          </main>
        </div>
        <Timeline />
      </div>
    </RuntimeProvider>
  );
}

function NoServer({ onScenario }: { onScenario: (id: string) => void }) {
  return (
    <div className="aa-waiting">
      <h2>No watch server at this address</h2>
      <p>
        Run <code>baml agent watch</code> in the project your agent is working
        on; it opens this page and streams every file change. Or replay a
        recorded session:
      </p>
      <div className="aa-waiting-actions">
        {MOCK_SCENARIOS.map((s) => (
          <button
            className="aa-scenario"
            key={s.id}
            onClick={() => onScenario(s.id)}
            title={s.description}
            type="button"
          >
            <span className="aa-scenario-title">{s.title}</span>
            <span className="aa-scenario-meta">{s.summary}</span>
          </button>
        ))}
      </div>
    </div>
  );
}
