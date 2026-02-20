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
import type { WorkerOutMessage, WorkerInMessage, PlaygroundNotification } from '../worker-protocol';

/** Server → Client message shapes (must match playground_ws.rs WsOutMessage) */
type WsOutMessage =
  | { type: 'ready' }
  | { type: 'playgroundNotification'; notification: PlaygroundNotification }
  | { type: 'callFunctionResult'; id: number; result: string }
  | { type: 'callFunctionError'; id: number; error: string }
  | { type: 'envVarRequest'; id: number; variable: string }
  | { type: 'fetchLogNew'; callId: number; id: number; method: string; url: string; requestHeaders: Record<string, string>; requestBody: string }
  | { type: 'fetchLogUpdate'; callId: number; logId: number; status?: number; durationMs?: number; responseBody?: string; error?: string };

/** Client → Server message shapes (must match playground_ws.rs WsInMessage) */
type WsInMessage =
  | { type: 'callFunction'; id: number; project: string; name: string; argsProto: string }
  | { type: 'envVarResponse'; id: number; value: string | undefined; variable?: string }
  | { type: 'requestState' };

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
        return {
          type: 'callFunction',
          id: msg.id,
          project: msg.project,
          name: msg.name,
          argsProto: uint8ArrayToBase64(msg.argsProto),
        };
      case 'envVarResponse':
        return {
          type: 'envVarResponse',
          id: msg.id,
          value: msg.value,
          variable: msg.variable,
        };
      case 'setEnvVar':
        return null; // UI cache only — not sent to server
      case 'deleteEnvVar':
        return null; // UI cache only — not sent to server
      case 'selectProject':
        return null; // handled locally for now
      case 'filesChanged':
        return null; // handled locally, not sent to server
      case 'requestState':
        return { type: 'requestState' };
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
      case 'callFunctionResult':
        return {
          type: 'callFunctionResult',
          id: raw.id,
          result: base64ToUint8Array(raw.result),
        };
      case 'callFunctionError':
        return { type: 'callFunctionError', id: raw.id, error: raw.error };
      case 'envVarRequest':
        return { type: 'envVarRequest', id: raw.id, variable: raw.variable };
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
          },
        };
      default:
        return null;
    }
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
