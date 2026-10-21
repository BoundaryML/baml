import {
  createContext,
  type ReactNode,
  useContext,
  useEffect,
  useState,
  useSyncExternalStore,
} from 'react';
import type { ActivityModel } from './engine/model';
import type { Player, PlayerSnapshot } from './engine/player';
import type { ActivitySource, ConnectionStatus } from './sources/source';

export interface ActivityRuntime {
  model: ActivityModel;
  player: Player;
  source: ActivitySource;
  status: ConnectionStatus;
  /** Focus a file in the current view (from the file rail or the timeline). */
  focusFile: (path: string) => void;
  focusRequest: { path: string; nonce: number } | null;
  /** Show the agent's messages as captions. */
  captions: boolean;
}

const RuntimeContext = createContext<ActivityRuntime | null>(null);

export function RuntimeProvider({
  value,
  children,
}: {
  value: ActivityRuntime;
  children: ReactNode;
}) {
  return (
    <RuntimeContext.Provider value={value}>{children}</RuntimeContext.Provider>
  );
}

export function useRuntime(): ActivityRuntime {
  const runtime = useContext(RuntimeContext);
  if (!runtime) throw new Error('useRuntime outside RuntimeProvider');
  return runtime;
}

/** The model, re-rendering whenever it changes. */
export function useModel(): ActivityModel {
  const { model } = useRuntime();
  useSyncExternalStore(
    (listener) => model.subscribe(listener),
    () => model.version,
  );
  return model;
}

export function usePlayer(): PlayerSnapshot {
  const { player } = useRuntime();
  return useSyncExternalStore(player.subscribe, player.getSnapshot);
}

/** `performance.now()`, refreshed every `intervalMs`. */
export function useTicker(intervalMs: number): number {
  const [now, setNow] = useState(() => performance.now());
  useEffect(() => {
    const id = setInterval(() => setNow(performance.now()), intervalMs);
    return () => clearInterval(id);
  }, [intervalMs]);
  return now;
}
