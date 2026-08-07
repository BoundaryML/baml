import { describe, expect, it } from 'vitest';
import { BamlOutboundValue } from '@b/pkg-proto';

import {
  buildExecutionProfileProjection,
  executionProfileColorKey,
  executionProfileSearchFunctionKeys,
  filterExecutionProfileProjection,
  runToGraphNodeValues,
  runToDisplayRun,
  runToOutputChunks,
  runToTraceRows,
} from './run-store-projections';
import type { GraphRuntimeOverlay, Run, ValueRef } from './worker-protocol';
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

  it('attaches call input/output/error captured values under their owning call nodes', () => {
    const inputBytes = outboundStringBytes('args');
    const outputBytes = outboundStringBytes('ok');
    const errorBytes = outboundStringBytes('boom');
    const inputRef = valueRefFixture('call_input', inputBytes);
    const outputRef = valueRefFixture('call_output', outputBytes);
    const errorRef = valueRefFixture('call_error', errorBytes);
    const run = runFixture({
      calls: [
        callFixture({
          id: 'root',
          payloadIds: [],
        }),
        callFixture({
          id: 'child',
          parentId: 'root',
          functionName: 'user.leaf',
          payloadIds: ['payload-input', 'payload-error'],
        }),
      ],
      payloads: [
        payloadFixture({
          id: 'payload-input',
          callNodeId: 'child',
          timestampMs: 100,
          kind: {
            type: 'capturedValue',
            role: 'callInput',
            label: 'inputs',
            valueRef: inputRef,
          },
        }),
        payloadFixture({
          id: 'payload-output',
          callNodeId: 'child',
          timestampMs: 101,
          kind: {
            type: 'capturedValue',
            role: 'callOutput',
            label: 'output',
            valueRef: outputRef,
          },
        }),
        payloadFixture({
          id: 'payload-error',
          callNodeId: null,
          timestampMs: 102,
          kind: {
            type: 'capturedValue',
            role: 'callError',
            label: 'error',
            valueRef: errorRef,
          },
        }),
      ],
    });

    const rows = runToTraceRows(
      run,
      cacheWithEntries({
        call_input: inputBytes,
        call_output: outputBytes,
        call_error: errorBytes,
      }),
    );

    expect(rows.find((row) => row.id === 'root')?.callValues).toEqual([]);
    expect(rows.find((row) => row.id === 'child')?.callValues).toEqual([
      expect.objectContaining({
        id: 'payload-input',
        role: 'callInput',
        label: 'inputs',
        state: 'available',
        value: 'args',
      }),
      expect.objectContaining({
        id: 'payload-output',
        role: 'callOutput',
        label: 'output',
        state: 'available',
        value: 'ok',
      }),
      expect.objectContaining({
        id: 'payload-error',
        role: 'callError',
        label: 'error',
        state: 'available',
        value: 'boom',
      }),
    ]);
  });

  it('projects graph node value previews from captured call values', () => {
    const inputBytes = outboundStringBytes('prompt text');
    const outputBytes = outboundImageBytes('https://example.com/generated.png');
    const errorBytes = outboundStringBytes('bad image');
    const inputRef = valueRefFixture('graph_input', inputBytes);
    const outputRef = valueRefFixture('image_output', outputBytes);
    const errorRef = valueRefFixture('graph_error', errorBytes);
    const run = runFixture({
      calls: [
        callFixture({
          id: 'image-call',
          payloadIds: ['payload-input', 'payload-output', 'payload-error'],
        }),
      ],
      payloads: [
        payloadFixture({
          id: 'payload-input',
          callNodeId: null,
          timestampMs: 99,
          kind: {
            type: 'capturedValue',
            role: 'callInput',
            label: 'inputs',
            valueRef: inputRef,
          },
        }),
        payloadFixture({
          id: 'payload-output',
          callNodeId: null,
          timestampMs: 100,
          kind: {
            type: 'capturedValue',
            role: 'callOutput',
            label: 'generated',
            valueRef: outputRef,
          },
        }),
        payloadFixture({
          id: 'payload-error',
          callNodeId: null,
          timestampMs: 101,
          kind: {
            type: 'capturedValue',
            role: 'callError',
            label: 'error',
            valueRef: errorRef,
          },
        }),
      ],
    });

    const values = runToGraphNodeValues(
      run,
      graphOverlayFixture(7, ['image-call']),
      cacheWithEntries({
        graph_input: inputBytes,
        image_output: outputBytes,
        graph_error: errorBytes,
      }),
    ).get('7');

    expect(values).toEqual([
      expect.objectContaining({
        id: 'payload-input',
        role: 'callInput',
        label: 'inputs',
        value: 'prompt text',
      }),
      expect.objectContaining({
        id: 'payload-output',
        role: 'callOutput',
        label: 'generated',
        value: expect.objectContaining({
          media_type: 'image',
          content_type: 'url',
          url: 'https://example.com/generated.png',
        }),
      }),
      expect.objectContaining({
        id: 'payload-error',
        role: 'callError',
        label: 'error',
        value: 'bad image',
      }),
    ]);
  });

  it('projects root input values onto the root call graph node', () => {
    const inputBytes = outboundStringBytes('a happy golden retriever');
    const inputRef = valueRefFixture('root_input', inputBytes);
    const run = runFixture({
      rootCallNodeId: 'root-call',
      calls: [callFixture({ id: 'root-call', payloadIds: [] })],
      payloads: [
        payloadFixture({
          id: 'payload-root-input',
          timestampMs: 98,
          kind: {
            type: 'capturedValue',
            role: 'rootInput',
            label: 'inputs',
            valueRef: inputRef,
          },
        }),
      ],
    });

    const values = runToGraphNodeValues(
      run,
      graphOverlayFixture(7, ['root-call']),
      cacheWith('root_input', inputBytes),
    ).get('7');

    expect(values).toEqual([
      expect.objectContaining({
        id: 'payload-root-input',
        role: 'callInput',
        label: 'inputs',
        value: 'a happy golden retriever',
      }),
    ]);
  });

  it('projects direct root run result values onto the root call graph node', () => {
    const bytes = outboundImageBytes('https://example.com/root-result.png');
    const valueRef = valueRefFixture('root_result', bytes);
    const run = runFixture({
      status: 'succeeded',
      completedAtMs: 150,
      rootCallNodeId: 'root-llm-call',
      result: {
        valueRef,
        rendererHint: null,
        supportingPayloadIds: [],
      },
      calls: [callFixture({ id: 'root-llm-call', payloadIds: [] })],
      payloads: [],
    });

    const values = runToGraphNodeValues(
      run,
      graphOverlayFixture(11, ['root-llm-call']),
      cacheWith('root_result', bytes),
    ).get('11');

    expect(values).toEqual([
      expect.objectContaining({
        id: 'root-result',
        role: 'callOutput',
        label: 'output',
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
      status: 'succeeded',
      completedAtMs: 150,
      rootCallNodeId: 'root-user-function',
      result: {
        valueRef,
        rendererHint: null,
        supportingPayloadIds: [],
      },
      calls: [
        callFixture({
          id: 'root-user-function',
          functionName: 'user.generate_image',
          payloadIds: [],
        }),
        callFixture({
          id: 'inner-llm-call',
          functionName: 'baml.llm.call_llm_function',
          parentId: 'root-user-function',
          payloadIds: [],
        }),
      ],
      payloads: [],
    });

    const valuesByNodeId = runToGraphNodeValues(
      run,
      {
        ...graphOverlayFixture(12, ['inner-llm-call']),
        unattachedCallNodeIds: ['root-user-function'],
      },
      cacheWith('direct_llm_result', bytes),
      { rootGraphNodeId: '1' },
    );

    expect(valuesByNodeId.get('12')).toBeUndefined();
    expect(valuesByNodeId.get('1')).toEqual([
      expect.objectContaining({
        id: 'root-result',
        role: 'callOutput',
        label: 'output',
        value: expect.objectContaining({
          media_type: 'image',
          url: 'https://example.com/direct-llm.png',
        }),
      }),
    ]);
  });

  it('keeps root input and result separate from child call values', () => {
    const rootInputBytes = outboundStringBytes('Paulo Rodrigues');
    const rootResultBytes = outboundStringBytes(
      'Hello, Paulo. Your full name is Paulo Rodrigues!',
    );
    const childInputBytes = outboundStringBytes('Paulo Rodrigues');
    const childOutputBytes = outboundStringBytes('["Paulo","Rodrigues"]');
    const rootInputRef = valueRefFixture('root_input_screenshot', rootInputBytes);
    const rootResultRef = valueRefFixture('root_result_screenshot', rootResultBytes);
    const childInputRef = valueRefFixture('child_input', childInputBytes);
    const childOutputRef = valueRefFixture('child_output', childOutputBytes);
    const run = runFixture({
      status: 'succeeded',
      completedAtMs: 150,
      rootCallNodeId: 'root-main',
      result: {
        valueRef: rootResultRef,
        rendererHint: null,
        supportingPayloadIds: [],
      },
      calls: [
        callFixture({
          id: 'root-main',
          functionName: 'throws.main',
          payloadIds: [],
        }),
        callFixture({
          id: 'child-call',
          functionName: 'throws.child',
          parentId: 'root-main',
          payloadIds: ['payload-child-input', 'payload-child-output'],
        }),
      ],
      payloads: [
        payloadFixture({
          id: 'payload-root-input',
          timestampMs: 98,
          kind: {
            type: 'capturedValue',
            role: 'rootInput',
            label: 'inputs',
            valueRef: rootInputRef,
          },
        }),
        payloadFixture({
          id: 'payload-child-input',
          callNodeId: 'child-call',
          timestampMs: 99,
          kind: {
            type: 'capturedValue',
            role: 'callInput',
            label: 'inputs',
            valueRef: childInputRef,
          },
        }),
        payloadFixture({
          id: 'payload-child-output',
          callNodeId: 'child-call',
          timestampMs: 100,
          kind: {
            type: 'capturedValue',
            role: 'callOutput',
            label: 'output',
            valueRef: childOutputRef,
          },
        }),
      ],
    });

    const valuesByNodeId = runToGraphNodeValues(
      run,
      {
        ...graphOverlayFixture(12, ['child-call']),
        unattachedCallNodeIds: ['root-main'],
      },
      cacheWithEntries({
        root_input_screenshot: rootInputBytes,
        root_result_screenshot: rootResultBytes,
        child_input: childInputBytes,
        child_output: childOutputBytes,
      }),
      { rootGraphNodeId: '1' },
    );

    expect(valuesByNodeId.get('12')).toEqual([
      expect.objectContaining({
        id: 'payload-child-input',
        role: 'callInput',
        label: 'inputs',
        value: 'Paulo Rodrigues',
      }),
      expect.objectContaining({
        id: 'payload-child-output',
        role: 'callOutput',
        label: 'output',
        value: '["Paulo","Rodrigues"]',
      }),
    ]);
    expect(valuesByNodeId.get('1')).toEqual([
      expect.objectContaining({
        id: 'payload-root-input',
        role: 'callInput',
        label: 'inputs',
        value: 'Paulo Rodrigues',
      }),
      expect.objectContaining({
        id: 'root-result',
        role: 'callOutput',
        label: 'output',
        value: 'Hello, Paulo. Your full name is Paulo Rodrigues!',
      }),
    ]);
  });

  it('keeps child call errors separate from root errors', () => {
    const childErrorBytes = outboundStringBytes('child failed');
    const rootErrorBytes = outboundStringBytes('root failed');
    const childErrorRef = valueRefFixture('child_error', childErrorBytes);
    const rootErrorRef = valueRefFixture('root_error', rootErrorBytes);
    const run = runFixture({
      status: 'failed',
      completedAtMs: 150,
      rootCallNodeId: 'root-main',
      error: {
        class: 'Runtime',
        message: 'root failed',
        details: null,
        valueRef: rootErrorRef,
      },
      calls: [
        callFixture({
          id: 'root-main',
          functionName: 'throws.main',
          payloadIds: [],
          status: 'errored',
        }),
        callFixture({
          id: 'child-call',
          functionName: 'throws.child',
          parentId: 'root-main',
          payloadIds: ['payload-child-error'],
          status: 'errored',
        }),
      ],
      payloads: [
        payloadFixture({
          id: 'payload-child-error',
          callNodeId: 'child-call',
          timestampMs: 100,
          kind: {
            type: 'capturedValue',
            role: 'callError',
            label: 'error',
            valueRef: childErrorRef,
          },
        }),
      ],
    });

    const valuesByNodeId = runToGraphNodeValues(
      run,
      {
        ...graphOverlayFixture(12, ['child-call']),
        unattachedCallNodeIds: ['root-main'],
      },
      cacheWithEntries({
        child_error: childErrorBytes,
        root_error: rootErrorBytes,
      }),
      { rootGraphNodeId: '1' },
    );

    expect(valuesByNodeId.get('12')).toEqual([
      expect.objectContaining({
        id: 'payload-child-error',
        role: 'callError',
        label: 'error',
        value: 'child failed',
      }),
    ]);
    expect(valuesByNodeId.get('1')).toEqual([
      expect.objectContaining({
        id: 'root-error',
        role: 'callError',
        label: 'error',
        value: 'root failed',
      }),
    ]);
  });

  it('projects root errors without captured values onto the root graph node', () => {
    const run = runFixture({
      status: 'failed',
      completedAtMs: 150,
      rootCallNodeId: 'root-main',
      error: {
        class: 'Runtime',
        message: 'plain runtime failure',
        details: null,
        valueRef: null,
      },
      calls: [
        callFixture({
          id: 'root-main',
          functionName: 'throws.main',
          payloadIds: [],
          status: 'errored',
        }),
      ],
    });

    const valuesByNodeId = runToGraphNodeValues(
      run,
      {
        ...graphOverlayFixture(12, []),
        unattachedCallNodeIds: ['root-main'],
      },
      undefined,
      { rootGraphNodeId: '1' },
    );

    expect(valuesByNodeId.get('1')).toEqual([
      expect.objectContaining({
        id: 'root-error',
        role: 'callError',
        label: 'error',
        value: null,
        state: 'error',
        diagnostic: 'plain runtime failure',
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

  it('keeps baml.io output chunks verbatim, in order, with streams interleaved', () => {
    const run = runFixture({
      payloads: [
        payloadFixture({
          id: 'out-1',
          timestampMs: 100,
          // An escape sequence split across two print calls: reshaping or
          // reordering chunks would corrupt it.
          kind: { type: 'output', stream: 'stdout', text: '[3' },
        }),
        payloadFixture({
          id: 'out-2',
          timestampMs: 101,
          kind: { type: 'output', stream: 'stderr', text: 'warn\n' },
        }),
        payloadFixture({
          id: 'out-3',
          timestampMs: 102,
          kind: { type: 'output', stream: 'stdout', text: '1mred[0m' },
        }),
        payloadFixture({
          id: 'log-1',
          timestampMs: 103,
          kind: {
            type: 'log',
            level: 'info',
            message: 'not output',
            source: null,
            valueRef: null,
          },
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
    id,
    codec: 'bamlOutboundValue',
    availability: 'available',
    originalSizeBytes: bytes.length,
    retainedSizeBytes: bytes.length,
    diagnostic: null,
  };
}

function graphOverlayFixture(
  cfgNodeId: number,
  callNodeIds: string[],
): GraphRuntimeOverlay {
  return {
    boundaryId: 'run-1',
    projectGeneration: 1,
    entries: [{ cfgNodeId, callNodeIds }],
    unattachedCallNodeIds: [],
    diagnostics: [],
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
        boundaryId: 'run-1',
        valueRefId: valueRef.id,
        codec: 'bamlOutboundValue',
        availability: 'available',
        bytes,
        diagnostic: null,
      };
    },
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
