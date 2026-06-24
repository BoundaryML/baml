import type { RuntimePort } from './runtime-port';

const ENV_VARS_STORAGE_KEY = 'baml-playground-env-vars';

export type EnvVars = Record<string, string>;
export type EnvVarsUpdate = EnvVars | ((prev: EnvVars) => EnvVars);
export type StringSetUpdate = Set<string> | ((prev: Set<string>) => Set<string>);

export interface SessionStoreSnapshot {
  envVars: EnvVars;
  knownRequiredKeys: Set<string>;
  shellEnvVars: EnvVars;
  shellOverriddenKeys: Set<string>;
  shellDeletedKeys: Set<string>;
}

export interface SessionStoreStorage {
  readEnvVars(): EnvVars;
  writeEnvVars(vars: EnvVars): void;
}

export interface SessionStore {
  getSnapshot(): SessionStoreSnapshot;
  subscribe(listener: (snapshot: SessionStoreSnapshot) => void): () => void;
  attachRuntimePort(port: RuntimePort): () => void;
  setEnvVars(update: EnvVarsUpdate): void;
  addEnvVar(key: string, value: string): void;
  removeEnvVar(key: string): void;
  importEnvVars(vars: EnvVars): void;
  setKnownRequiredKeys(update: StringSetUpdate): void;
  addRequiredKey(key: string): void;
  addShellEnvVar(key: string, value: string): void;
  importShellEnvVars(vars: EnvVars): void;
  revertToShell(key: string): void;
}

interface CreateSessionStoreOptions {
  storage?: SessionStoreStorage;
  initialEnvVars?: EnvVars;
}

export function createSessionStore(
  options: CreateSessionStoreOptions = {},
): SessionStore {
  const storage = options.storage ?? browserSessionStoreStorage;
  const listeners = new Set<(snapshot: SessionStoreSnapshot) => void>();
  const runtimePorts = new Set<RuntimePort>();

  let envVars: EnvVars = {
    ...storage.readEnvVars(),
    ...(options.initialEnvVars ?? {}),
  };
  let knownRequiredKeys = new Set<string>();
  let shellEnvVars: EnvVars = {};
  let shellOverriddenKeys = new Set<string>();
  let shellDeletedKeys = new Set<string>();

  function snapshot(): SessionStoreSnapshot {
    return {
      envVars: { ...envVars },
      knownRequiredKeys: new Set(knownRequiredKeys),
      shellEnvVars: { ...shellEnvVars },
      shellOverriddenKeys: new Set(shellOverriddenKeys),
      shellDeletedKeys: new Set(shellDeletedKeys),
    };
  }

  function emit() {
    const current = snapshot();
    for (const listener of listeners) listener(current);
  }

  function persistEnv() {
    storage.writeEnvVars(envVars);
  }

  function syncEnvVarsToPorts(prev: EnvVars, next: EnvVars) {
    for (const port of runtimePorts) syncEnvVarsToPort(port, prev, next);
  }

  function replaceEnvVars(next: EnvVars): boolean {
    if (sameEnvVars(envVars, next)) return false;
    const prev = envVars;
    envVars = { ...next };
    persistEnv();
    syncEnvVarsToPorts(prev, envVars);
    return true;
  }

  function postDeleteEnvOverride(key: string) {
    for (const port of runtimePorts) {
      port.postMessage({ type: 'deleteEnvVar', key });
    }
  }

  return {
    getSnapshot: snapshot,

    subscribe(listener) {
      listeners.add(listener);
      listener(snapshot());
      return () => {
        listeners.delete(listener);
      };
    },

    attachRuntimePort(port) {
      runtimePorts.add(port);
      syncEnvVarsToPort(port, {}, envVars);
      return () => {
        runtimePorts.delete(port);
      };
    },

    setEnvVars(update) {
      const next =
        typeof update === 'function' ? update({ ...envVars }) : update;
      if (replaceEnvVars(next)) emit();
    },

    addEnvVar(key, value) {
      const shellVal = shellEnvVars[key];
      let flagsChanged = false;

      if (shellVal !== undefined && shellVal !== value) {
        if (!shellOverriddenKeys.has(key)) {
          shellOverriddenKeys = new Set([...shellOverriddenKeys, key]);
          flagsChanged = true;
        }
        if (shellDeletedKeys.has(key)) {
          shellDeletedKeys = deleteFromSet(shellDeletedKeys, key);
          flagsChanged = true;
        }
      } else if (shellVal === value && shellOverriddenKeys.has(key)) {
        shellOverriddenKeys = deleteFromSet(shellOverriddenKeys, key);
        flagsChanged = true;
      }

      const envChanged = replaceEnvVars({ ...envVars, [key]: value });
      if (envChanged || flagsChanged) emit();
    },

    removeEnvVar(key) {
      const { [key]: _removed, ...next } = envVars;
      let flagsChanged = false;

      if (shellEnvVars[key] !== undefined) {
        if (!shellDeletedKeys.has(key)) {
          shellDeletedKeys = new Set([...shellDeletedKeys, key]);
          flagsChanged = true;
        }
        if (!shellOverriddenKeys.has(key)) {
          shellOverriddenKeys = new Set([...shellOverriddenKeys, key]);
          flagsChanged = true;
        }
      } else if (shellOverriddenKeys.has(key)) {
        shellOverriddenKeys = deleteFromSet(shellOverriddenKeys, key);
        flagsChanged = true;
      }

      const envChanged = replaceEnvVars(next);
      if (envChanged || flagsChanged) emit();
    },

    importEnvVars(vars) {
      const envChanged = replaceEnvVars({ ...envVars, ...vars });
      if (envChanged) emit();
    },

    setKnownRequiredKeys(update) {
      const next =
        typeof update === 'function'
          ? update(new Set(knownRequiredKeys))
          : update;
      if (sameStringSet(knownRequiredKeys, next)) return;
      knownRequiredKeys = new Set(next);
      emit();
    },

    addRequiredKey(key) {
      if (knownRequiredKeys.has(key)) return;
      knownRequiredKeys = new Set([...knownRequiredKeys, key]);
      emit();
    },

    addShellEnvVar(key, value) {
      let changed = false;
      if (shellEnvVars[key] !== value) {
        shellEnvVars = { ...shellEnvVars, [key]: value };
        changed = true;
      }

      if (!(key in envVars)) {
        replaceEnvVars({ ...envVars, [key]: value });
        changed = true;
      }

      if (changed) emit();
    },

    importShellEnvVars(vars) {
      shellEnvVars = { ...vars };
      let nextEnvVars = envVars;
      let envChanged = false;
      for (const [key, value] of Object.entries(vars)) {
        if (!(key in nextEnvVars)) {
          if (!envChanged) nextEnvVars = { ...nextEnvVars };
          nextEnvVars[key] = value;
          envChanged = true;
        }
      }
      if (envChanged) replaceEnvVars(nextEnvVars);
      emit();
    },

    revertToShell(key) {
      const shellVal = shellEnvVars[key];
      if (shellVal === undefined) return;

      const envChanged = replaceEnvVars({ ...envVars, [key]: shellVal });
      let flagsChanged = false;
      if (shellOverriddenKeys.has(key)) {
        shellOverriddenKeys = deleteFromSet(shellOverriddenKeys, key);
        flagsChanged = true;
      }
      if (shellDeletedKeys.has(key)) {
        shellDeletedKeys = deleteFromSet(shellDeletedKeys, key);
        flagsChanged = true;
      }

      postDeleteEnvOverride(key);
      if (envChanged || flagsChanged) emit();
    },
  };
}

export const browserSessionStoreStorage: SessionStoreStorage = {
  readEnvVars: readStoredEnvVars,
  writeEnvVars: writeStoredEnvVars,
};

export const defaultSessionStore = createSessionStore({
  storage: browserSessionStoreStorage,
});

function storage(): Storage | null {
  if (typeof window === 'undefined') return null;
  return window.localStorage;
}

function readStoredEnvVars(): EnvVars {
  if (typeof window === 'undefined') return {};

  try {
    const store = storage();
    const localRaw = store?.getItem(ENV_VARS_STORAGE_KEY);
    const sessionRaw =
      localRaw == null
        ? window.sessionStorage.getItem(ENV_VARS_STORAGE_KEY)
        : null;
    const raw = localRaw ?? sessionRaw;
    if (sessionRaw != null) {
      store?.setItem(ENV_VARS_STORAGE_KEY, sessionRaw);
      window.sessionStorage.removeItem(ENV_VARS_STORAGE_KEY);
    }
    if (!raw) return {};

    const parsed = JSON.parse(raw) as unknown;
    if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) {
      return {};
    }

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

function syncEnvVarsToPort(port: RuntimePort, prev: EnvVars, next: EnvVars) {
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

function sameEnvVars(left: EnvVars, right: EnvVars): boolean {
  const leftKeys = Object.keys(left);
  const rightKeys = Object.keys(right);
  if (leftKeys.length !== rightKeys.length) return false;
  return leftKeys.every((key) => left[key] === right[key]);
}

function sameStringSet(left: Set<string>, right: Set<string>): boolean {
  if (left.size !== right.size) return false;
  for (const value of left) {
    if (!right.has(value)) return false;
  }
  return true;
}

function deleteFromSet(values: Set<string>, key: string): Set<string> {
  const next = new Set(values);
  next.delete(key);
  return next;
}
