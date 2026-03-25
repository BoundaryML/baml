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

import type { ChangeEvent, FC } from 'react';
import { useCallback, useEffect, useRef, useState } from 'react';
import { encodeCallArgs } from '@b/pkg-proto';
import { KeyRound, PanelLeft, Square } from 'lucide-react';
import { ApiKeysDialog } from './components/ApiKeysDialog';
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
  TestInfo,
  WorkerOutMessage,
} from './worker-protocol';
import type { ResultRendererProps } from './result-renderers';
import { ResultDisplay } from './ResultDisplay';
import { registerBuiltinResultRenderers } from './renderers/registerBuiltins';
import { GraphView } from './graph/GraphView';
import { FunctionSidebar } from './FunctionSidebar';

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

/** Shared classes for <pre> code blocks */
const codeBlockCls = 'whitespace-pre-wrap break-all font-vsc-mono text-xs leading-relaxed p-2 rounded bg-vsc-bg border border-vsc-border text-vsc-text overflow-auto max-h-[200px] m-0';

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
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export const ExecutionPanel: FC<ExecutionPanelProps> = ({ port, connectionVersion, resultRenderers }) => {
  const [projectRoots, setProjectRoots] = useState<string[]>([]);
  const [projectUpdates, setProjectUpdates] = useState<Record<string, ProjectUpdate>>({});
  const [selectedProject, setSelectedProject] = useState<string | null>(null);
  const [diags, setDiags] = useState<DiagnosticEntry[]>([]);

  const [selectedFn, setSelectedFn] = useState<string | null>(null);
  const [argsJson, setArgsJson] = useState('{}');

  // Run history — each entry is a complete invocation with its logs + result
  const [runs, setRuns] = useState<RunEntry[]>([]);
  const [expandedLogId, setExpandedLogId] = useState<number | null>(null);
  const outputRef = useRef<HTMLDivElement>(null);

  const [controlFlowGraph, setControlFlowGraph] = useState<ControlFlowGraph | null>(null);
  const [activeTab, setActiveTab] = useState<'run' | 'graph' | 'prompt' | 'curl'>('run');
  const [highlightedNodeId, setHighlightedNodeId] = useState<number | null>(null);
  const [promptPreviewResult, setPromptPreviewResult] = useState<string | null>(null);
  const [curlPreviewResult, setCurlPreviewResult] = useState<string | null>(null);
  const [promptPreviewError, setPromptPreviewError] = useState<string | null>(null);
  const [curlPreviewError, setCurlPreviewError] = useState<string | null>(null);
  const [previewLoading, setPreviewLoading] = useState(false);
  const [sidebarOpen, setSidebarOpen] = useState(true);
  const [sidebarWidth, setSidebarWidth] = useState(220);
  const resizingRef = useRef(false);
  const [resultModes, setResultModes] = useState<Record<number, 'parsed' | 'raw'>>({});

  const [showApiKeysDialog, setShowApiKeysDialog] = useState(false);
  const showApiKeysDialogRef = useRef(false);

  const [buildTime, setBuildTime] = useState<number | null>(null);
  const [envVars, setEnvVarsState] = useState<Record<string, string>>({});
  // Keys the project is known to need — accumulated from envVarRequests, never shrunk.
  const [knownRequiredKeys, setKnownRequiredKeys] = useState<Set<string>>(new Set());
  // In-flight worker requests waiting for a value: id → variable name. Ref because it doesn't drive renders.
  const pendingEnvRequestsRef = useRef<Map<number, string>>(new Map());

  // Ref mirror of envVars so the message handler closure always sees current values.
  const envVarsRef = useRef(envVars);
  useEffect(() => { envVarsRef.current = envVars; }, [envVars]);

  // Ref mirrors for cursor context handler (avoids stale closures in port.onMessage).
  const selectedFnRef = useRef(selectedFn);
  useEffect(() => { selectedFnRef.current = selectedFn; }, [selectedFn]);
  const controlFlowGraphRef = useRef(controlFlowGraph);
  useEffect(() => { controlFlowGraphRef.current = controlFlowGraph; }, [controlFlowGraph]);

  const nextCallIdRef = useRef(0);
  const pendingCallsRef = useRef<Map<number, { resolve: (v: string) => void; reject: (e: Error) => void }>>(new Map());

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
    if (!ctx.functionName) return;

    const currentFn = selectedFnRef.current;
    const cachedGraph = controlFlowGraphRef.current;

    // Use the ordered candidate list (most-specific to least-specific) to
    // find the closest matching graph node. Falls back to label matching.
    const candidates = ctx.sourceExprCandidates ?? [];
    const nodeId = resolveCandidatesToNodeId(cachedGraph, candidates)
      ?? (ctx.sourceExprId != null ? resolveNodeByFunctionName(cachedGraph, ctx.functionName) : null);

    console.log('[handleCursorContext]', { currentFn, functionName: ctx.functionName, nodeId });

    // Rule 0: cursor is on a function definition (not a call site) — switch to that function.
    // sourceExprId is null when the cursor is on the definition line, not inside a call.
    if (ctx.sourceExprId == null && ctx.functionName !== currentFn) {
      console.log('[handleCursorContext] Rule 0: switch to definition', ctx.functionName);
      setSelectedFn(ctx.functionName);
      setHighlightedNodeId(null);
      return;
    }

    // Rule 1: cursor is on a node in the currently-displayed workflow
    if (nodeId != null && ctx.functionName === currentFn) {
      console.log('[handleCursorContext] Rule 1: highlight node', nodeId);
      setHighlightedNodeId(nodeId);
      return;
    }

    // Rule 2: cursor is on a call site inside the current workflow — highlight the call node.
    if (nodeId != null && ctx.workflowMemberships.includes(currentFn ?? '')) {
      console.log('[handleCursorContext] Rule 2: highlight node', nodeId);
      setHighlightedNodeId(nodeId);
      return;
    }

    // Rule 3: switch to the function or its first workflow parent
    if (ctx.workflowMemberships.length > 0) {
      const target = ctx.workflowMemberships[0];
      console.log('[handleCursorContext] Rule 3: switch to workflow', target);
      setSelectedFn(target);
      setHighlightedNodeId(null);
    } else {
      console.log('[handleCursorContext] Rule 3: switch to', ctx.functionName);
      setSelectedFn(ctx.functionName);
      setHighlightedNodeId(null);
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

        case 'diagnostics':
          setDiags(data.entries ?? []);
          break;

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

        case 'fetchLogNew':
          setRuns((prev) => {
            const targetIdx = prev.findIndex((r) => r.id === data.entry.callId);
            if (targetIdx === -1) return prev;
            const target = prev[targetIdx];
            return [...prev.slice(0, targetIdx), { ...target, fetchLogs: [...target.fetchLogs, data.entry] }, ...prev.slice(targetIdx + 1)];
          });
          break;

        case 'fetchLogUpdate':
          setRuns((prev) =>
            prev.map((r) => ({
              ...r,
              fetchLogs: r.fetchLogs.map((e) => (e.id === data.logId ? { ...e, ...data.patch } : e)),
            })),
          );
          break;

        case 'envVarRequest': {
          // Always track as a known required key (proactive indicator)
          setKnownRequiredKeys((prev) => prev.has(data.variable) ? prev : new Set([...prev, data.variable]));
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

        case "ready":
          break;

        case 'buildTime':
          setBuildTime(Number(data.value) || null);
          break;

        case "vfsFileChanged":
        case "vfsFileDeleted":
          break;

        case "controlFlowGraphResult":
          if (data.graph) setControlFlowGraph(data.graph);
          break;

        case "cursorContext":
          handleCursorContext(data.context);
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

  // Request control flow graph when selected function changes
  useEffect(() => {
    setControlFlowGraph(null);
    setHighlightedNodeId(null);
    if (!selectedFn || !selectedProject) return;
    port.postMessage({ type: 'requestControlFlowGraph', project: selectedProject, functionName: selectedFn });
  }, [port, selectedFn, selectedProject]);

  // Clear preview results when selected function changes
  useEffect(() => {
    setPromptPreviewResult(null);
    setCurlPreviewResult(null);
    setPromptPreviewError(null);
    setCurlPreviewError(null);
    setPreviewLoading(false);
  }, [selectedFn]);

  // Re-trigger preview when the project update changes (e.g. Bex becomes available)
  const projectUpdateVersion = selectedProject ? projectUpdates[selectedProject] : undefined;

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
        const resultStr = await new Promise<string>((resolve, reject) => {
          pendingCallsRef.current.set(callId, { resolve, reject });
          port.postMessage({
            type: 'callFunction',
            id: callId,
            name: `${selectedFn}.${subFn}`,
            argsProto: new Uint8Array(argsProto),
            project: selectedProject,
          });
        });
        setResult(resultStr);
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

  // ── Env var helpers ────────────────────────────────────────────────────

  const addEnvVar = useCallback((key: string, value: string) => {
    setEnvVarsState((prev) => ({ ...prev, [key]: value }));
    envVarsRef.current[key] = value;
    port.postMessage({ type: 'setEnvVar', key, value });
  }, [port]);

  const removeEnvVar = useCallback((key: string) => {
    setEnvVarsState((prev) => { const { [key]: _, ...rest } = prev; return rest; });
    delete envVarsRef.current[key];
    port.postMessage({ type: 'deleteEnvVar', key });
  }, [port]);

  const handleImportEnvVars = useCallback((vars: Record<string, string>) => {
    for (const [key, value] of Object.entries(vars)) {
      addEnvVar(key, value);
    }
  }, [addEnvVar]);

  const onArgsJsonChange = useCallback((e: ChangeEvent<HTMLInputElement>) => {
    setArgsJson(e.target.value);
  }, []);

  // ── Run function ───────────────────────────────────────────────────────

  const isRunning = runs.length > 0 && runs[runs.length - 1].status === 'running';

  const onCancelRun = useCallback((runId: number) => {
    if (!selectedProject) return;
    port.postMessage({ type: 'cancelCall', id: runId, project: selectedProject });
  }, [port, selectedProject]);

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

      const resultStr = await new Promise<string>((resolve, reject) => {
        pendingCallsRef.current.set(runId, { resolve, reject });
        port.postMessage(
          { type: 'callFunction', id: runId, name: selectedFn, argsProto: new Uint8Array(argsProto), project: selectedProject },
        );
      });

      const dur = Math.round(performance.now() - startTime);
      setRuns((prev) => prev.map((r) => r.id === runId ? { ...r, result: resultStr, status: 'success', durationMs: dur } : r));
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

  const handleSelectTest = useCallback((test: TestInfo) => {
    setSelectedFn(test.functionName);
    setArgsJson(test.argsJson);
    setActiveTab('run');
  }, []);

  // Core test execution — no isRunning guard, usable from batch loops
  const executeTest = useCallback(async (test: TestInfo) => {
    if (!selectedProject) return;

    const runId = nextCallIdRef.current++;
    const startTime = performance.now();
    const newRun: RunEntry = {
      id: runId,
      functionName: test.functionName,
      argsJson: test.argsJson,
      testName: test.name,
      fetchLogs: [],
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
      const parsed = JSON.parse(test.argsJson);
      if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) {
        throw new Error('Test args must be a JSON object');
      }
      const argsProto = encodeCallArgs(parsed as Record<string, unknown>);
      const resultStr = await new Promise<string>((resolve, reject) => {
        pendingCallsRef.current.set(runId, { resolve, reject });
        port.postMessage({
          type: 'callFunction',
          id: runId,
          name: test.functionName,
          argsProto: new Uint8Array(argsProto),
          project: selectedProject,
        });
      });
      const dur = Math.round(performance.now() - startTime);
      setRuns((prev) => prev.map((r) => r.id === runId ? { ...r, result: resultStr, status: 'success', durationMs: dur } : r));
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
  }, [selectedProject, port]);

  const handleRunTest = useCallback(async (test: TestInfo) => {
    if (!selectedProject || isRunning) return;
    setSelectedFn(test.functionName);
    setArgsJson(test.argsJson);
    setActiveTab('run');
    await executeTest(test);
  }, [selectedProject, isRunning, executeTest]);

  // ── Derived state ──────────────────────────────────────────────────────

  const currentUpdate = selectedProject ? projectUpdates[selectedProject] : undefined;
  const functions: FunctionInfo[] = currentUpdate?.functions ?? [];
  const functionNames = functions.map((f) => f.name);
  const tests: TestInfo[] = currentUpdate?.tests ?? [];
  const engineStale = currentUpdate ? !currentUpdate.isBexCurrent : false;

  const selectedFnInfo = functions.find((f) => f.name === selectedFn);
  const canPreviewPrompt = selectedFnInfo?.capabilities?.renderPrompt ?? false;
  const canPreviewCurl = selectedFnInfo?.capabilities?.buildRequest ?? false;

  useEffect(() => {
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

  // Derive per-test status from run history (most recent run wins)
  const testStatuses = new Map<string, RunEntry['status']>();
  for (const run of runs) {
    if (run.testName) {
      testStatuses.set(run.testName, run.status);
    }
  }

  // ── Batch test execution ───────────────────────────────────────────────

  const [parallelTests, setParallelTests] = useState(false);
  const batchAbortRef = useRef(false);

  const handleRunAllTests = useCallback(async (testsToRun: TestInfo[]) => {
    if (!selectedProject) return;
    batchAbortRef.current = false;
    setActiveTab('run');

    if (parallelTests) {
      // Fire all at once — don't await, each resolves independently
      for (const test of testsToRun) {
        executeTest(test);
      }
    } else {
      // Sequential: run one at a time, check abort between each
      for (const test of testsToRun) {
        if (batchAbortRef.current) break;
        await executeTest(test);
      }
    }
  }, [selectedProject, parallelTests, executeTest]);

  const handleStopAllTests = useCallback(() => {
    if (!selectedProject) return;
    // Signal sequential loop to stop queuing new tests
    batchAbortRef.current = true;
    // Cancel currently running tests
    for (const run of runs) {
      if (run.status === 'running' && run.testName) {
        port.postMessage({ type: 'cancelCall', id: run.id, project: selectedProject });
      }
    }
  }, [runs, selectedProject, port]);

  const handleRerunFailed = useCallback(() => {
    const failedTests = tests.filter((t) =>
      runs.some((r) => r.testName === t.name && r.status === 'error')
    );
    if (failedTests.length > 0) handleRunAllTests(failedTests);
  }, [tests, runs, handleRunAllTests]);

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
        <button
          onClick={() => setShowApiKeysDialog(true)}
          className="relative p-1 rounded hover:bg-vsc-hover text-vsc-text-muted"
          title="API Keys"
        >
          <KeyRound size={14} />
          {hasMissingKeys && (
            <span className="absolute -top-0.5 -right-0.5 w-2 h-2 rounded-full bg-yellow-400" />
          )}
        </button>
      </div>

      {/* Project selector (shown when multiple projects exist) */}
      {projectRoots.length > 1 && (
        <div className="flex items-center gap-1.5 px-2.5 py-1 border-b border-vsc-border shrink-0 bg-vsc-surface">
          <span className="text-[10px] text-vsc-text-faint font-vsc-mono select-none">PROJECT</span>
          {projectRoots.map((root) => {
            const isSelected = root === selectedProject;
            const update = projectUpdates[root];
            return (
              <button
                key={root}
                onClick={() => setSelectedProject(root)}
                title={root}
                className={`px-2 py-0.5 rounded font-vsc-mono text-[10px] cursor-pointer border ${
                  isSelected
                    ? 'bg-vsc-accent text-vsc-accent-fg border-vsc-accent font-semibold'
                    : 'bg-transparent text-vsc-text-muted border-vsc-border'
                }`}
              >
                {root}
                {update && !update.isBexCurrent && <span className="ml-0.5 text-vsc-yellow">*</span>}
              </button>
            );
          })}
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

      {/* Diagnostics banner */}
      {(hasErrors || engineStale) && (
        <div className="px-2.5 py-1 border-b border-vsc-border shrink-0 bg-[#3e1a1a]">
          <div className="font-vsc-mono text-[10px] text-[#f48771]">
            {hasErrors ? `${errors.length} error${errors.length !== 1 ? 's' : ''}` : 'Build is stale'} — using last successful build
          </div>
        </div>
      )}

      {/* Sidebar toggle */}
      <div className="flex items-center gap-1.5 px-2.5 py-1 border-b border-vsc-border shrink-0 bg-vsc-surface">
        <button
          onClick={() => setSidebarOpen((prev) => !prev)}
          className="p-0.5 rounded hover:bg-vsc-hover cursor-pointer"
          title={sidebarOpen ? 'Hide sidebar' : 'Show sidebar'}
        >
          <PanelLeft className="h-3.5 w-3.5 text-vsc-text-muted" />
        </button>
        {selectedFn && (
          <span className="text-[11px] font-vsc-mono text-vsc-accent font-semibold">{selectedFn}()</span>
        )}
        {selectedFn && (
          <div className="flex items-center gap-1 ml-auto">
            <button
              disabled={hasErrors || isRunning || !selectedProject}
              onClick={onRunFunction}
              className={`px-3 py-0.5 rounded border-none font-semibold text-[11px] ${
                hasErrors || isRunning || !selectedProject
                  ? 'bg-vsc-text-faint text-vsc-text-muted cursor-not-allowed'
                  : 'bg-vsc-green text-white cursor-pointer'
              }`}
            >
              {isRunning ? 'Running...' : 'Run'}
            </button>
            {runs.length > 0 && !isRunning && (
              <button
                onClick={() => {
                  const runIds = runs.map((r) => r.id);
                  port.postMessage({ type: 'clearHandles', runIds });
                  setRuns([]);
                }}
                className="px-2 py-0.5 rounded border border-vsc-border bg-transparent text-vsc-text-muted text-[10px] cursor-pointer"
              >
                Clear
              </button>
            )}
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
                functions={functions}
                tests={tests}
                selectedFn={selectedFn}
                onSelectFn={setSelectedFn}
                onSelectTest={handleSelectTest}
                onRunTest={handleRunTest}
                isRunning={isRunning}
                testStatuses={testStatuses}
                onRunAllTests={() => handleRunAllTests(tests)}
                onStopAllTests={handleStopAllTests}
                onRerunFailed={handleRerunFailed}
                hasFailedTests={runs.some((r) => r.testName != null && r.status === 'error')}
                hasRunningTests={runs.some((r) => r.testName != null && r.status === 'running')}
                parallelTests={parallelTests}
                onToggleParallel={() => setParallelTests((p) => !p)}
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
          {/* Tab switcher */}
          {selectedFn && (
            <div className="flex items-center gap-0 px-2.5 py-0 border-b border-vsc-border shrink-0 bg-vsc-surface">
              <button
                onClick={() => setActiveTab('run')}
                className={`px-3 py-1.5 text-[11px] font-vsc-mono border-b-2 cursor-pointer bg-transparent ${
                  activeTab === 'run'
                    ? 'border-vsc-accent text-vsc-text font-semibold'
                    : 'border-transparent text-vsc-text-muted'
                }`}
              >
                Run
              </button>
              <button
                onClick={() => setActiveTab('graph')}
                className={`px-3 py-1.5 text-[11px] font-vsc-mono border-b-2 cursor-pointer bg-transparent ${
                  activeTab === 'graph'
                    ? 'border-vsc-accent text-vsc-text font-semibold'
                    : 'border-transparent text-vsc-text-muted'
                }`}
              >
                Graph
              </button>
              {canPreviewPrompt && (
                <button
                  onClick={() => setActiveTab('prompt')}
                  className={`flex items-center gap-1 px-3 py-1.5 text-[11px] font-vsc-mono border-b-2 cursor-pointer bg-transparent ${
                    activeTab === 'prompt'
                      ? 'border-vsc-accent text-vsc-text font-semibold'
                      : 'border-transparent text-vsc-text-muted'
                  }`}
                >
                  Prompt
                  {selectedFnInfo?.capabilities?.clientName && (
                    <span className="ml-1 px-1 py-0 text-[9px] rounded bg-vsc-bg-secondary text-vsc-text-faint">
                      {selectedFnInfo.capabilities.clientName}
                    </span>
                  )}
                </button>
              )}
              {canPreviewCurl && (
                <button
                  onClick={() => setActiveTab('curl')}
                  className={`px-3 py-1.5 text-[11px] font-vsc-mono border-b-2 cursor-pointer bg-transparent ${
                    activeTab === 'curl'
                      ? 'border-vsc-accent text-vsc-text font-semibold'
                      : 'border-transparent text-vsc-text-muted'
                  }`}
                >
                  cURL
                </button>
              )}
            </div>
          )}

          {/* Graph view */}
          {selectedFn && activeTab === 'graph' ? (
            <div className="flex-1 min-h-0" style={{ minHeight: 300 }}>
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
            </div>
          ) : null}

          {/* Prompt preview */}
          {selectedFn && activeTab === 'prompt' ? (
            <div className="flex-1 flex flex-col overflow-hidden">
              {promptPreviewError && (
                <div className="px-2.5 py-1.5 text-[10px] text-vsc-error bg-vsc-error/10 border-b border-vsc-error/20 shrink-0">
                  Preview error: {promptPreviewError}
                </div>
              )}
              <div className="flex-1 overflow-auto font-vsc-mono text-xs bg-vsc-bg p-2.5 group relative">
                {promptPreviewResult != null && (
                  <div className="absolute top-1 right-1 z-10">
                    <CopyButton text={promptPreviewResult} />
                  </div>
                )}
                {promptPreviewResult != null ? (
                  <ResultDisplay resultJson={promptPreviewResult} customRenderers={resultRenderers} />
                ) : (
                  <div className="flex items-center justify-center text-vsc-text-faint text-xs h-full">
                    {previewLoading ? 'Loading prompt preview...' : 'Enter args to preview prompt'}
                  </div>
                )}
              </div>
              {promptPreviewResult && <PromptStats text={promptPreviewResult} />}
            </div>
          ) : null}

          {/* cURL preview */}
          {selectedFn && activeTab === 'curl' ? (
            <div className="group relative flex-1 overflow-auto font-vsc-mono text-xs bg-vsc-bg p-2.5">
              {curlPreviewResult != null && (
                <div className="absolute top-1 right-1 z-10">
                  <CopyButton text={curlPreviewResult} />
                </div>
              )}
              {curlPreviewResult != null ? (
                <ResultDisplay resultJson={curlPreviewResult} customRenderers={resultRenderers} />
              ) : curlPreviewError ? (
                <div className="flex items-center justify-center text-vsc-error text-xs h-full">
                  {curlPreviewError}
                </div>
              ) : (
                <div className="flex items-center justify-center text-vsc-text-faint text-xs h-full">
                  {previewLoading ? 'Loading cURL preview...' : 'Enter args to preview cURL'}
                </div>
              )}
            </div>
          ) : null}

          {/* Execution area */}
          {selectedFn && activeTab === 'run' ? (
            <div className="flex-1 flex flex-col min-h-0">
              {/* Args */}
              <div className="flex items-center border-b border-vsc-border shrink-0">
                <span className="px-2 py-1 text-[10px] text-vsc-text-faint font-vsc-mono bg-vsc-surface border-r border-vsc-border self-stretch flex items-center">
                  args
                </span>
                <input
                  spellCheck={false}
                  value={argsJson}
                  onChange={onArgsJsonChange}
                  className="flex-1 px-2 py-1 font-vsc-mono text-xs bg-vsc-input-bg text-vsc-input-fg border-none outline-none"
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
                            <button
                              onClick={() => onCancelRun(run.id)}
                              className="p-0.5 rounded hover:bg-vsc-hover text-vsc-text-muted hover:text-vsc-error"
                              title="Cancel execution"
                            >
                              <Square size={12} />
                            </button>
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
                                {log.error && <pre className={`${codeBlockCls} border-vsc-red! text-vsc-red!`}>{log.error}</pre>}
                                <div>
                                  <div className="text-[10px] font-semibold text-vsc-text-muted mb-0.5 uppercase tracking-wide">Request Headers</div>
                                  <pre className={codeBlockCls}>{JSON.stringify(log.requestHeaders, null, 2)}</pre>
                                </div>
                                {log.requestBody && (
                                  <div>
                                    <div className="text-[10px] font-semibold text-vsc-text-muted mb-0.5 uppercase tracking-wide">Request Body</div>
                                    <pre className={codeBlockCls}>{tryFormatJson(log.requestBody)}</pre>
                                  </div>
                                )}
                                {log.responseBody != null && (
                                  <div>
                                    <div className="text-[10px] font-semibold text-vsc-text-muted mb-0.5 uppercase tracking-wide">Response Body</div>
                                    <pre className={codeBlockCls}>{tryFormatJson(log.responseBody)}</pre>
                                  </div>
                                )}
                              </div>
                            )}
                          </div>
                        );
                      })}

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
                              <button
                                onClick={() => toggleResultMode(run.id)}
                                className={`px-1.5 py-0.5 text-[10px] rounded ${(resultModes[run.id] ?? 'parsed') === 'parsed' ? 'bg-vsc-accent text-vsc-accent-fg' : 'text-vsc-text-muted'}`}
                              >
                                Parsed
                              </button>
                              <button
                                onClick={() => toggleResultMode(run.id)}
                                className={`px-1.5 py-0.5 text-[10px] rounded ${(resultModes[run.id] ?? 'parsed') === 'raw' ? 'bg-vsc-accent text-vsc-accent-fg' : 'text-vsc-text-muted'}`}
                              >
                                Raw
                              </button>
                              <CopyButton text={run.result} iconSize={11} />
                            </div>
                            {(resultModes[run.id] ?? 'parsed') === 'parsed' ? (
                              <ResultDisplay resultJson={run.result} customRenderers={resultRenderers} />
                            ) : (
                              <pre className="whitespace-pre-wrap break-all font-vsc-mono text-[11px] text-vsc-text bg-vsc-bg-secondary p-2 rounded border border-vsc-border max-h-[400px] overflow-auto">
                                {run.result}
                              </pre>
                            )}
                          </div>
                        </div>
                      )}
                    </div>
                  );
                })}
              </div>
            </div>
          ) : null}

          {/* No function selected fallback */}
          {!selectedFn && (
            <div className="flex-1 flex items-center justify-center text-vsc-text-faint text-xs bg-vsc-bg">
              Select a function to run
            </div>
          )}
        </div>
      </div>

      <ApiKeysDialog
        open={showApiKeysDialog}
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
        envVars={envVars}
        requiredKeys={knownRequiredKeys}
        onSetEnvVar={addEnvVar}
        onDeleteEnvVar={removeEnvVar}
        onImportEnvVars={handleImportEnvVars}
      />
    </>
  );
};
