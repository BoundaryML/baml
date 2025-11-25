import { describe, it, expect } from 'vitest';
import { sdkGraphToReactflow } from '../adapter';
import type { GraphNode, GraphEdge } from '../interface';

describe('Graph conversion integration', () => {
  it('converts ConditionalWorkflow graph to expected structure', () => {
    // Root node now has lexical_id format with |root:0
    const nodes: GraphNode[] = [
      {
        id: 'ConditionalWorkflow|root:0',
        type: 'group',
        label: 'ConditionalWorkflow',
        functionName: 'ConditionalWorkflow',
        codeHash: '',
        lastModified: 0,
      },
      {
        id: 'ConditionalWorkflow|root:0|hdr:validate-payload-structure:0',
        type: 'group',
        label: 'validate payload structure',
        functionName: 'ConditionalWorkflow',
        parent: 'ConditionalWorkflow|root:0',
        codeHash: '',
        lastModified: 0,
      },
      {
        id: 'ConditionalWorkflow|root:0|hdr:check-summary-confidence:1',
        type: 'conditional',
        label: 'check summary confidence',
        functionName: 'ConditionalWorkflow',
        parent: 'ConditionalWorkflow|root:0',
        codeHash: '',
        lastModified: 0,
      },
      {
        id: 'ConditionalWorkflow|root:0|hdr:check-summary-confidence:1|bg:if-checkcondition-validation-summary:0',
        type: 'group',
        label: 'if checkCondition(validation.summary)',
        functionName: 'ConditionalWorkflow',
        parent: 'ConditionalWorkflow|root:0|hdr:check-summary-confidence:1',
        codeHash: '',
        lastModified: 0,
      },
    ];

    const edges: GraphEdge[] = [
      {
        id: 'ConditionalWorkflow|root:0->ConditionalWorkflow|root:0|hdr:validate-payload-structure:0',
        source: 'ConditionalWorkflow|root:0',
        target: 'ConditionalWorkflow|root:0|hdr:validate-payload-structure:0',
      },
      {
        id: 'ConditionalWorkflow|root:0|hdr:validate-payload-structure:0->ConditionalWorkflow|root:0|hdr:check-summary-confidence:1',
        source: 'ConditionalWorkflow|root:0|hdr:validate-payload-structure:0',
        target: 'ConditionalWorkflow|root:0|hdr:check-summary-confidence:1',
      },
      {
        id: 'ConditionalWorkflow|root:0|hdr:check-summary-confidence:1->ConditionalWorkflow|root:0|hdr:check-summary-confidence:1|bg:if-checkcondition-validation-summary:0',
        source: 'ConditionalWorkflow|root:0|hdr:check-summary-confidence:1',
        target: 'ConditionalWorkflow|root:0|hdr:check-summary-confidence:1|bg:if-checkcondition-validation-summary:0',
      },
    ];

    const reactflow = sdkGraphToReactflow(nodes, edges);

    expect(reactflow.nodes).toHaveLength(nodes.length);
    const rootNode = reactflow.nodes.find((node) => node.id === 'ConditionalWorkflow|root:0');
    expect(rootNode?.type).toBe('group');

    const headerNode = reactflow.nodes.find((node) =>
      node.id === 'ConditionalWorkflow|root:0|hdr:validate-payload-structure:0'
    );
    expect(headerNode?.data?.parentId).toBe('ConditionalWorkflow|root:0');

    const branchGroup = reactflow.nodes.find((node) =>
      node.id === 'ConditionalWorkflow|root:0|hdr:check-summary-confidence:1|bg:if-checkcondition-validation-summary:0'
    );
    expect(branchGroup).toBeTruthy();
    expect(branchGroup?.data?.parentId).toBe('ConditionalWorkflow|root:0|hdr:check-summary-confidence:1');

    // Ensure edges survived with the same count and targets
    expect(reactflow.edges).toHaveLength(edges.length);
    expect(
      reactflow.edges.some(
        (edge) => edge.target === 'ConditionalWorkflow|root:0|hdr:check-summary-confidence:1|bg:if-checkcondition-validation-summary:0'
      )
    ).toBe(true);
  });
});
