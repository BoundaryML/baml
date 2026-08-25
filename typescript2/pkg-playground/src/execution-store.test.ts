import { describe, expect, it, vi } from 'vitest';

import { applyRunPatch, createExecutionStore } from './execution-store';
import type { RunStoreClient } from './run-store-client';
import type { Run, RunPatch } from './worker-protocol';

describe('execution-store', () => {
  it('applies RunStore patches without mutating the original snapshot', () => {
    const initial = runFixture('run-1', 100);
    const patch: RunPatch = {
      boundaryId: 'run-1',
      changes: [
        { status: 'succeeded', type: 'setStatus' },
        {
          outcome: {
            result: {
              rendererHint: null,
              supportingPayloadIds: [],
              value: 'ok',
              valueRef: null,
            },
            status: 'succeeded',
          },
          type: 'complete',
        },
      ],
      cursor: 4,
    };

    const next = applyRunPatch(initial, patch);

    expect(initial.cursor).toBe(0);
    expect(initial.status).toBe('running');
    expect(next.cursor).toBe(4);
    expect(next.status).toBe('succeeded');
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
      cursor: 7,
      request: {
        argsSummary: null,
        optionsSummary: null,
        projectGeneration: 4,
        projectId: 'project',
        target: { generation: 4, kind: 'test', testName: 'suite/test' },
      },
      target: { generation: 4, kind: 'test', testName: 'suite/test' },
    });
    vi.mocked(client.startTestRun).mockResolvedValue('test-run');
    vi.mocked(client.snapshot).mockResolvedValue(testRun);
    vi.mocked(client.subscribe).mockReturnValue({
      events: emptyAsyncIterable(),
      subscriptionId: 'sub-test-run',
      unsubscribe: vi.fn(),
    });
    const store = createExecutionStore(client);

    await expect(
      store.startTestRun({
        generation: 4,
        project: 'project',
        testName: 'suite/test',
      }),
    ).resolves.toBe('test-run');

    expect(client.startTestRun).toHaveBeenCalledWith({
      generation: 4,
      project: 'project',
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
    cancellation: null,
    completedAtMs: null,
    createdAtMs,
    cursor: 0,
    diagnostics: [],
    error: null,
    payloads: [],
    request: {
      argsSummary: null,
      optionsSummary: null,
      projectGeneration: 1,
      projectId: 'project',
      target: { functionName: 'main', kind: 'function' },
    },
    result: null,
    startedAtMs: createdAtMs,
    status: 'running',
    target: { functionName: 'main', kind: 'function' },
    timeAnchor: {
      epochCreatedAtMs: createdAtMs,
      traceZeroNs: '0',
    },
    visibility: { kind: 'history' },
    ...overrides,
  };
}

async function* emptyAsyncIterable<T>(): AsyncIterable<T> {}

function mockRunStoreClient(): RunStoreClient {
  return {
    cancelRun: vi.fn(),
    dispose: vi.fn(),
    listHistory: vi.fn(),
    listRuns: vi.fn(),
    openHistory: vi.fn(),
    readValue: vi.fn(),
    respondToEnv: vi.fn(),
    respondToInput: vi.fn(),
    snapshot: vi.fn(),
    startRun: vi.fn(),
    startTestRun: vi.fn(),
    subscribe: vi.fn(),
    unsubscribe: vi.fn(),
  } as unknown as RunStoreClient;
}
