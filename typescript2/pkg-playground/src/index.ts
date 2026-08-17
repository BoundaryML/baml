'use client';

// Execution panel + transport abstraction
export { ExecutionPanel } from './ExecutionPanel';
export type { ExecutionPanelProps } from './ExecutionPanel';
export type { RuntimePort } from './runtime-port';
export { WorkerRuntimePort } from './ports/WorkerRuntimePort';
export { WebSocketRuntimePort } from './ports/WebSocketRuntimePort';
export { createRunStoreClient } from './run-store-client';
export { applyRunPatch, createExecutionStore } from './execution-store';
export { decodeRunResultValue } from './run-store-projections';
export { createValueBodyCache } from './value-body-cache';
export type { ValueBodyCache } from './value-body-cache';
export { awaitRunCompletion } from './run-await';
export type { AwaitedRun } from './run-await';
export type {
  RunStoreClient,
  RunSubscriptionEvent,
  RunSubscriptionHandle,
  StartRunRequest,
  StartTestRunRequest,
} from './run-store-client';
export type {
  ExecutionStore,
  ExecutionStoreListener,
  ExecutionStoreSnapshot,
} from './execution-store';
export {
  createSessionStore,
  defaultSessionStore,
  browserSessionStoreStorage,
} from './session-store';
export type {
  EnvVars,
  SessionStore,
  SessionStoreSnapshot,
  SessionStoreStorage,
} from './session-store';
export {
  BOUNDARY_PROXY_URL_KEY,
  configureProxyEnvVar,
  getProxyEnvVarConfig,
} from './proxy-config';
export type { ProxyEnvVarConfig } from './proxy-config';
export { initPlaygroundEnv, setGatewayEnabled } from './gateway';
export {
  normalizeSerializedTestTree,
  parseSerializedTestTreeJson,
} from './serialized-test-tree';
export type {
  SerializedLazyTestSet,
  SerializedTest,
  SerializedTestDef,
  SerializedTestSet,
} from './serialized-test-tree';

// Result renderers: register custom React components per BAML type
export {
  registerResultRenderer,
  getBamlType,
  getResultRenderer,
  getRegisteredResultRenderers,
  BAML_TYPE_KEY,
  BAML_TYPE_FIELD,
} from './result-renderers';
export type { ResultRendererProps } from './result-renderers';
export { ResultDisplay } from './ResultDisplay';
export type { ResultDisplayProps } from './ResultDisplay';
export {
  HttpRequestCurlRenderer,
  httpRequestToCurl,
  isHttpRequest,
} from './renderers/HttpRequestCurl';

// Worker protocol types (needed by worker implementations and consumers)
export type {
  WorkerOutMessage,
  WorkerInMessage,
  WorkerInitMessage,
  WebSocketInMessage,
  WebSocketOutMessage,
  ControlFlowGraph,
  DiagnosticEntry,
  FetchLogEntry,
  EnvVarRequest,
  PlaygroundNotification,
  ProjectUpdate,
  Run,
  RunCursor,
  RunCursorExpiredReason,
  BoundaryId,
  RunPatch,
  RunPatchChange,
  RunStatus,
  RunSummary,
  RunTarget,
  LogDecoration,
  LogLevel,
  SourceNavigationTarget,
} from './worker-protocol';

// Utility
export { cn } from './lib/utils';

// WASM panic detection
export {
  installWasmPanicHook,
  onWasmPanic,
  isWasmPanic,
  getWasmError,
  getWasmPanicRegistry,
  handleWasmError,
  WasmPanicRegistry,
  type WasmPanic,
} from './wasm-panic';

// Observability (§9.3 BQF1 wire + /api/obs client + Telemetry tab)
export {
  decodeFrame,
  crc32c,
  asRunsList,
  asRunMeta,
  asTimeline,
  asLeftHeavy,
  asTopFunctions,
  asRecentCalls,
  asStatus,
  FrameKind,
  FOLD_ROW_FUNCTION,
  BqfDecodeError,
} from './obs/bqf1';
export type {
  BqfFrame,
  BqfColumn,
  RunsListColumns,
  RunMetaColumns,
  TimelineColumns,
  LeftHeavyColumns,
  TopFunctionsColumns,
  RecentCallsColumns,
  StatusColumns,
} from './obs/bqf1';
export { WsObserveClient, defaultObsUrl } from './obs/observe-client';
export type { ObsQueryMethod, ObsQueryParams } from './obs/observe-client';
export { ObsTelemetryTab } from './obs/TelemetryView';
export type { ObsTelemetryTabProps } from './obs/TelemetryView';
