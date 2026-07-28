import { describe, expect, it } from 'vitest';

import type { GraphNodeValuePreview } from '../run-store-projections';
import { layoutGraph } from './layout';
import type { WorkflowNode } from './types';

describe('graph value preview layout', () => {
  it.each([
    ['text', 'hydrated output'],
    [
      'image',
      {
        $baml: { type: '$media' },
        media_type: 'image',
        content_type: 'url',
        url: 'https://example.com/output.png',
      },
    ],
  ])(
    'keeps node geometry stable while a captured %s body hydrates',
    async (_kind, hydratedValue) => {
      const pending = await layoutGraph(
        [nodeWithPreview(valuePreview(null, 'loading'))],
        [],
      );
      const hydrated = await layoutGraph(
        [nodeWithPreview(valuePreview(hydratedValue, 'available'))],
        [],
      );

      expect(hydrated.nodes[0]?.style?.width).toBe(
        pending.nodes[0]?.style?.width,
      );
      expect(hydrated.nodes[0]?.style?.height).toBe(
        pending.nodes[0]?.style?.height,
      );
    },
  );
});

function nodeWithPreview(value: GraphNodeValuePreview): WorkflowNode {
  return {
    id: 'node',
    type: 'base',
    position: { x: 0, y: 0 },
    data: {
      label: 'node',
      graphNodeType: 'function',
      executionState: 'running',
      selected: false,
      logFilterKey: 'node',
      valuePreviews: [value],
    },
  };
}

function valuePreview(
  value: GraphNodeValuePreview['value'],
  state: GraphNodeValuePreview['state'],
): GraphNodeValuePreview {
  return {
    id: 'value',
    timestampMs: 0,
    role: 'callOutput',
    label: 'output',
    valueRef: null,
    value,
    state,
    diagnostic: null,
  };
}
