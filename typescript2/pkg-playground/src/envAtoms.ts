/**
 * Jotai atoms for environment variable state.
 *
 * These atoms are the single source of truth for env vars across the
 * playground UI. Components read/write via `useEnvVars()` or directly
 * via `useAtom(envVarsAtom)` / `useAtomValue(knownRequiredKeysAtom)`.
 */

import { useCallback } from 'react';
import { atom, useAtom, useSetAtom, useAtomValue } from 'jotai';
import type { RuntimePort } from './runtime-port';

// ---------------------------------------------------------------------------
// Atoms
// ---------------------------------------------------------------------------

/** Current env var key-value pairs. */
export const envVarsAtom = atom<Record<string, string>>({});

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

  const addEnvVar = useCallback((key: string, value: string) => {
    setEnvVars((prev) => ({ ...prev, [key]: value }));
    port.postMessage({ type: 'setEnvVar', key, value });
  }, [setEnvVars, port]);

  const removeEnvVar = useCallback((key: string) => {
    setEnvVars((prev: Record<string, string>) => {
      const { [key]: _, ...rest } = prev;
      return rest;
    });
    port.postMessage({ type: 'deleteEnvVar', key });
  }, [setEnvVars, port]);

  const importEnvVars = useCallback((vars: Record<string, string>) => {
    setEnvVars((prev) => ({ ...prev, ...vars }));
    for (const [key, value] of Object.entries(vars)) {
      port.postMessage({ type: 'setEnvVar', key, value });
    }
  }, [setEnvVars, port]);

  const addRequiredKey = useCallback((key: string) => {
    setRequiredKeys((prev) => prev.has(key) ? prev : new Set([...prev, key]));
  }, [setRequiredKeys]);

  return { envVars, knownRequiredKeys: requiredKeys, addEnvVar, removeEnvVar, importEnvVars, addRequiredKey };
}
