import { describe, expect, it } from 'vitest';

import {
  buildFunctionSidebarTree,
  type FunctionSidebarFolderNode,
  type FunctionSidebarTreeNode,
} from './function-sidebar-tree';
import type { FunctionInfo } from './worker-protocol';

describe('function sidebar tree', () => {
  it('groups functions into nested namespace folders while keeping bare functions at root', () => {
    const tree = buildFunctionSidebarTree([
      functionInfo('Main'),
      functionInfo('demo.Foo'),
      functionInfo('demo.Bar'),
      functionInfo('other.Baz'),
    ]);

    expect(tree.functionCount).toBe(4);
    expect(tree.nodes.map(nodeLabel)).toEqual([
      'function:Main',
      'folder:demo',
      'folder:other',
    ]);

    const demo = folder(tree.nodes, 'demo');
    expect(demo.functionCount).toBe(2);
    expect(demo.children.map(nodeLabel)).toEqual([
      'function:Foo',
      'function:Bar',
    ]);
  });

  it('keeps only matching namespace branches during search', () => {
    const tree = buildFunctionSidebarTree(
      [
        functionInfo('Main'),
        functionInfo('demo.Foo'),
        functionInfo('demo.Bar'),
        functionInfo('other.Baz'),
      ],
      { search: 'bar' },
    );

    expect(tree.functionCount).toBe(1);
    expect(tree.nodes.map(nodeLabel)).toEqual(['folder:demo']);

    const demo = folder(tree.nodes, 'demo');
    expect(demo.functionCount).toBe(1);
    expect(demo.children.map(nodeLabel)).toEqual(['function:Bar']);
    expect([...tree.forcedOpenFolderKeys]).toEqual(['demo']);
  });

  it('forces the selected function namespace path open', () => {
    const tree = buildFunctionSidebarTree(
      [
        functionInfo('Main'),
        functionInfo('demo.Foo'),
        functionInfo('demo.Bar'),
        functionInfo('other.Baz'),
      ],
      { selectedFunctionName: 'demo.Foo' },
    );

    expect([...tree.forcedOpenFolderKeys]).toEqual(['demo']);
  });

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
      'function:LargeFirst',
      'function:LargeSecond',
      'function:Medium',
      'function:Small',
    ]);
  });

  it('orders namespace folders and their children by their largest workflows', () => {
    const tree = buildFunctionSidebarTree(
      [
        functionInfo('small.One'),
        functionInfo('large.Helper'),
        functionInfo('small.Two'),
        functionInfo('large.Entry'),
      ],
      {
        workflowNodeCounts: new Map([
          ['small.One', 2],
          ['large.Helper', 7],
          ['small.Two', 4],
          ['large.Entry', 12],
        ]),
      },
    );

    expect(tree.nodes.map(nodeLabel)).toEqual(['folder:large', 'folder:small']);
    expect(folder(tree.nodes, 'large').children.map(nodeLabel)).toEqual([
      'function:Entry',
      'function:Helper',
    ]);
    expect(folder(tree.nodes, 'small').children.map(nodeLabel)).toEqual([
      'function:Two',
      'function:One',
    ]);
  });
});

function functionInfo(name: string): FunctionInfo {
  return {
    name,
    kind: 'expr',
    origin: 'userDefined',
  };
}

function folder(
  nodes: FunctionSidebarTreeNode[],
  name: string,
): FunctionSidebarFolderNode {
  const node = nodes.find(
    (candidate): candidate is FunctionSidebarFolderNode =>
      candidate.type === 'folder' && candidate.name === name,
  );
  if (!node) throw new Error(`missing folder ${name}`);
  return node;
}

function nodeLabel(node: FunctionSidebarTreeNode): string {
  return node.type === 'folder'
    ? `folder:${node.name}`
    : `function:${node.label}`;
}
