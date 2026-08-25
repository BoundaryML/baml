/**
 * Shared types for communication between the main thread and the BAML web worker.
 *
 * All postMessage calls between SplitPreview (main) and baml-lsp-worker (worker)
 * use discriminated unions keyed on `type`. This eliminates `as any` casts and
 * gives exhaustive switch narrowing.
 */

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

export type SourceNavigationTarget = SourceNavigationTargetBase &
  (
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
export type FunctionOrigin =
  | 'userDefined'
  | 'companion'
  | 'internal'
  | 'autoDerive';

export interface LlmCapabilities {
  /** Whether render_prompt preview is available through `startPreviewRun`. */
  renderPrompt: boolean;
  /** Whether build_request preview is available through `startPreviewRun`. */
  buildRequest: boolean;
  /** The LLM client name (e.g., "MyClient"). */
  clientName?: string;
}

/** Schema for one function parameter (mirrors `baml_project::ParamSchema`). */
export interface ParamSchema {
  name: string;
  /** The parameter has a default value and may be omitted entirely. Distinct
   *  from a nullable type, which appears as `{ type: 'optional' }` in
   *  `schema`. */
  hasDefault: boolean;
  /** Exact, unevaluated source text for the declared default expression. */
  defaultExpression?: string;
  schema: FieldSchema;
}

/** One class field; optionality is folded into `schema` as
 *  `{ type: 'optional' }`, not a flag here. */
export interface FieldSchemaField {
  name: string;
  schema: FieldSchema;
}

/** Recursive type schema for the args form (mirrors
 *  `baml_project::FieldSchema`). Named types are `ref`s into
 *  `ProjectUpdate.types`; `name`s are the canonical dotted FQN the engine
 *  registers (`user.shapes.Foo`), usable verbatim in `$baml` markers. */
export type FieldSchema =
  | { type: 'string' }
  | { type: 'int' }
  | { type: 'float' }
  | { type: 'bool' }
  | { type: 'null' }
  | { type: 'bigint' }
  | { type: 'media'; kind: string }
  | { type: 'literal'; value: unknown }
  /** Reference to a named type in `ProjectUpdate.types`; a dangling name
   *  (mid-edit inconsistency) degrades to the raw-JSON fallback. */
  | { type: 'ref'; name: string }
  /** A specific-variant param type (`s: Status.Active`) — self-contained so
   *  the form can emit the enum wire marker without a table entry. */
  | { type: 'enumVariant'; name: string; value: string }
  | { type: 'list'; item: FieldSchema }
  | { type: 'map'; key: FieldSchema; value: FieldSchema }
  | { type: 'optional'; inner: FieldSchema }
  | { type: 'union'; variants: FieldSchema[] }
  | { type: 'unsupported'; display: string };

/** A named type's definition in the per-project table (mirrors
 *  `baml_project::TypeSchema`), keyed by canonical dotted FQN. */
export type TypeSchema =
  | { kind: 'class'; fields: FieldSchemaField[] }
  | { kind: 'enum'; values: string[] }
  | { kind: 'alias'; schema: FieldSchema };

/** Metadata about a BAML function exposed to the playground.
 *
 *  Sub-functions (render_prompt, build_request) are not separate entries —
 *  they are represented as capabilities on the parent function and executed
 *  through `startPreviewRun`.
 */
export interface FunctionInfo {
  name: string;
  kind: FunctionKind;
  origin: FunctionOrigin;
  /** Source-like declaration including parameters, return type, and throws. */
  signature?: string;
  /** One-based source position of the function name. */
  sourcePosition?: {
    file: string;
    line: number;
    column: number;
  };
  capabilities?: LlmCapabilities;
  /** Parameter schemas for the args form. `undefined` = no schema available
   *  (old WASM binary or extraction skipped) → raw-JSON-only mode; `[]` = the
   *  function takes no arguments. */
  params?: ParamSchema[];
}

/** A statically declared legacy test that can seed function previews. */
export interface TestInfo {
  name: string;
  functionName: string;
  argsJson: string;
}

/** Stable identity shared by preview selection state and sidebar rows. */
export function previewTestKey(
  test: Pick<TestInfo, 'functionName' | 'name'>,
): string {
  return `${test.functionName}\u0000${test.name}`;
}

export interface ProjectUpdate {
  isBexCurrent: boolean;
  /** Generation of the installed engine backing this update. Omitted by older runtimes. */
  generation?: number;
  functions: FunctionInfo[];
  /** Omitted by older runtimes; the UI treats that as no previewable tests. */
  tests?: TestInfo[];
  /** Shared type table for `FunctionInfo.params` refs. `undefined` = binary
   *  predates the args form (refs, if any, degrade to raw JSON); may be an
   *  empty object when no function references named types. */
  types?: Record<string, TypeSchema>;
  diagnostics: DiagnosticEntry[];
}

export type PlaygroundNotification =
  | { type: 'listProjects'; projects: string[] }
  | { type: 'updateProject'; project: string; update: ProjectUpdate }
  | {
      type: 'openPlayground';
      project: string;
      functionName?: string;
      testName?: string;
      testsetName?: string;
    }
  | {
      type: 'controlFlowGraphResult';
      functionName: string;
      graph: ControlFlowGraph | null;
      requestId?: number;
    }
  | { type: 'cursorContext'; context: CursorContext }
  | {
      type: 'testCollectionResult';
      project: string;
      generation: number;
      callId: number;
      data: number[];
      expandError?: { testsetName: string; message: string };
    }
  | ({ type: 'valueBody' } & ValueBodyResponse);

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
  | 'otherScope'
  | 'return';

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
  /** ALL functions called anywhere inside this node's expression subtree
   *  (e.g. an if-condition `Abs(LineTotal(x))` reports both). Superset of
   *  calleeName. Absent on older runtimes. */
  calleeNames?: string[];
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

// ---------------------------------------------------------------------------
// RunStore snapshot protocol
// ---------------------------------------------------------------------------

export type BoundaryId = string;
export type RunCursor = number;

export type ValueCodec = 'bamlOutboundValue';
export type ValueAvailability =
  | 'pending'
  | 'available'
  | 'missing'
  | 'omitted'
  | 'lost';

export interface ValueRef {
  id: string;
  codec: ValueCodec;
  availability: ValueAvailability;
  originalSizeBytes: number | null;
  retainedSizeBytes: number | null;
  diagnostic: string | null;
}

export interface ValueBodyResponse {
  requestId: number;
  boundaryId: BoundaryId;
  valueRefId: string;
  codec: ValueCodec;
  availability: ValueAvailability;
  bodyBase64?: string;
  diagnostic?: string;
}

export type RunStatus =
  | 'pending'
  | 'running'
  | 'waitingForInput'
  | 'waitingForEnv'
  | 'cancelling'
  | 'succeeded'
  | 'failed'
  | 'cancelled'
  | 'panicked';

export type RunTarget =
  | { kind: 'function'; functionName: string }
  | { kind: 'test'; generation: number; testName: string }
  | { kind: 'preview'; parentFunctionName: string; helper: string }
  | {
      kind: 'companion';
      parentBoundaryId: BoundaryId | null;
      functionName: string;
    }
  | { kind: 'internal'; name: string };

export type RunVisibility =
  | { kind: 'history' }
  | { kind: 'scoped'; scopeId: string }
  | { kind: 'hidden' }
  | { kind: 'debugOnly' };

export interface RunRequestSummary {
  projectId: string;
  projectGeneration: number;
  target: RunTarget;
  argsSummary: string | null;
  optionsSummary: string | null;
}

export interface RunResult {
  valueRef: ValueRef | null;
  /** Compatibility for older runtimes during local development. */
  value?: string | null;
  rendererHint: string | null;
  supportingPayloadIds: string[];
}

export interface RunError {
  class: string;
  message: string;
  details: string | null;
  valueRef: ValueRef | null;
}

export interface RunCancellation {
  requestedAtMs: number;
  completedAtMs: number | null;
  reason: string | null;
}

export interface RunDiagnostic {
  severity: 'error' | 'warning' | 'info';
  code: string | null;
  message: string;
  callNodeId: string | null;
  payloadId: string | null;
}

export interface RunSourceLocation {
  filePath?: string | null;
  fileId?: number | null;
  line: number;
  column: number;
  endLine?: number | null;
  endColumn?: number | null;
  startOffset?: number | null;
  endOffset?: number | null;
}

export type RunRequestState =
  | 'pending'
  | 'resolved'
  | 'cancelled'
  | 'expired'
  | 'runTerminal';

export interface PayloadBody {
  state:
    | { kind: 'inlineBytes' }
    | { kind: 'inlineJson' }
    | { kind: 'retainedByRef'; id: string }
    | { kind: 'truncated' }
    | { kind: 'compacted' }
    | { kind: 'omittedByPolicy' };
  contentType: string | null;
  originalSizeBytes: number | null;
  retainedSizeBytes: number | null;
}

export interface PayloadEvent {
  id: string;
  callNodeId: string | null;
  timestampMs: number;
  kind:
    | {
        type: 'fetchStarted';
        fetchId: string;
        method: string;
        url: string;
        requestHeaders: Array<{
          name: string;
          valueRedacted: boolean;
          value?: string | null;
        }>;
      }
    | {
        type: 'fetchUpdated';
        fetchId: string;
        status: number | null;
        durationMs: number | null;
        responseHeaders: Array<{
          name: string;
          valueRedacted: boolean;
          value?: string | null;
        }>;
        error: string | null;
      }
    | {
        type: 'inputRequested';
        requestId: string;
        prompt: string | null;
        state: RunRequestState;
      }
    | { type: 'inputResolved'; requestId: string; state: RunRequestState }
    | {
        type: 'envRequested';
        requestId: string;
        key: string;
        state: RunRequestState;
        waiterCount: number;
      }
    | {
        type: 'envResolved';
        requestId: string;
        key: string;
        status:
          | 'resolvedFromOverride'
          | 'resolvedFromProcess'
          | 'resolvedFromUser'
          | 'declinedMissing';
        state: RunRequestState;
        valueRedacted: boolean;
        displayValue: string | null;
      }
    | {
        type: 'log';
        level: string | null;
        message: string;
        source: RunSourceLocation | null;
        valueRef: ValueRef | null;
      }
    | {
        // A chunk written by baml.io.print/println/eprint/eprintln. `print`
        // carries no trailing newline, so consecutive chunks on one stream
        // must be concatenated rather than rendered one row each.
        type: 'output';
        stream: 'stdout' | 'stderr';
        text: string;
      }
    | {
        type: 'capturedValue';
        role: 'rootInput' | 'callInput' | 'callOutput' | 'callError';
        label: string | null;
        valueRef: ValueRef | null;
      };
  redaction: {
    valueRedacted: boolean;
    displaySafe: boolean;
    reason: string | null;
    policyId: string | null;
  };
  body: PayloadBody | null;
}

export interface Run {
  boundaryId: BoundaryId;
  target: RunTarget;
  visibility: RunVisibility;
  status: RunStatus;
  createdAtMs: number;
  startedAtMs: number | null;
  completedAtMs: number | null;
  timeAnchor: { epochCreatedAtMs: number; traceZeroNs: string };
  request: RunRequestSummary;
  result: RunResult | null;
  error: RunError | null;
  cancellation: RunCancellation | null;
  payloads: PayloadEvent[];
  diagnostics: RunDiagnostic[];
  cursor: RunCursor;
}

export interface RunSummary {
  boundaryId: BoundaryId;
  target: RunTarget;
  visibility: RunVisibility;
  status: RunStatus;
  request: RunRequestSummary;
  touchedFunctions: string[];
  createdAtMs: number;
  completedAtMs: number | null;
  retention: string;
}

// ---------------------------------------------------------------------------
// Telemetry (profiles-v1, catalog v1)
//
// Structure and timing live in the profile store, not the run store. These
// mirror the catalog relations one field per column, so a reader can check a
// value against `baml query` output without a translation step.
//
// The grains are deliberately different and must not be conflated:
//   - `TelemetryCallPath` is population-true: every call contributes.
//   - `TelemetryCall` is one individually retained span, bounded by capture
//     policy. It is never "all the calls".
// ---------------------------------------------------------------------------

/** One execution: a root thread, and a row in the executions table. */
export interface TelemetryExecution {
  executionId: string;
  /** Root function. Null when the root span was not retained. */
  entryFqn: string | null;
  /** Human label for whatever started this run. */
  sourceLabel: string | null;
  revisionId: string | null;
  status:
    | 'running'
    | 'abandoned'
    | 'succeeded'
    | 'failed'
    | 'cancelled'
    | 'panicked'
    | null;
  /**
   * `complete` | `no_root_ended` | `root_started_lost` | `index_corrupt`.
   * Anything but `complete` means this execution's evidence is partial and
   * the UI must say so rather than present it as whole.
   */
  indexState: string | null;
  /** `complete` | `partial` | `none`: whether captured values survived. */
  valueState: string | null;
  startedAtMs: number | null;
  durationNs: number | null;
  /** Every call that ran, retained or not. */
  totalCalls: number | null;
  totalErrors: number | null;
  /** Calls kept as spans. The shortfall against `totalCalls` is the gap. */
  callsRetained: number | null;
  threadsTotal: number | null;
}

/** One logical thread. Root threads are executions. */
export interface TelemetryThread {
  threadId: string;
  parentThreadId: string | null;
  spawnCallId: string | null;
  spawnFqn: string | null;
  spawnSiteFile: string | null;
  spawnSiteLine: number | null;
  name: string | null;
  kind: 'root' | 'spawn' | null;
  startedNs: number | null;
  endedNs: number | null;
  endStatus: 'completed' | 'cancelled' | 'errored' | null;
}

/**
 * One calling context: complete counts for every call that took this path,
 * with no per-instance ordering or timestamps by construction.
 */
export interface TelemetryCallPath {
  callPathId: string;
  parentCallPathId: string | null;
  depth: number | null;
  fqn: string | null;
  /** `bytecode` | `sysop` | `native` | `native_unresolved`. */
  kind: string | null;
  /** `user` | `companion` | `internal` | `builtin` | `auto_derive`. */
  origin: string | null;
  /** `root` | `call` | `spawn`. Spawned paths overlap their parent in time. */
  edgeKind: 'root' | 'call' | 'spawn' | null;
  callSiteFile: string | null;
  callSiteLine: number | null;
  callSiteStart: number | null;
  callSiteEnd: number | null;
  /** Population entries: the denominator for any rate or mean. */
  callsStarted: number | null;
  callsSelected: number | null;
  completedOk: number | null;
  completedError: number | null;
  completedCancelled: number | null;
  inclusiveNs: number | null;
  directChildNs: number | null;
  awaitNs: number | null;
  /**
   * `inclusiveNs - directChildNs - awaitNs`. The three are disjoint parts of
   * inclusive time, so summing `selfNs` across paths is a valid CPU total
   * and summing `awaitNs` a valid waiting total, with no double counting.
   */
  selfNs: number | null;
  /** False when a counter saturated or self time underflowed. */
  timingComplete: boolean | null;
  /** Non-null only on synthetic rows standing in for folded-away paths. */
  overflowReason: string | null;
}

/** One individually retained call, with exact timestamps. Evidence. */
export interface TelemetryCall {
  callId: string;
  parentCallId: string | null;
  threadId: string | null;
  /** Exact join to the aggregate: never inferred from the function name. */
  callPathId: string | null;
  fqn: string | null;
  kind: string | null;
  edgeKind: 'root' | 'call' | 'spawn' | null;
  callSiteFile: string | null;
  callSiteLine: number | null;
  startedNs: number | null;
  endedNs: number | null;
  durationNs: number | null;
  status: 'ok' | 'errored' | 'cancelled' | 'exited' | null;
  /** Why this call was kept: `root` | `llm` | `manual`. */
  selectionReasons: string[];
  /**
   * `available` | `not_captured` | `lost:<reason>` | `not_applicable`.
   * Not captured and lost are different facts with different remedies, so
   * the UI must never collapse them into one "no value" state.
   */
  argsState: string | null;
  outputState: string | null;
  errorState: string | null;
  argsCid: string | null;
  outputCid: string | null;
  errorCid: string | null;
  errorId: string | null;
  /**
   * Hydrated captured values, rendered. Media appears as a descriptor
   * (`{"$media":…,"bytes_len":N}`) rather than its bytes, which are fetched
   * separately by content id.
   */
  args: string | null;
  output: string | null;
  error: string | null;
}

/** One captured media payload, fetched on demand by content id. */
export interface TelemetryMedia {
  /** `image`, `audio`, `pdf`. */
  kind: string;
  mime: string;
  /** Base64 bytes, or null when the value carried a URL instead. */
  base64: string | null;
  url: string | null;
  bytesLen: number | null;
}

/** One captured error. */
export interface TelemetryErrorCapture {
  errorId: string;
  throwCallId: string | null;
  throwThreadId: string | null;
  throwCallPathId: string | null;
  throwFqn: string | null;
  throwSiteFile: string | null;
  throwSiteLine: number | null;
  /** `fresh` | `rethrow`. */
  kind: string | null;
  source: string | null;
  valueState: string | null;
  valueCid: string | null;
  /** False when the stack has gaps: not a complete root-to-throw path. */
  stackComplete: boolean | null;
  /** Function names, root to throw. */
  stack: string[];
  /**
   * The captured error, hydrated server-side and carried as its rendered
   * form. Null when nothing was captured or the capture was lost, which
   * `valueState` distinguishes.
   */
  value: string | null;
}

/** One execution's evidence, in the four grains the catalog serves. */
export interface ExecutionTelemetry {
  execution: TelemetryExecution | null;
  threads: TelemetryThread[];
  callPaths: TelemetryCallPath[];
  calls: TelemetryCall[];
  errors: TelemetryErrorCapture[];
}

export interface RunListFilter {
  projectId?: string;
  projectGeneration?: number;
  kinds?: RunTarget['kind'][];
  callTreeContainsFunction?: string;
  visibility?: 'historyOnly' | 'includeHidden' | 'allForDebug';
}

export type RunPatchChange =
  | { type: 'upsertPayload'; payload: PayloadEvent }
  | { type: 'upsertDiagnostic'; diagnostic: RunDiagnostic }
  | { type: 'setStatus'; status: RunStatus }
  | {
      type: 'complete';
      outcome:
        | { status: 'succeeded'; result: RunResult }
        | { status: 'failed'; error: RunError }
        | { status: 'cancelled'; cancellation: RunCancellation }
        | { status: 'panicked'; error: RunError };
    };

export interface RunPatch {
  boundaryId: BoundaryId;
  cursor: RunCursor;
  changes: RunPatchChange[];
}

export type RequestCommandOutcome =
  | 'accepted'
  | 'alreadyResolved'
  | 'rejectedStale'
  | 'cancelled'
  | 'missing'
  | 'alreadyTerminal';

export type RunCursorExpiredReason =
  | 'expired'
  | 'compacted'
  | 'unknown'
  | 'future'
  | 'unavailable';

// ---------------------------------------------------------------------------
// WebSocket transport messages
// ---------------------------------------------------------------------------

/** Server -> client messages sent by `playground_ws.rs` over `/api/ws`. */
export type WebSocketOutMessage =
  | {
      type: 'hello';
      toolchainVersion: string;
      playgroundProtocol: number;
      minClientPlaygroundProtocol: number;
      capabilities: string[];
    }
  | { type: 'ready' }
  | { type: 'playgroundNotification'; notification: PlaygroundNotification }
  | { type: 'runStarted'; requestId?: number; run: Run }
  | { type: 'runPatch'; patch: RunPatch }
  | { type: 'commandAck'; requestId: number; outcome: string }
  | { type: 'commandError'; requestId: number; code: string; message: string }
  | { type: 'runList'; requestId: number; runs: RunSummary[] }
  | { type: 'historyList'; requestId: number; runs: RunSummary[] }
  | {
      type: 'executionList';
      requestId: number;
      executions: TelemetryExecution[];
      /** True when the project has no profile store yet: an empty state. */
      storeMissing?: boolean;
    }
  | {
      type: 'executionTelemetry';
      requestId: number;
      executionId: string;
      telemetry: ExecutionTelemetry;
    }
  | {
      type: 'telemetryMedia';
      requestId: number;
      cid: string;
      media: TelemetryMedia;
    }
  | {
      type: 'runSnapshot';
      requestId?: number;
      boundaryId: BoundaryId;
      snapshot: Run;
    }
  | ({ type: 'valueBody' } & ValueBodyResponse)
  | {
      type: 'runCursorExpired';
      requestId?: number;
      subscriptionId?: string;
      boundaryId: BoundaryId;
      reason: RunCursorExpiredReason;
    }
  | { type: 'envVarRequest'; id: number; variable: string }
  | { type: 'processEnvVars'; vars: Record<string, string> }
  | { type: 'envVarFromShell'; variable: string; value: string }
  | { type: 'knownEnvVarNames'; names: string[] }
  | {
      type: 'inputRequest';
      id: number;
      prompt: string | undefined;
      callId: number;
    }
  | { type: 'inputResolved'; id: number; callId: number }
  | {
      type: 'fetchLogNew';
      callId: number;
      id: number;
      method: string;
      url: string;
      requestHeaders: Record<string, string>;
      requestBody: string;
    }
  | {
      type: 'fetchLogUpdate';
      callId: number;
      logId: number;
      status?: number;
      durationMs?: number;
      responseBody?: string;
      error?: string;
      responseHeaders?: Record<string, string>;
    }
  | {
      type: 'controlFlowGraphResult';
      functionName: string;
      graph: ControlFlowGraph | null;
      requestId?: number;
    }
  | { type: 'cursorContext'; context: CursorContext };

/** Client -> server messages sent by `WebSocketRuntimePort` over `/api/ws`. */
export type WebSocketInMessage =
  | {
      type: 'startRun';
      requestId: number;
      project: string;
      functionName: string;
      argsBytes: string;
    }
  | {
      type: 'startPreviewRun';
      requestId: number;
      project: string;
      parentFunctionName: string;
      helper: string;
      functionName: string;
      argsBytes: string;
    }
  | {
      type: 'startTestRun';
      requestId: number;
      project: string;
      generation: number;
      testName: string;
    }
  | { type: 'cancelRun'; requestId: number; boundaryId: BoundaryId }
  | {
      type: 'respondToInput';
      requestId: number;
      boundaryId: BoundaryId;
      inputRequestId: string;
      value: string;
    }
  | {
      type: 'respondToEnv';
      requestId: number;
      boundaryId: BoundaryId;
      envRequestId: string;
      value?: string;
    }
  | { type: 'listRuns'; requestId: number; filter?: RunListFilter }
  | { type: 'listHistory'; requestId: number; filter?: RunListFilter }
  | { type: 'listExecutions'; requestId: number; project: string }
  | {
      type: 'readTelemetryMedia';
      requestId: number;
      project: string;
      cid: string;
    }
  | {
      type: 'openExecution';
      requestId: number;
      project: string;
      executionId: string;
    }
  | { type: 'openHistory'; requestId: number; boundaryId: BoundaryId }
  | { type: 'snapshot'; requestId: number; boundaryId: BoundaryId }
  | {
      type: 'readValue';
      requestId: number;
      boundaryId: BoundaryId;
      valueRef: ValueRef;
    }
  | {
      type: 'subscribe';
      requestId: number;
      subscriptionId: string;
      boundaryId: BoundaryId;
      afterCursor?: RunCursor;
    }
  | { type: 'unsubscribe'; requestId: number; subscriptionId: string }
  | {
      type: 'expandTestSet';
      project: string;
      generation: number;
      testsetName: string;
    }
  | {
      type: 'envVarResponse';
      id: number;
      value: string | undefined;
      variable?: string;
    }
  | { type: 'inputResponse'; id: number; value: string; callId: number }
  | { type: 'setEnvVar'; key: string; value: string }
  | { type: 'deleteEnvVar'; key: string }
  | { type: 'requestState' }
  | {
      type: 'ensureProjectRuntime';
      requestId: number;
      project: string;
      incarnation?: number;
    }
  | {
      type: 'releaseProjectRuntime';
      requestId: number;
      project: string;
      incarnation?: number;
    }
  | { type: 'requestCollectTests'; project: string }
  | {
      type: 'requestControlFlowGraph';
      project: string;
      functionName: string;
      requestId?: number;
    }
  | { type: 'cursorPosition'; file: string; line: number; column: number };

// ---------------------------------------------------------------------------
// Worker → Main thread messages
// ---------------------------------------------------------------------------

export type WorkerOutMessage =
  | { type: 'ready'; version?: string; commit?: string }
  | { type: 'playgroundNotification'; notification: PlaygroundNotification }
  | { type: 'diagnostics'; entries: DiagnosticEntry[] }
  | { type: 'runStarted'; requestId?: number; run: Run }
  | { type: 'runPatch'; patch: RunPatch }
  | {
      type: 'commandAck';
      requestId: number;
      outcome: RequestCommandOutcome | string;
    }
  | { type: 'commandError'; requestId: number; code: string; message: string }
  | { type: 'runList'; requestId: number; runs: RunSummary[] }
  | { type: 'historyList'; requestId: number; runs: RunSummary[] }
  | {
      type: 'executionList';
      requestId: number;
      executions: TelemetryExecution[];
      /** True when the project has no profile store yet: an empty state. */
      storeMissing?: boolean;
    }
  | {
      type: 'executionTelemetry';
      requestId: number;
      executionId: string;
      telemetry: ExecutionTelemetry;
    }
  | {
      type: 'telemetryMedia';
      requestId: number;
      cid: string;
      media: TelemetryMedia;
    }
  | {
      type: 'runSnapshot';
      requestId?: number;
      boundaryId: BoundaryId;
      snapshot: Run;
    }
  | ({ type: 'valueBody' } & ValueBodyResponse)
  | {
      type: 'runCursorExpired';
      requestId?: number;
      subscriptionId?: string;
      boundaryId: BoundaryId;
      reason: RunCursorExpiredReason;
    }
  | { type: 'fetchLogNew'; callId: number; entry: FetchLogEntry }
  | { type: 'fetchLogUpdate'; logId: number; patch: Partial<FetchLogEntry> }
  | { type: 'envVarRequest'; id: number; variable: string }
  | { type: 'processEnvVars'; vars: Record<string, string> }
  | { type: 'envVarFromShell'; variable: string; value: string }
  | { type: 'knownEnvVarNames'; names: string[] }
  | {
      type: 'inputRequest';
      id: number;
      prompt: string | undefined;
      callId: number;
    }
  | { type: 'inputResolved'; id: number; callId: number }
  | { type: 'vfsFileChanged'; path: string; content: string }
  | { type: 'vfsFileDeleted'; path: string }
  | { type: 'buildTime'; value: string }
  | {
      type: 'controlFlowGraphResult';
      functionName: string;
      graph: ControlFlowGraph | null;
      requestId?: number;
    }
  | { type: 'cursorContext'; context: CursorContext }
  | { type: 'logDecorations'; decorations: LogDecoration[] }
  | { type: 'clearLogDecorations' }
  | { type: 'wasmPanic'; message: string; stack?: string };

// ---------------------------------------------------------------------------
// Main thread → Worker messages
// ---------------------------------------------------------------------------

export type WorkerInMessage =
  | {
      type: 'startRun';
      requestId: number;
      project: string;
      functionName: string;
      argsBytes: Uint8Array;
    }
  | {
      type: 'startPreviewRun';
      requestId: number;
      project: string;
      parentFunctionName: string;
      helper: string;
      functionName: string;
      argsBytes: Uint8Array;
    }
  | {
      type: 'startTestRun';
      requestId: number;
      project: string;
      generation: number;
      testName: string;
    }
  | { type: 'cancelRun'; requestId: number; boundaryId: BoundaryId }
  | {
      type: 'respondToInput';
      requestId: number;
      boundaryId: BoundaryId;
      inputRequestId: string;
      value: string;
    }
  | {
      type: 'respondToEnv';
      requestId: number;
      boundaryId: BoundaryId;
      envRequestId: string;
      value?: string;
    }
  | { type: 'listRuns'; requestId: number; filter?: RunListFilter }
  | { type: 'listHistory'; requestId: number; filter?: RunListFilter }
  | { type: 'listExecutions'; requestId: number; project: string }
  | {
      type: 'readTelemetryMedia';
      requestId: number;
      project: string;
      cid: string;
    }
  | {
      type: 'openExecution';
      requestId: number;
      project: string;
      executionId: string;
    }
  | { type: 'openHistory'; requestId: number; boundaryId: BoundaryId }
  | { type: 'snapshot'; requestId: number; boundaryId: BoundaryId }
  | {
      type: 'readValue';
      requestId: number;
      boundaryId: BoundaryId;
      valueRef: ValueRef;
    }
  | {
      type: 'subscribe';
      requestId: number;
      subscriptionId: string;
      boundaryId: BoundaryId;
      afterCursor?: RunCursor;
    }
  | { type: 'unsubscribe'; requestId: number; subscriptionId: string }
  | {
      type: 'envVarResponse';
      id: number;
      value: string | undefined;
      variable?: string;
    }
  | { type: 'inputResponse'; id: number; value: string; callId: number }
  | { type: 'setEnvVar'; key: string; value: string }
  | { type: 'deleteEnvVar'; key: string }
  | { type: 'selectProject'; root: string }
  | { type: 'requestState' }
  /** Standing intent: the client wants a live runtime for this project.
   *  Unlike run commands this is not session-scoped — transports must
   *  re-assert the latest lease after every reconnect handshake. */
  | {
      type: 'ensureProjectRuntime';
      requestId: number;
      project: string;
      incarnation?: number;
    }
  | {
      type: 'releaseProjectRuntime';
      requestId: number;
      project: string;
      incarnation?: number;
    }
  | {
      type: 'requestControlFlowGraph';
      project: string;
      functionName: string;
      requestId?: number;
    }
  | { type: 'cursorPosition'; file: string; line: number; column: number }
  | { type: 'requestCollectTests'; project: string }
  | {
      type: 'expandTestSet';
      project: string;
      generation: number;
      testsetName: string;
    }
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
