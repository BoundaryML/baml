import { describe, expect, it } from 'vitest';

import type { ContextNode } from './evidence';
import {
  type ContextTreeNode,
  collapseToUserFrames,
  layoutChildren,
} from './TelemetryView';

function context(
  overrides: Partial<ContextNode> & { id: string },
): ContextNode {
  return {
    awaitMs: 0,
    enters: 1,
    errors: 0,
    fn: overrides.id,
    folded: false,
    fqn: `user.${overrides.id}`,
    kind: 'baml',
    parentId: null,
    selfMs: 0,
    source: null,
    spawn: false,
    subtreeAwaitMs: 0,
    timingComplete: true,
    totalMs: 100,
    ...overrides,
  };
}

function node(
  id: string,
  totalMs: number,
  children: ContextTreeNode[] = [],
  overrides: Partial<ContextNode> = {},
): ContextTreeNode {
  return { children, context: context({ id, totalMs, ...overrides }) };
}

const byTotal = (c: ContextNode) => c.totalMs;

describe('layoutChildren', () => {
  it('never lets children sum past the parent', () => {
    // The bug this replaces gave every child a minimum width, so enough
    // small siblings overflowed the row and squashed into each other.
    const parent = node(
      'p',
      100,
      Array.from({ length: 40 }, (_, i) => node(`c${i}`, 2.5)),
    );
    const { slots, foldedFraction } = layoutChildren(parent, 800, byTotal);
    const total =
      slots.reduce((sum, slot) => sum + slot.fraction, 0) + foldedFraction;
    expect(total).toBeLessThanOrEqual(1.0000001);
  });

  it('folds frames too narrow to draw into one sliver', () => {
    const parent = node('p', 100, [
      node('big', 96),
      node('tiny1', 2),
      node('tiny2', 2),
    ]);
    // At 100px wide, a 2% child is 2px: below the visible threshold.
    const { slots, foldedCount, foldedFraction } = layoutChildren(
      parent,
      100,
      byTotal,
    );
    expect(slots.map((slot) => slot.node.context.id)).toEqual(['big']);
    expect(foldedCount).toBe(2);
    expect(foldedFraction).toBeCloseTo(0.04, 5);
  });

  it('keeps the same frames when there is room to draw them', () => {
    const parent = node('p', 100, [
      node('big', 96),
      node('tiny1', 2),
      node('tiny2', 2),
    ]);
    // The same tree at 4000px: 2% is 80px, comfortably drawable.
    const { slots, foldedCount } = layoutChildren(parent, 4000, byTotal);
    expect(slots).toHaveLength(3);
    expect(foldedCount).toBe(0);
  });

  it('normalises spawned children that outlast their parent', () => {
    // Spawned subtrees overlap the parent in wall time, so their sum can
    // exceed it. Widths must still fit inside the parent's box.
    const parent = node('p', 100, [
      node('a', 90, [], { spawn: true }),
      node('b', 90, [], { spawn: true }),
    ]);
    const { slots } = layoutChildren(parent, 1000, byTotal);
    const total = slots.reduce((sum, slot) => sum + slot.fraction, 0);
    expect(total).toBeCloseTo(1, 5);
  });

  it('handles a parent with no measured time', () => {
    const parent = node('p', 0, [node('a', 0)]);
    const { slots, foldedCount } = layoutChildren(parent, 500, byTotal);
    expect(slots).toHaveLength(0);
    expect(foldedCount).toBe(1);
  });
});

describe('collapseToUserFrames', () => {
  it('lifts user code out from under runtime frames', () => {
    // One model call sits on several frames of client plumbing.
    const tree = [
      node('main', 100, [
        node('run', 100, [node('invoke', 100, [node('Describe', 90)])], {
          fqn: 'ai.Agent.Runner.run',
        }),
      ]),
    ];
    (tree[0].children[0].children[0].context as ContextNode).fqn =
      'openai.Client.invoke';
    const collapsed = collapseToUserFrames(tree);
    expect(collapsed).toHaveLength(1);
    expect(collapsed[0].context.id).toBe('main');
    expect(collapsed[0].children.map((c) => c.context.id)).toEqual([
      'Describe',
    ]);
  });

  it('drops a subtree that is entirely runtime', () => {
    const tree = [
      node('main', 100, [
        node('plumbing', 40, [], { fqn: 'ai.internal.send' }),
      ]),
    ];
    const collapsed = collapseToUserFrames(tree);
    // The time is not lost: it stays part of main's width, just unfilled.
    expect(collapsed[0].children).toHaveLength(0);
  });

  it('keeps folded overflow rows, which stand for real calls', () => {
    const tree = [
      node('main', 100, [
        node('overflow', 40, [], { folded: true, fqn: null }),
      ]),
    ];
    const collapsed = collapseToUserFrames(tree);
    expect(collapsed[0].children).toHaveLength(1);
  });
});
