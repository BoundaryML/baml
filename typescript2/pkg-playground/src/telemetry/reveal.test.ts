import { describe, expect, it } from 'vitest';

import type { ContextNode } from './evidence';
import { ancestorIdsOf } from './TelemetryView';

function context(id: string, parentId: string | null): ContextNode {
  return {
    awaitMs: 0,
    enters: 1,
    errors: 0,
    fn: id,
    folded: false,
    fqn: `user.${id}`,
    id,
    kind: 'baml',
    parentId,
    selfMs: 0,
    source: null,
    spawn: false,
    subtreeAwaitMs: 0,
    timingComplete: true,
    totalMs: 1,
  };
}

const tree = [
  context('root', null),
  context('mid', 'root'),
  context('leaf', 'mid'),
  context('other', 'root'),
];

describe('ancestorIdsOf', () => {
  it('walks the whole chain up from the selection', () => {
    expect([...ancestorIdsOf(tree, 'leaf')].sort()).toEqual(['mid', 'root']);
  });

  it('excludes the selected row itself', () => {
    expect(ancestorIdsOf(tree, 'leaf').has('leaf')).toBe(false);
  });

  it('returns nothing for a root', () => {
    expect([...ancestorIdsOf(tree, 'root')]).toEqual([]);
  });

  it('returns nothing when there is no selection', () => {
    expect([...ancestorIdsOf(tree, null)]).toEqual([]);
  });

  it('ignores a selection that is not in the tree', () => {
    expect([...ancestorIdsOf(tree, 'missing')]).toEqual([]);
  });

  it('terminates on a cyclic parent chain', () => {
    // A malformed tree must not hang the panel.
    const cyclic = [context('a', 'b'), context('b', 'a')];
    expect([...ancestorIdsOf(cyclic, 'a')].sort()).toEqual(['a', 'b']);
  });
});
