'use client';

import '@xyflow/react/dist/style.css';

import {
  Background,
  BackgroundVariant,
  Controls,
  ReactFlow,
  SelectionMode,
  useEdgesState,
  useNodesState,
  useReactFlow,
  useViewport,
} from '@xyflow/react';
import { useEffect, useId, useLayoutEffect, useRef, useState } from 'react';
import type { Node } from '@xyflow/react';

// Import graph primitives and components from WorkflowApp
import { kEdgeTypes, ColorfulMarkerDefinitions, kNodeTypes } from '../../../../graph-primitives';
import { ReactflowInstance } from '../../../../features/graph/components';
import { useActiveWorkflow, useLayoutDirection } from '../../../../sdk/hooks';
import { flowStore } from '../../../../states/reactflow';
import { Loader as Spinner } from '@baml/ui/custom/loader';
import { useGraphSync } from '../../../../features/graph/hooks';
import { useAtom, useAtomValue, useSetAtom } from 'jotai';
import { graphControlsTipDismissedAtom, unifiedSelectionAtom } from '../atoms';
import { MousePointer2, ZoomIn, X, ChevronLeft } from 'lucide-react';
import { navigationDispatcherAtom } from '../../../../sdk/navigation/dispatcher';
import type { NavigationIntent } from '../../../../sdk/types';

type FunctionIntent = Extract<NavigationIntent, { type: 'function' }>;

const mapNodeTypeToFunctionType = (node: Node): FunctionIntent['functionType'] => {
  switch (node.type) {
    case 'llm':
      return 'llm_function';
    case 'diamond':
      return 'conditional';
    case 'hexagon':
      return 'loop';
    default:
      return 'function';
  }
};

/**
 * GraphView - ReactFlow graph component for the Graph tab
 *
 * This component renders the workflow graph and handles:
 * - Auto-layout
 * - Node selection
 * - Detail panel integration
 */
export const GraphView = () => {
  const [nodes, _setNodes, onNodesChange] = useNodesState([]);
  const [edges, _setEdges, onEdgesChange] = useEdgesState([]);

  // Feature hooks
  const { convertedGraph, isLayoutLoading } = useGraphSync();

  // SDK hooks
  const { activeWorkflowId } = useActiveWorkflow();
  const [direction] = useLayoutDirection();
  const dispatchNavigation = useSetAtom(navigationDispatcherAtom);
  const [graphTipDismissed, setGraphTipDismissed] = useAtom(
    graphControlsTipDismissedAtom
  );
  const selection = useAtomValue(unifiedSelectionAtom);
  const selectedNodeId = selection.mode === 'workflow' ? selection.selectedNodeId : null;

  useEffect(() => {
    const nodes = flowStore.value.getNodes?.();
    if (!nodes || !nodes.length) return;
    const updated = nodes.map((node) =>
      node.selected === (node.id === selectedNodeId)
        ? node
        : { ...node, selected: node.id === selectedNodeId }
    );
    flowStore.value.setNodes?.(updated);
  }, [selectedNodeId]);

  const { getEdges, setNodes } = useReactFlow();
  const viewport = useViewport();
  const containerRef = useRef<HTMLDivElement>(null);
  const [indicatorPosition, setIndicatorPosition] = useState<{ x: number; y: number } | null>(null);

  // UI state
  const backgroundId = useId();

  // Clear node states when workflow changes
  useEffect(() => {
    // Clear all node states AND outputs in UI when switching workflows
    setNodes((currentNodes) =>
      currentNodes.map((node) => ({
        ...node,
        data: {
          ...node.data,
          executionState: 'not-started',
          isExecutionActive: false,
          outputs: undefined,
          error: undefined,
        },
      }))
    );
  }, [activeWorkflowId, setNodes]);

  // Recalculate edge routing when a node is being dragged or moved
  const handleNodeDrag = () => {
    const currentEdges = getEdges();

    // Clear ELK routing data so edges use dynamic routing with new node positions
    const edgesWithoutElkRouting = currentEdges.map((edge) => ({
      ...edge,
      data: {
        ...edge.data,
        layout: edge.data?.layout ? {
          ...edge.data.layout,
          inputPoints: undefined, // Clear ELK routing
        } : undefined,
      },
    }));

    // Update edges to trigger re-render with new positions
    flowStore.value.setEdges(edgesWithoutElkRouting);
  };

  // Handle node click - select the node and open detail panel
  const handleNodeClick = (_event: React.MouseEvent, node: Node) => {
    console.log('Node clicked:', node.id);
    dispatchNavigation({
      type: 'function',
      functionName: node.id,
      functionType: mapNodeTypeToFunctionType(node),
      filePath: 'unknown',
      source: 'graph',
    });
  };

  useLayoutEffect(() => {
    if (!selectedNodeId) {
      setIndicatorPosition(null);
      return;
    }

    const container = containerRef.current;
    if (!container) return;

    const nodeElement = container.querySelector<HTMLElement>(`[data-id="${selectedNodeId}"]`);
    if (!nodeElement) {
      setIndicatorPosition(null);
      return;
    }

    const nodeRect = nodeElement.getBoundingClientRect();
    const containerRect = container.getBoundingClientRect();

    setIndicatorPosition({
      x: nodeRect.right - containerRect.left + 8,
      y: nodeRect.top - containerRect.top + nodeRect.height / 2,
    });
  }, [selectedNodeId, viewport.x, viewport.y, viewport.zoom, nodes]);

  return (
    <div ref={containerRef} className="relative w-full h-full">
      <ColorfulMarkerDefinitions />

      {/* Loading overlay */}
      {isLayoutLoading && (
        <div className="absolute inset-0 z-50 flex items-center justify-center bg-background">
          <div className="flex flex-col items-center gap-3">
            <Spinner className="size-8" />
            <p className="text-sm text-muted-foreground">Calculating layout...</p>
          </div>
        </div>
      )}

      {/* ReactFlow Graph */}
      <ReactFlow
        edges={edges}
        edgeTypes={kEdgeTypes}
        nodes={nodes}
        nodeTypes={kNodeTypes}
        onEdgesChange={onEdgesChange}
        onNodesChange={onNodesChange}
        onNodeDrag={handleNodeDrag}
        onNodeClick={handleNodeClick}
        panOnScroll
        // by making this true note sometimes clicks wont register since it will think you are dragging.
        nodesDraggable={false}
        selectionOnDrag
        panOnDrag={[1, 2]}
        // autoPanOnNodeFocus={true}
        selectionMode={SelectionMode.Partial}
        colorMode="light"
      >
        <Background
          className="bg-background"
          color="hsl(var(--muted))"
          id={backgroundId}
          variant={BackgroundVariant.Dots}
        />
        <ReactflowInstance />
        <Controls showInteractive={false} />
      </ReactFlow>

      {indicatorPosition && (
        <div
          className="pointer-events-none absolute z-50"
          style={{
            left: 0,
            top: 0,
            transform: `translate(${indicatorPosition.x}px, ${indicatorPosition.y}px)`,
          }}
        >
          <div className="-translate-y-1/2 flex items-center justify-center rounded-md bg-primary px-1.5 py-0.5 shadow-lg shadow-primary/40">
            <ChevronLeft className="h-3.5 w-3.5 text-background" strokeWidth={3} />
          </div>
        </div>
      )}

      {!graphTipDismissed && (
        <div className="absolute top-4 left-4 z-20 max-w-xs rounded-md border border-border bg-background/95 shadow-lg p-3 text-xs text-muted-foreground">
          <div className="flex items-center justify-between gap-2 mb-2 text-[11px] font-semibold text-foreground">
            <span>Navigate like Figma</span>
            <button
              type="button"
              className="text-muted-foreground hover:text-foreground"
              onClick={() => setGraphTipDismissed(true)}
            >
              <X className="w-3 h-3" />
            </button>
          </div>
          <div className="flex items-center gap-2 mb-1">
            <MousePointer2 className="w-3.5 h-3.5 text-primary" />
            <span>Right-click + drag to pan</span>
          </div>
          <div className="flex items-center gap-2">
            <ZoomIn className="w-3.5 h-3.5 text-primary" />
            <span>⌘ + scroll to zoom</span>
          </div>
        </div>
      )}
    </div>
  );
};
