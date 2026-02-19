// Execution panel + transport abstraction
export { ExecutionPanel } from './ExecutionPanel';
export type { ExecutionPanelProps } from './ExecutionPanel';
export type { RuntimePort } from './runtime-port';
export { WorkerRuntimePort } from './ports/WorkerRuntimePort';
export { WebSocketRuntimePort } from './ports/WebSocketRuntimePort';

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
