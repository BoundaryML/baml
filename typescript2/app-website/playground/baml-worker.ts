// biome-ignore-all assist/source/organizeImports: Preserve the existing import layout in this legacy worker.
// biome-ignore-all assist/source/useSortedKeys: Preserve the existing wire-message property layout in this legacy worker.
// biome-ignore-all lint/style/noParameterAssign: Preserve the existing JSON-RPC normalization flow in this legacy worker.
// biome-ignore-all lint/suspicious/noExplicitAny: Preserve the existing recursive wire-value conversion in this legacy worker.
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

// RunStore commands carry a requestId and report failures via `commandError`
// (not a thrown promise). Guard the runtime once, here, so each handler stays
// a thin pass-through to the wasm runtime.
function runtimeForCommand(
  requestId: number,
  code: string,
): BamlWasmRuntime | null {
  if (runtime) return runtime;
  postOut({
    type: 'commandError',
    requestId,
    code,
    message: 'WASM runtime is not initialized.',
  });
  return null;
}

// ---------------------------------------------------------------------------
// Env vars
// ---------------------------------------------------------------------------

// Internal env vars the runtime probes optionally — resolve to undefined when
// unset rather than prompting. BOUNDARY_PROXY_URL is controlled solely by the
// gateway toggle: when off it's simply absent (no proxy), and a missing-key
// popup for it would make no sense. Every other unset var still opens the popup.
const OPTIONAL_INTERNAL_ENV = new Set<string>(['BOUNDARY_PROXY_URL']);

function resolveEnv(variable: string): Promise<string | undefined> {
  if (variable in envVars) return Promise.resolve(envVars[variable]);
  if (OPTIONAL_INTERNAL_ENV.has(variable)) return Promise.resolve(undefined);
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
    callId,
    entry: {
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
        ...(notification.requestId !== undefined
          ? { requestId: notification.requestId }
          : {}),
      });
      break;
    case 'cursorContext':
      postOut({
        type: 'cursorContext',
        context: notification.context,
      });
      break;
    // RunStore protocol: the runtime owns run state and pushes snapshots +
    // patches. Unwrap each to a top-level WorkerOutMessage so the panel's
    // RunStoreClient (over WorkerRuntimePort) consumes them. The wasm-generated
    // types use `Value` for run/patch/snapshot; the cast lands them on the
    // narrower worker-protocol types (runtime values are always valid).
    case 'runStarted':
      postOut({
        type: 'runStarted',
        requestId: notification.requestId,
        run: notification.run,
      } as WorkerOutMessage);
      break;
    case 'runPatch':
      postOut({
        type: 'runPatch',
        patch: notification.patch,
      } as WorkerOutMessage);
      break;
    case 'runSnapshot':
      postOut({
        type: 'runSnapshot',
        requestId: notification.requestId,
        boundaryId: notification.boundaryId,
        snapshot: notification.snapshot,
      } as WorkerOutMessage);
      break;
    case 'runList':
      postOut({
        type: 'runList',
        requestId: notification.requestId,
        runs: notification.runs,
      } as WorkerOutMessage);
      break;
    case 'historyList':
      postOut({
        type: 'historyList',
        requestId: notification.requestId,
        runs: notification.runs,
      } as WorkerOutMessage);
      break;
    case 'valueBody':
      postOut({
        type: 'valueBody',
        requestId: notification.requestId,
        boundaryId: notification.boundaryId,
        valueRefId: notification.valueRefId,
        codec: notification.codec,
        availability: notification.availability,
        bodyBase64: notification.bodyBase64,
        diagnostic: notification.diagnostic,
      } as WorkerOutMessage);
      break;
    case 'runCursorExpired':
      postOut({
        type: 'runCursorExpired',
        requestId: notification.requestId,
        subscriptionId: notification.subscriptionId,
        boundaryId: notification.boundaryId,
        reason: notification.reason,
      } as WorkerOutMessage);
      break;
    case 'commandAck':
      postOut({
        type: 'commandAck',
        requestId: notification.requestId,
        outcome: notification.outcome,
      });
      break;
    case 'commandError':
      postOut({
        type: 'commandError',
        requestId: notification.requestId,
        code: notification.code,
        message: notification.message,
      });
      break;
    case 'profileArtifactChunk':
      postOut(notification as WorkerOutMessage);
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
      // Only open BAML sources as language documents. baml.toml lives in the
      // vfs (above) for project config, but didOpen-ing it as `languageId: baml`
      // makes the parser read TOML as BAML — "Expected top-level declaration"
      // errors that mark the project as having diagnostics, which makes the
      // runtime skip building the bex ("engine not ready" at run time).
      if (!rel.endsWith('.baml')) continue;
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
    // ── RunStore protocol: execution lives in the runtime, results stream
    //    back as runStarted/runPatch notifications (see onPlaygroundNotification).
    case 'startRun': {
      const rt = runtimeForCommand(msg.requestId, 'wasmRuntimeNotReady');
      if (!rt) return;
      try {
        rt.startRun(msg.requestId, msg.project, msg.functionName, msg.argsBytes);
      } catch (e) {
        postOut({
          type: 'commandError',
          requestId: msg.requestId,
          code: 'wasmStartRunFailed',
          message: e instanceof Error ? e.message : String(e),
        });
      }
      return;
    }

    case 'startPreviewRun': {
      const rt = runtimeForCommand(msg.requestId, 'wasmRuntimeNotReady');
      if (!rt) return;
      try {
        rt.startPreviewRun(
          msg.requestId,
          msg.project,
          msg.parentFunctionName,
          msg.helper,
          msg.functionName,
          msg.argsBytes,
        );
      } catch (e) {
        postOut({
          type: 'commandError',
          requestId: msg.requestId,
          code: 'wasmStartPreviewRunFailed',
          message: e instanceof Error ? e.message : String(e),
        });
      }
      return;
    }

    case 'startTestRun': {
      const rt = runtimeForCommand(msg.requestId, 'wasmRuntimeNotReady');
      if (!rt) return;
      try {
        rt.startTestRun(
          msg.requestId,
          msg.project,
          msg.generation,
          msg.testName,
        );
      } catch (e) {
        postOut({
          type: 'commandError',
          requestId: msg.requestId,
          code: 'wasmStartTestRunFailed',
          message: e instanceof Error ? e.message : String(e),
        });
      }
      return;
    }

    case 'cancelRun': {
      const rt = runtimeForCommand(msg.requestId, 'wasmRuntimeNotReady');
      if (!rt) return;
      try {
        rt.cancelRun(msg.requestId, msg.boundaryId);
      } catch (e) {
        postOut({
          type: 'commandError',
          requestId: msg.requestId,
          code: 'wasmCancelRunFailed',
          message: e instanceof Error ? e.message : String(e),
        });
      }
      return;
    }

    case 'respondToInput': {
      const rt = runtimeForCommand(msg.requestId, 'wasmRuntimeNotReady');
      if (!rt) return;
      try {
        rt.respondToInput(msg.requestId, msg.boundaryId, msg.inputRequestId);
      } catch (e) {
        postOut({
          type: 'commandError',
          requestId: msg.requestId,
          code: 'wasmRespondToInputFailed',
          message: e instanceof Error ? e.message : String(e),
        });
      }
      return;
    }

    case 'respondToEnv': {
      const rt = runtimeForCommand(msg.requestId, 'wasmRuntimeNotReady');
      if (!rt) return;
      try {
        const outcome = rt.respondToEnv(
          msg.requestId,
          msg.boundaryId,
          msg.envRequestId,
          msg.value,
        );
        // The runtime's `env` callback (resolveEnv) posts envVarRequest with a
        // numeric id; the panel echoes it back as envRequestId. Resolve that
        // pending promise so an in-flight run unblocks.
        if (outcome === 'accepted') {
          const id = Number(msg.envRequestId);
          const resolve = Number.isFinite(id)
            ? pendingEnvResolvers.get(id)
            : undefined;
          if (resolve) {
            pendingEnvResolvers.delete(id);
            resolve(msg.value);
          }
        }
      } catch (e) {
        postOut({
          type: 'commandError',
          requestId: msg.requestId,
          code: 'wasmRespondToEnvFailed',
          message: e instanceof Error ? e.message : String(e),
        });
      }
      return;
    }

    case 'listRuns': {
      const rt = runtimeForCommand(msg.requestId, 'wasmRuntimeNotReady');
      if (!rt) return;
      try {
        rt.listRuns(msg.requestId, msg.filter);
      } catch (e) {
        postOut({
          type: 'commandError',
          requestId: msg.requestId,
          code: 'wasmListRunsFailed',
          message: e instanceof Error ? e.message : String(e),
        });
      }
      return;
    }

    case 'snapshot': {
      const rt = runtimeForCommand(msg.requestId, 'wasmRuntimeNotReady');
      if (!rt) return;
      try {
        rt.snapshot(msg.requestId, msg.boundaryId);
      } catch (e) {
        postOut({
          type: 'commandError',
          requestId: msg.requestId,
          code: 'wasmSnapshotFailed',
          message: e instanceof Error ? e.message : String(e),
        });
      }
      return;
    }

    case 'subscribe': {
      const rt = runtimeForCommand(msg.requestId, 'wasmRuntimeNotReady');
      if (!rt) return;
      try {
        rt.subscribe(
          msg.requestId,
          msg.subscriptionId,
          msg.boundaryId,
          msg.afterCursor == null ? undefined : BigInt(msg.afterCursor),
        );
      } catch (e) {
        postOut({
          type: 'commandError',
          requestId: msg.requestId,
          code: 'wasmSubscribeFailed',
          message: e instanceof Error ? e.message : String(e),
        });
      }
      return;
    }

    case 'unsubscribe': {
      const rt = runtimeForCommand(msg.requestId, 'wasmRuntimeNotReady');
      if (!rt) return;
      try {
        rt.unsubscribe(msg.requestId, msg.subscriptionId);
      } catch (e) {
        postOut({
          type: 'commandError',
          requestId: msg.requestId,
          code: 'wasmUnsubscribeFailed',
          message: e instanceof Error ? e.message : String(e),
        });
      }
      return;
    }

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
      runtime?.requestControlFlowGraph(
        msg.project,
        msg.functionName,
        msg.requestId,
      );
      return;

    case 'cursorPosition':
      runtime?.handleCursorPosition(msg.file, msg.line, msg.column);
      return;

    case 'requestCollectTests':
      runtime?.requestCollectTests(msg.project);
      return;

    case 'expandTestSet':
      runtime?.expandTestSet(msg.project, msg.generation, msg.testsetName);
      return;

    case 'listHistory': {
      const rt = runtimeForCommand(msg.requestId, 'wasmRuntimeNotReady');
      if (!rt) return;
      try {
        rt.listHistory(msg.requestId, msg.filter);
      } catch (e) {
        postOut({
          type: 'commandError',
          requestId: msg.requestId,
          code: 'wasmListHistoryFailed',
          message: e instanceof Error ? e.message : String(e),
        });
      }
      return;
    }

    case 'openHistory': {
      const rt = runtimeForCommand(msg.requestId, 'wasmRuntimeNotReady');
      if (!rt) return;
      try {
        rt.openHistory(msg.requestId, msg.boundaryId);
      } catch (e) {
        postOut({
          type: 'commandError',
          requestId: msg.requestId,
          code: 'wasmOpenHistoryFailed',
          message: e instanceof Error ? e.message : String(e),
        });
      }
      return;
    }

    case 'readValue': {
      const rt = runtimeForCommand(msg.requestId, 'wasmRuntimeNotReady');
      if (!rt) return;
      try {
        rt.readValue(msg.requestId, msg.boundaryId, msg.valueRef);
      } catch (e) {
        postOut({
          type: 'commandError',
          requestId: msg.requestId,
          code: 'wasmReadValueFailed',
          message: e instanceof Error ? e.message : String(e),
        });
      }
      return;
    }

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
