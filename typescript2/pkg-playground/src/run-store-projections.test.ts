import { describe, expect, it } from 'vitest';

import {
  runToDisplayRun,
  runToFlamegraphRows,
  runToTraceRows,
} from './run-store-projections';
import type { Run } from './worker-protocol';

describe('run-store-projections', () => {
  it('projects fetch payloads from RunStore snapshots without exposing values', () => {
    const run = runFixture({
      payloads: [
        {
          id: '1',
          callNodeId: null,
          timestampMs: 120,
          kind: {
            type: 'fetchStarted',
            fetchId: '9',
            method: 'POST',
            url: 'https://example.test',
            requestHeaders: [{ name: 'authorization', valueRedacted: true }],
          },
          redaction: {
            valueRedacted: true,
            displaySafe: false,
            reason: 'redacted',
            policyId: 'test',
          },
          body: null,
        },
        {
          id: '2',
          callNodeId: null,
          timestampMs: 150,
          kind: {
            type: 'fetchUpdated',
            fetchId: '9',
            status: 200,
            durationMs: 30,
            responseHeaders: [{ name: 'content-type', valueRedacted: true }],
            error: null,
          },
          redaction: {
            valueRedacted: true,
            displaySafe: false,
            reason: 'redacted',
            policyId: 'test',
          },
          body: null,
        },
      ],
    });

    const display = runToDisplayRun(run, { 'run-1': '{"x":1}' });

    expect(display?.id).toBe('run-1');
    expect(display?.kind).toBe('function');
    expect(display?.projectGeneration).toBe(1);
    expect(display?.argsJson).toBe('{"x":1}');
    expect(display?.fetchLogs).toEqual([
      expect.objectContaining({
        id: 9,
        method: 'POST',
        url: 'https://example.test',
        status: 200,
        durationMs: 30,
        requestHeaders: { authorization: '<redacted>' },
        responseHeaders: { 'content-type': '<redacted>' },
      }),
    ]);
  });

  it('projects terminal status and duration from RunStore outcome fields', () => {
    const run = runFixture({
      status: 'failed',
      startedAtMs: 110,
      completedAtMs: 175,
      error: {
        class: 'Runtime',
        message: 'boom',
        details: null,
      },
    });

    const display = runToDisplayRun(run, {});

    expect(display?.status).toBe('error');
    expect(display?.durationMs).toBe(65);
    expect(display?.error).toBe('boom');
  });

  it('projects test execution runs without modeling discovery as a run', () => {
    const run = runFixture({
      target: { kind: 'test', generation: 7, testName: 'suite/test' },
      request: {
        projectId: 'project',
        projectGeneration: 7,
        target: { kind: 'test', generation: 7, testName: 'suite/test' },
        argsSummary: null,
        optionsSummary: null,
      },
    });

    const display = runToDisplayRun(run, {});

    expect(display).toMatchObject({
      id: 'run-1',
      kind: 'test',
      projectGeneration: 7,
      functionName: 'testing.run_test',
      testName: 'suite/test',
      argsJson: '',
    });
  });

  it('projects only unresolved input requests from RunStore payloads', () => {
    const run = runFixture({
      payloads: [
        payloadFixture({
          id: 'input-1',
          kind: {
            type: 'inputRequested',
            requestId: '1',
            prompt: 'Name?',
            state: 'pending',
          },
        }),
        payloadFixture({
          id: 'input-2',
          kind: {
            type: 'inputRequested',
            requestId: '2',
            prompt: 'City?',
            state: 'pending',
          },
        }),
        payloadFixture({
          id: 'input-3',
          kind: {
            type: 'inputResolved',
            requestId: '1',
            state: 'resolved',
          },
        }),
      ],
    });

    const display = runToDisplayRun(run, {});

    expect(display?.inputRequests).toEqual([{ id: '2', prompt: 'City?' }]);
  });

  it('projects trace rows from RunStore call nodes without reconstructing structure', () => {
    const run = runFixture({
      calls: [
        callFixture({
          id: 'child',
          parentId: 'root',
          functionId: 2,
          functionName: null,
          startedAtNs: '125000000',
          endedAtNs: '175000000',
          status: 'ok',
          callSiteSource: { line: 12, column: 3 },
        }),
        callFixture({
          id: 'root',
          parentId: null,
          functionId: 1,
          functionName: 'user.Main',
          startedAtNs: '100000000',
          endedAtNs: '200000000',
          status: 'ok',
        }),
      ],
    });

    expect(runToTraceRows(run)).toEqual([
      expect.objectContaining({
        id: 'root',
        depth: 0,
        functionName: 'user.Main',
        offsetMs: 0,
        durationMs: 100,
      }),
      expect.objectContaining({
        id: 'child',
        depth: 1,
        functionName: 'function#2',
        offsetMs: 25,
        durationMs: 50,
        sourceLine: 12,
      }),
    ]);
  });

  it('projects flamegraph rows from RunStore call edges and timestamps', () => {
    const run = runFixture({
      rootCallNodeId: 'root',
      calls: [
        callFixture({
          id: 'child-b',
          parentId: 'root',
          functionId: 3,
          functionName: 'user.ChildB',
          startedAtNs: '200000000',
          endedAtNs: '260000000',
          status: 'ok',
        }),
        callFixture({
          id: 'root',
          parentId: null,
          functionId: 1,
          functionName: 'user.Main',
          startedAtNs: '100000000',
          endedAtNs: '300000000',
          status: 'ok',
        }),
        callFixture({
          id: 'child-a',
          parentId: 'root',
          functionId: 2,
          functionName: 'user.ChildA',
          startedAtNs: '120000000',
          endedAtNs: '170000000',
          status: 'ok',
        }),
      ],
    });

    expect(runToFlamegraphRows(run)).toEqual([
      expect.objectContaining({
        id: 'root',
        depth: 0,
        durationMs: 200,
        selfMs: 90,
        spanLeftPct: 0,
        spanWidthPct: 100,
      }),
      expect.objectContaining({
        id: 'child-a',
        depth: 1,
        functionName: 'user.ChildA',
        durationMs: 50,
        spanLeftPct: 10,
        spanWidthPct: 25,
      }),
      expect.objectContaining({
        id: 'child-b',
        depth: 1,
        functionName: 'user.ChildB',
        durationMs: 60,
        spanLeftPct: 50,
        spanWidthPct: 30,
      }),
    ]);
  });
});

function runFixture(overrides: Partial<Run>): Run {
  return {
    runId: 'run-1',
    target: { kind: 'function', functionName: 'main' },
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

function payloadFixture(
  overrides: Partial<Run['payloads'][number]>,
): Run['payloads'][number] {
  return {
    id: 'payload',
    callNodeId: null,
    timestampMs: 100,
    kind: {
      type: 'log',
      level: 'info',
      message: 'placeholder',
    },
    redaction: {
      valueRedacted: false,
      displaySafe: true,
      reason: null,
      policyId: null,
    },
    body: null,
    ...overrides,
  };
}

function callFixture(
  overrides: Partial<Run['calls'][number]>,
): Run['calls'][number] {
  return {
    id: 'call',
    threadId: 'thread',
    parentId: null,
    functionId: 1,
    functionName: 'user.call',
    functionOrigin: 'user',
    calleeSource: null,
    callSiteSource: null,
    startedAtNs: '0',
    endedAtNs: '0',
    status: 'ok',
    payloadIds: [],
    ...overrides,
  };
}
