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

function readStoredEnvVars(): Record<string, string> {
  if (typeof window === 'undefined') return {};

  try {
    const raw = window.localStorage.getItem(ENV_VARS_STORAGE_KEY);
    if (!raw) return {};

    const parsed = JSON.parse(raw) as unknown;
    if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) return {};

    const result: Record<string, string> = {};
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

function writeStoredEnvVars(vars: Record<string, string>) {
  if (typeof window === 'undefined') return;

  try {
    if (Object.keys(vars).length === 0) {
      window.localStorage.removeItem(ENV_VARS_STORAGE_KEY);
    } else {
      window.localStorage.setItem(ENV_VARS_STORAGE_KEY, JSON.stringify(vars));
    }
  } catch {
    // localStorage may be unavailable or full; env vars still work in memory.
  }
}

/** Current env var key-value pairs. */
export const envVarsAtom = atom<Record<string, string>>(readStoredEnvVars());

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
  const hydratedRef = useRef(false);

  useEffect(() => {
    if (hydratedRef.current) return;
    hydratedRef.current = true;

    const stored = readStoredEnvVars();
    if (Object.keys(stored).length === 0) return;

    setEnvVars((prev) => ({ ...stored, ...prev }));
    for (const [key, value] of Object.entries(stored)) {
      port.postMessage({ type: 'setEnvVar', key, value });
    }
  }, [setEnvVars, port]);

  const addEnvVar = useCallback((key: string, value: string) => {
    setEnvVars((prev) => {
      const next = { ...prev, [key]: value };
      writeStoredEnvVars(next);
      return next;
    });
    port.postMessage({ type: 'setEnvVar', key, value });
  }, [setEnvVars, port]);

  const removeEnvVar = useCallback((key: string) => {
    setEnvVars((prev: Record<string, string>) => {
      const { [key]: _, ...rest } = prev;
      writeStoredEnvVars(rest);
      return rest;
    });
    port.postMessage({ type: 'deleteEnvVar', key });
  }, [setEnvVars, port]);

  const importEnvVars = useCallback((vars: Record<string, string>) => {
    setEnvVars((prev) => {
      const next = { ...prev, ...vars };
      writeStoredEnvVars(next);
      return next;
    });
    for (const [key, value] of Object.entries(vars)) {
      port.postMessage({ type: 'setEnvVar', key, value });
    }
  }, [setEnvVars, port]);

  const addRequiredKey = useCallback((key: string) => {
    setRequiredKeys((prev) => prev.has(key) ? prev : new Set([...prev, key]));
  }, [setRequiredKeys]);

  return { envVars, knownRequiredKeys: requiredKeys, addEnvVar, removeEnvVar, importEnvVars, addRequiredKey };
}
