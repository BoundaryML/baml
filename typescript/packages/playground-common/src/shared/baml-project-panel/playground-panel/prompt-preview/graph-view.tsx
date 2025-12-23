'use client';

import '@xyflow/react/dist/style.css';

import {
  Background,
  BackgroundVariant,
  ControlButton,
  Controls,
  ReactFlow,
  SelectionMode,
  useEdgesState,
  useNodesInitialized,
  useNodesState,
  useReactFlow,
  useViewport,
} from '@xyflow/react';
import { useEffect, useId, useLayoutEffect, useRef, useState } from 'react';
import type { Node } from '@xyflow/react';

// Import graph primitives and components from WorkflowApp
import { kEdgeTypes, ColorfulMarkerDefinitions, kNodeTypes } from '../../../../graph-primitives';
import { ReactflowInstance } from '../../../../features/graph/components';
import { useActiveWorkflow, useLayoutDirection, useNavigation } from '../../../../sdk/hooks';
import { flowStore } from '../../../../states/reactflow';
import { Loader as Spinner } from '@baml/ui/custom/loader';
import { useGraphSync } from '../../../../features/graph/hooks';
import { useAtom, useAtomValue, useSetAtom } from 'jotai';
import { graphControlsTipDismissedAtom, unifiedSelectionAtom } from '../atoms';
import { MousePointer2, ZoomIn, X, ChevronLeft, FlipHorizontal, FlipVertical } from 'lucide-react';
import { Tooltip, TooltipContent, TooltipTrigger, TooltipProvider } from '@baml/ui/tooltip';
import type { NavigationInput } from '../../../../sdk/navigation';
import { panToNodeIfNeeded } from '../../../../utils/cameraPan';
import { allNodeStatesAtom, allNodeIterationsAtom, currentGraphAtom, scrollToNodeIdAtom } from '../../../../sdk/atoms/core.atoms';

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
  const nodesInitialized = useNodesInitialized({ includeHiddenNodes: false });

  // Feature hooks
  const { convertedGraph, isLayoutLoading } = useGraphSync();

  // SDK hooks
  const { activeWorkflowId } = useActiveWorkflow();
  const [direction, setDirection] = useLayoutDirection();
  const navigate = useNavigation();
  const [graphTipDismissed, setGraphTipDismissed] = useAtom(
    graphControlsTipDismissedAtom
  );
  const selection = useAtomValue(unifiedSelectionAtom);
  const selectedNodeId = selection.mode === 'workflow' ? selection.selectedNodeId : null;
  const nodeStates = useAtomValue(allNodeStatesAtom);
  const nodeIterations = useAtomValue(allNodeIterationsAtom);
  const currentGraph = useAtomValue(currentGraphAtom);
  const setScrollToNodeId = useSetAtom(scrollToNodeIdAtom);

  // Get workflow ID from the displayed graph (more reliable than selection state)
  const displayedWorkflowId = currentGraph.workflow?.id ?? activeWorkflowId;

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
      [...currentNodes].map((node) => ({
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
  }, [activeWorkflowId]);

  // Update ReactFlow nodes when SDK node states change
  useEffect(() => {
    if (!nodesInitialized) return;
    if (nodeStates.size === 0) return;

    setNodes((currentNodes) =>
      currentNodes.map((node) => {
        const state = nodeStates.get(node.id);
        if (state && state !== node.data.executionState) {
          return {
            ...node,
            data: {
              ...node.data,
              executionState: state,
              isExecutionActive: state === 'running',
            },
          };
        }
        return node;
      })
    );
  }, [nodeStates, setNodes, nodesInitialized]);

  // Update ReactFlow nodes when SDK node iterations change (for loops)
  useEffect(() => {
    if (!nodesInitialized) return;
    if (nodeIterations.size === 0) return;

    setNodes((currentNodes) =>
      currentNodes.map((node) => {
        const iteration = nodeIterations.get(node.id) ?? 0;
        if (iteration !== (node.data.iteration ?? 0)) {
          return {
            ...node,
            data: {
              ...node.data,
              iteration,
            },
          };
        }
        return node;
      })
    );
  }, [nodeIterations, setNodes, nodesInitialized]);

  // set selected node id if it changes
  useEffect(() => {
    if (selectedNodeId && nodesInitialized) {

      setNodes((currentNodes) => {
        return currentNodes.map((node) => ({ ...node, selected: node.id === selectedNodeId }));
      });


      // pan if selected node id changes
      setTimeout(() => {
        // Get the node INSIDE the timeout to ensure we have the latest position
        // after React Flow has finished any layout updates
        const node = flowStore.value.getNode(selectedNodeId ?? '');
        if (!node) {
          console.error("Node not found. Can't pan to it:", selectedNodeId);
          return;
        }
        console.log('Panning to node:', node.id);
        panToNodeIfNeeded(node, flowStore.value);
      }, 10);



    }
  }, [selectedNodeId, setNodes, isLayoutLoading, nodesInitialized]);



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

    // Use displayedWorkflowId from the current graph (more reliable than selection state)
    if (!displayedWorkflowId) {
      console.error('No workflow ID available for node click. Selection state may be out of sync.');
      return;
    }

    console.log('node clicked', node);

    const input: NavigationInput = {
      kind: 'node',
      source: 'graph',
      timestamp: Date.now(),
      workflowId: displayedWorkflowId,
      nodeId: node.id,
    };
    navigate(input);

    // Also trigger scroll-to in execution log panel
    setScrollToNodeId(node.id);
  };

  useLayoutEffect(() => {
    if (!selectedNodeId || !nodesInitialized || isLayoutLoading) {
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

      {/* Loading spinner - top right */}
      {isLayoutLoading && (
        <div className="absolute top-4 right-4 z-50">
          <Spinner className="size-6" />
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
        zoomOnDoubleClick={false}
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
        <Controls showInteractive={false}>
          <TooltipProvider>
            <Tooltip delayDuration={100}>
              <TooltipTrigger asChild>
                <ControlButton
                  onClick={() => setDirection(direction === 'vertical' ? 'horizontal' : 'vertical')}
                >
                  {direction === 'vertical' ? <FlipHorizontal className="w-4 h-4" /> : <FlipVertical className="w-4 h-4" />}
                </ControlButton>
              </TooltipTrigger>
              <TooltipContent>
                <p>Switch to {direction === 'vertical' ? 'horizontal' : 'vertical'} layout</p>
              </TooltipContent>
            </Tooltip>
          </TooltipProvider>
        </Controls>
      </ReactFlow>

      {indicatorPosition && nodesInitialized && (
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
