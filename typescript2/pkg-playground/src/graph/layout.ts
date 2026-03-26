import ELK from 'elkjs/lib/elk.bundled.js';
import type { ElkNode, ElkExtendedEdge } from 'elkjs/lib/elk-api';
import type { WorkflowNode, WorkflowEdge, EdgePathData } from './types';

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
    } else {
      // Leaf node: explicit size + FIXED_SIDE ports
      elkNode.width = size.w;
      elkNode.height = size.h;
      elkNode.layoutOptions = {
        'org.eclipse.elk.portConstraints': 'FIXED_SIDE',
      };
      elkNode.ports = [
        {
          id: `${node.id}-target`,
          layoutOptions: {
            'port.side': isHorizontal ? 'WEST' : 'NORTH',
          },
        },
        {
          id: `${node.id}-source`,
          layoutOptions: {
            'port.side': isHorizontal ? 'EAST' : 'SOUTH',
          },
        },
      ];
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

  const validEdges = edges.filter((e) => nodeIds.has(e.source) && nodeIds.has(e.target));

  // Track which nodes are groups (no ports) vs leaves (have ports)
  const groupNodeIds = new Set(nodes.filter((n) => n.type === 'group').map((n) => n.id));

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
      'elk.edgeRouting': 'ORTHOGONAL',
      'elk.layered.edgeRouting.selfLoopDistribution': 'EQUALLY',
    },
    children: buildElkNodes(nodes, direction),
    edges: validEdges.map(
      (e): ElkExtendedEdge => ({
        id: `elk-${e.id}`,
        // Only reference ports on leaf nodes; groups don't have ports
        sources: [groupNodeIds.has(e.source) ? e.source : `${e.source}-source`],
        targets: [groupNodeIds.has(e.target) ? e.target : `${e.target}-target`],
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

  // Extract ELK edge path sections, accumulating absolute offsets for nested groups.
  // ELK sections are relative to the containing graph node, so we add parent offsets
  // when descending into children.
  const edgePathMap = new Map<string, EdgePathData>();

  function extractEdgePaths(elkNode: ElkNode, offsetX = 0, offsetY = 0) {
    const nodeX = offsetX + (elkNode.x ?? 0);
    const nodeY = offsetY + (elkNode.y ?? 0);

    if (elkNode.edges) {
      for (const elkEdge of elkNode.edges) {
        const section = elkEdge.sections?.[0];
        if (!section) continue;

        // Edges stored on 'root' are in absolute coordinates; edges on child
        // groups are relative to that group's position.
        const absOffsetX = elkNode.id === 'root' ? 0 : nodeX;
        const absOffsetY = elkNode.id === 'root' ? 0 : nodeY;

        const transformPoint = (p: { x: number; y: number }) => ({
          x: p.x + absOffsetX,
          y: p.y + absOffsetY,
        });

        const points: Array<{ x: number; y: number }> = [];
        points.push(transformPoint(section.startPoint));
        if (section.bendPoints) {
          points.push(...section.bendPoints.map(transformPoint));
        }
        points.push(transformPoint(section.endPoint));

        // Map back from elk edge id ("elk-<ourId>") to our edge id
        const ourEdgeId = elkEdge.id.replace(/^elk-/, '');
        edgePathMap.set(ourEdgeId, { points });
      }
    }

    if (elkNode.children) {
      for (const child of elkNode.children) {
        extractEdgePaths(child, nodeX, nodeY);
      }
    }
  }

  extractEdgePaths(layouted);

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

  const laidEdges = edges.map((e) => {
    const pathData = edgePathMap.get(e.id);
    return pathData ? { ...e, data: { ...(e.data ?? {}), pathData } } : e;
  });

  return { nodes: laidNodes, edges: laidEdges };
}
