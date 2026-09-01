import { readRunResult } from './result.mjs';

const TERMINAL = new Set(['succeeded', 'failed', 'cancelled', 'panicked']);

/** serde-wasm-bindgen returns Maps for Rust structs. */
export function toPlain(value) {
  if (value instanceof Map) {
    return Object.fromEntries(
      [...value].map(([key, item]) => [String(key), toPlain(item)]),
    );
  }
  if (Array.isArray(value)) return value.map(toPlain);
  if (value && typeof value === 'object') {
    return Object.fromEntries(
      Object.entries(value).map(([key, item]) => [key, toPlain(item)]),
    );
  }
  return value;
}

export class RunTimeout extends Error {
  constructor(milliseconds) {
    super(`the run did not finish within ${milliseconds}ms`);
    this.name = 'RunTimeout';
  }
}

/** Create an isolated, zero-network BAML project session. */
export async function createSession(wasm, Vfs, files, options = {}) {
  const root = options.root ?? '/workspace';
  const vfs = new Vfs(root);
  vfs.setFiles(files);

  const lspPending = new Map();
  const runListeners = new Set();
  const valueWaiters = new Map();
  let nextLspId = 0;
  let nextRequestId = 1;
  let projectId = null;
  let resolveProject;
  const projectReady = new Promise((resolve) => {
    resolveProject = resolve;
  });

  const unavailable = (capability) => () => {
    throw new Error(`${capability} is disabled for documentation examples`);
  };

  await wasm.start();
  const runtime = wasm.BamlWasmRuntime.create(
    {
      env: async () => undefined,
      fetch: unavailable('network access'),
      exec: unavailable('exec'),
      shell: unavailable('shell'),
      input: unavailable('input'),
      host_dispatch: unavailable('host functions'),
      lsp_make_request: () => {},
      lsp_send_notification: () => {},
      lsp_send_response: (raw) => {
        const response = toPlain(raw);
        const resolve = lspPending.get(response.id);
        if (resolve) {
          lspPending.delete(response.id);
          resolve(response);
        }
      },
      playground_send_notification: (raw) => {
        const notification = toPlain(raw);
        if (notification.type === 'updateProject' && projectId === null) {
          projectId = notification.project;
          resolveProject(notification);
        }
        if (notification.type === 'valueBody') {
          const resolve = valueWaiters.get(notification.valueRefId);
          if (resolve) {
            valueWaiters.delete(notification.valueRefId);
            resolve(notification);
          }
        }
        for (const listener of runListeners) listener(notification);
      },
    },
    vfs.wasmVfs,
  );

  const lspRequest = (method, params) =>
    new Promise((resolve) => {
      const id = nextLspId++;
      lspPending.set(id, resolve);
      runtime.handleLspRequest({ id, method, params });
    });

  await lspRequest('initialize', {
    capabilities: {
      textDocument: {
        publishDiagnostics: { relatedInformation: true },
        synchronization: { didSave: true, dynamicRegistration: true },
      },
      workspace: {},
    },
    processId: null,
    rootUri: `file://${root}`,
    workspaceFolders: [{ name: 'docs-example', uri: `file://${root}` }],
  });
  runtime.handleLspNotification({ method: 'initialized', params: {} });

  let documentVersion = 1;
  for (const [relativePath, text] of Object.entries(files)) {
    if (!relativePath.endsWith('.baml')) continue;
    runtime.handleLspNotification({
      method: 'textDocument/didOpen',
      params: {
        textDocument: {
          languageId: 'baml',
          text,
          uri: `file://${root}/${relativePath}`,
          version: documentVersion++,
        },
      },
    });
  }
  runtime.requestPlaygroundState();
  const project = await projectReady;

  return {
    projectId,
    diagnostics: project?.update?.diagnostics ?? [],
    free: () => runtime.free(),

    async run(functionName, { timeoutMs = 30_000, signal } = {}) {
      const requestId = nextRequestId++;
      let boundaryId = null;
      let settle;
      const terminal = new Promise((resolve) => {
        settle = resolve;
      });

      const listener = (notification) => {
        if (
          notification.type === 'runStarted' &&
          notification.requestId === requestId
        ) {
          boundaryId = notification.run?.boundaryId ?? null;
          if (TERMINAL.has(notification.run?.status)) {
            settle({ outcome: notification.run, boundaryId });
          }
        }
        if (
          notification.type === 'commandError' &&
          notification.requestId === requestId
        ) {
          settle({
            boundaryId,
            outcome: {
              status: 'failed',
              error: {
                class: notification.code,
                message: notification.message,
              },
            },
          });
        }
        for (const change of notification.patch?.changes ?? []) {
          if (change.type === 'complete') {
            settle({
              outcome: change.outcome,
              boundaryId: notification.patch.boundaryId,
            });
          }
        }
      };
      runListeners.add(listener);

      const cancel = () => {
        if (!boundaryId) return;
        try {
          runtime.cancelRun(nextRequestId++, boundaryId);
        } catch {
          // The runtime may have completed between the signal and this call.
        }
      };
      signal?.addEventListener('abort', cancel, { once: true });

      let timer;
      try {
        runtime.startRun(
          requestId,
          projectId,
          functionName,
          new Uint8Array(0),
        );
        const completed = await Promise.race([
          terminal,
          new Promise((resolve) => {
            timer = setTimeout(() => resolve('timeout'), timeoutMs);
          }),
        ]);
        if (completed === 'timeout') {
          cancel();
          throw new RunTimeout(timeoutMs);
        }

        const { boundaryId: completedBoundaryId, outcome } = completed;
        if (outcome?.status !== 'succeeded') {
          return {
            status: outcome?.status ?? 'failed',
            value: null,
            error: outcome?.error ?? null,
          };
        }

        const value = await readRunResult({
          boundaryId: completedBoundaryId,
          outcome,
          readValue: (id, valueRef) =>
            readValue(runtime, valueWaiters, () => nextRequestId++, id, valueRef, timeoutMs),
        });
        return { status: 'succeeded', value, error: null };
      } finally {
        clearTimeout(timer);
        runListeners.delete(listener);
        signal?.removeEventListener('abort', cancel);
      }
    },
  };
}

function readValue(
  runtime,
  valueWaiters,
  nextId,
  boundaryId,
  valueRef,
  timeoutMs,
) {
  return new Promise((resolve) => {
    const timer = setTimeout(() => {
      valueWaiters.delete(valueRef.id);
      resolve(null);
    }, timeoutMs);
    valueWaiters.set(valueRef.id, (value) => {
      clearTimeout(timer);
      resolve(value);
    });
    queueMicrotask(() => runtime.readValue(nextId(), boundaryId, valueRef));
  });
}
