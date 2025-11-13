import ELK from 'elkjs/lib/elk.bundled.js';
import type { ElkExtendedEdge, ElkNode } from 'elkjs/lib/elk-api';

import type {
  ReactflowEdgeWithData,
  ReactflowNodeWithData,
} from '../../../../../mock-data/types';

import { getEdgeLayouted, getNodeLayouted, getNodeSize } from '../../metadata';
import type { LayoutAlgorithmProps } from '..';
import { findCommonAncestor, type TreeData } from './elk-find-common-ancestor';
import type { NodeLike, P } from './elk-geometry';

// ELK algorithm mappings
const ELK_ALGORITHMS = {
  elk: 'layered',
  'elk.layered': 'layered',
  'elk.stress': 'stress',
  'elk.force': 'force',
  'elk.mrtree': 'mrtree',
  'elk.sporeOverlap': 'sporeOverlap',
} as const;

export type ELKMermaidAlgorithm = keyof typeof ELK_ALGORITHMS;

interface ElkLayoutOptions extends LayoutAlgorithmProps {
  algorithm?: ELKMermaidAlgorithm;
}

// Extended ReactFlow node that can have children
type ReactFlowNodeExtended = ReactflowNodeWithData & {
  parentId?: string;
  isGroup?: boolean;
};

// Node database to track ELK nodes
interface NodeDbEntry extends NodeLike {
  id: string;
  isGroup?: boolean;
  parentId?: string;
  layoutOptions?: Record<string, any>;
  labels?: { text: string; width: number; height: number }[];
  width?: number;
  height?: number;
  padding?: number;
  labelData?: { width: number; height: number };
  effectiveWidth?: number; // For edge calculations (like Mermaid)
  // Offset tracking (for Mermaid-style edge routing)
  offset?: {
    posX: number;
    posY: number;
    x: number;
    y: number;
    depth: number;
    width: number;
    height: number;
  };
  // Center coordinates (calculated for edge intersection)
  x?: number; // center x
  y?: number; // center y
}

// Convert direction to ELK direction
const getElkDirection = (direction: 'horizontal' | 'vertical'): string => {
  return direction === 'horizontal' ? 'RIGHT' : 'DOWN';
};

// Build parent-child lookup database (same as Mermaid)
const buildParentLookupDb = (nodes: ReactFlowNodeExtended[]): TreeData => {
  const parentLookupDb: TreeData = {
    parentById: {},
    childrenById: {},
  };

  // Find all group/subgraph nodes
  const subgraphs = nodes.filter((node) => node.isGroup);

  for (const subgraph of subgraphs) {
    // Find children of this subgraph
    const children = nodes.filter((node) => node.parentId === subgraph.id);

    for (const child of children) {
      parentLookupDb.parentById[child.id] = subgraph.id;

      parentLookupDb.childrenById[subgraph.id] ??= [];
      parentLookupDb.childrenById[subgraph.id]!.push(child.id);
    }
  }

  return parentLookupDb;
};

// Set hierarchy handling policy recursively (from Mermaid's setIncludeChildrenPolicy)
const setIncludeChildrenPolicy = (
  nodeId: string,
  ancestorId: string,
  nodeDb: Record<string, NodeDbEntry>,
) => {
  const node = nodeDb[nodeId];

  if (!node) {
    return;
  }

  if (!node.layoutOptions) {
    node.layoutOptions = {};
  }

  node.layoutOptions['elk.hierarchyHandling'] = 'INCLUDE_CHILDREN';

  if (node.id !== ancestorId && node.parentId) {
    setIncludeChildrenPolicy(node.parentId, ancestorId, nodeDb);
  }
};

// Measure node labels separately (like Mermaid's labelHelper)
// This ensures we get accurate label dimensions independent of the node container
const measureNodeLabels = (
  nodes: ReactFlowNodeExtended[],
): Map<string, { width: number; height: number }> => {
  const labelMeasurements = new Map<
    string,
    { width: number; height: number }
  >();

  for (const node of nodes) {
    const labelText = node.data?.label || node.id;

    if (!labelText) {
      labelMeasurements.set(node.id, { width: 0, height: 0 });
      continue;
    }

    // Create temporary DOM element to measure label (like Mermaid's labelHelper)
    const temp = document.createElement('div');
    temp.style.cssText = `
      position: absolute;
      visibility: hidden;
      pointer-events: none;
      white-space: nowrap;
      font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
      font-size: 14px;
      padding: 4px 8px;
    `;
    temp.textContent = labelText;
    document.body.appendChild(temp);

    const { width, height } = temp.getBoundingClientRect();
    labelMeasurements.set(node.id, {
      width: Math.ceil(width),
      height: Math.ceil(height),
    });

    document.body.removeChild(temp);
  }

  return labelMeasurements;
};

// Build hierarchical ELK node structure (MERMAID WAY - recursive like addVertices)
// This mirrors Mermaid's addVertices function (render.ts:128-141)
const buildHierarchicalElkNodes = (
  allNodes: ReactFlowNodeExtended[],
  isHorizontal: boolean,
  parentLookupDb: TreeData,
  nodeDb: Record<string, NodeDbEntry>,
  labelMeasurements: Map<string, { width: number; height: number }>,
  parentId?: string,
): ElkNode[] => {
  // Filter nodes that belong to this parent level (like Mermaid line 134)
  const siblings = allNodes.filter((node) => node.parentId === parentId);


  const elkNodes: ElkNode[] = [];

  for (const node of siblings) {
    const { widthWithDefault, heightWithDefault } = getNodeSize(node);

    // Track in nodeDb
    nodeDb[node.id] = {
      id: node.id,
      isGroup: node.isGroup,
      parentId: node.parentId,
      width: widthWithDefault,
      height: heightWithDefault,
      padding: 10, // Default padding
    };

    const elkNode: ElkNode = {
      id: node.id,
      width: widthWithDefault,
      height: heightWithDefault,
    };

    // Build ports for LEAF nodes only (not groups)
    if (!node.isGroup && node.data.sourceHandles && node.data.targetHandles) {
      const sourcePorts = node.data.sourceHandles.map((id) => ({
        id,
        properties: {
          side: isHorizontal ? 'EAST' : 'SOUTH',
        },
      }));

      const targetPorts = node.data.targetHandles.map((id) => ({
        id,
        properties: {
          side: isHorizontal ? 'WEST' : 'NORTH',
        },
      }));

      elkNode.ports = [...targetPorts, ...sourcePorts];
    }

    // @ts-expect-error - ELK accepts properties
    elkNode.properties = {
      'org.eclipse.elk.portConstraints': 'FIXED_ORDER',
    };

    // For SUBGRAPHS/GROUPS - follow Mermaid's approach exactly
    if (node.isGroup && parentLookupDb.childrenById[node.id]) {
      // Get measured label dimensions (like Mermaid's labelHelper)
      const labelDims = labelMeasurements.get(node.id) || {
        width: 0,
        height: 0,
      };
      const nodeEntry = nodeDb[node.id]!;
      const padding = nodeEntry.padding || 10;

      // Store label data (like Mermaid lines 108-123)
      nodeEntry.labelData = labelDims;

      // Set labels for ELK (Mermaid lines 803-809)
      nodeEntry.labels = [
        {
          text: node.data?.label || node.id,
          width: labelDims.width,
          height: labelDims.height,
        },
      ];

      // Calculate effective width (Mermaid line 936: max(node.width, labelWidth + padding))
      const effectiveWidth = Math.max(
        widthWithDefault ?? 150,
        labelDims.width + 2 * padding,
      );
      nodeEntry.effectiveWidth = effectiveWidth;


      // Set initial width with padding (line 810)
      nodeEntry.width = effectiveWidth;

      // Set layout options for subgraphs (lines 812-825)
      nodeEntry.layoutOptions = {
        'spacing.baseValue': 30,
        'nodeLabels.placement': '[H_CENTER, V_TOP, INSIDE]',
      };

      // CRITICAL: Delete dimensions for subgraphs (lines 826-829)
      // ELK will calculate them based on children
      delete nodeEntry.width;
      delete nodeEntry.height;

      // Don't set width/height in elkNode for groups
      delete elkNode.width;
      delete elkNode.height;

      // Set labels in ELK node
      elkNode.labels = nodeEntry.labels;

      // Apply layout options
      elkNode.layoutOptions = nodeEntry.layoutOptions;

      // CRITICAL: RECURSIVELY build children (like Mermaid's recursive addVertices call at line 106)
      const children = buildHierarchicalElkNodes(
        allNodes,
        isHorizontal,
        parentLookupDb,
        nodeDb,
        labelMeasurements,
        node.id, // ← Pass this node's ID as the new parent
      );

      if (children.length > 0) {
        elkNode.children = children;
      }
    }

    elkNodes.push(elkNode);
  }

  return elkNodes;
};

// Build ELK edge structure
const buildElkEdge = (
  edge: ReactflowEdgeWithData,
  sourceNode: ReactFlowNodeExtended | undefined,
  targetNode: ReactFlowNodeExtended | undefined,
): ElkExtendedEdge => {
  // For group nodes, use node ID directly; for leaf nodes, use port handles
  const sourceId = sourceNode?.isGroup
    ? edge.source
    : edge.sourceHandle || edge.source;
  const targetId = targetNode?.isGroup
    ? edge.target
    : edge.targetHandle || edge.target;

  return {
    id: edge.id,
    sources: [sourceId],
    targets: [targetId],
  };
};

// Get ELK layout options (matching Mermaid's configuration)
const getElkLayoutOptions = (
  algorithm: string,
  direction: string,
  _spacing: { x: number; y: number },
): Record<string, string> => {
  const isHorizontal = direction === 'RIGHT';

  return {
    // Core options (from Mermaid lines 718-729)
    'elk.hierarchyHandling': 'INCLUDE_CHILDREN',
    'elk.algorithm': algorithm,
    'elk.direction': direction,
    'spacing.baseValue': '40',

    // Layered algorithm options (from Mermaid lines 728-750)
    'elk.layered.unnecessaryBendpoints': 'true',
    'elk.layered.wrapping.multiEdge.improveCuts': 'true',
    'elk.layered.wrapping.multiEdge.improveWrappedEdges': 'true',
    'elk.layered.edgeRouting.selfLoopDistribution': 'EQUALLY',
    'elk.layered.mergeHierarchyEdges': 'true',

    // Control node ordering in horizontal layouts - nodes should be ordered top to bottom
    ...(isHorizontal ? {
      'elk.layered.considerModelOrder.strategy': 'NODES_AND_EDGES',
      'elk.layered.cycleBreaking.strategy': 'DEPTH_FIRST',
      // 'elk.layered.nodePlacement.strategy': 'SIMPLE',
    } : {}),

    // Additional spacing
    'spacing.nodeNode': '40',
    'spacing.nodeNodeBetweenLayers': '50',
    'spacing.edgeNode': '20',
    'spacing.edgeEdge': '10',
  };
};

// ============================================================================
// EDGE ROUTING HELPERS
// ============================================================================

// Calculate offset for nested subgraphs (Mermaid lines 255-263)
const calcOffset = (
  src: string,
  dest: string,
  parentLookupDb: TreeData,
  nodeDb: Record<string, NodeDbEntry>,
): { x: number; y: number } => {
  const ancestor = findCommonAncestor(src, dest, parentLookupDb);
  if (ancestor === undefined || ancestor === 'root') {
    return { x: 0, y: 0 };
  }
  const ancestorOffset = nodeDb[ancestor]?.offset;
  if (!ancestorOffset) {
    return { x: 0, y: 0 };
  }
  return {
    x: ancestorOffset.posX,
    y: ancestorOffset.posY,
  };
};

// Main layout function using ELK with MERMAID'S EXACT APPROACH
export const layoutELKMermaid = async (
  props: ElkLayoutOptions,
): Promise<
  { nodes: ReactflowNodeWithData[]; edges: ReactflowEdgeWithData[] } | undefined
> => {
  const {
    nodes,
    edges,
    direction,
    visibility,
    spacing,
    algorithm = 'elk.layered',
  } = props;

  const isHorizontal = direction === 'horizontal';
  const elkAlgorithm = ELK_ALGORITHMS[algorithm];
  const elkDirection = getElkDirection(direction);

  // Cast nodes to extended type
  const extendedNodes = nodes as ReactFlowNodeExtended[];

  // Build parent-child lookup database (Mermaid's addSubGraphs)
  const parentLookupDb = buildParentLookupDb(extendedNodes);

  // Measure all node labels separately (like Mermaid's labelHelper)
  const labelMeasurements = measureNodeLabels(extendedNodes);

  // Initialize ELK
  const elk = new ELK();

  // Build HIERARCHICAL ELK node structure (Mermaid way - recursive with proper nesting)
  const nodeDb: Record<string, NodeDbEntry> = {};
  const elkNodes = buildHierarchicalElkNodes(
    extendedNodes,
    isHorizontal,
    parentLookupDb,
    nodeDb,
    labelMeasurements,
  );


  // Build ELK edges
  const elkEdges = edges.map((edge) => {
    const sourceNode = extendedNodes.find((n) => n.id === edge.source);
    const targetNode = extendedNodes.find((n) => n.id === edge.target);
    return buildElkEdge(edge, sourceNode, targetNode);
  });

  // Process edges that cross subgraph boundaries (Mermaid lines 841-847)
  for (const edge of edges) {
    const source = edge.source;
    const target = edge.target;

    const sourceNodeEntry = nodeDb[source];
    const targetNodeEntry = nodeDb[target];

    if (
      sourceNodeEntry &&
      targetNodeEntry &&
      sourceNodeEntry.parentId !== targetNodeEntry.parentId
    ) {
      // Edge crosses subgraph boundary - find common ancestor
      const ancestorId = findCommonAncestor(source, target, parentLookupDb);


      // Set hierarchy policy recursively (Mermaid's approach)
      setIncludeChildrenPolicy(source, ancestorId, nodeDb);
      setIncludeChildrenPolicy(target, ancestorId, nodeDb);
    }
  }

  // Apply layout options from nodeDb back to elkNodes
  for (const elkNode of elkNodes) {
    const nodeEntry = nodeDb[elkNode.id];
    if (nodeEntry?.layoutOptions) {
      elkNode.layoutOptions = {
        ...elkNode.layoutOptions,
        ...nodeEntry.layoutOptions,
      };
    }
  }

  // Build ELK graph (hierarchical structure like Mermaid - children nested inside parents)
  const elkGraph = {
    id: 'root',
    children: elkNodes, // Only root-level nodes; their children are in elkNode.children
    edges: elkEdges,
    layoutOptions: getElkLayoutOptions(elkAlgorithm, elkDirection, spacing),
  };

  // console.log(
  //   '🎯 ELK Graph (hierarchical structure):',
  //   JSON.stringify(elkGraph, null, 2),
  // );

  // Run ELK layout
  let layoutedGraph: ElkNode | undefined;
  try {
    layoutedGraph = await elk.layout(elkGraph);
    console.log('✅ ELK layout completed!');
    // console.log('🎯 Layouted result:', JSON.stringify(layoutedGraph, null, 2));
  } catch (e) {
    console.error('❌ ELK layout failed:', e);
    return undefined;
  }

  if (!layoutedGraph?.children) {
    console.error('❌ No children in layouted graph');
    return undefined;
  }

  // Flatten hierarchy to get absolute positions AND extract calculated dimensions (handles nested groups)
  const nodePositionMap = new Map<string, { x: number; y: number }>();
  const nodeDimensionMap = new Map<string, { width: number; height: number }>();

  function extractNodeData(
    elkNode: ElkNode,
    parentAbsX: number,
    parentAbsY: number,
    depth: number = 0,
    isRoot: boolean = false,
  ) {
    // ELK returns positions relative to parent
    const relativeX = elkNode.x || 0;
    const relativeY = elkNode.y || 0;

    // Calculate absolute position (for root nodes or for debugging)
    const absX = parentAbsX + relativeX;
    const absY = parentAbsY + relativeY;

    // For React Flow:
    // - Root nodes: use absolute positions
    // - Child nodes: use positions relative to their parent (which is what ELK gives us)
    if (isRoot) {
      nodePositionMap.set(elkNode.id, { x: absX, y: absY });
    } else {
      // Child nodes: position relative to parent (ELK already provides this)
      nodePositionMap.set(elkNode.id, { x: relativeX, y: relativeY });
    }

    // Extract calculated dimensions (CRITICAL for groups - ELK calculates these)
    if (elkNode.width !== undefined && elkNode.height !== undefined) {
      nodeDimensionMap.set(elkNode.id, {
        width: elkNode.width,
        height: elkNode.height,
      });

      // CRITICAL: Store offset in nodeDb for edge routing (like Mermaid)
      const nodeEntry = nodeDb[elkNode.id];
      if (nodeEntry) {
        nodeEntry.offset = {
          posX: absX,
          posY: absY,
          x: absX,
          y: absY,
          depth,
          width: elkNode.width,
          height: elkNode.height,
        };
        // Update width and height from ELK calculation
        nodeEntry.width = elkNode.width;
        nodeEntry.height = elkNode.height;
      }
    }

    // Recursively process children (children are NOT root, depth increases)
    if (elkNode.children) {
      for (const child of elkNode.children) {
        extractNodeData(child, absX, absY, depth + 1, false);
      }
    }
  }

  for (const child of layoutedGraph.children) {
    extractNodeData(child, 0, 0, 0, true); // Root-level nodes are "root", depth 0
  }



  // Apply positions AND dimensions to ReactFlow nodes
  const layoutedNodes = nodes.map((node) => {
    const position = nodePositionMap.get(node.id);
    const dimensions = nodeDimensionMap.get(node.id);

    if (!position) {
      console.warn(`No position found for node ${node.id}`);
      return getNodeLayouted({
        node,
        position: { x: 0, y: 0 },
        direction,
        visibility,
      });
    }

    // Apply ELK-calculated dimensions (critical for groups)
    // For GROUPS: Always apply ELK dimensions (both passes) since ELK calculates them based on children
    // For REGULAR nodes: Only apply on visible pass to allow natural measurement on first pass
    const shouldApplyDimensions = dimensions && (node.data.isGroup || visibility === 'visible');
    const nodeWithDimensions = shouldApplyDimensions
      ? {
        ...node,
        // Set measured dimensions so metadata.ts can use them
        measured: {
          width: dimensions.width,
          height: dimensions.height,
        },
        style: {
          ...node.style,
          width: dimensions.width,
          height: dimensions.height,
        },
      }
      : node;

    return getNodeLayouted({
      node: nodeWithDimensions,
      position,
      direction,
      visibility,
    });
  });

  // ============================================================================
  // COMPLETE EDGE PROCESSING PIPELINE - Mermaid Implementation (lines 885-1089)
  // ============================================================================

  // Extract edge routing data from ELK
  const elkEdgeMap = new Map<string, any>();

  function collectElkEdges(elkNode: ElkNode) {
    if (elkNode.edges) {
      for (const edge of elkNode.edges) {
        elkEdgeMap.set(edge.id, edge);
      }
    }
    if (elkNode.children) {
      for (const child of elkNode.children) {
        collectElkEdges(child);
      }
    }
  }

  collectElkEdges(layoutedGraph);

  // Apply edge routing from ELK data (COMPLETE MERMAID PIPELINE)
  const layoutedEdges = edges.map((edge) => {
    const elkEdge = elkEdgeMap.get(edge.id);

    if (elkEdge?.sections && elkEdge.sections.length > 0) {
      const section = elkEdge.sections[0];
      const startPoint = section.startPoint;
      const endPoint = section.endPoint;
      const bendPoints = section.bendPoints || [];

      const startNode = nodeDb[edge.source];
      const endNode = nodeDb[edge.target];

      if (!startNode || !endNode) {
        console.warn(`Missing node data for edge ${edge.id}`);
        return getEdgeLayouted({ edge, visibility });
      }

      // STEP 1: Calculate offset for nested subgraphs (Mermaid lines 904)
      const offset = calcOffset(
        edge.source,
        edge.target,
        parentLookupDb,
        nodeDb,
      );

      // STEP 2: Build points with offset applied (Mermaid lines 922-929)
      const segPoints = bendPoints.map((segment: { x: number; y: number }) => ({
        x: segment.x + offset.x,
        y: segment.y + offset.y,
      }));

      let points: P[] = [
        { x: startPoint.x + offset.x, y: startPoint.y + offset.y },
        ...segPoints,
        { x: endPoint.x + offset.x, y: endPoint.y + offset.y },
      ];

      // STEP 3: Calculate node centers (Mermaid lines 969-972)
      startNode.x = (startNode.offset?.posX ?? 0) + (startNode.width ?? 0) / 2;
      startNode.y = (startNode.offset?.posY ?? 0) + (startNode.height ?? 0) / 2;
      endNode.x = (endNode.offset?.posX ?? 0) + (endNode.width ?? 0) / 2;
      endNode.y = (endNode.offset?.posY ?? 0) + (endNode.height ?? 0) / 2;

      // STEP 4: Add center points for intersection calculations
      // NOTE: For React Flow, ELK already provides edge start/end points at handle positions.
      // Adding center points would create unnecessary bends from center→handle.
      // We only need center points for the cutter2 intersection calculations,
      // but we can use them without adding them to the path.
      // const centerPoints = {
      //   start: { x: startNode.x, y: startNode.y },
      //   end: { x: endNode.x, y: endNode.y },
      // };

      // Don't add center points to the path - keep ELK's handle positions
      // (Mermaid needs center points because it draws directly to SVG,
      //  but React Flow already handles this via the handle positioning)

      // STEP 5: Apply edge clipping (if needed)
      // NOTE: For React Flow with ELK, we skip cutter2 because:
      // 1. ELK already routes edges to handle positions (not node centers)
      // 2. Adding center points and then clipping creates unnecessary bends
      // 3. The edge routing from ELK is already correct
      //
      // We keep the points as-is from ELK (with offset applied)

      // STEP 6: Validate and clean (Mermaid lines 1043-1053)
      const hasNaN = (pts: P[]) =>
        pts?.some((p) => !Number.isFinite(p?.x) || !Number.isFinite(p?.y));

      if (!Array.isArray(points) || points.length < 2 || hasNaN(points)) {
        console.warn(
          `⚠️ Invalid points from edge processing for ${edge.id}`,
          points,
        );
        // Keep the original ELK points, just filter out invalid values
        const cleaned = points.filter(
          (p) => Number.isFinite(p?.x) && Number.isFinite(p?.y),
        );
        points = cleaned.length >= 2 ? cleaned : points;
      }

      // STEP 7: Deduplicate consecutive points (Mermaid lines 1056-1071)
      const deduped = points.filter((p, i, arr) => {
        if (i === 0) return true;
        const prev = arr[i - 1]!;
        return Math.abs(p.x - prev.x) > 1e-6 || Math.abs(p.y - prev.y) > 1e-6;
      });

      if (deduped.length !== points.length) {
        console.debug(
          `🎯 Edge ${edge.id}: Removed ${points.length - deduped.length} duplicate points`,
        );
      }
      points = deduped;



      // STEP 8: Convert to React Flow format
      const edgePoints = points.map((p, i) => ({
        id: `${i}`,
        x: p.x,
        y: p.y,
      }));

      // Calculate label position (midpoint of path)
      const labelPosition =
        edgePoints.length >= 2
          ? (() => {
            const first = edgePoints[0]!;
            const last = edgePoints[edgePoints.length - 1]!;
            return {
              x: (first.x + last.x) / 2,
              y: (first.y + last.y) / 2,
            };
          })()
          : { x: 0, y: 0 };

      const edgeWithRouting: ReactflowEdgeWithData = {
        ...edge,
        data: {
          ...edge.data,
          sourcePort: edge.data!.sourcePort,
          targetPort: edge.data!.targetPort,
          layout: {
            points: edgePoints,
            path: '', // Will be calculated by layoutEdge in BaseEdge component
            labelPosition,
            inputPoints: edgePoints,
          },
        },
      };

      return getEdgeLayouted({ edge: edgeWithRouting, visibility });
    }

    // No routing data from ELK, use default
    return getEdgeLayouted({ edge, visibility });
  });



  return {
    nodes: layoutedNodes,
    edges: layoutedEdges,
  };
};

// Export algorithm variants
export const kElkMermaidAlgorithms: Record<
  string,
  (props: LayoutAlgorithmProps) => ReturnType<typeof layoutELKMermaid>
> = {
  elk: (props) => layoutELKMermaid({ ...props, algorithm: 'elk' }),
  'elk.layered': (props) =>
    layoutELKMermaid({ ...props, algorithm: 'elk.layered' }),
  'elk.stress': (props) =>
    layoutELKMermaid({ ...props, algorithm: 'elk.stress' }),
  'elk.force': (props) =>
    layoutELKMermaid({ ...props, algorithm: 'elk.force' }),
  'elk.mrtree': (props) =>
    layoutELKMermaid({ ...props, algorithm: 'elk.mrtree' }),
  'elk.sporeOverlap': (props) =>
    layoutELKMermaid({ ...props, algorithm: 'elk.sporeOverlap' }),
};
