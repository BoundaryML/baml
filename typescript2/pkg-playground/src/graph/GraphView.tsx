import { useCallback, useEffect, useMemo, useRef, useState, type FC } from 'react';
import {
  ReactFlow,
  ReactFlowProvider,
  useNodesState,
  useEdgesState,
  useReactFlow,
  useStore,
  Controls,
  Background,
  BackgroundVariant,
  type NodeMouseHandler,
} from '@xyflow/react';
import '@xyflow/react/dist/style.css';

import type { ControlFlowGraph, DeserializedRuntimeEvent, RunEntry } from '../worker-protocol';
import type { ResultRendererProps } from '../result-renderers';
import { cfgToGraphNodes, graphToReactflow } from './convert';
import { collectGraphNodeRuntime } from './runtime-output';
import { layoutGraph } from './layout';
import { kNodeTypes } from './nodes';
import { kEdgeTypes, ColorfulMarkerDefinitions } from './edges';
import type { WorkflowNode, WorkflowEdge } from './types';

interface GraphViewProps {
  graph: ControlFlowGraph;
  runtimeEvents?: DeserializedRuntimeEvent[];
  runStatus?: RunEntry['status'];
  runError?: string | null;
  runFunctionName?: string | null;
  customRenderers?: Record<string, FC<ResultRendererProps>>;
  selectedNodeId: number | null;
  onNodeClick: (nodeId: number) => void;
}

const EMPTY_RUNTIME_EVENTS: DeserializedRuntimeEvent[] = [];

function GraphViewInner({
  graph,
  runtimeEvents = EMPTY_RUNTIME_EVENTS,
  runStatus,
  runError,
  runFunctionName,
  customRenderers,
  selectedNodeId,
  onNodeClick,
}: GraphViewProps) {
  const [nodes, setNodes, onNodesChange] = useNodesState<WorkflowNode>([]);
  const [edges, setEdges, onEdgesChange] = useEdgesState<WorkflowEdge>([]);
  const [direction, setDirection] = useState<'horizontal' | 'vertical'>('horizontal');
  const selectedNodeIdRef = useRef(selectedNodeId);

  useEffect(() => {
    selectedNodeIdRef.current = selectedNodeId;
  }, [selectedNodeId]);

  const graphModel = useMemo(() => {
    const { nodes: graphNodes, edges: graphEdges } = cfgToGraphNodes(graph);
    const { nodes: rfNodes, edges: rfEdges } = graphToReactflow(graphNodes, graphEdges);
    return { graphNodes, rfNodes, rfEdges };
  }, [graph]);

  const runtimeInputsRef = useRef({
    runtimeEvents,
    runStatus,
    runError,
    runFunctionName,
    customRenderers,
  });
  runtimeInputsRef.current = {
    runtimeEvents,
    runStatus,
    runError,
    runFunctionName,
    customRenderers,
  };

  const graphNodesRef = useRef(graphModel.graphNodes);
  graphNodesRef.current = graphModel.graphNodes;

  const decorateNodesWithRuntime = useCallback((baseNodes: WorkflowNode[]): WorkflowNode[] => {
    const {
      runtimeEvents: latestRuntimeEvents,
      runStatus: latestRunStatus,
      runError: latestRunError,
      runFunctionName: latestRunFunctionName,
      customRenderers: latestCustomRenderers,
    } = runtimeInputsRef.current;
    const runtimeByNode = collectGraphNodeRuntime(graphNodesRef.current, latestRuntimeEvents, {
      status: latestRunStatus,
      error: latestRunError,
      functionName: latestRunFunctionName,
    });
    const selectedId = selectedNodeIdRef.current == null
      ? null
      : String(selectedNodeIdRef.current);

    return baseNodes.map((node) => {
      const runtime = runtimeByNode.get(node.id);
      if (!runtime) {
        return {
          ...node,
          data: {
            ...node.data,
            result: undefined,
            hasResult: undefined,
            imageOutputs: [],
            executionState: 'not-started' as const,
            errorMessage: undefined,
            customRenderers: latestCustomRenderers,
            selected: node.id === selectedId,
          },
        };
      }

      return {
        ...node,
        data: {
          ...node.data,
          result: runtime.result,
          hasResult: runtime.hasResult,
          imageOutputs: runtime.imageOutputs,
          executionState: runtime.executionState,
          errorMessage: runtime.errorMessage,
          customRenderers: latestCustomRenderers,
          selected: node.id === selectedId,
        },
      };
    });
  }, []);

  const layoutRunIdRef = useRef(0);

  // Convert CFG -> ReactFlow and run layout when graph geometry changes.
  // Runtime output affects geometry because result previews can make nodes larger.
  useEffect(() => {
    const layoutRunId = ++layoutRunIdRef.current;
    const nodesWithRuntime = decorateNodesWithRuntime(graphModel.rfNodes);

    layoutGraph(nodesWithRuntime, graphModel.rfEdges, direction)
      .then(({ nodes: laid, edges: laidEdges }) => {
        if (layoutRunId !== layoutRunIdRef.current) return;
        setNodes(decorateNodesWithRuntime(laid));
        setEdges(laidEdges);
      })
      .catch((err) => {
        console.error('[GraphView] Layout failed:', err);
      });
  }, [
    graphModel,
    runtimeEvents,
    runStatus,
    runError,
    runFunctionName,
    direction,
    setNodes,
    setEdges,
    decorateNodesWithRuntime,
  ]);

  // Keep runtime node data fresh while a new ELK layout is pending.
  useEffect(() => {
    setNodes((nds) => decorateNodesWithRuntime(nds));
  }, [
    runtimeEvents,
    runStatus,
    runError,
    runFunctionName,
    customRenderers,
    selectedNodeId,
    setNodes,
    decorateNodesWithRuntime,
  ]);

  // Update selected state on nodes
  useEffect(() => {
    setNodes((nds) =>
      nds.map((n) => ({
        ...n,
        data: { ...n.data, selected: n.id === String(selectedNodeId) },
      })),
    );
  }, [selectedNodeId, setNodes]);

  // Auto-pan viewport to center the selected node — only when it's off-screen
  const { setCenter, getNode, getViewport } = useReactFlow();
  const containerWidth = useStore((s) => s.width);
  const containerHeight = useStore((s) => s.height);
  const prevGraphRef = useRef(graph);
  useEffect(() => {
    if (selectedNodeId == null) return;
    // Skip auto-pan when the graph itself just changed (fitView handles that)
    if (prevGraphRef.current !== graph) {
      prevGraphRef.current = graph;
      return;
    }
    const target = getNode(String(selectedNodeId));
    if (!target) return;

    // Compute absolute position by walking up parentId chain
    let absX = target.position.x;
    let absY = target.position.y;
    let current = target;
    while (current.parentId) {
      const parent = getNode(current.parentId);
      if (!parent) break;
      absX += parent.position.x;
      absY += parent.position.y;
      current = parent;
    }

    const w = target.measured?.width ?? 150;
    const h = target.measured?.height ?? 40;
    const centerX = absX + w / 2;
    const centerY = absY + h / 2;

    // Check if node center is already visible in the viewport
    const { x: vx, y: vy, zoom } = getViewport();
    const screenX = centerX * zoom + vx;
    const screenY = centerY * zoom + vy;
    const pad = 60;
    const isVisible =
      screenX >= pad && screenX <= containerWidth - pad &&
      screenY >= pad && screenY <= containerHeight - pad;

    if (!isVisible) {
      // Pan to the node; if over-zoomed, ease back to 1.0
      const targetZoom = Math.min(zoom, 1.0);
      setCenter(centerX, centerY, { duration: 300, zoom: targetZoom });
    }
  }, [selectedNodeId, graph, setCenter, getNode, getViewport, containerWidth, containerHeight]);

  const handleNodeClick: NodeMouseHandler<WorkflowNode> = useCallback(
    (_event, node) => {
      onNodeClick(Number(node.id));
    },
    [onNodeClick],
  );

  return (
    <div style={{ width: '100%', height: '100%', position: 'relative' }}>
      {/* Override @xyflow/react defaults:
            - .react-flow__node-group has a built-in light gray fill + 1px
              border (so nested groups stack into visible gray patches).
            - .react-flow__node-group.selected adds a square box-shadow halo
              on the (un-rounded) wrapper, which mismatches our rounded frame.
            - .react-flow__node:focus adds a browser outline on click.
          Strip all of those so our custom node styles render unobstructed. */}
      <style>{`
        .react-flow__node-group,
        .react-flow__node.parent {
          background: transparent !important;
          border: none !important;
          padding: 0 !important;
          box-shadow: none !important;
          border-radius: 12px !important;
        }
        .react-flow__node-group > div:first-child,
        .react-flow__node.parent > div:first-child {
          background: transparent !important;
        }
        .react-flow__node-group.selected,
        .react-flow__node.parent.selected {
          border: none !important;
          box-shadow: none !important;
        }
        .react-flow__node:focus,
        .react-flow__node:focus-visible,
        .react-flow__node.selectable:focus,
        .react-flow__node.selectable:focus-visible {
          outline: none !important;
          box-shadow: 0 0 0 3px rgba(59, 130, 246, 0.55) !important;
        }
        .react-flow.dark {
          --xy-controls-button-background-color-default: rgba(24, 24, 27, 0.92);
          --xy-controls-button-background-color-hover-default: rgba(39, 39, 42, 0.96);
          --xy-controls-button-color-default: #e4e4e7;
          --xy-controls-button-color-hover-default: #ffffff;
          --xy-controls-button-border-color-default: rgba(255, 255, 255, 0.10);
          --xy-controls-box-shadow-default: 0 8px 24px rgba(0, 0, 0, 0.24);
        }
        .react-flow__controls {
          border: 1px solid rgba(255, 255, 255, 0.10);
          border-radius: 8px;
          overflow: hidden;
          backdrop-filter: blur(8px);
          -webkit-backdrop-filter: blur(8px);
        }
        @keyframes baml-graph-spin {
          to { transform: rotate(360deg); }
        }
        .react-flow__attribution { display: none !important; }
      `}</style>
      <ReactFlow
        nodes={nodes}
        edges={edges}
        onNodesChange={onNodesChange}
        onEdgesChange={onEdgesChange}
        nodeTypes={kNodeTypes}
        edgeTypes={kEdgeTypes}
        onNodeClick={handleNodeClick}
        nodesDraggable={false}
        nodesFocusable
        edgesFocusable={false}
        elementsSelectable
        selectNodesOnDrag={false}
        elevateNodesOnSelect={false}
        elevateEdgesOnSelect={false}
        panOnDrag={[0, 1, 2]}
        panOnScroll
        fitView
        fitViewOptions={{ minZoom: 0.3, maxZoom: 0.85, padding: 0.2 }}
        proOptions={{ hideAttribution: true }}
        colorMode="dark"
      >
        <Controls
          position="bottom-left"
          style={{ display: 'flex', flexDirection: 'row' }}
        />
        <Background variant={BackgroundVariant.Dots} color="rgba(255,255,255,0.10)" gap={18} size={1} />
        <ColorfulMarkerDefinitions />
      </ReactFlow>
      <button
        onClick={() => setDirection((d) => (d === 'horizontal' ? 'vertical' : 'horizontal'))}
        style={{
          position: 'absolute',
          top: 10,
          right: 10,
          zIndex: 10,
          width: 30,
          height: 30,
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          padding: 0,
          borderRadius: 8,
          border: '1px solid rgba(255,255,255,0.10)',
          background: 'rgba(24,24,27,0.75)',
          backdropFilter: 'blur(8px)',
          WebkitBackdropFilter: 'blur(8px)',
          color: '#e4e4e7',
          cursor: 'pointer',
          fontSize: 14,
          lineHeight: 1,
          boxShadow: '0 1px 2px rgba(0,0,0,0.4), inset 0 1px 0 rgba(255,255,255,0.04)',
          transition: 'background 120ms ease, border-color 120ms ease',
        }}
        onMouseEnter={(e) => {
          e.currentTarget.style.background = 'rgba(39,39,42,0.85)';
          e.currentTarget.style.borderColor = 'rgba(255,255,255,0.18)';
        }}
        onMouseLeave={(e) => {
          e.currentTarget.style.background = 'rgba(24,24,27,0.75)';
          e.currentTarget.style.borderColor = 'rgba(255,255,255,0.10)';
        }}
        title={`Switch to ${direction === 'horizontal' ? 'vertical' : 'horizontal'} layout`}
        aria-label="Toggle layout direction"
      >
        {direction === 'horizontal' ? '\u2195' : '\u2194'}
      </button>
    </div>
  );
}

export function GraphView(props: GraphViewProps) {
  return (
    <ReactFlowProvider>
      <GraphViewInner {...props} />
    </ReactFlowProvider>
  );
}
