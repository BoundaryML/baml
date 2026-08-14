// biome-ignore-all lint/style/useFilenamingConvention: Preserve the existing test filename beside its implementation.
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
    capabilities: [],
    minClientPlaygroundProtocol: 2,
    playgroundProtocol: 2,
    toolchainVersion: 'test',
    type: 'hello',
  });
}

function startRunMessage(requestId: number) {
  return {
    argsBytes: new Uint8Array([1]),
    functionName: 'Extract',
    project: '/project',
    requestId,
    type: 'startRun' as const,
  };
}

const SENT_START_RUN = (requestId: number) => ({
  argsBytes: 'AQ==',
  functionName: 'Extract',
  project: '/project',
  requestId,
  type: 'startRun',
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
      capabilities: [],
      minClientPlaygroundProtocol: 99,
      playgroundProtocol: 99,
      toolchainVersion: 'future',
      type: 'hello',
    });
    socket.receive({
      notification: { projects: ['/stale'], type: 'listProjects' },
      type: 'playgroundNotification',
    });
    socket.receive({ type: 'ready' });

    expect(received).toEqual([]);
    expect(socket.parsedSent()).toEqual([{ type: 'requestState' }]);

    // Outgoing commands are refused locally instead of hanging forever.
    port.postMessage(startRunMessage(40));
    expect(socket.parsedSent()).toEqual([{ type: 'requestState' }]);
    expect(received.filter((msg) => msg.type === 'commandError')).toEqual([
      expect.objectContaining({
        code: 'disconnected',
        requestId: 40,
        type: 'commandError',
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
        code: 'disconnected',
        requestId: 50,
        type: 'commandError',
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
        code: 'disconnected',
        requestId: 0,
        type: 'commandError',
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

describe('WebSocketRuntimePort control-flow graph correlation', () => {
  afterEach(() => {
    FakeWebSocket.instances = [];
    vi.unstubAllGlobals();
  });

  it('preserves the optional request ID in both directions', () => {
    vi.stubGlobal('WebSocket', FakeWebSocket);
    const port = new WebSocketRuntimePort('ws://playground.test/api/ws');
    const socket = FakeWebSocket.instances[0]!;
    const received: WorkerOutMessage[] = [];
    port.onMessage((message) => received.push(message));

    socket.open();
    compatibleHello(socket);
    socket.sent.length = 0;

    port.postMessage({
      functionName: 'Extract',
      project: '/project',
      requestId: 17,
      type: 'requestControlFlowGraph',
    });
    expect(socket.parsedSent()).toEqual([
      {
        functionName: 'Extract',
        project: '/project',
        requestId: 17,
        type: 'requestControlFlowGraph',
      },
    ]);

    socket.receive({
      functionName: 'Extract',
      graph: null,
      requestId: 17,
      type: 'controlFlowGraphResult',
    });
    expect(received).toEqual([
      {
        functionName: 'Extract',
        graph: null,
        requestId: 17,
        type: 'controlFlowGraphResult',
      },
    ]);
    port.dispose();
  });

  it('remains compatible when the optional request ID is omitted', () => {
    vi.stubGlobal('WebSocket', FakeWebSocket);
    const port = new WebSocketRuntimePort('ws://playground.test/api/ws');
    const socket = FakeWebSocket.instances[0]!;
    const received: WorkerOutMessage[] = [];
    port.onMessage((message) => received.push(message));

    socket.open();
    compatibleHello(socket);
    socket.sent.length = 0;

    port.postMessage({
      functionName: 'Extract',
      project: '/project',
      type: 'requestControlFlowGraph',
    });
    expect(socket.parsedSent()).toEqual([
      {
        functionName: 'Extract',
        project: '/project',
        type: 'requestControlFlowGraph',
      },
    ]);

    socket.receive({
      functionName: 'Extract',
      graph: null,
      type: 'controlFlowGraphResult',
    });
    expect(received).toEqual([
      {
        functionName: 'Extract',
        graph: null,
        type: 'controlFlowGraphResult',
      },
    ]);
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
      incarnation: 3,
      project: '/selected',
      requestId: 10,
      type: 'ensureProjectRuntime',
    });
    socket.open();
    expect(socket.parsedSent()).toEqual([{ type: 'requestState' }]);

    compatibleHello(socket);
    expect(socket.parsedSent()).toEqual([
      { type: 'requestState' },
      {
        incarnation: 3,
        project: '/selected',
        requestId: 10,
        type: 'ensureProjectRuntime',
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
      incarnation: 1,
      project: '/old',
      requestId: 20,
      type: 'ensureProjectRuntime',
    });
    port.postMessage({
      incarnation: 1,
      project: '/old',
      requestId: 21,
      type: 'releaseProjectRuntime',
    });
    port.postMessage({
      incarnation: 3,
      project: '/selected',
      requestId: 22,
      type: 'ensureProjectRuntime',
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
        incarnation: 3,
        project: '/selected',
        requestId: 22,
        type: 'ensureProjectRuntime',
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
        incarnation: 3,
        project: '/selected',
        requestId: 22,
        type: 'ensureProjectRuntime',
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
      incarnation: 5,
      project: '/selected',
      requestId: 30,
      type: 'ensureProjectRuntime',
    });

    first.serverClose();
    // The release arrives while disconnected: it must still retire the lease.
    port.postMessage({
      incarnation: 5,
      project: '/selected',
      requestId: 31,
      type: 'releaseProjectRuntime',
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
      incarnation: 7,
      project: '/selected',
      requestId: 40,
      type: 'ensureProjectRuntime',
    });
    port.postMessage({
      incarnation: 6,
      project: '/selected',
      requestId: 41,
      type: 'releaseProjectRuntime',
    });

    expect(socket.parsedSent()).toEqual([
      {
        incarnation: 7,
        project: '/selected',
        requestId: 40,
        type: 'ensureProjectRuntime',
      },
      {
        incarnation: 6,
        project: '/selected',
        requestId: 41,
        type: 'releaseProjectRuntime',
      },
    ]);
    port.dispose();
  });
});
