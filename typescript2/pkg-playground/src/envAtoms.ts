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
  return window.sessionStorage;
}

function readStoredEnvVars(): EnvVars {
  if (typeof window === 'undefined') return {};

  try {
    const store = storage();
    const sessionRaw = store?.getItem(ENV_VARS_STORAGE_KEY);
    const legacyRaw = sessionRaw == null
      ? window.localStorage.getItem(ENV_VARS_STORAGE_KEY)
      : null;
    const raw = sessionRaw ?? legacyRaw;
    if (legacyRaw != null) {
      store?.setItem(ENV_VARS_STORAGE_KEY, legacyRaw);
    }
    window.localStorage.removeItem(ENV_VARS_STORAGE_KEY);
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

// ---------------------------------------------------------------------------
// Hook
// ---------------------------------------------------------------------------

export interface UseEnvVars {
  envVars: Record<string, string>;
  knownRequiredKeys: Set<string>;
  addEnvVar: (key: string, value: string) => void;
  removeEnvVar: (key: string) => void;
  importEnvVars: (vars: Record<string, string>) => void;
  addRequiredKey: (key: string) => void;
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

  useEffect(() => {
    envVarsRef.current = envVars;
  }, [envVars]);

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
    setEnvVars((prev) => {
      const next = { ...prev, [key]: value };
      return next;
    });
  }, [setEnvVars]);

  const removeEnvVar = useCallback((key: string) => {
    setEnvVars((prev: EnvVars) => {
      const { [key]: _, ...rest } = prev;
      return rest;
    });
  }, [setEnvVars]);

  const importEnvVars = useCallback((vars: EnvVars) => {
    setEnvVars((prev) => {
      const next = { ...prev, ...vars };
      return next;
    });
  }, [setEnvVars]);

  const addRequiredKey = useCallback((key: string) => {
    setRequiredKeys((prev) => prev.has(key) ? prev : new Set([...prev, key]));
  }, [setRequiredKeys]);

  return { envVars, knownRequiredKeys: requiredKeys, addEnvVar, removeEnvVar, importEnvVars, addRequiredKey };
}
