import type { ControlFlowGraph, CfgNodeType } from '../worker-protocol';
import type {
  GraphNode,
  GraphEdge,
  GraphNodeType,
  WorkflowNode,
  WorkflowEdge,
} from './types';
import { getMarkerColors } from './edges/Marker';

// Stage 1: ControlFlowGraph JSON -> GraphNode[] / GraphEdge[]
export function cfgToGraphNodes(cfg: ControlFlowGraph): {
  nodes: GraphNode[];
  edges: GraphEdge[];
} {
  const nodes: GraphNode[] = [];
  const edges: GraphEdge[] = [];

  for (const [idStr, node] of Object.entries(cfg.nodes)) {
    nodes.push({
      id: idStr,
      label: node.label,
      type: cfgNodeTypeToGraphType(node.nodeType),
      parent: node.parentNodeId != null ? String(node.parentNodeId) : null,
      metadata: {
        logFilterKey: node.logFilterKey,
        sourceExpr: node.sourceExpr,
        sourceSpan: node.sourceSpan,
        isContainer: node.isContainer,
        llmClient: node.llmClient,
        calleeName: node.calleeName,
      },
    });
  }

  for (const edgeList of Object.values(cfg.edgesBySrc)) {
    for (const edge of edgeList) {
      edges.push({
        source: String(edge.src),
        target: String(edge.dst),
        label: edge.label,
      });
    }
  }

  return { nodes, edges };
}

function cfgNodeTypeToGraphType(nt: CfgNodeType): GraphNodeType {
  // Preserve semantic type — whether a node is a group is determined
  // separately in graphToReactflow via isContainer.
  switch (nt) {
    case 'functionRoot':
      return 'function';
    case 'llmFunction':
      return 'llm_function';
    case 'headerContextEnter':
      return 'header';
    case 'branchGroup':
      return 'conditional';
    case 'branchArm':
      return 'scope';
    case 'loop':
      return 'loop';
    case 'otherScope':
      return 'scope';
    case 'return':
      return 'return';
  }
}

// Stage 2+3: GraphNode[] / GraphEdge[] -> ReactFlow nodes/edges
export function graphToReactflow(
  graphNodes: GraphNode[],
  graphEdges: GraphEdge[],
): { nodes: WorkflowNode[]; edges: WorkflowEdge[] } {
  // Build a lookup for quick access
  const nodeMap = new Map(graphNodes.map((n) => [n.id, n]));

  // ── Nodes ──────────────────────────────────────────────────────────
  const nodes: WorkflowNode[] = graphNodes.map((gn) => {
    const isGroup = gn.metadata.isContainer;

    // Determine parentId: only set if the parent is itself a container.
    // After reparenting, a node's parentNodeId may point to a diamond
    // (BranchGroup) which is NOT a ReactFlow group container.
    let parentId: string | undefined;
    if (gn.parent) {
      const parentNode = nodeMap.get(gn.parent);
      if (parentNode?.metadata.isContainer) {
        parentId = gn.parent;
      }
    }

    return {
      id: gn.id,
      type: isGroup ? 'group' : graphTypeToReactflowType(gn.type),
      position: { x: 0, y: 0 },
      data: {
        label: gn.label,
        graphNodeType: gn.type,
        executionState: 'not-started' as const,
        selected: false,
        logFilterKey: gn.metadata.logFilterKey,
        llmClient: gn.metadata.llmClient,
      },
      ...(parentId ? { parentId } : {}),
    } as WorkflowNode;
  });

  // ── Edges ──────────────────────────────────────────────────────────
  // No synthetic edges — Rust provides fan-out edges directly.
  const edges: WorkflowEdge[] = graphEdges.map((e, i) => ({
    id: `e-${e.source}-${e.target}-${i}`,
    source: e.source,
    target: e.target,
    type: 'base',
    data: { label: e.label },
  }));

  // Build source→edges index for sibling count determination
  const edgesBySource = new Map<string, WorkflowEdge[]>();
  for (const edge of edges) {
    const list = edgesBySource.get(edge.source) ?? [];
    list.push(edge);
    edgesBySource.set(edge.source, list);
  }

  // Resolve theme-aware marker colors once for the whole graph rather than
  // re-probing the DOM per edge.
  const colors = getMarkerColors();

  // Apply color data to fan-out edges
  for (const [, siblings] of edgesBySource) {
    const siblingCount = siblings.length;
    siblings.forEach((edge, idx) => {
      const rawLabel = edge.data?.label;
      edge.data = {
        ...edge.data,
        color: computeEdgeColor(colors, rawLabel, siblingCount, idx),
      };
    });
  }

  return { nodes, edges };
}

function computeEdgeColor(
  colors: ReturnType<typeof getMarkerColors>,
  label: string | undefined,
  siblingCount: number,
  siblingIndex: number,
): string {
  // No label = sequential edge → base color
  if (!label) return colors.base;

  // 2-arm branch: green for true/first, red for else/default
  if (siblingCount === 2) {
    return /^(else|default)$/i.test(label) ? colors.no : colors.yes;
  }

  // 3+ arm branch: rotating colorful palette
  if (siblingCount > 2) {
    return colors.colors[siblingIndex % colors.colors.length]!;
  }

  return colors.base;
}

function graphTypeToReactflowType(gt: GraphNodeType): string {
  switch (gt) {
    case 'function':
      return 'base';
    case 'llm_function':
      return 'llm';
    case 'conditional':
      return 'diamond';
    case 'loop':
      return 'hexagon';
    case 'scope':
      return 'base';
    case 'header':
      return 'base';
    case 'return':
      return 'base';
  }
}
