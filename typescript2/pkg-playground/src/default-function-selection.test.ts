import { describe, expect, it } from 'vitest';
import {
  selectDefaultFunctionName,
  selectLongestWorkflowFunctionName,
  selectMainFunctionName,
} from './default-function-selection';
import type { ControlFlowGraph } from './worker-protocol';

function graph(nodeCount: number, callees: string[] = []): ControlFlowGraph {
  const nodes: ControlFlowGraph['nodes'] = {};
  for (let i = 0; i < nodeCount; i += 1) {
    nodes[String(i)] = {
      id: i,
      parentNodeId: null,
      logFilterKey: `node-${i}`,
      label: `node-${i}`,
      sourceExpr: null,
      nodeType: i === 0 ? 'functionRoot' : 'return',
      isContainer: false,
      ...(i === 0 && callees.length > 0 ? { calleeNames: callees } : {}),
    };
  }
  return { nodes, edgesBySrc: {} };
}

describe('default function selection', () => {
  it('prefers an exact main function', () => {
    expect(selectMainFunctionName(['Extract', 'main', 'ns.main'])).toBe('main');
  });

  it('falls back to a namespace-qualified main function', () => {
    expect(selectMainFunctionName(['Extract', 'ns.main'])).toBe('ns.main');
  });

  it('selects the workflow root with the largest graph', () => {
    const graphs = new Map<string, ControlFlowGraph>([
      ['SmallWorkflow', graph(3, ['SharedHelper'])],
      ['LargeWorkflow', graph(7, ['SharedHelper'])],
      ['SharedHelper', graph(20)],
    ]);

    expect(
      selectLongestWorkflowFunctionName(
        ['SmallWorkflow', 'LargeWorkflow', 'SharedHelper'],
        graphs,
      ),
    ).toBe('LargeWorkflow');
  });

  it('uses function order to break longest-workflow ties', () => {
    const graphs = new Map<string, ControlFlowGraph>([
      ['First', graph(4)],
      ['Second', graph(4)],
    ]);

    expect(selectLongestWorkflowFunctionName(['First', 'Second'], graphs)).toBe(
      'First',
    );
  });

  it('uses main before the longest workflow', () => {
    const graphs = new Map<string, ControlFlowGraph>([
      ['main', graph(1)],
      ['LargeWorkflow', graph(10)],
    ]);

    expect(selectDefaultFunctionName(['LargeWorkflow', 'main'], graphs)).toBe(
      'main',
    );
  });
});
