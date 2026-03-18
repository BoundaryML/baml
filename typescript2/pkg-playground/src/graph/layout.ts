import ELK from 'elkjs/lib/elk.bundled.js';
import type { ElkNode, ElkExtendedEdge } from 'elkjs/lib/elk-api';
import type { WorkflowNode, WorkflowEdge } from './types';

const elk = new ELK();

// Default node sizes by type
const NODE_SIZES: Record<string, { w: number; h: number }> = {
  base: { w: 180, h: 50 },
  llm: { w: 200, h: 60 },
  diamond: { w: 180, h: 50 },
  hexagon: { w: 180, h: 50 },
};

function buildElkNodes(
  allNodes: WorkflowNode[],
  direction: 'horizontal' | 'vertical',
  parentId?: string,
): ElkNode[] {
  const isHorizontal = direction === 'horizontal';

  // Get nodes at this level
  const siblings = allNodes.filter((n) =>
    parentId ? n.parentId === parentId : !n.parentId,
  );

  return siblings.map((node) => {
    const isGroup = node.type === 'group';
    const size = NODE_SIZES[node.type ?? 'base'] ?? NODE_SIZES.base;

    const elkNode: ElkNode = {
      id: node.id,
      // Groups: no explicit size — ELK sizes them from children
      // Leaves: explicit size
      ...(isGroup ? {} : { width: size.w, height: size.h }),
    };

    if (isGroup) {
      // Recursively build children
      const children = buildElkNodes(allNodes, direction, node.id);
      elkNode.layoutOptions = {
        'elk.algorithm': 'layered',
        'elk.direction': isHorizontal ? 'RIGHT' : 'DOWN',
        'elk.hierarchyHandling': 'INCLUDE_CHILDREN',
        'elk.padding': '[top=35,left=15,bottom=15,right=15]',
        'spacing.nodeNode': '30',
        'spacing.nodeNodeBetweenLayers': '40',
      };
      elkNode.labels = [{ text: node.data.label, width: 80, height: 20 }];
      if (children.length > 0) {
        elkNode.children = children;
      } else {
        // Empty group: give it a minimum size
        elkNode.width = 120;
        elkNode.height = 60;
      }
    }

    return elkNode;
  });
}

export async function layoutGraph(
  nodes: WorkflowNode[],
  edges: WorkflowEdge[],
  direction: 'horizontal' | 'vertical' = 'horizontal',
): Promise<{ nodes: WorkflowNode[]; edges: WorkflowEdge[] }> {
  if (nodes.length === 0) return { nodes, edges };

  const isHorizontal = direction === 'horizontal';

  // Collect all node IDs that exist in the ELK graph
  const nodeIds = new Set(nodes.map((n) => n.id));

  const elkGraph: ElkNode = {
    id: 'root',
    layoutOptions: {
      'elk.algorithm': 'layered',
      'elk.direction': isHorizontal ? 'RIGHT' : 'DOWN',
      'elk.hierarchyHandling': 'INCLUDE_CHILDREN',
      'spacing.nodeNode': '30',
      'spacing.nodeNodeBetweenLayers': '50',
      'spacing.edgeNode': '20',
      'spacing.edgeEdge': '15',
    },
    children: buildElkNodes(nodes, direction),
    edges: edges
      .filter((e) => nodeIds.has(e.source) && nodeIds.has(e.target))
      .map(
        (e): ElkExtendedEdge => ({
          id: e.id,
          sources: [e.source],
          targets: [e.target],
        }),
      ),
  };

  const layouted = await elk.layout(elkGraph);

  // Extract positioned nodes from the hierarchical ELK output.
  // For nodes inside groups, ReactFlow expects positions RELATIVE to the parent,
  // so we do NOT accumulate parent offsets for children.
  const positionMap = new Map<string, { x: number; y: number; w: number; h: number }>();

  function extractPositions(elkNodes: ElkNode[] | undefined) {
    if (!elkNodes) return;
    for (const en of elkNodes) {
      positionMap.set(en.id, {
        x: en.x ?? 0,
        y: en.y ?? 0,
        w: en.width ?? 0,
        h: en.height ?? 0,
      });
      if (en.children) {
        extractPositions(en.children);
      }
    }
  }

  extractPositions(layouted.children);

  const laidNodes = nodes.map((node) => {
    const pos = positionMap.get(node.id);
    if (!pos) return node;

    const isGroup = node.type === 'group';
    return {
      ...node,
      position: { x: pos.x, y: pos.y },
      ...(isGroup
        ? {
            style: {
              width: pos.w,
              height: pos.h,
            },
          }
        : {}),
    };
  });

  return { nodes: laidNodes, edges };
}
