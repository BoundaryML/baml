import { describe, expect, it } from 'vitest';

import { createRunStoreClient } from './run-store-client';
import type { RuntimePort } from './runtime-port';
import type { Run, WorkerInMessage, WorkerOutMessage } from './worker-protocol';

describe('run-store-client', () => {
  it('starts preview runs with request correlation and resolves from runStarted', async () => {
    const port = new FakeRuntimePort();
    const client = createRunStoreClient(port);

    const pending = client.startPreviewRun({
      project: 'project',
      parentFunctionName: 'Extract',
      helper: 'render_prompt',
      functionName: 'Extract$render_prompt',
      argsBytes: new Uint8Array([1, 2, 3]),
    });

    expect(port.sent).toEqual([
      {
        type: 'startPreviewRun',
        requestId: 1,
        project: 'project',
        parentFunctionName: 'Extract',
        helper: 'render_prompt',
        functionName: 'Extract$render_prompt',
        argsBytes: new Uint8Array([1, 2, 3]),
      },
    ]);

    port.emit({
      type: 'runStarted',
      requestId: 1,
      run: runFixture('baml_run_1_00000000000000000000000000000001'),
    });

    await expect(pending).resolves.toBe(
      'baml_run_1_00000000000000000000000000000001',
    );
    client.dispose();
  });

  it('starts test runs with request correlation and resolves from runStarted', async () => {
    const port = new FakeRuntimePort();
    const client = createRunStoreClient(port);

    const pending = client.startTestRun({
      project: 'project',
      generation: 2,
      testName: 'suite/test',
    });

    expect(port.sent).toEqual([
      {
        type: 'startTestRun',
        requestId: 1,
        project: 'project',
        generation: 2,
        testName: 'suite/test',
      },
    ]);

    port.emit({
      type: 'runStarted',
      requestId: 1,
      run: runFixture('baml_run_1_00000000000000000000000000000001'),
    });

    await expect(pending).resolves.toBe(
      'baml_run_1_00000000000000000000000000000001',
    );
    client.dispose();
  });

  it('responds to input through run-scoped command frames', async () => {
    const port = new FakeRuntimePort();
    const client = createRunStoreClient(port);

    const pending = client.respondToInput(
      'baml_run_1_00000000000000000000000000000001',
      '3',
      'answer',
    );

    expect(port.sent).toEqual([
      {
        type: 'respondToInput',
        requestId: 1,
        runId: 'baml_run_1_00000000000000000000000000000001',
        inputRequestId: '3',
        value: 'answer',
      },
    ]);

    port.emit({
      type: 'commandAck',
      requestId: 1,
      outcome: 'accepted',
    });

    await expect(pending).resolves.toBe('accepted');
    client.dispose();
  });

  it('responds to env through run-scoped command frames', async () => {
    const port = new FakeRuntimePort();
    const client = createRunStoreClient(port);

    const pending = client.respondToEnv(
      'baml_run_1_00000000000000000000000000000001',
      '4',
      'secret',
    );

    expect(port.sent).toEqual([
      {
        type: 'respondToEnv',
        requestId: 1,
        runId: 'baml_run_1_00000000000000000000000000000001',
        envRequestId: '4',
        value: 'secret',
      },
    ]);

    port.emit({
      type: 'commandAck',
      requestId: 1,
      outcome: 'accepted',
    });

    await expect(pending).resolves.toBe('accepted');
    client.dispose();
  });

  it('clears pending subscribe requests after initial cursor expiration', async () => {
    const port = new FakeRuntimePort();
    const client = createRunStoreClient(port);
    const runId = 'baml_run_1_00000000000000000000000000000001';

    const handle = client.subscribe(runId, 99);
    const iterator = handle.events[Symbol.asyncIterator]();

    expect(port.sent).toEqual([
      {
        type: 'subscribe',
        requestId: 1,
        subscriptionId: handle.subscriptionId,
        runId,
        afterCursor: 99,
      },
    ]);

    port.emit({
      type: 'runCursorExpired',
      requestId: 1,
      subscriptionId: handle.subscriptionId,
      runId,
      reason: 'future',
    });

    await expect(iterator.next()).resolves.toEqual({
      value: { type: 'cursorExpired', runId, reason: 'future' },
      done: false,
    });

    client.dispose();
    await expect(iterator.next()).resolves.toEqual({
      value: undefined,
      done: true,
    });
  });

  it('lists runs with RunStore-owned filters', async () => {
    const port = new FakeRuntimePort();
    const client = createRunStoreClient(port);

    const pending = client.listRuns({
      projectId: '/tmp/project',
      projectGeneration: 4,
      kinds: ['function'],
      callTreeContainsFunction: 'Extract',
      visibility: 'historyOnly',
    });

    expect(port.sent).toEqual([
      {
        type: 'listRuns',
        requestId: 1,
        filter: {
          projectId: '/tmp/project',
          projectGeneration: 4,
          kinds: ['function'],
          callTreeContainsFunction: 'Extract',
          visibility: 'historyOnly',
        },
      },
    ]);

    port.emit({
      type: 'runList',
      requestId: 1,
      runs: [
        {
          runId: 'baml_run_1_00000000000000000000000000000001',
          target: { kind: 'function', functionName: 'Extract' },
          visibility: { kind: 'history' },
          status: 'succeeded',
          request: {
            projectId: '/tmp/project',
            projectGeneration: 4,
            target: { kind: 'function', functionName: 'Extract' },
            argsSummary: null,
            optionsSummary: null,
          },
          touchedFunctions: ['Extract'],
          createdAtMs: 100,
          completedAtMs: 150,
          retention: 'Full',
        },
      ],
    });

    await expect(pending).resolves.toHaveLength(1);
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

function runFixture(runId: string): Run {
  return {
    runId,
    target: { kind: 'test', generation: 2, testName: 'suite/test' },
    visibility: { kind: 'history' },
    status: 'running',
    createdAtMs: 100,
    startedAtMs: 100,
    completedAtMs: null,
    timeAnchor: {
      epochCreatedAtMs: 100,
      traceZeroNs: '0',
    },
    request: {
      projectId: 'project',
      projectGeneration: 2,
      target: { kind: 'test', generation: 2, testName: 'suite/test' },
      argsSummary: null,
      optionsSummary: null,
    },
    result: null,
    error: null,
    cancellation: null,
    rootCallNodeId: null,
    graphRuntimeOverlay: null,
    calls: [],
    threads: [],
    payloads: [],
    diagnostics: [],
    cursor: 0,
  };
}
