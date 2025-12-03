import { lastOf } from '@del-wang/utils';

import type { Graph, Reactflow, Workflow } from './types';

export const graph2workflow = (graph: Graph): Workflow => {
  const { nodes = [], edges = [] } = graph ?? {};

  // Track the number of outgoing edges per node to assign source handle indices
  const sourceHandleIndex: Record<string, number> = {};
  // Track the number of incoming edges per node to assign target handle indices
  const targetHandleIndex: Record<string, number> = {};

  // Convert edges
  const workflowEdges = edges.map((edge) => {
    const { id, from, to } = edge;

    // Get or initialize handle indices
    if (sourceHandleIndex[from] === undefined) {
      sourceHandleIndex[from] = 0;
    }
    if (targetHandleIndex[to] === undefined) {
      targetHandleIndex[to] = 0;
    }

    const sourceHandle = `${from}#source#${sourceHandleIndex[from]}`;
    const targetHandle = `${to}#target#${targetHandleIndex[to]}`;

    // Increment indices for next edge
    sourceHandleIndex[from]++;
    targetHandleIndex[to]++;

    return {
      id,
      source: from,
      target: to,
      sourceHandle,
      targetHandle,
    };
  });

  // Convert nodes
  const workflowNodes = nodes.map((node) => {
    const { id, label, kind, parent, shape } = node;

    // Determine node type based on kind and shape
    let type: 'base' | 'group' | 'diamond' | 'hexagon';
    if (kind === 'group') {
      type = 'group';
    } else if (shape === 'diamond') {
      type = 'diamond';
    } else if (shape === 'hexagon') {
      type = 'hexagon';
    } else {
      type = 'base';
    }

    return {
      id,
      type,
      label,
      shape,
      ...(kind === 'group' ? { isGroup: true } : {}),
      ...(parent ? { parentId: parent } : {}),
    };
  });

  return {
    nodes: workflowNodes,
    edges: workflowEdges,
  };
};

export const workflow2reactflow = (workflow: Workflow): Reactflow => {
  const { nodes = [], edges = [] } = workflow ?? {};
  const edgesCount: Record<string, number> = {};
  const edgesIndex: Record<string, { source: number; target: number }> = {};
  const nodeHandles: Record<
    string,
    {
      sourceHandles: Record<string, number>;
      targetHandles: Record<string, number>;
    }
  > = {};

  for (const edge of edges) {
    const { source, target, sourceHandle, targetHandle } = edge;
    edgesCount[sourceHandle] = (edgesCount[sourceHandle] ?? 0) + 1;
    edgesCount[targetHandle] = (edgesCount[targetHandle] ?? 0) + 1;
    edgesCount[`source-${source}`] = (edgesCount[`source-${source}`] ?? 0) + 1;
    edgesCount[`target-${target}`] = (edgesCount[`target-${target}`] ?? 0) + 1;
    edgesIndex[edge.id] = {
      source: edgesCount[sourceHandle] - 1,
      target: edgesCount[targetHandle] - 1,
    };
    if (!nodeHandles[source]) {
      nodeHandles[source] = { sourceHandles: {}, targetHandles: {} };
    }
    if (!nodeHandles[target]) {
      nodeHandles[target] = { sourceHandles: {}, targetHandles: {} };
    }
    if (!nodeHandles[source].sourceHandles[sourceHandle]) {
      nodeHandles[source].sourceHandles[sourceHandle] = 1;
    } else {
      nodeHandles[source].sourceHandles[sourceHandle] += 1;
    }
    if (!nodeHandles[target].targetHandles[targetHandle]) {
      nodeHandles[target].targetHandles[targetHandle] = 1;
    } else {
      nodeHandles[target].targetHandles[targetHandle] += 1;
    }
  }

  return {
    nodes: nodes.map((node) => {
      const reactFlowNode = {
        ...node,
        data: {
          ...node,
          sourceHandles:
            Object.keys(nodeHandles[node.id]?.sourceHandles ?? {}) ?? [],
          targetHandles:
            Object.keys(nodeHandles[node.id]?.targetHandles ?? {}) ?? [],
        },
        position: { x: 0, y: 0 },
        // For child nodes, set extent and expandParent
        ...(node.parentId
          ? {
            parentNode: node.parentId,
            extent: 'parent' as const,
            expandParent: true,
          }
          : {}),
        // For group nodes, set proper styling (dimensions will be set by ELK layout)
        ...(node.isGroup
          ? {
            style: {
              // DO NOT set width/height here - ELK will calculate based on children
              // width and height will be set in the layout algorithm
              background: 'rgba(240, 240, 255, 0.25)',
              border: '2px solid #555',
              borderRadius: '8px',
              padding: '20px',
            },
          }
          : {}),
      };

      return reactFlowNode;
    }),
    edges: edges.map((edge) => ({
      ...edge,
      data: {
        sourcePort: {
          edges: edgesCount[`source-${edge.source}`] ?? 0,
          portIndex: parseInt(lastOf(edge.sourceHandle.split('#'))!, 10),
          portCount:
            Object.keys(nodeHandles[edge.source]?.sourceHandles ?? {}).length,
          edgeIndex: edgesIndex[edge.id]?.source ?? 0,
          edgeCount: edgesCount[edge.sourceHandle] ?? 0,
        },
        targetPort: {
          edges: edgesCount[`target-${edge.target}`] ?? 0,
          portIndex: parseInt(lastOf(edge.targetHandle.split('#'))!, 10),
          portCount:
            Object.keys(nodeHandles[edge.target]?.targetHandles ?? {}).length,
          edgeIndex: edgesIndex[edge.id]?.target ?? 0,
          edgeCount: edgesCount[edge.targetHandle] ?? 0,
        },
      },
    })),
  };
};
