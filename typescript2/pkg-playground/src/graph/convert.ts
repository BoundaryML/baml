import type { ControlFlowGraph, CfgNodeType } from '../worker-protocol';
import type { GraphNode, GraphEdge, GraphNodeType, WorkflowNode, WorkflowEdge } from './types';

// Stage 1: ControlFlowGraph JSON -> GraphNode[] / GraphEdge[]
export function cfgToGraphNodes(cfg: ControlFlowGraph): { nodes: GraphNode[]; edges: GraphEdge[] } {
  const nodes: GraphNode[] = [];
  const edges: GraphEdge[] = [];

  for (const [, node] of Object.entries(cfg.nodes)) {
    nodes.push({
      id: String(node.id),
      label: node.label,
      type: cfgNodeTypeToGraphType(node.nodeType),
      parent: node.parentNodeId != null ? String(node.parentNodeId) : null,
      metadata: {
        logFilterKey: node.logFilterKey,
        sourceExpr: node.sourceExpr,
      },
    });
  }

  // Collect ALL edges from the CFG
  for (const [, edgeList] of Object.entries(cfg.edgesBySrc)) {
    for (const edge of edgeList) {
      edges.push({
        source: String(edge.src),
        target: String(edge.dst),
      });
    }
  }

  return { nodes, edges };
}

function cfgNodeTypeToGraphType(nt: CfgNodeType): GraphNodeType {
  // Preserve semantic type — whether a node is a group is determined
  // separately in graphToReactflow via hasChildren.
  switch (nt) {
    case 'functionRoot': return 'function';
    case 'headerContextEnter': return 'header';
    case 'branchGroup': return 'conditional';
    case 'branchArm': return 'scope';
    case 'loop': return 'loop';
    case 'otherScope': return 'scope';
  }
}

// Stage 2+3: GraphNode[] / GraphEdge[] -> ReactFlow nodes/edges
export function graphToReactflow(
  graphNodes: GraphNode[],
  graphEdges: GraphEdge[],
): { nodes: WorkflowNode[]; edges: WorkflowEdge[] } {
  // Reparent branchArm children: move them from branchGroup to branchGroup's parent.
  // This makes branchGroup a diamond node (not a nested group) with edges fanning out
  // to sibling branch arm groups — producing a proper flowchart layout.
  const conditionalIds = new Set(graphNodes.filter(gn => gn.type === 'conditional').map(gn => gn.id));
  const effectiveParent = new Map<string, string | null>();
  for (const gn of graphNodes) {
    if (gn.parent != null && conditionalIds.has(gn.parent)) {
      // This node's parent is a branchGroup — reparent to the branchGroup's parent
      const branchGroup = graphNodes.find(n => n.id === gn.parent)!;
      effectiveParent.set(gn.id, branchGroup.parent);
    } else {
      effectiveParent.set(gn.id, gn.parent);
    }
  }

  // Determine groups from effective parent relationships (excluding conditionals).
  const groupIds = new Set<string>();
  for (const gn of graphNodes) {
    const ep = effectiveParent.get(gn.id) ?? null;
    if (ep != null) {
      groupIds.add(ep);
    }
  }
  // BranchGroup (conditional) nodes should be diamonds, never groups.
  for (const id of conditionalIds) {
    groupIds.delete(id);
  }

  const nodes: WorkflowNode[] = graphNodes.map((gn) => {
    const isGroup = groupIds.has(gn.id);
    const rfType = isGroup ? 'group' : graphTypeToReactflowType(gn.type);

    // Use effective parent for parentId
    const ep = effectiveParent.get(gn.id) ?? null;
    const parentId = ep != null && groupIds.has(ep) ? ep : undefined;

    return {
      id: gn.id,
      type: rfType,
      position: { x: 0, y: 0 },
      data: {
        label: gn.label,
        graphNodeType: gn.type,
        executionState: 'not-started' as const,
        selected: false,
        logFilterKey: gn.metadata.logFilterKey,
      },
      ...(parentId != null ? { parentId } : {}),
    };
  });

  const edges: WorkflowEdge[] = graphEdges.map((ge, i) => ({
    id: `e-${ge.source}-${ge.target}-${i}`,
    source: ge.source,
    target: ge.target,
    type: 'base',
  }));

  // Synthesize fan-out edges: branchGroup diamond → each branchArm.
  for (const gn of graphNodes) {
    if (gn.type === 'conditional') {
      const children = graphNodes.filter(c => c.parent === gn.id);
      for (const child of children) {
        edges.push({
          id: `e-synth-${gn.id}-${child.id}`,
          source: gn.id,
          target: child.id,
          type: 'base',
        });
      }
    }
  }

  return { nodes, edges };
}

function graphTypeToReactflowType(gt: GraphNodeType): string {
  switch (gt) {
    case 'function': return 'base';
    case 'llm_function': return 'llm';
    case 'conditional': return 'diamond';
    case 'loop': return 'hexagon';
    case 'scope': return 'base';
    case 'header': return 'base';
  }
}
