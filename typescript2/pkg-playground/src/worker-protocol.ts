/**
 * Shared types for communication between the main thread and the BAML web worker.
 *
 * All postMessage calls between SplitPreview (main) and baml-lsp-worker (worker)
 * use discriminated unions keyed on `type`. This eliminates `as any` casts and
 * gives exhaustive switch narrowing.
 */

// ---------------------------------------------------------------------------
// Shared domain types
// ---------------------------------------------------------------------------

export interface DiagnosticEntry {
  severity: 'error' | 'warning' | 'info';
  message: string;
}

export type FunctionKind = 'llm' | 'expr';

export interface LlmCapabilities {
  /** Whether render_prompt sub-function exists. Call via `callFunction("${name}.render_prompt", args)`. */
  renderPrompt: boolean;
  /** Whether build_request sub-function exists. Call via `callFunction("${name}.build_request", args)`. */
  buildRequest: boolean;
  /** The LLM client name (e.g., "MyClient"). */
  clientName?: string;
}

/** Metadata about a BAML function exposed to the playground.
 *
 *  Sub-functions (render_prompt, build_request) are not separate entries —
 *  they are represented as capabilities on the parent function.
 *  To call them, use the naming convention with `callFunction`:
 *  - `callFunction("${fn.name}.render_prompt", args)` → PromptAst
 *  - `callFunction("${fn.name}.build_request", args)` → HTTP Request
 */
export interface FunctionInfo {
  name: string;
  kind: FunctionKind;
  capabilities?: LlmCapabilities;
}

/** Metadata about a BAML test case.
 *
 *  Each test targets a single function (the first in `functions [...]`)
 *  and carries pre-serialized args JSON for immediate use.
 */
export interface TestInfo {
  name: string;
  functionName: string;
  argsJson: string;
}

export interface ProjectUpdate {
  isBexCurrent: boolean;
  functions: FunctionInfo[];
  tests: TestInfo[];
}

export type PlaygroundNotification =
  | { type: 'listProjects'; projects: string[] }
  | { type: 'updateProject'; project: string; update: ProjectUpdate }
  | { type: 'openPlayground'; project: string; functionName?: string }
  | { type: 'controlFlowGraphResult'; functionName: string; graph: ControlFlowGraph | null }
  | { type: 'cursorContext'; context: CursorContext };

// ---------------------------------------------------------------------------
// Control flow graph types (matches Rust serde output from baml_compiler2_visualization)
// ---------------------------------------------------------------------------

export type CfgNodeType =
  | 'functionRoot'
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
  nodeType: CfgNodeType;
}

export interface CfgEdge {
  src: number;
  dst: number;
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
  testName: string | null;
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
  result: string | null;
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
  | { type: 'callFunctionResult'; id: number; result: string }
  | { type: 'callFunctionError'; id: number; error: string; cancelled?: boolean }
  | { type: 'fetchLogNew'; entry: FetchLogEntry }
  | { type: 'fetchLogUpdate'; logId: number; patch: Partial<FetchLogEntry> }
  | { type: 'envVarRequest'; id: number; variable: string }
  | { type: 'vfsFileChanged'; path: string; content: string }
  | { type: 'vfsFileDeleted'; path: string }
  | { type: 'buildTime'; value: string }
  | { type: 'controlFlowGraphResult'; functionName: string; graph: ControlFlowGraph | null }
  | { type: 'cursorContext'; context: CursorContext };

// ---------------------------------------------------------------------------
// Main thread → Worker messages
// ---------------------------------------------------------------------------

export type WorkerInMessage =
  | { type: 'callFunction'; id: number; name: string; argsProto: Uint8Array; project: string }
  | { type: 'cancelCall'; id: number; project: string }
  | { type: 'clearHandles'; runIds: number[] }
  | { type: 'envVarResponse'; id: number; value: string | undefined; variable?: string }
  | { type: 'setEnvVar'; key: string; value: string }
  | { type: 'deleteEnvVar'; key: string }
  | { type: 'selectProject'; root: string }
  | { type: 'requestState' }
  | { type: 'requestControlFlowGraph'; project: string; functionName: string }
  | { type: 'cursorPosition'; file: string; line: number; column: number }
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
