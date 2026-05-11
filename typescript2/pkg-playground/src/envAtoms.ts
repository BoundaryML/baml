/**
 * Jotai atoms for environment variable state.
 *
 * These atoms are the single source of truth for env vars across the
 * playground UI. Components read/write via `useEnvVars()` or directly
 * via `useAtom(envVarsAtom)` / `useAtomValue(knownRequiredKeysAtom)`.
 */

import { useCallback, useEffect, useRef } from 'react';
import { atom, useAtom, useSetAtom, useAtomValue } from 'jotai';
import type { RuntimePort } from './runtime-port';

// ---------------------------------------------------------------------------
// Atoms
// ---------------------------------------------------------------------------

const ENV_VARS_STORAGE_KEY = 'baml-playground-env-vars';
type EnvVars = Record<string, string>;
type EnvVarsUpdate = EnvVars | ((prev: EnvVars) => EnvVars);

function storage(): Storage | null {
  if (typeof window === 'undefined') return null;
  return window.localStorage;
}

function readStoredEnvVars(): EnvVars {
  if (typeof window === 'undefined') return {};

  try {
    const store = storage();
    const localRaw = store?.getItem(ENV_VARS_STORAGE_KEY);
    const sessionRaw = localRaw == null
      ? window.sessionStorage.getItem(ENV_VARS_STORAGE_KEY)
      : null;
    const raw = localRaw ?? sessionRaw;
    if (sessionRaw != null) {
      store?.setItem(ENV_VARS_STORAGE_KEY, sessionRaw);
      window.sessionStorage.removeItem(ENV_VARS_STORAGE_KEY);
    }
    if (!raw) return {};

    const parsed = JSON.parse(raw) as unknown;
    if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) return {};

    const result: EnvVars = {};
    for (const [key, value] of Object.entries(parsed)) {
      if (typeof key === 'string' && typeof value === 'string') {
        result[key] = value;
      }
    }
    return result;
  } catch {
    return {};
  }
}

function writeStoredEnvVars(vars: EnvVars) {
  if (typeof window === 'undefined') return;

  try {
    const store = storage();
    if (!store) return;
    if (Object.keys(vars).length === 0) {
      store.removeItem(ENV_VARS_STORAGE_KEY);
    } else {
      store.setItem(ENV_VARS_STORAGE_KEY, JSON.stringify(vars));
    }
  } catch {
    // Storage may be unavailable or full; env vars still work in memory.
  }
}

function syncEnvVarsToPort(port: RuntimePort | null, prev: EnvVars, next: EnvVars) {
  if (!port) return;

  for (const key of Object.keys(prev)) {
    if (!(key in next)) {
      port.postMessage({ type: 'deleteEnvVar', key });
    }
  }
  for (const [key, value] of Object.entries(next)) {
    if (prev[key] !== value) {
      port.postMessage({ type: 'setEnvVar', key, value });
    }
  }
}

/** Current env var key-value pairs. */
const envVarsBaseAtom = atom<EnvVars>(readStoredEnvVars());
const runtimePortAtom = atom<RuntimePort | null>(null);
export const envVarsAtom = atom(
  (get) => get(envVarsBaseAtom),
  (get, set, update: EnvVarsUpdate) => {
    const prev = get(envVarsBaseAtom);
    const next = typeof update === 'function' ? update(prev) : update;
    set(envVarsBaseAtom, next);
    writeStoredEnvVars(next);
    syncEnvVarsToPort(get(runtimePortAtom), prev, next);
  },
);

/**
 * Keys the project is known to need — accumulated from worker envVarRequests.
 * Never shrunk during a session so the UI can proactively show missing keys.
 */
export const knownRequiredKeysAtom = atom<Set<string>>(new Set<string>());

/**
 * Original process env vars from the server (set once on init, never mutated).
 * Used to display shell-sourced vars and support "revert to shell" functionality.
 */
export const shellEnvVarsAtom = atom<EnvVars>({});

/**
 * Shell env keys that the user has manually overridden or deleted.
 * A key present here means the user changed or removed the value in the dialog.
 */
export const shellOverriddenKeysAtom = atom<Set<string>>(new Set<string>());

/**
 * Shell env keys that the user has deleted.
 * These are still shown in the dialog so the user can revert them.
 */
export const shellDeletedKeysAtom = atom<Set<string>>(new Set<string>());

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
  const [envVars, setEnvVars] = useAtom(envVarsAtom);
  const requiredKeys = useAtomValue(knownRequiredKeysAtom);
  const setRequiredKeys = useSetAtom(knownRequiredKeysAtom);
  const setRuntimePort = useSetAtom(runtimePortAtom);
  const lastHydratedPortRef = useRef<RuntimePort | null>(null);
  const envVarsRef = useRef(envVars);

  const [shellEnvVars, setShellEnvVars] = useAtom(shellEnvVarsAtom);
  const [shellOverriddenKeys, setShellOverriddenKeys] = useAtom(shellOverriddenKeysAtom);
  const [shellDeletedKeys, setShellDeletedKeys] = useAtom(shellDeletedKeysAtom);
  const shellEnvVarsRef = useRef(shellEnvVars);

  useEffect(() => {
    envVarsRef.current = envVars;
  }, [envVars]);

  useEffect(() => {
    shellEnvVarsRef.current = shellEnvVars;
  }, [shellEnvVars]);

  useEffect(() => {
    setRuntimePort(port);
    return () => {
      setRuntimePort((current) => current === port ? null : current);
    };
  }, [setRuntimePort, port]);

  useEffect(() => {
    if (lastHydratedPortRef.current === port) return;
    lastHydratedPortRef.current = port;

    const stored = readStoredEnvVars();
    const merged = { ...stored, ...envVarsRef.current };

    if (Object.keys(stored).length > 0) {
      setEnvVars(merged);
    }
    for (const [key, value] of Object.entries(merged)) {
      port.postMessage({ type: 'setEnvVar', key, value });
    }
  }, [setEnvVars, port]);

  const addEnvVar = useCallback((key: string, value: string) => {
    setEnvVars((prev) => ({ ...prev, [key]: value }));
    // If this key came from the shell and the value differs, mark as overridden.
    const shellVal = shellEnvVarsRef.current[key];
    if (shellVal !== undefined && shellVal !== value) {
      setShellOverriddenKeys((prev) => {
        if (prev.has(key)) return prev;
        return new Set([...prev, key]);
      });
    } else if (shellVal === value) {
      // Value matches shell — no longer overridden.
      setShellOverriddenKeys((prev) => {
        if (!prev.has(key)) return prev;
        const next = new Set(prev);
        next.delete(key);
        return next;
      });
    }
  }, [setEnvVars, setShellOverriddenKeys]);

  const removeEnvVar = useCallback((key: string) => {
    setEnvVars((prev: EnvVars) => {
      const { [key]: _, ...rest } = prev;
      return rest;
    });
    // If this was a shell key, track it as deleted (so the UI can offer revert).
    if (shellEnvVarsRef.current[key] !== undefined) {
      setShellDeletedKeys((prev) => {
        if (prev.has(key)) return prev;
        return new Set([...prev, key]);
      });
      setShellOverriddenKeys((prev) => {
        if (prev.has(key)) return prev;
        return new Set([...prev, key]);
      });
    } else {
      setShellOverriddenKeys((prev) => {
        if (!prev.has(key)) return prev;
        const next = new Set(prev);
        next.delete(key);
        return next;
      });
    }
  }, [setEnvVars, setShellOverriddenKeys, setShellDeletedKeys]);

  const importEnvVars = useCallback((vars: EnvVars) => {
    setEnvVars((prev) => ({ ...prev, ...vars }));
  }, [setEnvVars]);

  const addRequiredKey = useCallback((key: string) => {
    setRequiredKeys((prev) => prev.has(key) ? prev : new Set([...prev, key]));
  }, [setRequiredKeys]);

  const addShellEnvVar = useCallback((key: string, value: string) => {
    // Store as shell original.
    setShellEnvVars((prev) => prev[key] === value ? prev : { ...prev, [key]: value });
    // Only import into env vars if user hasn't already set a value.
    setEnvVars((prev) => {
      if (key in prev) return prev;
      return { ...prev, [key]: value };
    });
  }, [setEnvVars, setShellEnvVars]);

  const importShellEnvVars = useCallback((vars: EnvVars) => {
    // Store all originals.
    setShellEnvVars(vars);
    // Merge into env vars, but don't overwrite existing user entries.
    setEnvVars((prev) => {
      const merged = { ...prev };
      let changed = false;
      for (const [key, value] of Object.entries(vars)) {
        if (!(key in merged)) {
          merged[key] = value;
          changed = true;
        }
      }
      return changed ? merged : prev;
    });
  }, [setEnvVars, setShellEnvVars]);

  const revertToShell = useCallback((key: string) => {
    const shellVal = shellEnvVarsRef.current[key];
    if (shellVal === undefined) return;
    // Restore the shell value.
    setEnvVars((prev) => ({ ...prev, [key]: shellVal }));
    // Clear override and deleted flags.
    setShellOverriddenKeys((prev) => {
      if (!prev.has(key)) return prev;
      const next = new Set(prev);
      next.delete(key);
      return next;
    });
    setShellDeletedKeys((prev) => {
      if (!prev.has(key)) return prev;
      const next = new Set(prev);
      next.delete(key);
      return next;
    });
    // Tell the server to remove the override so it falls back to process env.
    port.postMessage({ type: 'deleteEnvVar', key });
  }, [setEnvVars, setShellOverriddenKeys, setShellDeletedKeys, port]);

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
