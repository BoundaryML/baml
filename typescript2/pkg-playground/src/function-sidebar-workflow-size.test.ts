import { describe, expect, it } from 'vitest';

import {
  buildFunctionSidebarTree,
  type FunctionSidebarFunctionNode,
  type FunctionSidebarTreeNode,
} from './function-sidebar-tree';
import type { FunctionInfo } from './worker-protocol';

describe('function sidebar workflow size', () => {
  it('sorts functions by rendered workflow node count while preserving ties', () => {
    const tree = buildFunctionSidebarTree(
      [
        functionInfo('Small'),
        functionInfo('LargeFirst'),
        functionInfo('LargeSecond'),
        functionInfo('Medium'),
      ],
      {
        workflowNodeCounts: new Map([
          ['Small', 2],
          ['LargeFirst', 10],
          ['LargeSecond', 10],
          ['Medium', 5],
        ]),
      },
    );

    expect(tree.nodes.map(nodeLabel)).toEqual([
      'LargeFirst',
      'LargeSecond',
      'Medium',
      'Small',
    ]);
  });

  it('keeps the workflow node count on each function node for its badge', () => {
    const tree = buildFunctionSidebarTree(
      [functionInfo('OneNode'), functionInfo('ManyNodes')],
      {
        workflowNodeCounts: new Map([
          ['OneNode', 1],
          ['ManyNodes', 12],
        ]),
      },
    );

    expect(functionNode(tree.nodes, 'OneNode').workflowNodeCount).toBe(1);
    expect(functionNode(tree.nodes, 'ManyNodes').workflowNodeCount).toBe(12);
  });

  it('sorts naturally by alphanumeric name while preserving source-order ties', () => {
    const tree = buildFunctionSidebarTree(
      [
        functionInfo('Function10'),
        functionInfo('function2'),
        functionInfo('Function2'),
        functionInfo('Function1'),
      ],
      {
        sortOrder: 'alphanumeric',
        workflowNodeCounts: new Map([
          ['Function10', 20],
          ['function2', 1],
          ['Function2', 10],
          ['Function1', 5],
        ]),
      },
    );

    expect(tree.nodes.map(nodeLabel)).toEqual([
      'Function1',
      'function2',
      'Function2',
      'Function10',
    ]);
    expect(functionNode(tree.nodes, 'Function10').workflowNodeCount).toBe(20);
  });
});

function functionInfo(name: string): FunctionInfo {
  return {
    kind: 'expr',
    name,
    origin: 'userDefined',
  };
}

function functionNode(
  nodes: FunctionSidebarTreeNode[],
  name: string,
): FunctionSidebarFunctionNode {
  const node = nodes.find(
    (candidate): candidate is FunctionSidebarFunctionNode =>
      candidate.type === 'function' && candidate.fullName === name,
  );
  if (!node) throw new Error(`missing function ${name}`);
  return node;
}

function nodeLabel(node: FunctionSidebarTreeNode): string {
  return node.type === 'function' ? node.label : node.name;
}
