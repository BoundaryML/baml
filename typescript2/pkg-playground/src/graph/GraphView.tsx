import { useCallback, useEffect, useRef, useState } from 'react';
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

import type { ControlFlowGraph, DeserializedRuntimeEvent } from '../worker-protocol';
import { cfgToGraphNodes, graphToReactflow } from './convert';
import { collectGraphNodeOutputs } from './runtime-output';
import { layoutGraph } from './layout';
import { kNodeTypes } from './nodes';
import { kEdgeTypes, ColorfulMarkerDefinitions } from './edges';
import type { WorkflowNode, WorkflowEdge } from './types';

interface GraphViewProps {
  graph: ControlFlowGraph;
  runtimeEvents?: DeserializedRuntimeEvent[];
  selectedNodeId: number | null;
  onNodeClick: (nodeId: number) => void;
}

const EMPTY_RUNTIME_EVENTS: DeserializedRuntimeEvent[] = [];

function GraphViewInner({ graph, runtimeEvents = EMPTY_RUNTIME_EVENTS, selectedNodeId, onNodeClick }: GraphViewProps) {
  const [nodes, setNodes, onNodesChange] = useNodesState<WorkflowNode>([]);
  const [edges, setEdges, onEdgesChange] = useEdgesState<WorkflowEdge>([]);
  const [direction, setDirection] = useState<'horizontal' | 'vertical'>('horizontal');

  // Convert CFG -> ReactFlow and run layout
  useEffect(() => {
    const { nodes: graphNodes, edges: graphEdges } = cfgToGraphNodes(graph);
    const { nodes: rfNodes, edges: rfEdges } = graphToReactflow(graphNodes, graphEdges);
    const outputs = collectGraphNodeOutputs(graphNodes, runtimeEvents);
    const selectedId = selectedNodeId == null ? null : String(selectedNodeId);
    const nodesWithOutputs = rfNodes.map((node) => {
      const output = outputs.get(node.id);
      const selected = node.id === selectedId;
      if (!output) {
        return {
          ...node,
          data: {
            ...node.data,
            selected,
          },
        };
      }

      return {
        ...node,
        data: {
          ...node.data,
          result: output.result,
          imageOutputs: output.imageOutputs,
          executionState: 'success' as const,
          selected,
        },
      };
    });
    console.log('[GraphView] CFG nodes:', Object.entries(graph.nodes).map(([k, n]) =>
      `${k}: ${n.nodeType} "${n.label}" parent=${n.parentNodeId}`));
    console.log('[GraphView] CFG edges:', Object.entries(graph.edgesBySrc).flatMap(([, es]) =>
      es.map(e => `${e.src}→${e.dst}`)));
    console.log('[GraphView] RF nodes:', rfNodes.map(n => `${n.id}:${n.type}`),
      'RF edges:', rfEdges.map(e => `${e.source}→${e.target}`));

    layoutGraph(nodesWithOutputs, rfEdges, direction)
      .then(({ nodes: laid, edges: laidEdges }) => {
        console.log('[GraphView] Layout complete:', laid.length, 'nodes,', laidEdges.length, 'edges');
        setNodes(laid);
        setEdges(laidEdges);
      })
      .catch((err) => {
        console.error('[GraphView] Layout failed:', err);
      });
  }, [graph, runtimeEvents, selectedNodeId, direction, setNodes, setEdges]);

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
      <ReactFlow
        nodes={nodes}
        edges={edges}
        onNodesChange={onNodesChange}
        onEdgesChange={onEdgesChange}
        nodeTypes={kNodeTypes}
        edgeTypes={kEdgeTypes}
        onNodeClick={handleNodeClick}
        nodesDraggable={false}
        panOnDrag={[0, 1, 2]}
        panOnScroll
        fitView
        fitViewOptions={{ minZoom: 0.3, maxZoom: 0.85, padding: 0.2 }}
        proOptions={{ hideAttribution: true }}
      >
        <Controls
          position="bottom-left"
          style={{ display: 'flex', flexDirection: 'row' }}
        />
        <Background variant={BackgroundVariant.Dots} color="#333" gap={16} />
        <ColorfulMarkerDefinitions />
      </ReactFlow>
      <button
        onClick={() => setDirection((d) => (d === 'horizontal' ? 'vertical' : 'horizontal'))}
        style={{
          position: 'absolute',
          top: 8,
          right: 8,
          zIndex: 10,
          padding: '4px 8px',
          borderRadius: 4,
          border: '1px solid #555',
          background: '#2d2d2d',
          color: '#ccc',
          cursor: 'pointer',
          fontSize: 14,
          lineHeight: 1,
        }}
        title={`Switch to ${direction === 'horizontal' ? 'vertical' : 'horizontal'} layout`}
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
