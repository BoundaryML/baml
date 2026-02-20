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

export interface ProjectUpdate {
  isBexCurrent: boolean;
  functions: string[];
}

export type PlaygroundNotification =
  | { type: 'listProjects'; projects: string[] }
  | { type: 'updateProject'; project: string; update: ProjectUpdate }
  | { type: 'openPlayground'; project: string; functionName?: string };

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
  fetchLogs: FetchLogEntry[];
  result: string | null;
  error: string | null;
  status: 'running' | 'success' | 'error';
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
  | { type: 'callFunctionResult'; id: number; result: Uint8Array }
  | { type: 'callFunctionError'; id: number; error: string }
  | { type: 'fetchLogNew'; entry: FetchLogEntry }
  | { type: 'fetchLogUpdate'; logId: number; patch: Partial<FetchLogEntry> }
  | { type: 'envVarRequest'; id: number; variable: string }
  | { type: 'vfsFileChanged'; path: string; content: string }
  | { type: 'vfsFileDeleted'; path: string }
  | { type: 'buildTime'; value: string };

// ---------------------------------------------------------------------------
// Main thread → Worker messages
// ---------------------------------------------------------------------------

export type WorkerInMessage =
  | { type: 'callFunction'; id: number; name: string; argsProto: Uint8Array; project: string }
  | { type: 'envVarResponse'; id: number; value: string | undefined; variable?: string }
  | { type: 'setEnvVar'; key: string; value: string }
  | { type: 'deleteEnvVar'; key: string }
  | { type: 'selectProject'; root: string }
  | { type: 'requestState' }
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
