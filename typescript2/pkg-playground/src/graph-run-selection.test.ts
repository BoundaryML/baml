import { describe, expect, it } from 'vitest';

import { findLatestGraphRunSnapshot } from './graph-run-selection';
import type { Run } from './worker-protocol';

describe('findLatestGraphRunSnapshot', () => {
  it('selects the newest direct run when snapshots are newest-first', () => {
    const newest = runFixture('newest', 200, {
      result: {
        rendererHint: null,
        supportingPayloadIds: [],
        value: 'new-output',
        valueRef: null,
      },
      status: 'succeeded',
    });
    const oldest = runFixture('oldest', 100, {
      error: {
        class: 'Runtime',
        details: null,
        message: 'old failure',
        valueRef: null,
      },
      status: 'failed',
    });

    expect(
      findLatestGraphRunSnapshot([newest, oldest], 'throws.main', 'project', 1)
        ?.boundaryId,
    ).toBe('newest');
  });

  it('does not depend on run array ordering', () => {
    const older = runFixture('older', 100);
    const newer = runFixture('newer', 300);
    const middleOtherProject = runFixture('middle-other-project', 200, {
      request: {
        argsSummary: null,
        optionsSummary: null,
        projectGeneration: 1,
        projectId: 'other-project',
        target: { functionName: 'throws.main', kind: 'function' },
      },
    });

    expect(
      findLatestGraphRunSnapshot(
        [older, middleOtherProject, newer],
        'throws.main',
        'project',
        1,
      )?.boundaryId,
    ).toBe('newer');
  });

  it('selects a requested historical run instead of the newest snapshot', () => {
    const older = runFixture('older', 100);
    const newer = runFixture('newer', 300);

    expect(
      findLatestGraphRunSnapshot(
        [newer, older],
        'throws.main',
        'project',
        1,
        'older',
      )?.boundaryId,
    ).toBe('older');
  });

  it('falls back to the newest matching snapshot when a requested run is unavailable', () => {
    const older = runFixture('older', 100);
    const newer = runFixture('newer', 300);

    expect(
      findLatestGraphRunSnapshot(
        [older, newer],
        'throws.main',
        'project',
        1,
        'missing',
      )?.boundaryId,
    ).toBe('newer');
  });

  it('can select a workflow run that contains the displayed function', () => {
    const run = runFixture('workflow', 100, {
      calls: [
        callFixture('root', 'throws.workflow'),
        callFixture('child', 'throws.main'),
      ],
      target: { functionName: 'throws.workflow', kind: 'function' },
    });

    expect(
      findLatestGraphRunSnapshot([run], 'throws.main', 'project', 1)
        ?.boundaryId,
    ).toBe('workflow');
  });

  it('does not pair an obsolete run generation with the current graph', () => {
    const obsolete = runFixture('obsolete', 100);

    expect(
      findLatestGraphRunSnapshot([obsolete], 'throws.main', 'project', 2),
    ).toBeUndefined();
  });
});

function runFixture(
  boundaryId: string,
  createdAtMs: number,
  overrides: Partial<Run> = {},
): Run {
  return {
    boundaryId,
    calls: [],
    cancellation: null,
    completedAtMs: null,
    createdAtMs,
    cursor: 0,
    diagnostics: [],
    error: null,
    graphRuntimeOverlay: null,
    payloads: [],
    request: {
      argsSummary: null,
      optionsSummary: null,
      projectGeneration: 1,
      projectId: 'project',
      target: { functionName: 'throws.main', kind: 'function' },
    },
    result: null,
    rootCallNodeId: null,
    startedAtMs: createdAtMs,
    status: 'running',
    target: { functionName: 'throws.main', kind: 'function' },
    threads: [],
    timeAnchor: {
      epochCreatedAtMs: createdAtMs,
      traceZeroNs: '0',
    },
    visibility: { kind: 'history' },
    ...overrides,
  };
}

function callFixture(id: string, functionName: string): Run['calls'][number] {
  return {
    calleeSource: null,
    callSiteSource: null,
    endedAtNs: null,
    functionId: id === 'root' ? 1 : 2,
    functionName,
    functionOrigin: null,
    id,
    parentId: id === 'root' ? null : 'root',
    payloadIds: [],
    startedAtNs: null,
    status: 'ok',
    threadId: 'thread',
  };
}
