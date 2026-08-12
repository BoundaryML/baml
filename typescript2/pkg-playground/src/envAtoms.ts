/**
 * Jotai mirrors for SessionStore environment variable state.
 *
 * SessionStore is the single source of truth for env values. These atoms keep
 * React components subscribed to that store and preserve the existing
 * `useEnvVars()` API.
 */

import { useCallback, useEffect, useRef } from 'react';
import { atom, useAtomValue } from 'jotai';
import type { RuntimePort } from './runtime-port';
import {
  defaultSessionStore,
  type EnvVars,
  type EnvVarsUpdate,
  type SessionStoreSnapshot,
  type StringSetUpdate,
} from './session-store';

// ---------------------------------------------------------------------------
// Atoms
// ---------------------------------------------------------------------------

const sessionSnapshotAtom = atom<SessionStoreSnapshot>(
  defaultSessionStore.getSnapshot(),
);
sessionSnapshotAtom.onMount = (setAtom) =>
  defaultSessionStore.subscribe(setAtom);

/** Current env var key-value pairs. */
export const envVarsAtom = atom(
  (get) => get(sessionSnapshotAtom).envVars,
  (_get, _set, update: EnvVarsUpdate) =>
    defaultSessionStore.setEnvVars(update),
);

/**
 * Keys the project is known to need, accumulated from worker envVarRequests.
 * Never shrunk during a session so the UI can proactively show missing keys.
 */
export const knownRequiredKeysAtom = atom(
  (get) => get(sessionSnapshotAtom).knownRequiredKeys,
  (_get, _set, update: StringSetUpdate) =>
    defaultSessionStore.setKnownRequiredKeys(update),
);

/**
 * Original process env vars from the server (set once on init, never mutated).
 * Used to display shell-sourced vars and support "revert to shell" functionality.
 */
export const shellEnvVarsAtom = atom(
  (get) => get(sessionSnapshotAtom).shellEnvVars,
);

/**
 * Shell env keys that the user has manually overridden or deleted.
 * A key present here means the user changed or removed the value in the dialog.
 */
export const shellOverriddenKeysAtom = atom(
  (get) => get(sessionSnapshotAtom).shellOverriddenKeys,
);

/**
 * Shell env keys that the user has deleted.
 * These are still shown in the dialog so the user can revert them.
 */
export const shellDeletedKeysAtom = atom(
  (get) => get(sessionSnapshotAtom).shellDeletedKeys,
);

// ---------------------------------------------------------------------------
// Hook
// ---------------------------------------------------------------------------

export interface UseEnvVars {
  envVars: Record<string, string>;
  knownRequiredKeys: Set<string>;
  /** Original process env vars from the server's shell. */
  shellEnvVars: Record<string, string>;
  /** Shell keys that the user has manually overridden or deleted. */
  shellOverriddenKeys: Set<string>;
  /** Shell keys that the user has deleted. */
  shellDeletedKeys: Set<string>;
  addEnvVar: (key: string, value: string) => void;
  removeEnvVar: (key: string) => void;
  importEnvVars: (vars: Record<string, string>) => void;
  addRequiredKey: (key: string) => void;
  /** Import a single shell-provided env var. Does NOT overwrite user-entered values. */
  addShellEnvVar: (key: string, value: string) => void;
  /** Bulk import process env vars on init. Stores originals and merges into env vars. */
  importShellEnvVars: (vars: Record<string, string>) => void;
  /** Revert a key to its original shell value (clears the user override). */
  revertToShell: (key: string) => void;
}

/**
 * Convenience hook that wires atom reads/writes to a RuntimePort.
 *
 * Every mutation is mirrored to the worker via `port.postMessage` so the
 * WASM runtime stays in sync.
 */
export function useEnvVars(port: RuntimePort): UseEnvVars {
  const session = useAtomValue(sessionSnapshotAtom);
  const envVars = session.envVars;
  const requiredKeys = session.knownRequiredKeys;
  const shellEnvVars = session.shellEnvVars;
  const shellOverriddenKeys = session.shellOverriddenKeys;
  const shellDeletedKeys = session.shellDeletedKeys;
  const envVarsRef = useRef(envVars);

  useEffect(() => {
    envVarsRef.current = envVars;
  }, [envVars]);

  useEffect(() => {
    return defaultSessionStore.attachRuntimePort(port);
  }, [port]);

  const addEnvVar = useCallback(
    (key: string, value: string) => {
      defaultSessionStore.addEnvVar(key, value);
    },
    [],
  );

  const removeEnvVar = useCallback(
    (key: string) => {
      defaultSessionStore.removeEnvVar(key);
    },
    [],
  );

  const importEnvVars = useCallback(
    (vars: EnvVars) => {
      defaultSessionStore.importEnvVars(vars);
    },
    [],
  );

  const addRequiredKey = useCallback(
    (key: string) => {
      defaultSessionStore.addRequiredKey(key);
    },
    [],
  );

  const addShellEnvVar = useCallback(
    (key: string, value: string) => {
      defaultSessionStore.addShellEnvVar(key, value);
    },
    [],
  );

  const importShellEnvVars = useCallback(
    (vars: EnvVars) => {
      defaultSessionStore.importShellEnvVars(vars);
    },
    [],
  );

  const revertToShell = useCallback(
    (key: string) => {
      defaultSessionStore.revertToShell(key);
    },
    [],
  );

  return {
    envVars,
    knownRequiredKeys: requiredKeys,
    shellEnvVars,
    shellOverriddenKeys,
    shellDeletedKeys,
    addEnvVar,
    removeEnvVar,
    importEnvVars,
    addRequiredKey,
    addShellEnvVar,
    importShellEnvVars,
    revertToShell,
  };
}
