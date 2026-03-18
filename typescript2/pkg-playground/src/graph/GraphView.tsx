import { useCallback, useEffect, useState } from 'react';
import {
  ReactFlow,
  ReactFlowProvider,
  useNodesState,
  useEdgesState,
  Controls,
  Background,
  BackgroundVariant,
  type NodeMouseHandler,
} from '@xyflow/react';
import '@xyflow/react/dist/style.css';

import type { ControlFlowGraph } from '../worker-protocol';
import { cfgToGraphNodes, graphToReactflow } from './convert';
import { layoutGraph } from './layout';
import { kNodeTypes } from './nodes';
import { kEdgeTypes, ColorfulMarkerDefinitions } from './edges';
import type { WorkflowNode, WorkflowEdge } from './types';

interface GraphViewProps {
  graph: ControlFlowGraph;
  selectedNodeId: number | null;
  onNodeClick: (nodeId: number) => void;
}

function GraphViewInner({ graph, selectedNodeId, onNodeClick }: GraphViewProps) {
  const [nodes, setNodes, onNodesChange] = useNodesState<WorkflowNode>([]);
  const [edges, setEdges, onEdgesChange] = useEdgesState<WorkflowEdge>([]);
  const [direction, setDirection] = useState<'horizontal' | 'vertical'>('horizontal');

  // Convert CFG -> ReactFlow and run layout
  useEffect(() => {
    const { nodes: graphNodes, edges: graphEdges } = cfgToGraphNodes(graph);
    const { nodes: rfNodes, edges: rfEdges } = graphToReactflow(graphNodes, graphEdges);
    console.log('[GraphView] CFG nodes:', Object.entries(graph.nodes).map(([k, n]) =>
      `${k}: ${n.nodeType} "${n.label}" parent=${n.parentNodeId}`));
    console.log('[GraphView] CFG edges:', Object.entries(graph.edgesBySrc).flatMap(([, es]) =>
      es.map(e => `${e.src}→${e.dst}`)));
    console.log('[GraphView] RF nodes:', rfNodes.map(n => `${n.id}:${n.type}`),
      'RF edges:', rfEdges.map(e => `${e.source}→${e.target}`));

    layoutGraph(rfNodes, rfEdges, direction)
      .then(({ nodes: laid, edges: laidEdges }) => {
        console.log('[GraphView] Layout complete:', laid.length, 'nodes,', laidEdges.length, 'edges');
        setNodes(laid);
        setEdges(laidEdges);
      })
      .catch((err) => {
        console.error('[GraphView] Layout failed:', err);
      });
  }, [graph, direction, setNodes, setEdges]);

  // Update selected state on nodes
  useEffect(() => {
    setNodes((nds) =>
      nds.map((n) => ({
        ...n,
        data: { ...n.data, selected: n.id === String(selectedNodeId) },
      })),
    );
  }, [selectedNodeId, setNodes]);

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
        fitViewOptions={{ minZoom: 0.3, maxZoom: 1.5, padding: 0.2 }}
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
