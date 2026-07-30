import { describe, expect, it } from 'vitest';

import { topologicallySortFunctions } from './function-topological-sort';
import type {
  CfgNode,
  ControlFlowGraph,
  FunctionInfo,
} from './worker-protocol';

describe('topologicallySortFunctions', () => {
  it('orders callers before the functions they call', () => {
    const functions = infos(['Leaf', 'Workflow', 'Transform']);
    const graphs = new Map([
      ['Workflow', graphCalling('Transform')],
      ['Transform', graphCalling('Leaf')],
      ['Leaf', graphCalling()],
    ]);

    expect(names(topologicallySortFunctions(functions, graphs))).toEqual([
      'Workflow',
      'Transform',
      'Leaf',
    ]);
  });

  it('resolves qualified function names from bare callees', () => {
    const functions = infos(['demo.Child', 'demo.Parent']);
    const graphs = new Map([
      ['demo.Parent', graphCalling('Child')],
      ['demo.Child', graphCalling()],
    ]);

    expect(names(topologicallySortFunctions(functions, graphs))).toEqual([
      'demo.Parent',
      'demo.Child',
    ]);
  });

  it('keeps recursive components stable and orders their dependencies after them', () => {
    const functions = infos(['Leaf', 'CycleB', 'Caller', 'CycleA']);
    const graphs = new Map([
      ['Caller', graphCalling('CycleA')],
      ['CycleA', graphCalling('CycleB')],
      ['CycleB', graphCalling('CycleA', 'Leaf')],
      ['Leaf', graphCalling()],
    ]);

    expect(names(topologicallySortFunctions(functions, graphs))).toEqual([
      'Caller',
      'CycleB',
      'CycleA',
      'Leaf',
    ]);
  });

  it('preserves source order when there are no call relationships', () => {
    const functions = infos(['Second', 'First', 'Third']);
    const graphs = new Map(
      functions.map((fn) => [fn.name, graphCalling()] as const),
    );

    expect(names(topologicallySortFunctions(functions, graphs))).toEqual([
      'Second',
      'First',
      'Third',
    ]);
  });
});

function infos(functionNames: string[]): FunctionInfo[] {
  return functionNames.map((name) => ({
    kind: 'expr',
    name,
    origin: 'userDefined',
  }));
}

function names(functions: FunctionInfo[]): string[] {
  return functions.map((fn) => fn.name);
}

function graphCalling(...callees: string[]): ControlFlowGraph {
  const node: CfgNode = {
    calleeNames: callees,
    id: 0,
    isContainer: false,
    label: 'body',
    logFilterKey: 'node-0',
    nodeType: 'otherScope',
    parentNodeId: null,
    sourceExpr: 0,
  };
  return {
    edgesBySrc: {},
    nodes: { 0: node },
  };
}
