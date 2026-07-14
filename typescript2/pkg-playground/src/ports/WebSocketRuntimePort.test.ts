import { afterEach, describe, expect, it, vi } from 'vitest';

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

  parsedSent(): unknown[] {
    return this.sent.map((raw) => JSON.parse(raw) as unknown);
  }
}

function compatibleHello(socket: FakeWebSocket): void {
  socket.receive({
    type: 'hello',
    toolchainVersion: 'test',
    playgroundProtocol: 2,
    minClientPlaygroundProtocol: 2,
    capabilities: [],
  });
}

function startRunMessage(requestId: number) {
  return {
    type: 'startRun' as const,
    requestId,
    project: '/project',
    functionName: 'Extract',
    argsBytes: new Uint8Array([1]),
  };
}

const SENT_START_RUN = (requestId: number) => ({
  type: 'startRun',
  requestId,
  project: '/project',
  functionName: 'Extract',
  argsBytes: 'AQ==',
});

describe('WebSocketRuntimePort handshake and command gating', () => {
  afterEach(() => {
    FakeWebSocket.instances = [];
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it('requests catalog state without sending anything else on open', () => {
    vi.stubGlobal('WebSocket', FakeWebSocket);
    const port = new WebSocketRuntimePort('ws://playground.test/api/ws');
    const socket = FakeWebSocket.instances[0]!;

    socket.open();

    expect(socket.parsedSent()).toEqual([{ type: 'requestState' }]);
    port.dispose();
  });

  it('holds startup commands and flushes them only after a compatible hello', () => {
    vi.stubGlobal('WebSocket', FakeWebSocket);
    const port = new WebSocketRuntimePort('ws://playground.test/api/ws');
    const socket = FakeWebSocket.instances[0]!;

    // Issued before the socket even opened.
    port.postMessage(startRunMessage(30));
    socket.open();
    // Issued while open but before the hello.
    port.postMessage(startRunMessage(31));
    expect(socket.parsedSent()).toEqual([{ type: 'requestState' }]);

    compatibleHello(socket);
    expect(socket.parsedSent()).toEqual([
      { type: 'requestState' },
      SENT_START_RUN(30),
      SENT_START_RUN(31),
    ]);

    // After the handshake, commands flow directly.
    port.postMessage(startRunMessage(32));
    expect(socket.parsedSent()).toHaveLength(4);
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

    expect(received).toEqual([]);
    expect(socket.parsedSent()).toEqual([{ type: 'requestState' }]);

    // Outgoing commands are refused locally instead of hanging forever.
    port.postMessage(startRunMessage(40));
    expect(socket.parsedSent()).toEqual([{ type: 'requestState' }]);
    expect(received.filter((msg) => msg.type === 'commandError')).toEqual([
      expect.objectContaining({
        type: 'commandError',
        requestId: 40,
        code: 'disconnected',
      }),
    ]);
    port.dispose();
  });

  it('drops incoming frames that precede the hello', () => {
    vi.stubGlobal('WebSocket', FakeWebSocket);
    const port = new WebSocketRuntimePort('ws://playground.test/api/ws');
    const socket = FakeWebSocket.instances[0]!;
    const received: WorkerOutMessage[] = [];
    port.onMessage((message) => received.push(message));

    socket.open();
    socket.receive({ type: 'ready' });
    expect(received).toEqual([]);

    compatibleHello(socket);
    socket.receive({ type: 'ready' });
    expect(received).toEqual([{ type: 'ready' }]);
    port.dispose();
  });

  it('never replays session-scoped commands into a new session after reconnect', () => {
    vi.useFakeTimers();
    vi.stubGlobal('WebSocket', FakeWebSocket);
    const port = new WebSocketRuntimePort('ws://playground.test/api/ws');
    const received: WorkerOutMessage[] = [];
    port.onMessage((message) => received.push(message));

    const first = FakeWebSocket.instances[0]!;
    first.open();
    compatibleHello(first);

    first.serverClose();
    // Session-scoped commands issued while disconnected fail fast…
    port.postMessage(startRunMessage(50));
    expect(received.filter((msg) => msg.type === 'commandError')).toEqual([
      expect.objectContaining({
        type: 'commandError',
        requestId: 50,
        code: 'disconnected',
      }),
    ]);

    vi.advanceTimersByTime(500);
    const second = FakeWebSocket.instances[1]!;
    second.open();
    compatibleHello(second);

    // …and are not replayed into the replacement session. The reconnect only
    // performs the full state resync.
    expect(second.parsedSent()).toEqual([{ type: 'requestState' }]);
    port.dispose();
  });

  it('bounds the pre-handshake queue and fails the overflow locally', () => {
    vi.stubGlobal('WebSocket', FakeWebSocket);
    const port = new WebSocketRuntimePort('ws://playground.test/api/ws');
    const socket = FakeWebSocket.instances[0]!;
    const received: WorkerOutMessage[] = [];
    port.onMessage((message) => received.push(message));

    for (let requestId = 0; requestId < 65; requestId++) {
      port.postMessage(startRunMessage(requestId));
    }

    // The oldest command fell off the bounded queue with a local failure.
    expect(received).toContainEqual(
      expect.objectContaining({
        type: 'commandError',
        requestId: 0,
        code: 'disconnected',
      }),
    );

    socket.open();
    compatibleHello(socket);
    const sent = socket.parsedSent();
    // requestState + the 64 retained commands.
    expect(sent).toHaveLength(65);
    expect(sent[1]).toEqual(SENT_START_RUN(1));
    expect(sent[64]).toEqual(SENT_START_RUN(64));
    port.dispose();
  });
});

describe('WebSocketRuntimePort project-runtime lease', () => {
  afterEach(() => {
    FakeWebSocket.instances = [];
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it('defers a pre-handshake lease and sends it once the hello completes', () => {
    vi.stubGlobal('WebSocket', FakeWebSocket);
    const port = new WebSocketRuntimePort('ws://playground.test/api/ws');
    const socket = FakeWebSocket.instances[0]!;

    port.postMessage({
      type: 'ensureProjectRuntime',
      requestId: 10,
      project: '/selected',
      incarnation: 3,
    });
    socket.open();
    expect(socket.parsedSent()).toEqual([{ type: 'requestState' }]);

    compatibleHello(socket);
    expect(socket.parsedSent()).toEqual([
      { type: 'requestState' },
      {
        type: 'ensureProjectRuntime',
        requestId: 10,
        project: '/selected',
        incarnation: 3,
      },
    ]);
    port.dispose();
  });

  it('re-sends the standing lease after every hello, surviving reconnects', () => {
    vi.useFakeTimers();
    vi.stubGlobal('WebSocket', FakeWebSocket);
    const port = new WebSocketRuntimePort('ws://playground.test/api/ws');

    const first = FakeWebSocket.instances[0]!;
    first.open();
    compatibleHello(first);

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
    vi.advanceTimersByTime(500);
    const second = FakeWebSocket.instances[1]!;
    second.open();
    compatibleHello(second);

    // Only the LATEST lease is re-asserted; released/stale leases are not.
    expect(second.parsedSent()).toEqual([
      { type: 'requestState' },
      {
        type: 'ensureProjectRuntime',
        requestId: 22,
        project: '/selected',
        incarnation: 3,
      },
    ]);

    // …and again on the next reconnect: the lease is standing intent.
    second.serverClose();
    vi.advanceTimersByTime(1000);
    const third = FakeWebSocket.instances[2]!;
    third.open();
    compatibleHello(third);
    expect(third.parsedSent()).toEqual([
      { type: 'requestState' },
      {
        type: 'ensureProjectRuntime',
        requestId: 22,
        project: '/selected',
        incarnation: 3,
      },
    ]);
    port.dispose();
  });

  it('does not replay a lease that was released, even while disconnected', () => {
    vi.useFakeTimers();
    vi.stubGlobal('WebSocket', FakeWebSocket);
    const port = new WebSocketRuntimePort('ws://playground.test/api/ws');

    const first = FakeWebSocket.instances[0]!;
    first.open();
    compatibleHello(first);
    port.postMessage({
      type: 'ensureProjectRuntime',
      requestId: 30,
      project: '/selected',
      incarnation: 5,
    });

    first.serverClose();
    // The release arrives while disconnected: it must still retire the lease.
    port.postMessage({
      type: 'releaseProjectRuntime',
      requestId: 31,
      project: '/selected',
      incarnation: 5,
    });

    vi.advanceTimersByTime(500);
    const second = FakeWebSocket.instances[1]!;
    second.open();
    compatibleHello(second);

    expect(second.parsedSent()).toEqual([{ type: 'requestState' }]);
    port.dispose();
  });

  it('keeps the lease when a release names a different incarnation', () => {
    vi.stubGlobal('WebSocket', FakeWebSocket);
    const port = new WebSocketRuntimePort('ws://playground.test/api/ws');
    const socket = FakeWebSocket.instances[0]!;
    socket.open();
    compatibleHello(socket);
    socket.sent.length = 0;

    port.postMessage({
      type: 'ensureProjectRuntime',
      requestId: 40,
      project: '/selected',
      incarnation: 7,
    });
    port.postMessage({
      type: 'releaseProjectRuntime',
      requestId: 41,
      project: '/selected',
      incarnation: 6,
    });

    expect(socket.parsedSent()).toEqual([
      {
        type: 'ensureProjectRuntime',
        requestId: 40,
        project: '/selected',
        incarnation: 7,
      },
      {
        type: 'releaseProjectRuntime',
        requestId: 41,
        project: '/selected',
        incarnation: 6,
      },
    ]);
    port.dispose();
  });
});
