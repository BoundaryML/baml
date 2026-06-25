import { describe, expect, it } from 'vitest';

import type { WorkflowEdge, WorkflowNode } from './types';
import {
  applyLevelOfDetail,
  computeNodeDepths,
  depthScale,
  maxNodeDepth,
  zoomToRevealDepth,
} from './lod';

// Minimal node/edge builders — LOD only reads id/parentId/type/data.
function n(id: string, parentId?: string, type = 'group'): WorkflowNode {
  return {
    id,
    type,
    position: { x: 0, y: 0 },
    data: { label: id } as WorkflowNode['data'],
    ...(parentId ? { parentId } : {}),
  } as WorkflowNode;
}
function e(source: string, target: string): WorkflowEdge {
  return { id: `${source}-${target}`, source, target } as WorkflowEdge;
}

// root → loop → callExpansion → ifGroup → {armA, armB}
//        └ split (leaf)
const NODES: WorkflowNode[] = [
  n('root'),
  n('split', 'root', 'base'),
  n('loop', 'root'),
  n('call', 'loop'),
  n('if', 'call'),
  n('armA', 'if', 'base'),
  n('armB', 'if', 'base'),
];

describe('computeNodeDepths', () => {
  it('counts container ancestors', () => {
    const d = computeNodeDepths(NODES);
    expect(d.get('root')).toBe(0);
    expect(d.get('split')).toBe(1);
    expect(d.get('loop')).toBe(1);
    expect(d.get('call')).toBe(2);
    expect(d.get('if')).toBe(3);
    expect(d.get('armA')).toBe(4);
    expect(maxNodeDepth(NODES)).toBe(4);
  });

  it('does not loop on a cyclic parent chain', () => {
    const cyclic = [n('a', 'b'), n('b', 'a')];
    expect(() => computeNodeDepths(cyclic)).not.toThrow();
  });
});

describe('zoomToRevealDepth', () => {
  it('clamps low zoom to the first level and high zoom to full depth', () => {
    expect(zoomToRevealDepth(0.3, 4)).toBe(1);
    expect(zoomToRevealDepth(2.0, 4)).toBe(4);
  });
  it('scales monotonically in between', () => {
    const mid = zoomToRevealDepth(0.95, 4);
    expect(mid).toBeGreaterThanOrEqual(1);
    expect(mid).toBeLessThanOrEqual(4);
    expect(zoomToRevealDepth(1.2, 4)).toBeGreaterThanOrEqual(mid);
  });
  it('is a no-op for a flat graph', () => {
    expect(zoomToRevealDepth(0.3, 1)).toBe(1);
  });
});

describe('depthScale', () => {
  it('is 1 at the root, shrinks with depth, and clamps', () => {
    expect(depthScale(0)).toBe(1);
    expect(depthScale(1)).toBeLessThan(1);
    expect(depthScale(2)).toBeLessThan(depthScale(1));
    expect(depthScale(100)).toBeGreaterThanOrEqual(0.7);
  });
});

describe('applyLevelOfDetail', () => {
  const EDGES: WorkflowEdge[] = [
    e('split', 'loop'),
    e('armA', 'split'), // crosses out of the collapsed subgraph
    e('armA', 'armB'), // internal to the collapsed subgraph
  ];

  it('reveals only the top level and collapses deeper containers', () => {
    const { nodes } = applyLevelOfDetail(NODES, EDGES, { revealDepth: 1 });
    const ids = nodes.map((x) => x.id).sort();
    expect(ids).toEqual(['loop', 'root', 'split']);
    // Each output node carries its nesting depth for depth-based sizing.
    expect(nodes.find((x) => x.id === 'root')!.data.depth).toBe(0);
    expect(nodes.find((x) => x.id === 'split')!.data.depth).toBe(1);
    // `loop` is a container with hidden children → rendered as a collapsed leaf.
    const loop = nodes.find((x) => x.id === 'loop')!;
    expect(loop.type).toBe('base');
    expect(loop.data.collapsed).toBe(true);
    expect(loop.data.collapsedCount).toBe(4); // call, if, armA, armB
  });

  it('reroutes edges to the nearest visible ancestor and drops internal ones', () => {
    const { edges } = applyLevelOfDetail(NODES, EDGES, { revealDepth: 1 });
    // split→loop stays; armA→split becomes loop→split; armA→armB is internal → dropped.
    const pairs = edges.map((x) => `${x.source}->${x.target}`).sort();
    expect(pairs).toEqual(['loop->split', 'split->loop']);
  });

  it('expands more as revealDepth grows', () => {
    expect(applyLevelOfDetail(NODES, EDGES, { revealDepth: 2 }).nodes.map((x) => x.id)).toContain(
      'call',
    );
    // Reveal-all via a high finite depth (the path "All" mode uses) keeps every
    // node AND still stamps depth — so depth-scaled layout/rendering applies.
    const all = applyLevelOfDetail(NODES, EDGES, { revealDepth: 99 });
    expect(all.nodes).toHaveLength(NODES.length);
    expect(all.nodes.find((x) => x.id === 'armA')!.data.depth).toBe(4);
  });

  it('is a no-op at infinite depth', () => {
    const out = applyLevelOfDetail(NODES, EDGES, { revealDepth: Number.POSITIVE_INFINITY });
    expect(out.nodes).toHaveLength(NODES.length);
    expect(out.edges).toHaveLength(EDGES.length);
  });

  it('reveals a manually-expanded container regardless of depth', () => {
    // Collapsed-by-default (revealDepth 1), but the user clicked `loop` open.
    const { nodes } = applyLevelOfDetail(NODES, EDGES, {
      revealDepth: 1,
      expanded: new Set(['loop']),
    });
    const byId = new Map(nodes.map((x) => [x.id, x]));
    // `loop` now reveals its child `call`...
    expect(byId.has('call')).toBe(true);
    // ...stays a real container, flagged collapsible for the UI...
    expect(byId.get('loop')!.type).toBe('group');
    expect(byId.get('loop')!.data.expanded).toBe(true);
    // ...and `call` is the new collapse boundary (its subtree stays hidden).
    expect(byId.get('call')!.data.collapsed).toBe(true);
    expect(byId.has('if')).toBe(false);
  });
});
