/**
 * RuntimePort backed by a WebSocket connection to the Rust playground server.
 *
 * Used in the VS Code webview where the Rust LSP server runs the BAML runtime.
 * Communicates over ws://localhost:{port}/api/ws with JSON messages.
 * Argument/result bytes are base64-encoded for transit.
 *
 * Features:
 *   - Fail-closed handshake: nothing is processed or sent (except the
 *     catalog-only `requestState`) until the server `hello` proves protocol
 *     compatibility
 *   - Bounded queue for commands issued before the FIRST handshake; once a
 *     session existed, session-scoped commands are never queued across a
 *     disconnect — they fail fast and the client resyncs after reconnect
 *   - The project-runtime lease (`ensureProjectRuntime`) is standing intent:
 *     the latest lease is re-asserted after every successful hello
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
import { isPlaygroundProtocolCompatible } from '../protocol';

const MAX_RECONNECT_DELAY = 5000;

/** Upper bound on commands held while waiting for the first handshake. */
const MAX_PRE_SESSION_QUEUE = 64;

/** Synthetic command-error code for commands dropped by the transport. */
export const PORT_DISCONNECTED_ERROR_CODE = 'disconnected';

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
  /** Commands held until the first compatible hello (never across sessions). */
  private outQueue: WebSocketInMessage[] = [];
  private inBuffer: WorkerOutMessage[] = [];
  private disposed = false;
  private reconnectDelay = 500;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  /** Fail closed: assume incompatible until a hello proves otherwise. */
  private playgroundCompatible = false;
  private handshakeComplete = false;
  /** True once any hello completed; after that, disconnected commands are
   *  dropped instead of queued so they can never replay into a new session. */
  private everHadSession = false;
  /** The one selected-project runtime lease this client wants to hold. This is
   *  desired state, not a one-shot command: it survives reconnects and is
   *  re-sent after every successful hello. */
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

  private connect(): void {
    if (this.disposed) return;

    // A reconnect performs a fresh handshake; do not retain trust from the
    // previous server instance while waiting for its replacement.
    this.handshakeComplete = false;
    this.playgroundCompatible = false;

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
      // The catalog-only state request is the sole pre-handshake frame; the
      // full resync it triggers arrives after the server's hello, which is
      // what re-establishes the session.
      socket.send(JSON.stringify({ type: 'requestState' }));
    };

    socket.onmessage = (event: MessageEvent) => {
      if (this.ws !== socket) return;
      try {
        const raw: WebSocketOutMessage = JSON.parse(event.data as string);
        const msg = this.fromServer(raw);
        if (!msg) return;
        this.deliver(msg);
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
      this.handshakeComplete = false;
      this.playgroundCompatible = false;
      if (this.everHadSession) {
        // Session-scoped commands must not replay into the next session.
        this.dropQueuedCommands('connection closed');
      }
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

  private get sendable(): boolean {
    return (
      this.ws !== null &&
      this.ws.readyState === WebSocket.OPEN &&
      this.handshakeComplete &&
      this.playgroundCompatible
    );
  }

  postMessage(msg: WorkerInMessage): void {
    // Track the standing lease regardless of connection state; `hello`
    // replays the latest lease into every new session.
    if (msg.type === 'ensureProjectRuntime') {
      this.desiredRuntimeLease = msg;
    } else if (
      msg.type === 'releaseProjectRuntime' &&
      this.desiredRuntimeLease?.project === msg.project &&
      this.desiredRuntimeLease.incarnation === msg.incarnation
    ) {
      this.desiredRuntimeLease = null;
    }

    // Every successful open issues one state request, so pre-open callers do
    // not need to accumulate duplicate requestState frames in the queue.
    if (
      msg.type === 'requestState' &&
      (!this.ws || this.ws.readyState !== WebSocket.OPEN)
    ) {
      return;
    }

    const serverMsg = this.toServer(msg);
    if (!serverMsg) return;

    // Lease controls describe desired session state; stale transitions must
    // never sit in the generic queue. The hello handler restores exactly the
    // latest lease.
    if (
      (msg.type === 'ensureProjectRuntime' ||
        msg.type === 'releaseProjectRuntime') &&
      !this.sendable
    ) {
      return;
    }

    if (this.sendable) {
      this.ws!.send(JSON.stringify(serverMsg));
      return;
    }

    if (!this.everHadSession && !this.handshakeComplete) {
      // No session has existed yet — hold startup commands (bounded) until
      // the first compatible hello, so early callers are not lost while the
      // socket connects.
      this.outQueue.push(serverMsg);
      if (this.outQueue.length > MAX_PRE_SESSION_QUEUE) {
        const dropped = this.outQueue.shift()!;
        this.synthesizeDropError(
          dropped,
          'queue overflow before the first playground handshake',
        );
      }
      return;
    }

    // Session-scoped command while disconnected, mid-handshake on a
    // reconnect, or against an incompatible server: fail fast instead of
    // replaying it into a session it was not issued against.
    this.synthesizeDropError(serverMsg, 'playground connection unavailable');
  }

  /** Clear the pre-session queue, failing any queued request/response pairs. */
  private dropQueuedCommands(reason: string): void {
    const dropped = this.outQueue.splice(0);
    for (const msg of dropped) {
      this.synthesizeDropError(msg, reason);
    }
  }

  /** Locally reject a dropped command so pending promises fail instead of
   *  hanging. Fire-and-forget frames (no requestId) are dropped silently. */
  private synthesizeDropError(msg: WebSocketInMessage, reason: string): void {
    const requestId = (msg as { requestId?: unknown }).requestId;
    if (typeof requestId !== 'number') return;
    this.deliver({
      type: 'commandError',
      requestId,
      code: PORT_DISCONNECTED_ERROR_CODE,
      message: `Playground command dropped: ${reason}.`,
    });
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
    this.outQueue = [];
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
    if (raw.type === 'hello') {
      this.playgroundCompatible = isPlaygroundProtocolCompatible(
        raw.playgroundProtocol,
        raw.minClientPlaygroundProtocol,
      );
      this.handshakeComplete = true;
      if (this.everHadSession) {
        // A replacement session: input buffered for the old session must not
        // replay to a late-registering handler.
        this.inBuffer = [];
      }
      this.everHadSession = true;
      if (!this.playgroundCompatible) {
        console.warn(
          `BAML playground protocol ${raw.playgroundProtocol} from toolchain ${raw.toolchainVersion} is incompatible with this extension.`,
        );
        this.dropQueuedCommands('playground protocol is incompatible');
        return null;
      }
      // Re-assert the standing project-runtime lease on EVERY hello. It is
      // desired state, not a one-shot command; the replacement server session
      // starts without it. (Deliberately not cleared here — dropping the
      // saved lease after hello would lose it across reconnects.)
      if (this.desiredRuntimeLease) {
        const lease = this.toServer(this.desiredRuntimeLease);
        if (lease) this.ws?.send(JSON.stringify(lease));
      }
      // Flush commands held from before the first handshake.
      const queued = this.outQueue.splice(0);
      for (const pending of queued) {
        this.ws?.send(JSON.stringify(pending));
      }
      return null;
    }

    // Fail closed until a compatible hello establishes this connection.
    if (!this.handshakeComplete || !this.playgroundCompatible) return null;

    switch (raw.type) {
      case 'ready':
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
