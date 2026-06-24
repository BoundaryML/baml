import { describe, expect, it } from 'vitest';
import { BamlOutboundValue } from '@b/pkg-proto';

import {
  buildExecutionProfileProjection,
  executionProfileColorKey,
  executionProfileSearchFunctionKeys,
  filterExecutionProfileProjection,
  runToDisplayRun,
  runToTraceRows,
} from './run-store-projections';
import type { Run, ValueRef } from './worker-protocol';
import type { ValueBodyCache } from './value-body-cache';

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
        valueRef: null,
      },
    });

    const display = runToDisplayRun(run, {});

    expect(display?.status).toBe('error');
    expect(display?.durationMs).toBe(65);
    expect(display?.error).toBe('boom');
  });

  it('hydrates RunResult valueRef bytes through the value body cache', () => {
    const bytes = outboundStringBytes('hello from ref');
    const valueRef = valueRefFixture('value_1', bytes);
    const cache = cacheWith('value_1', bytes);
    const run = runFixture({
      result: {
        valueRef,
        rendererHint: 'baml.outbound.base64',
        supportingPayloadIds: [],
      },
    });

    const display = runToDisplayRun(run, {}, cache);

    expect(display?.result).toBe('hello from ref');
  });

  it('hydrates root thrown value refs through the value body cache', () => {
    const bytes = outboundStringBytes('bad input');
    const valueRef = valueRefFixture('error_value', bytes);
    const display = runToDisplayRun(
      runFixture({
        status: 'failed',
        error: {
          class: 'Runtime',
          message: 'failed',
          details: null,
          valueRef,
        },
      }),
      {},
      cacheWith('error_value', bytes),
    );

    expect(display?.error).toBe('failed');
    expect(display?.errorValue).toBe('bad input');
  });

  it('hydrates root input capturedValue payload refs through the value body cache', () => {
    const bytes = BamlOutboundValue.encode({
      value: {
        $case: 'mapValue',
        mapValue: {
          keyType: undefined,
          valueType: undefined,
          entries: [
            {
              key: 'topic',
              value: {
                value: { $case: 'stringValue', stringValue: 'volcanoes' },
              },
            },
          ],
        },
      },
    }).finish();
    const valueRef = valueRefFixture('input_value', bytes);
    const display = runToDisplayRun(
      runFixture({
        payloads: [
          payloadFixture({
            id: 'payload-input',
            kind: {
              type: 'capturedValue',
              role: 'rootInput',
              label: 'inputs',
              valueRef,
            },
          }),
        ],
      }),
      {},
      cacheWith('input_value', bytes),
    );

    expect(display?.rootInput).toEqual({ topic: 'volcanoes' });
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

  it('projects identified logs under their owning call node', () => {
    const run = runFixture({
      calls: [
        callFixture({
          id: 'root',
          parentId: null,
          functionName: 'user.Main',
          startedAtNs: '100000000',
          endedAtNs: '200000000',
        }),
        callFixture({
          id: 'child',
          parentId: 'root',
          functionName: 'user.Work',
          startedAtNs: '125000000',
          endedAtNs: '175000000',
        }),
      ],
      payloads: [
        payloadFixture({
          id: 'log-1',
          callNodeId: 'child',
          timestampMs: 120,
          kind: {
            type: 'log',
            level: 'warn',
            message: 'watch this',
            source: { line: 12, column: 3 },
            valueRef: null,
          },
        }),
      ],
    });

    const rows = runToTraceRows(run);

    expect(rows.find((row) => row.id === 'root')?.logs).toEqual([]);
    expect(rows.find((row) => row.id === 'child')?.logs).toEqual([
      expect.objectContaining({
        id: 'log-1',
        level: 'warn',
        message: 'watch this',
        sourceLine: 12,
        state: 'unavailable',
        value: null,
      }),
    ]);
  });

  it('attaches logs through call payload ids and hydrates value refs', () => {
    const bytes = outboundStringBytes('full log body');
    const valueRef = valueRefFixture('log_value', bytes);
    const run = runFixture({
      calls: [
        callFixture({
          id: 'root',
          payloadIds: ['log-1'],
        }),
      ],
      payloads: [
        payloadFixture({
          id: 'log-1',
          callNodeId: null,
          kind: {
            type: 'log',
            level: 'info',
            message: 'full log body',
            source: null,
            valueRef,
          },
        }),
      ],
    });

    expect(runToTraceRows(run, cacheWith('log_value', bytes))).toEqual([
      expect.objectContaining({
        id: 'root',
        logs: [
          expect.objectContaining({
            id: 'log-1',
            state: 'available',
            value: 'full log body',
          }),
        ],
      }),
    ]);
  });

  it('projects explicit log body availability states', () => {
    const run = runFixture({
      calls: [
        callFixture({
          id: 'root',
          payloadIds: ['log-lost', 'log-truncated', 'log-omitted'],
        }),
      ],
      payloads: [
        payloadFixture({
          id: 'log-lost',
          timestampMs: 101,
          kind: {
            type: 'log',
            level: 'error',
            message: 'lost',
            source: null,
            valueRef: {
              ...valueRefFixture('lost_value', new Uint8Array()),
              availability: 'lost',
              originalSizeBytes: null,
              retainedSizeBytes: null,
              diagnostic: 'queue full',
            },
          },
        }),
        payloadFixture({
          id: 'log-truncated',
          timestampMs: 102,
          body: {
            state: { kind: 'truncated' },
            contentType: null,
            originalSizeBytes: 512,
            retainedSizeBytes: 128,
          },
        }),
        payloadFixture({
          id: 'log-omitted',
          timestampMs: 103,
          body: {
            state: { kind: 'omittedByPolicy' },
            contentType: null,
            originalSizeBytes: null,
            retainedSizeBytes: null,
          },
        }),
      ],
    });

    const logs = runToTraceRows(run)[0].logs;

    expect(logs.map((log) => [log.id, log.state, log.diagnostic])).toEqual([
      ['log-lost', 'lost', 'queue full'],
      ['log-truncated', 'truncated', null],
      ['log-omitted', 'omitted', null],
    ]);
  });

  it('projects profile blocks from RunStore call edges and timestamps', () => {
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

    expect(buildExecutionProfileProjection(run).blocks).toEqual([
      expect.objectContaining({
        id: 'root',
        threadId: 'thread',
        depth: 0,
        durationMs: 200,
        selfMs: 90,
        spanLeftPct: 0,
        spanWidthPct: 100,
      }),
      expect.objectContaining({
        id: 'child-a',
        threadId: 'thread',
        depth: 1,
        functionName: 'user.ChildA',
        durationMs: 50,
        spanLeftPct: 10,
        spanWidthPct: 25,
      }),
      expect.objectContaining({
        id: 'child-b',
        threadId: 'thread',
        depth: 1,
        functionName: 'user.ChildB',
        durationMs: 60,
        spanLeftPct: 50,
        spanWidthPct: 30,
      }),
    ]);
  });

  it('projects spawned thread roots under their parent call stack', () => {
    const run = runFixture({
      rootCallNodeId: 'root',
      threads: [
        threadFixture({
          id: 'main-thread',
          callNodeIds: ['root'],
        }),
        threadFixture({
          id: 'branch-thread',
          parentThreadId: 'main-thread',
          parentCallNodeId: 'root',
          callNodeIds: ['branch', 'leaf'],
        }),
      ],
      calls: [
        callFixture({
          id: 'root',
          threadId: 'main-thread',
          functionName: 'user.FlameGraphFanoutDemo',
          startedAtNs: '100000000',
          endedAtNs: '500000000',
        }),
        callFixture({
          id: 'branch',
          threadId: 'branch-thread',
          parentId: null,
          functionId: 2,
          functionName: 'user.fg_branch',
          startedAtNs: '150000000',
          endedAtNs: '450000000',
        }),
        callFixture({
          id: 'leaf',
          threadId: 'branch-thread',
          parentId: 'branch',
          functionId: 3,
          functionName: 'user.fg_leaf_sleep',
          startedAtNs: '200000000',
          endedAtNs: '300000000',
        }),
      ],
    });

    expect(runToTraceRows(run)).toEqual([
      expect.objectContaining({ id: 'root', depth: 0 }),
      expect.objectContaining({ id: 'branch', depth: 1 }),
      expect.objectContaining({ id: 'leaf', depth: 2 }),
    ]);
    expect(buildExecutionProfileProjection(run).blocks).toEqual([
      expect.objectContaining({
        id: 'root',
        threadId: 'main-thread',
        depth: 0,
        selfMs: 100,
      }),
      expect.objectContaining({
        id: 'branch',
        threadId: 'branch-thread',
        depth: 1,
        selfMs: 200,
      }),
      expect.objectContaining({
        id: 'leaf',
        threadId: 'branch-thread',
        depth: 2,
        selfMs: 100,
      }),
    ]);
  });

  it('aggregates execution profile rows by function', () => {
    const projection = buildExecutionProfileProjection(
      runFixture({
        rootCallNodeId: 'root',
        calls: [
          callFixture({
            id: 'root',
            functionName: 'user.Main',
            startedAtNs: '0',
            endedAtNs: '400000000',
          }),
          callFixture({
            id: 'work-a',
            parentId: 'root',
            functionName: 'user.Work',
            startedAtNs: '50000000',
            endedAtNs: '150000000',
          }),
          callFixture({
            id: 'work-b',
            parentId: 'root',
            functionName: 'user.Work',
            startedAtNs: '200000000',
            endedAtNs: '300000000',
          }),
        ],
      }),
    );

    expect(projection.functionRows).toEqual([
      expect.objectContaining({
        functionName: 'user.Main',
        callCount: 1,
        selfMs: 200,
        totalMs: 400,
      }),
      expect.objectContaining({
        functionName: 'user.Work',
        callCount: 2,
        selfMs: 200,
        totalMs: 200,
      }),
    ]);
  });

  it('finds search matches without filtering profile blocks', () => {
    const projection = buildExecutionProfileProjection(
      runFixture({
        calls: [
          callFixture({
            id: 'left',
            functionName: 'user.LeftBranch',
            startedAtNs: '0',
            endedAtNs: '100000000',
          }),
          callFixture({
            id: 'right',
            functionName: 'user.RightBranch',
            startedAtNs: '100000000',
            endedAtNs: '200000000',
          }),
        ],
      }),
    );

    const visible = filterExecutionProfileProjection(projection, {
      includeSystemCalls: true,
    });

    expect(visible.blocks.map((block) => block.id)).toEqual(['left', 'right']);
    expect(executionProfileSearchFunctionKeys(visible, 'left')).toEqual([
      'user:user.LeftBranch',
    ]);
  });

  it('hides system frames and reparents visible descendants', () => {
    const projection = buildExecutionProfileProjection(
      runFixture({
        rootCallNodeId: 'root',
        calls: [
          callFixture({
            id: 'root',
            functionName: 'user.Main',
            startedAtNs: '0',
            endedAtNs: '400000000',
          }),
          callFixture({
            id: 'system',
            parentId: 'root',
            functionName: 'baml.sys.sleep',
            functionOrigin: 'builtin',
            startedAtNs: '50000000',
            endedAtNs: '350000000',
          }),
          callFixture({
            id: 'leaf',
            parentId: 'system',
            functionName: 'user.Leaf',
            startedAtNs: '100000000',
            endedAtNs: '200000000',
          }),
        ],
      }),
    );

    const withSystem = filterExecutionProfileProjection(projection, {
      includeSystemCalls: true,
    });
    const withoutSystem = filterExecutionProfileProjection(projection, {
      includeSystemCalls: false,
    });

    expect(withSystem.blocks.find((block) => block.id === 'leaf')).toEqual(
      expect.objectContaining({ parentId: 'system', depth: 2 }),
    );
    expect(withoutSystem.blocks.map((block) => block.id)).toEqual([
      'root',
      'leaf',
    ]);
    expect(withoutSystem.blocks.find((block) => block.id === 'leaf')).toEqual(
      expect.objectContaining({ parentId: 'root', depth: 1 }),
    );
    expect(withoutSystem.blocks.find((block) => block.id === 'root')).toEqual(
      expect.objectContaining({ selfMs: 300 }),
    );
  });

  it('exposes stable execution profile color keys', () => {
    const projection = buildExecutionProfileProjection(
      runFixture({
        calls: [
          callFixture({
            id: 'call',
            threadId: 'thread-a',
            functionName: 'user.Main',
          }),
        ],
      }),
    );
    const block = projection.blocks[0];

    expect(executionProfileColorKey(block, 'function')).toBe(
      block.functionKey,
    );
    expect(executionProfileColorKey(block, 'origin')).toBe('user');
    expect(executionProfileColorKey(block, 'thread')).toBe('thread-a');
  });
});

function runFixture(overrides: Partial<Run>): Run {
  return {
    boundaryId: 'run-1',
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
      source: null,
      valueRef: null,
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

function outboundStringBytes(value: string): Uint8Array {
  return BamlOutboundValue.encode({
    value: { $case: 'stringValue', stringValue: value },
  }).finish();
}

function valueRefFixture(id: string, bytes: Uint8Array): ValueRef {
  return {
    id,
    codec: 'bamlOutboundValue',
    availability: 'available',
    originalSizeBytes: bytes.length,
    retainedSizeBytes: bytes.length,
    diagnostic: null,
  };
}

function cacheWith(valueRefId: string, bytes: Uint8Array): ValueBodyCache {
  return {
    get: () => ({
      boundaryId: 'run-1',
      valueRefId,
      codec: 'bamlOutboundValue',
      availability: 'available',
      bytes,
      diagnostic: null,
    }),
    read: async () => {
      throw new Error('cache hit should not read');
    },
    subscribe: () => () => {},
  };
}

function threadFixture(
  overrides: Partial<Run['threads'][number]>,
): Run['threads'][number] {
  return {
    id: 'thread',
    parentThreadId: null,
    parentCallNodeId: null,
    name: null,
    startedAtNs: '0',
    endedAtNs: '0',
    status: 'completed',
    callNodeIds: [],
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
