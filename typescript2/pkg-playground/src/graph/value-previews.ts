import type { WorkflowEdge, WorkflowNode } from './types';

export const GROUP_VALUE_PREVIEW_KIND = 'group-value-preview';

const GROUP_VALUE_PREVIEW_NODE_PREFIX = '__baml_group_value_preview__:';

export function groupValuePreviewNodeId(groupNodeId: string): string {
  return `${GROUP_VALUE_PREVIEW_NODE_PREFIX}${groupNodeId}`;
}

export function isGroupValuePreviewNode(
  node: Pick<WorkflowNode, 'data'>,
): boolean {
  return node.data.syntheticKind === GROUP_VALUE_PREVIEW_KIND;
}

export function groupValuePreviewSourceNodeId(
  node: Pick<WorkflowNode, 'data'>,
): string | null {
  return isGroupValuePreviewNode(node) && typeof node.data.sourceNodeId === 'string'
    ? node.data.sourceNodeId
    : null;
}

export function liftGroupValuePreviews(
  nodes: WorkflowNode[],
  edges: WorkflowEdge[],
): { nodes: WorkflowNode[]; edges: WorkflowEdge[] } {
  const nodeIds = new Set(nodes.map((node) => node.id));
  const directChildrenByParent = new Map<string, WorkflowNode[]>();

  for (const node of nodes) {
    if (!node.parentId) continue;
    const children = directChildrenByParent.get(node.parentId) ?? [];
    children.push(node);
    directChildrenByParent.set(node.parentId, children);
  }

  const liftedGroupIds = new Set<string>();
  const previewNodes: WorkflowNode[] = [];
  const previewEdges: WorkflowEdge[] = [];

  for (const node of nodes) {
    if (node.type !== 'group' || isGroupValuePreviewNode(node)) continue;
    const valuePreviews = node.data.valuePreviews ?? [];
    if (valuePreviews.length === 0) continue;

    const previewNodeId = groupValuePreviewNodeId(node.id);
    if (nodeIds.has(previewNodeId)) continue;

    liftedGroupIds.add(node.id);
    const previewNode: WorkflowNode = {
      id: previewNodeId,
      type: 'base',
      position: { x: 0, y: 0 },
      parentId: node.id,
      data: {
        ...node.data,
        label: node.data.label,
        selected: false,
        valuePreviews,
        syntheticKind: GROUP_VALUE_PREVIEW_KIND,
        sourceNodeId: node.id,
      },
    };
    previewNodes.push(previewNode);

    for (const child of groupStartChildren(
      node.id,
      directChildrenByParent,
      edges,
    )) {
      previewEdges.push({
        id: `${previewNodeId}->${child.id}`,
        source: previewNodeId,
        target: child.id,
        type: 'base',
        data: {},
      });
    }
  }

  if (previewNodes.length === 0) {
    return { nodes, edges };
  }

  return {
    nodes: [
      ...nodes.map((node) =>
        liftedGroupIds.has(node.id)
          ? {
              ...node,
              data: {
                ...node.data,
                valuePreviews: [],
                groupValuePreviewsLifted: true,
              },
            }
          : node,
      ),
      ...previewNodes,
    ],
    edges: [...edges, ...previewEdges],
  };
}

function groupStartChildren(
  groupNodeId: string,
  directChildrenByParent: Map<string, WorkflowNode[]>,
  edges: WorkflowEdge[],
): WorkflowNode[] {
  const children = directChildrenByParent.get(groupNodeId) ?? [];
  if (children.length === 0) return [];

  const childIds = new Set(children.map((child) => child.id));
  const childrenWithInternalIncomingEdges = new Set<string>();
  for (const edge of edges) {
    if (childIds.has(edge.source) && childIds.has(edge.target)) {
      childrenWithInternalIncomingEdges.add(edge.target);
    }
  }

  const starts = children.filter(
    (child) => !childrenWithInternalIncomingEdges.has(child.id),
  );
  return starts.length > 0 ? starts : children;
}
