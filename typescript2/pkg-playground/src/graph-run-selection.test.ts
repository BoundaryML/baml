import { describe, expect, it } from 'vitest';

import { findLatestGraphRunSnapshot } from './graph-run-selection';
import type { Run } from './worker-protocol';

describe('findLatestGraphRunSnapshot', () => {
  it('selects the newest direct run when snapshots are newest-first', () => {
    const newest = runFixture('newest', 200, {
      status: 'succeeded',
      result: {
        valueRef: null,
        value: 'new-output',
        rendererHint: null,
        supportingPayloadIds: [],
      },
    });
    const oldest = runFixture('oldest', 100, {
      status: 'failed',
      error: {
        class: 'Runtime',
        message: 'old failure',
        details: null,
        valueRef: null,
      },
    });

    expect(
      findLatestGraphRunSnapshot([newest, oldest], 'throws.main', 'project')
        ?.boundaryId,
    ).toBe('newest');
  });

  it('does not depend on run array ordering', () => {
    const older = runFixture('older', 100);
    const newer = runFixture('newer', 300);
    const middleOtherProject = runFixture('middle-other-project', 200, {
      request: {
        projectId: 'other-project',
        projectGeneration: 1,
        target: { kind: 'function', functionName: 'throws.main' },
        argsSummary: null,
        optionsSummary: null,
      },
    });

    expect(
      findLatestGraphRunSnapshot(
        [older, middleOtherProject, newer],
        'throws.main',
        'project',
      )?.boundaryId,
    ).toBe('newer');
  });

  it('can select a workflow run that contains the displayed function', () => {
    const run = runFixture('workflow', 100, {
      target: { kind: 'function', functionName: 'throws.workflow' },
      calls: [
        callFixture('root', 'throws.workflow'),
        callFixture('child', 'throws.main'),
      ],
    });

    expect(
      findLatestGraphRunSnapshot([run], 'throws.main', 'project')?.boundaryId,
    ).toBe('workflow');
  });
});

function runFixture(
  boundaryId: string,
  createdAtMs: number,
  overrides: Partial<Run> = {},
): Run {
  return {
    boundaryId,
    target: { kind: 'function', functionName: 'throws.main' },
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
      target: { kind: 'function', functionName: 'throws.main' },
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

function callFixture(id: string, functionName: string): Run['calls'][number] {
  return {
    id,
    threadId: 'thread',
    parentId: id === 'root' ? null : 'root',
    functionId: id === 'root' ? 1 : 2,
    functionName,
    functionOrigin: null,
    calleeSource: null,
    callSiteSource: null,
    startedAtNs: null,
    endedAtNs: null,
    status: 'ok',
    payloadIds: [],
  };
}
