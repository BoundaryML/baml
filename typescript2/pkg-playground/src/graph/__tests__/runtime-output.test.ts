import { describe, expect, it } from 'vitest';
import type { BamlJsMedia, BamlJsValue } from '@b/pkg-proto';
import type { DeserializedRuntimeEvent } from '../../worker-protocol';
import type { GraphNode } from '../types';
import { collectGraphNodeOutputs } from '../runtime-output';

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
    metadata: { logFilterKey: 'workflow', sourceExpr: null, isContainer: false },
  },
  {
    id: '2',
    label: 'GenerateImages(prompt)',
    type: 'scope',
    parent: null,
    metadata: { logFilterKey: 'call', sourceExpr: null, isContainer: false },
  },
];

function functionEndEvent(name: string, result: BamlJsValue): DeserializedRuntimeEvent {
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
});
