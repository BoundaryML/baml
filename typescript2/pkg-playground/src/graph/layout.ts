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

function nodeSize(node: WorkflowNode): { w: number; h: number } {
  const base = NODE_SIZES[node.type ?? 'base'] ?? NODE_SIZES.base;
  const imageCount = node.data.imageOutputs?.length ?? 0;
  if (imageCount === 0) return base;

  const visibleCount = Math.min(imageCount, 4);
  const previewRows = visibleCount === 1 ? 1 : Math.ceil(visibleCount / 2);
  const previewHeight = visibleCount === 1 ? 104 : previewRows * 68;
  return {
    w: Math.max(base.w, 220),
    h: base.h + previewHeight,
  };
}

function buildElkNodes(
  allNodes: WorkflowNode[],
  direction: 'horizontal' | 'vertical',
  edgesByOwner: Map<string, ElkExtendedEdge[]>,
  parentId?: string,
): ElkNode[] {
  const isHorizontal = direction === 'horizontal';

  const siblings = allNodes.filter((n) =>
    parentId ? n.parentId === parentId : !n.parentId,
  );

  return siblings.map((node) => {
    const isGroup = node.type === 'group';
    const size = nodeSize(node);

    const elkNode: ElkNode = { id: node.id };

    if (isGroup) {
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
        elkNode.width = 120;
        elkNode.height = 60;
      }
      // Attach edges whose LCA is this group — ELK uses them for
      // layer ordering and spacing within the group.
      const groupEdges = edgesByOwner.get(node.id);
      if (groupEdges && groupEdges.length > 0) {
        elkNode.edges = groupEdges;
      }
    } else {
      elkNode.width = size.w;
      elkNode.height = size.h;
      elkNode.layoutOptions = {
        'org.eclipse.elk.portConstraints': 'FIXED_SIDE',
      };
      elkNode.ports = [
        {
          id: `${node.id}-target`,
          layoutOptions: { 'port.side': isHorizontal ? 'WEST' : 'NORTH' },
        },
        {
          id: `${node.id}-source`,
          layoutOptions: { 'port.side': isHorizontal ? 'EAST' : 'SOUTH' },
        },
      ];
    }

    return elkNode;
  });
}

// ── Edge LCA (lowest common ancestor) ───────────────────────────────
// Placing each edge on the deepest group containing both endpoints
// gives ELK correct local context for layer ordering and spacing.

function getAncestorChain(nodeId: string, nodeById: Map<string, WorkflowNode>): string[] {
  const chain: string[] = [];
  let cur = nodeById.get(nodeId);
  while (cur) {
    chain.push(cur.id);
    if (!cur.parentId) break;
    cur = nodeById.get(cur.parentId);
  }
  return chain;
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
    if (tgtSet.has(a) && groupNodeIds.has(a)) return a;
  }
  return 'root';
}

// ── Public API ──────────────────────────────────────────────────────

export async function layoutGraph(
  nodes: WorkflowNode[],
  edges: WorkflowEdge[],
  direction: 'horizontal' | 'vertical' = 'horizontal',
): Promise<{ nodes: WorkflowNode[]; edges: WorkflowEdge[] }> {
  if (nodes.length === 0) return { nodes, edges };

  const isHorizontal = direction === 'horizontal';
  const nodeIds = new Set(nodes.map((n) => n.id));
  const validEdges = edges.filter((e) => nodeIds.has(e.source) && nodeIds.has(e.target));
  const groupNodeIds = new Set(nodes.filter((n) => n.type === 'group').map((n) => n.id));
  const nodeById = new Map(nodes.map((n) => [n.id, n]));

  // Distribute edges to their LCA group for better within-group layout.
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
    },
    children: buildElkNodes(nodes, direction, edgesByOwner),
    edges: edgesByOwner.get('root') ?? [],
  };

  const layouted = await elk.layout(elkGraph);

  // Extract node positions. ELK returns parent-relative coordinates,
  // which is exactly what ReactFlow expects for nodes inside groups.
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
      if (en.children) extractPositions(en.children);
    }
  }

  extractPositions(layouted.children);

  // ── Compute absolute positions for handle selection ─────────────────
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
    const pos = positionMap.get(n.id);
    absPositions.set(n.id, {
      x: absX,
      y: absY,
      w: pos?.w ?? 0,
      h: pos?.h ?? 0,
    });
  }

  // ── Pick optimal handle direction per edge ────────────────────────
  // Compare absolute center positions of source and target to determine
  // whether the edge should exit/enter horizontally or vertically.
  const laidEdges = edges.map((edge) => {
    const srcPos = absPositions.get(edge.source);
    const tgtPos = absPositions.get(edge.target);
    if (!srcPos || !tgtPos) return edge;

    const dx = (tgtPos.x + tgtPos.w / 2) - (srcPos.x + srcPos.w / 2);
    const dy = (tgtPos.y + tgtPos.h / 2) - (srcPos.y + srcPos.h / 2);

    let sourceHandle: string;
    let targetHandle: string;

    if (Math.abs(dx) >= Math.abs(dy)) {
      // Predominantly horizontal
      sourceHandle = dx >= 0 ? 'right-source' : 'left-source';
      targetHandle = dx >= 0 ? 'left-target' : 'right-target';
    } else {
      // Predominantly vertical
      sourceHandle = dy >= 0 ? 'bottom-source' : 'top-source';
      targetHandle = dy >= 0 ? 'top-target' : 'bottom-target';
    }

    return { ...edge, sourceHandle, targetHandle };
  });

  // Apply positions to nodes. Groups also get explicit width/height.
  const laidNodes = nodes.map((node) => {
    const pos = positionMap.get(node.id);
    if (!pos) return node;
    return {
      ...node,
      position: { x: pos.x, y: pos.y },
      ...(node.type === 'group'
        ? { style: { width: pos.w, height: pos.h } }
        : {}),
    };
  });

  return { nodes: laidNodes, edges: laidEdges };
}
