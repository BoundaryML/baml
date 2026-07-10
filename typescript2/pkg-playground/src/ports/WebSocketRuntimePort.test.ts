import { afterEach, describe, expect, it, vi } from 'vitest';

import { PLAYGROUND_SOURCE_POSITION_ENCODING } from '../protocol';
import type { WorkerOutMessage } from '../worker-protocol';
import { WebSocketRuntimePort } from './WebSocketRuntimePort';

class FakeWebSocket {
  static readonly CONNECTING = 0;
  static readonly OPEN = 1;
  static readonly CLOSED = 3;
  static instances: FakeWebSocket[] = [];

  readyState = FakeWebSocket.CONNECTING;
  onopen: (() => void) | null = null;
  onmessage: ((event: MessageEvent) => void) | null = null;
  onclose: (() => void) | null = null;
  onerror: (() => void) | null = null;
  readonly sent: string[] = [];

  constructor(readonly url: string) {
    FakeWebSocket.instances.push(this);
  }

  send(data: string): void {
    this.sent.push(data);
  }

  close(): void {}

  receive(message: unknown): void {
    this.onmessage?.({ data: JSON.stringify(message) } as MessageEvent);
  }

  open(): void {
    this.readyState = FakeWebSocket.OPEN;
    this.onopen?.();
  }

  serverClose(): void {
    this.readyState = FakeWebSocket.CLOSED;
    this.onclose?.();
  }
}

function advertiseRuntimeDemand(socket: FakeWebSocket, sessionEpoch = 1): void {
  socket.receive({
    type: 'hello',
    sessionEpoch,
    toolchainVersion: 'test',
    playgroundProtocol: 3,
    minClientPlaygroundProtocol: 2,
    capabilities: [],
    sourcePositionEncoding: PLAYGROUND_SOURCE_POSITION_ENCODING,
  });
}

describe('WebSocketRuntimePort source-position handshake', () => {
  afterEach(() => {
    FakeWebSocket.instances = [];
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it('exposes the fixed UTF-16 contract only after the server advertises it', () => {
    vi.stubGlobal('WebSocket', FakeWebSocket);
    const port = new WebSocketRuntimePort('ws://playground.test/api/ws');
    const socket = FakeWebSocket.instances[0];
    expect(socket).toBeDefined();
    expect(port.sourcePositionEncoding).toBeUndefined();

    socket?.receive({
      type: 'hello',
      sessionEpoch: 1,
      toolchainVersion: 'test',
      playgroundProtocol: 3,
      minClientPlaygroundProtocol: 3,
      capabilities: [],
      sourcePositionEncoding: PLAYGROUND_SOURCE_POSITION_ENCODING,
    });

    expect(port.sourcePositionEncoding).toBe(
      PLAYGROUND_SOURCE_POSITION_ENCODING,
    );
    port.dispose();
  });

  it('keeps the legacy gate for absent or unknown encodings', () => {
    vi.stubGlobal('WebSocket', FakeWebSocket);
    const port = new WebSocketRuntimePort('ws://playground.test/api/ws');
    const socket = FakeWebSocket.instances[0];

    socket?.receive({
      type: 'hello',
      sessionEpoch: 1,
      toolchainVersion: 'old',
      playgroundProtocol: 2,
      minClientPlaygroundProtocol: 2,
      capabilities: [],
    });
    expect(port.sourcePositionEncoding).toBeUndefined();

    socket?.receive({
      type: 'hello',
      sessionEpoch: 2,
      toolchainVersion: 'future',
      playgroundProtocol: 2,
      minClientPlaygroundProtocol: 2,
      capabilities: [],
      sourcePositionEncoding: 'utf8-zero-based-v1',
    });
    expect(port.sourcePositionEncoding).toBeUndefined();
    port.dispose();
  });
});

describe('WebSocketRuntimePort runtime demand', () => {
  afterEach(() => {
    FakeWebSocket.instances = [];
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it('requests catalog state without implicitly warming any project', () => {
    vi.stubGlobal('WebSocket', FakeWebSocket);
    const port = new WebSocketRuntimePort('ws://playground.test/api/ws');
    const socket = FakeWebSocket.instances[0]!;

    socket.open();

    expect(socket.sent.map((raw) => JSON.parse(raw))).toEqual([
      { type: 'requestState' },
    ]);
    port.dispose();
  });

  it('serializes explicit ensure, release, and retry messages', () => {
    vi.stubGlobal('WebSocket', FakeWebSocket);
    const port = new WebSocketRuntimePort('ws://playground.test/api/ws');
    const socket = FakeWebSocket.instances[0]!;
    socket.open();
    advertiseRuntimeDemand(socket);
    socket.sent.length = 0;

    port.postMessage({
      type: 'ensureProjectRuntime',
      requestId: 10,
      project: '/project',
      incarnation: 7,
    });
    port.postMessage({
      type: 'retryProjectRuntime',
      requestId: 11,
      project: '/project',
      incarnation: 7,
    });
    port.postMessage({
      type: 'releaseProjectRuntime',
      requestId: 12,
      project: '/project',
      incarnation: 7,
    });

    expect(socket.sent.map((raw) => JSON.parse(raw))).toEqual([
      {
        type: 'ensureProjectRuntime',
        requestId: 10,
        project: '/project',
        incarnation: 7,
      },
      {
        type: 'retryProjectRuntime',
        requestId: 11,
        project: '/project',
        incarnation: 7,
      },
      {
        type: 'releaseProjectRuntime',
        requestId: 12,
        project: '/project',
        incarnation: 7,
      },
    ]);
    port.dispose();
  });

  it('keeps demand controls local when connected to an eager protocol-v2 server', () => {
    vi.stubGlobal('WebSocket', FakeWebSocket);
    const port = new WebSocketRuntimePort('ws://playground.test/api/ws');
    const socket = FakeWebSocket.instances[0]!;
    socket.open();
    socket.receive({
      type: 'hello',
      sessionEpoch: 1,
      toolchainVersion: 'old',
      playgroundProtocol: 2,
      minClientPlaygroundProtocol: 2,
      capabilities: [],
    });

    port.postMessage({
      type: 'ensureProjectRuntime',
      requestId: 13,
      project: '/project',
    });

    expect(socket.sent.map((raw) => JSON.parse(raw))).toEqual([
      { type: 'requestState' },
    ]);
    port.dispose();
  });

  it('drops session-scoped commands until a fresh compatible hello', () => {
    vi.stubGlobal('WebSocket', FakeWebSocket);
    const port = new WebSocketRuntimePort('ws://playground.test/api/ws');
    const socket = FakeWebSocket.instances[0]!;
    const run = {
      type: 'startRun' as const,
      requestId: 30,
      project: '/project',
      functionName: 'Extract',
      argsBytes: new Uint8Array([1]),
    };

    port.postMessage(run);
    socket.open();
    port.postMessage({ ...run, requestId: 31 });
    expect(socket.sent.map((raw) => JSON.parse(raw))).toEqual([
      { type: 'requestState' },
    ]);

    advertiseRuntimeDemand(socket);
    port.postMessage({ ...run, requestId: 32 });
    expect(socket.sent.map((raw) => JSON.parse(raw))).toEqual([
      { type: 'requestState' },
      {
        type: 'startRun',
        requestId: 32,
        project: '/project',
        functionName: 'Extract',
        argsBytes: 'AQ==',
      },
    ]);
    port.dispose();
  });

  it('fails closed for all frames after an incompatible hello', () => {
    vi.stubGlobal('WebSocket', FakeWebSocket);
    const port = new WebSocketRuntimePort('ws://playground.test/api/ws');
    const socket = FakeWebSocket.instances[0]!;
    const received: WorkerOutMessage[] = [];
    port.onMessage((message) => received.push(message));
    socket.open();
    socket.receive({
      type: 'hello',
      sessionEpoch: 1,
      toolchainVersion: 'future',
      playgroundProtocol: 99,
      minClientPlaygroundProtocol: 99,
      capabilities: [],
    });
    socket.receive({
      type: 'playgroundNotification',
      notification: { type: 'listProjects', projects: ['/stale'] },
    });
    socket.receive({ type: 'ready' });

    expect(received).toEqual([
      { type: 'runtimeSessionReset', sessionEpoch: 1 },
    ]);
    port.dispose();
  });

  it('forwards qualified runtime state responses to the frontend', () => {
    vi.stubGlobal('WebSocket', FakeWebSocket);
    const port = new WebSocketRuntimePort('ws://playground.test/api/ws');
    const socket = FakeWebSocket.instances[0]!;
    const received: unknown[] = [];
    port.onMessage((message) => received.push(message));
    socket.open();
    advertiseRuntimeDemand(socket);

    socket.receive({
      type: 'projectRuntimeState',
      requestId: 14,
      project: '/project',
      state: {
        state: 'building',
        requestedRevision: 6,
        installedRevision: 5,
        generation: 2,
        hasLastKnownGood: true,
      },
    });

    expect(received).toContainEqual({
      type: 'projectRuntimeState',
      requestId: 14,
      project: '/project',
      state: {
        state: 'building',
        requestedRevision: 6,
        installedRevision: 5,
        generation: 2,
        hasLastKnownGood: true,
      },
    });
    port.dispose();
  });

  it('drops project-derived payloads from another server session', () => {
    vi.stubGlobal('WebSocket', FakeWebSocket);
    const port = new WebSocketRuntimePort('ws://playground.test/api/ws');
    const socket = FakeWebSocket.instances[0]!;
    const received: WorkerOutMessage[] = [];
    port.onMessage((message) => received.push(message));
    socket.open();
    advertiseRuntimeDemand(socket, 7);

    socket.receive({
      type: 'playgroundNotification',
      notification: {
        type: 'listProjects',
        sessionEpoch: 6,
        projects: ['/stale'],
      },
    });
    socket.receive({
      type: 'playgroundNotification',
      notification: {
        type: 'listProjects',
        sessionEpoch: 7,
        projects: ['/current'],
      },
    });

    expect(received).toEqual([
      { type: 'runtimeSessionReset', sessionEpoch: 7 },
      {
        type: 'playgroundNotification',
        notification: {
          type: 'listProjects',
          sessionEpoch: 7,
          projects: ['/current'],
        },
      },
    ]);
    port.dispose();
  });

  it('starts a fresh frontend session instead of replaying a stale lease after reconnect', () => {
    vi.useFakeTimers();
    vi.stubGlobal('WebSocket', FakeWebSocket);
    const port = new WebSocketRuntimePort('ws://playground.test/api/ws');
    const received: unknown[] = [];
    port.onMessage((message) => received.push(message));
    const first = FakeWebSocket.instances[0]!;
    first.open();
    advertiseRuntimeDemand(first);
    first.sent.length = 0;

    port.postMessage({
      type: 'ensureProjectRuntime',
      requestId: 20,
      project: '/old',
      incarnation: 1,
    });
    port.postMessage({
      type: 'releaseProjectRuntime',
      requestId: 21,
      project: '/old',
      incarnation: 1,
    });
    port.postMessage({
      type: 'ensureProjectRuntime',
      requestId: 22,
      project: '/selected',
      incarnation: 3,
    });

    first.serverClose();
    expect(port.sourcePositionEncoding).toBeUndefined();
    first.receive({ type: 'ready' });
    port.postMessage({
      type: 'retryProjectRuntime',
      requestId: 23,
      project: '/selected',
      incarnation: 3,
    });
    vi.advanceTimersByTime(500);
    const second = FakeWebSocket.instances[1]!;
    second.open();
    advertiseRuntimeDemand(second, 2);

    expect(second.sent.map((raw) => JSON.parse(raw))).toEqual([
      { type: 'requestState' },
    ]);
    expect(received).toContainEqual({
      type: 'runtimeSessionReset',
      sessionEpoch: 1,
    });
    expect(received).toContainEqual({
      type: 'runtimeSessionReset',
      sessionEpoch: 2,
    });
    expect(received).not.toContainEqual({ type: 'ready' });
    port.dispose();
  });
});
