import type { Node as ReactFlowNode, Edge as ReactFlowEdge } from '@xyflow/react';
import type { BamlJsMedia, BamlJsValue } from '@b/pkg-proto';
import type { FC } from 'react';
import type { ResultRendererProps } from '../result-renderers';
import type { SourceNavigationTarget } from '../worker-protocol';

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
    sourceSpan?: SourceNavigationTarget;
    isContainer: boolean;
    llmClient?: string;
    calleeName?: string;
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
  | 'cancelled'
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
  result?: BamlJsValue | null;
  hasResult?: boolean;
  imageOutputs?: BamlJsMedia[];
  errorMessage?: string | null;
  customRenderers?: Record<string, FC<ResultRendererProps>>;
  [key: string]: unknown;
}

export type WorkflowNode = ReactFlowNode<WorkflowNodeData>;

export interface WorkflowEdgeData {
  label?: string;
  color?: string;
  /**
   * ELK-routed polyline (start + bend points + end), in absolute flow coords.
   * If present, the edge renders along these exact points (rounded corners).
   * If absent, the edge falls back to a smoothstep path.
   */
  points?: { x: number; y: number }[];
  [key: string]: unknown;
}

export type WorkflowEdge = ReactFlowEdge<WorkflowEdgeData>;
