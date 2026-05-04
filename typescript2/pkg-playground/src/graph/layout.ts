import ELK from 'elkjs/lib/elk.bundled.js';
import type { ElkNode, ElkExtendedEdge } from 'elkjs/lib/elk-api';
import type { WorkflowNode, WorkflowEdge } from './types';
import {
  NODE_IMAGE_PREVIEW_GAP,
  NODE_IMAGE_PREVIEW_MAX,
  NODE_IMAGE_PREVIEW_SINGLE_HEIGHT,
  NODE_IMAGE_PREVIEW_TILE_HEIGHT,
  NODE_IMAGE_PREVIEW_WIDTH,
} from './nodes/NodeOutputPreview';

const elk = new ELK();

// Default node sizes by type — these are the *outer* (wrapper) sizes ELK
// uses for routing. The visible card sits NODE_BUFFER px inside on each
// side so arrow tips and selection rings have clearance.
const NODE_SIZES: Record<string, { w: number; h: number }> = {
  base: { w: 180, h: 50 },
  llm: { w: 200, h: 60 },
  diamond: { w: 180, h: 50 },
  hexagon: { w: 180, h: 50 },
};

/**
 * Visual buffer reserved on every side of a node — ELK routes edges to
 * the wrapper edge (this far outside the visible card), so arrow tips
 * and selection halos don't overlap the card border.
 */
export const NODE_BUFFER = 6;

function nodeSize(node: WorkflowNode): { w: number; h: number } {
  const base = NODE_SIZES[node.type ?? 'base'] ?? NODE_SIZES.base;
  const imageCount = node.data.imageOutputs?.length ?? 0;
  const visualSize = (() => {
    if (imageCount > 0) {
      const visibleCount = Math.min(imageCount, NODE_IMAGE_PREVIEW_MAX);
      const previewRows = visibleCount === 1 ? 1 : Math.ceil(visibleCount / 2);
      const previewHeight = visibleCount === 1
        ? NODE_IMAGE_PREVIEW_SINGLE_HEIGHT
        : previewRows * NODE_IMAGE_PREVIEW_TILE_HEIGHT
          + (previewRows - 1) * NODE_IMAGE_PREVIEW_GAP;
      return {
        w: Math.max(base.w, NODE_IMAGE_PREVIEW_WIDTH),
        h: base.h + previewHeight + 14,
      };
    }

    if (node.data.errorMessage) {
      return {
        w: Math.max(base.w, 360),
        h: base.h + 176,
      };
    }

    if (node.data.hasResult) {
      return {
        w: Math.max(base.w, 360),
        h: base.h + 216,
      };
    }

    return base;
  })();
  return {
    w: visualSize.w + 2 * NODE_BUFFER,
    h: visualSize.h + 2 * NODE_BUFFER,
  };
}

function buildElkNodes(
  allNodes: WorkflowNode[],
  direction: 'horizontal' | 'vertical',
  edgesByOwner: Map<string, ElkExtendedEdge[]>,
  portsByNode: Map<string, { incoming: number; outgoing: number }>,
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
      const children = buildElkNodes(allNodes, direction, edgesByOwner, portsByNode, node.id);
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
      // FIXED_SIDE: each port is locked to a side (W/E or N/S) but ELK
      // is free to spread ports along that side. With one port per edge,
      // multiple outgoing edges no longer pile through the same exit.
      elkNode.layoutOptions = {
        'org.eclipse.elk.portConstraints': 'FIXED_SIDE',
      };
      const counts = portsByNode.get(node.id) ?? { incoming: 0, outgoing: 0 };
      const targetCount = Math.max(1, counts.incoming);
      const sourceCount = Math.max(1, counts.outgoing);
      const targetPorts = Array.from({ length: targetCount }, (_, i) => ({
        id: `${node.id}-target-${i}`,
        layoutOptions: { 'port.side': isHorizontal ? 'WEST' : 'NORTH' },
      }));
      const sourcePorts = Array.from({ length: sourceCount }, (_, i) => ({
        id: `${node.id}-source-${i}`,
        layoutOptions: { 'port.side': isHorizontal ? 'EAST' : 'SOUTH' },
      }));
      elkNode.ports = [...targetPorts, ...sourcePorts];
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

  // Count outgoing/incoming edges per (non-group) node so we can declare
  // one ELK port per edge — that lets ELK spread multiple branches out
  // along the side rather than forcing them all through one exit point.
  const portsByNode = new Map<string, { incoming: number; outgoing: number }>();
  for (const e of validEdges) {
    if (!groupNodeIds.has(e.source)) {
      const v = portsByNode.get(e.source) ?? { incoming: 0, outgoing: 0 };
      v.outgoing += 1;
      portsByNode.set(e.source, v);
    }
    if (!groupNodeIds.has(e.target)) {
      const v = portsByNode.get(e.target) ?? { incoming: 0, outgoing: 0 };
      v.incoming += 1;
      portsByNode.set(e.target, v);
    }
  }

  // Per-edge port index counters — give each edge a unique slot.
  const sourceIdx = new Map<string, number>();
  const targetIdx = new Map<string, number>();
  const edgePortIds = new Map<string, { sourcePort: string; targetPort: string }>();

  // Distribute edges to their LCA group for better within-group layout.
  const edgesByOwner = new Map<string, ElkExtendedEdge[]>();
  for (const e of validEdges) {
    const lca = findLCA(e.source, e.target, nodeById, groupNodeIds);

    const sourcePort = groupNodeIds.has(e.source)
      ? e.source
      : `${e.source}-source-${sourceIdx.get(e.source) ?? 0}`;
    if (!groupNodeIds.has(e.source)) {
      sourceIdx.set(e.source, (sourceIdx.get(e.source) ?? 0) + 1);
    }

    const targetPort = groupNodeIds.has(e.target)
      ? e.target
      : `${e.target}-target-${targetIdx.get(e.target) ?? 0}`;
    if (!groupNodeIds.has(e.target)) {
      targetIdx.set(e.target, (targetIdx.get(e.target) ?? 0) + 1);
    }
    edgePortIds.set(e.id, { sourcePort, targetPort });

    const elkEdge: ElkExtendedEdge = {
      id: `elk-${e.id}`,
      sources: [sourcePort],
      targets: [targetPort],
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
    children: buildElkNodes(nodes, direction, edgesByOwner, portsByNode),
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

  // ── Extract ELK's orthogonal edge routes ─────────────────────────
  // ELK reports edge sections in coordinates relative to the edge's
  // OWNER container (the LCA group we attached the edge to). We collect
  // every edge along with its owner so we can translate to absolute
  // flow coordinates before handing the polyline to the edge component.
  type ElkSection = {
    startPoint?: { x: number; y: number };
    endPoint?: { x: number; y: number };
    bendPoints?: { x: number; y: number }[];
  };
  const elkEdgeOwners = new Map<string, { ownerId: string; sections: ElkSection[] }>();
  function collectElkEdges(elkNode: ElkNode, ownerId: string) {
    if (elkNode.edges) {
      for (const e of elkNode.edges as Array<ElkExtendedEdge & { sections?: ElkSection[] }>) {
        const sections = e.sections ?? [];
        if (sections.length > 0) elkEdgeOwners.set(e.id, { ownerId, sections });
      }
    }
    if (elkNode.children) {
      for (const child of elkNode.children) collectElkEdges(child, child.id);
    }
  }
  collectElkEdges(layouted, 'root');

  const laidEdges = edges.map((edge) => {
    const elkInfo = elkEdgeOwners.get(`elk-${edge.id}`);
    const srcPos = absPositions.get(edge.source);
    const tgtPos = absPositions.get(edge.target);

    // Default handles match ELK's port sides (West/East horizontally;
    // North/South vertically). If we have ELK routing, the visible path
    // comes from the polyline below — the handle just anchors the edge
    // ends in React Flow's coordinate model.
    const sourceHandle = isHorizontal ? 'right-source' : 'bottom-source';
    const targetHandle = isHorizontal ? 'left-target' : 'top-target';

    if (elkInfo) {
      // Owner offset: edges owned by 'root' need no offset; edges owned
      // by a group are shifted by that group's absolute position.
      const ownerAbs = elkInfo.ownerId === 'root'
        ? { x: 0, y: 0 }
        : absPositions.get(elkInfo.ownerId) ?? { x: 0, y: 0 };
      const ox = ownerAbs.x;
      const oy = ownerAbs.y;
      const points = elkInfo.sections.flatMap((sec) => {
        const start = { x: (sec.startPoint?.x ?? 0) + ox, y: (sec.startPoint?.y ?? 0) + oy };
        const end = { x: (sec.endPoint?.x ?? 0) + ox, y: (sec.endPoint?.y ?? 0) + oy };
        const bends = (sec.bendPoints ?? []).map((p) => ({
          x: p.x + ox,
          y: p.y + oy,
        }));
        return [start, ...bends, end];
      });
      // De-dupe consecutive identical points (ELK occasionally emits these
      // and they create zero-length segments that read as visual artifacts).
      const deduped = points.filter((p, i, arr) => {
        if (i === 0) return true;
        const prev = arr[i - 1]!;
        return Math.abs(p.x - prev.x) > 0.5 || Math.abs(p.y - prev.y) > 0.5;
      });

      return {
        ...edge,
        sourceHandle,
        targetHandle,
        data: { ...edge.data, points: deduped },
      };
    }

    // Fallback: no ELK route available — keep direction-heuristic handles
    // so getSmoothStepPath at least picks a reasonable side.
    if (!srcPos || !tgtPos) return { ...edge, sourceHandle, targetHandle };
    const dx = (tgtPos.x + tgtPos.w / 2) - (srcPos.x + srcPos.w / 2);
    const dy = (tgtPos.y + tgtPos.h / 2) - (srcPos.y + srcPos.h / 2);
    let sH: string;
    let tH: string;
    if (Math.abs(dx) >= Math.abs(dy)) {
      sH = dx >= 0 ? 'right-source' : 'left-source';
      tH = dx >= 0 ? 'left-target' : 'right-target';
    } else {
      sH = dy >= 0 ? 'bottom-source' : 'top-source';
      tH = dy >= 0 ? 'top-target' : 'bottom-target';
    }
    return { ...edge, sourceHandle: sH, targetHandle: tH };
  });

  // Apply positions and lock the React Flow wrapper to ELK's assumed
  // size. For non-group nodes the wrapper includes a NODE_BUFFER inset
  // so the visible card sits inside the edge-routing boundary.
  const laidNodes = nodes.map((node) => {
    const pos = positionMap.get(node.id);
    if (!pos) return node;
    const isGroup = node.type === 'group';
    return {
      ...node,
      position: { x: pos.x, y: pos.y },
      style: {
        ...node.style,
        width: pos.w,
        height: pos.h,
        ...(isGroup
          ? {}
          : { padding: NODE_BUFFER, boxSizing: 'border-box' as const }),
      },
    };
  });

  return { nodes: laidNodes, edges: laidEdges };
}
