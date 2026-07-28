import { describe, expect, it } from 'vitest';

import {
  buildFunctionSidebarTree,
  buildPreviewTestGroups,
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
});

describe('preview test groups', () => {
  it('keeps each detached legacy test attached to its target function', () => {
    const groups = buildPreviewTestGroups([
      testInfo('HappyPath', 'ClassifySentiment'),
      testInfo('SharedCase', 'ClassifySentiment'),
      testInfo('SharedCase', 'ExtractResume'),
    ]);

    expect(
      groups.map((group) => ({
        functionName: group.functionName,
        tests: group.tests.map((test) => test.name),
      })),
    ).toEqual([
      {
        functionName: 'ClassifySentiment',
        tests: ['HappyPath', 'SharedCase'],
      },
      {
        functionName: 'ExtractResume',
        tests: ['SharedCase'],
      },
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

function testInfo(name: string, functionName: string) {
  return { name, functionName, argsJson: '{}' };
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
