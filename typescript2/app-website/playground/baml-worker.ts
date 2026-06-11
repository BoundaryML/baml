/**
 * BAML Worker for the marketing-site playground.
 *
 * Runs in a Web Worker. Owns the single BamlWasmRuntime instance.
 * Speaks the WorkerInMessage / WorkerOutMessage protocol from
 * @b/pkg-playground so it can drive an `<ExecutionPanel>` via
 * `WorkerRuntimePort`.
 *
 * Adapted from app-promptfiddle/src/playground/baml-lsp-worker.ts:
 * the website does not use Monaco / vscode-languageclient, so the LSP
 * MessagePort path is removed and the runtime's LSP handshake is
 * driven directly with `runtime.handleLspRequest` / `handleLspNotification`.
 */

/// <reference lib="WebWorker" />

import initWasm, {
  BamlWasmRuntime,
  BamlHandle,
  type LspResponse,
  type PlaygroundNotification,
  start as setupLogger,
  getBuildTime,
} from '@b/bridge_wasm';

import type {
  WorkerOutMessage,
  WorkerInMessage,
  PlaygroundNotification as WorkerPlaygroundNotification,
} from '@b/pkg-playground';

import { decodeCallResult } from '@b/pkg-proto';

import { BamlVfs } from './vfs';

declare const self: DedicatedWorkerGlobalScope;

// ---------------------------------------------------------------------------
// Init payload — analogous to WorkerInitMessage but without a MessagePort.
// ---------------------------------------------------------------------------

interface WebsiteInitMessage {
  type: 'init';
  initialFiles: Record<string, string>;
  rootPath: string;
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

let disposed = false;
let runtime: BamlWasmRuntime | null = null;
let vfs: BamlVfs = new BamlVfs('/workspace');
let rootPath = '/workspace';

const liveHandles = new Map<number, BamlHandle[]>();

const envVars: Record<string, string> = {};
let nextEnvReqId = 0;
const pendingEnvResolvers = new Map<number, (v: string | undefined) => void>();

const requestPromises = new Map<
  number | string,
  (response: LspResponse) => void
>();
let nextLspReqId = 0;

let nextDocVersion = 1;

function dispose(): void {
  if (disposed) return;
  disposed = true;
  for (const resolve of pendingEnvResolvers.values()) {
    resolve(undefined);
  }
  pendingEnvResolvers.clear();
  if (runtime) {
    runtime.free();
    runtime = null;
  }
}

function postOut(msg: WorkerOutMessage, transfer?: Transferable[]): void {
  if (transfer) {
    self.postMessage(msg, transfer);
  } else {
    self.postMessage(msg);
  }
}

// ---------------------------------------------------------------------------
// Env vars
// ---------------------------------------------------------------------------

function resolveEnv(variable: string): Promise<string | undefined> {
  if (variable in envVars) return Promise.resolve(envVars[variable]);
  return new Promise<string | undefined>((resolve) => {
    const id = nextEnvReqId++;
    pendingEnvResolvers.set(id, resolve);
    postOut({ type: 'envVarRequest', id, variable });
  });
}

// ---------------------------------------------------------------------------
// Fetch logging (proxied to main thread for the UI inspector)
// ---------------------------------------------------------------------------

let nextLogId = 0;

async function loggingFetch(
  callId: number,
  method: string,
  url: string,
  headersJson: string,
  body: string,
): Promise<{
  status: number;
  headersJson: string;
  url: string;
  bodyPromise: Promise<string>;
}> {
  const logId = nextLogId++;
  let parsedHeaders: Record<string, string> = {};
  try {
    parsedHeaders = JSON.parse(headersJson);
  } catch {}

  postOut({
    type: 'fetchLogNew',
    entry: {
      callId,
      id: logId,
      timestamp: Date.now(),
      method,
      url,
      requestHeaders: parsedHeaders,
      requestBody: body,
      status: null,
      responseBody: null,
      error: null,
      durationMs: null,
      responseHeaders: null,
    },
  });

  const start = performance.now();

  try {
    const response = await fetch(url, {
      method,
      headers: parsedHeaders,
      body: method !== 'GET' && method !== 'HEAD' ? body : undefined,
    });

    const elapsed = Math.round(performance.now() - start);
    const responseHeaders: Record<string, string> = {};
    response.headers.forEach((v, k) => {
      responseHeaders[k] = v;
    });

    const bodyText = response.text();

    postOut({
      type: 'fetchLogUpdate',
      logId,
      patch: { status: response.status, durationMs: elapsed, responseHeaders },
    });

    bodyText.then(
      (text) =>
        postOut({
          type: 'fetchLogUpdate',
          logId,
          patch: { responseBody: text },
        }),
      (err) =>
        postOut({
          type: 'fetchLogUpdate',
          logId,
          patch: { error: `Body read error: ${err}` },
        }),
    );

    return {
      status: response.status,
      headersJson: JSON.stringify(responseHeaders),
      url: response.url,
      bodyPromise: bodyText,
    };
  } catch (err) {
    const elapsed = Math.round(performance.now() - start);
    const msg = err instanceof Error ? err.message : String(err);
    postOut({
      type: 'fetchLogUpdate',
      logId,
      patch: { status: 0, error: msg, durationMs: elapsed },
    });

    return {
      status: 0,
      headersJson: '{}',
      url,
      bodyPromise: Promise.resolve(''),
    };
  }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function mapsToRecordsDeep<T>(input: T): T {
  if (input instanceof Map) {
    const obj: Record<string, any> = {};
    for (const [key, value] of input.entries()) {
      obj[String(key)] = mapsToRecordsDeep(value);
    }
    return obj as T;
  }
  if (Array.isArray(input)) {
    return input.map(mapsToRecordsDeep) as T;
  }
  if (input !== null && typeof input === 'object') {
    const obj: Record<string, any> = {};
    for (const [key, value] of Object.entries(input)) {
      obj[key] = mapsToRecordsDeep(value);
    }
    return obj as T;
  }
  return input;
}

/** Project we've auto-triggered test collection for, so we don't loop. */
let autoCollectedFor: string | null = null;

function onPlaygroundNotification(notification: PlaygroundNotification): void {
  switch (notification.type) {
    case 'controlFlowGraphResult':
      postOut({
        type: 'controlFlowGraphResult',
        functionName: notification.functionName,
        graph: notification.graph ?? null,
      });
      break;
    case 'cursorContext':
      postOut({
        type: 'cursorContext',
        context: notification.context,
      });
      break;
    default:
      postOut({
        type: 'playgroundNotification',
        notification: notification as unknown as WorkerPlaygroundNotification,
      });
      // Auto-collect tests once per project so the Tests tree populates
      // without the user having to click the refresh wrench. Deferred via
      // queueMicrotask because this callback fires while the runtime mutex
      // is held — a synchronous re-entry would panic with "cannot
      // recursively acquire mutex".
      if (
        notification.type === 'updateProject' &&
        autoCollectedFor !== notification.project
      ) {
        autoCollectedFor = notification.project;
        const project = notification.project;
        queueMicrotask(() => {
          try {
            runtime?.requestCollectTests(project);
          } catch {}
        });
      }
  }
}

function sendLspRequest(method: string, params: any): Promise<LspResponse> {
  return new Promise((resolve) => {
    const id = nextLspReqId++;
    requestPromises.set(id, resolve);
    runtime!.handleLspRequest({ id, method, params });
  });
}

function fileUri(rel: string): string {
  const root = rootPath.endsWith('/') ? rootPath.slice(0, -1) : rootPath;
  return `file://${root}/${rel}`;
}

// ---------------------------------------------------------------------------
// Worker entry point
// ---------------------------------------------------------------------------

self.onmessage = async (event: MessageEvent) => {
  if (disposed) return;
  const data = event.data;

  // ── Init message (no LSP MessagePort) ────────────────────────────────────
  if (data && data.type === 'init') {
    const init = data as WebsiteInitMessage;
    rootPath = init.rootPath || '/workspace';
    vfs = new BamlVfs(rootPath);
    if (init.initialFiles) vfs.setFiles(init.initialFiles);

    vfs.onChange = (change) => {
      if ('deleted' in change && change.deleted) {
        postOut({ type: 'vfsFileDeleted', path: change.path });
      } else {
        postOut({
          type: 'vfsFileChanged',
          path: change.path,
          content: change.content,
        });
      }
    };

    await initWasm();
    await setupLogger();

    // The marketing-site playground is read-only-ish: we don't expose
    // `read_input`, shell `exec`, or `$shell` to BAML programs running here.
    // Stub them so the runtime fails fast and visibly if anyone uses them.
    const notSupported = (what: string) => {
      throw new Error(`${what} is not available in the website playground`);
    };

    runtime = BamlWasmRuntime.create(
      {
        fetch: loggingFetch,
        env: resolveEnv,
        input: () => notSupported('read_input'),
        exec: async () => notSupported('exec'),
        shell: async () => notSupported('shell'),
        lsp_send_notification: (n: unknown) => {
          // The marketing playground ignores LSP notifications, but custom
          // editors (the learn2 Monaco) want positioned diagnostics. Forward
          // publishDiagnostics so they can render squiggles + ErrorLens.
          const note = mapsToRecordsDeep(n) as {
            method?: string;
            params?: unknown;
          };
          if (note?.method === 'textDocument/publishDiagnostics') {
            self.postMessage({ type: 'lspDiagnostics', params: note.params });
          }
        },
        lsp_send_response: (response: LspResponse) => {
          response = mapsToRecordsDeep(response);
          const resolver = requestPromises.get(response.id);
          if (resolver) {
            requestPromises.delete(response.id);
            resolver(response);
          }
        },
        lsp_make_request: () => {},
        playground_send_notification: (
          notification: PlaygroundNotification,
        ) => {
          notification = mapsToRecordsDeep(notification);
          onPlaygroundNotification(notification);
        },
        // Throwing (vs a silent no-op) lets the runtime complete the host
        // call with an error instead of hanging the VM on it forever.
        host_dispatch: () => notSupported('host functions'),
      },
      vfs.wasmVfs,
    );

    // Drive the LSP handshake the runtime expects, without a real LSP client.
    await sendLspRequest('initialize', {
      processId: null,
      rootUri: `file://${rootPath}`,
      // Declare the client capabilities a real LSP client (VSCode /
      // monaco-languageclient) sends — the runtime tailors completion / inlay
      // hints / hover to these, and omitting them yields empty/degraded results.
      capabilities: {
        textDocument: {
          synchronization: { dynamicRegistration: true, didSave: true },
          completion: {
            dynamicRegistration: true,
            contextSupport: true,
            completionItem: {
              snippetSupport: true,
              documentationFormat: ['markdown', 'plaintext'],
              resolveSupport: { properties: ['documentation', 'detail'] },
            },
          },
          hover: {
            dynamicRegistration: true,
            contentFormat: ['markdown', 'plaintext'],
          },
          inlayHint: { dynamicRegistration: true },
          codeLens: { dynamicRegistration: true },
          publishDiagnostics: { relatedInformation: true },
        },
        workspace: {
          inlayHint: { refreshSupport: true },
          codeLens: { refreshSupport: true },
        },
      },
      workspaceFolders: [{ uri: `file://${rootPath}`, name: 'workspace' }],
    });
    runtime.handleLspNotification({ method: 'initialized', params: {} });

    for (const [rel, content] of Object.entries(init.initialFiles || {})) {
      runtime.handleLspNotification({
        method: 'textDocument/didOpen',
        params: {
          textDocument: {
            uri: fileUri(rel),
            languageId: 'baml',
            version: nextDocVersion++,
            text: content,
          },
        },
      });
    }

    runtime.requestPlaygroundState();

    postOut({ type: 'ready' });
    postOut({ type: 'buildTime', value: getBuildTime() });
    return;
  }

  // ── openFiles: add + didOpen new files after init (multi-editor) ──────────
  // Lets independent editors register their own project (baml.toml + main.baml)
  // into the shared worker dynamically. Handled before the typed switch so we
  // don't need to extend the shared WorkerInMessage union.
  if (data && data.type === 'openFiles') {
    const files = (data.files ?? {}) as Record<string, string>;
    vfs.setFiles(files);
    for (const [rel, content] of Object.entries(files)) {
      // didOpen triggers a full project refresh (builds the bex + fires
      // updateProject/diagnostics + auto-collect). Do NOT also call
      // requestPlaygroundState here — with several editors registering at once
      // it races the async re-eval and leaves earlier projects without a bex.
      runtime?.handleLspNotification({
        method: 'textDocument/didOpen',
        params: {
          textDocument: {
            uri: fileUri(rel),
            languageId: 'baml',
            version: nextDocVersion++,
            text: content,
          },
        },
      });
    }
    return;
  }

  // ── requestCodeLens: forward textDocument/codeLens for a file ─────────────
  if (data && data.type === 'requestCodeLens') {
    const uri = data.uri as string;
    const reqId = data.reqId;
    try {
      const resp = await sendLspRequest('textDocument/codeLens', {
        textDocument: { uri },
      });
      self.postMessage({
        type: 'codeLensResult',
        reqId,
        uri,
        lenses: resp.result ?? [],
      });
    } catch {
      self.postMessage({ type: 'codeLensResult', reqId, uri, lenses: [] });
    }
    return;
  }

  // ── lspRequest: generic LSP request passthrough (hover, inlayHint, …) ─────
  if (data && data.type === 'lspRequest') {
    const reqId = data.reqId;
    try {
      const resp = await sendLspRequest(data.method, data.params);
      self.postMessage({
        type: 'lspResult',
        reqId,
        result: resp.result ?? null,
      });
    } catch {
      self.postMessage({ type: 'lspResult', reqId, result: null });
    }
    return;
  }

  // ── Custom RPC messages ──────────────────────────────────────────────────

  const msg = data as WorkerInMessage;

  switch (msg.type) {
    case 'nextFunctionCall': {
      if (!runtime) {
        postOut({
          type: 'nextFunctionCallError',
          id: msg.id,
          error: 'Runtime not initialized',
        });
        return;
      }
      try {
        postOut({
          type: 'nextFunctionCallResult',
          id: msg.id,
          callId: (
            runtime as unknown as { nextFunctionCall(): number }
          ).nextFunctionCall(),
        });
      } catch (e) {
        postOut({
          type: 'nextFunctionCallError',
          id: msg.id,
          error: e instanceof Error ? e.message : String(e),
        });
      }
      return;
    }

    case 'callFunction': {
      if (!runtime) {
        postOut({
          type: 'callFunctionError',
          id: msg.id,
          error: 'Runtime not initialized',
        });
        return;
      }
      try {
        const resultBytes = await runtime.callFunction(
          msg.id,
          msg.project,
          msg.name,
          msg.argsProto,
        );
        const bytes = new Uint8Array(resultBytes);
        const handles: BamlHandle[] = [];
        // Return a plain handle descriptor (postMessage-cloneable) instead of
        // the BamlHandle WASM object. We keep the BamlHandle alive in
        // liveHandles so its Drop fires when the run is cleared.
        const decoded = decodeCallResult(bytes, (key, handleType, typeName) => {
          const h = new BamlHandle(key.toString(), handleType);
          handles.push(h);
          return {
            handle_key: key,
            handle_type: handleType,
            type_name: typeName,
          };
        });
        const existing = liveHandles.get(msg.id);
        if (existing) for (const h of existing) h.free();
        if (handles.length > 0) liveHandles.set(msg.id, handles);
        else liveHandles.delete(msg.id);
        // Send the decoded BamlJsValue directly — canary's worker-protocol
        // expects `result: BamlJsValue<PlainHandleDescriptor>`, not a string.
        postOut({ type: 'callFunctionResult', id: msg.id, result: decoded });
      } catch (e) {
        const isCancelled =
          e instanceof Error && (e as any).name === 'BamlCancelledError';
        postOut({
          type: 'callFunctionError',
          id: msg.id,
          error: e instanceof Error ? e.message : String(e),
          cancelled: isCancelled || undefined,
        });
      }
      return;
    }

    case 'cancelCall':
      try {
        runtime?.cancelCall(msg.project, msg.id);
      } catch {}
      return;

    case 'envVarResponse': {
      const resolve = pendingEnvResolvers.get(msg.id);
      if (resolve) {
        pendingEnvResolvers.delete(msg.id);
        if (msg.value !== undefined && msg.variable) {
          envVars[msg.variable] = msg.value;
        }
        resolve(msg.value);
      }
      return;
    }

    case 'setEnvVar':
      envVars[msg.key] = msg.value;
      return;

    case 'deleteEnvVar':
      delete envVars[msg.key];
      return;

    case 'filesChanged': {
      vfs.setFiles(msg.files);
      const version = nextDocVersion++;
      for (const [rel, content] of Object.entries(msg.files)) {
        runtime?.handleLspNotification({
          method: 'textDocument/didChange',
          params: {
            textDocument: { uri: fileUri(rel), version },
            contentChanges: [{ text: content }],
          },
        });
      }
      // Do NOT call runtime.requestPlaygroundState() here — didChange already
      // schedules a project re-eval that fires updateProject asynchronously.
      // Calling state explicitly races with that async work and can panic
      // ("cannot recursively acquire mutex" / OOB memory) when keystrokes
      // arrive faster than the runtime can drain.
      return;
    }

    case 'selectProject':
      return;

    case 'requestState':
      runtime?.requestPlaygroundState();
      postOut({ type: 'buildTime', value: getBuildTime() });
      return;

    case 'requestControlFlowGraph':
      runtime?.requestControlFlowGraph(msg.project, msg.functionName);
      return;

    case 'cursorPosition':
      runtime?.handleCursorPosition(msg.file, msg.line, msg.column);
      return;

    case 'requestCollectTests':
      runtime?.requestCollectTests(msg.project);
      return;

    case 'callTestFunction': {
      if (!runtime) {
        postOut({
          type: 'callFunctionError',
          id: msg.id,
          error: 'Runtime not initialized',
        });
        return;
      }
      try {
        const resultBytes = await runtime.callTestFunction(
          msg.id,
          msg.project,
          msg.generation,
          msg.testName,
        );
        const bytes = new Uint8Array(resultBytes);
        const handles: BamlHandle[] = [];
        const decoded = decodeCallResult(bytes, (key, handleType, typeName) => {
          const h = new BamlHandle(key.toString(), handleType);
          handles.push(h);
          return {
            handle_key: key,
            handle_type: handleType,
            type_name: typeName,
          };
        });
        const existing = liveHandles.get(msg.id);
        if (existing) for (const h of existing) h.free();
        if (handles.length > 0) liveHandles.set(msg.id, handles);
        else liveHandles.delete(msg.id);
        postOut({ type: 'callFunctionResult', id: msg.id, result: decoded });
      } catch (e) {
        const isCancelled =
          e instanceof Error && (e as any).name === 'BamlCancelledError';
        postOut({
          type: 'callFunctionError',
          id: msg.id,
          error: e instanceof Error ? e.message : String(e),
          cancelled: isCancelled || undefined,
        });
      }
      return;
    }

    case 'expandTestSet':
      runtime?.expandTestSet(msg.project, msg.generation, msg.testsetName);
      return;

    case 'clearHandles':
      for (const id of msg.runIds) {
        const handles = liveHandles.get(id);
        if (handles) for (const h of handles) h.free();
        liveHandles.delete(id);
      }
      return;

    case 'dispose':
      dispose();
      return;

    case 'inputResponse':
      // Website playground does not request input from the user — stub the
      // response handler so the protocol union is exhaustive.
      return;
  }

  msg satisfies never;
};
