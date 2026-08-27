import { BamlOutboundValue } from '@b/pkg-proto';
import { describe, expect, it } from 'vitest';

import {
  runToDisplayRun,
  runToGraphNodeValues,
  runToOutputChunks,
} from './run-store-projections';
import type { ValueBodyCache } from './value-body-cache';
import type { Run, ValueRef } from './worker-protocol';

describe('run-store-projections', () => {
  it('projects fetch payloads from RunStore snapshots without exposing values', () => {
    const run = runFixture({
      payloads: [
        {
          body: null,
          callNodeId: null,
          id: '1',
          kind: {
            fetchId: '9',
            method: 'POST',
            requestHeaders: [{ name: 'authorization', valueRedacted: true }],
            type: 'fetchStarted',
            url: 'https://example.test',
          },
          redaction: {
            displaySafe: false,
            policyId: 'test',
            reason: 'redacted',
            valueRedacted: true,
          },
          timestampMs: 120,
        },
        {
          body: null,
          callNodeId: null,
          id: '2',
          kind: {
            durationMs: 30,
            error: null,
            fetchId: '9',
            responseHeaders: [{ name: 'content-type', valueRedacted: true }],
            status: 200,
            type: 'fetchUpdated',
          },
          redaction: {
            displaySafe: false,
            policyId: 'test',
            reason: 'redacted',
            valueRedacted: true,
          },
          timestampMs: 150,
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
        durationMs: 30,
        id: 9,
        method: 'POST',
        requestHeaders: { authorization: '<redacted>' },
        responseHeaders: { 'content-type': '<redacted>' },
        status: 200,
        url: 'https://example.test',
      }),
    ]);
  });

  it('projects terminal status and duration from RunStore outcome fields', () => {
    const run = runFixture({
      completedAtMs: 175,
      error: {
        class: 'Runtime',
        details: null,
        message: 'boom',
        valueRef: null,
      },
      startedAtMs: 110,
      status: 'failed',
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
        rendererHint: 'baml.outbound.base64',
        supportingPayloadIds: [],
        valueRef,
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
        error: {
          class: 'Runtime',
          details: null,
          message: 'failed',
          valueRef,
        },
        status: 'failed',
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
          entries: [
            {
              key: 'topic',
              value: {
                value: { $case: 'stringValue', stringValue: 'volcanoes' },
              },
            },
          ],
          keyType: undefined,
          valueType: undefined,
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
              label: 'inputs',
              role: 'rootInput',
              type: 'capturedValue',
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
      request: {
        argsSummary: null,
        optionsSummary: null,
        projectGeneration: 7,
        projectId: 'project',
        target: { generation: 7, kind: 'test', testName: 'suite/test' },
      },
      target: { generation: 7, kind: 'test', testName: 'suite/test' },
    });

    const display = runToDisplayRun(run, {});

    expect(display).toMatchObject({
      argsJson: '',
      functionName: 'testing.run_test',
      id: 'run-1',
      kind: 'test',
      projectGeneration: 7,
      testName: 'suite/test',
    });
  });

  it('projects only unresolved input requests from RunStore payloads', () => {
    const run = runFixture({
      payloads: [
        payloadFixture({
          id: 'input-1',
          kind: {
            prompt: 'Name?',
            requestId: '1',
            state: 'pending',
            type: 'inputRequested',
          },
        }),
        payloadFixture({
          id: 'input-2',
          kind: {
            prompt: 'City?',
            requestId: '2',
            state: 'pending',
            type: 'inputRequested',
          },
        }),
        payloadFixture({
          id: 'input-3',
          kind: {
            requestId: '1',
            state: 'resolved',
            type: 'inputResolved',
          },
        }),
      ],
    });

    const display = runToDisplayRun(run, {});

    expect(display?.inputRequests).toEqual([{ id: '2', prompt: 'City?' }]);
  });

  it('projects root input values onto the root call graph node', () => {
    const inputBytes = outboundStringBytes('a happy golden retriever');
    const inputRef = valueRefFixture('root_input', inputBytes);
    const run = runFixture({
      payloads: [
        payloadFixture({
          id: 'payload-root-input',
          kind: {
            label: 'inputs',
            role: 'rootInput',
            type: 'capturedValue',
            valueRef: inputRef,
          },
          timestampMs: 98,
        }),
      ],
    });

    const values = runToGraphNodeValues(
      run,
      cacheWith('root_input', inputBytes),
      { rootGraphNodeId: '7' },
    ).get('7');

    expect(values).toEqual([
      expect.objectContaining({
        id: 'payload-root-input',
        label: 'inputs',
        role: 'callInput',
        value: 'a happy golden retriever',
      }),
    ]);
  });

  it('projects direct root run result values onto the root call graph node', () => {
    const bytes = outboundImageBytes('https://example.com/root-result.png');
    const valueRef = valueRefFixture('root_result', bytes);
    const run = runFixture({
      completedAtMs: 150,
      payloads: [],
      result: {
        rendererHint: null,
        supportingPayloadIds: [],
        valueRef,
      },
      status: 'succeeded',
    });

    const values = runToGraphNodeValues(run, cacheWith('root_result', bytes), {
      rootGraphNodeId: '11',
    }).get('11');

    expect(values).toEqual([
      expect.objectContaining({
        id: 'root-result',
        label: 'output',
        role: 'callOutput',
        value: expect.objectContaining({
          media_type: 'image',
          url: 'https://example.com/root-result.png',
        }),
      }),
    ]);
  });

  it('projects root run result values onto the provided root graph node instead of the only visible descendant', () => {
    const bytes = outboundImageBytes('https://example.com/direct-llm.png');
    const valueRef = valueRefFixture('direct_llm_result', bytes);
    const run = runFixture({
      completedAtMs: 150,
      payloads: [],
      result: {
        rendererHint: null,
        supportingPayloadIds: [],
        valueRef,
      },
      status: 'succeeded',
    });

    const valuesByNodeId = runToGraphNodeValues(
      run,
      cacheWith('direct_llm_result', bytes),
      { rootGraphNodeId: '1' },
    );

    expect(valuesByNodeId.get('12')).toBeUndefined();
    expect(valuesByNodeId.get('1')).toEqual([
      expect.objectContaining({
        id: 'root-result',
        label: 'output',
        role: 'callOutput',
        value: expect.objectContaining({
          media_type: 'image',
          url: 'https://example.com/direct-llm.png',
        }),
      }),
    ]);
  });

  it('projects root errors without captured values onto the root graph node', () => {
    const run = runFixture({
      completedAtMs: 150,
      error: {
        class: 'Runtime',
        details: null,
        message: 'plain runtime failure',
        valueRef: null,
      },
      status: 'failed',
    });

    const valuesByNodeId = runToGraphNodeValues(run, undefined, {
      rootGraphNodeId: '1',
    });

    expect(valuesByNodeId.get('1')).toEqual([
      expect.objectContaining({
        diagnostic: 'plain runtime failure',
        id: 'root-error',
        label: 'error',
        role: 'callError',
        state: 'error',
        value: null,
      }),
    ]);
  });

  it('keeps baml.io output chunks verbatim, in order, with streams interleaved', () => {
    const run = runFixture({
      payloads: [
        payloadFixture({
          id: 'out-1',
          // An escape sequence split across two print calls: reshaping or
          // reordering chunks would corrupt it.
          kind: { stream: 'stdout', text: '[3', type: 'output' },
          timestampMs: 100,
        }),
        payloadFixture({
          id: 'out-2',
          kind: { stream: 'stderr', text: 'warn\n', type: 'output' },
          timestampMs: 101,
        }),
        payloadFixture({
          id: 'out-3',
          kind: { stream: 'stdout', text: '1mred[0m', type: 'output' },
          timestampMs: 102,
        }),
        payloadFixture({
          id: 'log-1',
          kind: {
            level: 'info',
            message: 'not output',
            source: null,
            type: 'log',
            valueRef: null,
          },
          timestampMs: 103,
        }),
      ],
    });

    expect(runToOutputChunks(run)).toEqual([
      { id: 'out-1', stream: 'stdout', text: '[3', timestampMs: 100 },
      { id: 'out-2', stream: 'stderr', text: 'warn\n', timestampMs: 101 },
      { id: 'out-3', stream: 'stdout', text: '1mred[0m', timestampMs: 102 },
    ]);
  });

  it('returns no output chunks for a run that never printed', () => {
    expect(runToOutputChunks(runFixture({ payloads: [] }))).toEqual([]);
  });
});

function runFixture(overrides: Partial<Run>): Run {
  return {
    boundaryId: 'run-1',
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
      projectGeneration: 1,
      projectId: 'project',
      target: { functionName: 'main', kind: 'function' },
    },
    result: null,
    startedAtMs: 100,
    status: 'running',
    target: { functionName: 'main', kind: 'function' },
    timeAnchor: {
      epochCreatedAtMs: 100,
      traceZeroNs: '0',
    },
    visibility: { kind: 'history' },
    ...overrides,
  };
}

function payloadFixture(
  overrides: Partial<Run['payloads'][number]>,
): Run['payloads'][number] {
  return {
    body: null,
    callNodeId: null,
    id: 'payload',
    kind: {
      level: 'info',
      message: 'placeholder',
      source: null,
      type: 'log',
      valueRef: null,
    },
    redaction: {
      displaySafe: true,
      policyId: null,
      reason: null,
      valueRedacted: false,
    },
    timestampMs: 100,
    ...overrides,
  };
}

function outboundStringBytes(value: string): Uint8Array {
  return BamlOutboundValue.encode({
    value: { $case: 'stringValue', stringValue: value },
  }).finish();
}

function outboundImageBytes(url: string): Uint8Array {
  return BamlOutboundValue.encode({
    value: {
      $case: 'mediaValue',
      mediaValue: {
        media: 1,
        mimeType: 'image/png',
        value: { $case: 'url', url },
      },
    },
  }).finish();
}

function valueRefFixture(id: string, bytes: Uint8Array): ValueRef {
  return {
    availability: 'available',
    codec: 'bamlOutboundValue',
    diagnostic: null,
    id,
    originalSizeBytes: bytes.length,
    retainedSizeBytes: bytes.length,
  };
}

function cacheWith(valueRefId: string, bytes: Uint8Array): ValueBodyCache {
  return cacheWithEntries({ [valueRefId]: bytes });
}

function cacheWithEntries(entries: Record<string, Uint8Array>): ValueBodyCache {
  return {
    get: (_boundaryId, valueRef) => {
      const bytes = entries[valueRef.id];
      if (!bytes) return undefined;
      return {
        availability: 'available',
        boundaryId: 'run-1',
        bytes,
        codec: 'bamlOutboundValue',
        diagnostic: null,
        valueRefId: valueRef.id,
      };
    },
    read: async () => {
      throw new Error('cache hit should not read');
    },
    subscribe: () => () => {},
  };
}
