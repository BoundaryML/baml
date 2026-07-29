import ELK from 'elkjs/lib/elk.bundled.js';
import type { ElkExtendedEdge, ElkNode } from 'elkjs/lib/elk-api';
import {
  CAPTURED_VALUE_CARD_HEADER_HEIGHT,
  CAPTURED_VALUE_CARD_PADDING_Y,
  CAPTURED_VALUE_CARD_TEXT_HEIGHT,
  capturedValueCardContentHeight,
  capturedValueCardDiagnosticHeight,
} from '../CapturedValueCard';
import { depthScale } from './lod';
import {
  NODE_VALUE_PREVIEW_FOOTER_HEIGHT,
  NODE_VALUE_PREVIEW_GAP,
  NODE_VALUE_PREVIEW_MAX,
  NODE_VALUE_PREVIEW_WIDTH,
} from './nodes/NodeOutputPreview';
import type { WorkflowEdge, WorkflowNode } from './types';

const elk = new ELK();

// Default node sizes by type — these are the *outer* (wrapper) sizes ELK
// uses for routing. The visible card sits NODE_BUFFER px inside on each
// side so arrow tips and selection rings have clearance.
const NODE_SIZES: Record<string, { w: number; h: number }> = {
  base: { h: 50, w: 180 },
  diamond: { h: 50, w: 180 },
  hexagon: { h: 50, w: 180 },
  llm: { h: 60, w: 200 },
};

/**
 * Visual buffer reserved on every side of a node — ELK routes edges to
 * the wrapper edge (this far outside the visible card), so arrow tips
 * and selection halos don't overlap the card border.
 */
export const NODE_BUFFER = 6;

/**
 * A container with more than this many direct children in a chain would
 * otherwise extend forever in one direction. Above it, we let ELK *wrap*
 * the layering (see {@link wrappingOptions}). Short graphs are untouched.
 */
const WRAP_CHILD_THRESHOLD = 6;

/**
 * Target width:height ratio for a wrapped container. Higher = wider rows
 * before wrapping; lower = squarer. ELK cuts the chain to approach this.
 */
const WRAP_ASPECT_RATIO = 2.3;

/**
 * ELK layered "wrapping": instead of one infinite row, cut a long chain
 * where a single edge crosses and stack the pieces into rows (a serpentine
 * layout), targeting {@link WRAP_ASPECT_RATIO}. Returns no options (wrapping
 * off) until a container exceeds {@link WRAP_CHILD_THRESHOLD} children, so
 * small/branching graphs keep their current layout.
 *
 * `dagre` (mermaid's engine) has no equivalent — this is ELK-specific.
 *
 * `wrap === false` disables this entirely (returns no options), so a long
 * chain extends unbounded in a single row/column — full horizontal or full
 * vertical, no aspect-ratio cap.
 */
function wrappingOptions(
  childCount: number,
  wrap: boolean,
): Record<string, string> {
  if (!wrap || childCount <= WRAP_CHILD_THRESHOLD) return {};
  return {
    'org.eclipse.elk.aspectRatio': String(WRAP_ASPECT_RATIO),
    // Tighten the corridor reserved for the wrap-around edges (default 10)
    // so stacked rows sit closer together.
    'org.eclipse.elk.layered.wrapping.additionalEdgeSpacing': '5',
    // Aspect-ratio-driven cutting honours elk.aspectRatio below.
    'org.eclipse.elk.layered.wrapping.cutting.strategy': 'ARD',
    'org.eclipse.elk.layered.wrapping.strategy': 'SINGLE_EDGE',
  };
}

function nodeSize(node: WorkflowNode): { w: number; h: number } {
  const base = NODE_SIZES[node.type ?? 'base'] ?? NODE_SIZES.base;
  const previewHeight = graphValuePreviewHeight(node);
  const visualSize = (() => {
    if (previewHeight > 0) {
      return {
        h: base.h + previewHeight + 14,
        w: Math.max(base.w, NODE_VALUE_PREVIEW_WIDTH),
      };
    }

    if (node.data.errorMessage) {
      return {
        h:
          base.h +
          capturedValueCardHeightForContent(
            0,
            capturedValueCardDiagnosticHeight(node.data.errorMessage),
          ) +
          14,
        w: Math.max(base.w, NODE_VALUE_PREVIEW_WIDTH),
      };
    }

    if (node.data.hasResult) {
      return {
        h:
          base.h +
          capturedValueCardHeightForContent(
            CAPTURED_VALUE_CARD_TEXT_HEIGHT,
            0,
          ) +
          14,
        w: Math.max(base.w, NODE_VALUE_PREVIEW_WIDTH),
      };
    }

    return base;
  })();
  // Deeper nodes lay out smaller (semantic-zoom hierarchy) — matches the
  // depth-scaled font/padding in the node components. Skip scaling for nodes
  // that render a result/image/error preview, whose content doesn't shrink, so
  // the box stays sized to it. Groups auto-size from children. Buffer is fixed.
  const hasPreview =
    previewHeight > 0 || !!node.data.errorMessage || !!node.data.hasResult;
  const s = hasPreview
    ? 1
    : depthScale(typeof node.data.depth === 'number' ? node.data.depth : 0);
  return {
    h: visualSize.h * s + 2 * NODE_BUFFER,
    w: visualSize.w * s + 2 * NODE_BUFFER,
  };
}

function capturedValueCardHeight(
  value: NonNullable<WorkflowNode['data']['valuePreviews']>[number],
): number {
  return capturedValueCardHeightForContent(
    capturedValueCardContentHeight(value),
    capturedValueCardDiagnosticHeight(value.diagnostic),
  );
}

function capturedValueCardHeightForContent(
  contentHeight: number,
  diagnosticHeight: number,
): number {
  return (
    CAPTURED_VALUE_CARD_HEADER_HEIGHT +
    CAPTURED_VALUE_CARD_PADDING_Y +
    contentHeight +
    (contentHeight > 0 ? 6 : 0) +
    diagnosticHeight
  );
}

function buildElkNodes(
  allNodes: WorkflowNode[],
  direction: 'horizontal' | 'vertical',
  wrap: boolean,
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
      const children = buildElkNodes(
        allNodes,
        direction,
        wrap,
        edgesByOwner,
        portsByNode,
        node.id,
      );
      const previewHeight = graphValuePreviewHeight(node);
      const topPadding = previewHeight > 0 ? 42 + previewHeight : 30;
      elkNode.layoutOptions = {
        'elk.algorithm': 'layered',
        'elk.direction': isHorizontal ? 'RIGHT' : 'DOWN',
        'elk.hierarchyHandling': 'INCLUDE_CHILDREN',
        'elk.padding': `[top=${topPadding},left=12,bottom=12,right=12]`,
        'spacing.nodeNode': '30',
        'spacing.nodeNodeBetweenLayers': '40',
        // Wrap long sequential chains (e.g. many //# steps) into rows.
        ...wrappingOptions(children.length, wrap),
      };
      elkNode.labels = [
        {
          height: 20,
          text: node.data.label,
          width: Math.max(80, previewHeight > 0 ? NODE_VALUE_PREVIEW_WIDTH : 0),
        },
      ];
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

export function graphValuePreviewHeight(node: WorkflowNode): number {
  const allValues = node.data.valuePreviews ?? [];
  const values = allValues.slice(0, NODE_VALUE_PREVIEW_MAX);
  if (values.length === 0) return 0;
  const footerHeight =
    allValues.length > NODE_VALUE_PREVIEW_MAX
      ? NODE_VALUE_PREVIEW_GAP + NODE_VALUE_PREVIEW_FOOTER_HEIGHT
      : 0;
  return (
    values.reduce(
      (height, value) => height + capturedValueCardHeight(value),
      0,
    ) +
    Math.max(0, values.length - 1) * NODE_VALUE_PREVIEW_GAP +
    footerHeight
  );
}

// ── Edge LCA (lowest common ancestor) ───────────────────────────────
// Placing each edge on the deepest group containing both endpoints
// gives ELK correct local context for layer ordering and spacing.

function getAncestorChain(
  nodeId: string,
  nodeById: Map<string, WorkflowNode>,
): string[] {
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
  wrap = true,
): Promise<{ nodes: WorkflowNode[]; edges: WorkflowEdge[] }> {
  if (nodes.length === 0) return { edges, nodes };

  const isHorizontal = direction === 'horizontal';
  const nodeIds = new Set(nodes.map((n) => n.id));
  const validEdges = edges.filter(
    (e) => nodeIds.has(e.source) && nodeIds.has(e.target),
  );
  const groupNodeIds = new Set(
    nodes.filter((n) => n.type === 'group').map((n) => n.id),
  );
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
  const edgePortIds = new Map<
    string,
    { sourcePort: string; targetPort: string }
  >();

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

  const rootChildren = buildElkNodes(
    nodes,
    direction,
    wrap,
    edgesByOwner,
    portsByNode,
  );

  const elkGraph: ElkNode = {
    children: rootChildren,
    edges: edgesByOwner.get('root') ?? [],
    id: 'root',
    layoutOptions: {
      'elk.algorithm': 'layered',
      'elk.direction': isHorizontal ? 'RIGHT' : 'DOWN',
      'elk.edgeRouting': 'ORTHOGONAL',
      'elk.hierarchyHandling': 'INCLUDE_CHILDREN',
      'spacing.edgeEdge': '15',
      'spacing.edgeNode': '20',
      'spacing.nodeNode': '30',
      'spacing.nodeNodeBetweenLayers': '50',
      // Wrap a long top-level chain (function with many sequential steps).
      ...wrappingOptions(rootChildren.length, wrap),
    },
  };

  const layouted = await elk.layout(elkGraph);

  // Extract node positions. ELK returns parent-relative coordinates,
  // which is exactly what ReactFlow expects for nodes inside groups.
  const positionMap = new Map<
    string,
    { x: number; y: number; w: number; h: number }
  >();

  function extractPositions(elkNodes: ElkNode[] | undefined) {
    if (!elkNodes) return;
    for (const en of elkNodes) {
      positionMap.set(en.id, {
        h: en.height ?? 0,
        w: en.width ?? 0,
        x: en.x ?? 0,
        y: en.y ?? 0,
      });
      if (en.children) extractPositions(en.children);
    }
  }

  extractPositions(layouted.children);

  // ── Compute absolute positions for handle selection ─────────────────
  const absPositions = new Map<
    string,
    { x: number; y: number; w: number; h: number }
  >();
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
      h: pos?.h ?? 0,
      w: pos?.w ?? 0,
      x: absX,
      y: absY,
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
  const elkEdgeOwners = new Map<
    string,
    { ownerId: string; sections: ElkSection[] }
  >();
  function collectElkEdges(elkNode: ElkNode, ownerId: string) {
    if (elkNode.edges) {
      for (const e of elkNode.edges as Array<
        ElkExtendedEdge & { sections?: ElkSection[] }
      >) {
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
      const ownerAbs =
        elkInfo.ownerId === 'root'
          ? { x: 0, y: 0 }
          : (absPositions.get(elkInfo.ownerId) ?? { x: 0, y: 0 });
      const ox = ownerAbs.x;
      const oy = ownerAbs.y;
      const points = elkInfo.sections.flatMap((sec) => {
        const start = {
          x: (sec.startPoint?.x ?? 0) + ox,
          y: (sec.startPoint?.y ?? 0) + oy,
        };
        const end = {
          x: (sec.endPoint?.x ?? 0) + ox,
          y: (sec.endPoint?.y ?? 0) + oy,
        };
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
        data: { ...edge.data, points: deduped },
        sourceHandle,
        targetHandle,
      };
    }

    // Fallback: no ELK route available — keep direction-heuristic handles
    // so getSmoothStepPath at least picks a reasonable side.
    if (!srcPos || !tgtPos) return { ...edge, sourceHandle, targetHandle };
    const dx = tgtPos.x + tgtPos.w / 2 - (srcPos.x + srcPos.w / 2);
    const dy = tgtPos.y + tgtPos.h / 2 - (srcPos.y + srcPos.h / 2);
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
        height: pos.h,
        width: pos.w,
        ...(isGroup
          ? {}
          : { boxSizing: 'border-box' as const, padding: NODE_BUFFER }),
      },
    };
  });

  return { edges: laidEdges, nodes: laidNodes };
}
