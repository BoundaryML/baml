import { describe, expect, it } from 'vitest';

import {
  createRunStoreClient,
  isProjectNotReadyError,
  RunCommandError,
} from './run-store-client';
import type { RuntimePort } from './runtime-port';
import type { Run, WorkerInMessage, WorkerOutMessage } from './worker-protocol';

describe('run-store-client', () => {
  it('starts preview runs with request correlation and resolves from runStarted', async () => {
    const port = new FakeRuntimePort();
    const client = createRunStoreClient(port);

    const pending = client.startPreviewRun({
      argsBytes: new Uint8Array([1, 2, 3]),
      functionName: 'Extract$render_prompt',
      helper: 'render_prompt',
      parentFunctionName: 'Extract',
      project: 'project',
    });

    expect(port.sent).toEqual([
      {
        argsBytes: new Uint8Array([1, 2, 3]),
        functionName: 'Extract$render_prompt',
        helper: 'render_prompt',
        parentFunctionName: 'Extract',
        project: 'project',
        requestId: 1,
        type: 'startPreviewRun',
      },
    ]);

    port.emit({
      requestId: 1,
      run: runFixture('baml_id_1_AAAAAAAAAAAAAAAAAAAAAQ'),
      type: 'runStarted',
    });

    await expect(pending).resolves.toBe('baml_id_1_AAAAAAAAAAAAAAAAAAAAAQ');
    client.dispose();
  });

  it('starts test runs with request correlation and resolves from runStarted', async () => {
    const port = new FakeRuntimePort();
    const client = createRunStoreClient(port);

    const pending = client.startTestRun({
      generation: 2,
      project: 'project',
      testName: 'suite/test',
    });

    expect(port.sent).toEqual([
      {
        generation: 2,
        project: 'project',
        requestId: 1,
        testName: 'suite/test',
        type: 'startTestRun',
      },
    ]);

    port.emit({
      requestId: 1,
      run: runFixture('baml_id_1_AAAAAAAAAAAAAAAAAAAAAQ'),
      type: 'runStarted',
    });

    await expect(pending).resolves.toBe('baml_id_1_AAAAAAAAAAAAAAAAAAAAAQ');
    client.dispose();
  });

  it('responds to input through run-scoped command frames', async () => {
    const port = new FakeRuntimePort();
    const client = createRunStoreClient(port);

    const pending = client.respondToInput(
      'baml_id_1_AAAAAAAAAAAAAAAAAAAAAQ',
      '3',
      'answer',
    );

    expect(port.sent).toEqual([
      {
        boundaryId: 'baml_id_1_AAAAAAAAAAAAAAAAAAAAAQ',
        inputRequestId: '3',
        requestId: 1,
        type: 'respondToInput',
        value: 'answer',
      },
    ]);

    port.emit({
      outcome: 'accepted',
      requestId: 1,
      type: 'commandAck',
    });

    await expect(pending).resolves.toBe('accepted');
    client.dispose();
  });

  it('responds to env through run-scoped command frames', async () => {
    const port = new FakeRuntimePort();
    const client = createRunStoreClient(port);

    const pending = client.respondToEnv(
      'baml_id_1_AAAAAAAAAAAAAAAAAAAAAQ',
      '4',
      'secret',
    );

    expect(port.sent).toEqual([
      {
        boundaryId: 'baml_id_1_AAAAAAAAAAAAAAAAAAAAAQ',
        envRequestId: '4',
        requestId: 1,
        type: 'respondToEnv',
        value: 'secret',
      },
    ]);

    port.emit({
      outcome: 'accepted',
      requestId: 1,
      type: 'commandAck',
    });

    await expect(pending).resolves.toBe('accepted');
    client.dispose();
  });

  it('reads value bodies with request correlation', async () => {
    const port = new FakeRuntimePort();
    const client = createRunStoreClient(port);
    const boundaryId = 'baml_id_1_AAAAAAAAAAAAAAAAAAAAAQ';
    const valueRef = {
      availability: 'available' as const,
      codec: 'bamlOutboundValue' as const,
      diagnostic: null,
      id: 'value_1',
      originalSizeBytes: 3,
      retainedSizeBytes: 3,
    };

    const pending = client.readValue(boundaryId, valueRef);

    expect(port.sent).toEqual([
      {
        boundaryId,
        requestId: 1,
        type: 'readValue',
        valueRef,
      },
    ]);

    port.emit({
      availability: 'available',
      bodyBase64: 'AQID',
      boundaryId,
      codec: 'bamlOutboundValue',
      requestId: 1,
      type: 'valueBody',
      valueRefId: 'value_1',
    });

    await expect(pending).resolves.toEqual({
      availability: 'available',
      bodyBase64: 'AQID',
      boundaryId,
      codec: 'bamlOutboundValue',
      requestId: 1,
      type: 'valueBody',
      valueRefId: 'value_1',
    });
    client.dispose();
  });

  it('clears pending subscribe requests after initial cursor expiration', async () => {
    const port = new FakeRuntimePort();
    const client = createRunStoreClient(port);
    const boundaryId = 'baml_id_1_AAAAAAAAAAAAAAAAAAAAAQ';

    const handle = client.subscribe(boundaryId, 99);
    const iterator = handle.events[Symbol.asyncIterator]();

    expect(port.sent).toEqual([
      {
        afterCursor: 99,
        boundaryId,
        requestId: 1,
        subscriptionId: handle.subscriptionId,
        type: 'subscribe',
      },
    ]);

    port.emit({
      boundaryId,
      reason: 'future',
      requestId: 1,
      subscriptionId: handle.subscriptionId,
      type: 'runCursorExpired',
    });

    await expect(iterator.next()).resolves.toEqual({
      done: false,
      value: { boundaryId, reason: 'future', type: 'cursorExpired' },
    });

    client.dispose();
    await expect(iterator.next()).resolves.toEqual({
      done: true,
      value: undefined,
    });
  });

  it('lists runs with RunStore-owned filters', async () => {
    const port = new FakeRuntimePort();
    const client = createRunStoreClient(port);

    const pending = client.listRuns({
      callTreeContainsFunction: 'Extract',
      kinds: ['function'],
      projectGeneration: 4,
      projectId: '/tmp/project',
      visibility: 'historyOnly',
    });

    expect(port.sent).toEqual([
      {
        filter: {
          callTreeContainsFunction: 'Extract',
          kinds: ['function'],
          projectGeneration: 4,
          projectId: '/tmp/project',
          visibility: 'historyOnly',
        },
        requestId: 1,
        type: 'listRuns',
      },
    ]);

    port.emit({
      requestId: 1,
      runs: [
        {
          boundaryId: 'baml_id_1_AAAAAAAAAAAAAAAAAAAAAQ',
          completedAtMs: 150,
          createdAtMs: 100,
          request: {
            argsSummary: null,
            optionsSummary: null,
            projectGeneration: 4,
            projectId: '/tmp/project',
            target: { functionName: 'Extract', kind: 'function' },
          },
          retention: 'Full',
          status: 'succeeded',
          target: { functionName: 'Extract', kind: 'function' },
          touchedFunctions: ['Extract'],
          visibility: { kind: 'history' },
        },
      ],
      type: 'runList',
    });

    await expect(pending).resolves.toHaveLength(1);
    client.dispose();
  });

  it('lists persisted history with an explicit history command', async () => {
    const port = new FakeRuntimePort();
    const client = createRunStoreClient(port);

    const pending = client.listHistory({ visibility: 'historyOnly' });

    expect(port.sent).toEqual([
      {
        filter: { visibility: 'historyOnly' },
        requestId: 1,
        type: 'listHistory',
      },
    ]);

    port.emit({
      requestId: 1,
      runs: [],
      type: 'historyList',
    });

    await expect(pending).resolves.toEqual([]);
    client.dispose();
  });

  it('opens persisted history through the normal snapshot response', async () => {
    const port = new FakeRuntimePort();
    const client = createRunStoreClient(port);
    const boundaryId = 'baml_id_1_AAAAAAAAAAAAAAAAAAAAAQ';

    const pending = client.openHistory(boundaryId);

    expect(port.sent).toEqual([
      {
        boundaryId,
        requestId: 1,
        type: 'openHistory',
      },
    ]);

    const run = runFixture(boundaryId);
    port.emit({
      boundaryId,
      requestId: 1,
      snapshot: run,
      type: 'runSnapshot',
    });

    await expect(pending).resolves.toEqual(run);
    client.dispose();
  });

  it('rejects commands with a structured RunCommandError carrying the code', async () => {
    const port = new FakeRuntimePort();
    const client = createRunStoreClient(port);

    const pending = client.startRun({
      argsBytes: new Uint8Array([1]),
      functionName: 'Extract',
      project: 'project',
    });

    port.emit({
      code: 'projectNotReady',
      message: 'Cannot start run: rebuild pending',
      requestId: 1,
      type: 'commandError',
    });

    const error = await pending.then(
      () => undefined,
      (rejection: unknown) => rejection,
    );
    expect(error).toBeInstanceOf(RunCommandError);
    expect((error as RunCommandError).code).toBe('projectNotReady');
    expect(isProjectNotReadyError(error)).toBe(true);
    expect((error as RunCommandError).message).toBe(
      'projectNotReady: Cannot start run: rebuild pending',
    );
    client.dispose();
  });
});

class FakeRuntimePort implements RuntimePort {
  sent: WorkerInMessage[] = [];
  private handlers = new Set<(msg: WorkerOutMessage) => void>();

  postMessage(msg: WorkerInMessage): void {
    this.sent.push(msg);
  }

  onMessage(handler: (msg: WorkerOutMessage) => void): () => void {
    this.handlers.add(handler);
    return () => {
      this.handlers.delete(handler);
    };
  }

  emit(msg: WorkerOutMessage): void {
    for (const handler of this.handlers) {
      handler(msg);
    }
  }

  dispose(): void {
    this.handlers.clear();
  }
}

function runFixture(boundaryId: string): Run {
  return {
    boundaryId,
    cancellation: null,
    completedAtMs: null,
    createdAtMs: 100,
    cursor: 0,
    diagnostics: [],
    error: null,
    payloads: [],
    request: {
      argsSummary: null,
      optionsSummary: null,
      projectGeneration: 2,
      projectId: 'project',
      target: { generation: 2, kind: 'test', testName: 'suite/test' },
    },
    result: null,
    startedAtMs: 100,
    status: 'running',
    target: { generation: 2, kind: 'test', testName: 'suite/test' },
    timeAnchor: {
      epochCreatedAtMs: 100,
      traceZeroNs: '0',
    },
    visibility: { kind: 'history' },
  };
}
