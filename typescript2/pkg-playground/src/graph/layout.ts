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
  edgesByOwner: Map<string, ElkExtendedEdge[]>,
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
      const children = buildElkNodes(allNodes, direction, edgesByOwner, node.id);
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
      // Attach edges whose LCA is this group
      const groupEdges = edgesByOwner.get(node.id);
      if (groupEdges && groupEdges.length > 0) {
        elkNode.edges = groupEdges;
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

// ── Edge LCA (lowest common ancestor) computation ────────────────────
// Edges placed at root level get routed in global space and may exit
// group boundaries. Placing each edge on its LCA group makes ELK
// route it within that group's coordinate space, respecting boundaries.

function getAncestorChain(nodeId: string, nodeById: Map<string, WorkflowNode>): string[] {
  const chain: string[] = [];
  let cur = nodeById.get(nodeId);
  while (cur) {
    chain.push(cur.id);
    if (!cur.parentId) break;
    cur = nodeById.get(cur.parentId);
  }
  return chain; // [self, parent, grandparent, ..., root-level-node]
}

function findLCA(
  sourceId: string,
  targetId: string,
  nodeById: Map<string, WorkflowNode>,
  groupNodeIds: Set<string>,
): string {
  const srcChain = getAncestorChain(sourceId, nodeById);
  const tgtSet = new Set(getAncestorChain(targetId, nodeById));
  for (const a of srcChain) {
    // LCA must be a group (container) or root — not a leaf node
    if (tgtSet.has(a) && groupNodeIds.has(a)) return a;
  }
  return 'root';
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
  const nodeById = new Map(nodes.map((n) => [n.id, n]));

  // ── Distribute edges to their LCA group ──────────────────────────────
  // Instead of putting all edges at root, place each edge on the deepest
  // group that contains both its source and target. This makes ELK route
  // the edge within that group's coordinate space, keeping it inside the
  // group boundary.
  const edgesByOwner = new Map<string, ElkExtendedEdge[]>();
  for (const e of validEdges) {
    const lca = findLCA(e.source, e.target, nodeById, groupNodeIds);
    const elkEdge: ElkExtendedEdge = {
      id: `elk-${e.id}`,
      sources: [groupNodeIds.has(e.source) ? e.source : `${e.source}-source`],
      targets: [groupNodeIds.has(e.target) ? e.target : `${e.target}-target`],
    };
    const list = edgesByOwner.get(lca) ?? [];
    list.push(elkEdge);
    edgesByOwner.set(lca, list);
  }

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
    children: buildElkNodes(nodes, direction, edgesByOwner),
    edges: edgesByOwner.get('root') ?? [],
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

  // ── Extract ELK edge path sections ──────────────────────────────────
  // ELK sections are relative to the containing graph node. We must
  // convert them to the same coordinate system used by extractPositions
  // (which starts from layouted.children, NOT from layouted itself).
  //
  // Root-level edges: sections are already in the root coordinate system
  // (same as node positions), so no offset is needed.
  //
  // Child-group edges (if ELK redistributes any during INCLUDE_CHILDREN
  // layout): sections are relative to the group, so we accumulate group
  // offsets starting from 0 — NOT from root.x/root.y.
  const edgePathMap = new Map<string, EdgePathData>();

  function collectEdgeSections(elkNode: ElkNode, offsetX: number, offsetY: number) {
    const absX = offsetX + (elkNode.x ?? 0);
    const absY = offsetY + (elkNode.y ?? 0);

    if (elkNode.edges) {
      for (const elkEdge of elkNode.edges) {
        const section = elkEdge.sections?.[0];
        if (!section) continue;

        const transform = (p: { x: number; y: number }) => ({
          x: p.x + absX,
          y: p.y + absY,
        });

        const points: Array<{ x: number; y: number }> = [];
        points.push(transform(section.startPoint));
        if (section.bendPoints) {
          points.push(...section.bendPoints.map(transform));
        }
        points.push(transform(section.endPoint));

        const ourEdgeId = elkEdge.id.replace(/^elk-/, '');
        edgePathMap.set(ourEdgeId, { points });
      }
    }

    if (elkNode.children) {
      for (const child of elkNode.children) {
        collectEdgeSections(child, absX, absY);
      }
    }
  }

  // Root-level edges: no offset (sections already in root coordinates)
  if (layouted.edges) {
    for (const elkEdge of layouted.edges) {
      const section = elkEdge.sections?.[0];
      if (!section) continue;
      const points: Array<{ x: number; y: number }> = [];
      points.push(section.startPoint);
      if (section.bendPoints) points.push(...section.bendPoints);
      points.push(section.endPoint);
      edgePathMap.set(elkEdge.id.replace(/^elk-/, ''), { points });
    }
  }

  // Child-group edges: accumulate offsets from 0 (not root.x/root.y)
  for (const child of layouted.children ?? []) {
    collectEdgeSections(child, 0, 0);
  }

  // ── Validate edge paths and drop broken ones ─────────────────────────
  // If an edge's ELK path endpoints are far from the source/target nodes,
  // remove the pathData so BaseEdge falls back to getSmoothStepPath.
  const absPositions = new Map<string, { x: number; y: number; w: number; h: number }>();
  for (const n of nodes) {
    let absX = positionMap.get(n.id)?.x ?? 0;
    let absY = positionMap.get(n.id)?.y ?? 0;
    let cur = n;
    while (cur.parentId) {
      const parent = nodeById.get(cur.parentId);
      if (!parent) break;
      absX += positionMap.get(parent.id)?.x ?? 0;
      absY += positionMap.get(parent.id)?.y ?? 0;
      cur = parent;
    }
    absPositions.set(n.id, {
      x: absX,
      y: absY,
      w: positionMap.get(n.id)?.w ?? 0,
      h: positionMap.get(n.id)?.h ?? 0,
    });
  }

  for (const e of validEdges) {
    const pd = edgePathMap.get(e.id);
    if (!pd || pd.points.length < 2) continue;

    const srcPos = absPositions.get(e.source);
    const tgtPos = absPositions.get(e.target);
    if (!srcPos || !tgtPos) continue;

    const startPt = pd.points[0];
    const endPt = pd.points[pd.points.length - 1];

    const srcCenterX = srcPos.x + srcPos.w / 2;
    const srcCenterY = srcPos.y + srcPos.h / 2;
    const tgtCenterX = tgtPos.x + tgtPos.w / 2;
    const tgtCenterY = tgtPos.y + tgtPos.h / 2;

    const startDist = Math.hypot(startPt.x - srcCenterX, startPt.y - srcCenterY);
    const endDist = Math.hypot(endPt.x - tgtCenterX, endPt.y - tgtCenterY);

    // Groups can be large, so use a generous threshold
    const maxDim = Math.max(srcPos.w, srcPos.h, tgtPos.w, tgtPos.h, 200);
    if (startDist > maxDim * 1.5 || endDist > maxDim * 1.5) {
      // Edge path is too far from nodes — drop it so BaseEdge falls back
      edgePathMap.delete(e.id);
    }
  }

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
