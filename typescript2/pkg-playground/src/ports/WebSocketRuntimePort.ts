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
  WebSocketInMessage,
  WebSocketOutMessage,
} from '../worker-protocol';
import {
  isPlaygroundProtocolCompatible,
  parseSourcePositionEncoding,
  type SourcePositionEncoding,
} from '../protocol';

const MAX_RECONNECT_DELAY = 5000;

/**
 * Connection lifecycle as observable from the browser: `connecting` until the
 * first handshake resolves, `open` while connected, `retrying` once a
 * handshake has failed or the socket closed and the backoff loop is running.
 * (Browsers hide the HTTP status of a failed WS upgrade, so a 401 for a
 * missing session token is indistinguishable from the server being down.)
 */
export type WebSocketRuntimePortStatus = 'connecting' | 'open' | 'retrying';

export class WebSocketRuntimePort implements RuntimePort {
  private url: string;
  private ws: WebSocket | null = null;
  private handlers = new Set<(msg: WorkerOutMessage) => void>();
  private inBuffer: WorkerOutMessage[] = [];
  private disposed = false;
  private reconnectDelay = 500;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private playgroundCompatible = false;
  private handshakeComplete = false;
  private runtimeDemandSupported: boolean | undefined;
  private serverSessionEpoch: number | undefined;
  private _sourcePositionEncoding: SourcePositionEncoding | undefined;
  /** The one selected-project lease this browser session wants to hold. */
  private desiredRuntimeLease: Extract<
    WorkerInMessage,
    { type: 'ensureProjectRuntime' }
  > | null = null;
  private status: WebSocketRuntimePortStatus = 'connecting';
  private statusHandlers = new Set<(status: WebSocketRuntimePortStatus) => void>();

  constructor(url: string) {
    this.url = url;
    this.connect();
  }

  get sourcePositionEncoding(): SourcePositionEncoding | undefined {
    return this._sourcePositionEncoding;
  }

  private connect(): void {
    if (this.disposed) return;

    // A reconnect performs a fresh handshake; do not retain capabilities from
    // the previous server instance while waiting for its replacement.
    this._sourcePositionEncoding = undefined;
    this.runtimeDemandSupported = undefined;
    this.serverSessionEpoch = undefined;
    this.playgroundCompatible = false;
    this.handshakeComplete = false;

    let socket: WebSocket;
    try {
      socket = new WebSocket(this.url);
      this.ws = socket;
    } catch {
      this.scheduleReconnect();
      return;
    }

    socket.onopen = () => {
      if (this.ws !== socket) return;
      this.setStatus('open');
      this.reconnectDelay = 500; // reset backoff
      // State requests are catalog-only. Re-establishing demand is an explicit
      // message after the hello confirms the server supports protocol v3.
      socket.send(JSON.stringify({ type: 'requestState' }));
    };

    socket.onmessage = (event: MessageEvent) => {
      if (this.ws !== socket) return;
      try {
        const raw: WebSocketOutMessage = JSON.parse(event.data as string);
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

    socket.onclose = () => {
      if (this.ws !== socket) return;
      // Tombstone this socket immediately. A queued callback from the closed
      // connection must not be mistaken for output from the reconnect that
      // will be installed after the backoff.
      this.ws = null;
      this._sourcePositionEncoding = undefined;
      this.handshakeComplete = false;
      this.playgroundCompatible = false;
      this.runtimeDemandSupported = undefined;
      this.serverSessionEpoch = undefined;
      if (!this.disposed) {
        this.scheduleReconnect();
      }
    };

    socket.onerror = () => {
      // onclose will fire after onerror, which triggers reconnect.
    };
  }

  private scheduleReconnect(): void {
    if (this.disposed || this.reconnectTimer) return;
    this.setStatus('retrying');
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
    if (msg.type === 'ensureProjectRuntime') {
      this.desiredRuntimeLease = msg;
    } else if (msg.type === 'releaseProjectRuntime') {
      if (
        this.desiredRuntimeLease?.project === msg.project &&
        this.desiredRuntimeLease.incarnation === msg.incarnation
      ) {
        this.desiredRuntimeLease = null;
      }
    }

    // Every successful open issues one forced state request after the socket
    // subscription exists, so pre-open callers do not need to accumulate
    // duplicate requestState frames in the reconnect queue.
    if (
      msg.type === 'requestState' &&
      (!this.ws || this.ws.readyState !== WebSocket.OPEN)
    ) {
      return;
    }

    const serverMsg = this.toServer(msg);
    if (!serverMsg) return;

    // Lease controls describe desired session state, so stale transitions must
    // never accumulate in the generic reconnect queue. `connect()` restores
    // exactly the latest lease after requesting the catalog.
    if (
      (msg.type === 'ensureProjectRuntime' ||
        msg.type === 'releaseProjectRuntime' ||
        msg.type === 'retryProjectRuntime') &&
      (!this.ws ||
        this.ws.readyState !== WebSocket.OPEN ||
        this.runtimeDemandSupported !== true)
    ) {
      return;
    }
    // Commands are session-scoped. Never queue them across a disconnect, and
    // do not let them race the hello that establishes the new session. The
    // catalog-only requestState frame is the sole pre-handshake exception.
    if (
      !this.ws ||
      this.ws.readyState !== WebSocket.OPEN ||
      (!this.handshakeComplete && serverMsg.type !== 'requestState') ||
      (this.handshakeComplete && !this.playgroundCompatible)
    ) {
      return;
    }
    this.ws.send(JSON.stringify(serverMsg));
  }

  private setStatus(status: WebSocketRuntimePortStatus): void {
    if (this.status === status) return;
    this.status = status;
    for (const h of this.statusHandlers) h(status);
  }

  /**
   * Observe connection status changes. The handler is invoked immediately
   * with the current status, then on every transition. Returns unsubscribe.
   */
  onStatusChange(handler: (status: WebSocketRuntimePortStatus) => void): () => void {
    this.statusHandlers.add(handler);
    handler(this.status);
    return () => {
      this.statusHandlers.delete(handler);
    };
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
    this.statusHandlers.clear();
    this.inBuffer = [];
    this.desiredRuntimeLease = null;
  }

  // ---------------------------------------------------------------------------
  // Convert WorkerInMessage → WsInMessage (base64-encode argsBytes)
  // ---------------------------------------------------------------------------

  private toServer(msg: WorkerInMessage): WebSocketInMessage | null {
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
        return { type: 'cancelRun', requestId: msg.requestId, boundaryId: msg.boundaryId };
      case 'respondToInput':
        return {
          type: 'respondToInput',
          requestId: msg.requestId,
          boundaryId: msg.boundaryId,
          inputRequestId: msg.inputRequestId,
          value: msg.value,
        };
      case 'respondToEnv':
        return {
          type: 'respondToEnv',
          requestId: msg.requestId,
          boundaryId: msg.boundaryId,
          envRequestId: msg.envRequestId,
          value: msg.value,
        };
      case 'listRuns':
        return { type: 'listRuns', requestId: msg.requestId, filter: msg.filter };
      case 'listHistory':
        return { type: 'listHistory', requestId: msg.requestId, filter: msg.filter };
      case 'openHistory':
        return { type: 'openHistory', requestId: msg.requestId, boundaryId: msg.boundaryId };
      case 'snapshot':
        return { type: 'snapshot', requestId: msg.requestId, boundaryId: msg.boundaryId };
      case 'readValue':
        return {
          type: 'readValue',
          requestId: msg.requestId,
          boundaryId: msg.boundaryId,
          valueRef: msg.valueRef,
        };
      case 'subscribe':
        return {
          type: 'subscribe',
          requestId: msg.requestId,
          subscriptionId: msg.subscriptionId,
          boundaryId: msg.boundaryId,
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
      case 'ensureProjectRuntime':
        return {
          type: 'ensureProjectRuntime',
          requestId: msg.requestId,
          project: msg.project,
          incarnation: msg.incarnation,
        };
      case 'releaseProjectRuntime':
        return {
          type: 'releaseProjectRuntime',
          requestId: msg.requestId,
          project: msg.project,
          incarnation: msg.incarnation,
        };
      case 'retryProjectRuntime':
        return {
          type: 'retryProjectRuntime',
          requestId: msg.requestId,
          project: msg.project,
          incarnation: msg.incarnation,
        };
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
  // Convert WebSocketOutMessage → WorkerOutMessage.
  // ---------------------------------------------------------------------------

  private fromServer(raw: WebSocketOutMessage): WorkerOutMessage | null {
    switch (raw.type) {
      case 'hello':
        this.desiredRuntimeLease = null;
        // Drop anything buffered for a handler from the prior connection
        // before publishing the replacement-session boundary.
        this.inBuffer = [];
        this._sourcePositionEncoding = parseSourcePositionEncoding(
          raw.sourcePositionEncoding,
        );
        this.playgroundCompatible = isPlaygroundProtocolCompatible(
          raw.playgroundProtocol,
          raw.minClientPlaygroundProtocol,
        );
        this.runtimeDemandSupported =
          this.playgroundCompatible && raw.playgroundProtocol >= 3;
        this.serverSessionEpoch = raw.sessionEpoch;
        this.handshakeComplete = true;
        if (!this.playgroundCompatible) {
          console.warn(
            `BAML playground protocol ${raw.playgroundProtocol} from toolchain ${raw.toolchainVersion} is incompatible with this extension.`,
          );
        }
        this.deliver({
          type: 'runtimeSessionReset',
          sessionEpoch: raw.sessionEpoch,
        });
        return null;
      default:
        // Fail closed until a compatible hello establishes this connection.
        if (!this.handshakeComplete || !this.playgroundCompatible) return null;
        break;
    }

    switch (raw.type) {
      case 'ready':
        return { type: 'ready' };
      case 'projectRuntimeState':
        return {
          type: 'projectRuntimeState',
          requestId: raw.requestId,
          project: raw.project,
          state: raw.state,
        };
      case 'playgroundNotification':
        if (
          isProjectDerivedNotification(raw.notification) &&
          projectDerivedSessionEpoch(raw.notification) !==
            this.serverSessionEpoch
        ) {
          return null;
        }
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
          boundaryId: raw.boundaryId,
          snapshot: raw.snapshot,
        };
      case 'valueBody':
        return {
          type: 'valueBody',
          requestId: raw.requestId,
          boundaryId: raw.boundaryId,
          valueRefId: raw.valueRefId,
          codec: raw.codec,
          availability: raw.availability,
          bodyBase64: raw.bodyBase64,
          diagnostic: raw.diagnostic,
        };
      case 'runCursorExpired':
        return {
          type: 'runCursorExpired',
          requestId: raw.requestId,
          subscriptionId: raw.subscriptionId,
          boundaryId: raw.boundaryId,
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
        if (raw.sessionEpoch !== this.serverSessionEpoch) return null;
        return {
          type: 'controlFlowGraphResult',
          sessionEpoch: raw.sessionEpoch,
          project: raw.project,
          projectIncarnation: raw.projectIncarnation,
          sourceRevision: raw.sourceRevision,
          generation: raw.generation,
          derivedEpoch: raw.derivedEpoch,
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

function projectDerivedSessionEpoch(
  notification: import('../worker-protocol').PlaygroundNotification,
): number | undefined {
  switch (notification.type) {
    case 'listProjects':
    case 'updateProject':
    case 'controlFlowGraphResult':
    case 'testCollectionResult':
      return notification.sessionEpoch;
    default:
      return undefined;
  }
}

function isProjectDerivedNotification(
  notification: import('../worker-protocol').PlaygroundNotification,
): boolean {
  return (
    notification.type === 'listProjects' ||
    notification.type === 'updateProject' ||
    notification.type === 'controlFlowGraphResult' ||
    notification.type === 'testCollectionResult'
  );
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
