import type { NodeProps } from '@xyflow/react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import { GroupNode } from './group-node';

vi.mock('@xyflow/react', () => ({
  Handle: () => null,
  Position: {
    Bottom: 'bottom',
    Top: 'top',
  },
}));

describe('GroupNode', () => {
  it('constrains long labels while preserving the full text for hover', () => {
    const label = 'build_parity_report_with_tests_sdk_envs_inventories';
    const markup = renderToStaticMarkup(
      <GroupNode
        {...({
          data: {
            executionState: 'not-started',
            expanded: true,
            graphNodeType: 'scope',
            iterationCount: 3,
            label,
            logFilterKey: 'group',
            selected: false,
          },
          id: 'group',
        } as unknown as NodeProps)}
      />,
    );

    expect(markup).toContain('class="baml-graph-group-label"');
    expect(markup).toContain('class="baml-graph-group-label__text"');
    expect(markup).toContain(`title="${label} (3 iterations)"`);
    expect(markup).toContain('max-width:calc(100% - 28px)');
    expect(markup).toContain('text-overflow:ellipsis');
    expect(markup).toContain('>3</span>');
  });
});
