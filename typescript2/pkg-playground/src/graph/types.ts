import type { Node as ReactFlowNode, Edge as ReactFlowEdge } from '@xyflow/react';

// Internal graph types (between CFG JSON and ReactFlow)
export type GraphNodeType =
  | 'function'
  | 'llm_function'
  | 'conditional'
  | 'loop'
  | 'scope'
  | 'header';

export interface GraphNode {
  id: string;
  label: string;
  type: GraphNodeType;
  parent: string | null;
  metadata: {
    logFilterKey: string;
    sourceExpr: number | null;
    isContainer: boolean;
  };
}

export interface GraphEdge {
  source: string;
  target: string;
  label?: string;
}

// Execution state (for future Phase 4)
export type NodeExecutionState =
  | 'not-started'
  | 'pending'
  | 'running'
  | 'success'
  | 'error'
  | 'skipped'
  | 'cached';

// ReactFlow node data
export interface WorkflowNodeData {
  label: string;
  graphNodeType: GraphNodeType;
  executionState: NodeExecutionState;
  selected: boolean;
  logFilterKey: string;
  llmClient?: string;
  iterationCount?: number;
  [key: string]: unknown;
}

export type WorkflowNode = ReactFlowNode<WorkflowNodeData>;

export interface WorkflowEdgeData {
  label?: string;
  color?: string;
  [key: string]: unknown;
}

export type WorkflowEdge = ReactFlowEdge<WorkflowEdgeData>;
