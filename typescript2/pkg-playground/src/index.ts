// Execution panel + transport abstraction
export { ExecutionPanel } from './ExecutionPanel';
export type { ExecutionPanelProps } from './ExecutionPanel';
export type { RuntimePort } from './runtime-port';
export { WorkerRuntimePort } from './ports/WorkerRuntimePort';
export { WebSocketRuntimePort } from './ports/WebSocketRuntimePort';

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
export { HttpRequestCurlRenderer, httpRequestToCurl } from './renderers/HttpRequestCurl';

// Worker protocol types (needed by worker implementations and consumers)
export type {
  WorkerOutMessage,
  WorkerInMessage,
  WorkerInitMessage,
  DiagnosticEntry,
  FetchLogEntry,
  EnvVarRequest,
  PlaygroundNotification,
  ProjectUpdate,
  RunEntry,
} from './worker-protocol';
