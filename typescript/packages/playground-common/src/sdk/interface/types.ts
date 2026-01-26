/**
 * Unified Type System - Core Interface Types
 *
 * This file contains all unified interface types that work for both
 * mock and real BAML runtimes. No WASM dependencies allowed.
 *
 * Key principles:
 * - Pure TypeScript types (no WASM types)
 * - Used by both BamlRuntime and MockBamlRuntime
 * - Safe for atoms and UI components
 */

// ============================================================================
// SPAN & LOCATION TYPES
// ============================================================================

/**
 * Represents a location in a source file
 */
export interface SpanInfo {
  filePath: string;
  start: number;
  end: number;
  startLine: number;
  startColumn: number;
  endLine: number;
  endColumn: number;
}

/**
 * Information about a parent function
 */
export interface ParentFunctionInfo {
  name: string;
  start: number;
  end: number;
}

// ============================================================================
// PARAMETER & SIGNATURE TYPES
// ============================================================================

/**
 * Information about a function parameter
 */
export interface ParameterInfo {
  name: string;
  value?: string;
  error?: string;
  type?: string; // Optional: extracted from signature
}

// ============================================================================
// FUNCTION TYPES (Replacing FunctionDefinition)
// ============================================================================

/**
 * Orchestration scope info (from WasmScope)
 */
export interface OrchestrationScope {
  name: string;
  scopeInfo: unknown; // Opaque orchestration data
}

/**
 * Test case metadata (replaces WasmTestCase usage)
 * For backward compatibility, also includes TestCaseInput fields
 */
export interface TestCaseMetadata {
  name: string;
  inputs: ParameterInfo[];
  error?: string;
  span: SpanInfo;
  parentFunctions: ParentFunctionInfo[];

  // Backward compatibility with TestCaseInput
  id: string; // Generated ID
  source: 'test' | 'manual'; // Source of test case
  functionId: string; // Parent function name
  filePath: string; // From span.filePath
  status?: 'unknown' | 'passing' | 'failing'; // Test status
}

/**
 * Base function metadata (replaces FunctionDefinition)
 * No WASM dependencies - pure TypeScript types
 */
export interface FunctionMetadata {
  name: string;
  type: 'function' | 'llm_function' | 'workflow';
  functionFlavor: 'llm' | 'expr';
  span: SpanInfo;
  signature: string;
  testSnippet: string;

  // Test cases are now interface types, not WASM types
  testCases: TestCaseMetadata[];

  // LLM-specific (only for llm_function type)
  clientName?: string;

  // Orchestration graph (for complex functions)
  orchestrationGraph?: OrchestrationScope[];
}

// ============================================================================
// WORKFLOW/GRAPH TYPES
// ============================================================================

export type NodeType =
  | 'function'        // Regular BAML function
  | 'llm_function'    // LLM-calling function
  | 'conditional'     // If/else block
  | 'loop'           // Loop block
  | 'return'         // Return statement
  | 'group';         // Container/subgraph

export interface GraphNode {
  id: string;
  type: NodeType;
  label: string;
  functionName?: string;
  position?: { x: number; y: number };
  parent?: string; // ID of parent group node

  // Cache invalidation
  codeHash: string;
  lastModified: number;

  // LLM-specific
  llmClient?: string;

  metadata?: Record<string, unknown>;
}

export interface GraphEdge {
  id: string;
  source: string;
  target: string;
  label?: string;
  condition?: string;
}

export interface Parameter {
  name: string;
  type: string;
  optional: boolean;
  defaultValue?: unknown;
}

export interface WorkflowDefinition {
  id: string;
  displayName: string;
  filePath: string;
  startLine: number;
  endLine: number;
  nodes: GraphNode[];
  edges: GraphEdge[];
  entryPoint: string;
  parameters: Parameter[];
  returnType: string;
  childFunctions: string[];
  lastModified: number;
  codeHash: string;
}

// ============================================================================
// PROMPT TYPES
// ============================================================================

export type PromptType = 'chat' | 'completion';

export interface ChatMessagePart {
  type: 'text' | 'image' | 'audio' | 'pdf' | 'video';
  content: string;
  metadata?: unknown;
}

export interface ChatMessage {
  role: string;
  parts: ChatMessagePart[];
}

export interface PromptInfo {
  type: PromptType;
  clientName: string;
  // For chat prompts
  messages?: ChatMessage[];
  // For completion prompts
  text?: string;
}

// ============================================================================
// TEST EXECUTION TYPES
// ============================================================================

export type TestStatus =
  | 'passed'
  | 'llm_failed'
  | 'parse_failed'
  | 'finish_reason_failed'
  | 'constraints_failed'
  | 'assert_failed'
  | 'unable_to_run';

export interface ParsedTestResponse {
  value: string;
  checkCount: number;
  explanation?: string; // JSON string if parsing errors occurred
}

export interface LLMResponseInfo {
  clientName: string;
  model: string;
  content: string;
  prompt: PromptInfo;
  inputTokens?: number;
  outputTokens?: number;
  totalTokens?: number;
  stopReason?: string;
  startTimeUnixMs: number;
  latencyMs: number;
}

export interface LLMFailureInfo {
  clientName: string;
  model?: string;
  message: string;
  code: string;
  prompt: PromptInfo;
  startTimeUnixMs: number;
  latencyMs: number;
}

export interface TestExecutionResult {
  functionName: string;
  testName: string;
  status: TestStatus;
  parsedResponse?: ParsedTestResponse;
  llmResponse?: LLMResponseInfo;
  llmFailure?: LLMFailureInfo;
  failureMessage?: string;
  traceUrl?: string;
}

/**
 * Plain object version of WASM test/function response
 * Used for storing test results without WASM dependencies
 */
export interface TestResponseData {
  llm_response?: LLMResponseInfo;
  llm_failure?: LLMFailureInfo;
  parsed_response?: ParsedTestResponse;
  failure_message?: string;
}

// ============================================================================
// EXECUTION CONTEXT
// ============================================================================

export type VizStateUpdateState = 'running' | 'completed' | 'not_running';

export interface VizStateUpdate {
  nodeId: number;
  logFilterKey?: string;
  newState: VizStateUpdateState;
}

export interface WatchNotification {
  variableName?: string;
  channelName?: string;
  /** Function name that emitted this notification */
  functionName?: string;
  isStream: boolean;
  /** Optional serialized payload; may be synthesized from stateUpdate */
  value?: string;
  /**
   * Optional reducer-driven state update keyed by runtime node id; logFilterKey is metadata.
   */
  stateUpdate?: VizStateUpdate;
}

// ============================================================================
// WATCH NOTIFICATION VALUE TYPES (Discriminated Union)
// ============================================================================

/**
 * Span information from watch events (for code location mapping)
 */
export interface WatchEventSpan {
  filePath: string;
  startLine: number;
  startColumn: number;
  endLine: number;
  endColumn: number;
}

/**
 * Header event - workflow section entered
 * Emitted when execution enters a header section (e.g., //# gather applicant context)
 */
export interface WatchHeaderValue {
  type: 'header';
  /** Header title/label */
  label: string;
  /** Hierarchical level (1, 2, 3, etc.) */
  level: number;
  /** Span for code location mapping */
  span?: WatchEventSpan;
}

/**
 * Header stopped event - workflow section exited
 * HACK: Emitted synthetically when a new header comes in at the same or shallower level.
 * This is a workaround until proper exit events are emitted from the interpreter.
 */
export interface WatchHeaderStoppedValue {
  type: 'header_stopped';
  /** Header title/label */
  label: string;
  /** Hierarchical level (1, 2, 3, etc.) */
  level: number;
  /** Span for code location mapping */
  span?: WatchEventSpan;
}

/**
 * Stream start event - beginning of a streaming value
 */
export interface WatchStreamStartValue {
  type: 'stream_start';
  /** Unique stream identifier */
  id: string;
}

/**
 * Stream update event - partial value during streaming
 */
export interface WatchStreamUpdateValue {
  type: 'stream_update';
  /** Unique stream identifier */
  id: string;
  /** Partial value as JSON string */
  value: string;
}

/**
 * Stream end event - streaming completed
 */
export interface WatchStreamEndValue {
  type: 'stream_end';
  /** Unique stream identifier */
  id: string;
}

/**
 * Regular variable value (no type field means it's a plain value)
 */
export interface WatchVariableValue {
  type?: undefined;
  [key: string]: unknown;
}

/**
 * Discriminated union for all parsed watch value types
 */
export type WatchNotificationValue =
  | WatchHeaderValue
  | WatchHeaderStoppedValue
  | WatchStreamStartValue
  | WatchStreamUpdateValue
  | WatchStreamEndValue
  | WatchVariableValue;

export interface TestExecutionContext {
  apiKeys?: Record<string, string>;
  abortSignal?: AbortSignal;
  loadMediaFile?: (path: string) => Promise<Uint8Array>;
  /** Whether to run tests in parallel (default: false = sequential) */
  parallel?: boolean;
  /** Called when a partial response is received during streaming */
  onPartialResponse?: (functionName: string, testName: string, response: TestResponseData) => void;
  /** Called when a test completes (success or failure) */
  onTestComplete?: (functionName: string, testName: string, response: TestResponseData, status: 'passed' | 'llm_failed' | 'parse_failed' | 'constraints_failed' | 'assert_failed' | 'error', latencyMs: number) => void;
  /** Called when a watch notification is received */
  onWatchNotification?: (notification: WatchNotification) => void;
  /** Called when code should be highlighted */
  onHighlight?: (spans: SpanInfo[]) => void;
}

// ============================================================================
// CALL GRAPH (Static Definition)
// ============================================================================

export type BlockType = 'if' | 'loop' | 'return' | 'assignment' | 'expression';

/**
 * Static call graph node extracted at compile time
 */
export interface CallGraphNode {
  /** Node ID (function name or block ID) */
  id: string;
  /** Node type */
  type: 'function' | 'llm_function' | 'block';
  /** Block type (if block node) */
  blockType?: BlockType;
  /** User annotation (from comments) */
  annotation?: string;
  /** Child nodes */
  children: CallGraphNode[];
  /** Span info */
  span?: SpanInfo;
}

/**
 * Function with its call graph
 * For backward compatibility, also includes workflow fields
 */
export interface FunctionWithCallGraph extends FunctionMetadata {
  /** Static call graph (extracted at compile time) */
  callGraph: CallGraphNode;
  /** Whether this is a root function (not called by others) */
  isRoot: boolean;
  /** Depth of call graph */
  callGraphDepth: number;

  // Backward compatibility with WorkflowDefinition
  id: string; // Same as name
  displayName: string; // Same as name
  filePath: string; // From span.filePath
  startLine: number; // From span.startLine
  endLine: number; // From span.endLine
  nodes: GraphNode[]; // From callGraph
  edges: GraphEdge[]; // From callGraph
  entryPoint: string; // Same as name
  parameters: Parameter[]; // Parsed from signature
  returnType: string; // Parsed from signature
  childFunctions: string[]; // From callGraph
  lastModified: number; // Timestamp
  codeHash: string; // Hash of function code
}

// ============================================================================
// FUNCTION CALL (Actual Runtime Execution)
// ============================================================================

/**
 * Represents a specific invocation of a function
 * Captures actual runtime values vs static definition
 */
export interface FunctionCall {
  /** Call ID (unique for this invocation) */
  callId: string;
  /** Function name */
  functionName: string;
  /** Parent call ID (if nested) */
  parentCallId?: string;
  /** Iteration within parent (for loops) */
  iteration: number;

  /** Actual runtime values */
  runtime: {
    /** Actual LLM client used (may differ from definition) */
    actualClient?: string;
    /** Actual model used */
    actualModel?: string;
    /** Actual input types */
    actualInputTypes?: Record<string, string>;
    /** Actual output type */
    actualOutputType?: string;
  };

  /** Execution state */
  state: 'pending' | 'running' | 'success' | 'error';

  /** Inputs (actual values) */
  inputs?: Record<string, unknown>;

  /** Outputs (actual values) */
  outputs?: Record<string, unknown>;

  /** Error if failed */
  error?: {
    message: string;
    code?: string;
    stack?: string;
  };

  /** Timing */
  startTime: number;
  endTime?: number;
  duration?: number;

  /** Child calls */
  childCalls: FunctionCall[];
}
