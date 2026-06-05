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

import type { ChangeEvent, FC, ReactNode, RefObject } from 'react';
import { useCallback, useEffect, useRef, useState } from 'react';
import { encodeCallArgs } from '@b/pkg-proto';
import type { BamlJsValue } from '@b/pkg-proto';
import { KeyRound, PanelLeft, Square } from 'lucide-react';
import { Button } from './components/ui/button';
import { Tabs, TabsContent, TabsList, TabsTrigger } from './components/ui/tabs';
import { Input } from './components/ui/input';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from './components/ui/tooltip';
import { CodeBlock } from './components/ui/code-block';
import { ToggleGroup } from './components/ui/toggle-group';
import { cn } from './lib/utils';
import { ApiKeysDialog } from './components/ApiKeysDialog';
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
  ProjectUpdate,
  RunEntry,
  WorkerOutMessage,
} from './worker-protocol';
import type { ResultRendererProps } from './result-renderers';
import { ResultDisplay } from './ResultDisplay';
import { registerBuiltinResultRenderers } from './renderers/registerBuiltins';
import { HttpRequestCurlRenderer, isHttpRequest } from './renderers/HttpRequestCurl';
import { GraphView } from './graph/GraphView';
import { FunctionSidebar } from './FunctionSidebar';
import { EventValueDisplay } from './EventValueDisplay';

registerBuiltinResultRenderers();

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

function stringifyResult(value: BamlJsValue): string {
  return JSON.stringify(value, (_, v) => (typeof v === 'bigint' ? v.toString() : v), 2);
}

function formatBuildTime(epochSecs: number): { absolute: string; relative: string } {
  const d = new Date(epochSecs * 1000);
  const absolute = d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit', hour12: false });
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
  onNavigateToSource?: (source: { fileId: number; line: number; column: number }) => void;
  /** Initial value for the function arguments JSON input. Defaults to '{}'. */
  initialArgsJson?: string;
  /** Initial function to select once the runtime reports it. */
  initialFunctionName?: string;
  /** Optional allow-list for functions shown in the left sidebar. */
  visibleFunctionNames?: string[];
  /** Optional per-function example args JSON. When the user selects a function
   *  from the sidebar and an entry exists here, the args input is pre-filled
   *  (only when the args field is empty / `{}` so we don't clobber edits). */
  argsByFunction?: Record<string, string>;
}

// ---------------------------------------------------------------------------
// CollectionRunView — renders fetch logs from a collection/expansion RunEntry
// ---------------------------------------------------------------------------

interface CollectionRunViewProps {
  run: RunEntry;
  expandedLogId: number | null;
  setExpandedLogId: (id: number | null) => void;
  resultRenderers?: Record<string, FC<ResultRendererProps>>;
}

const CollectionRunView: FC<CollectionRunViewProps> = ({ run, expandedLogId, setExpandedLogId, resultRenderers }) => {
  const hasError = run.status === 'error';
  const errorMessage = run.error || 'Unknown expansion error';
  return (
    <div className="flex-1 flex flex-col min-h-0">
      {/* Header */}
      <div className="flex items-center gap-1.5 px-2.5 py-1.5 bg-vsc-surface border-b border-vsc-border shrink-0">
        <span className={cn('w-1.5 h-1.5 rounded-full shrink-0', hasError ? 'bg-vsc-red' : 'bg-vsc-green')} />
        <span className="text-vsc-accent font-semibold text-[11px]">$collect_tests</span>
        <span className="text-vsc-text-faint text-[10px] flex-1">{hasError ? 'expansion error' : 'collection fetch logs'}</span>
        <span className="text-vsc-text-faint text-[10px]">{run.fetchLogs.length} request{run.fetchLogs.length !== 1 ? 's' : ''}</span>
      </div>
      {/* Error message */}
      {hasError && (
        <div className="px-2.5 py-2 bg-vsc-surface border-b border-vsc-border">
          <div className="text-[10px] font-semibold text-red-500 mb-1 uppercase tracking-wide">Expansion Error</div>
          <pre className="text-[11px] text-vsc-text whitespace-pre-wrap font-vsc-mono bg-vsc-bg p-2 rounded border border-vsc-border overflow-auto max-h-[300px]">{errorMessage}</pre>
        </div>
      )}
      {/* Fetch logs */}
      <div className="flex-1 overflow-auto font-vsc-mono text-xs bg-vsc-bg">
        {run.fetchLogs.length === 0 && !hasError && (
          <div className="p-5 text-center text-vsc-text-faint text-[11px]">
            No fetch logs — collection may not have made any HTTP requests
          </div>
        )}
        {run.fetchLogs.map((log) => {
          const isExp = expandedLogId === log.id;
          const statusColorCls = log.status === null ? 'text-vsc-text-muted'
            : log.status >= 200 && log.status < 300 ? 'text-vsc-green'
            : log.status === 0 ? 'text-vsc-red' : 'text-vsc-yellow';
          return (
            <div key={`cl-${log.id}`}>
              <div
                onClick={() => setExpandedLogId(isExp ? null : log.id)}
                className="flex items-center gap-1.5 py-0.5 pr-2.5 pl-[22px] cursor-pointer border-b border-vsc-border-subtle"
              >
                <span className={`${statusColorCls} font-semibold text-[11px]`}>{log.status ?? '...'}</span>
                <span className="text-vsc-text-faint text-[10px]">{log.method}</span>
                <span className="text-vsc-text flex-1 overflow-hidden text-ellipsis whitespace-nowrap text-[11px]">{log.url}</span>
                {log.durationMs != null && <span className="text-vsc-text-faint text-[10px]">{log.durationMs}ms</span>}
                <span className="text-vsc-text-faint text-[9px]">{isExp ? '\u25B4' : '\u25BE'}</span>
              </div>
              {isExp && (
                <div className="py-2 pr-2.5 pl-[22px] flex flex-col gap-2 border-b border-vsc-border">
                  {log.error && <CodeBlock variant="error">{log.error}</CodeBlock>}
                  <div>
                    <div className="text-[10px] font-semibold text-vsc-text-muted mb-0.5 uppercase tracking-wide">Request Headers</div>
                    <CodeBlock>{JSON.stringify(log.requestHeaders, null, 2)}</CodeBlock>
                  </div>
                  {log.requestBody && (
                    <div>
                      <div className="text-[10px] font-semibold text-vsc-text-muted mb-0.5 uppercase tracking-wide">Request Body</div>
                      <CodeBlock>{tryFormatJson(log.requestBody)}</CodeBlock>
                    </div>
                  )}
                  {log.responseBody != null && (
                    <div>
                      <div className="text-[10px] font-semibold text-vsc-text-muted mb-0.5 uppercase tracking-wide">Response Body</div>
                      <CodeBlock>{tryFormatJson(log.responseBody)}</CodeBlock>
                    </div>
                  )}
                </div>
              )}
            </div>
          );
        })}
        {/* Runtime events */}
        {run.runtimeEvents.length > 0 && (
          <div className="py-1.5 pr-2.5 pl-[22px] border-b border-vsc-border-subtle">
            <div className="text-[10px] font-semibold text-vsc-text-muted mb-1 uppercase tracking-wide">
              Events ({run.runtimeEvents.length})
            </div>
            <div className="flex flex-col gap-0.5">
              {run.runtimeEvents.map((evt, evtIdx) => {
                const kind = evt.event;
                if (!kind) return null;

                let label: string;
                let payload: ReactNode;
                let colorCls: string;

                switch (kind.$case) {
                  case 'functionStart':
                    label = 'START';
                    payload = kind.functionStart.name;
                    colorCls = 'text-vsc-green';
                    break;
                  case 'functionEnd':
                    label = 'END';
                    payload = `${kind.functionEnd.name} (${kind.functionEnd.durationMs}ms)`;
                    colorCls = 'text-vsc-text-muted';
                    break;
                  case 'log': {
                    const lvl = kind.log.level;
                    label = lvl;
                    payload = <EventValueDisplay value={kind.log.data} customRenderers={resultRenderers} />;
                    colorCls = lvl === 'error' ? 'text-vsc-red'
                      : lvl === 'warn' ? 'text-vsc-yellow'
                      : lvl === 'debug' ? 'text-vsc-text-muted'
                      : 'text-vsc-blue';
                    break;
                  }
                  case 'custom':
                    label = 'EVENT';
                    payload = <><span>{kind.custom.name}: </span><EventValueDisplay value={kind.custom.data} customRenderers={resultRenderers} /></>;
                    colorCls = 'text-vsc-purple';
                    break;
                  case 'setTags':
                    label = 'TAGS';
                    payload = kind.setTags.tags.map(t => `${t.key}=${t.value}`).join(', ');
                    colorCls = 'text-vsc-text-muted';
                    break;
                  default:
                    return null;
                }

                return (
                  <div key={evtIdx} className="flex items-start gap-1.5 text-[11px]">
                    <span className={`${colorCls} font-semibold uppercase shrink-0`}>{label}</span>
                    <span className="text-vsc-text break-all">{payload}</span>
                  </div>
                );
              })}
            </div>
          </div>
        )}
      </div>
    </div>
  );
};

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export const ExecutionPanel: FC<ExecutionPanelProps> = ({ port, connectionVersion, resultRenderers, onReload, onNavigateToSource, initialArgsJson, initialFunctionName, visibleFunctionNames, argsByFunction }) => {
  const [projectRoots, setProjectRoots] = useState<string[]>([]);
  const [projectUpdates, setProjectUpdates] = useState<Record<string, ProjectUpdate>>({});
  const [testTree, setTestTree] = useState<any>(null);
  const [collectionCallId, setCollectionCallId] = useState<number | null>(null);
  const [generation, setGeneration] = useState<number>(0);
  const [testRunResults, setTestRunResults] = useState<Map<string, unknown>>(new Map());
  const [failedExpands, setFailedExpands] = useState<Set<string>>(new Set());
  // Synthetic RunEntry that accumulates fetch logs from test collection/expansion operations
  const [collectionRun, setCollectionRun] = useState<RunEntry | null>(null);
  // When true, the main content area shows the collection run's fetch logs
  const [viewingCollection, setViewingCollection] = useState(false);
  // When true, the main content area shows the test run history panel
  const [viewingTestRun, setViewingTestRun] = useState(false);
  const [selectedProject, setSelectedProject] = useState<string | null>(null);

  const [selectedFn, setSelectedFn] = useState<string | null>(
    initialFunctionName ?? null,
  );
  const [argsJson, setArgsJson] = useState(initialArgsJson ?? '{}');

  // Run history — each entry is a complete invocation with its logs + result
  const [runs, setRuns] = useState<RunEntry[]>([]);
  const [expandedLogId, setExpandedLogId] = useState<number | null>(null);
  const outputRef = useRef<HTMLDivElement>(null);
  const promptContentRef = useRef<HTMLDivElement>(null);

  const [controlFlowGraph, setControlFlowGraph] = useState<ControlFlowGraph | null>(null);
  const [activeTab, setActiveTab] = useState<'run' | 'graph' | 'prompt' | 'curl'>('run');
  const [highlightedNodeId, setHighlightedNodeId] = useState<number | null>(null);
  const [cursorOffset, setCursorOffset] = useState<number | null>(null);

  // Workflow context: when a function belongs to multiple workflows,
  // this tracks which workflow is being viewed and the alternatives.
  const [workflowContext, setWorkflowContext] = useState<{
    functionName: string;
    workflows: string[];
  } | null>(null);
  const [promptPreviewResult, setPromptPreviewResult] = useState<BamlJsValue | null>(null);
  const [curlPreviewResult, setCurlPreviewResult] = useState<BamlJsValue | null>(null);
  const [promptPreviewError, setPromptPreviewError] = useState<string | null>(null);
  const [curlPreviewError, setCurlPreviewError] = useState<string | null>(null);
  const [previewLoading, setPreviewLoading] = useState(false);
  const [sidebarOpen, setSidebarOpen] = useState(true);
  const [sidebarWidth, setSidebarWidth] = useState(220);
  const resizingRef = useRef(false);
  const [resultModes, setResultModes] = useState<Record<number, 'parsed' | 'raw'>>({});

  const [showApiKeysDialog, setShowApiKeysDialog] = useState(false);
  const showApiKeysDialogRef = useRef(false);

  // Pending io.input() requests keyed by callId, each entry is a queue of { id, prompt }
  const [pendingInputs, setPendingInputs] = useState<Map<number, Array<{ id: number; prompt: string | undefined }>>>(new Map());

  const [diagsExpanded, setDiagsExpanded] = useState(false);
  const [buildTime, setBuildTime] = useState<number | null>(null);
  const [wasmPanic, setWasmPanic] = useState<{ message: string; stack?: string } | null>(null);
  const { envVars, knownRequiredKeys, addEnvVar, removeEnvVar, importEnvVars, addRequiredKey } = useEnvVars(port);
  // In-flight worker requests waiting for a value: id → variable name. Ref because it doesn't drive renders.
  const pendingEnvRequestsRef = useRef<Map<number, string>>(new Map());

  // Ref mirror of envVars so the message handler closure always sees current values.
  const envVarsRef = useRef(envVars);
  useEffect(() => { envVarsRef.current = envVars; }, [envVars]);

  // Ref mirrors for cursor context handler (avoids stale closures in port.onMessage).
  const selectedFnRef = useRef(selectedFn);
  useEffect(() => { selectedFnRef.current = selectedFn; }, [selectedFn]);

  // When the user switches function in the sidebar, fill args from the example
  // map if the host provided one. Skips the very first run so initialArgsJson
  // wins on mount.
  const prevSelectedFnRef = useRef<string | null>(selectedFn);
  useEffect(() => {
    const prev = prevSelectedFnRef.current;
    prevSelectedFnRef.current = selectedFn;
    if (prev === selectedFn) return;
    if (!selectedFn || !argsByFunction) return;
    const example = argsByFunction[selectedFn];
    if (example != null) setArgsJson(example);
  }, [selectedFn, argsByFunction]);
  const controlFlowGraphRef = useRef(controlFlowGraph);
  useEffect(() => { controlFlowGraphRef.current = controlFlowGraph; }, [controlFlowGraph]);

  const nextCallIdRef = useRef(0);
  const pendingCallsRef = useRef<Map<number, { resolve: (v: BamlJsValue) => void; reject: (e: Error) => void }>>(new Map());
  // Buffer fetch logs by callId so logs that arrive before testCollectionResult are not lost.
  const pendingLogsRef = useRef<Map<number, FetchLogEntry[]>>(new Map());

  // ── Cursor context navigation ────────────────────────────────────────

  /** Build a lookup from sourceExpr → nodeId for the cached CFG.
   *  When multiple nodes share a sourceExpr, prefer semantic types
   *  (call/loop/branch/header) over structural ones (branchArm). */
  function buildSourceExprIndex(graph: ControlFlowGraph | null): Map<number, number> {
    const map = new Map<number, number>();
    if (!graph) return map;
    const preferred = new Set(['otherScope', 'loop', 'branchGroup', 'headerContextEnter']);
    for (const [, node] of Object.entries(graph.nodes)) {
      if (node.sourceExpr == null) continue;
      if (preferred.has(node.nodeType)) {
        map.set(node.sourceExpr, node.id);
      } else if (!map.has(node.sourceExpr)) {
        map.set(node.sourceExpr, node.id);
      }
    }
    return map;
  }

  /** Try each candidate expression ID (most-specific first) against the
   *  graph, returning the first node that matches. This gives "closest
   *  ancestor" highlighting — e.g. cursor on a local variable inside a
   *  call highlights the call; cursor on `if` keyword highlights the
   *  branch group; cursor inside a branch arm body highlights the arm. */
  function resolveCandidatesToNodeId(
    graph: ControlFlowGraph | null,
    candidates: number[],
  ): number | null {
    if (!graph || candidates.length === 0) return null;
    const index = buildSourceExprIndex(graph);
    for (const exprId of candidates) {
      const nodeId = index.get(exprId);
      if (nodeId != null) return nodeId;
    }
    return null;
  }

  /** Find a graph node whose label starts with `funcName(` — used when candidate
   *  matching fails because the cursor is on a callee Path expression but the
   *  graph stores the Call expression. */
  function resolveNodeByFunctionName(
    graph: ControlFlowGraph | null,
    funcName: string,
  ): number | null {
    if (!graph || !funcName) return null;
    const prefix = `${funcName}(`;
    for (const [, node] of Object.entries(graph.nodes)) {
      if (node.label.startsWith(prefix)) return node.id;
    }
    return null;
  }

  function handleCursorContext(ctx: CursorContext) {
    // Update cursor offset for event highlighting (cursor ↔ event matching)
    console.log('[DEBUG] CursorContext:', { cursorOffset: ctx.cursorOffset, functionName: ctx.functionName });
    setCursorOffset(ctx.cursorOffset ?? null);

    if (!ctx.functionName) return;

    const currentFn = selectedFnRef.current;
    const cachedGraph = controlFlowGraphRef.current;

    const candidates = ctx.sourceExprCandidates ?? [];
    const nodeId = resolveCandidatesToNodeId(cachedGraph, candidates)
      ?? (ctx.sourceExprId != null ? resolveNodeByFunctionName(cachedGraph, ctx.functionName) : null);

    // Rule 1: cursor is on a node in the currently-displayed workflow
    if (nodeId != null && ctx.functionName === currentFn) {
      setHighlightedNodeId(nodeId);
      return;
    }

    // Rule 2: cursor is on a call site inside the current workflow
    if (nodeId != null && ctx.workflowMemberships.includes(currentFn ?? '')) {
      setHighlightedNodeId(nodeId);
      return;
    }

    // Rule 3: navigate to the function the cursor is on.
    // Always show THAT function's own graph — never auto-redirect to a
    // workflow. If the function is called from workflows, expose them via
    // the "called from" picker so the user can opt in.
    if (ctx.functionName !== currentFn) {
      setSelectedFn(ctx.functionName);
      setHighlightedNodeId(null);
    }
    // Update "called from" context (shown as a picker above the graph).
    // Set on every navigation, including when already on the function,
    // so it reflects the current membership info.
    if (ctx.workflowMemberships.length > 0) {
      setWorkflowContext({
        functionName: ctx.functionName,
        workflows: ctx.workflowMemberships,
      });
    } else {
      setWorkflowContext(null);
    }
  }

  // ── Port message handler ─────────────────────────────────────────────

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
              break;
            case 'testCollectionResult': {
              try {
                const jsonStr = new TextDecoder().decode(new Uint8Array(n.data));
                const tree = JSON.parse(jsonStr);

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
                setTestRunResults(new Map());

                // Create/replace the synthetic RunEntry for collection, hydrating any
                // fetch logs that arrived before this notification.
                const buffered = pendingLogsRef.current.get(n.callId) ?? [];
                pendingLogsRef.current.delete(n.callId);
                const hasError = !!n.expandError;
                const collectionEntry: RunEntry = {
                  id: n.callId,
                  functionName: '$collect_tests',
                  argsJson: '',
                  fetchLogs: buffered,
                  runtimeEvents: [],
                  result: null,
                  error: hasError ? n.expandError!.message : null,
                  status: hasError ? 'error' : 'success',
                  startTime: performance.now(),
                  durationMs: null,
                };
                setCollectionRun(collectionEntry);
              } catch (e) {
                console.error('[testCollectionResult] decode error:', e);
              }
              break;
            }
            case 'openPlayground':
              setSelectedProject(n.project);
              if (n.functionName) setSelectedFn(n.functionName);
              break;
            case 'controlFlowGraphResult':
              if (n.graph) setControlFlowGraph(n.graph);
              break;
          }
          break;
        }

        case 'callFunctionResult': {
          const pending = pendingCallsRef.current.get(data.id);
          if (pending) {
            pendingCallsRef.current.delete(data.id);
            pending.resolve(data.result);
          }
          break;
        }

        case 'callFunctionError': {
          const pending = pendingCallsRef.current.get(data.id);
          if (pending) {
            pendingCallsRef.current.delete(data.id);
            const err = new Error(data.error);
            (err as any).cancelled = data.cancelled ?? false;
            pending.reject(err);
          }
          break;
        }

        case 'fetchLogNew': {
          const logEntry = data.entry;
          // Always buffer by callId so logs that arrive before testCollectionResult are not lost.
          const existing = pendingLogsRef.current.get(logEntry.callId);
          if (existing) {
            existing.push(logEntry);
          } else {
            pendingLogsRef.current.set(logEntry.callId, [logEntry]);
          }
          // Route to collection run if callId matches
          setCollectionRun((prev) => {
            if (prev && logEntry.callId === prev.id) {
              return { ...prev, fetchLogs: [...prev.fetchLogs, logEntry] };
            }
            return prev;
          });
          // Route to regular runs
          setRuns((prev) => {
            const targetIdx = prev.findIndex((r) => r.id === logEntry.callId);
            if (targetIdx === -1) return prev;
            const target = prev[targetIdx];
            return [...prev.slice(0, targetIdx), { ...target, fetchLogs: [...target.fetchLogs, logEntry] }, ...prev.slice(targetIdx + 1)];
          });
          break;
        }

        case 'fetchLogUpdate':
          // Also update collection run logs
          setCollectionRun((prev) => {
            if (!prev) return prev;
            const updated = prev.fetchLogs.map((e) => (e.id === data.logId ? { ...e, ...data.patch } : e));
            if (updated === prev.fetchLogs) return prev;
            return { ...prev, fetchLogs: updated };
          });
          setRuns((prev) =>
            prev.map((r) => ({
              ...r,
              fetchLogs: r.fetchLogs.map((e) => (e.id === data.logId ? { ...e, ...data.patch } : e)),
            })),
          );
          break;

        case 'runtimeEventNew': {
          const eventEntry = data.event;
          if (data.callId != null) {
            setRuns((prev) =>
              prev.map((r) =>
                r.id === data.callId
                  ? { ...r, runtimeEvents: [...r.runtimeEvents, eventEntry] }
                  : r
              )
            );
            // Also route to collection run if this event belongs to it
            setCollectionRun((prev) => {
              if (!prev || prev.id !== data.callId) return prev;
              return { ...prev, runtimeEvents: [...prev.runtimeEvents, eventEntry] };
            });
          }
          break;
        }

        case 'envVarRequest': {
          // Always track as a known required key (proactive indicator)
          addRequiredKey(data.variable);
          const cached = envVarsRef.current[data.variable];
          if (cached !== undefined) {
            port.postMessage({ type: 'envVarResponse', id: data.id, value: cached, variable: data.variable });
          } else {
            // Park the request — it will be resolved when the dialog closes
            pendingEnvRequestsRef.current.set(data.id, data.variable);
            if (!showApiKeysDialogRef.current) {
              setShowApiKeysDialog(true);
              showApiKeysDialogRef.current = true;
            }
          }
          break;
        }

        case 'inputRequest': {
          const { id, prompt, callId } = data;
          setPendingInputs((prev) => {
            const next = new Map(prev);
            const arr = next.get(callId) ?? [];
            next.set(callId, [...arr, { id, prompt }]);
            return next;
          });
          break;
        }

        case "ready":
          break;

        case 'buildTime':
          setBuildTime(Number(data.value) || null);
          break;

        case "vfsFileChanged":
        case "vfsFileDeleted":
        case "diagnostics":
          break;

        case "controlFlowGraphResult":
          if (data.graph) setControlFlowGraph(data.graph);
          break;

        case "cursorContext":
          handleCursorContext(data.context);
          break;

        case "wasmPanic":
          setWasmPanic({ message: data.message, stack: data.stack });
          break;

        case "logDecorations":
        case "clearLogDecorations":
          // These are handled by MonacoEditor, ignore here
          break;

        case "runtimeEventError":
          console.warn('[ExecutionPanel] runtimeEventError:', data.error);
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

  // Request control flow graph when selected function changes OR code is edited.
  // On function/project switch: clear the graph (shows loading state).
  // On code edit (projectUpdateVersion): keep old graph visible, swap when new one arrives.
  const prevGraphFnRef = useRef(selectedFn);
  const prevGraphProjectRef = useRef(selectedProject);
  const projectUpdateVersion = selectedProject ? projectUpdates[selectedProject] : undefined;

  useEffect(() => {
    const fnChanged = prevGraphFnRef.current !== selectedFn;
    const projChanged = prevGraphProjectRef.current !== selectedProject;
    prevGraphFnRef.current = selectedFn;
    prevGraphProjectRef.current = selectedProject;

    if (fnChanged || projChanged) {
      setControlFlowGraph(null);
      setHighlightedNodeId(null);
    }
    if (!selectedFn || !selectedProject) return;
    port.postMessage({ type: 'requestControlFlowGraph', project: selectedProject, functionName: selectedFn });
  }, [port, selectedFn, selectedProject, projectUpdateVersion]);

  // Clear preview results when selected function changes
  useEffect(() => {
    setPromptPreviewResult(null);
    setCurlPreviewResult(null);
    setPromptPreviewError(null);
    setCurlPreviewError(null);
    setPreviewLoading(false);
  }, [selectedFn]);

  // Auto-refresh prompt/curl preview when args change while tab is active
  useEffect(() => {
    if (activeTab !== 'prompt' && activeTab !== 'curl') return;
    if (!selectedFn || !selectedProject) return;

    const subFn = activeTab === 'prompt' ? 'render_prompt' : 'build_request';
    const setResult = activeTab === 'prompt' ? setPromptPreviewResult : setCurlPreviewResult;
    const setError = activeTab === 'prompt' ? setPromptPreviewError : setCurlPreviewError;

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

    if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) {
      setPreviewLoading(false);
      setError('Args must be a JSON object');
      return;
    }

    setPreviewLoading(true);

    const timer = setTimeout(async () => {
      try {
        const argsProto = encodeCallArgs(parsed as Record<string, unknown>);
        const callId = nextCallIdRef.current++;
        const resultValue = await new Promise<BamlJsValue>((resolve, reject) => {
          pendingCallsRef.current.set(callId, { resolve, reject });
          port.postMessage({
            type: 'callFunction',
            id: callId,
            name: `${selectedFn}$${subFn}`,
            argsProto: new Uint8Array(argsProto),
            project: selectedProject,
          });
        });
        setResult(resultValue);
        setError(null);
        setPreviewLoading(false);
      } catch (e) {
        const errMsg = e instanceof Error ? e.message : String(e);
        // Don't clear result — keep last valid prompt visible with error banner above
        setError(errMsg);
        setPreviewLoading(false);
      }
    }, 500);

    return () => { clearTimeout(timer); setPreviewLoading(false); };
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activeTab, selectedFn, selectedProject, argsJson, port, projectUpdateVersion]);

  // Sync existing envVars to the port whenever port changes
  useEffect(() => {
    for (const [key, value] of Object.entries(envVars)) {
      port.postMessage({ type: 'setEnvVar', key, value });
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps -- only sync when port changes
  }, [port]);

  const onArgsJsonChange = useCallback((e: ChangeEvent<HTMLInputElement>) => {
    setArgsJson(e.target.value);
  }, []);

  // ── Run function ───────────────────────────────────────────────────────

  const isRunning = runs.length > 0 && runs[runs.length - 1].status === 'running';

  const onCancelRun = useCallback((runId: number) => {
    if (!selectedProject) return;
    port.postMessage({ type: 'cancelCall', id: runId, project: selectedProject });
  }, [port, selectedProject]);

  const submitInput = useCallback((id: number, value: string, callId: number) => {
    port.postMessage({ type: 'inputResponse', id, value });
    setPendingInputs((prev) => {
      const next = new Map(prev);
      const arr = (next.get(callId) ?? []).filter((r) => r.id !== id);
      if (arr.length === 0) next.delete(callId);
      else next.set(callId, arr);
      return next;
    });
  }, [port]);

  const toggleResultMode = useCallback((runId: number) => {
    setResultModes((prev) => ({
      ...prev,
      [runId]: (prev[runId] ?? 'parsed') === 'parsed' ? 'raw' : 'parsed',
    }));
  }, []);

  const onResizeStart = useCallback((e: React.MouseEvent) => {
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
  }, [sidebarWidth]);

  const onRunFunction = useCallback(async () => {
    if (!selectedFn || !selectedProject || isRunning) return;

    const runId = nextCallIdRef.current++;
    const startTime = performance.now();
    const newRun: RunEntry = {
      id: runId,
      functionName: selectedFn,
      argsJson,
      fetchLogs: [],
      runtimeEvents: [],
      result: null,
      error: null,
      status: 'running',
      startTime,
      durationMs: null,
    };
    setRuns((prev) => [...prev, newRun]);
    setExpandedLogId(null);

    requestAnimationFrame(() => {
      outputRef.current?.scrollTo({ top: 0, behavior: 'smooth' });
    });

    try {
      const parsed = JSON.parse(argsJson);
      if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) {
        throw new Error('Arguments must be a JSON object, e.g. {"arr": [3,1,2]}');
      }
      const argsProto = encodeCallArgs(parsed as Record<string, unknown>);

      const resultValue = await new Promise<BamlJsValue>((resolve, reject) => {
        pendingCallsRef.current.set(runId, { resolve, reject });
        port.postMessage(
          { type: 'callFunction', id: runId, name: selectedFn, argsProto: new Uint8Array(argsProto), project: selectedProject },
        );
      });

      const dur = Math.round(performance.now() - startTime);
      setRuns((prev) => prev.map((r) => r.id === runId ? { ...r, result: resultValue, status: 'success', durationMs: dur } : r));
    } catch (e) {
      const isCancelled = e instanceof Error && (e as any).cancelled === true;
      const errMsg = e instanceof Error ? e.message : String(e);
      const dur = Math.round(performance.now() - startTime);
      setRuns((prev) => prev.map((r) => r.id === runId ? {
        ...r,
        error: isCancelled ? null : errMsg,
        status: isCancelled ? 'cancelled' : 'error',
        durationMs: dur,
      } : r));
    }
  }, [selectedFn, selectedProject, argsJson, isRunning, port]);

  const handleRefreshTests = useCallback(() => {
    if (!selectedProject) return;
    port.postMessage({ type: 'requestCollectTests', project: selectedProject });
  }, [selectedProject, port]);

  const handleRunTest = useCallback(async (name: string) => {
    if (!selectedProject) return;
    // Switch to the test run view so the runs panel is visible even when no function is selected.
    setViewingTestRun(true);
    setViewingCollection(false);
    const runId = nextCallIdRef.current++;
    const newRun: RunEntry = {
      id: runId,
      functionName: 'testing.run_test',
      testName: name,
      argsJson: `(test: ${name})`,
      fetchLogs: [],
      runtimeEvents: [],
      result: null,
      error: null,
      status: 'running',
      startTime: performance.now(),
      durationMs: null,
    };
    setRuns((prev) => [...prev, newRun]);

    try {
      const resultValue = await new Promise<BamlJsValue>((resolve, reject) => {
        pendingCallsRef.current.set(runId, { resolve, reject });
        port.postMessage({
          type: 'callTestFunction',
          id: runId,
          project: selectedProject,
          generation,
          testName: name,
        });
      });

      const dur = Math.round(performance.now() - newRun.startTime);
      setRuns((prev) =>
        prev.map((r) =>
          r.id === runId
            ? { ...r, result: resultValue, status: 'success', durationMs: dur }
            : r,
        ),
      );

      setTestRunResults((prev) => new Map(prev).set(name, resultValue));
    } catch (e: any) {
      const dur = Math.round(performance.now() - newRun.startTime);
      const cancelled = e instanceof Error && (e as any).cancelled === true;
      setRuns((prev) =>
        prev.map((r) =>
          r.id === runId
            ? { ...r, error: cancelled ? null : (e instanceof Error ? e.message : String(e)), status: cancelled ? 'cancelled' : 'error', durationMs: dur }
            : r,
        ),
      );

      setTestRunResults((prev) =>
        new Map(prev).set(name, { outcome: 'error', error: e instanceof Error ? e.message : String(e) }),
      );
    }
  }, [selectedProject, generation, port]);

  // Track which testsets we've already requested expansion for (per generation)
  const pendingExpandsRef = useRef<{ project: string | null; generation: number; names: Set<string> }>({ project: null, generation: -1, names: new Set() });

  // Auto-expand lazy testsets after receiving a new testTree
  useEffect(() => {
    if (!testTree || !selectedProject) return;
    // Reset pending set and failed state when generation or project changes.
    // Generation is per-project on the server, so different projects can share
    // the same generation number — we must track both to avoid leaking state.
    if (pendingExpandsRef.current.generation !== generation || pendingExpandsRef.current.project !== selectedProject) {
      pendingExpandsRef.current = { project: selectedProject, generation, names: new Set() };
      setFailedExpands(new Set());
    }
    const pending = pendingExpandsRef.current.names;
    const expandLazy = (items: any[]) => {
      for (const item of items) {
        if (item && item.type === 'lazyTestSet' && !pending.has(item.name)) {

          pending.add(item.name);
          port.postMessage({
            type: 'expandTestSet',
            project: selectedProject,
            generation,
            testsetName: item.name,
          });
        } else if (item && item.items && Array.isArray(item.items)) {
          // Recurse into expanded testsets to find nested lazy items
          expandLazy(item.items);
        }
      }
    };
    if (Array.isArray(testTree)) {
      expandLazy(testTree);
    }
  }, [testTree, selectedProject, generation, port]);

  // Retry expansion for a failed (or already expanded) testset
  const handleRetryExpand = useCallback((testsetName: string) => {
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
      type: 'expandTestSet',
      project: selectedProject,
      generation,
      testsetName,
    });
  }, [selectedProject, generation, port]);

  // ── Derived state ──────────────────────────────────────────────────────

  const currentUpdate = selectedProject ? projectUpdates[selectedProject] : undefined;
  const functions: FunctionInfo[] = currentUpdate?.functions ?? [];
  const visibleFunctionNameSet =
    visibleFunctionNames && visibleFunctionNames.length > 0
      ? new Set(visibleFunctionNames)
      : null;
  const sidebarFunctions = visibleFunctionNameSet
    ? functions.filter((f) => visibleFunctionNameSet.has(f.name))
    : functions;
  const functionNames = sidebarFunctions.map((f) => f.name);
  const engineStale = currentUpdate ? !currentUpdate.isBexCurrent : false;
  const diags = currentUpdate?.diagnostics ?? [];

  const selectedFnInfo = functions.find((f) => f.name === selectedFn);
  const canPreviewPrompt = selectedFnInfo?.capabilities?.renderPrompt ?? false;
  const canPreviewCurl = selectedFnInfo?.capabilities?.buildRequest ?? false;

  useEffect(() => {
    if (functionNames.length === 0) return;
    setSelectedFn((prev) => prev && !functionNames.includes(prev) ? null : prev);
  }, [functionNames]);

  // Reset active tab if current tab is no longer available for the selected function
  useEffect(() => {
    if (activeTab === 'prompt' && !canPreviewPrompt) setActiveTab('run');
    if (activeTab === 'curl' && !canPreviewCurl) setActiveTab('run');
  }, [activeTab, canPreviewPrompt, canPreviewCurl]);

  const errors = diags.filter((d) => d.severity === 'error');
  const warnings = diags.filter((d) => d.severity === 'warning');
  const hasErrors = errors.length > 0;

  // Whether any known-required keys are missing — proactive, not just reactive to pending requests
  const hasMissingKeys = [...knownRequiredKeys].some((k) => !envVars[k]);

  // ── Render ─────────────────────────────────────────────────────────────

  return (
    <>
      {buildTime != null && (
        <span data-testid="hot-reload-test" style={{ display: 'none' }}>{buildTime}</span>
      )}

      {/* ──── Status bar ──── */}
      <div className="flex items-center gap-2 px-2.5 py-1 shrink-0 border-b border-vsc-border bg-vsc-surface">
        {connectionVersion != null && (
          <code className="text-[10px] font-vsc-mono text-vsc-text-faint">v{connectionVersion}</code>
        )}
        {buildTime != null && (() => {
          const { absolute, relative } = formatBuildTime(buildTime);
          return (
            <code className="text-[10px] font-vsc-mono text-vsc-text-faint">
              {absolute} ({relative})
            </code>
          );
        })()}
        <div className="flex-1" />
        <TooltipProvider>
          <Tooltip>
            <TooltipTrigger asChild>
              <Button variant="ghost" size="icon" className="relative h-7 w-7" onClick={() => setShowApiKeysDialog(true)}>
                <KeyRound size={14} />
                {hasMissingKeys && (
                  <span className="absolute -top-0.5 -right-0.5 w-2 h-2 rounded-full bg-yellow-400" />
                )}
              </Button>
            </TooltipTrigger>
            <TooltipContent>API Keys</TooltipContent>
          </Tooltip>
        </TooltipProvider>
      </div>

      {/* Project selector (shown when multiple projects exist) */}
      {projectRoots.length > 1 && (
        <div className="flex items-center gap-1.5 px-2.5 py-1 border-b border-vsc-border shrink-0 bg-vsc-surface">
          <span className="text-[10px] text-vsc-text-faint font-vsc-mono select-none">PROJECT</span>
          <ToggleGroup
            value={selectedProject ?? projectRoots[0]}
            onValueChange={(v) => setSelectedProject(v)}
            options={projectRoots.map((root) => ({
              value: root,
              label: (
                <>
                  {root}
                  {projectUpdates[root] && !projectUpdates[root].isBexCurrent && (
                    <span className="ml-0.5 text-vsc-yellow">*</span>
                  )}
                </>
              ),
            }))}
            size="sm"
          />
        </div>
      )}

      {/* Project state info (single project) */}
      {projectRoots.length === 1 && (
        <div className="flex items-center gap-1.5 px-2.5 py-1 border-b border-vsc-border shrink-0 bg-vsc-surface">
          <span className="text-[10px] text-vsc-text-faint font-vsc-mono select-none">PROJECT</span>
          <span className="text-[10px] font-vsc-mono text-vsc-text-muted">
            {projectRoots[0]}
          </span>
        </div>
      )}

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

      {/* Diagnostics banner */}
      {(hasErrors || engineStale) && (
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
                  style={{ display: 'inline-block', transform: diagsExpanded ? 'rotate(90deg)' : 'rotate(0deg)' }}
                >
                  ▶
                </span>
                <span className="font-vsc-mono text-[10px] text-[#f48771]">
                  {errors.length > 0 ? `${errors.length} error${errors.length !== 1 ? 's' : ''}` : ''}
                  {errors.length > 0 && warnings.length > 0 ? ', ' : ''}
                  {warnings.length > 0 ? `${warnings.length} warning${warnings.length !== 1 ? 's' : ''}` : ''}
                  {' — using last successful build'}
                </span>
              </button>
              {diagsExpanded && (
                <div className="px-2.5 pb-1.5 flex flex-col gap-0.5 max-h-[200px] overflow-y-auto">
                  {errors.map((e, i) => (
                    <div key={`e${i}`} className="font-vsc-mono text-[10px] text-[#f48771]/80 pl-3.5 break-words whitespace-pre-wrap">
                      {e.message}
                    </div>
                  ))}
                  {warnings.map((w, i) => (
                    <div key={`w${i}`} className="font-vsc-mono text-[10px] text-[#cca700]/80 pl-3.5 break-words whitespace-pre-wrap">
                      {w.message}
                    </div>
                  ))}
                </div>
              )}
            </>
          ) : (
            <div className="px-2.5 py-1 font-vsc-mono text-[10px] text-[#f48771]">
              Build is stale — using last successful build
            </div>
          )}
        </div>
      )}

      {/* Sidebar toggle */}
      <div className="flex items-center gap-1.5 px-2.5 py-1 border-b border-vsc-border shrink-0 bg-vsc-surface">
        <TooltipProvider>
          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                variant="ghost"
                size="icon"
                className="h-6 w-6"
                onClick={() => setSidebarOpen((prev) => !prev)}
              >
                <PanelLeft className="h-3.5 w-3.5 text-vsc-text-muted" />
              </Button>
            </TooltipTrigger>
            <TooltipContent>{sidebarOpen ? 'Hide sidebar' : 'Show sidebar'}</TooltipContent>
          </Tooltip>
        </TooltipProvider>
        {selectedFn && (
          <span className="text-[11px] font-vsc-mono text-vsc-accent font-semibold">{selectedFn}()</span>
        )}
        {selectedFn && (
          <div className="flex items-center gap-1 ml-auto">
            <Button
              variant="success"
              size="sm"
              className="text-[11px] font-semibold"
              disabled={hasErrors || isRunning || !selectedProject}
              onClick={onRunFunction}
            >
              {isRunning ? 'Running...' : 'Run'}
            </Button>
          </div>
        )}
      </div>

      {/* Main layout: sidebar + content */}
      <div className="flex flex-1 min-h-0">
        {/* Sidebar */}
        {sidebarOpen && (
          <>
            <div className="shrink-0 overflow-hidden" style={{ width: sidebarWidth }}>
              <FunctionSidebar
                functions={sidebarFunctions}
                testTree={testTree}
                selectedFn={selectedFn}
                onSelectFn={(fn) => { setWorkflowContext(null); setSelectedFn(fn); setViewingCollection(false); setViewingTestRun(false); }}
                onRefreshTests={handleRefreshTests}
                onRunTest={handleRunTest}
                testRunResults={testRunResults}
                failedExpands={failedExpands}
                onRetryExpand={handleRetryExpand}
                collectionRun={collectionRun}
                viewingCollection={viewingCollection}
                onSelectCollectionView={() => { setViewingCollection(true); setViewingTestRun(false); setSelectedFn(null); }}
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
          {viewingCollection && collectionRun ? (
            <CollectionRunView
              run={collectionRun}
              expandedLogId={expandedLogId}
              setExpandedLogId={setExpandedLogId}
              resultRenderers={resultRenderers}
            />
          ) : viewingTestRun ? (
            <div ref={outputRef} className="flex-1 overflow-auto font-vsc-mono text-xs bg-vsc-bg">
              {runs.length === 0 && (
                <div className="p-5 text-center text-vsc-text-faint text-[11px]">
                  No test runs yet
                </div>
              )}
              {[...runs].reverse().map((run, runIdx) => {
                const isLatest = runIdx === 0;
                const statusCls = run.status === 'error' ? 'bg-vsc-red' : run.status === 'success' ? 'bg-vsc-green' : run.status === 'cancelled' ? 'bg-vsc-yellow' : 'bg-vsc-text-muted';
                return (
                  <div key={run.id} className={!isLatest ? 'border-b-2 border-vsc-border' : ''}>
                    <div className="flex items-center gap-1.5 px-2.5 py-1.5 bg-vsc-surface border-b border-vsc-border-subtle">
                      <span className={`w-1.5 h-1.5 rounded-full shrink-0 ${statusCls}`} />
                      <span className="text-vsc-accent font-semibold text-[11px]">
                        {run.testName ?? run.functionName}
                      </span>
                      {run.status === 'running' && (
                        <>
                          <span className="text-vsc-text-muted text-[10px]">running...</span>
                          <TooltipProvider>
                            <Tooltip>
                              <TooltipTrigger asChild>
                                <Button
                                  variant="ghost"
                                  size="icon"
                                  className="h-5 w-5 text-vsc-text-muted hover:text-vsc-error"
                                  onClick={() => onCancelRun(run.id)}
                                >
                                  <Square size={12} />
                                </Button>
                              </TooltipTrigger>
                              <TooltipContent>Cancel execution</TooltipContent>
                            </Tooltip>
                          </TooltipProvider>
                        </>
                      )}
                      {run.durationMs != null && (
                        <span className="text-vsc-text-faint text-[10px] shrink-0">{run.durationMs}ms</span>
                      )}
                    </div>
                    {run.fetchLogs.map((log) => {
                      const isExp = expandedLogId === log.id;
                      const statusColorCls = log.status === null ? 'text-vsc-text-muted'
                        : log.status >= 200 && log.status < 300 ? 'text-vsc-green'
                        : log.status === 0 ? 'text-vsc-red' : 'text-vsc-yellow';
                      return (
                        <div key={`t-${log.id}`}>
                          <div
                            onClick={() => setExpandedLogId(isExp ? null : log.id)}
                            className="flex items-center gap-1.5 py-0.5 pr-2.5 pl-[22px] cursor-pointer border-b border-vsc-border-subtle"
                          >
                            <span className={`${statusColorCls} font-semibold text-[11px]`}>{log.status ?? '...'}</span>
                            <span className="text-vsc-text-faint text-[10px]">{log.method}</span>
                            <span className="text-vsc-text flex-1 overflow-hidden text-ellipsis whitespace-nowrap text-[11px]">{log.url}</span>
                            {log.durationMs != null && <span className="text-vsc-text-faint text-[10px]">{log.durationMs}ms</span>}
                            <span className="text-vsc-text-faint text-[9px]">{isExp ? '\u25B4' : '\u25BE'}</span>
                          </div>
                          {isExp && (
                            <div className="py-2 pr-2.5 pl-[22px] flex flex-col gap-2 border-b border-vsc-border">
                              {log.error && <CodeBlock variant="error">{log.error}</CodeBlock>}
                              <div>
                                <div className="text-[10px] font-semibold text-vsc-text-muted mb-0.5 uppercase tracking-wide">Request Headers</div>
                                <CodeBlock>{JSON.stringify(log.requestHeaders, null, 2)}</CodeBlock>
                              </div>
                              {log.requestBody && (
                                <div>
                                  <div className="text-[10px] font-semibold text-vsc-text-muted mb-0.5 uppercase tracking-wide">Request Body</div>
                                  <CodeBlock>{tryFormatJson(log.requestBody)}</CodeBlock>
                                </div>
                              )}
                              {log.responseBody != null && (
                                <div>
                                  <div className="text-[10px] font-semibold text-vsc-text-muted mb-0.5 uppercase tracking-wide">Response Body</div>
                                  <CodeBlock>{tryFormatJson(log.responseBody)}</CodeBlock>
                                </div>
                              )}
                            </div>
                          )}
                        </div>
                      );
                    })}
                    {/* Runtime events (log.info, baml.events.send, etc.) */}
                    {run.runtimeEvents.length > 0 && (
                      <div className="py-1.5 pr-2.5 pl-[22px] border-b border-vsc-border-subtle">
                        <div className="text-[10px] font-semibold text-vsc-text-muted mb-1 uppercase tracking-wide">
                          Events ({run.runtimeEvents.length})
                        </div>
                        <div className="flex flex-col gap-0.5">
                          {run.runtimeEvents.map((evt, evtIdx) => {
                            const kind = evt.event;
                            if (!kind) return null;

                            let label: string;
                            let payload: ReactNode;
                            let colorCls: string;

                            switch (kind.$case) {
                              case 'functionStart':
                                label = 'START';
                                payload = kind.functionStart.name;
                                colorCls = 'text-vsc-green';
                                break;
                              case 'functionEnd':
                                label = 'END';
                                payload = `${kind.functionEnd.name} (${kind.functionEnd.durationMs}ms)`;
                                colorCls = 'text-vsc-text-muted';
                                break;
                              case 'log': {
                                const lvl = kind.log.level;
                                label = lvl;
                                payload = <EventValueDisplay value={kind.log.data} customRenderers={resultRenderers} />;
                                colorCls = lvl === 'error' ? 'text-vsc-red'
                                  : lvl === 'warn' ? 'text-vsc-yellow'
                                  : lvl === 'debug' ? 'text-vsc-text-muted'
                                  : 'text-vsc-blue';
                                break;
                              }
                              case 'custom':
                                label = 'EVENT';
                                payload = <><span>{kind.custom.name}: </span><EventValueDisplay value={kind.custom.data} customRenderers={resultRenderers} /></>;
                                colorCls = 'text-vsc-purple';
                                break;
                              case 'setTags':
                                label = 'TAGS';
                                payload = kind.setTags.tags.map(t => `${t.key}=${t.value}`).join(', ');
                                colorCls = 'text-vsc-text-muted';
                                break;
                              default:
                                return null;
                            }

                            // Check if cursor is within this event's source span
                            const source = kind.$case === 'log' ? kind.log.source : undefined;
                            const isCursorMatch = cursorOffset != null && source != null &&
                              cursorOffset > source.startOffset && cursorOffset <= source.endOffset;

                            return (
                              <div
                                key={`${evt.spanId}-${evtIdx}`}
                                className={cn(
                                  "flex items-start gap-1.5 text-[11px]",
                                  isCursorMatch && "bg-vsc-yellow/20 rounded px-1 -mx-1",
                                  source && onNavigateToSource && "cursor-pointer hover:bg-vsc-bg-secondary"
                                )}
                                onClick={source && onNavigateToSource ? () => onNavigateToSource({ fileId: source.fileId, line: source.line, column: source.column }) : undefined}
                              >
                                <span className={`${colorCls} font-semibold shrink-0 w-10 uppercase`}>{label}</span>
                                <span className="text-vsc-text flex-1 font-mono truncate">{payload}</span>
                              </div>
                            );
                          })}
                        </div>
                      </div>
                    )}
                    {/* Inline io.input() prompts for this run */}
                    {(pendingInputs.get(run.id) ?? []).map((req) => (
                      <div key={req.id} className="flex items-center gap-2 px-[22px] py-1.5 border-b border-vsc-border bg-vsc-surface">
                        <span className="text-vsc-text-faint text-xs shrink-0">{req.prompt ?? 'Input:'}</span>
                        <input
                          className="flex-1 bg-vsc-bg border border-vsc-border rounded px-2 py-1 text-xs text-vsc-text font-vsc-mono focus:outline-none focus:border-vsc-accent"
                          autoFocus
                          onKeyDown={(e) => {
                            if (e.key === 'Enter') {
                              submitInput(req.id, e.currentTarget.value, run.id);
                            }
                          }}
                        />
                      </div>
                    ))}
                    {run.status === 'cancelled' && (
                      <div className="py-1.5 pr-2.5 pl-[22px]">
                        <div className="text-[11px] text-vsc-text-faint italic">Cancelled</div>
                      </div>
                    )}
                    {run.error && (
                      <div className="py-1.5 pr-2.5 pl-[22px]">
                        <div className="text-[10px] font-semibold text-vsc-red mb-0.5 uppercase tracking-wide">Error</div>
                        <ErrorDisplay error={run.error} />
                      </div>
                    )}
                    {run.result != null && (
                      <div className="py-1.5 pr-2.5 pl-[22px]">
                        <div className="text-[10px] font-semibold text-vsc-green mb-0.5 uppercase tracking-wide">Result</div>
                        <ResultDisplay result={run.result} customRenderers={resultRenderers} />
                      </div>
                    )}
                  </div>
                );
              })}
            </div>
          ) : selectedFn ? (
            <Tabs
              value={activeTab}
              onValueChange={(v) => setActiveTab(v as typeof activeTab)}
              className="flex-1 flex flex-col min-h-0"
            >
              <div className="flex items-center px-2.5 shrink-0 bg-vsc-surface">
                <TabsList className="bg-transparent">
                  <TabsTrigger value="run">Run</TabsTrigger>
                  <TabsTrigger value="graph">Graph</TabsTrigger>
                  {canPreviewPrompt && (
                    <TabsTrigger value="prompt">
                      Prompt
                      {selectedFnInfo?.capabilities?.clientName && (
                        <span className="ml-1 px-1 py-0 text-[9px] rounded bg-vsc-bg-secondary text-vsc-text-faint">
                          {selectedFnInfo.capabilities.clientName}
                        </span>
                      )}
                    </TabsTrigger>
                  )}
                  {canPreviewCurl && <TabsTrigger value="curl">cURL</TabsTrigger>}
                </TabsList>
                {runs.length > 0 && !isRunning && (
                  <Button
                    variant="ghost"
                    size="sm"
                    className="ml-auto text-[10px] text-vsc-text-muted hover:text-vsc-text"
                    onClick={() => {
                      const runIds = runs.map((r) => r.id);
                      port.postMessage({ type: 'clearHandles', runIds });
                      setRuns([]);
                    }}
                  >
                    Clear
                  </Button>
                )}
              </div>

              {/* Graph view */}
              <TabsContent value="graph" className="flex-1 min-h-0 mt-0 flex flex-col" style={{ minHeight: 300 }}>
                {/* "Called from" bar — shown when the current function is called from workflows */}
                {workflowContext && (
                  <div className="flex items-center gap-1.5 px-2.5 py-1 text-[10px] bg-vsc-bg-secondary border-b border-vsc-border shrink-0">
                    <span className="text-vsc-text-faint">Called from:</span>
                    {workflowContext.workflows.map((wf) => (
                      <Button
                        key={wf}
                        variant="outline"
                        size="sm"
                        className="h-auto px-1.5 py-0.5 text-[10px]"
                        onClick={() => {
                          setWorkflowContext(null);
                          setSelectedFn(wf);
                          setHighlightedNodeId(null);
                        }}
                      >
                        {wf}
                      </Button>
                    ))}
                  </div>
                )}
                {controlFlowGraph ? (
                  <GraphView
                    graph={controlFlowGraph}
                    selectedNodeId={highlightedNodeId}
                    onNodeClick={(nodeId) => setHighlightedNodeId(nodeId)}
                  />
                ) : (
                  <div className="flex-1 flex items-center justify-center text-vsc-text-faint text-xs bg-vsc-bg h-full">
                    Loading graph...
                  </div>
                )}
              </TabsContent>

              {/* Prompt preview */}
              {canPreviewPrompt && (
                <TabsContent value="prompt" className="flex-1 flex flex-col overflow-hidden mt-0">
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
                        <ResultDisplay result={promptPreviewResult} customRenderers={resultRenderers} />
                      ) : (
                        <div className="flex items-center justify-center text-vsc-text-faint text-xs h-full">
                          {previewLoading ? 'Loading prompt preview...' : 'Enter args to preview prompt'}
                        </div>
                      )}
                    </div>
                  </div>
                  {promptPreviewResult && <PromptStats text={stringifyResult(promptPreviewResult)} />}
                </TabsContent>
              )}

              {/* cURL preview */}
              {canPreviewCurl && (
                <TabsContent value="curl" className="flex-1 overflow-auto font-vsc-mono text-xs bg-vsc-bg p-2.5 mt-0">
                  {curlPreviewResult != null ? (
                    <ResultDisplay result={curlPreviewResult} customRenderers={resultRenderers} />
                  ) : curlPreviewError ? (
                    <div className="flex items-center justify-center text-vsc-error text-xs h-full">
                      {curlPreviewError}
                    </div>
                  ) : (
                    <div className="flex items-center justify-center text-vsc-text-faint text-xs h-full">
                      {previewLoading ? 'Loading cURL preview...' : 'Enter args to preview cURL'}
                    </div>
                  )}
                </TabsContent>
              )}

              {/* Execution area */}
              <TabsContent value="run" className="flex-1 flex flex-col min-h-0 mt-0">
                {/* Args */}
                <div className="flex items-center border-b border-vsc-border shrink-0">
                  <span className="px-2 py-1 text-[10px] text-vsc-text-faint font-vsc-mono bg-vsc-surface border-r border-vsc-border self-stretch flex items-center">
                    args
                  </span>
                  <Input
                    spellCheck={false}
                    value={argsJson}
                    onChange={onArgsJsonChange}
                    className="flex-1 h-7 rounded-none border-none font-vsc-mono text-xs"
                    placeholder='{"key": "value"}'
                  />
                </div>

                {/* Run history (scrollable) */}
                <div ref={outputRef} className="flex-1 overflow-auto font-vsc-mono text-xs bg-vsc-bg">
                  {runs.length === 0 && (
                    <div className="p-5 text-center text-vsc-text-faint text-[11px]">
                      Press Run to execute {selectedFn}()
                    </div>
                  )}

                  {[...runs].reverse().map((run, runIdx) => {
                    const isLatest = runIdx === 0;
                    const statusCls = run.status === 'error' ? 'bg-vsc-red' : run.status === 'success' ? 'bg-vsc-green' : run.status === 'cancelled' ? 'bg-vsc-yellow' : 'bg-vsc-text-muted';

                    return (
                      <div key={run.id} className={!isLatest ? 'border-b-2 border-vsc-border' : ''}>
                        {/* Run header */}
                        <div className="flex items-center gap-1.5 px-2.5 py-1.5 bg-vsc-surface border-b border-vsc-border-subtle">
                          <span className={`w-1.5 h-1.5 rounded-full shrink-0 ${statusCls}`} />
                          <span className="text-vsc-accent font-semibold text-[11px]">
                            {run.functionName}()
                          </span>
                          <span className="text-vsc-text-faint text-[10px] flex-1 overflow-hidden text-ellipsis whitespace-nowrap">
                            {run.argsJson}
                          </span>
                          {run.status === 'running' && (
                            <>
                              <span className="text-vsc-text-muted text-[10px]">running...</span>
                              <TooltipProvider>
                                <Tooltip>
                                  <TooltipTrigger asChild>
                                    <Button
                                      variant="ghost"
                                      size="icon"
                                      className="h-5 w-5 text-vsc-text-muted hover:text-vsc-error"
                                      onClick={() => onCancelRun(run.id)}
                                    >
                                      <Square size={12} />
                                    </Button>
                                  </TooltipTrigger>
                                  <TooltipContent>Cancel execution</TooltipContent>
                                </Tooltip>
                              </TooltipProvider>
                            </>
                          )}
                          {run.durationMs != null && (
                            <span className="text-vsc-text-faint text-[10px] shrink-0">{run.durationMs}ms</span>
                          )}
                        </div>

                        {/* Fetch logs for this run */}
                        {run.fetchLogs.map((log) => {
                          const isExp = expandedLogId === log.id;
                          const statusColorCls = log.status === null ? 'text-vsc-text-muted'
                            : log.status >= 200 && log.status < 300 ? 'text-vsc-green'
                            : log.status === 0 ? 'text-vsc-red' : 'text-vsc-yellow';
                          return (
                            <div key={`n-${log.id}`}>
                              <div
                                onClick={() => setExpandedLogId(isExp ? null : log.id)}
                                className="flex items-center gap-1.5 py-0.5 pr-2.5 pl-[22px] cursor-pointer border-b border-vsc-border-subtle"
                              >
                                <span className={`${statusColorCls} font-semibold text-[11px]`}>{log.status ?? '...'}</span>
                                <span className="text-vsc-text-faint text-[10px]">{log.method}</span>
                                <span className="text-vsc-text flex-1 overflow-hidden text-ellipsis whitespace-nowrap text-[11px]">{log.url}</span>
                                {log.durationMs != null && <span className="text-vsc-text-faint text-[10px]">{log.durationMs}ms</span>}
                                <span className="text-vsc-text-faint text-[9px]">{isExp ? '\u25B4' : '\u25BE'}</span>
                              </div>
                              {isExp && (
                                <div className="py-2 pr-2.5 pl-[22px] flex flex-col gap-2 border-b border-vsc-border">
                                  {log.error && <CodeBlock variant="error">{log.error}</CodeBlock>}
                                  <div>
                                    <div className="text-[10px] font-semibold text-vsc-text-muted mb-0.5 uppercase tracking-wide">Request Headers</div>
                                    <CodeBlock>{JSON.stringify(log.requestHeaders, null, 2)}</CodeBlock>
                                  </div>
                                  {log.requestBody && (
                                    <div>
                                      <div className="text-[10px] font-semibold text-vsc-text-muted mb-0.5 uppercase tracking-wide">Request Body</div>
                                      <CodeBlock>{tryFormatJson(log.requestBody)}</CodeBlock>
                                    </div>
                                  )}
                                  {log.responseBody != null && (
                                    <div>
                                      <div className="text-[10px] font-semibold text-vsc-text-muted mb-0.5 uppercase tracking-wide">Response Body</div>
                                      <CodeBlock>{tryFormatJson(log.responseBody)}</CodeBlock>
                                    </div>
                                  )}
                                </div>
                              )}
                            </div>
                          );
                        })}

                        {/* Runtime events (log.info, baml.events.send, etc.) */}
                        {run.runtimeEvents.length > 0 && (
                          <div className="py-1.5 pr-2.5 pl-[22px] border-b border-vsc-border-subtle">
                            <div className="text-[10px] font-semibold text-vsc-text-muted mb-1 uppercase tracking-wide">
                              Events ({run.runtimeEvents.length})
                            </div>
                            <div className="flex flex-col gap-0.5">
                              {run.runtimeEvents.map((evt, evtIdx) => {
                                const kind = evt.event;
                                if (!kind) return null;

                                let label: string;
                                let payload: ReactNode;
                                let colorCls: string;

                                switch (kind.$case) {
                                  case 'functionStart':
                                    label = 'START';
                                    payload = kind.functionStart.name;
                                    colorCls = 'text-vsc-green';
                                    break;
                                  case 'functionEnd':
                                    label = 'END';
                                    payload = `${kind.functionEnd.name} (${kind.functionEnd.durationMs}ms)`;
                                    colorCls = 'text-vsc-text-muted';
                                    break;
                                  case 'log': {
                                    const lvl = kind.log.level;
                                    label = lvl;
                                    payload = <EventValueDisplay value={kind.log.data} customRenderers={resultRenderers} />;
                                    colorCls = lvl === 'error' ? 'text-vsc-red'
                                      : lvl === 'warn' ? 'text-vsc-yellow'
                                      : lvl === 'debug' ? 'text-vsc-text-muted'
                                      : 'text-vsc-blue';
                                    break;
                                  }
                                  case 'custom':
                                    label = 'EVENT';
                                    payload = <><span>{kind.custom.name}: </span><EventValueDisplay value={kind.custom.data} customRenderers={resultRenderers} /></>;
                                    colorCls = 'text-vsc-purple';
                                    break;
                                  case 'setTags':
                                    label = 'TAGS';
                                    payload = kind.setTags.tags.map(t => `${t.key}=${t.value}`).join(', ');
                                    colorCls = 'text-vsc-text-muted';
                                    break;
                                  default:
                                    return null;
                                }

                                // Check if cursor is within this event's source span
                                const source = kind.$case === 'log' ? kind.log.source : undefined;
                                const isCursorMatch = cursorOffset != null && source != null &&
                                  cursorOffset > source.startOffset && cursorOffset <= source.endOffset;

                                return (
                                  <div
                                    key={`${evt.spanId}-${evtIdx}`}
                                    className={cn(
                                      "flex items-start gap-1.5 text-[11px]",
                                      isCursorMatch && "bg-vsc-yellow/20 rounded px-1 -mx-1",
                                      source && onNavigateToSource && "cursor-pointer hover:bg-vsc-bg-secondary"
                                    )}
                                    onClick={source && onNavigateToSource ? () => onNavigateToSource({ fileId: source.fileId, line: source.line, column: source.column }) : undefined}
                                  >
                                    <span className={`${colorCls} font-semibold shrink-0 w-10 uppercase`}>{label}</span>
                                    <span className="text-vsc-text flex-1 font-mono truncate">{payload}</span>
                                  </div>
                                );
                              })}
                            </div>
                          </div>
                        )}
                        {/* Inline io.input() prompts for this run */}
                        {(pendingInputs.get(run.id) ?? []).map((req) => (
                          <div key={req.id} className="flex items-center gap-2 px-[22px] py-1.5 border-b border-vsc-border bg-vsc-surface">
                            <span className="text-vsc-text-faint text-xs shrink-0">{req.prompt ?? 'Input:'}</span>
                            <input
                              className="flex-1 bg-vsc-bg border border-vsc-border rounded px-2 py-1 text-xs text-vsc-text font-vsc-mono focus:outline-none focus:border-vsc-accent"
                              autoFocus
                              onKeyDown={(e) => {
                                if (e.key === 'Enter') {
                                  submitInput(req.id, e.currentTarget.value, run.id);
                                }
                              }}
                            />
                          </div>
                        ))}

                        {/* Result / Error / Cancelled for this run */}
                        {run.status === 'cancelled' && (
                          <div className="py-1.5 pr-2.5 pl-[22px]">
                            <div className="text-[11px] text-vsc-text-faint italic">Cancelled</div>
                          </div>
                        )}
                        {run.error && (
                          <div className="py-1.5 pr-2.5 pl-[22px]">
                            <div className="text-[10px] font-semibold text-vsc-red mb-0.5 uppercase tracking-wide">Error</div>
                            <ErrorDisplay error={run.error} onRetry={onRunFunction} />
                          </div>
                        )}
                        {run.result != null && (
                          <div className="py-1.5 pr-2.5 pl-[22px]">
                            {run.status === 'success' && run.fetchLogs.length > 0 && (
                              <div className="mb-1">
                                <MetadataBadges fetchLogs={run.fetchLogs} durationMs={run.durationMs} />
                              </div>
                            )}
                            <div className="space-y-1">
                              <div className="flex items-center gap-1">
                                <div className="text-[10px] font-semibold text-vsc-green uppercase tracking-wide">Result</div>
                                <ToggleGroup
                                  value={resultModes[run.id] ?? 'parsed'}
                                  onValueChange={(v) => setResultModes((prev) => ({ ...prev, [run.id]: v as 'parsed' | 'raw' }))}
                                  options={[
                                    { value: 'parsed', label: 'Parsed' },
                                    { value: 'raw', label: 'Raw' },
                                  ]}
                                  size="sm"
                                />
                                <CopyButton text={stringifyResult(run.result)} iconSize={11} />
                              </div>
                              {(resultModes[run.id] ?? 'parsed') === 'parsed' ? (
                                <ResultDisplay result={run.result} customRenderers={resultRenderers} />
                              ) : (
                                <pre className="whitespace-pre-wrap break-all font-vsc-mono text-[11px] text-vsc-text bg-vsc-bg-secondary p-2 rounded border border-vsc-border max-h-[400px] overflow-auto">
                                  {stringifyResult(run.result)}
                                </pre>
                              )}
                            </div>
                          </div>
                        )}
                      </div>
                    );
                  })}
                </div>
              </TabsContent>
            </Tabs>
          ) : (
            <div className="flex-1 flex items-center justify-center text-vsc-text-faint text-xs bg-vsc-bg">
              {viewingCollection ? 'Collection not yet available — click Refresh' : 'Select a function to run'}
            </div>
          )}
        </div>
      </div>

      <ApiKeysDialog
        open={showApiKeysDialog}
        envVars={envVars}
        requiredKeys={knownRequiredKeys}
        onOpenChange={(open) => {
          setShowApiKeysDialog(open);
          showApiKeysDialogRef.current = open;
          if (!open) {
            // Dialog closed — resolve ALL pending env requests in one batch.
            // If user provided a value, envVarsRef has it. If not, value is undefined → worker errors the call.
            for (const [id, variable] of pendingEnvRequestsRef.current) {
              const value = envVarsRef.current[variable];
              port.postMessage({ type: 'envVarResponse', id, value, variable });
            }
            pendingEnvRequestsRef.current.clear();
          }
        }}
        onSetEnvVar={addEnvVar}
        onDeleteEnvVar={removeEnvVar}
        onImportEnvVars={importEnvVars}
      />
    </>
  );
};
