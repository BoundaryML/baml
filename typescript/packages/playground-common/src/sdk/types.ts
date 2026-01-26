/**
 * BAML SDK Type Definitions
 * Based on the DESIGN.md specification
 */

import { WasmTestCase } from "@gloo-ai/baml-schema-wasm-web";
import type { FunctionWithCallGraph } from "./interface";

// ============================================================================
// RE-EXPORT UNIFIED TYPES FROM INTERFACE LAYER
// ============================================================================

// Re-export unified types for convenience
export type {
  // Core types
  FunctionMetadata,
  FunctionWithCallGraph,
  TestCaseMetadata,
  CallGraphNode,
  SpanInfo,
  ParentFunctionInfo,
  ParameterInfo,
  OrchestrationScope,

  // Prompt types
  PromptInfo,
  ChatMessage,
  ChatMessagePart,
  PromptType,

  // Test execution types
  TestStatus,
  TestExecutionResult,
  ParsedTestResponse,
  LLMResponseInfo,
  LLMFailureInfo,
  TestExecutionContext,
  WatchNotification,

  // Watch notification value types
  WatchEventSpan,
  WatchHeaderValue,
  WatchStreamStartValue,
  WatchStreamUpdateValue,
  WatchStreamEndValue,
  WatchVariableValue,
  WatchNotificationValue,

  // Event types
  RichExecutionEvent,
  BaseExecutionEvent,
  NodeEnterEvent,
  NodeExitEvent,
  BlockEnterEvent,
  BlockExitEvent,
  LLMRequestEvent,
  LLMResponseEvent,
  LLMFailureEvent,
  PartialResponseEvent,
  HeaderEnterEvent,
  HeaderExitEvent,
  VariableUpdateEvent,
  HighlightEvent,
  LogEvent,

  // Function call types
  FunctionCall,
} from './interface';

// Re-export generators for creating mock data
export {
  createMockFunction,
  createMockTestCase,
  createMockTestResult,
  createMockPrompt,
  createMockSpan,
} from './interface';

// ============================================================================
// Core Node & Execution States
// ============================================================================

export type NodeExecutionState =
  | 'not-started' // Never executed
  | 'pending' // Waiting for dependencies
  | 'running' // Currently executing
  | 'success' // Completed successfully
  | 'error' // Failed with error
  | 'skipped' // Conditionally skipped
  | 'cached'; // Using cached result

export type ExecutionStatus =
  | 'pending'
  | 'running'
  | 'paused'
  | 'completed'
  | 'error'
  | 'cancelled';

// ============================================================================
// Graph Structure Types
// ============================================================================

export type NodeType =
  | 'function'
  | 'llm_function'
  | 'conditional'
  | 'loop'
  | 'return'
  | 'group'; // Container/subgraph node

export interface GraphNode {
  id: string;
  type: NodeType;
  label: string;
  functionName?: string;
  position?: { x: number; y: number };
  parent?: string; // ID of parent group node (for subgraphs)

  // Cache invalidation tracking
  codeHash: string; // Hash of the node's implementation
  lastModified: number; // Timestamp when node code last changed

  // LLM-specific metadata
  llmClient?: string; // e.g., "GPT4o", "Claude-3"

  metadata?: Record<string, unknown>;
}

export interface GraphEdge {
  id: string;
  source: string;
  target: string;
  label?: string; // delete later for this
  condition?: string;
}

export interface Parameter {
  name: string;
  type: string;
  optional: boolean;
  defaultValue?: unknown;
}

// ============================================================================
// Workflow Definition
// ============================================================================

// WorkflowDefinition is deprecated - use FunctionWithCallGraph instead
// Workflows are now just FunctionWithCallGraph objects with workflow compatibility fields
// (id, displayName, nodes, edges, entryPoint, parameters, returnType, childFunctions, etc.)
export type WorkflowDefinition = FunctionWithCallGraph;

// ============================================================================
// Execution & Node Execution
// ============================================================================

export interface LogEntry {
  timestamp: number;
  level: 'debug' | 'info' | 'warn' | 'error';
  message: string;
  metadata?: Record<string, unknown>;
  executionId: string;
}

export interface NodeExecution {
  nodeId: string;
  executionId: string;
  state: NodeExecutionState;
  inputs: Record<string, any>;
  outputs?: Record<string, any>;
  logs: LogEntry[];
  startTime: number;
  endTime?: number;
  duration?: number;
  error?: Error;
  metadata?: {
    llmClient?: string;
    llmRequest?: Record<string, any>;
    llmResponse?: Record<string, any>;
    [key: string]: any;
  };
}

export interface ExecutionSnapshot {
  id: string;
  workflowId: string;
  timestamp: number;

  // Frozen graph structure at execution time
  graphSnapshot: {
    nodes: GraphNode[];
    edges: GraphEdge[];
    codeHash: string;
  };

  status: ExecutionStatus;
  nodeExecutions: Map<string, NodeExecution>;
  trigger: 'manual' | 'auto' | 'test';
  duration?: number;
  branchPath: string[]; // Which conditional branches were taken
  inputs: Record<string, any>;
  outputs?: Record<string, any>;
  error?: Error;
}

// ============================================================================
// Cache Types
// ============================================================================

export interface CacheEntry {
  nodeId: string;
  codeHash: string; // Hash of node implementation when cached
  inputs: Record<string, any>;
  inputsHash: string; // Hash of inputs for comparison
  outputs: Record<string, any>;
  executionId: string;
  timestamp: number;
  duration: number;
}

export type CachePolicy = 'auto' | 'always-run' | 'always-cache';

// ============================================================================
// Code Synchronization
// ============================================================================

export interface CodePosition {
  filePath: string;
  line: number;
  column: number;
  functionName?: string;
}

// ============================================================================
// Events
// ============================================================================

export type BAMLEvent =
  // Workflow events
  | { type: 'workflow.discovered'; workflow: FunctionWithCallGraph }
  | { type: 'workflow.updated'; workflow: FunctionWithCallGraph; changes: string[] }
  | { type: 'workflow.deleted'; workflowId: string }
  | { type: 'workflow.selected'; workflowId: string }

  // Execution events
  | { type: 'execution.started'; executionId: string; workflowId: string }
  | { type: 'execution.completed'; executionId: string; duration: number; outputs: Record<string, unknown> }
  | { type: 'execution.error'; executionId: string; error: Error; nodeId?: string }
  | { type: 'execution.cancelled'; executionId: string; reason?: string }

  // Node events
  | { type: 'node.started'; executionId: string; nodeId: string; inputs: Record<string, unknown> }
  | { type: 'node.progress'; executionId: string; nodeId: string; progress: number }
  | { type: 'node.completed'; executionId: string; nodeId: string; inputs?: Record<string, unknown>; outputs: Record<string, unknown>; duration: number }
  | { type: 'node.error'; executionId: string; nodeId: string; error: Error }
  | { type: 'node.log'; executionId: string; nodeId: string; log: LogEntry }
  | { type: 'node.cached'; executionId: string; nodeId: string; fromExecutionId: string }

  // Graph events
  | { type: 'graph.changed'; workflowId: string; graph: { nodes: GraphNode[]; edges: GraphEdge[] } }
  | { type: 'graph.layout.updated'; workflowId: string; positions: Map<string, { x: number; y: number }> }

  // Code events
  | { type: 'code.modified'; filePath: string; affectedWorkflows: string[] }
  | { type: 'code.function.clicked'; functionName: string; position: CodePosition }
  | { type: 'code.function.hovered'; functionName: string; position: CodePosition }

  // Cache events
  | { type: 'cache.hit'; nodeId: string; executionId: string }
  | { type: 'cache.miss'; nodeId: string; executionId: string }
  | { type: 'cache.invalidated'; nodeIds: string[]; reason: string }
  | { type: 'cache.cleared'; scope: 'all' | 'workflow'; workflowId?: string };

// ============================================================================
// Input Library Types (Phase 1: Previous Executions)
// ============================================================================

/**
 * Input source from a previous execution
 */
export interface ExecutionInput {
  id: string; // executionId
  name: string; // "Execution #3" or custom name
  source: 'execution';
  nodeId: string;
  executionId: string;
  timestamp: number;
  inputs: Record<string, any>;
  outputs?: Record<string, any>;
  status: 'success' | 'error' | 'running';
}

/**
 * Input source from a test case (future)
 */
export interface TestCaseInput {
  id: string; // testId
  name: string; // "success_case"
  source: 'test';
  functionId: string;
  filePath: string;
  inputs: Record<string, any>;
  expectedOutput?: Record<string, any>;
  status?: 'passing' | 'failing' | 'unknown';
  lastRun?: number;
}

/**
 * Manually created input source (future)
 */
export interface ManualInput {
  id: string; // UUID
  name: string; // user-provided name
  source: 'manual';
  nodeId: string;
  inputs: Record<string, any>;
  createdAt: number;
  saved: boolean;
}

/**
 * Union type for all input sources
 */
export type InputSource = ExecutionInput | TestCaseInput | ManualInput;

// ============================================================================
// Debug Panel Types (for simulating BAML file interactions)
// ============================================================================

export interface BAMLFunction {
  name: string;
  type: 'workflow' | 'function' | 'llm_function';
  filePath: string; // Relative to project/, e.g., "workflows/workflow1.baml"
}

export interface BAMLTest {
  name: string;
  functionName: string; // Which function this test is for
  filePath: string;
  nodeType: 'llm_function' | 'function';
}

export interface BAMLFile {
  path: string; // e.g., "workflows/workflow1.baml"
  functions: FunctionWithCallGraph[];
  tests: BAMLTest[];
}

export type CodeClickEvent =
  | {
    type: 'function';
    functionName: string;
    functionType: 'workflow' | 'function' | 'llm_function' | 'conditional' | 'loop' | 'group' | 'return' | 'block';
    filePath: string;
    /**
     * If clicking a node within a workflow graph, this should be the workflow ID.
     * This tells the navigation heuristic to select the node within the active workflow
     * rather than trying to find which workflow contains this function.
     */
    workflowId?: string;
    /**
     * For workflow nodes, this is the specific node ID (might be different from functionName)
     * For example, group nodes, if nodes, etc.
     */
    nodeId?: string;
  }
  | {
    type: 'test';
    testName: string;
    functionName: string;
    filePath: string;
    nodeType: 'llm_function' | 'function';
  };


// ============================================================================
// SDK Configuration
// ============================================================================

export interface BAMLSDKConfig {
  mode: 'vscode' | 'mock' | 'server';
  mockData?: MockDataProvider;
  provider?: MockDataProvider; // Alias for mockData (backward compatibility)
  serverUrl?: string;
}

export interface MockDataProvider {
  getWorkflows(): WorkflowDefinition[];
  getExecutions(workflowId: string): ExecutionSnapshot[];
  getTestCases(workflowId: string, nodeId: string): TestCaseInput[];
  simulateExecution(workflowId: string, inputs: Record<string, any>, startFromNodeId?: string): AsyncGenerator<BAMLEvent>;
  getBAMLFiles(): BAMLFile[];
}

// ============================================================================
// Compatibility Type Aliases
// ============================================================================

/**
 * @deprecated Use BAMLEvent instead
 * Alias for backward compatibility with old code
 */
export type TestExecutionEvent = BAMLEvent;
