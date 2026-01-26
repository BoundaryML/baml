import { describe, it, expect } from 'vitest';
import { sdkGraphToReactflow } from '../adapter';
import type { GraphNode, GraphEdge } from '../interface';

describe('Graph conversion integration', () => {
  it('converts ConditionalWorkflow graph to expected structure', () => {
    // Root/node ids are numeric strings; logFilterKey stored separately if needed
    const nodes: GraphNode[] = [
      {
        id: '0',
        type: 'group',
        label: 'ConditionalWorkflow',
        functionName: 'ConditionalWorkflow',
        codeHash: '',
        lastModified: 0,
        metadata: { logFilterKey: 'ConditionalWorkflow|root:0' },
      },
      {
        id: '1',
        type: 'group',
        label: 'validate payload structure',
        functionName: 'ConditionalWorkflow',
        parent: '0',
        codeHash: '',
        lastModified: 0,
        metadata: { logFilterKey: 'ConditionalWorkflow|root:0|hdr:validate-payload-structure:0' },
      },
      {
        id: '2',
        type: 'conditional',
        label: 'check summary confidence',
        functionName: 'ConditionalWorkflow',
        parent: '0',
        codeHash: '',
        lastModified: 0,
        metadata: { logFilterKey: 'ConditionalWorkflow|root:0|hdr:check-summary-confidence:1' },
      },
      {
        id: '3',
        type: 'group',
        label: 'if checkCondition(validation.summary)',
        functionName: 'ConditionalWorkflow',
        parent: '2',
        codeHash: '',
        lastModified: 0,
        metadata: { logFilterKey: 'ConditionalWorkflow|root:0|hdr:check-summary-confidence:1|bg:if-checkcondition-validation-summary:0' },
      },
    ];

    const edges: GraphEdge[] = [
      {
        id: '0->1',
        source: '0',
        target: '1',
      },
      {
        id: '1->2',
        source: '1',
        target: '2',
      },
      {
        id: '2->3',
        source: '2',
        target: '3',
      },
    ];

    const reactflow = sdkGraphToReactflow(nodes, edges);

    expect(reactflow.nodes).toHaveLength(nodes.length);
    const rootNode = reactflow.nodes.find((node) => node.id === '0');
    expect(rootNode?.type).toBe('group');

    const headerNode = reactflow.nodes.find((node) =>
      node.id === '1'
    );
    expect(headerNode?.data?.parentId).toBe('0');

    const branchGroup = reactflow.nodes.find((node) =>
      node.id === '3'
    );
    expect(branchGroup).toBeTruthy();
    expect(branchGroup?.data?.parentId).toBe('2');

    // Ensure edges survived with the same count and targets
    expect(reactflow.edges).toHaveLength(edges.length);
    expect(
      reactflow.edges.some(
        (edge) => edge.target === '3'
      )
    ).toBe(true);
  });
});
