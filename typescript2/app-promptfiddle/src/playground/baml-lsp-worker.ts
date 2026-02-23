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
  BrowserMessageReader,
  BrowserMessageWriter,
  createConnection,
  type Connection,
} from "vscode-languageserver/browser.js";

import initWasm, {
  BamlWasmRuntime,
  LspNotification,
  LspRequest,
  LspResponse,
  type PlaygroundNotification,
  start as setupLogger,
  getBuildTime,
} from "@b/bridge_wasm";

import type {
  WorkerOutMessage,
  WorkerInMessage,
  WorkerInitMessage,
} from "@b/pkg-playground";

import { BamlVfs } from "./vfs";

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
let vfs: BamlVfs = new BamlVfs("/workspace");

// ---------------------------------------------------------------------------
// Env vars (worker-side store)
// ---------------------------------------------------------------------------

const envVars: Record<string, string> = {};
let nextEnvReqId = 0;
const pendingEnvResolvers = new Map<number, (v: string | undefined) => void>();

function resolveEnv(variable: string): Promise<string | undefined> {
  if (variable in envVars) return Promise.resolve(envVars[variable]);
  return new Promise<string | undefined>((resolve) => {
    const id = nextEnvReqId++;
    pendingEnvResolvers.set(id, resolve);
    postOut({ type: "envVarRequest", id, variable });
  });
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
    type: "fetchLogNew",
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
    },
  });

  const start = performance.now();

  try {
    const response = await fetch(url, {
      method,
      headers: parsedHeaders,
      body: method !== "GET" && method !== "HEAD" ? body : undefined,
    });

    const elapsed = Math.round(performance.now() - start);
    const responseHeaders: Record<string, string> = {};
    response.headers.forEach((v, k) => {
      responseHeaders[k] = v;
    });

    const bodyText = response.text();

    // Update log with status immediately
    postOut({
      type: "fetchLogUpdate",
      logId,
      patch: { status: response.status, durationMs: elapsed },
    });

    // Update log with body when it resolves
    bodyText.then(
      (text) =>
        postOut({
          type: "fetchLogUpdate",
          logId,
          patch: { responseBody: text },
        }),
      (err) =>
        postOut({
          type: "fetchLogUpdate",
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
      type: "fetchLogUpdate",
      logId,
      patch: { status: 0, error: msg, durationMs: elapsed },
    });

    return {
      status: 0,
      headersJson: "{}",
      url,
      bodyPromise: Promise.resolve(""),
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

  if (input !== null && typeof input === "object") {
    const obj: Record<string, any> = {};
    for (const [key, value] of Object.entries(input)) {
      obj[key] = mapsToRecordsDeep(value);
    }
    return obj as T;
  }

  return input;
}


function onPlaygroundNotification(notification: PlaygroundNotification): void {
  postOut({ type: "playgroundNotification", notification });
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
    const { port, initialFiles, rootPath: initRootPath } =
      data as WorkerInitMessage;

    // Populate VFS with the initial file snapshot from the main thread
    if (initRootPath) vfs = new BamlVfs(initRootPath);
    if (initialFiles) vfs.setFiles(initialFiles);

    // Propagate WASM-initiated file mutations back to the main thread
    vfs.onChange = (change) => {
      if ('deleted' in change && change.deleted) {
        postOut({ type: 'vfsFileDeleted', path: change.path });
      } else {
        postOut({ type: 'vfsFileChanged', path: change.path, content: change.content });
      }
    };

    // 1. Initialize WASM
    await initWasm();
    await setupLogger();
    console.log("logger setup");

    // 2. Set up LSP connection on the MessagePort
    const reader = new BrowserMessageReader(port);
    const writer = new BrowserMessageWriter(port);
    connection = createConnection(reader, writer);
    connection.sendNotification

    let requestPromises = new Map<
      number | string,
      (result: LspResponse) => void
    >();
    runtime = BamlWasmRuntime.create(
      {
        fetch: loggingFetch,
        env: resolveEnv,
        lsp_send_notification: (notification: LspNotification) => {
          notification = mapsToRecordsDeep(notification);

          console.log("send_notification", notification);
          connection?.sendNotification(
            notification.method,
            notification.params,
          );
        },
        lsp_send_response: (response: LspResponse) => {
          response = mapsToRecordsDeep(response);
          console.log("send_response", response);
          let resolver = requestPromises.get(response.id);
          if (resolver) {
            resolver(response);
          }
        },
        lsp_make_request: (request: LspRequest) => {
          request = mapsToRecordsDeep(request);
          console.log("make_request", request);
          connection?.sendRequest(request.method, request.params);
        },
        playground_send_notification: (notification: PlaygroundNotification) => {
          notification = mapsToRecordsDeep(notification);
          onPlaygroundNotification(notification);
        },
      },
      vfs.wasmVfs,
    );

    connection.onShutdown(() => {
      console.log("[LSP] shutdown requested");
      if (runtime) {
        runtime.free();
        runtime = null;
      }
    });

    connection.onExit(() => {
      console.log("[LSP] exit received");
      disposed = true;
    });

    // The LSP library dispatches "initialize" to onInitialize, not to onRequest.
    // We must handle it here and forward to the WASM runtime so the client gets a response.
    connection.onInitialize((params) => {
      const id = nextRequestId++;
      console.log("onInitialize", id, params);
      return new Promise((resolve, reject) => {
        requestPromises.set(id, (response: LspResponse) => {
          if (response.error) {
            reject(response.error);
          } else {
            resolve(response.result ?? undefined);
          }
        });
        try {
          runtime?.handleLspRequest({ id, method: "initialize", params });
        } catch (e) {
          console.error("[LSP] initialize request failed:", e);
          requestPromises.delete(id);
          reject(e);
        }
      });
    });

    connection.onNotification((method: string, params: any) => {
      console.log("onNotification", method, params);
      try {
        runtime?.handleLspNotification({ method, params });
      } catch (e) {
        console.error(`[LSP] notification "${method}" failed:`, e);
      }
    });

    let nextRequestId = 0;
    connection.onRequest((method: string, params: any) => {
      let id = nextRequestId++;
      console.log("onRequest", id, method, params);
      let promise = new Promise((resolve, reject) => {
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
        console.error(`[LSP] request "${method}" failed:`, e);
        requestPromises.delete(id);
        return Promise.reject(e);
      }
      return promise;
    });

    // 6. Start LSP listening
    connection.listen();

    // 7. Notify main thread and push initial state
    postOut({ type: "ready" });
    postOut({ type: "buildTime", value: getBuildTime() });
    // notifySourceChanged();

    return;
  }

  // ── Custom RPC messages (non-LSP) ──────────────────────────────────────

  const msg = data as WorkerInMessage;

  switch (msg.type) {
    case "callFunction": {
      if (!runtime) {
        postOut({
          type: "callFunctionError",
          id: msg.id,
          error: "Runtime not initialized",
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
        const result = new Uint8Array(resultBytes);
        postOut({ type: "callFunctionResult", id: msg.id, result }, [
          result.buffer,
        ]);
      } catch (e) {
        postOut({
          type: "callFunctionError",
          id: msg.id,
          error: e instanceof Error ? e.message : String(e),
        });
      }
      return;
    }

    case "envVarResponse": {
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

    case "setEnvVar":
      envVars[msg.key] = msg.value;
      return;

    case "deleteEnvVar":
      delete envVars[msg.key];
      return;

    case "filesChanged": {
      vfs.setFiles(msg.files);
      return;
    }

    case "selectProject":
      return;

    case "requestState":
      runtime?.requestPlaygroundState();
      postOut({ type: "buildTime", value: getBuildTime() });
      return;

    case "dispose":
      dispose();
      return;
  }
  msg satisfies never;
};
