// biome-ignore-all lint/style/useFilenamingConvention: Preserve the existing public class filename.
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

import { isPlaygroundProtocolCompatible } from '../protocol';
import type { RuntimePort } from '../runtime-port';
import type {
  WebSocketInMessage,
  WebSocketOutMessage,
  WorkerInMessage,
  WorkerOutMessage,
} from '../worker-protocol';

const MAX_RECONNECT_DELAY = 5000;

/** Upper bound on commands held while waiting for the first handshake. */
const MAX_PRE_SESSION_QUEUE = 64;

/** Synthetic command-error code for commands dropped by the transport. */
export const PORT_DISCONNECTED_ERROR_CODE = 'disconnected';

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
      code: PORT_DISCONNECTED_ERROR_CODE,
      message: `Playground command dropped: ${reason}.`,
      requestId,
      type: 'commandError',
    });
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
          argsBytes: uint8ArrayToBase64(msg.argsBytes),
          functionName: msg.functionName,
          project: msg.project,
          requestId: msg.requestId,
          type: 'startRun',
        };
      case 'startPreviewRun':
        this.clearLogDecorations();
        return {
          argsBytes: uint8ArrayToBase64(msg.argsBytes),
          functionName: msg.functionName,
          helper: msg.helper,
          parentFunctionName: msg.parentFunctionName,
          project: msg.project,
          requestId: msg.requestId,
          type: 'startPreviewRun',
        };
      case 'startTestRun':
        this.clearLogDecorations();
        return {
          generation: msg.generation,
          project: msg.project,
          requestId: msg.requestId,
          testName: msg.testName,
          type: 'startTestRun',
        };
      case 'cancelRun':
        return {
          boundaryId: msg.boundaryId,
          requestId: msg.requestId,
          type: 'cancelRun',
        };
      case 'respondToInput':
        return {
          boundaryId: msg.boundaryId,
          inputRequestId: msg.inputRequestId,
          requestId: msg.requestId,
          type: 'respondToInput',
          value: msg.value,
        };
      case 'respondToEnv':
        return {
          boundaryId: msg.boundaryId,
          envRequestId: msg.envRequestId,
          requestId: msg.requestId,
          type: 'respondToEnv',
          value: msg.value,
        };
      case 'listRuns':
        return {
          filter: msg.filter,
          requestId: msg.requestId,
          type: 'listRuns',
        };
      case 'listHistory':
        return {
          filter: msg.filter,
          requestId: msg.requestId,
          type: 'listHistory',
        };
      case 'openHistory':
        return {
          boundaryId: msg.boundaryId,
          requestId: msg.requestId,
          type: 'openHistory',
        };
      case 'listExecutions':
        return {
          project: msg.project,
          requestId: msg.requestId,
          type: 'listExecutions',
        };
      case 'openExecution':
        return {
          executionId: msg.executionId,
          project: msg.project,
          requestId: msg.requestId,
          type: 'openExecution',
        };
      case 'readTelemetryMedia':
        return {
          cid: msg.cid,
          project: msg.project,
          requestId: msg.requestId,
          type: 'readTelemetryMedia',
        };
      case 'snapshot':
        return {
          boundaryId: msg.boundaryId,
          requestId: msg.requestId,
          type: 'snapshot',
        };
      case 'readValue':
        return {
          boundaryId: msg.boundaryId,
          requestId: msg.requestId,
          type: 'readValue',
          valueRef: msg.valueRef,
        };
      case 'subscribe':
        return {
          afterCursor: msg.afterCursor,
          boundaryId: msg.boundaryId,
          requestId: msg.requestId,
          subscriptionId: msg.subscriptionId,
          type: 'subscribe',
        };
      case 'unsubscribe':
        return {
          requestId: msg.requestId,
          subscriptionId: msg.subscriptionId,
          type: 'unsubscribe',
        };
      case 'envVarResponse':
        return {
          id: msg.id,
          type: 'envVarResponse',
          value: msg.value,
          variable: msg.variable,
        };
      case 'setEnvVar':
        return { key: msg.key, type: 'setEnvVar', value: msg.value };
      case 'deleteEnvVar':
        return { key: msg.key, type: 'deleteEnvVar' };
      case 'selectProject':
        return null; // handled locally for now
      case 'filesChanged':
        return null; // handled locally, not sent to server
      case 'requestState':
        return { type: 'requestState' };
      case 'ensureProjectRuntime':
        return {
          incarnation: msg.incarnation,
          project: msg.project,
          requestId: msg.requestId,
          type: 'ensureProjectRuntime',
        };
      case 'releaseProjectRuntime':
        return {
          incarnation: msg.incarnation,
          project: msg.project,
          requestId: msg.requestId,
          type: 'releaseProjectRuntime',
        };
      case 'requestControlFlowGraph':
        return {
          functionName: msg.functionName,
          project: msg.project,
          type: 'requestControlFlowGraph',
          ...(msg.requestId !== undefined ? { requestId: msg.requestId } : {}),
        };
      case 'cursorPosition':
        return {
          column: msg.column,
          file: msg.file,
          line: msg.line,
          type: 'cursorPosition',
        };
      case 'requestCollectTests':
        return { project: msg.project, type: 'requestCollectTests' };
      case 'expandTestSet':
        return {
          generation: msg.generation,
          project: msg.project,
          testsetName: msg.testsetName,
          type: 'expandTestSet',
        };
      case 'inputResponse':
        return {
          callId: msg.callId,
          id: msg.id,
          type: 'inputResponse',
          value: msg.value,
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
          notification: raw.notification,
          type: 'playgroundNotification',
        };
      case 'runStarted':
        return {
          requestId: raw.requestId,
          run: raw.run,
          type: 'runStarted',
        };
      case 'runPatch':
        return { patch: raw.patch, type: 'runPatch' };
      case 'commandAck':
        return {
          outcome: raw.outcome,
          requestId: raw.requestId,
          type: 'commandAck',
        };
      case 'commandError':
        return {
          code: raw.code,
          message: raw.message,
          requestId: raw.requestId,
          type: 'commandError',
        };
      case 'runList':
        return { requestId: raw.requestId, runs: raw.runs, type: 'runList' };
      case 'executionList':
        return {
          executions: raw.executions,
          requestId: raw.requestId,
          storeMissing: raw.storeMissing,
          type: 'executionList',
        };
      case 'executionTelemetry':
        return {
          executionId: raw.executionId,
          requestId: raw.requestId,
          telemetry: raw.telemetry,
          type: 'executionTelemetry',
        };
      case 'telemetryMedia':
        return {
          cid: raw.cid,
          media: raw.media,
          requestId: raw.requestId,
          type: 'telemetryMedia',
        };
      case 'runSnapshot':
        return {
          boundaryId: raw.boundaryId,
          requestId: raw.requestId,
          snapshot: raw.snapshot,
          type: 'runSnapshot',
        };
      case 'valueBody':
        return {
          availability: raw.availability,
          bodyBase64: raw.bodyBase64,
          boundaryId: raw.boundaryId,
          codec: raw.codec,
          diagnostic: raw.diagnostic,
          requestId: raw.requestId,
          type: 'valueBody',
          valueRefId: raw.valueRefId,
        };
      case 'runCursorExpired':
        return {
          boundaryId: raw.boundaryId,
          reason: raw.reason,
          requestId: raw.requestId,
          subscriptionId: raw.subscriptionId,
          type: 'runCursorExpired',
        };
      case 'envVarRequest':
        return { id: raw.id, type: 'envVarRequest', variable: raw.variable };
      case 'processEnvVars':
        return { type: 'processEnvVars', vars: raw.vars };
      case 'envVarFromShell':
        return {
          type: 'envVarFromShell',
          value: raw.value,
          variable: raw.variable,
        };
      case 'knownEnvVarNames':
        return { names: raw.names, type: 'knownEnvVarNames' };
      case 'inputRequest':
        return {
          callId: raw.callId,
          id: raw.id,
          prompt: raw.prompt,
          type: 'inputRequest',
        };
      case 'inputResolved':
        return { callId: raw.callId, id: raw.id, type: 'inputResolved' };
      case 'fetchLogNew':
        return {
          callId: raw.callId,
          entry: {
            durationMs: null,
            error: null,
            id: raw.id,
            method: raw.method,
            requestBody: raw.requestBody,
            requestHeaders: raw.requestHeaders,
            responseBody: null,
            responseHeaders: null,
            status: null,
            timestamp: Date.now(),
            url: raw.url,
          },
          type: 'fetchLogNew',
        };
      case 'fetchLogUpdate':
        return {
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
          type: 'fetchLogUpdate',
        };
      case 'controlFlowGraphResult':
        return {
          functionName: raw.functionName,
          graph: (raw.graph ?? null) as
            | import('../worker-protocol').ControlFlowGraph
            | null,
          type: 'controlFlowGraphResult',
          ...(raw.requestId !== undefined ? { requestId: raw.requestId } : {}),
        };
      case 'cursorContext':
        return {
          context: raw.context as import('../worker-protocol').CursorContext,
          type: 'cursorContext',
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
