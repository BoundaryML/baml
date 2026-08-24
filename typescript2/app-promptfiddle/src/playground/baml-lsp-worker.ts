// biome-ignore-all lint/style/noParameterAssign: Preserve the existing JSON-RPC normalization flow in this legacy worker.
// biome-ignore-all lint/suspicious/noExplicitAny: Preserve the existing recursive wire-value conversion in this legacy worker.
/**
 * BAML Unified Worker
 *
 * Runs in a Web Worker. Owns the single BamlWasmRuntime instance.
 * Two communication channels:
 *
 *   Channel 1 — MessagePort (LSP JSON-RPC):
 *     Handles Monaco language features (hover, completions, diagnostics,
 *     go-to-definition, references) via the LSP protocol.
 *
 *   Channel 2 — postMessage (custom RPC):
 *     Handles function execution, function names, diagnostics text,
 *     fetch logs, and env var requests.
 */

/// <reference lib="WebWorker" />

import {
  getWasmError,
  installWasmPanicHook,
  isWasmPanic,
  onWasmPanic,
} from '@b/pkg-playground/wasm-panic';
import {
  BrowserMessageReader,
  BrowserMessageWriter,
  type Connection,
  createConnection,
} from 'vscode-languageserver/browser.js';

// Install panic hook before any WASM code runs
installWasmPanicHook();

// Register callback to notify main thread of any WASM panic
onWasmPanic((message) => {
  self.postMessage({ message, type: 'wasmPanic' });
});

import initWasm, {
  BamlWasmRuntime,
  version as bamlVersion,
  commitHash,
  getBuildTime,
  type LspNotification,
  type LspRequest,
  type LspResponse,
  type PlaygroundNotification,
  start as setupLogger,
} from '@b/bridge_wasm';

import type {
  WorkerInitMessage,
  WorkerInMessage,
  WorkerOutMessage,
  PlaygroundNotification as WorkerPlaygroundNotification,
} from '@b/pkg-playground';
import { Bash, InMemoryFs, MountableFs } from 'just-bash/browser';
import { BamlVfsAdapter } from './baml-vfs-adapter';
import { BamlVfs } from './vfs';

declare const self: DedicatedWorkerGlobalScope;

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

let disposed = false;

function dispose(): void {
  if (disposed) return;
  disposed = true;
  // Resolve any pending env requests so awaiting callers don't hang
  for (const resolve of pendingEnvResolvers.values()) {
    resolve(undefined);
  }
  pendingEnvResolvers.clear();
  // Resolve any pending input requests with empty string so awaiting callers don't hang
  for (const entry of pendingInputResolvers.values()) {
    entry.resolve('');
  }
  pendingInputResolvers.clear();
  if (connection) {
    connection.dispose();
    connection = null;
  }
  if (runtime) {
    runtime.free();
    runtime = null;
  }
}

let connection: Connection | null = null;
let runtime: BamlWasmRuntime | null = null;
let vfs: BamlVfs = new BamlVfs('/workspace');

// ---------------------------------------------------------------------------
// Env vars (worker-side store)
// ---------------------------------------------------------------------------

const envVars: Record<string, string> = {};
let nextEnvReqId = 1;
const pendingEnvResolvers = new Map<number, (v: string | undefined) => void>();

// Internal env vars the runtime probes optionally — resolve to undefined when
// unset rather than prompting. BOUNDARY_PROXY_URL is controlled solely by the
// gateway toggle: when off it's simply absent (no proxy), and a missing-key
// popup for it would make no sense. Every other unset var still opens the popup.
const OPTIONAL_INTERNAL_ENV = new Set<string>(['BOUNDARY_PROXY_URL']);

function resolveEnv(
  variable: string,
  requestId?: number,
): Promise<string | undefined> {
  if (variable in envVars) return Promise.resolve(envVars[variable]);
  if (OPTIONAL_INTERNAL_ENV.has(variable)) return Promise.resolve(undefined);
  return new Promise<string | undefined>((resolve) => {
    const id = requestId ?? nextEnvReqId++;
    pendingEnvResolvers.set(id, resolve);
    postOut({ id, type: 'envVarRequest', variable });
  });
}

let nextInputReqId = 1;
const pendingInputResolvers = new Map<
  number,
  { callId: number; resolve: (value: string) => void }
>();

function resolveInput(
  requestId: number,
  prompt: string | undefined,
): Promise<string> {
  return new Promise<string>((resolve) => {
    const id = requestId ?? nextInputReqId++;
    pendingInputResolvers.set(id, { callId: id, resolve });
    postOut({ callId: id, id, prompt, type: 'inputRequest' });
  });
}

// ---------------------------------------------------------------------------
// Shell callbacks (just-bash powered)
// ---------------------------------------------------------------------------

/** Shared Bash instance, created lazily on first use. */
let bashInstance: Bash | null = null;

function getOrCreateBash(): Bash {
  if (!bashInstance) {
    const base = new InMemoryFs();
    const mountable = new MountableFs({ base });
    mountable.mount('/workspace', new BamlVfsAdapter(vfs.wasmVfs));
    bashInstance = new Bash({ cwd: '/workspace', fs: mountable });
  }
  return bashInstance;
}

/** Shell result shape expected by the Rust WASM bridge. */
interface ShellResult {
  stdout: string;
  stderr: string;
  exit_code: number;
  stdout_bytes: Uint8Array;
  stderr_bytes: Uint8Array;
}

/** Options passed from Rust as a JSON string. */
interface ProcessOptionsJson {
  cwd?: string;
  env?: Record<string, string>;
  timeout_ms?: number;
  stdin?: string;
}

async function executeShell(
  command: string,
  optionsJson: string | undefined,
): Promise<ShellResult> {
  const bash = getOrCreateBash();
  const options: ProcessOptionsJson | undefined = optionsJson
    ? (JSON.parse(optionsJson) as ProcessOptionsJson)
    : undefined;

  const execOptions: Parameters<Bash['exec']>[1] = {
    cwd: options?.cwd,
    env: options?.env,
    stdin: options?.stdin,
    ...(options?.env ? { replaceEnv: false } : {}),
    ...(options?.timeout_ms != null
      ? { signal: AbortSignal.timeout(options.timeout_ms) }
      : {}),
  };

  const result = await bash.exec(command, execOptions);
  const encoder = new TextEncoder();
  return {
    exit_code: result.exitCode,
    stderr: result.stderr,
    stderr_bytes: encoder.encode(result.stderr),
    stdout: result.stdout,
    stdout_bytes: encoder.encode(result.stdout),
  };
}

async function executeExec(
  program: string,
  args: string[] | undefined,
  optionsJson: string | undefined,
): Promise<ShellResult> {
  const bash = getOrCreateBash();
  const options: ProcessOptionsJson | undefined = optionsJson
    ? (JSON.parse(optionsJson) as ProcessOptionsJson)
    : undefined;

  // Build the command line: program + args joined. just-bash executes this
  // as a shell script so we must quote args to prevent shell splitting.
  const quotedArgs = (args ?? [])
    .map((a) => `'${a.replace(/'/g, "'\\''")}'`)
    .join(' ');
  const commandLine = quotedArgs ? `${program} ${quotedArgs}` : program;

  const execOptions: Parameters<Bash['exec']>[1] = {
    cwd: options?.cwd,
    env: options?.env,
    stdin: options?.stdin,
    ...(options?.env ? { replaceEnv: false } : {}),
    ...(options?.timeout_ms != null
      ? { signal: AbortSignal.timeout(options.timeout_ms) }
      : {}),
  };

  const result = await bash.exec(commandLine, execOptions);
  const encoder = new TextEncoder();
  return {
    exit_code: result.exitCode,
    stderr: result.stderr,
    stderr_bytes: encoder.encode(result.stderr),
    stdout: result.stdout,
    stdout_bytes: encoder.encode(result.stdout),
  };
}

/** Clear all decorations and notify the main thread. */
function clearLogDecorations(): void {
  postOut({ type: 'clearLogDecorations' });
}

// ---------------------------------------------------------------------------
// Typed postMessage helper
// ---------------------------------------------------------------------------

function postOut(msg: WorkerOutMessage, transfer?: Transferable[]): void {
  if (transfer) {
    self.postMessage(msg, transfer);
  } else {
    self.postMessage(msg);
  }
}

function runtimeForCommand(
  requestId: number,
  code: string,
): BamlWasmRuntime | null {
  if (runtime) return runtime;
  postOut({
    code,
    message: 'WASM runtime is not initialized.',
    requestId,
    type: 'commandError',
  });
  return null;
}

// ---------------------------------------------------------------------------
// Fetch logging (proxied to main thread for UI)
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
    callId,
    entry: {
      durationMs: null,
      error: null,
      id: logId,
      method,
      requestBody: body,
      requestHeaders: parsedHeaders,
      responseBody: null,
      responseHeaders: null,
      status: null,
      timestamp: Date.now(),
      url,
    },
    type: 'fetchLogNew',
  });

  const start = performance.now();

  try {
    const response = await fetch(url, {
      body: method !== 'GET' && method !== 'HEAD' ? body : undefined,
      headers: parsedHeaders,
      method,
    });

    const elapsed = Math.round(performance.now() - start);
    const responseHeaders: Record<string, string> = {};
    response.headers.forEach((v, k) => {
      responseHeaders[k] = v;
    });

    const bodyText = response.text();

    // Update log with status immediately
    postOut({
      logId,
      patch: { durationMs: elapsed, responseHeaders, status: response.status },
      type: 'fetchLogUpdate',
    });

    // Update log with body when it resolves
    bodyText.then(
      (text) =>
        postOut({
          logId,
          patch: { responseBody: text },
          type: 'fetchLogUpdate',
        }),
      (err) =>
        postOut({
          logId,
          patch: { error: `Body read error: ${err}` },
          type: 'fetchLogUpdate',
        }),
    );

    return {
      bodyPromise: bodyText,
      headersJson: JSON.stringify(responseHeaders),
      status: response.status,
      url: response.url,
    };
  } catch (err) {
    const elapsed = Math.round(performance.now() - start);
    const msg = err instanceof Error ? err.message : String(err);
    postOut({
      logId,
      patch: { durationMs: elapsed, error: msg, status: 0 },
      type: 'fetchLogUpdate',
    });

    return {
      bodyPromise: Promise.resolve(''),
      headersJson: '{}',
      status: 0,
      url,
    };
  }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

export function mapsToRecordsDeep<T>(input: T): T {
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

function onPlaygroundNotification(notification: PlaygroundNotification): void {
  // Request-response messages get unwrapped to top-level WorkerOutMessage.
  // Only unsolicited push notifications stay wrapped in playgroundNotification.
  switch (notification.type) {
    case 'controlFlowGraphResult':
      postOut({
        functionName: notification.functionName,
        graph: notification.graph ?? null,
        type: 'controlFlowGraphResult',
        ...(notification.requestId !== undefined
          ? { requestId: notification.requestId }
          : {}),
      });
      break;
    case 'cursorContext':
      postOut({
        context: notification.context,
        type: 'cursorContext',
      });
      break;
    case 'runStarted':
      postOut({
        requestId: notification.requestId,
        run: notification.run,
        type: 'runStarted',
      } as WorkerOutMessage);
      break;
    case 'runPatch':
      postOut({
        patch: notification.patch,
        type: 'runPatch',
      } as WorkerOutMessage);
      break;
    case 'runSnapshot':
      postOut({
        boundaryId: notification.boundaryId,
        requestId: notification.requestId,
        snapshot: notification.snapshot,
        type: 'runSnapshot',
      } as WorkerOutMessage);
      break;
    case 'valueBody':
      postOut({
        availability: notification.availability,
        bodyBase64: notification.bodyBase64,
        boundaryId: notification.boundaryId,
        codec: notification.codec,
        diagnostic: notification.diagnostic,
        requestId: notification.requestId,
        type: 'valueBody',
        valueRefId: notification.valueRefId,
      } as WorkerOutMessage);
      break;
    case 'runList':
      postOut({
        requestId: notification.requestId,
        runs: notification.runs,
        type: 'runList',
      } as WorkerOutMessage);
      break;
    case 'historyList':
      postOut({
        requestId: notification.requestId,
        runs: notification.runs,
        type: 'historyList',
      } as WorkerOutMessage);
      break;
    case 'runCursorExpired':
      postOut({
        boundaryId: notification.boundaryId,
        reason: notification.reason,
        requestId: notification.requestId,
        subscriptionId: notification.subscriptionId,
        type: 'runCursorExpired',
      } as WorkerOutMessage);
      break;
    case 'commandAck':
      postOut({
        outcome: notification.outcome,
        requestId: notification.requestId,
        type: 'commandAck',
      });
      break;
    case 'commandError':
      postOut({
        code: notification.code,
        message: notification.message,
        requestId: notification.requestId,
        type: 'commandError',
      });
      break;
    default:
      // Cast to worker-protocol type: the WASM-generated type uses `string` for severity
      // while the protocol narrows it to a literal union; the runtime values are always valid.
      postOut({
        notification: notification as unknown as WorkerPlaygroundNotification,
        type: 'playgroundNotification',
      });
  }
}

// ---------------------------------------------------------------------------
// LSP diagnostics push (for Monaco squiggly lines)
// ---------------------------------------------------------------------------

/** Track which file URIs had diagnostics last time so we can clear stale ones. */
// const previousDiagnosticUris = new Set<string>();

// function publishDiagnostics(): void {
//   if (!runtime || !connection) return;

//   const diagsByFile = runtime.lspDiagnostics();
//   if (!diagsByFile) return;

//   const currentUris = new Set<string>();

//   diagsByFile.forEach((diags, filePath) => {
//     const uri = `file://${filePath}`;
//     currentUris.add(uri);
//     connection!.sendDiagnostics({
//       uri,
//       diagnostics: diags.map((d) => ({
//         severity: d.severity === 'error' ? 1 : d.severity === 'warning' ? 2 : 3,
//         range: d.range ?? { start: { line: 0, character: 0 }, end: { line: 0, character: 0 } },
//         message: d.message,
//         source: 'baml',
//       })),
//     });
//   });

//   // Clear diagnostics for files that no longer have any
//   for (const uri of previousDiagnosticUris) {
//     if (!currentUris.has(uri)) {
//       connection.sendDiagnostics({ uri, diagnostics: [] });
//     }
//   }

//   // Remember current set for next time
//   previousDiagnosticUris.clear();
//   for (const uri of currentUris) {
//     previousDiagnosticUris.add(uri);
//   }
// }

// ---------------------------------------------------------------------------
// Worker entry point
// ---------------------------------------------------------------------------

self.onmessage = async (event: MessageEvent) => {
  if (disposed) return;
  const data = event.data;

  // ── Init message (contains the LSP MessagePort) ──────────────────────
  if (data.port) {
    if (disposed) return;
    const {
      port,
      initialFiles,
      rootPath: initRootPath,
    } = data as WorkerInitMessage;

    // Populate VFS with the initial file snapshot from the main thread
    if (initRootPath) vfs = new BamlVfs(initRootPath);
    if (initialFiles) vfs.setFiles(initialFiles);

    // Propagate WASM-initiated file mutations back to the main thread
    vfs.onChange = (change) => {
      if ('deleted' in change && change.deleted) {
        postOut({ path: change.path, type: 'vfsFileDeleted' });
      } else {
        postOut({
          content: change.content,
          path: change.path,
          type: 'vfsFileChanged',
        });
      }
    };

    // 1. Initialize WASM
    await initWasm();
    await setupLogger();
    console.log('logger setup');

    // 2. Set up LSP connection on the MessagePort
    const reader = new BrowserMessageReader(port);
    const writer = new BrowserMessageWriter(port);
    connection = createConnection(reader, writer);
    connection.sendNotification;

    const requestPromises = new Map<
      number | string,
      (result: LspResponse) => void
    >();
    const callbacks = {
      env: resolveEnv,
      exec: executeExec,
      fetch: loggingFetch,
      host_dispatch: () => {},
      input: resolveInput,
      lsp_make_request: (request: LspRequest) => {
        request = mapsToRecordsDeep(request);
        console.log('make_request', request);
        connection?.sendRequest(request.method, request.params);
      },
      lsp_send_notification: (notification: LspNotification) => {
        notification = mapsToRecordsDeep(notification);

        console.log('send_notification', notification);
        connection?.sendNotification(notification.method, notification.params);
      },
      lsp_send_response: (response: LspResponse) => {
        response = mapsToRecordsDeep(response);
        console.log('send_response', response);
        const resolver = requestPromises.get(response.id);
        if (resolver) {
          requestPromises.delete(response.id);
          resolver(response);
        }
      },
      playground_send_notification: (notification: PlaygroundNotification) => {
        notification = mapsToRecordsDeep(notification);
        onPlaygroundNotification(notification);
      },
      shell: executeShell,
    } as unknown as Parameters<typeof BamlWasmRuntime.create>[0];
    runtime = BamlWasmRuntime.create(callbacks, vfs.wasmVfs);

    connection.onShutdown(() => {
      console.log('[LSP] shutdown requested');
      if (runtime) {
        runtime.free();
        runtime = null;
      }
    });

    connection.onExit(() => {
      console.log('[LSP] exit received');
      disposed = true;
    });

    // The LSP library dispatches "initialize" to onInitialize, not to onRequest.
    // We must handle it here and forward to the WASM runtime so the client gets a response.
    connection.onInitialize((params) => {
      const id = nextRequestId++;
      console.log('onInitialize', id, params);
      return new Promise((resolve, reject) => {
        requestPromises.set(id, (response: LspResponse) => {
          if (response.error) {
            reject(response.error);
          } else {
            resolve(response.result ?? undefined);
          }
        });
        try {
          runtime?.handleLspRequest({ id, method: 'initialize', params });
        } catch (e) {
          if (e instanceof Error && isWasmPanic(e)) {
            const { message, stack } = getWasmError(e);
            console.error('[LSP] initialize request WASM panic:', message);
            postOut({ message, stack, type: 'wasmPanic' });
          } else {
            console.error('[LSP] initialize request failed:', e);
          }
          requestPromises.delete(id);
          reject(e);
        }
      });
    });

    connection.onNotification((method: string, params: any) => {
      console.log('onNotification', method, params);
      try {
        runtime?.handleLspNotification({ method, params });
      } catch (e) {
        if (e instanceof Error && isWasmPanic(e)) {
          const { message, stack } = getWasmError(e);
          console.error(`[LSP] notification "${method}" WASM panic:`, message);
          postOut({ message, stack, type: 'wasmPanic' });
        } else {
          console.error(`[LSP] notification "${method}" failed:`, e);
        }
      }
    });

    let nextRequestId = 0;
    connection.onRequest((method: string, params: any) => {
      const id = nextRequestId++;
      console.log('onRequest', id, method, params);
      const promise = new Promise((resolve, reject) => {
        requestPromises.set(id, (result: LspResponse) => {
          if (result.error) {
            reject(result.error);
          } else {
            resolve(result.result);
          }
        });
      });
      try {
        runtime?.handleLspRequest({ id, method, params });
      } catch (e) {
        if (e instanceof Error && isWasmPanic(e)) {
          const { message, stack } = getWasmError(e);
          console.error(`[LSP] request "${method}" WASM panic:`, message);
          postOut({ message, stack, type: 'wasmPanic' });
        } else {
          console.error(`[LSP] request "${method}" failed:`, e);
        }
        requestPromises.delete(id);
        return Promise.reject(e);
      }
      return promise;
    });

    // 6. Start LSP listening
    connection.listen();

    // 7. Notify main thread and push initial state
    postOut({ commit: commitHash(), type: 'ready', version: bamlVersion() });
    postOut({ type: 'buildTime', value: getBuildTime() });
    // notifySourceChanged();

    return;
  }

  // ── Custom RPC messages (non-LSP) ──────────────────────────────────────

  const msg = data as WorkerInMessage;

  switch (msg.type) {
    case 'startRun': {
      const rt = runtimeForCommand(msg.requestId, 'wasmRuntimeNotReady');
      if (!rt) return;
      try {
        rt.startRun(
          msg.requestId,
          msg.project,
          msg.functionName,
          msg.argsBytes,
        );
      } catch (e) {
        postOut({
          code: 'wasmStartRunFailed',
          message: e instanceof Error ? e.message : String(e),
          requestId: msg.requestId,
          type: 'commandError',
        });
      }
      return;
    }

    case 'respondToInput': {
      const rt = runtimeForCommand(msg.requestId, 'wasmRuntimeNotReady');
      if (!rt) return;
      try {
        const outcome = rt.respondToInput(
          msg.requestId,
          msg.boundaryId,
          msg.inputRequestId,
        );
        if (outcome === 'accepted') {
          const id = Number(msg.inputRequestId);
          const pending = Number.isFinite(id)
            ? pendingInputResolvers.get(id)
            : undefined;
          if (pending) {
            pendingInputResolvers.delete(id);
            pending.resolve(msg.value);
          }
        }
      } catch (e) {
        postOut({
          code: 'wasmRespondToInputFailed',
          message: e instanceof Error ? e.message : String(e),
          requestId: msg.requestId,
          type: 'commandError',
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
          code: 'wasmRespondToEnvFailed',
          message: e instanceof Error ? e.message : String(e),
          requestId: msg.requestId,
          type: 'commandError',
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
          code: 'wasmStartPreviewRunFailed',
          message: e instanceof Error ? e.message : String(e),
          requestId: msg.requestId,
          type: 'commandError',
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
          code: 'wasmStartTestRunFailed',
          message: e instanceof Error ? e.message : String(e),
          requestId: msg.requestId,
          type: 'commandError',
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
          code: 'wasmCancelRunFailed',
          message: e instanceof Error ? e.message : String(e),
          requestId: msg.requestId,
          type: 'commandError',
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
          code: 'wasmListRunsFailed',
          message: e instanceof Error ? e.message : String(e),
          requestId: msg.requestId,
          type: 'commandError',
        });
      }
      return;
    }

    case 'listHistory': {
      const rt = runtimeForCommand(msg.requestId, 'wasmRuntimeNotReady');
      if (!rt) return;
      try {
        rt.listHistory(msg.requestId, msg.filter);
      } catch (e) {
        postOut({
          code: 'wasmListHistoryFailed',
          message: e instanceof Error ? e.message : String(e),
          requestId: msg.requestId,
          type: 'commandError',
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
          code: 'wasmOpenHistoryFailed',
          message: e instanceof Error ? e.message : String(e),
          requestId: msg.requestId,
          type: 'commandError',
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
          code: 'wasmSnapshotFailed',
          message: e instanceof Error ? e.message : String(e),
          requestId: msg.requestId,
          type: 'commandError',
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
          code: 'wasmReadValueFailed',
          message: e instanceof Error ? e.message : String(e),
          requestId: msg.requestId,
          type: 'commandError',
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
          code: 'wasmSubscribeFailed',
          message: e instanceof Error ? e.message : String(e),
          requestId: msg.requestId,
          type: 'commandError',
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
          code: 'wasmUnsubscribeFailed',
          message: e instanceof Error ? e.message : String(e),
          requestId: msg.requestId,
          type: 'commandError',
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

    case 'inputResponse': {
      const pending = pendingInputResolvers.get(msg.id);
      if (pending) {
        pendingInputResolvers.delete(msg.id);
        pending.resolve(msg.value);
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
      // Clear decorations when files change
      clearLogDecorations();
      return;
    }

    case 'selectProject':
      return;

    // Runtime-demand leases are only meaningful for the remote (WebSocket)
    // transport; the in-worker runtime is always resident.
    case 'ensureProjectRuntime':
    case 'releaseProjectRuntime':
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

    case 'dispose':
      dispose();
      return;
  }
  msg satisfies never;
};
