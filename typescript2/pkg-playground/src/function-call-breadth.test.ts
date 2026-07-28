import { describe, expect, it } from 'vitest';

import {
  functionCallBreadths,
  orderFunctionsByCallBreadth,
  orderFunctionsForExplorer,
} from './function-call-breadth';
import type {
  ControlFlowGraph,
  FunctionInfo,
} from './worker-protocol';

describe('function call breadth', () => {
  it('orders broader transitive callers before their callees', () => {
    const functions = [
      functionInfo('Leaf'),
      functionInfo('Branch'),
      functionInfo('Root'),
      functionInfo('Standalone'),
    ];
    const graphs = new Map<string, ControlFlowGraph>([
      ['Leaf', graph()],
      ['Branch', graph(['Leaf'])],
      ['Root', graph(['Branch'])],
      ['Standalone', graph()],
    ]);

    expect(functionCallBreadths(functions.map((fn) => fn.name), graphs)).toEqual(
      new Map([
        ['Leaf', 0],
        ['Branch', 1],
        ['Root', 2],
        ['Standalone', 0],
      ]),
    );
    expect(
      orderFunctionsByCallBreadth(functions, graphs).map((fn) => fn.name),
    ).toEqual(['Root', 'Branch', 'Leaf', 'Standalone']);
  });

  it('counts shared callees once and breaks equal-breadth ties by name', () => {
    const functions = [
      functionInfo('Zulu'),
      functionInfo('Alpha'),
      functionInfo('Shared'),
      functionInfo('Root'),
    ];
    const graphs = new Map<string, ControlFlowGraph>([
      ['Zulu', graph(['Shared'])],
      ['Alpha', graph(['Shared'])],
      ['Shared', graph()],
      ['Root', graph(['Zulu', 'Alpha', 'Shared'])],
    ]);

    expect(
      orderFunctionsByCallBreadth(functions, graphs).map((fn) => fn.name),
    ).toEqual(['Root', 'Alpha', 'Zulu', 'Shared']);
  });

  it('resolves bare calls in the caller namespace and handles cycles', () => {
    const functions = [
      functionInfo('Helper'),
      functionInfo('other.Helper'),
      functionInfo('demo.Second'),
      functionInfo('demo.Helper'),
      functionInfo('demo.First'),
    ];
    const graphs = new Map<string, ControlFlowGraph>([
      ['other.Helper', graph()],
      ['Helper', graph(['other.Helper'])],
      ['demo.First', graph(['Second'])],
      ['demo.Second', graph(['Helper', 'First'])],
      ['demo.Helper', graph()],
    ]);

    expect(
      functionCallBreadths(
        functions.map((fn) => fn.name),
        graphs,
      ),
    ).toEqual(
      new Map([
        ['Helper', 1],
        ['other.Helper', 0],
        ['demo.Second', 2],
        ['demo.Helper', 0],
        ['demo.First', 2],
      ]),
    );
    expect(
      orderFunctionsByCallBreadth(functions, graphs).map((fn) => fn.name),
    ).toEqual([
      'demo.First',
      'demo.Second',
      'Helper',
      'demo.Helper',
      'other.Helper',
    ]);
  });

  it('keeps source order until every graph response has arrived', () => {
    const functions = [functionInfo('Leaf'), functionInfo('Root')];
    const graphs = new Map<string, ControlFlowGraph>([
      ['Leaf', graph()],
      ['Root', graph(['Leaf'])],
    ]);

    expect(
      orderFunctionsForExplorer(
        functions,
        new Map([['Root', graphs.get('Root')!]]),
        graphs,
      ).map((fn) => fn.name),
    ).toEqual(['Leaf', 'Root']);
    expect(
      orderFunctionsForExplorer(
        functions,
        new Map([
          ['Root', graphs.get('Root')!],
          ['Leaf', graphs.get('Leaf')!],
        ]),
        graphs,
      ).map((fn) => fn.name),
    ).toEqual(['Root', 'Leaf']);
  });
});

function functionInfo(name: string): FunctionInfo {
  return {
    name,
    kind: 'expr',
    origin: 'userDefined',
  };
}

function graph(callees: string[] = []): ControlFlowGraph {
  return {
    nodes: {
      0: {
        id: 0,
        parentNodeId: null,
        logFilterKey: 'root',
        label: 'root',
        sourceExpr: null,
        nodeType: 'functionRoot',
        isContainer: false,
        ...(callees.length > 0 ? { calleeNames: callees } : {}),
      },
    },
    edgesBySrc: {},
  };
}
