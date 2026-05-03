import { describe, expect, it } from 'vitest';
import type { BamlJsMedia, BamlJsValue } from '@b/pkg-proto';
import type { DeserializedRuntimeEvent } from '../../worker-protocol';
import type { GraphNode } from '../types';
import { collectGraphNodeOutputs, collectGraphNodeRuntime } from '../runtime-output';

const imageA: BamlJsMedia = {
  $baml: { type: '$media' },
  media_type: 'image',
  mime_type: 'image/png',
  content_type: 'url',
  url: 'https://example.com/a.png',
};

const imageB: BamlJsMedia = {
  $baml: { type: '$media' },
  media_type: 'image',
  mime_type: 'image/png',
  content_type: 'url',
  url: 'https://example.com/b.png',
};

const graphNodes: GraphNode[] = [
  {
    id: '1',
    label: 'Workflow',
    type: 'function',
    parent: null,
    metadata: { logFilterKey: 'workflow', sourceExpr: null, isContainer: true },
  },
  {
    id: '2',
    label: 'GenerateImages(prompt)',
    type: 'scope',
    parent: '1',
    metadata: { logFilterKey: 'call', sourceExpr: null, isContainer: false },
  },
];

function functionStartEvent(name: string): DeserializedRuntimeEvent {
  return {
    spanId: `${name}-span`,
    rootSpanId: 'root',
    timestampMs: 1,
    callStack: [],
    event: {
      $case: 'functionStart',
      functionStart: {
        name,
        args: [],
      },
    },
  };
}

function functionEndEvent(name: string, result: BamlJsValue, error?: string): DeserializedRuntimeEvent {
  return {
    spanId: `${name}-span`,
    rootSpanId: 'root',
    timestampMs: 1,
    callStack: [],
    event: {
      $case: 'functionEnd',
      functionEnd: {
        name,
        durationMs: 12,
        result,
        error,
      },
    },
  };
}

describe('collectGraphNodeOutputs', () => {
  it('maps mixed string/image function results to call-node image previews', () => {
    const mixedResult: BamlJsValue = ['caption', imageA, { nested: imageB } as Record<string, BamlJsValue>];
    const outputs = collectGraphNodeOutputs(graphNodes, [
      functionEndEvent('GenerateImages', mixedResult),
    ]);

    expect(outputs.get('2')?.imageOutputs).toEqual([imageA, imageB]);
    expect(outputs.get('2')?.result).toEqual(mixedResult);
  });

  it('marks function starts as running and propagates state to parent frames', () => {
    const runtime = collectGraphNodeRuntime(graphNodes, [
      functionStartEvent('GenerateImages'),
    ]);

    expect(runtime.get('2')?.executionState).toBe('running');
    expect(runtime.get('1')?.executionState).toBe('running');
  });

  it('marks function ends as success while preserving image previews', () => {
    const runtime = collectGraphNodeRuntime(graphNodes, [
      functionStartEvent('GenerateImages'),
      functionEndEvent('GenerateImages', imageA),
    ]);

    expect(runtime.get('2')?.executionState).toBe('success');
    expect(runtime.get('2')?.imageOutputs).toEqual([imageA]);
    expect(runtime.get('1')?.executionState).toBe('success');
  });

  it('marks errored function ends as error and propagates to parent frames', () => {
    const runtime = collectGraphNodeRuntime(graphNodes, [
      functionStartEvent('GenerateImages'),
      functionEndEvent('GenerateImages', null, 'HTTP request failed'),
    ]);

    expect(runtime.get('2')?.executionState).toBe('error');
    expect(runtime.get('2')?.errorMessage).toBe('HTTP request failed');
    expect(runtime.get('2')?.hasResult).toBe(false);
    expect(runtime.get('1')?.executionState).toBe('error');
    expect(runtime.get('1')?.errorMessage).toBe('HTTP request failed');
  });
});
