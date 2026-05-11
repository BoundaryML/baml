/**
 * RuntimePort backed by a WebSocket connection to the Rust playground server.
 *
 * Used in the VS Code webview where the Rust LSP server runs the BAML runtime.
 * Communicates over ws://localhost:{port}/api/ws with JSON messages.
 * Proto bytes (argsProto / result) are base64-encoded for transit.
 *
 * Features:
 *   - Queues outgoing messages while WebSocket is connecting
 *   - Buffers incoming messages until a handler is registered (avoids race)
 *   - Auto-reconnects on close/error with exponential backoff
 */

import type { RuntimePort } from '../runtime-port';
import type { WorkerOutMessage, WorkerInMessage, PlaygroundNotification, LogLevel, LogDecoration } from '../worker-protocol';
import { decodeCallResult, RuntimeEvent } from '@b/pkg-proto';
import { truncateMessage, normalizeLogLevel } from '../shared/log-decorations';
import { formatValue } from '../shared/format-value';
import { deserializeRuntimeEvent } from '../shared/deserialize-event';

/** Server → Client message shapes (must match playground_ws.rs WsOutMessage) */
type WsOutMessage =
  | { type: 'ready' }
  | { type: 'playgroundNotification'; notification: PlaygroundNotification }
  | { type: 'callFunctionResult'; id: number; result: string }
  | { type: 'callFunctionError'; id: number; error: string; cancelled?: boolean }
  | { type: 'envVarRequest'; id: number; variable: string }
  | { type: 'processEnvVars'; vars: Record<string, string> }
  | { type: 'envVarFromShell'; variable: string; value: string }
  | { type: 'knownEnvVarNames'; names: string[] }
  | { type: 'inputRequest'; id: number; prompt: string | undefined; callId: number }
  | { type: 'inputResolved'; id: number; callId: number }
  | { type: 'fetchLogNew'; callId: number; id: number; method: string; url: string; requestHeaders: Record<string, string>; requestBody: string }
  | { type: 'fetchLogUpdate'; callId: number; logId: number; status?: number; durationMs?: number; responseBody?: string; error?: string; responseHeaders?: Record<string, string> }
  | { type: 'controlFlowGraphResult'; functionName: string; graph: unknown | null }
  | { type: 'cursorContext'; context: unknown }
  | { type: 'runtimeEvent'; data: string; callId: number };

/** Client → Server message shapes (must match playground_ws.rs WsInMessage) */
type WsInMessage =
  | { type: 'callFunction'; id: number; project: string; name: string; argsProto: string }
  | { type: 'cancelCall'; id: number; project: string }
  | { type: 'callTestFunction'; id: number; project: string; generation: number; testName: string }
  | { type: 'expandTestSet'; project: string; generation: number; testsetName: string }
  | { type: 'envVarResponse'; id: number; value: string | undefined; variable?: string }
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
  private decorationsByLine = new Map<number, { level: LogLevel; message: string; count: number }>();
  private textEncoder = new TextEncoder();

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
    this.reconnectDelay = Math.min(this.reconnectDelay * 2, MAX_RECONNECT_DELAY);
  }

  postMessage(msg: WorkerInMessage): void {
    const serverMsg = this.toServer(msg);
    if (!serverMsg) return;
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
  // Convert WorkerInMessage → WsInMessage (base64-encode argsProto)
  // ---------------------------------------------------------------------------

  private toServer(msg: WorkerInMessage): WsInMessage | null {
    switch (msg.type) {
      case 'callFunction':
        this.clearLogDecorations();
        return {
          type: 'callFunction',
          id: msg.id,
          project: msg.project,
          name: msg.name,
          argsProto: uint8ArrayToBase64(msg.argsProto),
        };
      case 'cancelCall':
        return { type: 'cancelCall', id: msg.id, project: msg.project };
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
      case 'callTestFunction':
        this.clearLogDecorations();
        return {
          type: 'callTestFunction',
          id: msg.id,
          project: msg.project,
          generation: msg.generation,
          testName: msg.testName,
        };
      case 'expandTestSet':
        return {
          type: 'expandTestSet',
          project: msg.project,
          generation: msg.generation,
          testsetName: msg.testsetName,
        };
      case 'inputResponse':
        return { type: 'inputResponse', id: msg.id, value: msg.value, callId: msg.callId };
      case 'clearHandles':
        return null; // handles live in the Rust process; no TS-side cleanup needed
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
      case 'ready':
        return { type: 'ready' };
      case 'playgroundNotification':
        return { type: 'playgroundNotification', notification: raw.notification };
      case 'callFunctionResult': {
        try {
          const bytes = base64ToUint8Array(raw.result);
          const decoded = decodeCallResult(bytes, (key, handleType, typeName) => ({
            handle_key: key,
            handle_type: handleType,
            type_name: typeName,
          }));
          return {
            type: 'callFunctionResult',
            id: raw.id,
            result: decoded,
          };
        } catch (e) {
          return {
            type: 'callFunctionError',
            id: raw.id,
            error: `Failed to decode result: ${e instanceof Error ? e.message : String(e)}`,
          };
        }
      }
      case 'callFunctionError':
        return { type: 'callFunctionError', id: raw.id, error: raw.error, cancelled: raw.cancelled };
      case 'envVarRequest':
        return { type: 'envVarRequest', id: raw.id, variable: raw.variable };
      case 'processEnvVars':
        return { type: 'processEnvVars', vars: raw.vars };
      case 'envVarFromShell':
        return { type: 'envVarFromShell', variable: raw.variable, value: raw.value };
      case 'knownEnvVarNames':
        return { type: 'knownEnvVarNames', names: raw.names };
      case 'inputRequest':
        return { type: 'inputRequest', id: raw.id, prompt: raw.prompt, callId: raw.callId };
      case 'inputResolved':
        return { type: 'inputResolved', id: raw.id, callId: raw.callId };
      case 'fetchLogNew':
        return {
          type: 'fetchLogNew',
          entry: {
            id: raw.id,
            callId: raw.callId,
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
            ...(raw.durationMs !== undefined ? { durationMs: raw.durationMs } : {}),
            ...(raw.responseBody !== undefined ? { responseBody: raw.responseBody } : {}),
            ...(raw.error !== undefined ? { error: raw.error } : {}),
            ...(raw.responseHeaders !== undefined ? { responseHeaders: raw.responseHeaders } : {}),
          },
        };
      case 'controlFlowGraphResult':
        return {
          type: 'controlFlowGraphResult',
          functionName: raw.functionName,
          graph: (raw.graph ?? null) as import('../worker-protocol').ControlFlowGraph | null,
        };
      case 'cursorContext':
        return {
          type: 'cursorContext',
          context: raw.context as import('../worker-protocol').CursorContext,
        };
      case 'runtimeEvent': {
        try {
          const bytes = base64ToUint8Array(raw.data);
          const event = RuntimeEvent.decode(bytes);
          const deserialized = deserializeRuntimeEvent(event);
          // Forward the decoded event via the buffer-or-dispatch path
          this.deliver({ type: 'runtimeEventNew', event: deserialized, callId: raw.callId ?? null });

          // Extract log decorations (same logic as baml-lsp-worker.ts)
          const kind = deserialized.event;
          if (kind?.$case === 'log' && kind.log.source) {
            const source = kind.log.source;
            const line = source.line;
            const level = normalizeLogLevel(kind.log.level);
            const message = formatValue(kind.log.data, 'inline-hint');
            const sourceSpanLength = source.endOffset - source.startOffset;
            // Compare UTF-8 byte lengths since source offsets are byte offsets from Rust
            const messageByteLen = this.textEncoder.encode(message).length;
            const isLikelyVariable = messageByteLen > sourceSpanLength + 5;
            if (isLikelyVariable) {
              const existing = this.decorationsByLine.get(line);
              if (existing) {
                existing.message = message;
                existing.level = level;
                existing.count += 1;
              } else {
                this.decorationsByLine.set(line, { level, message, count: 1 });
              }
              this.emitLogDecorations();
            }
          }
          return null; // already dispatched via handlers above
        } catch (e) {
          return {
            type: 'runtimeEventError' as const,
            error: `Failed to decode runtime event: ${e instanceof Error ? e.message : String(e)}`,
          } as WorkerOutMessage;
        }
      }
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

  private emitLogDecorations(): void {
    const decorations: LogDecoration[] = [];
    for (const [line, entry] of this.decorationsByLine) {
      decorations.push({
        line,
        level: entry.level,
        message: truncateMessage(entry.message),
        count: entry.count,
      });
    }
    this.deliver({ type: 'logDecorations', decorations });
  }

  private clearLogDecorations(): void {
    this.decorationsByLine.clear();
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

function base64ToUint8Array(b64: string): Uint8Array {
  const binary = atob(b64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) {
    bytes[i] = binary.charCodeAt(i);
  }
  return bytes;
}
