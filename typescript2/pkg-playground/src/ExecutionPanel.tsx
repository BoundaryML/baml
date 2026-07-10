/**
 * ExecutionPanel — the right-side panel for running BAML functions.
 *
 * Displays available functions, accepts JSON arguments, executes them via
 * the WASM runtime, and shows fetch logs + results. Communicates with the
 * runtime through a transport-agnostic RuntimePort.
 *
 * Extracted from SplitPreview.tsx so it can be used standalone (e.g. in a
 * VS Code webview without an embedded Monaco editor).
 */

import type { ChangeEvent, FC, RefObject } from 'react';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { encodeRunArgs } from '@b/pkg-proto';
import type { BamlJsValue } from '@b/pkg-proto';
import {
  KeyRound,
  Loader2,
  PanelLeft,
  Play,
  RefreshCw,
  Settings,
  Square,
} from 'lucide-react';
import { Button } from './components/ui/button';
import { Tabs, TabsContent, TabsList, TabsTrigger } from './components/ui/tabs';
import { Input } from './components/ui/input';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from './components/ui/tooltip';
import { CodeBlock } from './components/ui/code-block';
import { ToggleGroup } from './components/ui/toggle-group';
import { cn } from './lib/utils';
import { ApiKeysDialog } from './components/ApiKeysDialog';
import { BOUNDARY_PROXY_URL_KEY, getProxyEnvVarConfig } from './proxy-config';
import { setGatewayEnabled } from './gateway';
import { useEnvVars } from './envAtoms';
import { CopyButton } from './components/CopyButton';
import { ErrorDisplay } from './components/ErrorDisplay';
import { MetadataBadges } from './components/MetadataBadges';
import { PromptStats } from './components/PromptStats';
import type { RuntimePort } from './runtime-port';
import type {
  ControlFlowGraph,
  CursorContext,
  DiagnosticEntry,
  FetchLogEntry,
  FunctionInfo,
  ProjectRuntimeStatus,
  ProjectUpdate,
  Run,
  BoundaryId,
  RunStatus,
  SourceNavigationTarget,
  WorkerOutMessage,
} from './worker-protocol';
import {
  ProjectPayloadFencer,
  acceptMonotonicEpoch,
  preparingRuntimeStatus,
  projectIdentityKey,
  runtimeIsReady,
  runtimeStatusFromUpdate,
  type ProjectIdentity,
} from './project-runtime-state';
import type { ResultRendererProps } from './result-renderers';
import { ArgsForm } from './ArgsForm';
import {
  defaultValueForSchema,
  isPlainObject,
  normalizeArgs,
  typeLookupFrom,
} from './args-form-model';
import { ResultDisplay } from './ResultDisplay';
import { ValueRenderer } from './ValueRenderer';
import { CapturedValueCard } from './CapturedValueCard';
import { registerBuiltinResultRenderers } from './renderers/registerBuiltins';
import {
  HttpRequestCurlRenderer,
  isHttpRequest,
} from './renderers/HttpRequestCurl';
import { GraphView } from './graph/GraphView';
import { FunctionSidebar } from './FunctionSidebar';
import { companionFunctionName } from './shared/companion-functions';
import { createExecutionStore, type ExecutionStore } from './execution-store';
import { createRunStoreClient } from './run-store-client';
import { createValueBodyCache } from './value-body-cache';
import type { ValueBodyCache } from './value-body-cache';
import type { ExecutionStoreSnapshot } from './execution-store';
import {
  decodeRunResultValue,
  runToTraceRows,
  runToDisplayRun,
  type RunTraceLog,
  type RunStoreDisplayRun,
} from './run-store-projections';
import {
  selectDefaultFunctionName,
  selectMainFunctionName,
} from './default-function-selection';
import { findLatestGraphRunSnapshot } from './graph-run-selection';
import { ExecutionProfileView } from './ExecutionProfileView';
import {
  parseSerializedTestTreeJson,
  type SerializedTestDef,
  type SerializedTestSet,
} from './serialized-test-tree';

registerBuiltinResultRenderers();

const LOGS_PANEL_DEFAULT_HEIGHT = 180;
const LOGS_PANEL_MIN_HEIGHT = 40;
const LOGS_PANEL_MAX_HEIGHT = 620;

const IS_MAC =
  typeof navigator !== 'undefined' && /Mac|iP/.test(navigator.platform);
/** Shown on the Run button; the actual binding is the panel-scoped keydown
 *  handler on the Tabs root. */
const RUN_SHORTCUT_HINT = IS_MAC ? '⌘↵' : 'Ctrl+↵';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function tryFormatJson(str: string): string {
  try {
    return JSON.stringify(JSON.parse(str), null, 2);
  } catch {
    /* not valid JSON */
    return str;
  }
}

function isTerminalRunStatus(status: RunStatus): boolean {
  return (
    status === 'succeeded' ||
    status === 'failed' ||
    status === 'cancelled' ||
    status === 'panicked'
  );
}

type RunScopedEnvRequest = { boundaryId: BoundaryId; envRequestId: string };
type PendingEnvDialogRequest = {
  variable: string;
  runScoped: RunScopedEnvRequest | null;
};

function findPendingEnvRequest(
  runs: Run[],
  requestId: string,
  variable: string,
): RunScopedEnvRequest | null {
  for (const run of runs) {
    for (const payload of run.payloads) {
      const kind = payload.kind;
      if (
        kind.type === 'envRequested' &&
        kind.requestId === requestId &&
        kind.key === variable &&
        kind.state === 'pending'
      ) {
        return { boundaryId: run.boundaryId, envRequestId: kind.requestId };
      }
    }
  }
  return null;
}

function stringifyResult(value: BamlJsValue): string {
  return JSON.stringify(
    value,
    (_, v) => (typeof v === 'bigint' ? v.toString() : v),
    2,
  );
}

type PendingTestTarget = {
  project: string;
  kind: 'test' | 'testset';
  name: string;
};

function testTargetMatches(candidate: string, target: string): boolean {
  return (
    candidate === target ||
    candidate.endsWith(`/${target}`) ||
    candidate.split('/').pop() === target
  );
}

function isExpandedTestSet(def: SerializedTestDef): def is SerializedTestSet {
  return Array.isArray((def as { items?: unknown }).items);
}

function findTestNameInTree(
  items: SerializedTestDef[],
  target: string,
): string | null {
  for (const item of items) {
    if ('type' in item && item.type === 'test') {
      if (testTargetMatches(item.name, target)) return item.name;
      continue;
    }
    if (isExpandedTestSet(item)) {
      const found = findTestNameInTree(item.items, target);
      if (found) return found;
    }
  }
  return null;
}

function collectAllTestNames(item: SerializedTestDef): string[] {
  if ('type' in item && item.type === 'test') return [item.name];
  if (!isExpandedTestSet(item)) return [];
  return item.items.flatMap(collectAllTestNames);
}

function collectTestNamesInSet(
  items: SerializedTestDef[],
  target: string,
): string[] | 'pending' | null {
  for (const item of items) {
    if ('type' in item && item.type === 'lazyTestSet') {
      if (testTargetMatches(item.name, target)) return 'pending';
      continue;
    }
    if (!isExpandedTestSet(item)) continue;
    if (testTargetMatches(item.name, target)) return collectAllTestNames(item);
    const nested = collectTestNamesInSet(item.items, target);
    if (nested !== null) return nested;
  }
  return null;
}

function hasLazyTestSets(items: SerializedTestDef[]): boolean {
  for (const item of items) {
    if ('type' in item && item.type === 'lazyTestSet') return true;
    if (isExpandedTestSet(item) && hasLazyTestSets(item.items)) return true;
  }
  return false;
}

function isInternalFunction(fn: FunctionInfo): boolean {
  return fn.origin != null && fn.origin !== 'userDefined';
}

function formatBuildTime(epochSecs: number): {
  absolute: string;
  relative: string;
} {
  const d = new Date(epochSecs * 1000);
  const absolute = d.toLocaleTimeString([], {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    hour12: false,
  });
  const delta = Math.floor(Date.now() / 1000) - epochSecs;
  let relative: string;
  if (delta < 60) relative = `${delta}s ago`;
  else if (delta < 3600) relative = `${Math.floor(delta / 60)}m ago`;
  else if (delta < 86400) relative = `${Math.floor(delta / 3600)}h ago`;
  else relative = `${Math.floor(delta / 86400)}d ago`;
  return { absolute, relative };
}

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

export interface ExecutionPanelProps {
  /** Transport-agnostic port for communicating with the BAML runtime. */
  port: RuntimePort;
  /** Dev-only: connection version so we can verify the port changed on restart. */
  connectionVersion?: number;
  /**
   * Optional custom result renderers: BAML type string -> React component.
   * E.g. { 'baml.http.Request': MyCurlComponent }.
   * Built-in renderers (e.g. curl for baml.http.Request) are always available.
   */
  resultRenderers?: Record<string, FC<ResultRendererProps>>;
  /** Called when user clicks the WASM panic banner to reload the worker. */
  onReload?: () => void;
  /** Called when user clicks an event with source location to jump to that line. */
  onNavigateToSource?: (source: SourceNavigationTarget) => void;
  /** Called whenever the selected project changes. */
  onSelectedProjectChange?: (project: string | null) => void;
  /** Tab shown on mount (default 'run'). Embedded views often want 'graph'. */
  initialTab?: 'run' | 'graph' | 'trace' | 'flame' | 'prompt' | 'curl';
  /** Auto-select this function once the project reports it (applied once). */
  initialFunctionName?: string;
  /** Auto-run this test once the test tree reports it (applied once). */
  initialTestName?: string;
  /** Auto-run tests under this testset once the test tree reports it (applied once). */
  initialTestsetName?: string;
  /** Seed for the args JSON editor (default '{}'). */
  initialArgsJson?: string;
  /** Per-function seeds for the args editor, keyed by bare or fully-qualified
      function name. Applied when a function is selected; args the user typed
      for a function this session take precedence over its seed. */
  argsByFunction?: Record<string, string>;
  /** Whether the function/tests sidebar starts open (default true). */
  initialSidebarOpen?: boolean;
}

// ---------------------------------------------------------------------------
// RequestUrlLabel — for requests routed through the playground proxy, show the
// original upstream URL (carried in the baml-original-url header) and note the
// proxy it went through, e.g. "https://api.anthropic.com/v1/messages
// (via https://proxy.promptfiddle.com)".
// ---------------------------------------------------------------------------

const ORIGINAL_URL_HEADER = 'baml-original-url';

function findHeaderValue(
  headers: Record<string, string> | null | undefined,
  name: string,
): string | undefined {
  if (!headers) return undefined;
  const lower = name.toLowerCase();
  for (const key of Object.keys(headers)) {
    if (key.toLowerCase() === lower) return headers[key];
  }
  return undefined;
}

const RequestUrlLabel: FC<{
  url: string;
  requestHeaders: Record<string, string> | null | undefined;
}> = ({ url, requestHeaders }) => {
  const original = findHeaderValue(requestHeaders, ORIGINAL_URL_HEADER);
  let display = url;
  let via: string | null = null;
  // Only de-proxy when the original URL came through unredacted.
  if (original && original !== '<redacted>') {
    try {
      const parsed = new URL(url);
      display = `${original.replace(/\/+$/, '')}${parsed.pathname}${parsed.search}`;
      via = parsed.origin;
    } catch {
      /* malformed URL — fall back to showing it verbatim */
    }
  }
  return (
    <span className="text-vsc-text flex-1 overflow-hidden text-ellipsis whitespace-nowrap text-[11px]">
      {display}
      {via && <span className="text-vsc-text-faint"> (via {via})</span>}
    </span>
  );
};

// ---------------------------------------------------------------------------
// CollectionDebugView — renders project/test discovery diagnostics without
// modeling discovery as a playground execution run.
// ---------------------------------------------------------------------------

interface CollectionDebugState {
  id: number;
  fetchLogs: FetchLogEntry[];
  error: string | null;
  status: 'success' | 'error';
}

interface CollectionDebugViewProps {
  state: CollectionDebugState;
  expandedLogId: number | null;
  setExpandedLogId: (id: number | null) => void;
}

const CollectionDebugView: FC<CollectionDebugViewProps> = ({
  state,
  expandedLogId,
  setExpandedLogId,
}) => {
  const hasError = state.status === 'error';
  const errorMessage = state.error || 'Unknown expansion error';
  return (
    <div className="flex-1 flex flex-col min-h-0">
      {/* Header */}
      <div className="flex items-center gap-1.5 px-2.5 py-1.5 bg-vsc-surface border-b border-vsc-border shrink-0">
        <span
          className={cn(
            'w-1.5 h-1.5 rounded-full shrink-0',
            hasError ? 'bg-vsc-red' : 'bg-vsc-green',
          )}
        />
        <span className="text-vsc-accent font-semibold text-[11px]">
          Test collection
        </span>
        <span className="text-vsc-text-faint text-[10px] flex-1">
          {hasError ? 'collection error' : 'collection fetch logs'}
        </span>
        <span className="text-vsc-text-faint text-[10px]">
          {state.fetchLogs.length} request
          {state.fetchLogs.length !== 1 ? 's' : ''}
        </span>
      </div>
      {/* Error message */}
      {hasError && (
        <div className="px-2.5 py-2 bg-vsc-surface border-b border-vsc-border">
          <div className="text-[10px] font-semibold text-red-500 mb-1 uppercase tracking-wide">
            Collection Error
          </div>
          <pre className="text-[11px] text-vsc-text whitespace-pre-wrap font-vsc-mono bg-vsc-bg p-2 rounded border border-vsc-border overflow-auto max-h-[300px]">
            {errorMessage}
          </pre>
        </div>
      )}
      {/* Fetch logs */}
      <div className="flex-1 overflow-auto font-vsc-mono text-xs bg-vsc-bg">
        {state.fetchLogs.length === 0 && !hasError && (
          <div className="p-5 text-center text-vsc-text-faint text-[11px]">
            No fetch logs — collection may not have made any HTTP requests
          </div>
        )}
        {state.fetchLogs.map((log) => {
          const isExp = expandedLogId === log.id;
          const statusColorCls =
            log.status === null
              ? 'text-vsc-text-muted'
              : log.status >= 200 && log.status < 300
                ? 'text-vsc-green'
                : log.status === 0
                  ? 'text-vsc-red'
                  : 'text-vsc-yellow';
          return (
            <div key={`cl-${log.id}`}>
              <div
                onClick={() => setExpandedLogId(isExp ? null : log.id)}
                className="flex items-center gap-1.5 py-0.5 pr-2.5 pl-[22px] cursor-pointer border-b border-vsc-border-subtle"
              >
                <span className={`${statusColorCls} font-semibold text-[11px]`}>
                  {log.status ?? '...'}
                </span>
                <span className="text-vsc-text-faint text-[10px]">
                  {log.method}
                </span>
                <RequestUrlLabel
                  url={log.url}
                  requestHeaders={log.requestHeaders}
                />
                {log.durationMs != null && (
                  <span className="text-vsc-text-faint text-[10px]">
                    {log.durationMs}ms
                  </span>
                )}
                <span className="text-vsc-text-faint text-[9px]">
                  {isExp ? '\u25B4' : '\u25BE'}
                </span>
              </div>
              {isExp && (
                <div className="py-2 pr-2.5 pl-[22px] flex flex-col gap-2 border-b border-vsc-border">
                  {log.error && (
                    <CodeBlock variant="error">{log.error}</CodeBlock>
                  )}
                  <div>
                    <div className="text-[10px] font-semibold text-vsc-text-muted mb-0.5 uppercase tracking-wide">
                      Request Headers
                    </div>
                    <CodeBlock>
                      {JSON.stringify(log.requestHeaders, null, 2)}
                    </CodeBlock>
                  </div>
                  {log.requestBody && (
                    <div>
                      <div className="text-[10px] font-semibold text-vsc-text-muted mb-0.5 uppercase tracking-wide">
                        Request Body
                      </div>
                      <CodeBlock>{tryFormatJson(log.requestBody)}</CodeBlock>
                    </div>
                  )}
                  {log.responseBody != null && (
                    <div>
                      <div className="text-[10px] font-semibold text-vsc-text-muted mb-0.5 uppercase tracking-wide">
                        Response Body
                      </div>
                      <CodeBlock>{tryFormatJson(log.responseBody)}</CodeBlock>
                    </div>
                  )}
                </div>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
};

const traceStatusClass = (status: Run['calls'][number]['status']): string => {
  switch (status) {
    case 'ok':
      return 'bg-vsc-green';
    case 'errored':
      return 'bg-vsc-red';
    case 'cancelled':
    case 'exited':
      return 'bg-vsc-yellow';
    case 'running':
      return 'bg-vsc-text-muted';
    default:
      status satisfies never;
      return 'bg-vsc-text-muted';
  }
};

function formatTraceMs(value: number | null): string {
  if (value == null) return '';
  if (value < 1) return `${value.toFixed(2)}ms`;
  if (value < 100) return `${value.toFixed(1)}ms`;
  return `${Math.round(value)}ms`;
}

function traceLogLevelClass(level: string | null): string {
  switch (level) {
    case 'error':
      return 'text-vsc-red';
    case 'warn':
      return 'text-vsc-yellow';
    case 'debug':
      return 'text-vsc-text-muted';
    case 'info':
    case null:
      return 'text-vsc-accent';
    default:
      return 'text-vsc-text-muted';
  }
}

function traceValueStateLabel(value: { state: RunTraceLog['state'] }): string | null {
  switch (value.state) {
    case 'available':
      return null;
    case 'loading':
      return 'loading';
    case 'pending':
      return 'pending';
    case 'omitted':
      return 'omitted';
    case 'truncated':
      return 'truncated';
    case 'missing':
      return 'missing';
    case 'lost':
      return 'lost';
    case 'error':
      return 'error';
    case 'unavailable':
      return 'unavailable';
    default:
      value.state satisfies never;
      return null;
  }
}

const TraceLogView: FC<{ log: RunTraceLog }> = ({ log }) => {
  const stateLabel = traceValueStateLabel(log);
  return (
    <div className="rounded border border-vsc-border-subtle bg-vsc-surface/60 px-2 py-1">
      <div className="flex min-w-0 items-center gap-1.5">
        <span
          className={cn(
            'font-vsc-mono text-[10px] uppercase',
            traceLogLevelClass(log.level),
          )}
        >
          {log.level ?? 'log'}
        </span>
        {log.sourceLine != null && (
          <span className="text-vsc-text-faint text-[10px]">
            :{log.sourceLine}
          </span>
        )}
        <span className="min-w-0 truncate text-vsc-text-muted text-[11px]">
          {log.message}
        </span>
        {stateLabel && (
          <span className="ml-auto shrink-0 rounded border border-vsc-border-subtle px-1 py-0.5 text-[10px] text-vsc-text-faint">
            {stateLabel}
          </span>
        )}
      </div>
      {log.value !== null && (
        <div className="mt-1 overflow-x-auto">
          <ValueRenderer value={log.value} displayMode="inline" />
        </div>
      )}
      {log.diagnostic && (
        <div className="mt-1 text-[10px] text-vsc-text-faint">
          {log.diagnostic}
        </div>
      )}
    </div>
  );
};

const TraceTimelineView: FC<{
  run: Run | undefined;
  valueBodyCache: ValueBodyCache;
}> = ({ run, valueBodyCache }) => {
  const rows = runToTraceRows(run, valueBodyCache);
  if (rows.length === 0) {
    return (
      <div className="flex-1 flex items-center justify-center text-vsc-text-faint text-xs bg-vsc-bg">
        No trace yet
      </div>
    );
  }

  return (
    <div className="flex-1 overflow-auto bg-vsc-bg font-vsc-mono text-xs">
      <div className="min-w-[560px] p-2">
        {rows.map((row) => (
          <div
            key={row.id}
            className="grid grid-cols-[72px_minmax(200px,1fr)_80px] gap-2 items-center border-b border-vsc-border-subtle py-1"
          >
            <div className="text-[10px] text-vsc-text-faint text-right">
              {formatTraceMs(row.offsetMs)}
            </div>
            <div className="min-w-0">
              <div
                className="flex items-center gap-1.5 min-w-0"
                style={{ paddingLeft: Math.min(row.depth, 12) * 12 }}
              >
                <span
                  className={cn(
                    'w-1.5 h-1.5 rounded-full shrink-0',
                    traceStatusClass(row.status),
                  )}
                />
                <span className="text-vsc-text truncate">
                  {row.functionName}
                </span>
                {row.sourceLine != null && (
                  <span className="text-vsc-text-faint text-[10px] shrink-0">
                    :{row.sourceLine}
                  </span>
                )}
              </div>
              <div className="relative mt-1 h-1.5 rounded bg-vsc-surface overflow-hidden">
                <div
                  className="absolute top-0 bottom-0 rounded bg-vsc-accent"
                  style={{
                    left: `${row.spanLeftPct}%`,
                    width: `${row.spanWidthPct}%`,
                  }}
                />
              </div>
              {row.logs.length > 0 && (
                <div
                  className="mt-1.5 space-y-1"
                  style={{ paddingLeft: Math.min(row.depth, 12) * 12 + 10 }}
                >
                  {row.logs.map((log) => (
                    <TraceLogView key={log.id} log={log} />
                  ))}
                </div>
              )}
              {row.callValues.length > 0 && (
                <div
                  className="mt-1.5 space-y-1"
                  style={{ paddingLeft: Math.min(row.depth, 12) * 12 + 10 }}
                >
                  {row.callValues.map((value) => (
                    <CapturedValueCard key={value.id} value={value} compact />
                  ))}
                </div>
              )}
            </div>
            <div className="text-[10px] text-vsc-text-faint">
              {formatTraceMs(row.durationMs)}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
};

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export const ExecutionPanel: FC<ExecutionPanelProps> = ({
  port,
  connectionVersion,
  resultRenderers,
  onReload,
  onNavigateToSource,
  onSelectedProjectChange,
  initialTab,
  initialFunctionName,
  initialTestName,
  initialTestsetName,
  initialArgsJson,
  argsByFunction,
  initialSidebarOpen = true,
}) => {
  const runStoreClient = useMemo(() => createRunStoreClient(port), [port]);
  const executionStore = useMemo(
    () => createExecutionStore(runStoreClient),
    [runStoreClient],
  );
  const valueBodyCache = useMemo(
    () => createValueBodyCache(runStoreClient),
    [runStoreClient],
  );
  const pendingExecutionStoreDisposalsRef = useRef(
    new Map<ExecutionStore, ReturnType<typeof setTimeout>>(),
  );
  const [executionSnapshot, setExecutionSnapshot] =
    useState<ExecutionStoreSnapshot>(() => executionStore.getSnapshot());
  const [valueBodyCacheVersion, setValueBodyCacheVersion] = useState(0);
  const [argsJsonByBoundaryId, setArgsJsonByBoundaryId] = useState<
    Record<string, string>
  >({});

  useEffect(() => {
    setExecutionSnapshot(executionStore.getSnapshot());
    return executionStore.subscribe(setExecutionSnapshot);
  }, [executionStore]);

  useEffect(
    () =>
      valueBodyCache.subscribe(() => {
        setValueBodyCacheVersion((version) => version + 1);
      }),
    [valueBodyCache],
  );

  useEffect(() => {
    const pendingDispose =
      pendingExecutionStoreDisposalsRef.current.get(executionStore);
    if (pendingDispose) {
      clearTimeout(pendingDispose);
      pendingExecutionStoreDisposalsRef.current.delete(executionStore);
    }

    return () => {
      // React StrictMode runs effect cleanup+setup once after mount in dev.
      // Defer disposal so the remount can keep the same render-created store.
      const timer = setTimeout(() => {
        pendingExecutionStoreDisposalsRef.current.delete(executionStore);
        executionStore.dispose();
      }, 0);
      pendingExecutionStoreDisposalsRef.current.set(executionStore, timer);
    };
  }, [executionStore]);

  const projectFencerRef = useRef(new ProjectPayloadFencer());
  const [projectCatalog, setProjectCatalog] = useState<ProjectIdentity[]>([]);
  const [runtimeSessionEpoch, setRuntimeSessionEpoch] = useState(0);
  const [projectUpdates, setProjectUpdates] = useState<
    Record<string, ProjectUpdate>
  >({});
  const [runtimeStates, setRuntimeStates] = useState<
    Record<string, ProjectRuntimeStatus>
  >({});
  const runtimeStatesRef = useRef(runtimeStates);
  const commitRuntimeStates = useCallback(
    (
      update: (
        previous: Record<string, ProjectRuntimeStatus>,
      ) => Record<string, ProjectRuntimeStatus>,
    ) => {
      const next = update(runtimeStatesRef.current);
      runtimeStatesRef.current = next;
      setRuntimeStates(next);
    },
    [],
  );
  const nextRuntimeRequestIdRef = useRef(Number.MAX_SAFE_INTEGER);
  const runtimeRequestsRef = useRef(
    new Map<
      number,
      { action: 'ensure' | 'retry'; identity: ProjectIdentity }
    >(),
  );
  const selectedRuntimeLeaseRef = useRef<ProjectIdentity | null>(null);
  const [testTree, setTestTree] = useState<SerializedTestDef[] | null>(null);
  const [testTreeStale, setTestTreeStale] = useState(false);
  const [collectionCallId, setCollectionCallId] = useState<number | null>(null);
  const [generation, setGeneration] = useState<number>(0);
  const [testStartErrors, setTestStartErrors] = useState<Map<string, string>>(
    new Map(),
  );
  const [failedExpands, setFailedExpands] = useState<Set<string>>(new Set());
  const testCollectionEpochsRef = useRef(new Map<string, number>());
  const pendingExpandsRef = useRef<{
    project: string | null;
    generation: number;
    names: Set<string>;
  }>({ project: null, generation: -1, names: new Set() });
  const [collectionDebug, setCollectionDebug] =
    useState<CollectionDebugState | null>(null);
  // When true, the main content area shows the collection run's fetch logs
  const [viewingCollection, setViewingCollection] = useState(false);
  // When true, the main content area shows the test run history panel
  const [viewingTestRun, setViewingTestRun] = useState(false);
  const [selectedProject, setSelectedProject] = useState<string | null>(null);
  const selectedProjectRef = useRef(selectedProject);
  useEffect(() => {
    selectedProjectRef.current = selectedProject;
  }, [selectedProject]);
  const [pendingTestTarget, setPendingTestTarget] =
    useState<PendingTestTarget | null>(null);

  const projectRoots = useMemo(
    () => projectCatalog.map((entry) => entry.project),
    [projectCatalog],
  );
  const selectedProjectIdentity = useMemo(
    () => projectCatalog.find((entry) => entry.project === selectedProject),
    [projectCatalog, selectedProject],
  );
  const selectedProjectIdentityKey = `${runtimeSessionEpoch}\u0000${projectIdentityKey(
    selectedProjectIdentity,
  )}`;

  const [selectedFn, setSelectedFn] = useState<string | null>(null);
  const [showInternalFunctions, setShowInternalFunctions] = useState(false);
  const [argsJson, setArgsJson] = useState(initialArgsJson ?? '{}');
  // Args editor mode. 'form' renders the schema-driven ArgsForm when the
  // selected function carries a param schema; 'raw' is the JSON input. With
  // no schema (`FunctionInfo.params === undefined`) raw is the only mode.
  const [argsMode, setArgsMode] = useState<'form' | 'raw'>('form');
  // Args the user typed for each function this session — restored (over the
  // `argsByFunction` seed) when they switch back to that function.
  const typedArgsByFnRef = useRef<Record<string, string>>({});

  const displayRuns = useMemo(
    () =>
      executionSnapshot.runs
        .map((run) => runToDisplayRun(run, argsJsonByBoundaryId, valueBodyCache))
        .filter(
          (run): run is RunStoreDisplayRun =>
            run != null,
        ),
    [executionSnapshot.runs, argsJsonByBoundaryId, valueBodyCache, valueBodyCacheVersion],
  );
  const functionRuns = useMemo(
    () =>
      displayRuns.filter(
        (run) =>
          run.kind === 'function' &&
          (!selectedProject || run.projectId === selectedProject),
      ),
    [displayRuns, selectedProject],
  );
  const testRuns = useMemo(
    () =>
      displayRuns.filter(
        (run) =>
          run.kind === 'test' &&
          (!selectedProject || run.projectId === selectedProject) &&
          run.projectGeneration === generation,
      ),
    [displayRuns, generation, selectedProject],
  );
  const testRunResults = useMemo(() => {
    const results = new Map<string, unknown>();
    for (const [testName, error] of testStartErrors) {
      results.set(testName, { outcome: 'error', error });
    }
    for (const run of testRuns) {
      if (!run.testName) continue;
      if (run.result != null) {
        results.set(run.testName, run.result);
      } else if (run.error) {
        results.set(run.testName, { outcome: 'error', error: run.error });
      } else if (run.status === 'cancelled') {
        results.set(run.testName, {
          outcome: 'error',
          error: 'Cancelled',
        });
      }
    }
    return results;
  }, [testRuns, testStartErrors]);
  const [expandedLogId, setExpandedLogId] = useState<number | null>(null);
  const outputRef = useRef<HTMLDivElement>(null);
  const promptContentRef = useRef<HTMLDivElement>(null);

  const [controlFlowGraph, setControlFlowGraph] =
    useState<ControlFlowGraph | null>(null);
  // CFGs for EVERY function in the project (prefetched) — powers the
  // workflow-root heuristic when a function is picked from the list.
  const workflowCfgCacheRef = useRef<Map<string, ControlFlowGraph>>(new Map());
  const workflowCfgResponsesRef = useRef<Map<string, ControlFlowGraph | null>>(
    new Map(),
  );
  const cfgRequestIdentitiesRef = useRef(
    new Map<
      string,
      Array<{ identity: ProjectIdentity; generation?: number | null }>
    >(),
  );
  const cfgDerivedEpochsRef = useRef(new Map<string, number>());
  const [workflowCacheVersion, setWorkflowCacheVersion] = useState(0);
  const [activeTab, setActiveTab] = useState<
    'run' | 'graph' | 'trace' | 'flame' | 'prompt' | 'curl'
  >(initialTab ?? 'run');
  const [highlightedNodeId, setHighlightedNodeId] = useState<number | null>(
    null,
  );
  const [cursorOffset, setCursorOffset] = useState<number | null>(null);

  // Workflow context: when a function belongs to multiple workflows,
  // this tracks which workflow is being viewed and the alternatives.
  const [workflowContext, setWorkflowContext] = useState<{
    functionName: string;
    workflows: string[];
  } | null>(null);
  const [promptPreviewResult, setPromptPreviewResult] =
    useState<BamlJsValue | null>(null);
  const [curlPreviewResult, setCurlPreviewResult] =
    useState<BamlJsValue | null>(null);
  const [promptPreviewError, setPromptPreviewError] = useState<string | null>(
    null,
  );
  const [curlPreviewError, setCurlPreviewError] = useState<string | null>(null);
  const [previewLoading, setPreviewLoading] = useState(false);
  const [sidebarOpen, setSidebarOpen] = useState(initialSidebarOpen);
  const [sidebarWidth, setSidebarWidth] = useState(168);
  const [showSettingsMenu, setShowSettingsMenu] = useState(false);
  const [logsPanelHeight, setLogsPanelHeight] = useState(
    LOGS_PANEL_DEFAULT_HEIGHT,
  );
  const resizingRef = useRef(false);
  const [resultModes, setResultModes] = useState<
    Record<string, 'parsed' | 'raw'>
  >({});
  const [runValidationError, setRunValidationError] = useState<string | null>(
    null,
  );

  const [showApiKeysDialog, setShowApiKeysDialog] = useState(false);
  const showApiKeysDialogRef = useRef(false);

  const [diagsExpanded, setDiagsExpanded] = useState(false);
  const [buildTime, setBuildTime] = useState<number | null>(null);
  const [wasmPanic, setWasmPanic] = useState<{
    message: string;
    stack?: string;
  } | null>(null);
  const {
    envVars,
    knownRequiredKeys,
    shellEnvVars,
    shellOverriddenKeys,
    shellDeletedKeys,
    addEnvVar,
    removeEnvVar,
    importEnvVars,
    addRequiredKey,
    addShellEnvVar,
    importShellEnvVars,
    revertToShell,
  } = useEnvVars(port);
  // In-flight worker requests waiting for a value. Ref because it doesn't drive renders.
  const pendingEnvRequestsRef = useRef<Map<number, PendingEnvDialogRequest>>(
    new Map(),
  );

  // Ref mirror of envVars so the message handler closure always sees current values.
  const envVarsRef = useRef(envVars);
  useEffect(() => {
    envVarsRef.current = envVars;
  }, [envVars]);
  const executionRunsRef = useRef(executionSnapshot.runs);
  useEffect(() => {
    executionRunsRef.current = executionSnapshot.runs;
    for (const [id, pending] of pendingEnvRequestsRef.current) {
      if (pending.runScoped) continue;
      const runScoped = findPendingEnvRequest(
        executionSnapshot.runs,
        String(id),
        pending.variable,
      );
      if (runScoped) {
        pendingEnvRequestsRef.current.set(id, { ...pending, runScoped });
      }
    }
  }, [executionSnapshot.runs]);

  useEffect(() => {
    onSelectedProjectChange?.(selectedProject);
  }, [onSelectedProjectChange, selectedProject]);

  // Ref mirrors for cursor context handler (avoids stale closures in port.onMessage).
  // workflowRouteFor is defined further down (it needs the function list);
  // the cursor handler only runs on messages, after the mirror is set.
  const workflowRouteForRef = useRef<
    (fn: string) => { roots: string[]; firstHop: Map<string, string> }
  >(() => ({ roots: [], firstHop: new Map() }));
  // Highlight to apply once the promoted workflow's graph arrives (the
  // selection-change effect clears highlights, so apply after, not before).
  const pendingHighlightRef = useRef<{ fn: string; nodeId: number } | null>(
    null,
  );
  const selectedFnRef = useRef(selectedFn);
  useEffect(() => {
    selectedFnRef.current = selectedFn;
  }, [selectedFn]);
  const controlFlowGraphRef = useRef(controlFlowGraph);
  useEffect(() => {
    controlFlowGraphRef.current = controlFlowGraph;
  }, [controlFlowGraph]);
  const graphNavigationRef = useRef<{
    nodeId: number;
    startOffset: number;
    endOffset: number;
    expiresAt: number;
  } | null>(null);

  // Buffer fetch logs by callId so logs that arrive before testCollectionResult are not lost.
  const pendingLogsRef = useRef<Map<number, FetchLogEntry[]>>(new Map());

  const nextRuntimeRequestId = useCallback(() => {
    const requestId = nextRuntimeRequestIdRef.current;
    nextRuntimeRequestIdRef.current -= 1;
    return requestId;
  }, []);

  const purgeSelectedProjectState = useCallback(
    (project: string) => {
      if (selectedProjectRef.current !== project) return;
      setSelectedFn(null);
      selectedFnRef.current = null;
      setWorkflowContext(null);
      setViewingCollection(false);
      setViewingTestRun(false);
      setPendingTestTarget(null);
      setTestTree(null);
      setTestTreeStale(false);
      setCollectionCallId(null);
      setGeneration(0);
      setTestStartErrors(new Map());
      setFailedExpands(new Set());
      setCollectionDebug(null);
      setControlFlowGraph(null);
      controlFlowGraphRef.current = null;
      setHighlightedNodeId(null);
      setPromptPreviewResult(null);
      setCurlPreviewResult(null);
      setPromptPreviewError(null);
      setCurlPreviewError(null);
      setPreviewLoading(false);
      workflowCfgCacheRef.current = new Map();
      workflowCfgResponsesRef.current = new Map();
      cfgRequestIdentitiesRef.current.clear();
      cfgDerivedEpochsRef.current.clear();
      pendingHighlightRef.current = null;
      graphNavigationRef.current = null;
      pendingLogsRef.current.clear();
      for (const key of testCollectionEpochsRef.current.keys()) {
        if (key.startsWith(`${project}\u0000`)) {
          testCollectionEpochsRef.current.delete(key);
        }
      }
      pendingExpandsRef.current = {
        project: null,
        generation: -1,
        names: new Set(),
      };
      typedArgsByFnRef.current = {};
      setArgsJson(initialArgsJson ?? '{}');
    },
    [initialArgsJson],
  );

  /**
   * Move the visible project and invalidate every project-qualified view in
   * the same event. Ref mirrors are updated before React commits so a late
   * message from the old project cannot observe the new project with stale
   * function/test/graph state.
   */
  const selectProject = useCallback(
    (next: string | null) => {
      const previous = selectedProjectRef.current;
      if (previous === next) return;
      if (previous) purgeSelectedProjectState(previous);
      selectedProjectRef.current = next;
      setSelectedProject(next);
    },
    [purgeSelectedProjectState],
  );

  const invalidateSelectedDerivedState = useCallback((project: string) => {
    if (selectedProjectRef.current !== project) return;
    // The prior collection remains useful context while the current source is
    // rebuilding (and especially when it is invalid), but it must never become
    // launchable again until a current-revision collection replaces it.
    setTestTreeStale(true);
    setTestStartErrors(new Map());
    setFailedExpands(new Set());
    setControlFlowGraph(null);
    controlFlowGraphRef.current = null;
    setHighlightedNodeId(null);
    workflowCfgCacheRef.current = new Map();
    workflowCfgResponsesRef.current = new Map();
    cfgRequestIdentitiesRef.current.clear();
    cfgDerivedEpochsRef.current.clear();
    pendingHighlightRef.current = null;
    graphNavigationRef.current = null;
    pendingLogsRef.current.clear();
    for (const key of testCollectionEpochsRef.current.keys()) {
      if (key.startsWith(`${project}\u0000`)) {
        testCollectionEpochsRef.current.delete(key);
      }
    }
    pendingExpandsRef.current = {
      project: null,
      generation: -1,
      names: new Set(),
    };
  }, []);

  const acceptControlFlowGraphResponse = useCallback(
    (
      functionName: string,
      sessionEpoch?: number,
      project?: string,
      projectIncarnation?: number,
      sourceRevision?: number,
      generation?: number,
      derivedEpoch?: number,
    ) => {
      if (!projectFencerRef.current.acceptSession(sessionEpoch)) return false;
      const queue = cfgRequestIdentitiesRef.current.get(functionName);
      const requested = queue?.shift();
      const requestedIdentity = requested?.identity;
      if (queue?.length === 0) {
        cfgRequestIdentitiesRef.current.delete(functionName);
      }
      const responseProject =
        project ?? requestedIdentity?.project ?? selectedProjectRef.current;
      if (!responseProject || responseProject !== selectedProjectRef.current) {
        return false;
      }
      const catalogIdentity =
        projectFencerRef.current.identity(responseProject);
      if (
        catalogIdentity?.incarnation !== undefined ||
        catalogIdentity?.sourceRevision !== undefined
      ) {
        if (
          projectIncarnation === undefined ||
          sourceRevision === undefined ||
          generation === undefined ||
          derivedEpoch === undefined
        ) {
          return false;
        }
        const currentGeneration =
          runtimeStatesRef.current[responseProject]?.generation;
        if (
          currentGeneration == null ||
          generation !== currentGeneration ||
          (requested?.generation != null &&
            generation !== requested.generation)
        ) {
          return false;
        }
        if (!projectFencerRef.current.accept(
          responseProject,
          projectIncarnation,
          sourceRevision,
        )) {
          return false;
        }
        return acceptMonotonicEpoch(
          cfgDerivedEpochsRef.current,
          [
            sessionEpoch,
            responseProject,
            projectIncarnation,
            sourceRevision,
            generation,
            functionName,
          ].join('\u0000'),
          derivedEpoch,
        );
      }
      if (
        projectIncarnation !== undefined ||
        sourceRevision !== undefined
      ) {
        return projectFencerRef.current.accept(
          responseProject,
          projectIncarnation,
          sourceRevision,
        );
      }
      if (!requestedIdentity) {
        return projectFencerRef.current.identity(responseProject)?.incarnation === undefined;
      }
      return projectFencerRef.current.accept(
        responseProject,
        requestedIdentity.incarnation,
        requestedIdentity.sourceRevision,
      );
    },
    [],
  );

  const requestControlFlowGraph = useCallback(
    (project: string, functionName: string) => {
      const identity = projectFencerRef.current.identity(project) ?? { project };
      const queue = cfgRequestIdentitiesRef.current.get(functionName) ?? [];
      queue.push({
        identity,
        generation: runtimeStatesRef.current[project]?.generation,
      });
      cfgRequestIdentitiesRef.current.set(functionName, queue);
      port.postMessage({
        type: 'requestControlFlowGraph',
        project,
        functionName,
      });
    },
    [port],
  );

  // Move the browser session's one selected-project lease. `requestState`
  // remains catalog/snapshot delivery only and never creates demand itself.
  useEffect(() => {
    const next = selectedProjectIdentity ?? null;
    const previous = selectedRuntimeLeaseRef.current;
    if (projectIdentityKey(previous ?? undefined) === projectIdentityKey(next ?? undefined)) {
      return;
    }

    if (previous) {
      port.postMessage({
        type: 'releaseProjectRuntime',
        requestId: nextRuntimeRequestId(),
        project: previous.project,
        incarnation: previous.incarnation,
      });
    }
    selectedRuntimeLeaseRef.current = null;

    if (!next) return;
    const requestId = nextRuntimeRequestId();
    runtimeRequestsRef.current.set(requestId, {
      action: 'ensure',
      identity: next,
    });
    selectedRuntimeLeaseRef.current = next;
    commitRuntimeStates((prev) => {
      const current = prev[next.project];
      if (
        current &&
        (next.sourceRevision === undefined ||
          current.requestedRevision >= next.sourceRevision)
      ) {
        return prev;
      }
      return {
        ...prev,
        [next.project]: preparingRuntimeStatus(next, current),
      };
    });
    port.postMessage({
      type: 'ensureProjectRuntime',
      requestId,
      project: next.project,
      incarnation: next.incarnation,
    });
  }, [
    nextRuntimeRequestId,
    port,
    commitRuntimeStates,
    selectedProjectIdentityKey,
  ]);

  useEffect(
    () => () => {
      const lease = selectedRuntimeLeaseRef.current;
      if (!lease) return;
      selectedRuntimeLeaseRef.current = null;
      port.postMessage({
        type: 'releaseProjectRuntime',
        requestId: nextRuntimeRequestId(),
        project: lease.project,
        incarnation: lease.incarnation,
      });
    },
    [nextRuntimeRequestId, port],
  );

  // ── Cursor context navigation ────────────────────────────────────────

  /** Build a lookup from sourceExpr → nodeId for the cached CFG.
   *  When multiple nodes share a sourceExpr, prefer semantic types
   *  (call/loop/branch/header) over structural ones (branchArm). */
  function graphNodeOwnerFunction(
    node: ControlFlowGraph['nodes'][string],
  ): string | null {
    return node.logFilterKey.split('|', 1)[0] || null;
  }

  function setSourceExprIndexEntry(
    map: Map<number, number>,
    sourceExpr: number,
    nodeId: number,
    nodeType: string,
    preferred: Set<string>,
  ) {
    if (preferred.has(nodeType) || !map.has(sourceExpr)) {
      map.set(sourceExpr, nodeId);
    }
  }

  function buildSourceExprIndexes(
    graph: ControlFlowGraph | null,
    functionName: string | null,
  ): { owner: Map<number, number>; any: Map<number, number> } {
    const owner = new Map<number, number>();
    const any = new Map<number, number>();
    if (!graph) return { owner, any };
    const preferred = new Set([
      'otherScope',
      'loop',
      'branchGroup',
      'headerContextEnter',
    ]);
    for (const [, node] of Object.entries(graph.nodes)) {
      if (node.sourceExpr == null) continue;
      setSourceExprIndexEntry(
        any,
        node.sourceExpr,
        node.id,
        node.nodeType,
        preferred,
      );
      if (functionName && graphNodeOwnerFunction(node) === functionName) {
        setSourceExprIndexEntry(
          owner,
          node.sourceExpr,
          node.id,
          node.nodeType,
          preferred,
        );
      }
    }
    return { owner, any };
  }

  /** Try each candidate expression ID (most-specific first) against the
   *  graph, returning the first node that matches. This gives "closest
   *  ancestor" highlighting — e.g. cursor on a local variable inside a
   *  call highlights the call; cursor on `if` keyword highlights the
   *  branch group; cursor inside a branch arm body highlights the arm. */
  function resolveCandidatesToNodeId(
    graph: ControlFlowGraph | null,
    candidates: number[],
    functionName: string | null,
  ): number | null {
    if (!graph || candidates.length === 0) return null;
    const index = buildSourceExprIndexes(graph, functionName);
    for (const exprId of candidates) {
      const nodeId = index.owner.get(exprId);
      if (nodeId != null) return nodeId;
    }
    for (const exprId of candidates) {
      const nodeId = index.any.get(exprId);
      if (nodeId != null) return nodeId;
    }
    return null;
  }

  function functionNameAliases(functionName: string): string[] {
    const shortName = functionName.split('.').pop();
    return shortName && shortName !== functionName
      ? [functionName, shortName]
      : [functionName];
  }

  function labelCalleeName(label: string): string {
    const trimmed = label.trim();
    const optionalCallStart = trimmed.indexOf('?.(');
    if (optionalCallStart >= 0)
      return trimmed.slice(0, optionalCallStart).trim();

    const callStart = trimmed.indexOf('(');
    if (callStart >= 0) return trimmed.slice(0, callStart).trim();

    return trimmed;
  }

  function nodeLabelMatchesFunctionName(
    label: string,
    functionName: string,
  ): boolean {
    const calleeName = labelCalleeName(label);
    return functionNameAliases(functionName).some(
      (name) => calleeName === name,
    );
  }

  function nodeMatchesFunctionName(
    node: ControlFlowGraph['nodes'][string],
    functionName: string,
  ): boolean {
    const aliases = functionNameAliases(functionName);
    if (node.calleeName && aliases.includes(node.calleeName)) return true;
    return nodeLabelMatchesFunctionName(node.label, functionName);
  }

  /** Find a graph node for `funcName` — used when candidate matching fails
   *  because the cursor is on a callee Path expression but the graph stores
   *  the full Call expression. Prefer runtime-provided callee metadata, with
   *  label matching only as a compatibility fallback for older graph payloads. */
  function resolveNodeByFunctionName(
    graph: ControlFlowGraph | null,
    funcName: string,
  ): number | null {
    if (!graph || !funcName) return null;
    for (const [, node] of Object.entries(graph.nodes)) {
      if (nodeMatchesFunctionName(node, funcName)) return node.id;
    }
    return null;
  }

  function handleGraphNodeClick(nodeId: number) {
    setHighlightedNodeId(nodeId);

    const source = (controlFlowGraph ?? controlFlowGraphRef.current)?.nodes[
      String(nodeId)
    ]?.sourceSpan;
    if (source && onNavigateToSource) {
      if (source.startOffset != null && source.endOffset != null) {
        graphNavigationRef.current = {
          nodeId,
          startOffset: source.startOffset,
          endOffset: source.endOffset,
          expiresAt: performance.now() + 1000,
        };
      }
      onNavigateToSource(source);
    }
  }

  function handleCursorContext(ctx: CursorContext) {
    // Update cursor offset for event highlighting (cursor ↔ event matching)
    setCursorOffset(ctx.cursorOffset ?? null);

    const graphNavigation = graphNavigationRef.current;
    if (
      graphNavigation &&
      performance.now() <= graphNavigation.expiresAt &&
      ctx.cursorOffset != null &&
      ctx.cursorOffset >= graphNavigation.startOffset &&
      ctx.cursorOffset <= graphNavigation.endOffset
    ) {
      setHighlightedNodeId(graphNavigation.nodeId);
      graphNavigationRef.current = null;
      return;
    }
    if (graphNavigation && performance.now() > graphNavigation.expiresAt) {
      graphNavigationRef.current = null;
    }

    if (!ctx.functionName) return;

    const currentFn = selectedFnRef.current;
    const cachedGraph = controlFlowGraphRef.current;

    const candidates = ctx.sourceExprCandidates ?? [];
    const sourceExprFunctionName =
      ctx.sourceExprFunctionName ?? ctx.functionName;
    const nodeId =
      resolveCandidatesToNodeId(
        cachedGraph,
        candidates,
        sourceExprFunctionName,
      ) ??
      (ctx.sourceExprId != null
        ? resolveNodeByFunctionName(cachedGraph, ctx.functionName)
        : null);
    const isCallSite = sourceExprFunctionName !== ctx.functionName;

    // Rule 1: cursor is on a node in the currently-displayed workflow
    if (nodeId != null && sourceExprFunctionName === currentFn) {
      setHighlightedNodeId(nodeId);
      return;
    }

    // Rule 2: cursor is on a call site inside the current workflow
    if (nodeId != null && ctx.workflowMemberships.includes(currentFn ?? '')) {
      setHighlightedNodeId(nodeId);
      return;
    }

    // Rule 3: cursor is on a call expression. Keep/show the function body
    // that owns the call so the top-level workflow remains the primary view.
    if (isCallSite) {
      if (sourceExprFunctionName !== currentFn) {
        setSelectedFn(sourceExprFunctionName);
        setViewingCollection(false);
        setViewingTestRun(false);
        setHighlightedNodeId(null);
      }
      setWorkflowContext(null);
      return;
    }

    // Rule 4: navigate to the function the cursor is on — always promoted to
    // a workflow that contains it. LineTotal inside Main→Review→ValidateInvoice
    // shows Main's graph, not LineTotal's, with the call-site node on the path
    // highlighted. When the helper belongs to several workflows, stay on the
    // current one if it's among them (else pick the first) and offer a
    // switcher above the graph. Whole-call-graph roots are preferred; the
    // worker's direct-caller membership info is the fallback.
    const route = workflowRouteForRef.current(ctx.functionName);
    const workflows =
      route.roots.length > 0 ? route.roots : ctx.workflowMemberships;
    const root =
      currentFn && workflows.includes(currentFn) ? currentFn : workflows[0];
    if (root) {
      // Prefer the node that calls the function under the cursor (its call
      // site survives in the expanded graph, e.g. relabeled by a //# header
      // above the function); fall back to the first hop from the root.
      const hop = route.firstHop.get(root) ?? ctx.functionName;
      const target =
        findCallSiteNode(root, ctx.functionName) ?? findCallSiteNode(root, hop);
      if (root !== currentFn) {
        pendingHighlightRef.current =
          target != null ? { fn: root, nodeId: target } : null;
        setSelectedFn(root);
        setViewingCollection(false);
        setViewingTestRun(false);
        setHighlightedNodeId(null);
      } else if (target != null) {
        setHighlightedNodeId(target);
      }
      setWorkflowContext(
        workflows.length > 1
          ? { functionName: ctx.functionName, workflows }
          : null,
      );
      return;
    }
    // Not part of any workflow — show the function's own graph.
    if (ctx.functionName !== currentFn) {
      setSelectedFn(ctx.functionName);
      setViewingCollection(false);
      setViewingTestRun(false);
      setHighlightedNodeId(null);
    }
    setWorkflowContext(null);
  }

  // ── Port message handler ─────────────────────────────────────────────

  useEffect(() => {
    const unsubscribe = port.onMessage((data: WorkerOutMessage) => {
      switch (data.type) {
        case 'runtimeSessionReset': {
          const selected = selectedProjectRef.current;
          if (selected) purgeSelectedProjectState(selected);
          projectFencerRef.current = new ProjectPayloadFencer(data.sessionEpoch);
          runtimeRequestsRef.current.clear();
          cfgRequestIdentitiesRef.current.clear();
          cfgDerivedEpochsRef.current.clear();
          testCollectionEpochsRef.current.clear();
          selectedRuntimeLeaseRef.current = null;
          runtimeStatesRef.current = {};
          setProjectCatalog([]);
          setProjectUpdates({});
          setRuntimeStates({});
          setRuntimeSessionEpoch(data.sessionEpoch);
          break;
        }

        case 'playgroundNotification': {
          const n = data.notification;
          if (!n) break;
          switch (n.type) {
            case 'listProjects': {
              if (!projectFencerRef.current.acceptSession(n.sessionEpoch)) break;
              const change = projectFencerRef.current.applyCatalog(
                n.projects ?? [],
                n.entries,
              );
              const catalogByProject = new Map(
                change.entries.map((entry) => [entry.project, entry]),
              );
              setProjectCatalog(change.entries);
              for (const [requestId, pending] of runtimeRequestsRef.current) {
                if (
                  projectIdentityKey(catalogByProject.get(pending.identity.project)) !==
                  projectIdentityKey(pending.identity)
                ) {
                  runtimeRequestsRef.current.delete(requestId);
                }
              }

              for (const project of change.purgedProjects) {
                purgeSelectedProjectState(project);
              }
              for (const project of change.advancedProjects) {
                invalidateSelectedDerivedState(project);
              }

              setProjectUpdates((prev) => {
                let changed = false;
                const next: Record<string, ProjectUpdate> = {};
                for (const [project, update] of Object.entries(prev)) {
                  const identity = catalogByProject.get(project);
                  const purgeForIdentity = change.purgedProjects.has(project);
                  const staleForRevision =
                    identity?.sourceRevision !== undefined &&
                    (update.sourceRevision === undefined ||
                      update.sourceRevision < identity.sourceRevision);
                  if (!identity || purgeForIdentity || staleForRevision) {
                    changed = true;
                    continue;
                  }
                  next[project] = update;
                }
                return changed ? next : prev;
              });

              commitRuntimeStates((prev) => {
                let changed = false;
                const next = { ...prev };
                for (const project of Object.keys(next)) {
                  if (!catalogByProject.has(project) || change.purgedProjects.has(project)) {
                    delete next[project];
                    changed = true;
                  }
                }
                for (const project of change.advancedProjects) {
                  const identity = catalogByProject.get(project);
                  if (!identity) continue;
                  const current = next[project];
                  if (
                    current &&
                    identity.sourceRevision !== undefined &&
                    current.requestedRevision >= identity.sourceRevision
                  ) {
                    continue;
                  }
                  next[project] =
                    selectedProjectRef.current === project
                      ? preparingRuntimeStatus(identity, current)
                      : {
                          ...preparingRuntimeStatus(identity, current),
                          state: 'idleStale',
                        };
                  changed = true;
                }
                return changed ? next : prev;
              });

              const selected = selectedProjectRef.current;
              selectProject(
                selected && catalogByProject.has(selected)
                  ? selected
                  : change.entries[0]?.project ?? null,
              );
              break;
            }
            case 'updateProject': {
              if (
                !projectFencerRef.current.acceptSession(n.sessionEpoch) ||
                !projectFencerRef.current.accept(
                  n.project,
                  n.update.projectIncarnation,
                  n.update.sourceRevision,
                )
              ) {
                break;
              }
              const status = runtimeStatusFromUpdate(n.update);
              const previousStatus = runtimeStatesRef.current[n.project];
              if (
                selectedProjectRef.current === n.project &&
                previousStatus &&
                ((runtimeIsReady(previousStatus) && !runtimeIsReady(status)) ||
                  (previousStatus.generation != null &&
                    status.generation != null &&
                    previousStatus.generation !== status.generation))
              ) {
                // Runtime-input changes can rebuild the same source revision.
                // Invalidate engine-derived UI on the runtime transition too;
                // the catalog's source-revision fence cannot observe it.
                invalidateSelectedDerivedState(n.project);
              }
              setProjectUpdates((prev) => ({ ...prev, [n.project]: n.update }));
              commitRuntimeStates((prev) => ({
                ...prev,
                [n.project]: status,
              }));
              break;
            }
            case 'testCollectionResult': {
              if (n.project !== selectedProjectRef.current) break;
              if (
                !projectFencerRef.current.acceptSession(n.sessionEpoch) ||
                !projectFencerRef.current.accept(
                  n.project,
                  n.projectIncarnation,
                  n.sourceRevision,
                )
              ) {
                break;
              }
              const currentRuntime = runtimeStatesRef.current[n.project];
              const collectionIdentity =
                projectFencerRef.current.identity(n.project);
              const qualifiedCollection =
                collectionIdentity?.incarnation !== undefined ||
                collectionIdentity?.sourceRevision !== undefined;
              if (
                qualifiedCollection &&
                (currentRuntime?.generation == null ||
                  n.collectionEpoch === undefined)
              ) {
                break;
              }
              if (
                currentRuntime?.generation != null &&
                currentRuntime.generation !== n.generation
              ) {
                break;
              }
              if (n.collectionEpoch !== undefined) {
                const epochKey = [
                  n.project,
                  collectionIdentity?.incarnation ?? 'legacy',
                  n.sourceRevision ??
                    collectionIdentity?.sourceRevision ??
                    'legacy',
                  n.generation,
                ].join('\u0000');
                if (
                  !acceptMonotonicEpoch(
                    testCollectionEpochsRef.current,
                    epochKey,
                    n.collectionEpoch,
                  )
                ) {
                  break;
                }
              }
              if (n.collectionError) {
                const buffered = pendingLogsRef.current.get(n.callId) ?? [];
                pendingLogsRef.current.delete(n.callId);
                setCollectionDebug({
                  id: n.callId,
                  fetchLogs: buffered,
                  error: n.collectionError,
                  status: 'error',
                });
                setPendingTestTarget(null);
                setViewingCollection(true);
                break;
              }
              try {
                const jsonStr = new TextDecoder().decode(
                  new Uint8Array(n.data),
                );
                const tree = parseSerializedTestTreeJson(jsonStr);

                // Track failed expansions from the server-provided error field
                if (n.expandError) {
                  setFailedExpands((prev) => {
                    const next = new Set(prev);
                    next.add(n.expandError!.testsetName);
                    return next;
                  });
                }

                setTestTree(tree);
                setTestTreeStale(false);
                setCollectionCallId(n.callId);
                setGeneration(n.generation);
                setTestStartErrors(new Map());

                // Create/replace collection debug state, hydrating any fetch
                // logs that arrived before this notification.
                const buffered = pendingLogsRef.current.get(n.callId) ?? [];
                pendingLogsRef.current.delete(n.callId);
                const hasError = !!n.expandError;
                const collectionState: CollectionDebugState = {
                  id: n.callId,
                  fetchLogs: buffered,
                  error: hasError ? n.expandError!.message : null,
                  status: hasError ? 'error' : 'success',
                };
                setCollectionDebug(collectionState);
              } catch (e) {
                console.error('[testCollectionResult] decode error:', e);
              }
              break;
            }
            case 'openPlayground':
              selectProject(n.project);
              if (n.functionName) {
                setWorkflowContext(null);
                selectedFnRef.current = n.functionName;
                setSelectedFn(n.functionName);
                setViewingCollection(false);
                setViewingTestRun(false);
              } else if (n.testName || n.testsetName) {
                setWorkflowContext(null);
                selectedFnRef.current = null;
                setSelectedFn(null);
                setViewingCollection(false);
                setViewingTestRun(true);
                setTestTree(null);
                setCollectionCallId(null);
                setCollectionDebug(null);
                setTestStartErrors(new Map());
                setPendingTestTarget({
                  project: n.project,
                  kind: n.testName ? 'test' : 'testset',
                  name: n.testName ?? n.testsetName!,
                });
              }
              break;
            case 'controlFlowGraphResult':
              if (
                !acceptControlFlowGraphResponse(
                  n.functionName,
                  n.sessionEpoch,
                  n.project,
                  n.projectIncarnation,
                  n.sourceRevision,
                  n.generation,
                  n.derivedEpoch,
                )
              ) {
                break;
              }
              workflowCfgResponsesRef.current.set(n.functionName, n.graph);
              setWorkflowCacheVersion((v) => v + 1);
              if (n.graph) {
                workflowCfgCacheRef.current.set(n.functionName, n.graph);
                // Only the selected function's graph drives the display —
                // prefetched graphs for other functions just fill the cache.
                if (n.functionName === selectedFnRef.current) {
                  setControlFlowGraph(n.graph);
                  const pending = pendingHighlightRef.current;
                  if (pending && pending.fn === n.functionName) {
                    pendingHighlightRef.current = null;
                    setHighlightedNodeId(pending.nodeId);
                  }
                }
              }
              break;
          }
          break;
        }

        case 'projectRuntimeState': {
          const pending = runtimeRequestsRef.current.get(data.requestId);
          runtimeRequestsRef.current.delete(data.requestId);
          if (!pending || pending.identity.project !== data.project) break;
          if (
            projectIdentityKey(projectFencerRef.current.identity(data.project)) !==
            projectIdentityKey(pending.identity)
          ) {
            break;
          }
          if (
            !projectFencerRef.current.accept(
              data.project,
              pending.identity.incarnation,
              data.state.requestedRevision,
            )
          ) {
            break;
          }
          commitRuntimeStates((prev) => ({
            ...prev,
            [data.project]: data.state,
          }));
          break;
        }

        case 'runStarted':
        case 'runPatch':
        case 'runList':
        case 'historyList':
        case 'runSnapshot':
        case 'valueBody':
        case 'runCursorExpired':
        case 'profileArtifactChunk':
          // RunStoreClient consumes these during the staged migration. The
          // legacy reducer keeps ignoring them until the UI cutover.
          break;

        case 'commandAck':
          runtimeRequestsRef.current.delete(data.requestId);
          break;

        case 'commandError': {
          const pending = runtimeRequestsRef.current.get(data.requestId);
          runtimeRequestsRef.current.delete(data.requestId);
          if (!pending) break;
          if (
            projectIdentityKey(projectFencerRef.current.identity(pending.identity.project)) !==
            projectIdentityKey(pending.identity)
          ) {
            break;
          }
          if (
            !projectFencerRef.current.accept(
              pending.identity.project,
              pending.identity.incarnation,
              pending.identity.sourceRevision,
            )
          ) {
            break;
          }
          commitRuntimeStates((prev) => {
            const current = prev[pending.identity.project];
            const failed: ProjectRuntimeStatus = {
              state: 'failed',
              requestedRevision:
                current?.requestedRevision ?? pending.identity.sourceRevision ?? 0,
              installedRevision: current?.installedRevision ?? null,
              generation: current?.generation ?? null,
              hasLastKnownGood: current?.hasLastKnownGood ?? false,
              error: data.message,
            };
            return { ...prev, [pending.identity.project]: failed };
          });
          break;
        }

        case 'fetchLogNew': {
          const logEntry = data.entry;
          // Always buffer by callId so logs that arrive before testCollectionResult are not lost.
          const existing = pendingLogsRef.current.get(data.callId);
          if (existing) {
            existing.push(logEntry);
          } else {
            pendingLogsRef.current.set(data.callId, [logEntry]);
          }
          // Route to collection run if callId matches
          setCollectionDebug((prev) => {
            if (prev && data.callId === prev.id) {
              return { ...prev, fetchLogs: [...prev.fetchLogs, logEntry] };
            }
            return prev;
          });
          break;
        }

        case 'fetchLogUpdate':
          // Also update collection run logs
          setCollectionDebug((prev) => {
            if (!prev) return prev;
            const updated = prev.fetchLogs.map((e) =>
              e.id === data.logId ? { ...e, ...data.patch } : e,
            );
            if (updated === prev.fetchLogs) return prev;
            return { ...prev, fetchLogs: updated };
          });
          break;

        case 'processEnvVars': {
          importShellEnvVars(data.vars);
          break;
        }

        case 'envVarFromShell': {
          addRequiredKey(data.variable);
          addShellEnvVar(data.variable, data.value);
          break;
        }

        case 'knownEnvVarNames': {
          for (const name of data.names) {
            addRequiredKey(name);
          }
          break;
        }

        case 'envVarRequest': {
          // Always track as a known required key (proactive indicator)
          addRequiredKey(data.variable);
          const cached = envVarsRef.current[data.variable];
          const runScoped = findPendingEnvRequest(
            executionRunsRef.current,
            String(data.id),
            data.variable,
          );
          if (cached !== undefined) {
            if (runScoped) {
              void executionStore
                .respondToEnv(runScoped.boundaryId, runScoped.envRequestId, cached)
                .catch((error) => {
                  console.warn('[ExecutionPanel] respondToEnv failed:', error);
                });
            } else {
              port.postMessage({
                type: 'envVarResponse',
                id: data.id,
                value: cached,
                variable: data.variable,
              });
            }
          } else {
            // Park the request — it will be resolved when the dialog closes
            pendingEnvRequestsRef.current.set(data.id, {
              variable: data.variable,
              runScoped,
            });
            if (!showApiKeysDialogRef.current) {
              setShowApiKeysDialog(true);
              showApiKeysDialogRef.current = true;
            }
          }
          break;
        }

        case 'inputRequest': {
          // Migrated run/test views render input prompts from RunStore payloads.
          // Legacy input frames are ignored here during the transport cutover.
          break;
        }

        case 'inputResolved': {
          // RunStore inputResolved payloads remove prompts from snapshots.
          break;
        }

        case 'ready':
          break;

        case 'buildTime':
          setBuildTime(Number(data.value) || null);
          break;

        case 'vfsFileChanged':
        case 'vfsFileDeleted':
        case 'diagnostics':
          break;

        case 'controlFlowGraphResult':
          if (
            !acceptControlFlowGraphResponse(
              data.functionName,
              data.sessionEpoch,
              data.project,
              data.projectIncarnation,
              data.sourceRevision,
              data.generation,
              data.derivedEpoch,
            )
          ) {
            break;
          }
          workflowCfgResponsesRef.current.set(data.functionName, data.graph);
          setWorkflowCacheVersion((v) => v + 1);
          if (data.graph) {
            workflowCfgCacheRef.current.set(data.functionName, data.graph);
            if (data.functionName === selectedFnRef.current) {
              setControlFlowGraph(data.graph);
              const pending = pendingHighlightRef.current;
              if (pending && pending.fn === data.functionName) {
                pendingHighlightRef.current = null;
                setHighlightedNodeId(pending.nodeId);
              }
            }
          }
          break;

        case 'cursorContext':
          handleCursorContext(data.context);
          break;

        case 'wasmPanic':
          setWasmPanic({ message: data.message, stack: data.stack });
          break;

        case 'logDecorations':
        case 'clearLogDecorations':
          // These are handled by MonacoEditor, ignore here
          break;

        default:
          data satisfies never;
      }
    });

    // Force the catalog after the listener is installed. This does not create
    // runtime demand; the selected-project lease effect above does that only
    // after the catalog chooses one project.
    port.postMessage({ type: 'requestState' });

    return unsubscribe;
  }, [
    acceptControlFlowGraphResponse,
    commitRuntimeStates,
    invalidateSelectedDerivedState,
    port,
    purgeSelectedProjectState,
    selectProject,
  ]);

  // Request control flow graph when selected function changes OR code is edited.
  // On function/project switch: clear the graph (shows loading state).
  // On code edit (projectUpdateVersion): keep old graph visible, swap when new one arrives.
  const prevGraphFnRef = useRef(selectedFn);
  const prevGraphProjectRef = useRef(selectedProject);
  const currentUpdate = selectedProject
    ? projectUpdates[selectedProject]
    : undefined;
  const projectUpdateVersion = currentUpdate;
  const selectedRuntimeStatus = selectedProject
    ? runtimeStates[selectedProject] ??
      (currentUpdate ? runtimeStatusFromUpdate(currentUpdate) : undefined)
    : undefined;
  const currentUpdateRuntimeStatus = currentUpdate
    ? runtimeStatusFromUpdate(currentUpdate)
    : undefined;
  const runtimeReady =
    runtimeIsReady(selectedRuntimeStatus) &&
    runtimeIsReady(currentUpdateRuntimeStatus) &&
    (currentUpdate?.sourceRevision === undefined ||
      selectedRuntimeStatus === undefined ||
      currentUpdate.sourceRevision >= selectedRuntimeStatus.requestedRevision);
  const runtimePreparing =
    selectedProject != null &&
    !runtimeReady &&
    (!selectedRuntimeStatus ||
      selectedRuntimeStatus.state === 'idleStale' ||
      selectedRuntimeStatus.state === 'building' ||
      selectedRuntimeStatus.state === 'ready');

  const retryCurrentRuntime = useCallback(() => {
    if (!selectedProjectIdentity) return;
    const requestId = nextRuntimeRequestId();
    runtimeRequestsRef.current.set(requestId, {
      action: 'retry',
      identity: selectedProjectIdentity,
    });
    commitRuntimeStates((prev) => {
      const next = {
        ...prev,
        [selectedProjectIdentity.project]: preparingRuntimeStatus(
          selectedProjectIdentity,
          prev[selectedProjectIdentity.project],
        ),
      };
      return next;
    });
    port.postMessage({
      type: 'retryProjectRuntime',
      requestId,
      project: selectedProjectIdentity.project,
      incarnation: selectedProjectIdentity.incarnation,
    });
  }, [
    commitRuntimeStates,
    nextRuntimeRequestId,
    port,
    selectedProjectIdentityKey,
  ]);

  useEffect(() => {
    const fnChanged = prevGraphFnRef.current !== selectedFn;
    const projChanged = prevGraphProjectRef.current !== selectedProject;
    prevGraphFnRef.current = selectedFn;
    prevGraphProjectRef.current = selectedProject;

    if (fnChanged || projChanged) {
      setControlFlowGraph(null);
      setHighlightedNodeId(null);
    }
    if (!selectedFn || !selectedProject || !runtimeReady) return;
    requestControlFlowGraph(selectedProject, selectedFn);
  }, [
    requestControlFlowGraph,
    selectedFn,
    selectedProject,
    projectUpdateVersion,
    runtimeReady,
  ]);

  // Clear preview results when selected function changes
  useEffect(() => {
    setPromptPreviewResult(null);
    setCurlPreviewResult(null);
    setPromptPreviewError(null);
    setCurlPreviewError(null);
    setPreviewLoading(false);
  }, [selectedFn]);

  // The authoritative args string for a function at any moment: args the
  // user typed for it this session win, then its `argsByFunction` seed
  // (exact or bare-name key, since selection may be namespaced), then the
  // panel seed. The swap effect writes this into argsJson on selection
  // change; effects that must not read the (one-commit-stale on selection
  // change) argsJson state read this instead.
  const baseArgsFor = useCallback(
    (fn: string) =>
      typedArgsByFnRef.current[fn] ??
      argsByFunction?.[fn] ??
      argsByFunction?.[fn.split('.').pop() ?? ''] ??
      initialArgsJson ??
      '{}',
    [argsByFunction, initialArgsJson],
  );

  // Swap the args editor when the selected function changes.
  const prevArgsFnRef = useRef(selectedFn);
  useEffect(() => {
    if (prevArgsFnRef.current === selectedFn) return;
    prevArgsFnRef.current = selectedFn;
    if (!selectedFn) return;
    setArgsJson(baseArgsFor(selectedFn));
  }, [selectedFn, baseArgsFor]);

  // Auto-refresh prompt/curl preview when args change while tab is active
  useEffect(() => {
    if (activeTab !== 'prompt' && activeTab !== 'curl') return;
    if (!selectedFn || !selectedProject || !runtimeReady) {
      setPreviewLoading(false);
      return;
    }

    const subFn = activeTab === 'prompt' ? 'render_prompt' : 'build_request';
    const setResult =
      activeTab === 'prompt' ? setPromptPreviewResult : setCurlPreviewResult;
    const setError =
      activeTab === 'prompt' ? setPromptPreviewError : setCurlPreviewError;

    // Clear previous error while loading (keep last result visible)
    setError(null);

    // Don't attempt if args are empty or not valid JSON
    if (!argsJson.trim()) {
      setPreviewLoading(false);
      return;
    }

    let parsed: unknown;
    try {
      parsed = JSON.parse(argsJson);
    } catch {
      setPreviewLoading(false);
      setError('Invalid JSON — fix args to preview');
      return;
    }

    if (
      typeof parsed !== 'object' ||
      parsed === null ||
      Array.isArray(parsed)
    ) {
      setPreviewLoading(false);
      setError('Args must be a JSON object');
      return;
    }

    setPreviewLoading(true);

    let cancelled = false;
    const timer = setTimeout(async () => {
      const waitForPreviewRun = (boundaryId: BoundaryId): Promise<Run> => {
        const existing = executionStore
          .getSnapshot()
          .runs.find((run) => run.boundaryId === boundaryId);
        if (existing && isTerminalRunStatus(existing.status)) {
          return Promise.resolve(existing);
        }

        return new Promise((resolve) => {
          let resolved = false;
          let shouldUnsubscribe = false;
          let unsubscribe: (() => void) | null = null;
          const finish = (run: Run) => {
            if (resolved) return;
            resolved = true;
            if (unsubscribe) {
              unsubscribe();
            } else {
              shouldUnsubscribe = true;
            }
            resolve(run);
          };

          unsubscribe = executionStore.subscribe((snapshot) => {
            const run = snapshot.runs.find((entry) => entry.boundaryId === boundaryId);
            if (run && isTerminalRunStatus(run.status)) {
              finish(run);
            }
          });
          if (shouldUnsubscribe) {
            unsubscribe();
          }
        });
      };

      try {
        const previewFunctionName = companionFunctionName(selectedFn, subFn);
        const argsBytes = encodeRunArgs(parsed as Record<string, unknown>);
        const boundaryId = await executionStore.startPreviewRun({
          project: selectedProject,
          parentFunctionName: selectedFn,
          helper: subFn,
          functionName: previewFunctionName,
          argsBytes: new Uint8Array(argsBytes),
        });
        const run = await waitForPreviewRun(boundaryId);
        if (run.error) {
          throw new Error(run.error.message);
        }
        if (run.status === 'cancelled') {
          throw new Error('Cancelled');
        }
        if (run.result?.valueRef) {
          await valueBodyCache.read(run.boundaryId, run.result.valueRef);
        }
        const resultValue = decodeRunResultValue(run, valueBodyCache);
        if (resultValue == null) {
          throw new Error('Preview completed without a result');
        }
        if (cancelled) return;
        setResult(resultValue);
        setError(null);
        setPreviewLoading(false);
      } catch (e) {
        if (cancelled) return;
        const errMsg = e instanceof Error ? e.message : String(e);
        // Don't clear result — keep last valid prompt visible with error banner above
        setError(errMsg);
        setPreviewLoading(false);
      }
    }, 500);

    return () => {
      cancelled = true;
      clearTimeout(timer);
      setPreviewLoading(false);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [
    activeTab,
    selectedFn,
    selectedProject,
    argsJson,
    port,
    executionStore,
    valueBodyCache,
    projectUpdateVersion,
    runtimeReady,
  ]);

  // Single write path for args edits (form and raw): the prompt/cURL preview
  // and run-history snapshots read `argsJson`, and per-function memory reads
  // `typedArgsByFnRef` — an edit that misses either silently desyncs them.
  const updateArgsJson = useCallback(
    (next: string) => {
      setArgsJson(next);
      if (selectedFn) typedArgsByFnRef.current[selectedFn] = next;
    },
    [selectedFn],
  );

  const onArgsJsonChange = useCallback(
    (e: ChangeEvent<HTMLInputElement>) => updateArgsJson(e.target.value),
    [updateArgsJson],
  );

  const onArgsFormChange = useCallback(
    (next: Record<string, unknown>) => updateArgsJson(JSON.stringify(next)),
    [updateArgsJson],
  );

  // ── Run function ───────────────────────────────────────────────────────

  const isRunning = functionRuns[0]?.status === 'running';

  const onCancelFunctionRun = useCallback(
    (boundaryId: BoundaryId) => {
      void executionStore.cancelRun(boundaryId).catch((error) => {
        console.warn('[ExecutionPanel] cancelRun failed:', error);
      });
    },
    [executionStore],
  );

  const submitRunInput = useCallback(
    (boundaryId: BoundaryId, inputRequestId: string, value: string) => {
      void executionStore
        .respondToInput(boundaryId, inputRequestId, value)
        .catch((error) => {
          console.warn('[ExecutionPanel] respondToInput failed:', error);
        });
    },
    [executionStore],
  );

  const toggleResultMode = useCallback((boundaryId: string) => {
    setResultModes((prev) => ({
      ...prev,
      [boundaryId]: (prev[boundaryId] ?? 'parsed') === 'parsed' ? 'raw' : 'parsed',
    }));
  }, []);

  const onResizeStart = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      resizingRef.current = true;
      const startX = e.clientX;
      const startWidth = sidebarWidth;

      const onMouseMove = (moveE: MouseEvent) => {
        const delta = moveE.clientX - startX;
        setSidebarWidth(Math.max(160, Math.min(400, startWidth + delta)));
      };
      const onMouseUp = () => {
        resizingRef.current = false;
        document.removeEventListener('mousemove', onMouseMove);
        document.removeEventListener('mouseup', onMouseUp);
      };
      document.addEventListener('mousemove', onMouseMove);
      document.addEventListener('mouseup', onMouseUp);
    },
    [sidebarWidth],
  );

  const onLogsResizeStart = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      const startY = e.clientY;
      const startHeight = logsPanelHeight;

      const onMouseMove = (moveE: MouseEvent) => {
        const delta = startY - moveE.clientY;
        const maxHeight = Math.max(
          LOGS_PANEL_MIN_HEIGHT,
          Math.min(LOGS_PANEL_MAX_HEIGHT, window.innerHeight - 220),
        );
        setLogsPanelHeight(
          Math.max(
            LOGS_PANEL_MIN_HEIGHT,
            Math.min(maxHeight, startHeight + delta),
          ),
        );
      };
      const onMouseUp = () => {
        document.removeEventListener('mousemove', onMouseMove);
        document.removeEventListener('mouseup', onMouseUp);
      };
      document.addEventListener('mousemove', onMouseMove);
      document.addEventListener('mouseup', onMouseUp);
    },
    [logsPanelHeight],
  );

  const onRunFunction = useCallback(async () => {
    if (!selectedFn || !selectedProject || isRunning || !runtimeReady) return;

    // Don't force the 'run' tab — running keeps the user on whatever tab
    // they're viewing (graph, trace, prompt, etc.).
    setExpandedLogId(null);
    setRunValidationError(null);

    requestAnimationFrame(() => {
      outputRef.current?.scrollTo({ top: 0, behavior: 'smooth' });
    });

    try {
      const parsed = JSON.parse(argsJson);
      if (
        typeof parsed !== 'object' ||
        parsed === null ||
        Array.isArray(parsed)
      ) {
        throw new Error(
          'Arguments must be a JSON object, e.g. {"arr": [3,1,2]}',
        );
      }
      const argsBytes = encodeRunArgs(parsed as Record<string, unknown>);

      const boundaryId = await executionStore.startRun({
        project: selectedProject,
        functionName: selectedFn,
        argsBytes: new Uint8Array(argsBytes),
      });
      setArgsJsonByBoundaryId((prev) => ({ ...prev, [boundaryId]: argsJson }));
    } catch (e) {
      const errMsg = e instanceof Error ? e.message : String(e);
      setRunValidationError(errMsg);
    }
  }, [
    selectedFn,
    selectedProject,
    argsJson,
    isRunning,
    executionStore,
    runtimeReady,
  ]);

  const handleRefreshTests = useCallback(() => {
    if (!selectedProject || !runtimeReady) return;
    port.postMessage({ type: 'requestCollectTests', project: selectedProject });
  }, [selectedProject, port, runtimeReady]);

  const appliedInitialTestTargetRef = useRef(false);
  useEffect(() => {
    if (
      appliedInitialTestTargetRef.current ||
      !selectedProject ||
      !runtimeReady ||
      (!initialTestName && !initialTestsetName)
    ) {
      return;
    }

    appliedInitialTestTargetRef.current = true;
    setWorkflowContext(null);
    setSelectedFn(null);
    setViewingCollection(false);
    setViewingTestRun(true);
    setTestTree(null);
    setCollectionCallId(null);
    setCollectionDebug(null);
    setTestStartErrors(new Map());
    setPendingTestTarget({
      project: selectedProject,
      kind: initialTestName ? 'test' : 'testset',
      name: initialTestName ?? initialTestsetName!,
    });
    port.postMessage({ type: 'requestCollectTests', project: selectedProject });
  }, [
    initialTestName,
    initialTestsetName,
    selectedProject,
    port,
    runtimeReady,
  ]);

  useEffect(() => {
    if (
      !pendingTestTarget ||
      !runtimeReady ||
      (testTree && !testTreeStale)
    ) {
      return;
    }
    if (pendingTestTarget.project !== selectedProject) return;
    port.postMessage({
      type: 'requestCollectTests',
      project: pendingTestTarget.project,
    });
  }, [
    pendingTestTarget,
    port,
    runtimeReady,
    selectedProject,
    testTree,
    testTreeStale,
  ]);

  const waitForTerminalRun = useCallback(
    (boundaryId: BoundaryId) => {
      const existing = executionStore
        .getSnapshot()
        .runs.find((run) => run.boundaryId === boundaryId);
      if (existing && isTerminalRunStatus(existing.status)) {
        return Promise.resolve();
      }

      return new Promise<void>((resolve) => {
        let resolved = false;
        let unsubscribe: (() => void) | null = null;

        const finish = () => {
          if (resolved) return;
          resolved = true;
          resolve();
          unsubscribe?.();
        };

        unsubscribe = executionStore.subscribe((snapshot) => {
          const run = snapshot.runs.find((entry) => entry.boundaryId === boundaryId);
          if (run && isTerminalRunStatus(run.status)) {
            finish();
          }
        });
        if (resolved) unsubscribe();
      });
    },
    [executionStore],
  );

  const handleRunTest = useCallback(
    async (name: string) => {
      if (!selectedProject || !runtimeReady || testTreeStale) return;
      // Switch to the test run view so the runs panel is visible even when no function is selected.
      setViewingTestRun(true);
      setViewingCollection(false);
      setTestStartErrors((prev) => {
        if (!prev.has(name)) return prev;
        const next = new Map(prev);
        next.delete(name);
        return next;
      });
      try {
        const boundaryId = await executionStore.startTestRun({
          project: selectedProject,
          generation,
          testName: name,
        });
        await waitForTerminalRun(boundaryId);
      } catch (e) {
        setTestStartErrors(
          (prev) =>
            new Map(prev).set(
              name,
              e instanceof Error ? e.message : String(e),
            ),
        );
      }
    },
    [
      executionStore,
      generation,
      selectedProject,
      waitForTerminalRun,
      runtimeReady,
      testTreeStale,
    ],
  );

  useEffect(() => {
    if (
      !pendingTestTarget ||
      !selectedProject ||
      !testTree ||
      !runtimeReady ||
      testTreeStale
    ) {
      return;
    }
    if (pendingTestTarget.project !== selectedProject) {
      setPendingTestTarget(null);
      setViewingTestRun(false);
      return;
    }

    const treeItems = testTree;
    const hasPendingLazyTestSets = hasLazyTestSets(treeItems);
    if (pendingTestTarget.kind === 'test') {
      const testName = findTestNameInTree(treeItems, pendingTestTarget.name);
      if (!testName) {
        if (hasPendingLazyTestSets) return;
        setPendingTestTarget(null);
        setViewingTestRun(false);
        return;
      }
      setPendingTestTarget(null);
      void handleRunTest(testName);
      return;
    }

    const testNames = collectTestNamesInSet(treeItems, pendingTestTarget.name);
    if (testNames === 'pending') return;
    if (testNames === null) {
      if (hasPendingLazyTestSets) return;
      setPendingTestTarget(null);
      setViewingTestRun(false);
      return;
    }
    setPendingTestTarget(null);
    setViewingTestRun(true);
    void (async () => {
      for (const testName of testNames) {
        await handleRunTest(testName);
      }
    })();
  }, [
    pendingTestTarget,
    selectedProject,
    testTree,
    handleRunTest,
    runtimeReady,
    testTreeStale,
  ]);

  // Auto-expand lazy testsets after receiving a new testTree
  useEffect(() => {
    if (!testTree || !selectedProject || !runtimeReady || testTreeStale) return;
    // Reset pending set and failed state when generation or project changes.
    // Generation is per-project on the server, so different projects can share
    // the same generation number — we must track both to avoid leaking state.
    if (
      pendingExpandsRef.current.generation !== generation ||
      pendingExpandsRef.current.project !== selectedProject
    ) {
      pendingExpandsRef.current = {
        project: selectedProject,
        generation,
        names: new Set(),
      };
      setFailedExpands(new Set());
    }
    const pending = pendingExpandsRef.current.names;
    const expandLazy = (items: SerializedTestDef[]) => {
      for (const item of items) {
        if ('type' in item && item.type === 'lazyTestSet' && !pending.has(item.name)) {
          pending.add(item.name);
          port.postMessage({
            type: 'expandTestSet',
            project: selectedProject,
            generation,
            testsetName: item.name,
          });
        } else if (isExpandedTestSet(item)) {
          // Recurse into expanded testsets to find nested lazy items
          expandLazy(item.items);
        }
      }
    };
    expandLazy(testTree);
  }, [
    testTree,
    selectedProject,
    generation,
    port,
    runtimeReady,
    testTreeStale,
  ]);

  // Retry expansion for a failed (or already expanded) testset
  const handleRetryExpand = useCallback(
    (testsetName: string) => {
      if (!selectedProject || !runtimeReady || testTreeStale) return;
      // Remove from failed set so it shows spinner again
      setFailedExpands((prev) => {
        const next = new Set(prev);
        next.delete(testsetName);
        return next;
      });
      // Remove from pending so auto-expand doesn't skip it
      pendingExpandsRef.current.names.delete(testsetName);
      // Re-send expansion request
      pendingExpandsRef.current.names.add(testsetName);
      port.postMessage({
        type: 'expandTestSet',
        project: selectedProject,
        generation,
        testsetName,
      });
    },
    [selectedProject, generation, port, runtimeReady, testTreeStale],
  );

  // ── Derived state ──────────────────────────────────────────────────────

  const isLoadingProject = selectedProject != null && currentUpdate == null;
  const functions: FunctionInfo[] = currentUpdate?.functions ?? [];
  const internalFunctionCount = functions.filter(isInternalFunction).length;
  const visibleFunctions = showInternalFunctions
    ? functions
    : functions.filter((fn) => !isInternalFunction(fn));
  const functionNames = visibleFunctions.map((f) => f.name);
  const diags = currentUpdate?.diagnostics ?? [];

  const selectedFnInfo = visibleFunctions.find((f) => f.name === selectedFn);
  const canPreviewPrompt = selectedFnInfo?.capabilities?.renderPrompt ?? false;
  const canPreviewCurl = selectedFnInfo?.capabilities?.buildRequest ?? false;

  // ── Args form wiring ─────────────────────────────────────────────────────
  // `undefined` = no schema shipped (old engine / extraction miss) → raw-only.
  const paramSchemas = selectedFnInfo?.params;
  const projectTypes = currentUpdate?.types;
  const typeLookup = useMemo(
    () => typeLookupFrom(projectTypes),
    [projectTypes],
  );
  // The form can only render args that parse to a plain JSON object; anything
  // else (mid-edit raw JSON, array) falls back to the raw input with a notice
  // instead of destroying the user's text.
  const parsedArgs = useMemo<Record<string, unknown> | null>(() => {
    try {
      const parsed: unknown = JSON.parse(argsJson);
      if (isPlainObject(parsed)) {
        return parsed;
      }
    } catch {
      // fall through
    }
    return null;
  }, [argsJson]);
  const showArgsForm =
    argsMode === 'form' && paramSchemas !== undefined && parsedArgs !== null;
  const argsFormUnavailable =
    argsMode === 'form' && paramSchemas !== undefined && parsedArgs === null;

  // Seed empty args with schema defaults, once per function per session. This
  // is what injects `$baml` class markers and required keys without the user
  // touching every field. Skipped when the function already has typed args or
  // a host seed. Deliberately does NOT read `argsJson`/`parsedArgs` — on the
  // render where `selectedFn` changes those still reflect the previous
  // function (the swap effect's setState lands next render), and reading them
  // here used to clobber just-restored host seeds with machine defaults.
  const seededFnsRef = useRef<Set<string>>(new Set());
  useEffect(() => {
    if (!selectedFn || !paramSchemas || paramSchemas.length === 0) return;
    if (seededFnsRef.current.has(selectedFn)) return;
    if (typedArgsByFnRef.current[selectedFn] !== undefined) return;
    try {
      const base: unknown = JSON.parse(baseArgsFor(selectedFn));
      if (!isPlainObject(base) || Object.keys(base).length > 0) {
        return; // a host seed exists — never overwrite it
      }
    } catch {
      return; // leave an unparseable host seed for the user to see and fix
    }
    seededFnsRef.current.add(selectedFn);
    const seeded: Record<string, unknown> = {};
    for (const param of paramSchemas) {
      if (!param.hasDefault) {
        seeded[param.name] = defaultValueForSchema(param.schema, typeLookup);
      }
    }
    // Write through the shared setter so the seed also lands in
    // typedArgsByFnRef — otherwise switching away and back restores '{}' and
    // the seeded defaults/markers are lost.
    updateArgsJson(JSON.stringify(seeded));
  }, [selectedFn, paramSchemas, typeLookup, baseArgsFor, updateArgsJson]);

  // Normalize wire markers once per function while form mode is active: bare
  // enum strings (hand-edited raw JSON, host seeds, pre-marker session
  // memory) and markerless class objects render as typed widgets but would
  // encode untyped (string / mapValue) — no String→Enum coercion exists on
  // the args path. Rewriting through the shared setter keeps argsJson
  // matching what the widgets display. Reads the authoritative args via
  // baseArgsFor, never argsJson state (stale on the selection-change
  // commit). Raw mode re-arms it, so hand edits get re-normalized on the
  // way back into form mode.
  const normalizeStateRef = useRef<{ fn: string | null; done: boolean }>({
    fn: null,
    done: false,
  });
  useEffect(() => {
    const state = normalizeStateRef.current;
    if (argsMode === 'raw') {
      state.done = false;
      return;
    }
    if (!selectedFn || !paramSchemas) return;
    if (state.fn === selectedFn && state.done) return;
    normalizeStateRef.current = { fn: selectedFn, done: true };
    let args: unknown;
    try {
      args = JSON.parse(baseArgsFor(selectedFn));
    } catch {
      return; // not form-renderable; the raw fallback shows it as-is
    }
    if (!isPlainObject(args)) return;
    const normalized = normalizeArgs(args, paramSchemas, typeLookup);
    if (normalized !== args) {
      updateArgsJson(JSON.stringify(normalized));
    }
  }, [
    argsMode,
    selectedFn,
    paramSchemas,
    typeLookup,
    baseArgsFor,
    updateArgsJson,
  ]);
  // Names of LLM functions — only these have a meaningful raw (un-parsed LLM
  // output) vs parsed distinction, so the Parsed/Raw toggle is shown only for
  // them. expr functions just return a structured value (raw == parsed).
  const llmFunctionNames = new Set(
    functions.filter((f) => f.kind === 'llm').map((f) => f.name),
  );
  const latestGraphRunSnapshot = useMemo(
    () =>
      findLatestGraphRunSnapshot(
        executionSnapshot.runs,
        selectedFn,
        selectedProject,
      ),
    [executionSnapshot.runs, selectedFn, selectedProject],
  );
  // The run-history/logs strip is lifted out of the sidebar+content row so it
  // spans the panel's full width; the row gets bottom padding to make room.
  const runLogsVisible =
    activeTab === 'run' &&
    !!selectedFn &&
    !viewingCollection &&
    !viewingTestRun;

  useEffect(() => {
    setSelectedFn((prev) =>
      prev && !functionNames.includes(prev) ? null : prev,
    );
  }, [functionNames]);

  // Prefetch every visible function's CFG once per project build so the
  // workflow-root heuristic can see the whole call graph. The responses
  // land in workflowCfgCacheRef (display is guarded by selectedFn).
  const prefetchedCfgRef = useRef<{ version: unknown; names: Set<string> }>({
    version: undefined,
    names: new Set(),
  });
  useEffect(() => {
    if (!selectedProject || !runtimeReady) return;
    const slot = prefetchedCfgRef.current;
    if (slot.version !== projectUpdateVersion) {
      slot.version = projectUpdateVersion;
      slot.names = new Set();
      workflowCfgCacheRef.current = new Map();
      workflowCfgResponsesRef.current = new Map();
    }
    for (const name of functionNames) {
      if (slot.names.has(name)) continue;
      slot.names.add(name);
      requestControlFlowGraph(selectedProject, name);
    }
  }, [
    functionNames,
    selectedProject,
    projectUpdateVersion,
    requestControlFlowGraph,
    runtimeReady,
  ]);

  // Reverse call map over the cached CFGs: callee -> the functions that
  // call it. calleeName may be bare while function names are qualified
  // (`main.illustrate`), so resolve by exact match or trailing segment.
  const workflowCallers = useMemo(() => {
    void workflowCacheVersion;
    const resolve = (callee: string): string | null =>
      functionNames.find(
        (n) =>
          n === callee || n.endsWith(`.${callee}`) || callee.endsWith(`.${n}`),
      ) ?? null;
    const callers = new Map<string, Set<string>>();
    for (const [fn, g] of workflowCfgCacheRef.current) {
      for (const node of Object.values(g.nodes)) {
        // calleeNames covers calls nested inside conditions/arguments;
        // calleeName alone only covers nodes that ARE a call.
        const names =
          node.calleeNames && node.calleeNames.length > 0
            ? node.calleeNames
            : node.calleeName
              ? [node.calleeName]
              : [];
        for (const raw of names) {
          const callee = resolve(raw);
          if (!callee || callee === fn) continue;
          let set = callers.get(callee);
          if (!set) {
            set = new Set();
            callers.set(callee, set);
          }
          set.add(fn);
        }
      }
    }
    return callers;
  }, [workflowCacheVersion, functionNames]);

  // The topmost workflows containing fn: walk the caller chain upward to
  // functions nobody else calls. Tests never appear — they are not in the
  // function list, so their call sites are not in the cache. The input may
  // be a bare name (cursor context) while the map keys are the qualified
  // list names, so canonicalize first. Alongside the roots, report each
  // root's "first hop" — the function the root calls on the path down to
  // fn — so the call-site node can be highlighted after promotion.
  const workflowRouteFor = useCallback(
    (rawFn: string): { roots: string[]; firstHop: Map<string, string> } => {
      const fn =
        functionNames.find(
          (n) =>
            n === rawFn || n.endsWith(`.${rawFn}`) || rawFn.endsWith(`.${n}`),
        ) ?? rawFn;
      const roots = new Set<string>();
      const firstHop = new Map<string, string>();
      const seen = new Set<string>([fn]);
      const stack: Array<{ node: string; via: string }> = [
        ...(workflowCallers.get(fn) ?? []),
      ].map((c) => ({ node: c, via: fn }));
      while (stack.length > 0) {
        const entry = stack.pop();
        if (!entry || seen.has(entry.node)) continue;
        seen.add(entry.node);
        const ups = workflowCallers.get(entry.node);
        if (!ups || ups.size === 0) {
          roots.add(entry.node);
          if (!firstHop.has(entry.node)) firstHop.set(entry.node, entry.via);
        } else {
          stack.push(...[...ups].map((u) => ({ node: u, via: entry.node })));
        }
      }
      return { roots: [...roots], firstHop };
    },
    [workflowCallers, functionNames],
  );
  useEffect(() => {
    workflowRouteForRef.current = workflowRouteFor;
  }, [workflowRouteFor]);

  // The node in rootFn's CFG whose expression calls hopFn — what to
  // highlight after promoting to the workflow root.
  const findCallSiteNode = useCallback(
    (rootFn: string, hopFn: string): number | null => {
      const g = workflowCfgCacheRef.current.get(rootFn);
      if (!g) return null;
      const matches = (raw: string) =>
        raw === hopFn || hopFn.endsWith(`.${raw}`) || raw.endsWith(`.${hopFn}`);
      let fallback: number | null = null;
      for (const node of Object.values(g.nodes)) {
        const names =
          node.calleeNames && node.calleeNames.length > 0
            ? node.calleeNames
            : node.calleeName
              ? [node.calleeName]
              : [];
        if (!names.some(matches)) continue;
        if (!node.isContainer) return node.id;
        fallback = fallback ?? node.id;
      }
      return fallback;
    },
    [],
  );

  // Apply the caller's initial function selection once, as soon as the
  // project reports a matching function. Function names may arrive
  // namespace-qualified (e.g. `main.illustrate`), so match by exact name
  // or trailing segment. Never overrides a user choice.
  const appliedInitialFnRef = useRef(false);
  useEffect(() => {
    if (appliedInitialFnRef.current || !initialFunctionName) return;
    const match = functionNames.find(
      (n) => n === initialFunctionName || n.endsWith(`.${initialFunctionName}`),
    );
    if (!match) return;
    appliedInitialFnRef.current = true;
    setSelectedFn((prev) => prev ?? match);
  }, [functionNames, initialFunctionName]);

  const defaultSelectionScopeRef = useRef<{
    project: string | null;
    update: ProjectUpdate | undefined;
    applied: boolean;
  }>({ project: null, update: undefined, applied: false });
  useEffect(() => {
    const scope = defaultSelectionScopeRef.current;
    if (scope.project !== selectedProject || scope.update !== projectUpdateVersion) {
      defaultSelectionScopeRef.current = {
        project: selectedProject,
        update: projectUpdateVersion,
        applied: false,
      };
    }

    const currentScope = defaultSelectionScopeRef.current;
    if (
      currentScope.applied ||
      selectedFn ||
      !selectedProject ||
      functionNames.length === 0 ||
      viewingCollection ||
      viewingTestRun
    ) {
      return;
    }

    if (initialFunctionName && !appliedInitialFnRef.current) {
      const initialMatch = functionNames.find(
        (n) => n === initialFunctionName || n.endsWith(`.${initialFunctionName}`),
      );
      if (initialMatch) return;
    }

    let next = selectMainFunctionName(functionNames);
    if (!next) {
      const graphResponses = workflowCfgResponsesRef.current;
      const hasAllGraphResponses = functionNames.every((name) =>
        graphResponses.has(name),
      );
      if (!hasAllGraphResponses) return;
      next = selectDefaultFunctionName(functionNames, workflowCfgCacheRef.current);
    }
    if (!next) return;

    currentScope.applied = true;
    setWorkflowContext(null);
    setSelectedFn(next);
    setViewingCollection(false);
    setViewingTestRun(false);
  }, [
    functionNames,
    initialFunctionName,
    projectUpdateVersion,
    selectedFn,
    selectedProject,
    viewingCollection,
    viewingTestRun,
    workflowCacheVersion,
  ]);

  // Reset active tab if current tab is no longer available for the selected function
  useEffect(() => {
    if (activeTab === 'prompt' && !canPreviewPrompt) setActiveTab('run');
    if (activeTab === 'curl' && !canPreviewCurl) setActiveTab('run');
  }, [activeTab, canPreviewPrompt, canPreviewCurl]);

  const errors = diags.filter((d) => d.severity === 'error');
  const warnings = diags.filter((d) => d.severity === 'warning');
  const hasErrors = errors.length > 0;
  const runtimeControlsDisabled = !runtimeReady || hasErrors;

  // Whether any known-required keys are missing — proactive, not just reactive to pending requests
  const hasMissingKeys = [...knownRequiredKeys].some((k) => !envVars[k]);

  // Workflow switcher bar — shown above the graph when the function under
  // the cursor belongs to more than one workflow. The graph always shows a
  // containing workflow; this bar switches between them.
  const workflowSwitcherBar = workflowContext && (
    <div className="flex items-center gap-1.5 px-2.5 py-1 text-[10px] bg-vsc-bg-secondary border-b border-vsc-border shrink-0">
      <span className="text-vsc-text-faint">Workflow:</span>
      {workflowContext.workflows.map((wf) => (
        <Button
          key={wf}
          variant={wf === selectedFn ? 'secondary' : 'outline'}
          size="sm"
          className="h-auto px-1.5 py-0.5 text-[10px]"
          onClick={() => {
            if (wf === selectedFn) return;
            const route = workflowRouteFor(workflowContext.functionName);
            const hop = route.firstHop.get(wf) ?? workflowContext.functionName;
            const target =
              findCallSiteNode(wf, workflowContext.functionName) ??
              findCallSiteNode(wf, hop);
            pendingHighlightRef.current =
              target != null ? { fn: wf, nodeId: target } : null;
            setSelectedFn(wf);
            setHighlightedNodeId(null);
          }}
        >
          {wf}
        </Button>
      ))}
    </div>
  );

  // ── Render ─────────────────────────────────────────────────────────────

  return (
    <>
      {buildTime != null && (
        <span data-testid="hot-reload-test" style={{ display: 'none' }}>
          {buildTime}
        </span>
      )}
      <Tabs
        value={activeTab}
        onValueChange={(v) => setActiveTab(v as typeof activeTab)}
        className="relative flex h-full min-h-0 w-full flex-1 flex-col gap-0 overflow-hidden"
        // Panel-scoped run shortcut: fires for focus anywhere inside the
        // playground (form fields, raw input, graph) without stealing
        // Cmd/Ctrl+Enter from the host's code editor.
        onKeyDown={(e) => {
          if (!((e.metaKey || e.ctrlKey) && e.key === 'Enter')) return;
          // Same gates as the Run buttons: no runs from the collection or
          // test-run views (where the run tab, history, and error strip are
          // all hidden — a run started here would be invisible), and never
          // over build errors. (onRunFunction can't check these itself —
          // hasErrors is derived after its declaration.) Don't swallow the
          // keystroke when nothing will run.
          if (viewingCollection || viewingTestRun) return;
          e.preventDefault();
          if (!runtimeControlsDisabled) void onRunFunction();
        }}
      >
        {/* ──── Combined top bar ──── */}
        <div className="flex items-center gap-1.5 px-2 py-1 shrink-0 border-b border-vsc-border bg-vsc-surface">
          <TooltipProvider>
            <Tooltip>
              <TooltipTrigger asChild>
                <Button
                  variant="ghost"
                  size="icon"
                  className="h-6 w-6 shrink-0"
                  onClick={() => setSidebarOpen((prev) => !prev)}
                >
                  <PanelLeft className="h-3.5 w-3.5 text-vsc-text-muted" />
                </Button>
              </TooltipTrigger>
              <TooltipContent>
                {sidebarOpen ? 'Hide sidebar' : 'Show sidebar'}
              </TooltipContent>
            </Tooltip>
          </TooltipProvider>

          {selectedFn && !viewingCollection && !viewingTestRun && (
            <>
              <span className="text-[11px] font-vsc-mono text-vsc-accent font-semibold whitespace-nowrap">
                {selectedFn}()
              </span>
              <TabsList className="bg-transparent border-b-0 ml-1 h-7">
                <TabsTrigger value="run" className="py-1 h-7">
                  Run
                </TabsTrigger>
                <TabsTrigger value="graph" className="py-1 h-7">
                  Graph
                </TabsTrigger>
                <TabsTrigger value="trace" className="py-1 h-7">
                  Trace
                </TabsTrigger>
                <TabsTrigger value="flame" className="py-1 h-7">
                  Flame
                </TabsTrigger>
                {canPreviewPrompt && (
                  <TabsTrigger value="prompt" className="py-1 h-7">
                    Prompt
                    {selectedFnInfo?.capabilities?.clientName && (
                      <span className="ml-1 px-1 py-0 text-[9px] rounded bg-vsc-bg-secondary text-vsc-text-faint">
                        {selectedFnInfo.capabilities.clientName}
                      </span>
                    )}
                  </TabsTrigger>
                )}
                {canPreviewCurl && (
                  <TabsTrigger value="curl" className="py-1 h-7">
                    cURL
                  </TabsTrigger>
                )}
              </TabsList>
            </>
          )}

          <div className="flex-1" />

          {projectRoots.length > 1 && (
            <ToggleGroup
              value={selectedProject ?? projectRoots[0]}
              onValueChange={selectProject}
              options={projectRoots.map((root) => ({
                value: root,
                label: (
                  <>
                    {root}
                    {projectUpdates[root] &&
                      !projectUpdates[root].isBexCurrent && (
                        <span className="ml-0.5 text-vsc-yellow">*</span>
                      )}
                  </>
                ),
              }))}
              size="sm"
            />
          )}

          {/* The primary Run button lives next to the args editor inside the
              Run tab; other tabs keep a compact icon so re-running while
              watching the graph/trace stays one click away. */}
          {selectedFn &&
            !viewingCollection &&
            !viewingTestRun &&
            activeTab !== 'run' && (
              <TooltipProvider>
                <Tooltip>
                  <TooltipTrigger asChild>
                    <Button
                      variant="success"
                      size="icon-xs"
                      className="h-7 w-7"
                      aria-label="Run"
                      disabled={
                        runtimeControlsDisabled || isRunning || !selectedProject
                      }
                      onClick={onRunFunction}
                    >
                      <Play />
                    </Button>
                  </TooltipTrigger>
                  <TooltipContent>
                    Run {selectedFn}() ({RUN_SHORTCUT_HINT})
                  </TooltipContent>
                </Tooltip>
              </TooltipProvider>
            )}

          <TooltipProvider>
            <Tooltip>
              <TooltipTrigger asChild>
                <Button
                  variant="ghost"
                  size="icon"
                  className="relative h-7 w-7 shrink-0"
                  onClick={() => setShowApiKeysDialog(true)}
                >
                  <KeyRound size={14} />
                  {hasMissingKeys && (
                    <span className="absolute -top-0.5 -right-0.5 w-2 h-2 rounded-full bg-yellow-400" />
                  )}
                </Button>
              </TooltipTrigger>
              <TooltipContent>
                <div className="flex flex-col gap-0.5">
                  <span>API Keys</span>
                  {connectionVersion != null && (
                    <span className="text-[9px] text-vsc-text-faint font-vsc-mono">
                      v{connectionVersion}
                    </span>
                  )}
                  {buildTime != null &&
                    (() => {
                      const { absolute, relative } = formatBuildTime(buildTime);
                      return (
                        <span className="text-[9px] text-vsc-text-faint font-vsc-mono">
                          {absolute} ({relative})
                        </span>
                      );
                    })()}
                  {projectRoots.length === 1 && (
                    <span className="text-[9px] text-vsc-text-faint font-vsc-mono">
                      {projectRoots[0]}
                    </span>
                  )}
                </div>
              </TooltipContent>
            </Tooltip>
          </TooltipProvider>

          {/* Settings (gear) menu */}
          <div className="relative shrink-0">
            <TooltipProvider>
              <Tooltip>
                <TooltipTrigger asChild>
                  <Button
                    variant="ghost"
                    size="icon"
                    className="h-7 w-7 shrink-0"
                    onClick={() => setShowSettingsMenu((v) => !v)}
                  >
                    <Settings size={14} />
                  </Button>
                </TooltipTrigger>
                <TooltipContent>Playground settings</TooltipContent>
              </Tooltip>
            </TooltipProvider>
            {showSettingsMenu && (
              <>
                <button
                  type="button"
                  aria-label="Close settings"
                  className="fixed inset-0 z-40 cursor-default bg-transparent border-none"
                  onClick={() => setShowSettingsMenu(false)}
                />
                <div className="absolute right-0 top-full mt-1 z-50 w-60 rounded border border-vsc-border bg-vsc-surface shadow-lg p-2.5">
                  <label className="flex items-center gap-1.5 text-[11px] text-vsc-text-muted cursor-pointer select-none">
                    <input
                      type="checkbox"
                      checked={showInternalFunctions}
                      onChange={(e) =>
                        setShowInternalFunctions(e.currentTarget.checked)
                      }
                      className="h-3 w-3 accent-vsc-accent"
                    />
                    <span>Show internal functions</span>
                    <span className="ml-auto font-vsc-mono text-vsc-text-faint">
                      {internalFunctionCount}
                    </span>
                  </label>
                </div>
              </>
            )}
          </div>
        </div>

        {/* WASM Panic banner */}
        {wasmPanic && (
          <button
            type="button"
            onClick={() => {
              setWasmPanic(null);
              if (onReload) {
                onReload();
              } else {
                window.location.reload();
              }
            }}
            className="w-full flex items-center gap-2 px-2.5 py-2 border-none border-b border-vsc-border shrink-0 bg-[#5c1a1a] hover:bg-[#6e1f1f] transition-colors cursor-pointer text-left"
          >
            <span className="text-[12px]">⚠️</span>
            <div className="flex-1 min-w-0">
              <span className="font-vsc-mono text-[11px] text-[#ff6b6b] font-medium">
                WASM Panic — Click to reload worker
              </span>
              <div className="font-vsc-mono text-[10px] text-[#ff6b6b]/70 truncate">
                {wasmPanic.message}
              </div>
            </div>
          </button>
        )}

        {/* Current-source runtime state. Catalog/source data renders
            immediately, but all runtime-derived controls stay disabled until
            the requested revision is installed. */}
        {selectedProject && !runtimeReady && (
          <div
            role="status"
            className="flex shrink-0 items-center gap-2 border-b border-vsc-border bg-vsc-surface px-2.5 py-2"
          >
            {runtimePreparing ? (
              <Loader2 className="h-3.5 w-3.5 shrink-0 animate-spin text-vsc-accent" />
            ) : (
              <span
                className={cn(
                  'h-2 w-2 shrink-0 rounded-full',
                  selectedRuntimeStatus?.state === 'failed'
                    ? 'bg-vsc-red'
                    : 'bg-vsc-yellow',
                )}
              />
            )}
            <div className="min-w-0 flex-1">
              <div className="font-vsc-mono text-[11px] text-vsc-text">
                {runtimePreparing
                  ? 'Preparing current build…'
                  : selectedRuntimeStatus?.state === 'blockedByDiagnostics'
                    ? 'Current build is blocked by diagnostics'
                    : 'Current build failed'}
              </div>
              {(selectedRuntimeStatus?.error ||
                selectedRuntimeStatus?.hasLastKnownGood) && (
                <div className="truncate font-vsc-mono text-[10px] text-vsc-text-faint">
                  {selectedRuntimeStatus?.error ??
                    'A last-known-good build exists, but new Run/Test actions require current source.'}
                </div>
              )}
            </div>
            {selectedRuntimeStatus?.state === 'failed' && (
              <Button
                variant="outline"
                size="xs"
                onClick={retryCurrentRuntime}
                className="shrink-0 text-[10px]"
              >
                <RefreshCw className="h-3 w-3" />
                Retry build
              </Button>
            )}
          </div>
        )}

        {/* Diagnostics banner */}
        {hasErrors && (
          <div className="border-b border-vsc-border shrink-0 bg-[#3e1a1a]">
            {diags.length > 0 ? (
              <>
                <button
                  type="button"
                  onClick={() => setDiagsExpanded((v) => !v)}
                  className="w-full flex items-center gap-1 px-2.5 py-1 bg-transparent border-none cursor-pointer text-left"
                >
                  <span
                    className="text-[10px] text-[#f48771] select-none transition-transform duration-150"
                    style={{
                      display: 'inline-block',
                      transform: diagsExpanded
                        ? 'rotate(90deg)'
                        : 'rotate(0deg)',
                    }}
                  >
                    ▶
                  </span>
                  <span className="font-vsc-mono text-[10px] text-[#f48771]">
                    {errors.length > 0
                      ? `${errors.length} error${errors.length !== 1 ? 's' : ''}`
                      : ''}
                    {errors.length > 0 && warnings.length > 0 ? ', ' : ''}
                    {warnings.length > 0
                      ? `${warnings.length} warning${warnings.length !== 1 ? 's' : ''}`
                      : ''}
                    {selectedRuntimeStatus?.hasLastKnownGood
                      ? ' — last successful build retained'
                      : ' — current build unavailable'}
                  </span>
                </button>
                {diagsExpanded && (
                  <div className="px-2.5 pb-1.5 flex flex-col gap-0.5 max-h-[200px] overflow-y-auto">
                    {errors.map((e, i) => (
                      <div
                        key={`e${i}`}
                        className="font-vsc-mono text-[10px] text-[#f48771]/80 pl-3.5 break-words whitespace-pre-wrap"
                      >
                        {e.message}
                      </div>
                    ))}
                    {warnings.map((w, i) => (
                      <div
                        key={`w${i}`}
                        className="font-vsc-mono text-[10px] text-[#cca700]/80 pl-3.5 break-words whitespace-pre-wrap"
                      >
                        {w.message}
                      </div>
                    ))}
                  </div>
                )}
              </>
            ) : null}
          </div>
        )}

        {/* Main layout: sidebar + content. When the run-history strip is
            visible it is absolutely positioned across the panel's full
            width, so the row ends above it. */}
        <div
          className="flex flex-1 min-h-0"
          style={{
            paddingBottom: runLogsVisible ? logsPanelHeight + 6 : 0,
          }}
        >
          {/* Sidebar */}
          {sidebarOpen && (
            <>
              <div
                className="shrink-0 overflow-hidden"
                style={{ width: sidebarWidth }}
              >
                <FunctionSidebar
                  functions={visibleFunctions}
                  showInternalFunctions={showInternalFunctions}
                  internalFunctionCount={internalFunctionCount}
                  isLoadingProject={isLoadingProject}
                  runtimeControlsDisabled={runtimeControlsDisabled}
                  testTree={testTree}
                  testTreeStale={testTreeStale}
                  selectedFn={selectedFn}
                  onSelectFn={(fn) => {
                    setViewingCollection(false);
                    setViewingTestRun(false);
                    setHighlightedNodeId(null);
                    setWorkflowContext(null);
                    setSelectedFn(fn);
                  }}
                  onRefreshTests={handleRefreshTests}
                  onRunTest={handleRunTest}
                  testRunResults={testRunResults}
                  failedExpands={failedExpands}
                  onRetryExpand={handleRetryExpand}
                  collectionLogCount={collectionDebug?.fetchLogs.length ?? 0}
                  viewingCollection={viewingCollection}
                  onSelectCollectionView={() => {
                    setViewingCollection(true);
                    setViewingTestRun(false);
                    setSelectedFn(null);
                  }}
                />
              </div>
              <div
                onMouseDown={onResizeStart}
                className="w-1 shrink-0 cursor-col-resize hover:bg-vsc-accent/30 transition-colors border-r border-vsc-border"
              />
            </>
          )}

          {/* Content area */}
          <div className="flex-1 flex flex-col min-h-0 min-w-0">
            {viewingCollection && collectionDebug ? (
              <CollectionDebugView
                state={collectionDebug}
                expandedLogId={expandedLogId}
                setExpandedLogId={setExpandedLogId}
              />
            ) : viewingTestRun ? (
              <div
                ref={outputRef}
                className="flex-1 overflow-auto font-vsc-mono text-xs bg-vsc-bg"
              >
                {testRuns.length === 0 && (
                  <div className="p-5 text-center text-vsc-text-faint text-[11px]">
                    No test runs yet
                  </div>
                )}
                {testRuns.map((run, boundaryIdx) => {
                  const isLatest = boundaryIdx === 0;
                  const statusCls =
                    run.status === 'error'
                      ? 'bg-vsc-red'
                      : run.status === 'success'
                        ? 'bg-vsc-green'
                        : run.status === 'cancelled'
                          ? 'bg-vsc-yellow'
                          : 'bg-vsc-text-muted';
                  return (
                    <div
                      key={run.id}
                      className={
                        !isLatest ? 'border-b-2 border-vsc-border' : ''
                      }
                    >
                      <div className="flex items-center gap-1.5 px-2.5 py-1.5 bg-vsc-surface border-b border-vsc-border-subtle">
                        <span
                          className={`w-1.5 h-1.5 rounded-full shrink-0 ${statusCls}`}
                        />
                        <span className="text-vsc-accent font-semibold text-[11px]">
                          {run.testName ?? run.functionName}
                        </span>
                        {run.status === 'running' && (
                          <>
                            <span className="text-vsc-text-muted text-[10px]">
                              running...
                            </span>
                            <TooltipProvider>
                              <Tooltip>
                                <TooltipTrigger asChild>
                                  <Button
                                    variant="ghost"
                                    size="icon"
                                    className="h-5 w-5 text-vsc-text-muted hover:text-vsc-error"
                                    onClick={() => onCancelFunctionRun(run.id)}
                                  >
                                    <Square size={12} />
                                  </Button>
                                </TooltipTrigger>
                                <TooltipContent>
                                  Cancel execution
                                </TooltipContent>
                              </Tooltip>
                            </TooltipProvider>
                          </>
                        )}
                        {run.durationMs != null && (
                          <span className="text-vsc-text-faint text-[10px] shrink-0">
                            {run.durationMs}ms
                          </span>
                        )}
                      </div>
                      {run.rootInput != null && (
                        <div className="py-1.5 pr-2.5 pl-[22px] border-b border-vsc-border-subtle">
                          <div className="text-[10px] font-semibold text-vsc-text-muted mb-0.5 uppercase tracking-wide">
                            Input
                          </div>
                          <ResultDisplay
                            result={run.rootInput}
                            customRenderers={resultRenderers}
                          />
                        </div>
                      )}
                      {run.fetchLogs.map((log) => {
                        const isExp = expandedLogId === log.id;
                        const statusColorCls =
                          log.status === null
                            ? 'text-vsc-text-muted'
                            : log.status >= 200 && log.status < 300
                              ? 'text-vsc-green'
                              : log.status === 0
                                ? 'text-vsc-red'
                                : 'text-vsc-yellow';
                        return (
                          <div key={`t-${log.id}`}>
                            <div
                              onClick={() =>
                                setExpandedLogId(isExp ? null : log.id)
                              }
                              className="flex items-center gap-1.5 py-0.5 pr-2.5 pl-[22px] cursor-pointer border-b border-vsc-border-subtle"
                            >
                              <span
                                className={`${statusColorCls} font-semibold text-[11px]`}
                              >
                                {log.status ?? '...'}
                              </span>
                              <span className="text-vsc-text-faint text-[10px]">
                                {log.method}
                              </span>
                              <RequestUrlLabel
                                url={log.url}
                                requestHeaders={log.requestHeaders}
                              />
                              {log.durationMs != null && (
                                <span className="text-vsc-text-faint text-[10px]">
                                  {log.durationMs}ms
                                </span>
                              )}
                              <span className="text-vsc-text-faint text-[9px]">
                                {isExp ? '\u25B4' : '\u25BE'}
                              </span>
                            </div>
                            {isExp && (
                              <div className="py-2 pr-2.5 pl-[22px] flex flex-col gap-2 border-b border-vsc-border">
                                {log.error && (
                                  <CodeBlock variant="error">
                                    {log.error}
                                  </CodeBlock>
                                )}
                                <div>
                                  <div className="text-[10px] font-semibold text-vsc-text-muted mb-0.5 uppercase tracking-wide">
                                    Request Headers
                                  </div>
                                  <CodeBlock>
                                    {JSON.stringify(
                                      log.requestHeaders,
                                      null,
                                      2,
                                    )}
                                  </CodeBlock>
                                </div>
                                {log.requestBody && (
                                  <div>
                                    <div className="text-[10px] font-semibold text-vsc-text-muted mb-0.5 uppercase tracking-wide">
                                      Request Body
                                    </div>
                                    <CodeBlock>
                                      {tryFormatJson(log.requestBody)}
                                    </CodeBlock>
                                  </div>
                                )}
                                {log.responseBody != null && (
                                  <div>
                                    <div className="text-[10px] font-semibold text-vsc-text-muted mb-0.5 uppercase tracking-wide">
                                      Response Body
                                    </div>
                                    <CodeBlock>
                                      {tryFormatJson(log.responseBody)}
                                    </CodeBlock>
                                  </div>
                                )}
                              </div>
                            )}
                          </div>
                        );
                      })}
                      {run.inputRequests.map((req) => (
                        <div
                          key={req.id}
                          className="flex items-center gap-2 px-[22px] py-1.5 border-b border-vsc-border bg-vsc-surface"
                        >
                          <span className="text-vsc-text-faint text-xs shrink-0">
                            {req.prompt ?? 'Input:'}
                          </span>
                          <input
                            className="flex-1 bg-vsc-bg border border-vsc-border rounded px-2 py-1 text-xs text-vsc-text font-vsc-mono focus:outline-none focus:border-vsc-accent"
                            autoFocus
                            onKeyDown={(e) => {
                              if (e.key === 'Enter') {
                                submitRunInput(
                                  run.id,
                                  req.id,
                                  e.currentTarget.value,
                                );
                              }
                            }}
                          />
                        </div>
                      ))}
                      {run.status === 'cancelled' && (
                        <div className="py-1.5 pr-2.5 pl-[22px]">
                          <div className="text-[11px] text-vsc-text-faint italic">
                            Cancelled
                          </div>
                        </div>
                      )}
                      {run.error && (
                        <div className="py-1.5 pr-2.5 pl-[22px]">
                          <div className="text-[10px] font-semibold text-vsc-red mb-0.5 uppercase tracking-wide">
                            Error
                          </div>
                          <ErrorDisplay error={run.error} />
                          {run.errorValue != null && (
                            <div className="mt-1">
                              <ResultDisplay
                                result={run.errorValue}
                                customRenderers={resultRenderers}
                              />
                            </div>
                          )}
                        </div>
                      )}
                      {run.result != null && (
                        <div className="py-1.5 pr-2.5 pl-[22px]">
                          <div className="text-[10px] font-semibold text-vsc-green mb-0.5 uppercase tracking-wide">
                            Result
                          </div>
                          <ResultDisplay
                            result={run.result}
                            customRenderers={resultRenderers}
                          />
                        </div>
                      )}
                    </div>
                  );
                })}
              </div>
            ) : selectedFn ? (
              <>
                {/* Graph view */}
                <TabsContent
                  value="graph"
                  className="flex-1 min-h-0 mt-0 flex flex-col"
                  style={{ minHeight: 300 }}
                >
                  {workflowSwitcherBar}
                  {controlFlowGraph ? (
                    <GraphView
                      graph={controlFlowGraph}
                      functionName={selectedFn}
                      graphRuntimeOverlay={
                        latestGraphRunSnapshot?.graphRuntimeOverlay
                      }
                      calls={latestGraphRunSnapshot?.calls}
                      run={latestGraphRunSnapshot ?? null}
                      valueBodyCache={valueBodyCache}
                      valueBodyCacheVersion={valueBodyCacheVersion}
                      runStatus={latestGraphRunSnapshot?.status}
                      runError={latestGraphRunSnapshot?.error?.message ?? null}
                      customRenderers={resultRenderers}
                      selectedNodeId={highlightedNodeId}
                      onNodeClick={handleGraphNodeClick}
                    />
                  ) : (
                    <div className="flex-1 flex items-center justify-center text-vsc-text-faint text-xs bg-vsc-bg h-full">
                      Loading graph...
                    </div>
                  )}
                </TabsContent>

                {/* Trace timeline */}
                <TabsContent
                  value="trace"
                  className="flex-1 min-h-0 mt-0 flex flex-col"
                  style={{ minHeight: 300 }}
                >
                  <TraceTimelineView
                    run={latestGraphRunSnapshot}
                    valueBodyCache={valueBodyCache}
                  />
                </TabsContent>

                {/* Profile flamegraph */}
                <TabsContent
                  value="flame"
                  className="flex-1 min-h-0 mt-0 flex flex-col"
                  style={{ minHeight: 300 }}
                >
                  {activeTab === 'flame' && (
                    <ExecutionProfileView run={latestGraphRunSnapshot} />
                  )}
                </TabsContent>

                {/* Prompt preview */}
                {canPreviewPrompt && (
                  <TabsContent
                    value="prompt"
                    className="flex-1 flex flex-col overflow-hidden mt-0"
                  >
                    {promptPreviewError && (
                      <div className="px-2.5 py-1.5 text-[10px] text-vsc-error bg-vsc-error/10 border-b border-vsc-error/20 shrink-0">
                        Preview error: {promptPreviewError}
                      </div>
                    )}
                    <div className="flex-1 overflow-auto font-vsc-mono text-xs bg-vsc-bg p-2.5 group relative">
                      {promptPreviewResult != null && (
                        <div className="absolute top-1 right-1 z-10">
                          <CopyButton textRef={promptContentRef} />
                        </div>
                      )}
                      <div ref={promptContentRef}>
                        {promptPreviewResult != null ? (
                          <ResultDisplay
                            result={promptPreviewResult}
                            customRenderers={resultRenderers}
                          />
                        ) : (
                          <div className="flex items-center justify-center text-vsc-text-faint text-xs h-full">
                            {previewLoading
                              ? 'Loading prompt preview...'
                              : 'Enter args to preview prompt'}
                          </div>
                        )}
                      </div>
                    </div>
                    {promptPreviewResult != null && (
                      <PromptStats
                        text={stringifyResult(promptPreviewResult)}
                      />
                    )}
                  </TabsContent>
                )}

                {/* cURL preview */}
                {canPreviewCurl && (
                  <TabsContent
                    value="curl"
                    className="flex-1 overflow-auto font-vsc-mono text-xs bg-vsc-bg p-2.5 mt-0"
                  >
                    {curlPreviewResult != null ? (
                      <ResultDisplay
                        result={curlPreviewResult}
                        customRenderers={resultRenderers}
                      />
                    ) : curlPreviewError ? (
                      <div className="flex items-center justify-center text-vsc-error text-xs h-full">
                        {curlPreviewError}
                      </div>
                    ) : (
                      <div className="flex items-center justify-center text-vsc-text-faint text-xs h-full">
                        {previewLoading
                          ? 'Loading cURL preview...'
                          : 'Enter args to preview cURL'}
                      </div>
                    )}
                  </TabsContent>
                )}

                {/* Execution area */}
                <TabsContent
                  value="run"
                  className="flex-1 flex flex-col min-h-0 mt-0"
                >
                  {/* Args */}
                  {/* `nokey`: keep React Flow's global key capture (Space,
                      Backspace, ...) out of the args inputs */}
                  <div className="nokey flex flex-col border-b border-vsc-border shrink-0">
                    <div className="flex items-center min-h-7">
                      <span className="px-2 py-1 text-[10px] text-vsc-text-faint font-vsc-mono bg-vsc-surface border-r border-vsc-border self-stretch flex items-center">
                        args
                      </span>
                      {showArgsForm ? (
                        <div className="flex-1" />
                      ) : (
                        <div className="flex-1 flex items-center min-w-0">
                          <Input
                            spellCheck={false}
                            value={argsJson}
                            onChange={onArgsJsonChange}
                            className="flex-1 h-7 rounded-none border-none font-vsc-mono text-xs"
                            placeholder='{"key": "value"}'
                          />
                          {argsFormUnavailable && (
                            <span className="px-2 text-[10px] text-vsc-text-faint whitespace-nowrap">
                              not a JSON object — form off
                            </span>
                          )}
                        </div>
                      )}
                      {paramSchemas !== undefined && (
                        <ToggleGroup
                          size="sm"
                          className="px-1.5 shrink-0"
                          value={argsMode}
                          options={[
                            { value: 'form', label: 'form' },
                            { value: 'raw', label: 'raw' },
                          ]}
                          onValueChange={setArgsMode}
                        />
                      )}
                      <Button
                        variant="success"
                        size="xs"
                        className="mx-1 my-0.5 shrink-0 text-[11px] font-semibold"
                        aria-label={isRunning ? 'Running' : 'Run'}
                        disabled={
                          runtimeControlsDisabled || isRunning || !selectedProject
                        }
                        onClick={onRunFunction}
                      >
                        {isRunning ? (
                          'Running...'
                        ) : (
                          <>
                            Run
                            <span className="font-normal opacity-70">
                              {RUN_SHORTCUT_HINT}
                            </span>
                          </>
                        )}
                      </Button>
                    </div>
                    {showArgsForm && paramSchemas && parsedArgs && (
                      <div className="max-h-56 overflow-y-auto px-2 py-1.5 border-t border-vsc-border">
                        {/* Key by function: the swap effect replaces argsJson
                            externally on selection change; remounting resets
                            widget drafts/collapse state with it. */}
                        <ArgsForm
                          key={selectedFn ?? ''}
                          params={paramSchemas}
                          types={projectTypes}
                          value={parsedArgs}
                          onChange={onArgsFormChange}
                        />
                      </div>
                    )}
                  </div>

                  {/* Live graph */}
                  <div
                    className="flex-1 min-h-0 flex flex-col bg-vsc-bg border-b border-vsc-border"
                    style={{ minHeight: 180 }}
                  >
                    {workflowSwitcherBar}
                    {controlFlowGraph ? (
                      <GraphView
                        graph={controlFlowGraph}
                        functionName={selectedFn}
                        graphRuntimeOverlay={
                          latestGraphRunSnapshot?.graphRuntimeOverlay
                        }
                        calls={latestGraphRunSnapshot?.calls}
                        run={latestGraphRunSnapshot ?? null}
                        valueBodyCache={valueBodyCache}
                        valueBodyCacheVersion={valueBodyCacheVersion}
                        runStatus={latestGraphRunSnapshot?.status}
                        runError={
                          latestGraphRunSnapshot?.error?.message ?? null
                        }
                        customRenderers={resultRenderers}
                        selectedNodeId={highlightedNodeId}
                        onNodeClick={handleGraphNodeClick}
                      />
                    ) : (
                      <div className="flex-1 flex items-center justify-center text-vsc-text-faint text-xs bg-vsc-bg h-full">
                        Loading graph...
                      </div>
                    )}
                  </div>

                  {/* Logs resize handle — spans the full panel width, pinned
                      just above the run-history strip. */}
                  <div
                    onMouseDown={onLogsResizeStart}
                    className="absolute left-0 right-0 z-10 h-1.5 cursor-row-resize bg-vsc-surface hover:bg-vsc-accent/30 transition-colors border-y border-vsc-border"
                    style={{ bottom: logsPanelHeight }}
                    title="Resize logs"
                  />

                  {/* Run history (scrollable) — full panel width, below the
                      sidebar+content row. */}
                  <div
                    ref={outputRef}
                    className="absolute left-0 right-0 bottom-0 z-10 overflow-auto font-vsc-mono text-xs bg-vsc-bg"
                    style={{ height: logsPanelHeight }}
                  >
                    {runValidationError && (
                      <div className="p-2.5 border-b border-vsc-border bg-vsc-error/10">
                        <ErrorDisplay error={runValidationError} />
                      </div>
                    )}

                    {functionRuns.length === 0 && !runValidationError && (
                      <div className="p-5 text-center text-vsc-text-faint text-[11px]">
                        Press Run to execute {selectedFn}()
                      </div>
                    )}

                    {functionRuns.map((run, boundaryIdx) => {
                      const isLatest = boundaryIdx === 0;
                      const isLlmFunctionRun = llmFunctionNames.has(
                        run.functionName,
                      );
                      const statusCls =
                        run.status === 'error'
                          ? 'bg-vsc-red'
                          : run.status === 'success'
                            ? 'bg-vsc-green'
                            : run.status === 'cancelled'
                              ? 'bg-vsc-yellow'
                              : 'bg-vsc-text-muted';

                      return (
                        <div
                          key={run.id}
                          className={
                            !isLatest ? 'border-b-2 border-vsc-border' : ''
                          }
                        >
                          {/* Run header */}
                          <div className="flex items-center gap-1.5 px-2.5 py-1.5 bg-vsc-surface border-b border-vsc-border-subtle">
                            <span
                              className={`w-1.5 h-1.5 rounded-full shrink-0 ${statusCls}`}
                            />
                            <span className="text-vsc-accent font-semibold text-[11px]">
                              {run.functionName}()
                            </span>
                            <span className="text-vsc-text-faint text-[10px] flex-1 overflow-hidden text-ellipsis whitespace-nowrap">
                              {run.argsJson}
                            </span>
                            {run.status === 'running' && (
                              <>
                                <span className="text-vsc-text-muted text-[10px]">
                                  running...
                                </span>
                                <TooltipProvider>
                                  <Tooltip>
                                    <TooltipTrigger asChild>
                                      <Button
                                        variant="ghost"
                                        size="icon"
                                        className="h-5 w-5 text-vsc-text-muted hover:text-vsc-error"
                                        onClick={() =>
                                          onCancelFunctionRun(run.id)
                                        }
                                      >
                                        <Square size={12} />
                                      </Button>
                                    </TooltipTrigger>
                                    <TooltipContent>
                                      Cancel execution
                                    </TooltipContent>
                                  </Tooltip>
                                </TooltipProvider>
                              </>
                            )}
                            {run.durationMs != null && (
                              <span className="text-vsc-text-faint text-[10px] shrink-0">
                                {run.durationMs}ms
                              </span>
                            )}
                          </div>

                          {run.rootInput != null && (
                            <div className="py-1.5 pr-2.5 pl-[22px] border-b border-vsc-border-subtle">
                              <div className="text-[10px] font-semibold text-vsc-text-muted mb-0.5 uppercase tracking-wide">
                                Input
                              </div>
                              <ResultDisplay
                                result={run.rootInput}
                                customRenderers={resultRenderers}
                              />
                            </div>
                          )}

                          {/* Fetch logs for this run */}
                          {run.fetchLogs.map((log) => {
                            const isExp = expandedLogId === log.id;
                            const statusColorCls =
                              log.status === null
                                ? 'text-vsc-text-muted'
                                : log.status >= 200 && log.status < 300
                                  ? 'text-vsc-green'
                                  : log.status === 0
                                    ? 'text-vsc-red'
                                    : 'text-vsc-yellow';
                            return (
                              <div key={`n-${log.id}`}>
                                <div
                                  onClick={() =>
                                    setExpandedLogId(isExp ? null : log.id)
                                  }
                                  className="flex items-center gap-1.5 py-0.5 pr-2.5 pl-[22px] cursor-pointer border-b border-vsc-border-subtle"
                                >
                                  <span
                                    className={`${statusColorCls} font-semibold text-[11px]`}
                                  >
                                    {log.status ?? '...'}
                                  </span>
                                  <span className="text-vsc-text-faint text-[10px]">
                                    {log.method}
                                  </span>
                                  <RequestUrlLabel
                                    url={log.url}
                                    requestHeaders={log.requestHeaders}
                                  />
                                  {log.durationMs != null && (
                                    <span className="text-vsc-text-faint text-[10px]">
                                      {log.durationMs}ms
                                    </span>
                                  )}
                                  <span className="text-vsc-text-faint text-[9px]">
                                    {isExp ? '\u25B4' : '\u25BE'}
                                  </span>
                                </div>
                                {isExp && (
                                  <div className="py-2 pr-2.5 pl-[22px] flex flex-col gap-2 border-b border-vsc-border">
                                    {log.error && (
                                      <CodeBlock variant="error">
                                        {log.error}
                                      </CodeBlock>
                                    )}
                                    <div>
                                      <div className="text-[10px] font-semibold text-vsc-text-muted mb-0.5 uppercase tracking-wide">
                                        Request Headers
                                      </div>
                                      <CodeBlock>
                                        {JSON.stringify(
                                          log.requestHeaders,
                                          null,
                                          2,
                                        )}
                                      </CodeBlock>
                                    </div>
                                    {log.requestBody && (
                                      <div>
                                        <div className="text-[10px] font-semibold text-vsc-text-muted mb-0.5 uppercase tracking-wide">
                                          Request Body
                                        </div>
                                        <CodeBlock>
                                          {tryFormatJson(log.requestBody)}
                                        </CodeBlock>
                                      </div>
                                    )}
                                    {log.responseBody != null && (
                                      <div>
                                        <div className="text-[10px] font-semibold text-vsc-text-muted mb-0.5 uppercase tracking-wide">
                                          Response Body
                                        </div>
                                        <CodeBlock>
                                          {tryFormatJson(log.responseBody)}
                                        </CodeBlock>
                                      </div>
                                    )}
                                  </div>
                                )}
                              </div>
                            );
                          })}

                          {/* Inline io.input() prompts for this run */}
                          {run.inputRequests.map((req) => (
                            <div
                              key={req.id}
                              className="flex items-center gap-2 px-[22px] py-1.5 border-b border-vsc-border bg-vsc-surface"
                            >
                              <span className="text-vsc-text-faint text-xs shrink-0">
                                {req.prompt ?? 'Input:'}
                              </span>
                              <input
                                className="flex-1 bg-vsc-bg border border-vsc-border rounded px-2 py-1 text-xs text-vsc-text font-vsc-mono focus:outline-none focus:border-vsc-accent"
                                autoFocus
                                onKeyDown={(e) => {
                                  if (e.key === 'Enter') {
                                    submitRunInput(
                                      run.id,
                                      req.id,
                                      e.currentTarget.value,
                                    );
                                  }
                                }}
                              />
                            </div>
                          ))}
                          {/* Result / Error / Cancelled for this run */}
                          {run.status === 'cancelled' && (
                            <div className="py-1.5 pr-2.5 pl-[22px]">
                              <div className="text-[11px] text-vsc-text-faint italic">
                                Cancelled
                              </div>
                            </div>
                          )}
                          {run.error && (
                            <div className="py-1.5 pr-2.5 pl-[22px]">
                              <div className="text-[10px] font-semibold text-vsc-red mb-0.5 uppercase tracking-wide">
                                Error
                              </div>
                              <ErrorDisplay
                                error={run.error}
                                onRetry={onRunFunction}
                              />
                              {run.errorValue != null && (
                                <div className="mt-1">
                                  <ResultDisplay
                                    result={run.errorValue}
                                    customRenderers={resultRenderers}
                                  />
                                </div>
                              )}
                            </div>
                          )}
                          {run.result != null && (
                            <div className="py-1.5 pr-2.5 pl-[22px]">
                              {run.status === 'success' &&
                                run.fetchLogs.length > 0 && (
                                  <div className="mb-1">
                                    <MetadataBadges
                                      fetchLogs={run.fetchLogs}
                                      durationMs={run.durationMs}
                                    />
                                  </div>
                                )}
                              <div className="space-y-1">
                                <div className="flex items-center gap-1">
                                  <div className="text-[10px] font-semibold text-vsc-green uppercase tracking-wide">
                                    Result
                                  </div>
                                  {/* Parsed/Raw only applies to LLM functions,
                                      where Raw is the un-parsed model output.
                                      expr functions return a structured value. */}
                                  {isLlmFunctionRun && (
                                    <ToggleGroup
                                      value={resultModes[run.id] ?? 'parsed'}
                                      onValueChange={(v) =>
                                        setResultModes((prev) => ({
                                          ...prev,
                                          [run.id]: v as 'parsed' | 'raw',
                                        }))
                                      }
                                      options={[
                                        { value: 'parsed', label: 'Parsed' },
                                        { value: 'raw', label: 'Raw' },
                                      ]}
                                      size="sm"
                                    />
                                  )}
                                  <CopyButton
                                    text={stringifyResult(run.result)}
                                    iconSize={11}
                                  />
                                </div>
                                {isLlmFunctionRun &&
                                (resultModes[run.id] ?? 'parsed') === 'raw' ? (
                                  <pre className="whitespace-pre-wrap break-all font-vsc-mono text-[11px] text-vsc-text bg-vsc-bg-secondary p-2 rounded border border-vsc-border max-h-[400px] overflow-auto">
                                    {stringifyResult(run.result)}
                                  </pre>
                                ) : (
                                  <ResultDisplay
                                    result={run.result}
                                    customRenderers={resultRenderers}
                                  />
                                )}
                              </div>
                            </div>
                          )}
                        </div>
                      );
                    })}
                  </div>
                </TabsContent>
              </>
            ) : (
              <div className="flex-1 flex items-center justify-center text-vsc-text-faint text-xs bg-vsc-bg">
                {isLoadingProject
                  ? 'Loading project...'
                  : viewingCollection
                  ? 'Collection not yet available — click Refresh'
                  : 'Select a function to run'}
              </div>
            )}
          </div>
        </div>
      </Tabs>

      <ApiKeysDialog
        open={showApiKeysDialog}
        envVars={envVars}
        requiredKeys={knownRequiredKeys}
        shellEnvVars={shellEnvVars}
        shellOverriddenKeys={shellOverriddenKeys}
        shellDeletedKeys={shellDeletedKeys}
        showProxyEnvVar={getProxyEnvVarConfig().visible}
        proxyEnabled={BOUNDARY_PROXY_URL_KEY in envVars}
        onToggleProxy={setGatewayEnabled}
        onOpenChange={(open) => {
          setShowApiKeysDialog(open);
          showApiKeysDialogRef.current = open;
          if (!open) {
            // Dialog closed — resolve ALL pending env requests in one batch.
            // If user provided a value, envVarsRef has it. If not, value is undefined → worker errors the call.
            for (const [id, pending] of pendingEnvRequestsRef.current) {
              const value = envVarsRef.current[pending.variable];
              if (pending.runScoped) {
                void executionStore
                  .respondToEnv(
                    pending.runScoped.boundaryId,
                    pending.runScoped.envRequestId,
                    value,
                  )
                  .catch((error) => {
                    console.warn(
                      '[ExecutionPanel] respondToEnv failed:',
                      error,
                    );
                  });
              } else {
                port.postMessage({
                  type: 'envVarResponse',
                  id,
                  value,
                  variable: pending.variable,
                });
              }
            }
            pendingEnvRequestsRef.current.clear();
          }
        }}
        onSetEnvVar={addEnvVar}
        onDeleteEnvVar={removeEnvVar}
        onImportEnvVars={importEnvVars}
        onRevertToShell={revertToShell}
      />
    </>
  );
};
