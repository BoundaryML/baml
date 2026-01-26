/**
 * Adapter: Convert SDK Graph Data to ReactFlow Format
 *
 * This keeps the graph rendering layer isolated from the SDK implementation.
 * The ReactFlow components continue to work with their existing types.
 */

import type { GraphNode, GraphEdge, NodeExecutionState } from './types';
import type { Graph, Reactflow, ReactflowNodeWithData } from '../mock-data/types';
import { graph2workflow, workflow2reactflow } from '../mock-data/convert';

/**
 * Convert SDK graph format to data Graph format
 */
function sdkToGraphFormat(sdkNodes: GraphNode[], sdkEdges: GraphEdge[]): Graph {
  const parentIds = new Set(
    sdkNodes
      .map((node) => node.parent)
      .filter((parentId): parentId is string => Boolean(parentId))
  );

  // Convert SDK nodes to Graph nodes
  const graphNodes: Graph['nodes'] = sdkNodes.map((node) => {
    // Map SDK types to shapes
    let shape: 'diamond' | 'hexagon' | undefined;
    if (node.type === 'conditional') shape = 'diamond';
    if (node.type === 'loop') shape = 'hexagon';

    return {
      id: node.id,
      label: node.label,
      kind: parentIds.has(node.id) ? 'group' : 'item',
      ...(shape ? { shape } : {}),
      ...(node.parent ? { parent: node.parent } : {}),
    };
  });

  // Convert SDK edges to Graph edges
  const graphEdges: Graph['edges'] = sdkEdges.map((edge) => ({
    id: edge.id,
    from: edge.source,
    to: edge.target,
    style: 'solid' as const,
  }));

  return {
    nodes: graphNodes,
    edges: graphEdges,
  };
}

/**
 * Convert full SDK graph to ReactFlow format using proven converters
 */
export function sdkGraphToReactflow(
  nodes: GraphNode[],
  edges: GraphEdge[],
  _direction?: 'horizontal' | 'vertical',
  _nodeStates?: Map<string, NodeExecutionState>
): Reactflow {
  // Convert SDK format to Graph format
  const graph = sdkToGraphFormat(nodes, edges);

  // Use the proven conversion pipeline
  const workflow = graph2workflow(graph);
  const reactflow = workflow2reactflow(workflow);

  // Post-process: Update LLM nodes to use 'llm' type and pass llmClient
  const processedNodes = reactflow.nodes.map((node) => {
    const sdkNode = nodes.find((n) => n.id === node.id);
    if (sdkNode?.type === 'llm_function') {
      return {
        ...node,
        type: 'llm' as const,
        data: {
          ...node.data,
          llmClient: sdkNode.llmClient,
        },
      };
    }
    return node;
  });

  return {
    nodes: processedNodes,
    edges: reactflow.edges,
  };
}

/**
 * Apply execution state styling to ReactFlow nodes
 */
export function applyExecutionStateToNode(
  node: ReactflowNodeWithData,
  state: NodeExecutionState
): ReactflowNodeWithData {
  // Add execution state to node data
  const styledNode = {
    ...node,
    data: {
      ...node.data,
      executionState: state,
    },
  };

  // Apply visual styling based on state
  switch (state) {
    case 'running':
      return {
        ...styledNode,
        className: 'animate-pulse border-blue-500',
      };
    case 'success':
      return {
        ...styledNode,
        className: 'border-green-500',
      };
    case 'error':
      return {
        ...styledNode,
        className: 'border-red-500',
      };
    case 'cached':
      return {
        ...styledNode,
        className: 'border-purple-500',
      };
    case 'pending':
      return {
        ...styledNode,
        className: 'border-yellow-500 opacity-60',
      };
    case 'skipped':
      return {
        ...styledNode,
        className: 'opacity-40',
      };
    default:
      return styledNode;
  }
}
