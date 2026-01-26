import type { Edge, Node, XYPosition } from '@xyflow/react';

import type { ControlPoint } from '../features/graph/layout/edge/point';

export interface Graph {
  nodes: GraphNode[];
  edges: GraphEdge[];
}

export type NodeShape =
  | 'rect' // Default rectangle
  | 'diamond' // Decision/if node (Mermaid: {text})
  | 'hexagon' // Loop/iteration node (Mermaid: {{text}})
  | 'stadium' // Rounded rectangle (Mermaid: ([text]))
  | 'circle' // Circle node
  | 'cylinder' // Database/storage node
  | 'round'; // Rounded corners

export interface GraphNode {
  id: string; // stable unique id
  label: string; // human-readable display text
  kind: 'item' | 'group'; // single node vs container/subgraph
  shape?: NodeShape; // visual shape (defaults to 'rect' for items)
  parent?: string; // id of parent group (if any)
}

export interface GraphEdge {
  id: string;
  from: string; // source node id
  to: string; // target node id
  style: 'solid' | 'dashed'; // relationship type (flow vs reference)
}

interface WorkflowNode extends Record<string, unknown> {
  id: string;
  type: 'base' | 'start' | 'group' | 'diamond' | 'hexagon' | 'llm';
  label?: string;
  shape?: NodeShape;
  isGroup?: boolean;
  parentId?: string;
}

interface WorkflowEdge {
  id: string;
  source: string;
  target: string;
  sourceHandle: string;
  targetHandle: string;
}

export interface Workflow {
  nodes: WorkflowNode[];
  edges: WorkflowEdge[];
}

export type ReactflowNodeData = WorkflowNode & {
  /**
   * The output ports of the current node.
   *
   * Format of Port ID: `nodeID#source#idx`
   */
  sourceHandles: string[];
  /**
   * The input port of the current node (only one).
   *
   * Format of Port ID: `nodeID#target#idx`
   */
  targetHandles: string[];
  /**
   * Layout direction for the graph
   */
  direction?: 'vertical' | 'horizontal';
  /**
   * Execution state of the node
   */
  executionState?: 'not-started' | 'pending' | 'running' | 'success' | 'error' | 'skipped' | 'cached';
  /**
   * Whether this is the active execution (true) or a past/completed execution (false)
   */
  isExecutionActive?: boolean;
  /**
   * LLM client name (for llm_function nodes)
   */
  llmClient?: string;
  /**
   * Node execution outputs
   */
  outputs?: Record<string, unknown>;
  /**
   * Node execution error
   */
  error?: Error | string;
};

export interface ReactflowEdgePort {
  /**
   * Total number of edges in this direction (source or target).
   */
  edges: number;
  /**
   * Number of ports
   */
  portCount: number;
  /**
   * Port's index.
   */
  portIndex: number;
  /**
   * Total number of Edges under the current port.
   */
  edgeCount: number;
  /**
   * Index of the Edge under the current port.
   */
  edgeIndex: number;
}

export interface EdgeLayout {
  /**
   * SVG path for edge rendering
   */
  path: string;
  /**
   * Control points on the edge.
   */
  points: ControlPoint[];
  labelPosition: XYPosition;
  /**
   * Current layout dependent variables (re-layout when changed).
   */
  deps?: any;
  /**
   * Potential control points on the edge, for debugging purposes only.
   */
  inputPoints: ControlPoint[];
}

export interface ReactflowEdgeData extends Record<string, unknown> {
  /**
   * Data related to the current edge's layout, such as control points.
   */
  layout?: EdgeLayout;
  sourcePort: ReactflowEdgePort;
  targetPort: ReactflowEdgePort;
}

export type ReactflowBaseNode = Node<ReactflowNodeData, 'base'>;
export type ReactflowStartNode = Node<ReactflowNodeData, 'start'>;
export type ReactflowGroupNode = Node<ReactflowNodeData, 'group'>;
export type ReactflowDiamondNode = Node<ReactflowNodeData, 'diamond'>;
export type ReactflowHexagonNode = Node<ReactflowNodeData, 'hexagon'>;
export type ReactflowLLMNode = Node<ReactflowNodeData, 'llm'>;
export type ReactflowNodeWithData =
  | ReactflowBaseNode
  | ReactflowStartNode
  | ReactflowGroupNode
  | ReactflowDiamondNode
  | ReactflowHexagonNode
  | ReactflowLLMNode;

export type ReactflowBaseEdge = Edge<ReactflowEdgeData, 'base'>;
export type ReactflowEdgeWithData = ReactflowBaseEdge;

export interface Reactflow {
  nodes: ReactflowNodeWithData[];
  edges: ReactflowEdgeWithData[];
}
