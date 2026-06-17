/**
 * RuntimePort backed by a WebSocket connection to the Rust playground server.
 *
 * Used in the VS Code webview where the Rust LSP server runs the BAML runtime.
 * Communicates over ws://localhost:{port}/api/ws with JSON messages.
 * Argument/result bytes are base64-encoded for transit.
 *
 * Features:
 *   - Queues outgoing messages while WebSocket is connecting
 *   - Buffers incoming messages until a handler is registered (avoids race)
 *   - Auto-reconnects on close/error with exponential backoff
 */

import type { RuntimePort } from '../runtime-port';
import type {
  WorkerOutMessage,
  WorkerInMessage,
  PlaygroundNotification,
  Run,
  RunCursorExpiredReason,
  RunListFilter,
  RunPatch,
  RunSummary,
} from '../worker-protocol';
import { isPlaygroundProtocolCompatible } from '../protocol';

/** Server → Client message shapes (must match playground_ws.rs WsOutMessage) */
type WsOutMessage =
  | {
      type: 'hello';
      toolchainVersion: string;
      playgroundProtocol: number;
      minClientPlaygroundProtocol: number;
      capabilities: string[];
    }
  | { type: 'ready' }
  | { type: 'playgroundNotification'; notification: PlaygroundNotification }
  | { type: 'runStarted'; requestId?: number; run: Run }
  | { type: 'runPatch'; patch: RunPatch }
  | { type: 'commandAck'; requestId: number; outcome: string }
  | { type: 'commandError'; requestId: number; code: string; message: string }
  | { type: 'runList'; requestId: number; runs: RunSummary[] }
  | { type: 'runSnapshot'; requestId?: number; runId: string; snapshot: Run }
  | {
      type: 'runCursorExpired';
      requestId?: number;
      subscriptionId?: string;
      runId: string;
      reason: RunCursorExpiredReason;
    }
  | { type: 'envVarRequest'; id: number; variable: string }
  | { type: 'processEnvVars'; vars: Record<string, string> }
  | { type: 'envVarFromShell'; variable: string; value: string }
  | { type: 'knownEnvVarNames'; names: string[] }
  | {
      type: 'inputRequest';
      id: number;
      prompt: string | undefined;
      callId: number;
    }
  | { type: 'inputResolved'; id: number; callId: number }
  | {
      type: 'fetchLogNew';
      callId: number;
      id: number;
      method: string;
      url: string;
      requestHeaders: Record<string, string>;
      requestBody: string;
    }
  | {
      type: 'fetchLogUpdate';
      callId: number;
      logId: number;
      status?: number;
      durationMs?: number;
      responseBody?: string;
      error?: string;
      responseHeaders?: Record<string, string>;
    }
  | {
      type: 'controlFlowGraphResult';
      functionName: string;
      graph: unknown | null;
    }
  | { type: 'cursorContext'; context: unknown };

/** Client → Server message shapes (must match playground_ws.rs WsInMessage) */
type WsInMessage =
  | {
      type: 'startRun';
      requestId: number;
      project: string;
      functionName: string;
      argsBytes: string;
    }
  | {
      type: 'startPreviewRun';
      requestId: number;
      project: string;
      parentFunctionName: string;
      helper: string;
      functionName: string;
      argsBytes: string;
    }
  | {
      type: 'startTestRun';
      requestId: number;
      project: string;
      generation: number;
      testName: string;
    }
  | { type: 'cancelRun'; requestId: number; runId: string }
  | {
      type: 'respondToInput';
      requestId: number;
      runId: string;
      inputRequestId: string;
      value: string;
    }
  | {
      type: 'respondToEnv';
      requestId: number;
      runId: string;
      envRequestId: string;
      value?: string;
    }
  | { type: 'listRuns'; requestId: number; filter?: RunListFilter }
  | { type: 'snapshot'; requestId: number; runId: string }
  | {
      type: 'subscribe';
      requestId: number;
      subscriptionId: string;
      runId: string;
      afterCursor?: number;
    }
  | { type: 'unsubscribe'; requestId: number; subscriptionId: string }
  | {
      type: 'expandTestSet';
      project: string;
      generation: number;
      testsetName: string;
    }
  | {
      type: 'envVarResponse';
      id: number;
      value: string | undefined;
      variable?: string;
    }
  | { type: 'inputResponse'; id: number; value: string; callId: number }
  | { type: 'setEnvVar'; key: string; value: string }
  | { type: 'deleteEnvVar'; key: string }
  | { type: 'requestState' }
  | { type: 'requestCollectTests'; project: string }
  | { type: 'requestControlFlowGraph'; project: string; functionName: string }
  | { type: 'cursorPosition'; file: string; line: number; column: number };

const MAX_RECONNECT_DELAY = 5000;

export class WebSocketRuntimePort implements RuntimePort {
  private url: string;
  private ws: WebSocket | null = null;
  private handlers = new Set<(msg: WorkerOutMessage) => void>();
  private outQueue: string[] = [];
  private inBuffer: WorkerOutMessage[] = [];
  private disposed = false;
  private reconnectDelay = 500;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private playgroundCompatible = true;

  constructor(url: string) {
    this.url = url;
    this.connect();
  }

  private connect(): void {
    if (this.disposed) return;

    try {
      this.ws = new WebSocket(this.url);
    } catch {
      this.scheduleReconnect();
      return;
    }

    this.ws.onopen = () => {
      this.reconnectDelay = 500; // reset backoff
      // Flush queued outgoing messages.
      for (const msg of this.outQueue) {
        this.ws!.send(msg);
      }
      this.outQueue = [];
      this.ws!.send(JSON.stringify({ type: 'requestState' }));
    };

    this.ws.onmessage = (event: MessageEvent) => {
      try {
        const raw: WsOutMessage = JSON.parse(event.data as string);
        const msg = this.fromServer(raw);
        if (!msg) return;

        if (this.handlers.size === 0) {
          // No handler registered yet — buffer the message.
          this.inBuffer.push(msg);
        } else {
          for (const h of this.handlers) h(msg);
        }
      } catch (e) {
        console.warn('WebSocketRuntimePort: failed to parse message', e);
      }
    };

    this.ws.onclose = () => {
      if (!this.disposed) {
        this.scheduleReconnect();
      }
    };

    this.ws.onerror = () => {
      // onclose will fire after onerror, which triggers reconnect.
    };
  }

  private scheduleReconnect(): void {
    if (this.disposed || this.reconnectTimer) return;
    this.reconnectTimer = setTimeout(() => {
      this.reconnectTimer = null;
      this.connect();
    }, this.reconnectDelay);
    this.reconnectDelay = Math.min(
      this.reconnectDelay * 2,
      MAX_RECONNECT_DELAY,
    );
  }

  postMessage(msg: WorkerInMessage): void {
    const serverMsg = this.toServer(msg);
    if (!serverMsg) return;
    this.sendServerMessage(serverMsg);
  }

  private sendServerMessage(serverMsg: WsInMessage): void {
    const raw = JSON.stringify(serverMsg);
    if (this.ws && this.ws.readyState === WebSocket.OPEN) {
      this.ws.send(raw);
    } else {
      this.outQueue.push(raw);
    }
  }

  onMessage(handler: (msg: WorkerOutMessage) => void): () => void {
    this.handlers.add(handler);

    // Replay any buffered messages that arrived before the handler was registered.
    if (this.inBuffer.length > 0) {
      const buffered = this.inBuffer.splice(0);
      for (const msg of buffered) {
        handler(msg);
      }
    }

    return () => {
      this.handlers.delete(handler);
    };
  }

  dispatchLocalMessage(msg: WorkerOutMessage): void {
    this.deliver(msg);
  }

  dispose(): void {
    this.disposed = true;
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
    if (this.ws) {
      this.ws.onclose = null; // prevent reconnect from firing
      this.ws.close();
      this.ws = null;
    }
    this.handlers.clear();
    this.outQueue = [];
    this.inBuffer = [];
  }

  // ---------------------------------------------------------------------------
  // Convert WorkerInMessage → WsInMessage (base64-encode argsBytes)
  // ---------------------------------------------------------------------------

  private toServer(msg: WorkerInMessage): WsInMessage | null {
    switch (msg.type) {
      case 'startRun':
        this.clearLogDecorations();
        return {
          type: 'startRun',
          requestId: msg.requestId,
          project: msg.project,
          functionName: msg.functionName,
          argsBytes: uint8ArrayToBase64(msg.argsBytes),
        };
      case 'startPreviewRun':
        this.clearLogDecorations();
        return {
          type: 'startPreviewRun',
          requestId: msg.requestId,
          project: msg.project,
          parentFunctionName: msg.parentFunctionName,
          helper: msg.helper,
          functionName: msg.functionName,
          argsBytes: uint8ArrayToBase64(msg.argsBytes),
        };
      case 'startTestRun':
        this.clearLogDecorations();
        return {
          type: 'startTestRun',
          requestId: msg.requestId,
          project: msg.project,
          generation: msg.generation,
          testName: msg.testName,
        };
      case 'cancelRun':
        return { type: 'cancelRun', requestId: msg.requestId, runId: msg.runId };
      case 'respondToInput':
        return {
          type: 'respondToInput',
          requestId: msg.requestId,
          runId: msg.runId,
          inputRequestId: msg.inputRequestId,
          value: msg.value,
        };
      case 'respondToEnv':
        return {
          type: 'respondToEnv',
          requestId: msg.requestId,
          runId: msg.runId,
          envRequestId: msg.envRequestId,
          value: msg.value,
        };
      case 'listRuns':
        return { type: 'listRuns', requestId: msg.requestId, filter: msg.filter };
      case 'snapshot':
        return { type: 'snapshot', requestId: msg.requestId, runId: msg.runId };
      case 'subscribe':
        return {
          type: 'subscribe',
          requestId: msg.requestId,
          subscriptionId: msg.subscriptionId,
          runId: msg.runId,
          afterCursor: msg.afterCursor,
        };
      case 'unsubscribe':
        return {
          type: 'unsubscribe',
          requestId: msg.requestId,
          subscriptionId: msg.subscriptionId,
        };
      case 'envVarResponse':
        return {
          type: 'envVarResponse',
          id: msg.id,
          value: msg.value,
          variable: msg.variable,
        };
      case 'setEnvVar':
        return { type: 'setEnvVar', key: msg.key, value: msg.value };
      case 'deleteEnvVar':
        return { type: 'deleteEnvVar', key: msg.key };
      case 'selectProject':
        return null; // handled locally for now
      case 'filesChanged':
        return null; // handled locally, not sent to server
      case 'requestState':
        return { type: 'requestState' };
      case 'requestControlFlowGraph':
        return {
          type: 'requestControlFlowGraph',
          project: msg.project,
          functionName: msg.functionName,
        };
      case 'cursorPosition':
        return {
          type: 'cursorPosition',
          file: msg.file,
          line: msg.line,
          column: msg.column,
        };
      case 'requestCollectTests':
        return { type: 'requestCollectTests', project: msg.project };
      case 'expandTestSet':
        return {
          type: 'expandTestSet',
          project: msg.project,
          generation: msg.generation,
          testsetName: msg.testsetName,
        };
      case 'inputResponse':
        return {
          type: 'inputResponse',
          id: msg.id,
          value: msg.value,
          callId: msg.callId,
        };
      case 'dispose':
        return null; // worker-only; no server equivalent
    }
    msg satisfies never;
    return null;
  }

  // ---------------------------------------------------------------------------
  // Convert WsOutMessage → WorkerOutMessage (base64-decode resultProto)
  // ---------------------------------------------------------------------------

  private fromServer(raw: WsOutMessage): WorkerOutMessage | null {
    switch (raw.type) {
      case 'hello':
        this.playgroundCompatible = isPlaygroundProtocolCompatible(
          raw.playgroundProtocol,
          raw.minClientPlaygroundProtocol,
        );
        if (!this.playgroundCompatible) {
          console.warn(
            `BAML playground protocol ${raw.playgroundProtocol} from toolchain ${raw.toolchainVersion} is incompatible with this extension.`,
          );
        }
        return null;
      case 'ready':
        if (!this.playgroundCompatible) {
          return null;
        }
        return { type: 'ready' };
      case 'playgroundNotification':
        return {
          type: 'playgroundNotification',
          notification: raw.notification,
        };
      case 'runStarted':
        return {
          type: 'runStarted',
          requestId: raw.requestId,
          run: raw.run,
        };
      case 'runPatch':
        return { type: 'runPatch', patch: raw.patch };
      case 'commandAck':
        return {
          type: 'commandAck',
          requestId: raw.requestId,
          outcome: raw.outcome,
        };
      case 'commandError':
        return {
          type: 'commandError',
          requestId: raw.requestId,
          code: raw.code,
          message: raw.message,
        };
      case 'runList':
        return { type: 'runList', requestId: raw.requestId, runs: raw.runs };
      case 'runSnapshot':
        return {
          type: 'runSnapshot',
          requestId: raw.requestId,
          runId: raw.runId,
          snapshot: raw.snapshot,
        };
      case 'runCursorExpired':
        return {
          type: 'runCursorExpired',
          requestId: raw.requestId,
          subscriptionId: raw.subscriptionId,
          runId: raw.runId,
          reason: raw.reason,
        };
      case 'envVarRequest':
        return { type: 'envVarRequest', id: raw.id, variable: raw.variable };
      case 'processEnvVars':
        return { type: 'processEnvVars', vars: raw.vars };
      case 'envVarFromShell':
        return {
          type: 'envVarFromShell',
          variable: raw.variable,
          value: raw.value,
        };
      case 'knownEnvVarNames':
        return { type: 'knownEnvVarNames', names: raw.names };
      case 'inputRequest':
        return {
          type: 'inputRequest',
          id: raw.id,
          prompt: raw.prompt,
          callId: raw.callId,
        };
      case 'inputResolved':
        return { type: 'inputResolved', id: raw.id, callId: raw.callId };
      case 'fetchLogNew':
        return {
          type: 'fetchLogNew',
          callId: raw.callId,
          entry: {
            id: raw.id,
            timestamp: Date.now(),
            method: raw.method,
            url: raw.url,
            requestHeaders: raw.requestHeaders,
            requestBody: raw.requestBody,
            status: null,
            responseBody: null,
            error: null,
            durationMs: null,
            responseHeaders: null,
          },
        };
      case 'fetchLogUpdate':
        return {
          type: 'fetchLogUpdate',
          logId: raw.logId,
          patch: {
            ...(raw.status !== undefined ? { status: raw.status } : {}),
            ...(raw.durationMs !== undefined
              ? { durationMs: raw.durationMs }
              : {}),
            ...(raw.responseBody !== undefined
              ? { responseBody: raw.responseBody }
              : {}),
            ...(raw.error !== undefined ? { error: raw.error } : {}),
            ...(raw.responseHeaders !== undefined
              ? { responseHeaders: raw.responseHeaders }
              : {}),
          },
        };
      case 'controlFlowGraphResult':
        return {
          type: 'controlFlowGraphResult',
          functionName: raw.functionName,
          graph: (raw.graph ?? null) as
            | import('../worker-protocol').ControlFlowGraph
            | null,
        };
      case 'cursorContext':
        return {
          type: 'cursorContext',
          context: raw.context as import('../worker-protocol').CursorContext,
        };
      default:
        return null;
    }
  }

  /** Buffer-or-dispatch: if no handlers are registered yet, buffer the message for replay. */
  private deliver(msg: WorkerOutMessage): void {
    if (this.handlers.size === 0) {
      this.inBuffer.push(msg);
    } else {
      for (const h of this.handlers) h(msg);
    }
  }

  private clearLogDecorations(): void {
    this.deliver({ type: 'clearLogDecorations' });
  }
}

// ---------------------------------------------------------------------------
// Base64 helpers
// ---------------------------------------------------------------------------

function uint8ArrayToBase64(bytes: Uint8Array): string {
  let binary = '';
  for (let i = 0; i < bytes.byteLength; i++) {
    binary += String.fromCharCode(bytes[i]!);
  }
  return btoa(binary);
}
