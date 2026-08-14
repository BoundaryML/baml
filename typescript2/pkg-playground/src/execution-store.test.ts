import { describe, expect, it, vi } from 'vitest';

import { applyRunPatch, createExecutionStore } from './execution-store';
import type { Run, RunPatch } from './worker-protocol';
import type { RunStoreClient } from './run-store-client';

describe('execution-store', () => {
  it('applies RunStore patches without mutating the original snapshot', () => {
    const initial = runFixture('run-1', 100);
    const patch: RunPatch = {
      boundaryId: 'run-1',
      cursor: 4,
      changes: [
        {
          type: 'upsertThreadNode',
          thread: {
            id: 'thread_node_1',
            parentThreadId: null,
            parentCallNodeId: null,
            name: null,
            startedAtNs: '10',
            endedAtNs: null,
            status: 'running',
            callNodeIds: ['call_node_1'],
          },
        },
        {
          type: 'upsertCallNode',
          call: {
            id: 'call_node_1',
            threadId: 'thread_node_1',
            parentId: null,
            functionId: 1,
            functionName: 'main',
            functionOrigin: null,
            calleeSource: null,
            callSiteSource: null,
            startedAtNs: '20',
            endedAtNs: null,
            status: 'running',
            payloadIds: [],
          },
        },
        { type: 'setRootCallNode', callNodeId: 'call_node_1' },
        {
          type: 'setGraphRuntimeOverlay',
          overlay: {
            boundaryId: 'run-1',
            projectGeneration: 1,
            entries: [],
            unattachedCallNodeIds: ['call_node_1'],
            diagnostics: [
              {
                severity: 'info',
                code: 'GraphOverlayCallSiteUnavailable',
                message: 'no call site',
                callNodeId: null,
                payloadId: null,
              },
            ],
          },
        },
        { type: 'setStatus', status: 'succeeded' },
        {
          type: 'complete',
          outcome: {
            status: 'succeeded',
            result: {
              valueRef: null,
              value: 'ok',
              rendererHint: null,
              supportingPayloadIds: [],
            },
          },
        },
      ],
    };

    const next = applyRunPatch(initial, patch);

    expect(initial.cursor).toBe(0);
    expect(initial.calls).toHaveLength(0);
    expect(next.cursor).toBe(4);
    expect(next.status).toBe('succeeded');
    expect(next.rootCallNodeId).toBe('call_node_1');
    expect(next.graphRuntimeOverlay?.unattachedCallNodeIds).toEqual([
      'call_node_1',
    ]);
    expect(next.calls).toHaveLength(1);
    expect(next.threads[0]?.callNodeIds).toEqual(['call_node_1']);
    expect(next.result?.value).toBe('ok');
  });

  it('keeps snapshots sorted and notifies subscribers', () => {
    const store = createExecutionStore(mockRunStoreClient());
    const listener = vi.fn();
    const unsubscribe = store.subscribe(listener);

    store.applySnapshot(runFixture('run-old', 100));
    store.applySnapshot(runFixture('run-new', 200));
    store.selectRun('run-old');

    expect(listener).toHaveBeenCalledTimes(4);
    const latest = listener.mock.calls.at(-1)?.[0];
    expect(latest.selectedBoundaryId).toBe('run-old');
    expect(latest.runs.map((run: Run) => run.boundaryId)).toEqual([
      'run-new',
      'run-old',
    ]);

    unsubscribe();
    store.dispose();
  });

  it('starts test runs through the RunStore client and follows the snapshot cursor', async () => {
    const client = mockRunStoreClient();
    const testRun = runFixture('test-run', 300, {
      target: { kind: 'test', generation: 4, testName: 'suite/test' },
      request: {
        projectId: 'project',
        projectGeneration: 4,
        target: { kind: 'test', generation: 4, testName: 'suite/test' },
        argsSummary: null,
        optionsSummary: null,
      },
      cursor: 7,
    });
    vi.mocked(client.startTestRun).mockResolvedValue('test-run');
    vi.mocked(client.snapshot).mockResolvedValue(testRun);
    vi.mocked(client.subscribe).mockReturnValue({
      subscriptionId: 'sub-test-run',
      events: emptyAsyncIterable(),
      unsubscribe: vi.fn(),
    });
    const store = createExecutionStore(client);

    await expect(
      store.startTestRun({
        project: 'project',
        generation: 4,
        testName: 'suite/test',
      }),
    ).resolves.toBe('test-run');

    expect(client.startTestRun).toHaveBeenCalledWith({
      project: 'project',
      generation: 4,
      testName: 'suite/test',
    });
    expect(client.snapshot).toHaveBeenCalledWith('test-run');
    expect(client.subscribe).toHaveBeenCalledWith('test-run', 7);
    expect(store.getSnapshot().runs[0]?.boundaryId).toBe('test-run');

    store.dispose();
  });
});

function runFixture(
  boundaryId: string,
  createdAtMs: number,
  overrides: Partial<Run> = {},
): Run {
  return {
    boundaryId,
    target: { kind: 'function', functionName: 'main' },
    visibility: { kind: 'history' },
    status: 'running',
    createdAtMs,
    startedAtMs: createdAtMs,
    completedAtMs: null,
    timeAnchor: {
      epochCreatedAtMs: createdAtMs,
      traceZeroNs: '0',
    },
    request: {
      projectId: 'project',
      projectGeneration: 1,
      target: { kind: 'function', functionName: 'main' },
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
    ...overrides,
  };
}

async function* emptyAsyncIterable<T>(): AsyncIterable<T> {}

function mockRunStoreClient(): RunStoreClient {
  return {
    startRun: vi.fn(),
    startTestRun: vi.fn(),
    cancelRun: vi.fn(),
    respondToInput: vi.fn(),
    respondToEnv: vi.fn(),
    listRuns: vi.fn(),
    listHistory: vi.fn(),
    openHistory: vi.fn(),
    snapshot: vi.fn(),
    readValue: vi.fn(),
    subscribe: vi.fn(),
    unsubscribe: vi.fn(),
    dispose: vi.fn(),
  } as unknown as RunStoreClient;
}
