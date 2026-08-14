import { describe, expect, it } from 'vitest';

import type { GraphNodeValuePreview } from '../run-store-projections';
import type { WorkflowEdge, WorkflowNode } from './types';
import {
  groupValuePreviewNodeId,
  groupValuePreviewSourceNodeId,
  isGroupValuePreviewNode,
  liftGroupValuePreviews,
} from './value-previews';

describe('liftGroupValuePreviews', () => {
  it('moves group previews into a synthetic child node', () => {
    const preview = valuePreview('root-output');
    const root = node('1', undefined, 'group', [preview]);
    const first = node('2', '1');
    const second = node('3', '1');
    const edge = graphEdge('2', '3');

    const result = liftGroupValuePreviews([root, first, second], [edge]);
    const previewNodeId = groupValuePreviewNodeId('1');
    const liftedRoot = result.nodes.find((item) => item.id === '1');
    const previewNode = result.nodes.find((item) => item.id === previewNodeId);

    expect(liftedRoot?.data.valuePreviews).toEqual([]);
    expect(liftedRoot?.data.groupValuePreviewsLifted).toBe(true);
    expect(previewNode?.type).toBe('base');
    expect(previewNode?.parentId).toBe('1');
    expect(previewNode?.data.valuePreviews).toEqual([preview]);
    expect(isGroupValuePreviewNode(previewNode!)).toBe(true);
    expect(groupValuePreviewSourceNodeId(previewNode!)).toBe('1');
    expect(result.edges).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          source: previewNodeId,
          target: '2',
        }),
      ]),
    );
  });

  it('keeps branch starts ordered below the preview node', () => {
    const preview = valuePreview('root-output');
    const root = node('1', undefined, 'group', [preview]);
    const left = node('2', '1');
    const right = node('3', '1');

    const result = liftGroupValuePreviews([root, left, right], []);
    const previewNodeId = groupValuePreviewNodeId('1');
    const previewEdges = result.edges.filter(
      (edge) => edge.source === previewNodeId,
    );

    expect(previewEdges.map((edge) => edge.target).sort()).toEqual(['2', '3']);
  });

  it('leaves nodes unchanged when no group has previews', () => {
    const root = node('1', undefined, 'group');
    const child = node('2', '1');
    const edge = graphEdge('1', '2');

    expect(liftGroupValuePreviews([root, child], [edge])).toEqual({
      nodes: [root, child],
      edges: [edge],
    });
  });
});

function node(
  id: string,
  parentId?: string,
  type: WorkflowNode['type'] = 'base',
  valuePreviews: GraphNodeValuePreview[] = [],
): WorkflowNode {
  return {
    id,
    type,
    position: { x: 0, y: 0 },
    data: {
      label: id,
      graphNodeType: 'function',
      executionState: 'not-started',
      selected: false,
      logFilterKey: id,
      valuePreviews,
    },
    ...(parentId ? { parentId } : {}),
  };
}

function graphEdge(source: string, target: string): WorkflowEdge {
  return {
    id: `${source}->${target}`,
    source,
    target,
    type: 'base',
    data: {},
  };
}

function valuePreview(id: string): GraphNodeValuePreview {
  return {
    id,
    timestampMs: 0,
    role: 'callOutput',
    label: 'output',
    valueRef: null,
    value: 'ok',
    state: 'available',
    diagnostic: null,
  };
}
