/**
 * Shared types for communication between the main thread and the BAML web worker.
 *
 * All postMessage calls between SplitPreview (main) and baml-lsp-worker (worker)
 * use discriminated unions keyed on `type`. This eliminates `as any` casts and
 * gives exhaustive switch narrowing.
 */

import type { BamlJsValue, PlainHandleDescriptor, SourceLocation, TagEntry } from '@b/pkg-proto';

/** Runtime event with BamlOutboundValue fields deserialized to BamlJsValue. */
export interface DeserializedRuntimeEvent {
  spanId: string;
  parentSpanId?: string;
  rootSpanId: string;
  timestampMs: number;
  callStack: string[];
  event: DeserializedEventKind | undefined;
}

export type DeserializedEventKind =
  | { $case: 'functionStart'; functionStart: { name: string; args: BamlJsValue[] } }
  | { $case: 'functionEnd'; functionEnd: { name: string; durationMs: number; result: BamlJsValue | null; error?: string | null } }
  | { $case: 'log'; log: { data: BamlJsValue | null; level: string; source?: SourceLocation } }
  | { $case: 'custom'; custom: { name: string; data: BamlJsValue | null } }
  | { $case: 'setTags'; setTags: { tags: TagEntry[] } };

// ---------------------------------------------------------------------------
// Log decoration types (inline log display like ErrorLens)
// ---------------------------------------------------------------------------

export type LogLevel = 'debug' | 'info' | 'warn' | 'error';

export interface LogDecoration {
  /** 1-indexed line number */
  line: number;
  level: LogLevel;
  /** Formatted log message (truncated to ~60 chars) */
  message: string;
  /** Number of logs on this line (for "×N" display) */
  count: number;
}

interface SourceNavigationTargetBase {
  line: number;
  column: number;
  endLine?: number;
  endColumn?: number;
  startOffset?: number;
  endOffset?: number;
}

export type SourceNavigationTarget = SourceNavigationTargetBase & (
  | { fileId: number; filePath?: string }
  | { filePath: string; fileId?: number }
);

// ---------------------------------------------------------------------------
// Shared domain types
// ---------------------------------------------------------------------------

export interface DiagnosticEntry {
  severity: 'error' | 'warning' | 'info';
  message: string;
}

export type FunctionKind = 'llm' | 'expr';
export type FunctionOrigin = 'userDefined' | 'companion' | 'internal';

export interface LlmCapabilities {
  /** Whether render_prompt sub-function exists. Call via `callFunction("${name}$render_prompt", args)`. */
  renderPrompt: boolean;
  /** Whether build_request sub-function exists. Call via `callFunction("${name}$build_request", args)`. */
  buildRequest: boolean;
  /** The LLM client name (e.g., "MyClient"). */
  clientName?: string;
}

/** Metadata about a BAML function exposed to the playground.
 *
 *  Sub-functions (render_prompt, build_request) are not separate entries —
 *  they are represented as capabilities on the parent function.
 *  To call them, use the naming convention with `callFunction`:
 *  - `callFunction("${fn.name}$render_prompt", args)` → PromptAst
 *  - `callFunction("${fn.name}$build_request", args)` → HTTP Request
 */
export interface FunctionInfo {
  name: string;
  kind: FunctionKind;
  origin: FunctionOrigin;
  capabilities?: LlmCapabilities;
}

export interface ProjectUpdate {
  isBexCurrent: boolean;
  functions: FunctionInfo[];
  diagnostics: DiagnosticEntry[];
}

export type PlaygroundNotification =
  | { type: 'listProjects'; projects: string[] }
  | { type: 'updateProject'; project: string; update: ProjectUpdate }
  | { type: 'openPlayground'; project: string; functionName?: string }
  | { type: 'controlFlowGraphResult'; functionName: string; graph: ControlFlowGraph | null }
  | { type: 'cursorContext'; context: CursorContext }
  | { type: 'testCollectionResult'; project: string; generation: number; callId: number; data: number[]; expandError?: { testsetName: string; message: string } }
  | { type: 'runtimeEvent'; data: number[]; callId?: number };

// ---------------------------------------------------------------------------
// Control flow graph types (matches Rust serde output from baml_compiler2_visualization)
// ---------------------------------------------------------------------------

export type CfgNodeType =
  | 'functionRoot'
  | 'llmFunction'
  | 'headerContextEnter'
  | 'branchGroup'
  | 'branchArm'
  | 'loop'
  | 'otherScope';

export interface CfgNode {
  id: number;
  parentNodeId: number | null;
  logFilterKey: string;
  label: string;
  sourceExpr: number | null;
  sourceSpan?: SourceNavigationTarget;
  nodeType: CfgNodeType;
  llmClient?: string;
  calleeName?: string;
  isContainer: boolean;
}

export interface CfgEdge {
  src: number;
  dst: number;
  label?: string;
}

export interface ControlFlowGraph {
  /** IndexMap<NodeId, Node> serializes as an object with numeric string keys. */
  nodes: Record<string, CfgNode>;
  /** IndexMap<NodeId, Vec<Edge>> serializes as an object with numeric string keys. */
  edgesBySrc: Record<string, CfgEdge[]>;
}

// ---------------------------------------------------------------------------
// Cursor context types (matches Rust CursorContext serde output)
// ---------------------------------------------------------------------------

export interface CursorContext {
  functionName: string | null;
  isWorkflow: boolean;
  workflowMemberships: string[];
  /** Raw ExprId index — NOT a CFG NodeId. Match against node.metadata.sourceExpr
   *  in the cached ControlFlowGraph to find the corresponding graph node. */
  sourceExprId: number | null;
  /** Ordered list of expression IDs containing the cursor, from most specific
   *  (smallest span) to least specific (largest span). The TS side tries each
   *  in order, highlighting the first that matches a CFG node. */
  sourceExprCandidates?: number[];
  /** Function body that owns sourceExprId/sourceExprCandidates. This can differ
   *  from functionName at call sites, where functionName is the callee. */
  sourceExprFunctionName?: string | null;
  testName: string | null;
  /** Byte offset of the cursor position for cursor ↔ event matching. */
  cursorOffset?: number | null;
}

export interface FetchLogEntry {
  id: number;
  callId: number;
  timestamp: number;
  method: string;
  url: string;
  requestHeaders: Record<string, string>;
  requestBody: string;
  status: number | null;
  responseBody: string | null;
  error: string | null;
  durationMs: number | null;
  responseHeaders: Record<string, string> | null;
}

export interface EnvVarRequest {
  id: number;
  variable: string;
}

/** A single function invocation with its associated logs and result. */
export interface RunEntry {
  id: number;
  functionName: string;
  argsJson: string;
  testName?: string;
  fetchLogs: FetchLogEntry[];
  /** Runtime events (log.info, baml.events.send, etc.) emitted during this run. */
  runtimeEvents: DeserializedRuntimeEvent[];
  result: BamlJsValue | null;
  error: string | null;
  status: 'running' | 'success' | 'error' | 'cancelled';
  startTime: number;
  durationMs: number | null;
}

// ---------------------------------------------------------------------------
// Worker → Main thread messages
// ---------------------------------------------------------------------------

export type WorkerOutMessage =
  | { type: 'ready' }
  | { type: 'playgroundNotification'; notification: PlaygroundNotification }
  | { type: 'diagnostics'; entries: DiagnosticEntry[] }
  | { type: 'callFunctionResult'; id: number; result: BamlJsValue<PlainHandleDescriptor> }
  | { type: 'callFunctionError'; id: number; error: string; cancelled?: boolean }
  | { type: 'fetchLogNew'; entry: FetchLogEntry }
  | { type: 'fetchLogUpdate'; logId: number; patch: Partial<FetchLogEntry> }
  | { type: 'runtimeEventNew'; event: DeserializedRuntimeEvent; callId: number | null }
  | { type: 'runtimeEventError'; error: string }
  | { type: 'envVarRequest'; id: number; variable: string }
  | { type: 'processEnvVars'; vars: Record<string, string> }
  | { type: 'envVarFromShell'; variable: string; value: string }
  | { type: 'knownEnvVarNames'; names: string[] }
  | { type: 'inputRequest'; id: number; prompt: string | undefined; callId: number }
  | { type: 'inputResolved'; id: number; callId: number }
  | { type: 'vfsFileChanged'; path: string; content: string }
  | { type: 'vfsFileDeleted'; path: string }
  | { type: 'buildTime'; value: string }
  | { type: 'controlFlowGraphResult'; functionName: string; graph: ControlFlowGraph | null }
  | { type: 'cursorContext'; context: CursorContext }
  | { type: 'logDecorations'; decorations: LogDecoration[] }
  | { type: 'clearLogDecorations' }
  | { type: 'wasmPanic'; message: string; stack?: string };

// ---------------------------------------------------------------------------
// Main thread → Worker messages
// ---------------------------------------------------------------------------

export type WorkerInMessage =
  | { type: 'callFunction'; id: number; name: string; argsProto: Uint8Array; project: string }
  | { type: 'cancelCall'; id: number; project: string }
  | { type: 'clearHandles'; runIds: number[] }
  | { type: 'envVarResponse'; id: number; value: string | undefined; variable?: string }
  | { type: 'inputResponse'; id: number; value: string; callId: number }
  | { type: 'setEnvVar'; key: string; value: string }
  | { type: 'deleteEnvVar'; key: string }
  | { type: 'selectProject'; root: string }
  | { type: 'requestState' }
  | { type: 'requestControlFlowGraph'; project: string; functionName: string }
  | { type: 'cursorPosition'; file: string; line: number; column: number }
  | { type: 'requestCollectTests'; project: string }
  | { type: 'callTestFunction'; id: number; project: string; generation: number; testName: string }
  | { type: 'expandTestSet'; project: string; generation: number; testsetName: string }
  | { type: 'filesChanged'; files: Record<string, string> }
  | { type: 'dispose' };

// ---------------------------------------------------------------------------
// Init message (sent once with MessagePort)
// ---------------------------------------------------------------------------

export interface WorkerInitMessage {
  port: MessagePort;
  /**
   * Initial file map (relative keys).
   * Text files (e.g. "baml_src/main.baml") have raw content strings.
   * Media files (e.g. "images/photo.png") have data-URL strings.
   */
  initialFiles: Record<string, string>;
  /** Workspace root path (e.g. "/workspace"). */
  rootPath: string;
}
