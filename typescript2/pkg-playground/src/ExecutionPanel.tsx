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

/** biome-ignore-all lint/style/useFilenamingConvention: preserve the public component filename */
import type { BamlJsValue } from '@b/pkg-proto';
import { encodeRunArgs } from '@b/pkg-proto';
import {
  KeyRound,
  Loader2,
  PanelLeft,
  Play,
  Settings,
  Square,
} from 'lucide-react';
import type { ChangeEvent, FC } from 'react';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { ArgsForm } from './ArgsForm';
import {
  isPlainObject,
  reconcileArgs,
  typeLookupFrom,
} from './args-form-model';
import { ApiKeysDialog } from './components/ApiKeysDialog';
import { CopyButton } from './components/CopyButton';
import { ErrorDisplay } from './components/ErrorDisplay';
import { MetadataBadges } from './components/MetadataBadges';
import { PromptStats } from './components/PromptStats';
import { Button } from './components/ui/button';
import { CodeBlock } from './components/ui/code-block';
import { Input } from './components/ui/input';
import { Tabs, TabsContent, TabsList, TabsTrigger } from './components/ui/tabs';
import { ToggleGroup } from './components/ui/toggle-group';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from './components/ui/tooltip';
import {
  selectDefaultFunctionName,
  selectMainFunctionName,
} from './default-function-selection';
import { useEnvVars } from './envAtoms';
import { ObsTelemetryTab } from './obs/TelemetryView';
import type { ExecutionStoreSnapshot } from './execution-store';
import { createExecutionStore, type ExecutionStore } from './execution-store';
import { FunctionSidebar } from './FunctionSidebar';
import { setGatewayEnabled } from './gateway';
import { GraphView } from './graph/GraphView';
import { findLatestGraphRunSnapshot } from './graph-run-selection';
import { cn } from './lib/utils';
import { BOUNDARY_PROXY_URL_KEY, getProxyEnvVarConfig } from './proxy-config';
import { ResultDisplay } from './ResultDisplay';
import { RunOutputTerminal } from './RunOutputTerminal';
import { registerBuiltinResultRenderers } from './renderers/registerBuiltins';
import type { ResultRendererProps } from './result-renderers';
import {
  applyProjectUpdateToGating,
  isRunGated,
  markProjectNotReady,
  NO_NOT_READY_PROJECTS,
  type NotReadyProjects,
} from './run-gating';
import {
  createRunStoreClient,
  isProjectNotReadyError,
} from './run-store-client';
import {
  decodeRunResultValue,
  type RunStoreDisplayRun,
  runToDisplayRun,
} from './run-store-projections';
import type { RuntimePort } from './runtime-port';
import {
  parseSerializedTestTreeJson,
  type SerializedTestDef,
  type SerializedTestSet,
} from './serialized-test-tree';
import { companionFunctionName } from './shared/companion-functions';
import { collectLatestTestRunResults } from './test-run-results';
import type { ValueBodyCache } from './value-body-cache';
import { createValueBodyCache } from './value-body-cache';
import {
  type BoundaryId,
  type ControlFlowGraph,
  type CursorContext,
  type FetchLogEntry,
  type FunctionInfo,
  type ProjectUpdate,
  previewTestKey,
  type Run,
  type RunStatus,
  type SourceNavigationTarget,
  type TestInfo,
  type WorkerOutMessage,
} from './worker-protocol';

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
    hour12: false,
    minute: '2-digit',
    second: '2-digit',
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
  initialTab?: 'run' | 'graph' | 'prompt' | 'curl' | 'telemetry';
  /** Override the `/api/obs` observability WebSocket URL used by the Telemetry
      tab (default: derived the same way as the `/api/ws` URL). */
  obsUrl?: string;
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
          {hasError ? 'expansion error' : 'collection fetch logs'}
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
            Expansion Error
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
              <button
                className="flex w-full items-center gap-1.5 border-0 border-b border-vsc-border-subtle bg-transparent py-0.5 pr-2.5 pl-[22px] text-left cursor-pointer"
                onClick={() => setExpandedLogId(isExp ? null : log.id)}
                type="button"
              >
                <span className={`${statusColorCls} font-semibold text-[11px]`}>
                  {log.status ?? '...'}
                </span>
                <span className="text-vsc-text-faint text-[10px]">
                  {log.method}
                </span>
                <RequestUrlLabel
                  requestHeaders={log.requestHeaders}
                  url={log.url}
                />
                {log.durationMs != null && (
                  <span className="text-vsc-text-faint text-[10px]">
                    {log.durationMs}ms
                  </span>
                )}
                <span className="text-vsc-text-faint text-[9px]">
                  {isExp ? '\u25B4' : '\u25BE'}
                </span>
              </button>
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
  obsUrl,
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

  const [projectRoots, setProjectRoots] = useState<string[]>([]);
  const [projectUpdates, setProjectUpdates] = useState<
    Record<string, ProjectUpdate>
  >({});
  // Projects whose last run/preview was refused with `projectNotReady`. The
  // fail-closed server rejects runs while a rebuild is pending; the UI
  // renders that as the transient "Preparing current build…" state and clears
  // it when the next current ProjectUpdate arrives.
  const [notReadyProjects, setNotReadyProjects] = useState<NotReadyProjects>(
    NO_NOT_READY_PROJECTS,
  );
  const [testTree, setTestTree] = useState<SerializedTestDef[] | null>(null);
  const [collectionCallId, setCollectionCallId] = useState<number | null>(null);
  const [generation, setGeneration] = useState<number>(0);
  const [testStartErrors, setTestStartErrors] = useState<Map<string, string>>(
    new Map(),
  );
  const [failedExpands, setFailedExpands] = useState<Set<string>>(new Set());
  const [collectionDebug, setCollectionDebug] =
    useState<CollectionDebugState | null>(null);
  // When true, the main content area shows the collection run's fetch logs
  const [viewingCollection, setViewingCollection] = useState(false);
  // When true, the main content area shows the test run history panel
  const [viewingTestRun, setViewingTestRun] = useState(false);
  const [selectedProject, setSelectedProject] = useState<string | null>(null);
  const [pendingTestTarget, setPendingTestTarget] =
    useState<PendingTestTarget | null>(null);

  const [selectedFn, setSelectedFn] = useState<string | null>(null);
  const [selectedTestName, setSelectedTestName] = useState<string | null>(null);
  const graphTargetName = selectedTestName ?? selectedFn;
  const [selectedGraphRunId, setSelectedGraphRunId] =
    useState<BoundaryId | null>(null);
  const [selectedPreviewTestKey, setSelectedPreviewTestKey] = useState<
    string | null
  >(null);
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
        .map((run) =>
          runToDisplayRun(run, argsJsonByBoundaryId, valueBodyCache),
        )
        .filter((run): run is RunStoreDisplayRun => run != null),
    [
      executionSnapshot.runs,
      argsJsonByBoundaryId,
      valueBodyCache,
      valueBodyCacheVersion,
    ],
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
  const testRunResults = useMemo(
    () => collectLatestTestRunResults(testRuns, testStartErrors),
    [testRuns, testStartErrors],
  );
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
  const [workflowCacheVersion, setWorkflowCacheVersion] = useState(0);
  const [activeTab, setActiveTab] = useState<
    'run' | 'graph' | 'prompt' | 'curl' | 'telemetry'
  >(() => {
    // Normalize unknown tab names (e.g. the removed legacy 'trace'/'flame'
    // profile tabs from an older host) to the default run tab.
    switch (initialTab) {
      case 'run':
      case 'graph':
      case 'prompt':
      case 'curl':
      case 'telemetry':
        return initialTab;
      default:
        return 'run';
    }
  });
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
  >(() => ({ firstHop: new Map(), roots: [] }));
  // Highlight to apply once the promoted workflow's graph arrives (the
  // selection-change effect clears highlights, so apply after, not before).
  const pendingHighlightRef = useRef<{ fn: string; nodeId: number } | null>(
    null,
  );
  const selectedFnRef = useRef(selectedFn);
  useEffect(() => {
    selectedFnRef.current = selectedFn;
  }, [selectedFn]);
  useEffect(() => {
    if (selectedFn && selectedTestName) {
      setSelectedTestName(null);
    }
  }, [selectedFn, selectedTestName]);
  const selectedTestNameRef = useRef(selectedTestName);
  useEffect(() => {
    selectedTestNameRef.current = selectedTestName;
  }, [selectedTestName]);
  const graphTargetNameRef = useRef(graphTargetName);
  useEffect(() => {
    graphTargetNameRef.current = graphTargetName;
  }, [graphTargetName]);
  const testGraphRequestsRef = useRef(new Set<string>());
  const pendingTestSourceNavigationRef = useRef<string | null>(null);
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
    if (!graph) return { any, owner };
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
    return { any, owner };
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
          endOffset: source.endOffset,
          expiresAt: performance.now() + 1000,
          nodeId,
          startOffset: source.startOffset,
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
        setSelectedPreviewTestKey(null);
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
        setSelectedPreviewTestKey(null);
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
      setSelectedPreviewTestKey(null);
      setSelectedFn(ctx.functionName);
      setViewingCollection(false);
      setViewingTestRun(false);
      setHighlightedNodeId(null);
    }
    setWorkflowContext(null);
  }

  // ── Port message handler ─────────────────────────────────────────────

  function handleControlFlowGraphResult(
    targetName: string,
    graph: ControlFlowGraph | null,
  ) {
    const isTestGraph = testGraphRequestsRef.current.has(targetName);
    if (!isTestGraph) {
      workflowCfgResponsesRef.current.set(targetName, graph);
      setWorkflowCacheVersion((version) => version + 1);
      if (graph) {
        workflowCfgCacheRef.current.set(targetName, graph);
      }
    }

    if (!graph || targetName !== graphTargetNameRef.current) return;
    setControlFlowGraph(graph);
    const pendingHighlight = pendingHighlightRef.current;
    if (pendingHighlight && pendingHighlight.fn === targetName) {
      pendingHighlightRef.current = null;
      setHighlightedNodeId(pendingHighlight.nodeId);
    }

    if (pendingTestSourceNavigationRef.current === targetName) {
      pendingTestSourceNavigationRef.current = null;
      const source = Object.values(graph.nodes).find(
        (node) => node.nodeType === 'functionRoot',
      )?.sourceSpan;
      if (source) onNavigateToSource?.(source);
    }
  }

  useEffect(() => {
    const unsubscribe = port.onMessage((data: WorkerOutMessage) => {
      switch (data.type) {
        case 'playgroundNotification': {
          const n = data.notification;
          if (!n) break;
          switch (n.type) {
            case 'listProjects':
              setProjectRoots(n.projects ?? []);
              setSelectedProject((prev) => {
                if (prev && (n.projects ?? []).includes(prev)) return prev;
                return (n.projects ?? [])[0] ?? null;
              });
              break;
            case 'updateProject':
              setProjectUpdates((prev) => ({ ...prev, [n.project]: n.update }));
              // A current build re-enables run controls automatically after a
              // fail-closed `projectNotReady` rejection.
              setNotReadyProjects((prev) =>
                applyProjectUpdateToGating(prev, n.project, n.update),
              );
              break;
            case 'testCollectionResult': {
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
                setCollectionCallId(n.callId);
                setGeneration(n.generation);
                setTestStartErrors(new Map());

                // Create/replace collection debug state, hydrating any fetch
                // logs that arrived before this notification.
                const buffered = pendingLogsRef.current.get(n.callId) ?? [];
                pendingLogsRef.current.delete(n.callId);
                const hasError = !!n.expandError;
                const collectionState: CollectionDebugState = {
                  error: hasError ? n.expandError!.message : null,
                  fetchLogs: buffered,
                  id: n.callId,
                  status: hasError ? 'error' : 'success',
                };
                setCollectionDebug(collectionState);
              } catch (e) {
                console.error('[testCollectionResult] decode error:', e);
              }
              break;
            }
            case 'openPlayground':
              setSelectedProject(n.project);
              if (n.functionName) {
                setWorkflowContext(null);
                setSelectedPreviewTestKey(null);
                setSelectedFn(n.functionName);
                setViewingCollection(false);
                setViewingTestRun(false);
              } else if (n.testName || n.testsetName) {
                setWorkflowContext(null);
                setSelectedPreviewTestKey(null);
                setSelectedFn(null);
                setViewingCollection(false);
                setViewingTestRun(true);
                setTestTree(null);
                setCollectionCallId(null);
                setCollectionDebug(null);
                setTestStartErrors(new Map());
                setPendingTestTarget({
                  kind: n.testName ? 'test' : 'testset',
                  name: n.testName ?? n.testsetName!,
                  project: n.project,
                });
                port.postMessage({
                  project: n.project,
                  type: 'requestCollectTests',
                });
              }
              break;
            case 'controlFlowGraphResult':
              handleControlFlowGraphResult(n.functionName, n.graph);
              break;
          }
          break;
        }

        case 'runStarted':
        case 'runPatch':
        case 'commandAck':
        case 'commandError':
        case 'runList':
        case 'historyList':
        case 'runSnapshot':
        case 'valueBody':
        case 'runCursorExpired':
          // RunStoreClient consumes these during the staged migration. The
          // legacy reducer keeps ignoring them until the UI cutover.
          break;

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
                .respondToEnv(
                  runScoped.boundaryId,
                  runScoped.envRequestId,
                  cached,
                )
                .catch((error) => {
                  console.warn('[ExecutionPanel] respondToEnv failed:', error);
                });
            } else {
              port.postMessage({
                id: data.id,
                type: 'envVarResponse',
                value: cached,
                variable: data.variable,
              });
            }
          } else {
            // Park the request — it will be resolved when the dialog closes
            pendingEnvRequestsRef.current.set(data.id, {
              runScoped,
              variable: data.variable,
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
          handleControlFlowGraphResult(data.functionName, data.graph);
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

    // Ask the worker to re-send functionNames/diagnostics/engineStale.
    // These are sent once during init but may arrive before this listener
    // is attached (race between worker 'ready' and dynamic imports).
    port.postMessage({ type: 'requestState' });

    return unsubscribe;
  }, [port]);

  // Request a control flow graph when the selected function/test changes OR code is edited.
  // On target/project switch: clear the graph (shows loading state).
  // On code edit (projectUpdateVersion): keep old graph visible, swap when new one arrives.
  const prevGraphTargetRef = useRef(graphTargetName);
  const prevGraphProjectRef = useRef(selectedProject);
  const projectUpdateVersion = selectedProject
    ? projectUpdates[selectedProject]
    : undefined;

  useEffect(() => {
    const targetChanged = prevGraphTargetRef.current !== graphTargetName;
    const projChanged = prevGraphProjectRef.current !== selectedProject;
    prevGraphTargetRef.current = graphTargetName;
    prevGraphProjectRef.current = selectedProject;

    if (targetChanged || projChanged) {
      setControlFlowGraph(null);
      setHighlightedNodeId(null);
    }
    if (!graphTargetName || !selectedProject) return;
    if (selectedTestName) {
      testGraphRequestsRef.current.add(selectedTestName);
    }
    port.postMessage({
      functionName: graphTargetName,
      project: selectedProject,
      type: 'requestControlFlowGraph',
    });
  }, [
    port,
    graphTargetName,
    selectedProject,
    selectedTestName,
    projectUpdateVersion,
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

  // Whether the fail-closed server is (re)building the selected project's
  // runtime: either the latest ProjectUpdate is stale (isBexCurrent false) or
  // a run/preview was refused with `projectNotReady`. Derived here — above
  // the run/preview callbacks that must consult it.
  const runtimePreparing = isRunGated(
    notReadyProjects,
    selectedProject,
    selectedProject ? projectUpdates[selectedProject] : undefined,
  );

  const markSelectedProjectNotReady = useCallback((project: string) => {
    setNotReadyProjects((prev) => markProjectNotReady(prev, project));
  }, []);

  // Auto-refresh prompt/curl preview when args change while tab is active
  useEffect(() => {
    if (activeTab !== 'prompt' && activeTab !== 'curl') return;
    if (!selectedFn || !selectedProject) return;

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

    // While the server prepares the current build, previews would only be
    // refused with `projectNotReady`. Skip issuing them; this effect re-runs
    // when the next ProjectUpdate flips `runtimePreparing` back off.
    if (runtimePreparing) {
      setPreviewLoading(false);
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
            const run = snapshot.runs.find(
              (entry) => entry.boundaryId === boundaryId,
            );
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
          argsBytes: new Uint8Array(argsBytes),
          functionName: previewFunctionName,
          helper: subFn,
          parentFunctionName: selectedFn,
          project: selectedProject,
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
        if (isProjectNotReadyError(e)) {
          // Transient fail-closed rejection — surface it as the "Preparing
          // current build…" state (not a raw error) and keep the last valid
          // preview visible. Cleared by the next current ProjectUpdate.
          markSelectedProjectNotReady(selectedProject);
          setError(null);
          setPreviewLoading(false);
          return;
        }
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
    runtimePreparing,
    markSelectedProjectNotReady,
  ]);

  // Single write path for args edits (form and raw): the prompt/cURL preview
  // and run-history snapshots read `argsJson`, and per-function memory reads
  // `typedArgsByFnRef` — an edit that misses either silently desyncs them.
  const updateArgsJson = useCallback(
    (next: string) => {
      setSelectedPreviewTestKey(null);
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
      [boundaryId]:
        (prev[boundaryId] ?? 'parsed') === 'parsed' ? 'raw' : 'parsed',
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

  const handleRefreshTests = useCallback(() => {
    if (!selectedProject) return;
    port.postMessage({ project: selectedProject, type: 'requestCollectTests' });
  }, [selectedProject, port]);

  const appliedInitialTestTargetRef = useRef(false);
  useEffect(() => {
    if (
      appliedInitialTestTargetRef.current ||
      !selectedProject ||
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
      kind: initialTestName ? 'test' : 'testset',
      name: initialTestName ?? initialTestsetName!,
      project: selectedProject,
    });
    port.postMessage({ project: selectedProject, type: 'requestCollectTests' });
  }, [initialTestName, initialTestsetName, selectedProject, port]);

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
          const run = snapshot.runs.find(
            (entry) => entry.boundaryId === boundaryId,
          );
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
      if (!selectedProject) return;
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
          generation,
          project: selectedProject,
          testName: name,
        });
        await waitForTerminalRun(boundaryId);
      } catch (e) {
        if (isProjectNotReadyError(e)) {
          // Fail-closed rebuild window: show the transient preparing state
          // instead of a per-test error; controls re-enable on the next
          // current ProjectUpdate.
          markSelectedProjectNotReady(selectedProject);
          return;
        }
        setTestStartErrors((prev) =>
          new Map(prev).set(name, e instanceof Error ? e.message : String(e)),
        );
      }
    },
    [
      executionStore,
      generation,
      selectedProject,
      waitForTerminalRun,
      markSelectedProjectNotReady,
    ],
  );

  useEffect(() => {
    if (!pendingTestTarget || !selectedProject || !testTree) {
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
  }, [pendingTestTarget, selectedProject, testTree, handleRunTest]);

  // Track which testsets we've already requested expansion for (per generation)
  const pendingExpandsRef = useRef<{
    project: string | null;
    generation: number;
    names: Set<string>;
  }>({ generation: -1, names: new Set(), project: null });

  // Auto-expand lazy testsets after receiving a new testTree
  useEffect(() => {
    if (!testTree || !selectedProject) return;
    // Reset pending set and failed state when generation or project changes.
    // Generation is per-project on the server, so different projects can share
    // the same generation number — we must track both to avoid leaking state.
    if (
      pendingExpandsRef.current.generation !== generation ||
      pendingExpandsRef.current.project !== selectedProject
    ) {
      pendingExpandsRef.current = {
        generation,
        names: new Set(),
        project: selectedProject,
      };
      setFailedExpands(new Set());
    }
    const pending = pendingExpandsRef.current.names;
    const expandLazy = (items: SerializedTestDef[]) => {
      for (const item of items) {
        if (
          'type' in item &&
          item.type === 'lazyTestSet' &&
          !pending.has(item.name)
        ) {
          pending.add(item.name);
          port.postMessage({
            generation,
            project: selectedProject,
            testsetName: item.name,
            type: 'expandTestSet',
          });
        } else if (isExpandedTestSet(item)) {
          // Recurse into expanded testsets to find nested lazy items
          expandLazy(item.items);
        }
      }
    };
    expandLazy(testTree);
  }, [testTree, selectedProject, generation, port]);

  // Retry expansion for a failed (or already expanded) testset
  const handleRetryExpand = useCallback(
    (testsetName: string) => {
      if (!selectedProject) return;
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
        generation,
        project: selectedProject,
        testsetName,
        type: 'expandTestSet',
      });
    },
    [selectedProject, generation, port],
  );

  // ── Derived state ──────────────────────────────────────────────────────

  const currentUpdate = selectedProject
    ? projectUpdates[selectedProject]
    : undefined;
  const isLoadingProject = selectedProject != null && currentUpdate == null;
  const functions: FunctionInfo[] = currentUpdate?.functions ?? [];
  const previewTests = currentUpdate?.tests ?? [];
  const internalFunctionCount = functions.filter(isInternalFunction).length;
  const visibleFunctions = showInternalFunctions
    ? functions
    : functions.filter((fn) => !isInternalFunction(fn));
  const functionNames = visibleFunctions.map((f) => f.name);
  const diags = currentUpdate?.diagnostics ?? [];

  const selectedFnInfo = visibleFunctions.find((f) => f.name === selectedFn);
  const canPreviewPrompt = selectedFnInfo?.capabilities?.renderPrompt ?? false;
  const canPreviewCurl = selectedFnInfo?.capabilities?.buildRequest ?? false;

  const handleSelectPreviewTest = useCallback((test: TestInfo) => {
    const key = previewTestKey(test);
    typedArgsByFnRef.current[test.functionName] = test.argsJson;
    setArgsJson(test.argsJson);
    setSelectedTestName(null);
    setSelectedPreviewTestKey(key);
    setSelectedFn(test.functionName);
    setViewingCollection(false);
    setViewingTestRun(false);
    setHighlightedNodeId(null);
    setWorkflowContext(null);
  }, []);

  const handleSelectTest = useCallback(
    (name: string) => {
      const currentGraph =
        graphTargetNameRef.current === name
          ? controlFlowGraphRef.current
          : null;
      const currentSource = currentGraph
        ? Object.values(currentGraph.nodes).find(
            (node) => node.nodeType === 'functionRoot',
          )?.sourceSpan
        : null;
      if (currentSource) {
        pendingTestSourceNavigationRef.current = null;
        onNavigateToSource?.(currentSource);
      } else {
        pendingTestSourceNavigationRef.current = name;
      }

      selectedTestNameRef.current = name;
      graphTargetNameRef.current = name;
      testGraphRequestsRef.current.add(name);
      setSelectedPreviewTestKey(null);
      setSelectedFn(null);
      setSelectedTestName(name);
      setViewingCollection(false);
      setViewingTestRun(false);
      setHighlightedNodeId(null);
      setWorkflowContext(null);
      setActiveTab('graph');
    },
    [onNavigateToSource],
  );

  // Keep a selected preview case synchronized with source edits. If the test
  // is deleted, retain the current function/args as an ordinary manual draft.
  useEffect(() => {
    if (!selectedPreviewTestKey) return;
    const test = previewTests.find(
      (candidate) => previewTestKey(candidate) === selectedPreviewTestKey,
    );
    if (!test) {
      setSelectedPreviewTestKey(null);
      return;
    }
    typedArgsByFnRef.current[test.functionName] = test.argsJson;
    setArgsJson(test.argsJson);
    setSelectedFn(test.functionName);
  }, [previewTests, selectedPreviewTestKey]);

  // ── Args form wiring ─────────────────────────────────────────────────────
  // `undefined` = no schema shipped (old engine / extraction miss) → raw-only.
  const paramSchemas = selectedFnInfo?.params;
  const projectTypes = currentUpdate?.types;
  const typeLookup = useMemo(
    () => typeLookupFrom(projectTypes),
    [projectTypes],
  );
  const argsSchemaKey = JSON.stringify([
    paramSchemas ?? null,
    projectTypes ?? null,
  ]);
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
  const reconciledFormArgs = useMemo(() => {
    if (parsedArgs === null || paramSchemas === undefined) return null;
    return reconcileArgs(parsedArgs, paramSchemas, typeLookup);
  }, [parsedArgs, paramSchemas, typeLookup]);
  const showArgsForm =
    argsMode === 'form' && paramSchemas !== undefined && parsedArgs !== null;
  const argsFormUnavailable =
    argsMode === 'form' && paramSchemas !== undefined && parsedArgs === null;

  // Reconcile once for each project/function/schema combination while form
  // mode is active. This replaces the old function-only seed and normalize
  // guards, which never re-ran when a same-name function changed type.
  const argsSchemaScope = selectedFn
    ? JSON.stringify([selectedProject ?? null, selectedFn, argsSchemaKey])
    : null;
  const reconcileStateRef = useRef<{ scope: string | null; done: boolean }>({
    done: false,
    scope: null,
  });
  useEffect(() => {
    const state = reconcileStateRef.current;
    if (argsMode === 'raw') {
      state.done = false;
      return;
    }
    if (!selectedFn || paramSchemas === undefined || argsSchemaScope === null) {
      return;
    }
    if (state.scope === argsSchemaScope && state.done) return;
    reconcileStateRef.current = { done: true, scope: argsSchemaScope };
    let args: unknown;
    try {
      args = JSON.parse(baseArgsFor(selectedFn));
    } catch {
      return; // not form-renderable; the raw fallback shows it as-is
    }
    if (!isPlainObject(args)) return;
    const reconciled = reconcileArgs(args, paramSchemas, typeLookup);
    if (reconciled !== args) {
      updateArgsJson(JSON.stringify(reconciled));
    }
  }, [
    argsMode,
    selectedFn,
    paramSchemas,
    typeLookup,
    argsSchemaScope,
    baseArgsFor,
    updateArgsJson,
  ]);

  const onRunFunction = useCallback(async () => {
    if (!selectedFn || !selectedProject || isRunning) return;

    // Don't force the 'run' tab — running keeps the user on whatever tab
    // they're viewing (graph, trace, prompt, etc.).
    setExpandedLogId(null);
    setRunValidationError(null);

    requestAnimationFrame(() => {
      outputRef.current?.scrollTo({ behavior: 'smooth', top: 0 });
    });

    try {
      const parsed: unknown = JSON.parse(argsJson);
      if (!isPlainObject(parsed)) {
        throw new Error(
          'Arguments must be a JSON object, e.g. {"arr": [3,1,2]}',
        );
      }
      // Effects normally keep form state canonical, but Run must also close
      // the same-commit race after a project/schema update. Raw mode remains
      // an exact escape hatch and is intentionally not reconciled here.
      const runArgs =
        argsMode === 'form' && paramSchemas !== undefined
          ? reconcileArgs(parsed, paramSchemas, typeLookup)
          : parsed;
      const runArgsJson =
        runArgs === parsed ? argsJson : JSON.stringify(runArgs);
      if (runArgs !== parsed) updateArgsJson(runArgsJson);
      const argsBytes = encodeRunArgs(runArgs);

      const boundaryId = await executionStore.startRun({
        argsBytes: new Uint8Array(argsBytes),
        functionName: selectedFn,
        project: selectedProject,
      });
      setSelectedGraphRunId(null);
      setArgsJsonByBoundaryId((prev) => ({
        ...prev,
        [boundaryId]: runArgsJson,
      }));
    } catch (e) {
      if (isProjectNotReadyError(e)) {
        // Fail-closed rebuild window: render the transient "Preparing current
        // build…" state instead of a raw error. Run re-enables automatically
        // when the next ProjectUpdate reports a current build.
        markSelectedProjectNotReady(selectedProject);
        return;
      }
      const errMsg = e instanceof Error ? e.message : String(e);
      setRunValidationError(errMsg);
    }
  }, [
    selectedFn,
    selectedProject,
    argsJson,
    argsMode,
    paramSchemas,
    typeLookup,
    isRunning,
    executionStore,
    updateArgsJson,
    markSelectedProjectNotReady,
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
        currentUpdate?.generation ?? null,
        selectedGraphRunId,
      ),
    [
      executionSnapshot.runs,
      selectedFn,
      selectedProject,
      currentUpdate?.generation,
      selectedGraphRunId,
    ],
  );
  const handleSelectHistoryRun = useCallback(
    (run: RunStoreDisplayRun) => {
      if (
        !functionNames.includes(run.functionName) ||
        currentUpdate?.generation == null ||
        run.projectGeneration !== currentUpdate.generation
      ) {
        return;
      }
      setWorkflowContext(null);
      setSelectedPreviewTestKey(null);
      setViewingCollection(false);
      setViewingTestRun(false);
      setSelectedTestName(null);
      setSelectedFn(run.functionName);
      setSelectedGraphRunId(run.id);
      setHighlightedNodeId(null);
      setActiveTab('graph');
    },
    [currentUpdate?.generation, functionNames],
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
    names: new Set(),
    version: undefined,
  });
  useEffect(() => {
    if (!selectedProject) return;
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
      port.postMessage({
        functionName: name,
        project: selectedProject,
        type: 'requestControlFlowGraph',
      });
    }
  }, [functionNames, selectedProject, projectUpdateVersion, port]);

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
      return { firstHop, roots: [...roots] };
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
  }>({ applied: false, project: null, update: undefined });
  useEffect(() => {
    const scope = defaultSelectionScopeRef.current;
    if (
      scope.project !== selectedProject ||
      scope.update !== projectUpdateVersion
    ) {
      defaultSelectionScopeRef.current = {
        applied: false,
        project: selectedProject,
        update: projectUpdateVersion,
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
        (n) =>
          n === initialFunctionName || n.endsWith(`.${initialFunctionName}`),
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
      next = selectDefaultFunctionName(
        functionNames,
        workflowCfgCacheRef.current,
      );
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
  // The fail-closed server refuses runs/previews over compile errors and
  // while a rebuild is pending; keep runtime-derived controls disabled for
  // both so users see one consistent gate instead of raw rejections.
  const runtimeControlsDisabled = hasErrors || runtimePreparing;

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
          className="h-auto px-1.5 py-0.5 text-[10px]"
          key={wf}
          onClick={() => {
            if (wf === selectedFn) return;
            const route = workflowRouteFor(workflowContext.functionName);
            const hop = route.firstHop.get(wf) ?? workflowContext.functionName;
            const target =
              findCallSiteNode(wf, workflowContext.functionName) ??
              findCallSiteNode(wf, hop);
            pendingHighlightRef.current =
              target != null ? { fn: wf, nodeId: target } : null;
            setSelectedPreviewTestKey(null);
            setSelectedFn(wf);
            setHighlightedNodeId(null);
          }}
          size="sm"
          variant={wf === selectedFn ? 'secondary' : 'outline'}
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
        className="relative flex h-full min-h-0 w-full flex-1 flex-col gap-0 overflow-hidden"
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
        onValueChange={(v) => setActiveTab(v as typeof activeTab)}
        // Panel-scoped run shortcut: fires for focus anywhere inside the
        // playground (form fields, raw input, graph) without stealing
        // Cmd/Ctrl+Enter from the host's code editor.
        value={activeTab}
      >
        {/* ──── Combined top bar ──── */}
        <div className="flex items-center gap-1.5 px-2 py-1 shrink-0 border-b border-vsc-border bg-vsc-surface">
          {activeTab !== 'telemetry' && (
            <TooltipProvider>
              <Tooltip>
                <TooltipTrigger asChild>
                  <Button
                    className="h-6 w-6 shrink-0"
                    onClick={() => setSidebarOpen((prev) => !prev)}
                    size="icon"
                    variant="ghost"
                  >
                    <PanelLeft className="h-3.5 w-3.5 text-vsc-text-muted" />
                  </Button>
                </TooltipTrigger>
                <TooltipContent>
                  {sidebarOpen ? 'Hide sidebar' : 'Show sidebar'}
                </TooltipContent>
              </Tooltip>
            </TooltipProvider>
          )}

          {selectedTestName && !viewingCollection && !viewingTestRun && (
            <>
              <span className="max-w-64 truncate text-[11px] font-vsc-mono text-vsc-accent font-semibold">
                {selectedTestName}
              </span>
              <TabsList className="bg-transparent border-b-0 ml-1 h-7">
                <TabsTrigger className="py-1 h-7" value="graph">
                  Graph
                </TabsTrigger>
              </TabsList>
            </>
          )}

          {selectedFn && !viewingCollection && !viewingTestRun && (
            <>
              <span className="text-[11px] font-vsc-mono text-vsc-accent font-semibold whitespace-nowrap">
                {selectedFn}()
              </span>
              <TabsList className="bg-transparent border-b-0 ml-1 h-7">
                <TabsTrigger className="py-1 h-7" value="run">
                  Run
                </TabsTrigger>
                <TabsTrigger className="py-1 h-7" value="graph">
                  Graph
                </TabsTrigger>
                {canPreviewPrompt && (
                  <TabsTrigger className="py-1 h-7" value="prompt">
                    Prompt
                    {selectedFnInfo?.capabilities?.clientName && (
                      <span className="ml-1 px-1 py-0 text-[9px] rounded bg-vsc-bg-secondary text-vsc-text-faint">
                        {selectedFnInfo.capabilities.clientName}
                      </span>
                    )}
                  </TabsTrigger>
                )}
                {canPreviewCurl && (
                  <TabsTrigger className="py-1 h-7" value="curl">
                    cURL
                  </TabsTrigger>
                )}
              </TabsList>
            </>
          )}

          {/* Local observability — available in every view state. */}
          <TabsList className="bg-transparent border-b-0 ml-1 h-7">
            <TabsTrigger className="py-1 h-7" value="telemetry">
              Telemetry
            </TabsTrigger>
          </TabsList>

          <div className="flex-1" />

          {projectRoots.length > 1 && (
            <ToggleGroup
              onValueChange={(v) => setSelectedProject(v)}
              options={projectRoots.map((root) => ({
                label: (
                  <>
                    {root}
                    {projectUpdates[root] &&
                      !projectUpdates[root].isBexCurrent && (
                        <span className="ml-0.5 text-vsc-yellow">*</span>
                      )}
                  </>
                ),
                value: root,
              }))}
              size="sm"
              value={selectedProject ?? projectRoots[0]}
            />
          )}

          {/* The primary Run button lives next to the args editor inside the
              Run tab; other tabs keep a compact icon so re-running while
              watching the graph stays one click away. */}
          {selectedFn &&
            !viewingCollection &&
            !viewingTestRun &&
            activeTab !== 'run' && (
              <TooltipProvider>
                <Tooltip>
                  <TooltipTrigger asChild>
                    <Button
                      aria-label="Run"
                      className="h-7 w-7"
                      disabled={
                        runtimeControlsDisabled || isRunning || !selectedProject
                      }
                      onClick={onRunFunction}
                      size="icon-xs"
                      variant="success"
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
                  className="relative h-7 w-7 shrink-0"
                  onClick={() => setShowApiKeysDialog(true)}
                  size="icon"
                  variant="ghost"
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
                    className="h-7 w-7 shrink-0"
                    onClick={() => setShowSettingsMenu((v) => !v)}
                    size="icon"
                    variant="ghost"
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
                  aria-label="Close settings"
                  className="fixed inset-0 z-40 cursor-default bg-transparent border-none"
                  onClick={() => setShowSettingsMenu(false)}
                  type="button"
                />
                <div className="absolute right-0 top-full mt-1 z-50 w-60 rounded border border-vsc-border bg-vsc-surface shadow-lg p-2.5">
                  <label className="flex items-center gap-1.5 text-[11px] text-vsc-text-muted cursor-pointer select-none">
                    <input
                      checked={showInternalFunctions}
                      className="h-3 w-3 accent-vsc-accent"
                      onChange={(e) =>
                        setShowInternalFunctions(e.currentTarget.checked)
                      }
                      type="checkbox"
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
            className="w-full flex items-center gap-2 px-2.5 py-2 border-none border-b border-vsc-border shrink-0 bg-[#5c1a1a] hover:bg-[#6e1f1f] transition-colors cursor-pointer text-left"
            onClick={() => {
              setWasmPanic(null);
              if (onReload) {
                onReload();
              } else {
                window.location.reload();
              }
            }}
            type="button"
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

        {/* Preparing banner. The sidebar keeps showing the previous
            function/test catalog, but runtime-derived controls stay disabled
            until the fail-closed server reports a current build. */}
        {selectedProject && runtimePreparing && !hasErrors && (
          <output className="flex shrink-0 items-center gap-2 border-b border-vsc-border bg-vsc-surface px-2.5 py-1.5">
            <Loader2 className="h-3.5 w-3.5 shrink-0 animate-spin text-vsc-accent" />
            <span className="font-vsc-mono text-[11px] text-vsc-text">
              Preparing current build…
            </span>
          </output>
        )}

        {/* Diagnostics banner */}
        {hasErrors && (
          <div className="border-b border-vsc-border shrink-0 bg-[#3e1a1a]">
            {diags.length > 0 ? (
              <>
                <button
                  className="w-full flex items-center gap-1 px-2.5 py-1 bg-transparent border-none cursor-pointer text-left"
                  onClick={() => setDiagsExpanded((v) => !v)}
                  type="button"
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
                    {' — current build unavailable'}
                  </span>
                </button>
                {diagsExpanded && (
                  <div className="px-2.5 pb-1.5 flex flex-col gap-0.5 max-h-[200px] overflow-y-auto">
                    {errors.map((e) => (
                      <div
                        className="font-vsc-mono text-[10px] text-[#f48771]/80 pl-3.5 break-words whitespace-pre-wrap"
                        key={`error-${e.message}`}
                      >
                        {e.message}
                      </div>
                    ))}
                    {warnings.map((w) => (
                      <div
                        className="font-vsc-mono text-[10px] text-[#cca700]/80 pl-3.5 break-words whitespace-pre-wrap"
                        key={`warning-${w.message}`}
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
          {sidebarOpen && activeTab !== 'telemetry' && (
            <>
              <div
                className="shrink-0 overflow-hidden"
                style={{ width: sidebarWidth }}
              >
                <FunctionSidebar
                  collectionLogCount={collectionDebug?.fetchLogs.length ?? 0}
                  failedExpands={failedExpands}
                  functions={visibleFunctions}
                  internalFunctionCount={internalFunctionCount}
                  isLoadingProject={isLoadingProject}
                  onRefreshTests={handleRefreshTests}
                  onRetryExpand={handleRetryExpand}
                  onRunTest={handleRunTest}
                  onSelectCollectionView={() => {
                    setViewingCollection(true);
                    setViewingTestRun(false);
                    setSelectedTestName(null);
                    setSelectedFn(null);
                  }}
                  onSelectFn={(fn) => {
                    setSelectedTestName(null);
                    setSelectedPreviewTestKey(null);
                    setViewingCollection(false);
                    setViewingTestRun(false);
                    setHighlightedNodeId(null);
                    setWorkflowContext(null);
                    setSelectedFn(fn);
                  }}
                  onSelectPreviewTest={handleSelectPreviewTest}
                  onSelectTest={handleSelectTest}
                  previewTests={previewTests}
                  runtimeControlsDisabled={runtimeControlsDisabled}
                  selectedFn={selectedFn}
                  selectedPreviewTestKey={selectedPreviewTestKey}
                  selectedTestName={selectedTestName}
                  showInternalFunctions={showInternalFunctions}
                  testRunResults={testRunResults}
                  testTree={testTree}
                  viewingCollection={viewingCollection}
                />
              </div>
              <div
                className="w-1 shrink-0 cursor-col-resize hover:bg-vsc-accent/30 transition-colors border-r border-vsc-border"
                onMouseDown={onResizeStart}
              />
            </>
          )}

          {/* Content area */}
          <div className="flex-1 flex flex-col min-h-0 min-w-0">
            {activeTab === 'telemetry' ? (
              <ObsTelemetryTab obsUrl={obsUrl} />
            ) : viewingCollection && collectionDebug ? (
              <CollectionDebugView
                expandedLogId={expandedLogId}
                setExpandedLogId={setExpandedLogId}
                state={collectionDebug}
              />
            ) : viewingTestRun ? (
              <div
                className="flex-1 overflow-auto font-vsc-mono text-xs bg-vsc-bg"
                ref={outputRef}
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
                      className={
                        !isLatest ? 'border-b-2 border-vsc-border' : ''
                      }
                      key={run.id}
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
                                    className="h-5 w-5 text-vsc-text-muted hover:text-vsc-error"
                                    onClick={() => onCancelFunctionRun(run.id)}
                                    size="icon"
                                    variant="ghost"
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
                            customRenderers={resultRenderers}
                            result={run.rootInput}
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
                            <button
                              className="flex w-full items-center gap-1.5 border-0 border-b border-vsc-border-subtle bg-transparent py-0.5 pr-2.5 pl-[22px] text-left cursor-pointer"
                              onClick={() =>
                                setExpandedLogId(isExp ? null : log.id)
                              }
                              type="button"
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
                                requestHeaders={log.requestHeaders}
                                url={log.url}
                              />
                              {log.durationMs != null && (
                                <span className="text-vsc-text-faint text-[10px]">
                                  {log.durationMs}ms
                                </span>
                              )}
                              <span className="text-vsc-text-faint text-[9px]">
                                {isExp ? '\u25B4' : '\u25BE'}
                              </span>
                            </button>
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
                          className="flex items-center gap-2 px-[22px] py-1.5 border-b border-vsc-border bg-vsc-surface"
                          key={req.id}
                        >
                          <span className="text-vsc-text-faint text-xs shrink-0">
                            {req.prompt ?? 'Input:'}
                          </span>
                          <input
                            // biome-ignore lint/a11y/noAutofocus: focus the newly requested inline input
                            autoFocus
                            className="flex-1 bg-vsc-bg border border-vsc-border rounded px-2 py-1 text-xs text-vsc-text font-vsc-mono focus:outline-none focus:border-vsc-accent"
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
                      {run.outputChunks.length > 0 && (
                        <div className="py-1.5 pr-2.5 pl-[22px] border-b border-vsc-border-subtle">
                          <div className="text-[10px] font-semibold text-vsc-text-muted mb-0.5 uppercase tracking-wide">
                            Output
                          </div>
                          <RunOutputTerminal
                            chunks={run.outputChunks}
                            runKey={run.id}
                          />
                        </div>
                      )}
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
                                customRenderers={resultRenderers}
                                result={run.errorValue}
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
                            customRenderers={resultRenderers}
                            result={run.result}
                          />
                        </div>
                      )}
                    </div>
                  );
                })}
              </div>
            ) : graphTargetName ? (
              <>
                {/* Graph view */}
                <TabsContent
                  className="flex-1 min-h-0 mt-0 flex flex-col"
                  style={{ minHeight: 300 }}
                  value="graph"
                >
                  {workflowSwitcherBar}
                  {controlFlowGraph ? (
                    <GraphView
                      calls={latestGraphRunSnapshot?.calls}
                      customRenderers={resultRenderers}
                      functionName={graphTargetName}
                      graph={controlFlowGraph}
                      graphRuntimeOverlay={
                        latestGraphRunSnapshot?.graphRuntimeOverlay
                      }
                      onNodeClick={handleGraphNodeClick}
                      run={latestGraphRunSnapshot ?? null}
                      runError={latestGraphRunSnapshot?.error?.message ?? null}
                      runStatus={latestGraphRunSnapshot?.status}
                      selectedNodeId={highlightedNodeId}
                      valueBodyCache={valueBodyCache}
                      valueBodyCacheVersion={valueBodyCacheVersion}
                    />
                  ) : (
                    <div className="flex-1 flex items-center justify-center text-vsc-text-faint text-xs bg-vsc-bg h-full">
                      Loading graph...
                    </div>
                  )}
                </TabsContent>

                {/* Prompt preview */}
                {canPreviewPrompt && (
                  <TabsContent
                    className="flex-1 flex flex-col overflow-hidden mt-0"
                    value="prompt"
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
                            customRenderers={resultRenderers}
                            result={promptPreviewResult}
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
                    className="flex-1 overflow-auto font-vsc-mono text-xs bg-vsc-bg p-2.5 mt-0"
                    value="curl"
                  >
                    {curlPreviewResult != null ? (
                      <ResultDisplay
                        customRenderers={resultRenderers}
                        result={curlPreviewResult}
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
                  className="flex-1 flex flex-col min-h-0 mt-0"
                  value="run"
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
                            className="flex-1 h-7 rounded-none border-none font-vsc-mono text-xs"
                            onChange={onArgsJsonChange}
                            placeholder='{"key": "value"}'
                            spellCheck={false}
                            value={argsJson}
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
                          className="px-1.5 shrink-0"
                          onValueChange={setArgsMode}
                          options={[
                            { label: 'form', value: 'form' },
                            { label: 'raw', value: 'raw' },
                          ]}
                          size="sm"
                          value={argsMode}
                        />
                      )}
                      <Button
                        aria-label={isRunning ? 'Running' : 'Run'}
                        className="mx-1 my-0.5 shrink-0 text-[11px] font-semibold"
                        disabled={
                          runtimeControlsDisabled ||
                          isRunning ||
                          !selectedProject
                        }
                        onClick={onRunFunction}
                        size="xs"
                        variant="success"
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
                    {showArgsForm && paramSchemas && reconciledFormArgs && (
                      <div className="max-h-56 overflow-y-auto px-2 py-1.5 border-t border-vsc-border">
                        {/* Remount on function or schema changes so local widget
                            drafts cannot outlive the schema they represent. */}
                        <ArgsForm
                          key={`${selectedProject ?? ''}:${selectedFn ?? ''}:${argsSchemaKey}`}
                          onChange={onArgsFormChange}
                          params={paramSchemas}
                          types={projectTypes}
                          value={reconciledFormArgs}
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
                        calls={latestGraphRunSnapshot?.calls}
                        customRenderers={resultRenderers}
                        functionName={graphTargetName}
                        graph={controlFlowGraph}
                        graphRuntimeOverlay={
                          latestGraphRunSnapshot?.graphRuntimeOverlay
                        }
                        onNodeClick={handleGraphNodeClick}
                        run={latestGraphRunSnapshot ?? null}
                        runError={
                          latestGraphRunSnapshot?.error?.message ?? null
                        }
                        runStatus={latestGraphRunSnapshot?.status}
                        selectedNodeId={highlightedNodeId}
                        valueBodyCache={valueBodyCache}
                        valueBodyCacheVersion={valueBodyCacheVersion}
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
                    className="absolute left-0 right-0 z-10 h-1.5 cursor-row-resize bg-vsc-surface hover:bg-vsc-accent/30 transition-colors border-y border-vsc-border"
                    onMouseDown={onLogsResizeStart}
                    style={{ bottom: logsPanelHeight }}
                    title="Resize logs"
                  />

                  {/* Run history (scrollable) — full panel width, below the
                      sidebar+content row. */}
                  <div
                    className="absolute left-0 right-0 bottom-0 z-10 overflow-auto font-vsc-mono text-xs bg-vsc-bg"
                    ref={outputRef}
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
                      const isCurrentGeneration =
                        currentUpdate?.generation != null &&
                        run.projectGeneration === currentUpdate.generation;
                      const canViewRunInGraph =
                        functionNames.includes(run.functionName) &&
                        isCurrentGeneration;
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
                          className={
                            !isLatest ? 'border-b-2 border-vsc-border' : ''
                          }
                          key={run.id}
                        >
                          {/* Run header */}
                          <div className="flex items-center bg-vsc-surface border-b border-vsc-border-subtle">
                            <button
                              aria-label={`View ${run.functionName} run in graph`}
                              className="flex flex-1 min-w-0 items-center gap-1.5 px-2.5 py-1.5 text-left hover:bg-vsc-accent/10 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-vsc-accent disabled:cursor-default disabled:hover:bg-transparent"
                              disabled={!canViewRunInGraph}
                              onClick={() => handleSelectHistoryRun(run)}
                              title={
                                !functionNames.includes(run.functionName)
                                  ? 'This function is not available in the current build'
                                  : isCurrentGeneration
                                    ? 'View this run in the graph'
                                    : 'This run belongs to an older build'
                              }
                              type="button"
                            >
                              <span
                                className={`w-1.5 h-1.5 rounded-full shrink-0 ${statusCls}`}
                              />
                              <span className="text-vsc-accent font-semibold text-[11px]">
                                {run.functionName}()
                              </span>
                              <span className="text-vsc-text-faint text-[10px] flex-1 overflow-hidden text-ellipsis whitespace-nowrap">
                                {run.argsJson}
                              </span>
                            </button>
                            {run.status === 'running' && (
                              <>
                                <span className="text-vsc-text-muted text-[10px]">
                                  running...
                                </span>
                                <TooltipProvider>
                                  <Tooltip>
                                    <TooltipTrigger asChild>
                                      <Button
                                        className="mr-1 h-5 w-5 text-vsc-text-muted hover:text-vsc-error"
                                        onClick={() =>
                                          onCancelFunctionRun(run.id)
                                        }
                                        size="icon"
                                        variant="ghost"
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
                              <span className="px-2.5 text-vsc-text-faint text-[10px] shrink-0">
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
                                customRenderers={resultRenderers}
                                result={run.rootInput}
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
                                <button
                                  className="flex w-full items-center gap-1.5 border-0 border-b border-vsc-border-subtle bg-transparent py-0.5 pr-2.5 pl-[22px] text-left cursor-pointer"
                                  onClick={() =>
                                    setExpandedLogId(isExp ? null : log.id)
                                  }
                                  type="button"
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
                                    requestHeaders={log.requestHeaders}
                                    url={log.url}
                                  />
                                  {log.durationMs != null && (
                                    <span className="text-vsc-text-faint text-[10px]">
                                      {log.durationMs}ms
                                    </span>
                                  )}
                                  <span className="text-vsc-text-faint text-[9px]">
                                    {isExp ? '\u25B4' : '\u25BE'}
                                  </span>
                                </button>
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
                              className="flex items-center gap-2 px-[22px] py-1.5 border-b border-vsc-border bg-vsc-surface"
                              key={req.id}
                            >
                              <span className="text-vsc-text-faint text-xs shrink-0">
                                {req.prompt ?? 'Input:'}
                              </span>
                              <input
                                // biome-ignore lint/a11y/noAutofocus: focus the newly requested inline input
                                autoFocus
                                className="flex-1 bg-vsc-bg border border-vsc-border rounded px-2 py-1 text-xs text-vsc-text font-vsc-mono focus:outline-none focus:border-vsc-accent"
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

                          {/* baml.io stream output, rendered as a terminal so
                              the program's own ANSI colors and cursor control
                              land the way they would in a shell. */}
                          {run.outputChunks.length > 0 && (
                            <div className="py-1.5 pr-2.5 pl-[22px] border-b border-vsc-border-subtle">
                              <div className="text-[10px] font-semibold text-vsc-text-muted mb-0.5 uppercase tracking-wide">
                                Output
                              </div>
                              <RunOutputTerminal
                                chunks={run.outputChunks}
                                runKey={run.id}
                              />
                            </div>
                          )}

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
                                    customRenderers={resultRenderers}
                                    result={run.errorValue}
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
                                      durationMs={run.durationMs}
                                      fetchLogs={run.fetchLogs}
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
                                      onValueChange={(v) =>
                                        setResultModes((prev) => ({
                                          ...prev,
                                          [run.id]: v as 'parsed' | 'raw',
                                        }))
                                      }
                                      options={[
                                        { label: 'Parsed', value: 'parsed' },
                                        { label: 'Raw', value: 'raw' },
                                      ]}
                                      size="sm"
                                      value={resultModes[run.id] ?? 'parsed'}
                                    />
                                  )}
                                  <CopyButton
                                    iconSize={11}
                                    text={stringifyResult(run.result)}
                                  />
                                </div>
                                {isLlmFunctionRun &&
                                (resultModes[run.id] ?? 'parsed') === 'raw' ? (
                                  <pre className="whitespace-pre-wrap break-all font-vsc-mono text-[11px] text-vsc-text bg-vsc-bg-secondary p-2 rounded border border-vsc-border max-h-[400px] overflow-auto">
                                    {stringifyResult(run.result)}
                                  </pre>
                                ) : (
                                  <ResultDisplay
                                    customRenderers={resultRenderers}
                                    result={run.result}
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
        envVars={envVars}
        onDeleteEnvVar={removeEnvVar}
        onImportEnvVars={importEnvVars}
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
                  id,
                  type: 'envVarResponse',
                  value,
                  variable: pending.variable,
                });
              }
            }
            pendingEnvRequestsRef.current.clear();
          }
        }}
        onRevertToShell={revertToShell}
        onSetEnvVar={addEnvVar}
        onToggleProxy={setGatewayEnabled}
        open={showApiKeysDialog}
        proxyEnabled={BOUNDARY_PROXY_URL_KEY in envVars}
        requiredKeys={knownRequiredKeys}
        shellDeletedKeys={shellDeletedKeys}
        shellEnvVars={shellEnvVars}
        shellOverriddenKeys={shellOverriddenKeys}
        showProxyEnvVar={getProxyEnvVarConfig().visible}
      />
    </>
  );
};
