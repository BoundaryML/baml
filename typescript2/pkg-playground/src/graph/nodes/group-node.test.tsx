// @vitest-environment jsdom

import type { NodeProps } from '@xyflow/react';
import { act } from 'react';
import { createRoot } from 'react-dom/client';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterAll, afterEach, describe, expect, it, vi } from 'vitest';
import { GroupNode } from './group-node';

vi.mock('@xyflow/react', () => ({
  Handle: () => null,
  Position: {
    Bottom: 'bottom',
    Top: 'top',
  },
}));

const label = 'build_parity_report_with_tests_sdk_envs_inventories';
const reactGlobal = globalThis as typeof globalThis & {
  IS_REACT_ACT_ENVIRONMENT?: boolean;
};
const previousActEnvironment = reactGlobal.IS_REACT_ACT_ENVIRONMENT;

reactGlobal.IS_REACT_ACT_ENVIRONMENT = true;

const groupNodeProps = (): NodeProps =>
  ({
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
  }) as unknown as NodeProps;

afterEach(() => {
  document.body.replaceChildren();
});

afterAll(() => {
  if (previousActEnvironment === undefined) {
    delete reactGlobal.IS_REACT_ACT_ENVIRONMENT;
  } else {
    reactGlobal.IS_REACT_ACT_ENVIRONMENT = previousActEnvironment;
  }
});

describe('GroupNode', () => {
  it('constrains long labels while preserving the full text for hover', () => {
    const markup = renderToStaticMarkup(<GroupNode {...groupNodeProps()} />);

    expect(markup).toContain('class="baml-graph-group-label"');
    expect(markup).toContain('class="baml-graph-group-label__text"');
    expect(markup).toContain(`title="${label} (3 iterations)"`);
    expect(markup).toContain(`>${label}</span>`);
    expect(markup).toContain('max-width:calc(100% - 28px)');
    expect(markup).toContain('text-overflow:ellipsis');
    expect(markup).toContain('title="Click to collapse"');
    expect(markup).toContain('>−</span>');
    expect(markup).toContain('>3</span>');
  });

  it('expands the full label on hover and restores the ambient clamp', () => {
    const container = document.createElement('div');
    document.body.append(container);
    const root = createRoot(container);

    act(() => root.render(<GroupNode {...groupNodeProps()} />));

    const chip = container.querySelector<HTMLElement>(
      '.baml-graph-group-label',
    );
    const text = container.querySelector<HTMLElement>(
      '.baml-graph-group-label__text',
    );
    const collapseBadge = container.querySelector<HTMLElement>(
      '[title="Click to collapse"]',
    );

    expect(chip).not.toBeNull();
    expect(text).not.toBeNull();
    expect(text?.textContent).toBe(label);
    expect(collapseBadge?.textContent).toBe('−');
    expect(chip?.style.maxWidth).toBe('calc(100% - 28px)');
    expect(text?.style.textOverflow).toBe('ellipsis');

    act(() => {
      chip?.dispatchEvent(new MouseEvent('mouseover', { bubbles: true }));
    });

    expect(chip?.style.maxWidth).toBe('none');
    expect(chip?.style.overflow).toBe('visible');
    expect(text?.style.overflow).toBe('visible');
    expect(text?.style.textOverflow).toBe('clip');
    expect(text?.textContent).toBe(label);
    expect(collapseBadge?.textContent).toBe('−');

    act(() => {
      chip?.dispatchEvent(new MouseEvent('mouseout', { bubbles: true }));
    });

    expect(chip?.style.maxWidth).toBe('calc(100% - 28px)');
    expect(chip?.style.overflow).toBe('hidden');
    expect(text?.style.overflow).toBe('hidden');
    expect(text?.style.textOverflow).toBe('ellipsis');
    expect(text?.textContent).toBe(label);

    act(() => root.unmount());
  });
});
