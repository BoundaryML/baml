import { describe, expect, it } from 'vitest';

import {
  createSessionStore,
  type EnvVars,
  type SessionStoreStorage,
} from './session-store';
import type { RuntimePort } from './runtime-port';
import type { WorkerInMessage, WorkerOutMessage } from './worker-protocol';

describe('session-store', () => {
  it('hydrates and persists env values through the configured storage adapter', () => {
    const storage = new MemorySessionStorage({ STORED: '1' });
    const store = createSessionStore({ storage });

    expect(store.getSnapshot().envVars).toEqual({ STORED: '1' });

    store.addEnvVar('OPENAI_API_KEY', 'secret');
    expect(storage.value).toEqual({
      STORED: '1',
      OPENAI_API_KEY: 'secret',
    });

    store.removeEnvVar('STORED');
    expect(storage.value).toEqual({ OPENAI_API_KEY: 'secret' });
  });

  it('mirrors env values to runtime ports only through SessionStore attachment', () => {
    const port = new FakeRuntimePort();
    const store = createSessionStore({
      storage: new MemorySessionStorage(),
      initialEnvVars: { A: '1' },
    });

    const detach = store.attachRuntimePort(port);
    expect(port.sent).toEqual([{ type: 'setEnvVar', key: 'A', value: '1' }]);

    store.addEnvVar('B', '2');
    store.removeEnvVar('A');
    expect(port.sent).toEqual([
      { type: 'setEnvVar', key: 'A', value: '1' },
      { type: 'setEnvVar', key: 'B', value: '2' },
      { type: 'deleteEnvVar', key: 'A' },
    ]);

    detach();
    store.addEnvVar('C', '3');
    expect(port.sent).toHaveLength(3);
  });

  it('keeps process env imports in SessionStore and clears native overrides on revert', () => {
    const port = new FakeRuntimePort();
    const store = createSessionStore({ storage: new MemorySessionStorage() });
    store.importShellEnvVars({ API_KEY: 'from-shell' });
    store.attachRuntimePort(port);

    store.addEnvVar('API_KEY', 'from-user');
    expect(store.getSnapshot().shellOverriddenKeys.has('API_KEY')).toBe(true);

    store.revertToShell('API_KEY');
    const snapshot = store.getSnapshot();
    expect(snapshot.envVars.API_KEY).toBe('from-shell');
    expect(snapshot.shellOverriddenKeys.has('API_KEY')).toBe(false);
    expect(snapshot.shellDeletedKeys.has('API_KEY')).toBe(false);
    expect(port.sent.at(-1)).toEqual({ type: 'deleteEnvVar', key: 'API_KEY' });
  });
});

class MemorySessionStorage implements SessionStoreStorage {
  value: EnvVars;

  constructor(initial: EnvVars = {}) {
    this.value = { ...initial };
  }

  readEnvVars(): EnvVars {
    return { ...this.value };
  }

  writeEnvVars(vars: EnvVars): void {
    this.value = { ...vars };
  }
}

class FakeRuntimePort implements RuntimePort {
  sent: WorkerInMessage[] = [];

  postMessage(msg: WorkerInMessage): void {
    this.sent.push(msg);
  }

  onMessage(_handler: (msg: WorkerOutMessage) => void): () => void {
    return () => {};
  }

  dispose(): void {}
}
