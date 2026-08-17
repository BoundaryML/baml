/**
 * WsObserveClient — client for the `/api/obs` binary observability WebSocket
 * (design §9.3). The client sends small JSON control messages; the server
 * replies with BINARY messages, one BQF1 frame each, `request_id` echoing the
 * JSON `id`.
 *
 * Protocol:
 *   {"op":"query","id":N,"method":"runs"}
 *   {"op":"query","id":N,"method":"run_meta"|"timeline"|"left_heavy"|
 *     "top_functions","run":"<key>","pixel_width":1024,"limit":50}
 *   {"op":"sub","id":N,...same fields...}   → server pushes fresh frames
 *   {"op":"unsub","id":N}
 *
 * Mirrors `WebSocketRuntimePort` transport behavior (same origin/auth story
 * as `/api/ws`: loopback binding + origin checks server-side, no token):
 * auto-reconnect with exponential backoff, and subscriptions are standing
 * intent — they are re-sent after every reconnect.
 */

import { asStatus, type BqfFrame, decodeFrame, FrameKind } from './bqf1';

const MAX_RECONNECT_DELAY = 5000;

export type ObsQueryMethod =
  | 'runs'
  | 'run_meta'
  | 'timeline'
  | 'left_heavy'
  | 'top_functions'
  | 'recent_calls'
  | 'bql';

export interface ObsQueryParams {
  /** Run key — required by every method except `runs`. */
  run?: string;
  /** Viewport width hint for LOD folds (server caps at 8192). */
  pixelWidth?: number;
  /** Row limit (e.g. top_functions). */
  limit?: number;
  /** BQL pipeline (method `bql` only, query-op only). */
  q?: string;
}

interface PendingQuery {
  resolve: (frame: BqfFrame) => void;
  reject: (error: Error) => void;
  /** Wire message; held until the socket opens if issued while connecting. */
  msg: Record<string, unknown>;
  sent: boolean;
}

interface Subscription {
  method: ObsQueryMethod;
  params: ObsQueryParams;
  cb: (frame: BqfFrame) => void;
}

/**
 * Derive the `/api/obs` URL the same way the playground derives `/api/ws`
 * (app-vscode-webview/src/App.tsx): the extension may inject
 * `__PLAYGROUND_WS_URL` (pointing at `/api/ws`); otherwise same-origin.
 */
export function defaultObsUrl(): string {
  const injected = (globalThis as { window?: { __PLAYGROUND_WS_URL?: string } })
    .window?.__PLAYGROUND_WS_URL;
  if (injected) return injected.replace(/\/api\/ws\/?$/, '/api/obs');
  const scheme = window.location.protocol === 'https:' ? 'wss' : 'ws';
  return `${scheme}://${window.location.host}/api/obs`;
}

export class WsObserveClient {
  private readonly urlFactory: () => string;
  private ws: WebSocket | null = null;
  private disposed = false;
  private reconnectDelay = 500;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private nextId = 1;
  private pendingQueries = new Map<number, PendingQuery>();
  private subscriptions = new Map<number, Subscription>();
  private connectionHandlers = new Set<(connected: boolean) => void>();

  /**
   * @param urlFactory Produces the `/api/obs` URL; called on every
   *   (re)connect. Pass `defaultObsUrl` for the playground's own derivation.
   */
  constructor(urlFactory: () => string = defaultObsUrl) {
    this.urlFactory = urlFactory;
    this.connect();
  }

  /** True while the socket is open. */
  get connected(): boolean {
    return this.ws !== null && this.ws.readyState === WebSocket.OPEN;
  }

  /** Observe connection state (invoked immediately with the current state). */
  onConnectionChange(handler: (connected: boolean) => void): () => void {
    this.connectionHandlers.add(handler);
    handler(this.connected);
    return () => {
      this.connectionHandlers.delete(handler);
    };
  }

  /**
   * One-shot query. Resolves with the frame whose `request_id` echoes the
   * allocated id; rejects on a Status error frame, disconnect, or dispose.
   * Queries issued while the socket is still connecting are held and sent
   * on open; a close rejects everything in flight (no cross-session replay).
   */
  query(
    method: ObsQueryMethod,
    params: ObsQueryParams = {},
  ): Promise<BqfFrame> {
    if (this.disposed) {
      return Promise.reject(new Error('WsObserveClient disposed'));
    }
    const id = this.nextId++;
    const msg = { id, op: 'query', ...wireParams(method, params) };
    return new Promise<BqfFrame>((resolve, reject) => {
      const pending: PendingQuery = { msg, reject, resolve, sent: false };
      this.pendingQueries.set(id, pending);
      if (this.connected) {
        pending.sent = true;
        this.send(msg);
      }
    });
  }

  /**
   * Standing subscription: the server pushes a fresh frame whenever data
   * changes. Survives reconnects (re-sent after every reconnect). Returns an
   * unsubscribe function that sends `unsub`.
   */
  subscribe(
    method: ObsQueryMethod,
    params: ObsQueryParams,
    cb: (frame: BqfFrame) => void,
  ): () => void {
    const id = this.nextId++;
    this.subscriptions.set(id, { cb, method, params });
    if (this.connected) {
      this.send({ id, op: 'sub', ...wireParams(method, params) });
    }
    return () => {
      if (!this.subscriptions.delete(id)) return;
      if (this.connected) this.send({ id, op: 'unsub' });
    };
  }

  dispose(): void {
    this.disposed = true;
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
    if (this.ws) {
      this.ws.onclose = null; // prevent reconnect
      this.ws.close();
      this.ws = null;
    }
    this.failPendingQueries('WsObserveClient disposed');
    this.subscriptions.clear();
    this.connectionHandlers.clear();
  }

  // -------------------------------------------------------------------------

  private connect(): void {
    if (this.disposed) return;

    let socket: WebSocket;
    try {
      socket = new WebSocket(this.urlFactory());
      socket.binaryType = 'arraybuffer';
      this.ws = socket;
    } catch {
      this.scheduleReconnect();
      return;
    }

    socket.onopen = () => {
      if (this.ws !== socket) return;
      this.reconnectDelay = 500; // reset backoff
      // Subscriptions are standing intent: re-assert every one of them on
      // every (re)connect so live views resume without caller involvement.
      for (const [id, sub] of this.subscriptions) {
        this.send({ id, op: 'sub', ...wireParams(sub.method, sub.params) });
      }
      // Flush queries that were issued while the socket was connecting.
      for (const pending of this.pendingQueries.values()) {
        if (!pending.sent) {
          pending.sent = true;
          this.send(pending.msg);
        }
      }
      this.notifyConnection(true);
    };

    socket.onmessage = (event: MessageEvent) => {
      if (this.ws !== socket) return;
      if (!(event.data instanceof ArrayBuffer)) return; // binary-only protocol
      let frame: BqfFrame;
      try {
        frame = decodeFrame(event.data);
      } catch (error) {
        console.warn('WsObserveClient: dropped undecodable frame', error);
        return;
      }
      this.route(frame);
    };

    socket.onclose = () => {
      if (this.ws !== socket) return;
      this.ws = null;
      // In-flight queries must fail fast, not replay into a future session.
      this.failPendingQueries('observability connection closed');
      this.notifyConnection(false);
      if (!this.disposed) this.scheduleReconnect();
    };

    socket.onerror = () => {
      // onclose fires after onerror, which triggers the reconnect.
    };
  }

  private route(frame: BqfFrame): void {
    const pending = this.pendingQueries.get(frame.requestId);
    if (pending) {
      this.pendingQueries.delete(frame.requestId);
      if (frame.kind === FrameKind.Status) {
        pending.reject(new Error(statusMessage(frame)));
      } else {
        pending.resolve(frame);
      }
      return;
    }
    const sub = this.subscriptions.get(frame.requestId);
    if (sub) sub.cb(frame);
  }

  private send(msg: Record<string, unknown>): void {
    if (this.ws?.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify(msg));
    }
  }

  private failPendingQueries(reason: string): void {
    const pending = [...this.pendingQueries.values()];
    this.pendingQueries.clear();
    for (const p of pending) p.reject(new Error(reason));
  }

  private notifyConnection(connected: boolean): void {
    for (const handler of this.connectionHandlers) handler(connected);
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
}

function wireParams(
  method: ObsQueryMethod,
  params: ObsQueryParams,
): Record<string, unknown> {
  const out: Record<string, unknown> = { method };
  if (params.run !== undefined) out.run = params.run;
  if (params.pixelWidth !== undefined) out.pixel_width = params.pixelWidth;
  if (params.limit !== undefined) out.limit = params.limit;
  if (params.q !== undefined) out.q = params.q;
  return out;
}

function statusMessage(frame: BqfFrame): string {
  try {
    const status = asStatus(frame);
    return status.message[0] ?? `observability error ${status.code[0] ?? ''}`;
  } catch {
    return 'observability error';
  }
}
