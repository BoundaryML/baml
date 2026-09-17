// biome-ignore-all lint/style/useFilenamingConvention: Preserve the existing interaction test naming convention.

import { fireEvent, render, screen } from '@testing-library/react';
import type { ReactNode } from 'react';
import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest';

import { GraphView } from '../../pkg-playground/src/graph/GraphView';
import type { ControlFlowGraph } from '../../pkg-playground/src/worker-protocol';

vi.mock('@xyflow/react', async () => {
  const React = await import('react');
  const store = {
    height: 600,
    transform: [0, 0, 1] as [number, number, number],
    width: 800,
  };

  return {
    Background: () => null,
    BackgroundVariant: { Dots: 'dots' },
    Controls: () => null,
    ReactFlow: ({ children }: { children?: ReactNode }) => (
      <div>{children}</div>
    ),
    ReactFlowProvider: ({ children }: { children?: ReactNode }) => (
      <>{children}</>
    ),
    useEdgesState: (initial: unknown[]) => {
      const [edges, setEdges] = React.useState(initial);
      return [edges, setEdges, vi.fn()];
    },
    useNodesState: (initial: unknown[]) => {
      const [nodes, setNodes] = React.useState(initial);
      return [nodes, setNodes, vi.fn()];
    },
    useReactFlow: () => ({
      fitView: vi.fn(),
      getNode: vi.fn(),
      getViewport: () => ({ x: 0, y: 0, zoom: 1 }),
      setCenter: vi.fn(),
    }),
    useStore: (selector: (state: typeof store) => unknown) => selector(store),
  };
});

vi.mock('../../pkg-playground/src/graph/layout', () => ({
  layoutGraph: () => new Promise(() => {}),
}));

const emptyGraph: ControlFlowGraph = {
  edgesBySrc: {},
  nodes: {},
};

beforeAll(() => {
  vi.stubGlobal(
    'ResizeObserver',
    class ResizeObserver {
      observe() {}
      unobserve() {}
      disconnect() {}
    },
  );
});

afterAll(() => {
  vi.unstubAllGlobals();
});

describe('GraphView expand mode', () => {
  it('keeps the selected mode when navigating between functions', () => {
    const { rerender } = render(
      <GraphView
        functionName="First"
        graph={emptyGraph}
        onNodeClick={vi.fn()}
        selectedNodeId={null}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'All' }));
    expect(screen.getByRole('button', { name: 'All' })).toHaveAttribute(
      'aria-pressed',
      'true',
    );

    rerender(
      <GraphView
        functionName="Second"
        graph={emptyGraph}
        onNodeClick={vi.fn()}
        selectedNodeId={null}
      />,
    );

    expect(screen.getByRole('button', { name: 'All' })).toHaveAttribute(
      'aria-pressed',
      'true',
    );
  });
});
