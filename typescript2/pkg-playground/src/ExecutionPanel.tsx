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
import { encodeCallArgs, decodeCallResult } from '@b/pkg-proto';
import type { RuntimePort } from './runtime-port';
import type {
  DiagnosticEntry,
  FetchLogEntry,
  EnvVarRequest,
  ProjectUpdate,
  RunEntry,
  WorkerOutMessage,
} from './worker-protocol';

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
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export const ExecutionPanel: FC<ExecutionPanelProps> = ({ port, connectionVersion }) => {
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

  const [buildTime, setBuildTime] = useState<number | null>(null);
  const [envRequests, setEnvRequests] = useState<EnvVarRequest[]>([]);
  const [envVars, setEnvVarsState] = useState<Record<string, string>>({});
  const [envInputs, setEnvInputs] = useState<Record<number, string>>({});
  const [newEnvKey, setNewEnvKey] = useState('');
  const [newEnvValue, setNewEnvValue] = useState('');

  // Ref mirror of envVars so the message handler closure always sees current values.
  const envVarsRef = useRef(envVars);
  useEffect(() => { envVarsRef.current = envVars; }, [envVars]);

  const nextCallIdRef = useRef(0);
  const pendingCallsRef = useRef<Map<number, { resolve: (v: Uint8Array) => void; reject: (e: Error) => void }>>(new Map());

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
            pending.resolve(new Uint8Array(data.result));
          }
          break;
        }

        case 'callFunctionError': {
          const pending = pendingCallsRef.current.get(data.id);
          if (pending) {
            pendingCallsRef.current.delete(data.id);
            pending.reject(new Error(data.error));
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
          const cached = envVarsRef.current[data.variable];
          if (cached !== undefined) {
            // Auto-respond from UI cache — no prompt needed.
            port.postMessage({ type: 'envVarResponse', id: data.id, value: cached, variable: data.variable });
          } else {
            setEnvRequests((prev) => [...prev, { id: data.id, variable: data.variable }]);
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

  // Sync existing envVars to the port whenever port changes
  useEffect(() => {
    for (const [key, value] of Object.entries(envVars)) {
      port.postMessage({ type: 'setEnvVar', key, value });
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps -- only sync when port changes
  }, [port]);

  // ── Env var helpers ────────────────────────────────────────────────────

  const resolveEnvRequest = useCallback((reqId: number, value: string | undefined) => {
    setEnvRequests((prev) => {
      const req = prev.find((r) => r.id === reqId);
      if (!req) return prev;
      if (value !== undefined) {
        setEnvVarsState((prevVars) => ({ ...prevVars, [req.variable]: value }));
      }
      port.postMessage({ type: 'envVarResponse', id: reqId, value, variable: req.variable });
      return prev.filter((r) => r.id !== reqId);
    });
  }, [port]);

  const addEnvVar = useCallback((key: string, value: string) => {
    setEnvVarsState((prev) => ({ ...prev, [key]: value }));
    port.postMessage({ type: 'setEnvVar', key, value });
  }, [port]);

  const removeEnvVar = useCallback((key: string) => {
    setEnvVarsState((prev) => { const { [key]: _, ...rest } = prev; return rest; });
    port.postMessage({ type: 'deleteEnvVar', key });
  }, [port]);

  const onArgsJsonChange = useCallback((e: ChangeEvent<HTMLInputElement>) => {
    setArgsJson(e.target.value);
  }, []);

  // ── Run function ───────────────────────────────────────────────────────

  const isRunning = runs.length > 0 && runs[runs.length - 1].status === 'running';

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

      const resultBytes = await new Promise<Uint8Array>((resolve, reject) => {
        pendingCallsRef.current.set(runId, { resolve, reject });
        port.postMessage(
          { type: 'callFunction', id: runId, name: selectedFn, argsProto: new Uint8Array(argsProto), project: selectedProject },
        );
      });

      const decoded = decodeCallResult(resultBytes);
      const resultStr = JSON.stringify(decoded, null, 2);
      const dur = Math.round(performance.now() - startTime);
      setRuns((prev) => prev.map((r) => r.id === runId ? { ...r, result: resultStr, status: 'success', durationMs: dur } : r));
    } catch (e) {
      const errMsg = e instanceof Error ? e.message : String(e);
      const dur = Math.round(performance.now() - startTime);
      setRuns((prev) => prev.map((r) => r.id === runId ? { ...r, error: errMsg, status: 'error', durationMs: dur } : r));
    }
  }, [selectedFn, selectedProject, argsJson, isRunning, port]);

  // ── Derived state ──────────────────────────────────────────────────────

  const currentUpdate = selectedProject ? projectUpdates[selectedProject] : undefined;
  const functionNames = currentUpdate?.functions ?? [];
  const engineStale = currentUpdate ? !currentUpdate.isBexCurrent : false;

  useEffect(() => {
    setSelectedFn((prev) => prev && !functionNames.includes(prev) ? null : prev);
  }, [functionNames]);

  const errors = diags.filter((d) => d.severity === 'error');
  const warnings = diags.filter((d) => d.severity === 'warning');
  const hasErrors = errors.length > 0;

  const envInputRow = (
    <form
      onSubmit={(e) => e.preventDefault()}
      className="contents"
    >
      <input
        placeholder="KEY"
        value={newEnvKey}
        onChange={(e) => setNewEnvKey(e.target.value)}
        className="w-[60px] px-1.5 py-px rounded-sm border border-vsc-input-border bg-vsc-input-bg text-vsc-input-fg font-vsc-mono text-[10px] outline-none"
      />
      <input
        type="password"
        autoComplete="off"
        data-1p-ignore
        data-lpignore="true"
        placeholder="value"
        value={newEnvValue}
        onChange={(e) => setNewEnvValue(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === 'Enter' && newEnvKey.trim()) {
            addEnvVar(newEnvKey.trim(), newEnvValue);
            setNewEnvKey('');
            setNewEnvValue('');
          }
        }}
        className="w-[90px] px-1.5 py-px rounded-sm border border-vsc-input-border bg-vsc-input-bg text-vsc-input-fg font-vsc-mono text-[10px] outline-none"
      />
      <button
        type="button"
        disabled={!newEnvKey.trim()}
        onClick={() => {
          if (newEnvKey.trim()) {
            addEnvVar(newEnvKey.trim(), newEnvValue);
            setNewEnvKey('');
            setNewEnvValue('');
          }
        }}
        className={`px-1.5 py-px rounded-sm border-none text-[10px] font-semibold ${
          newEnvKey.trim()
            ? 'bg-vsc-accent text-vsc-accent-fg cursor-pointer'
            : 'bg-vsc-text-faint text-vsc-text-muted cursor-default'
        }`}
      >
        +
      </button>
    </form>
  );

  // ── Render ─────────────────────────────────────────────────────────────

  return (
    <>
      {buildTime != null && (
        <span data-testid="hot-reload-test" style={{ display: 'none' }}>{buildTime}</span>
      )}
      {(connectionVersion != null || buildTime != null) && (
        <div className="flex items-center gap-1.5 px-2.5 py-0.5 shrink-0 border-b border-vsc-border bg-vsc-surface">
          {connectionVersion != null && (
            <>
              <span className="text-[10px] text-vsc-text-faint font-vsc-mono select-none">PORT</span>
              <code className="text-[10px] font-vsc-mono text-vsc-text-muted">v{connectionVersion}</code>
            </>
          )}
          {buildTime != null && (() => {
            const { absolute, relative } = formatBuildTime(buildTime);
            return (
              <>
                <span className="text-[10px] text-vsc-text-faint font-vsc-mono select-none ml-2">Built</span>
                <code className="text-[10px] font-vsc-mono text-vsc-text-muted">
                  {absolute} ({relative})
                </code>
              </>
            );
          })()}
        </div>
      )}
      {/* ──── Env vars ──── */}
      <div className="flex items-center gap-1.5 px-2.5 py-1 flex-wrap shrink-0 border-b border-vsc-border bg-vsc-surface">
        <span className="text-[10px] text-vsc-text-faint font-vsc-mono select-none">ENV</span>
        {Object.keys(envVars).map((key) => (
          <span key={key} className="text-[10px] font-vsc-mono text-vsc-text-muted bg-vsc-bg px-1.5 py-px rounded border border-vsc-border-subtle inline-flex items-center gap-0.5">
            {key}
            <span onClick={() => removeEnvVar(key)} className="cursor-pointer text-vsc-text-faint leading-none">&times;</span>
          </span>
        ))}
        {envInputRow}
      </div>

      {/* Env var request banner */}
      {envRequests.length > 0 && (
        <div className="bg-vsc-yellow-subtle border-b border-vsc-border shrink-0">
          {envRequests.map((req) => (
            <form
              key={req.id}
              onSubmit={(e) => e.preventDefault()}
              className="flex items-center gap-1.5 px-2.5 py-1.5"
            >
              <span className="text-[10px] text-vsc-yellow font-semibold">ENV</span>
              <code className="font-vsc-mono text-[11px] text-vsc-yellow">{req.variable}</code>
              <input
                type="password"
                autoComplete="off"
                data-1p-ignore
                data-lpignore="true"
                autoFocus
                placeholder="paste value..."
                value={envInputs[req.id] ?? ''}
                onChange={(e) => setEnvInputs((prev) => ({ ...prev, [req.id]: e.target.value }))}
                onKeyDown={(e) => {
                  if (e.key === 'Enter') {
                    resolveEnvRequest(req.id, envInputs[req.id] ?? '');
                    setEnvInputs((prev) => { const { [req.id]: _, ...rest } = prev; return rest; });
                  }
                }}
                className="flex-1 px-1.5 py-0.5 rounded-sm border border-vsc-input-border bg-vsc-input-bg font-vsc-mono text-[11px] text-vsc-input-fg outline-none"
              />
              <button
                type="button"
                onClick={() => { resolveEnvRequest(req.id, envInputs[req.id] ?? ''); setEnvInputs((prev) => { const { [req.id]: _, ...rest } = prev; return rest; }); }}
                className="px-2 py-0.5 rounded-sm border-none bg-vsc-accent text-vsc-accent-fg font-semibold text-[10px] cursor-pointer"
              >
                Set
              </button>
              <button
                type="button"
                onClick={() => { resolveEnvRequest(req.id, undefined); setEnvInputs((prev) => { const { [req.id]: _, ...rest } = prev; return rest; }); }}
                className="px-1.5 py-0.5 rounded-sm border border-vsc-border bg-transparent text-vsc-text-muted text-[10px] cursor-pointer"
              >
                Skip
              </button>
            </form>
          ))}
        </div>
      )}

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

      {/* Functions toolbar */}
      <div className="px-2.5 py-1.5 border-b border-vsc-border shrink-0">
        {functionNames.length > 0 ? (
          <div className="flex flex-wrap items-center gap-1">
            {functionNames.map((name) => {
              const sel = selectedFn === name;
              return (
                <button
                  key={name}
                  onClick={() => setSelectedFn(sel ? null : name)}
                  className={`px-2 py-0.5 rounded font-vsc-mono text-[11px] cursor-pointer border ${
                    sel
                      ? 'bg-vsc-accent text-vsc-accent-fg border-vsc-accent font-semibold'
                      : 'bg-transparent text-vsc-accent border-vsc-border'
                  }`}
                >
                  {name}()
                </button>
              );
            })}
            {selectedFn && (
              <>
                <button
                  disabled={hasErrors || isRunning || !selectedProject}
                  onClick={onRunFunction}
                  className={`px-3 py-0.5 rounded border-none ml-1 font-semibold text-[11px] ${
                    hasErrors || isRunning || !selectedProject
                      ? 'bg-vsc-text-faint text-vsc-text-muted cursor-not-allowed'
                      : 'bg-vsc-green text-white cursor-pointer'
                  }`}
                >
                  {isRunning ? 'Running...' : 'Run'}
                </button>
                {runs.length > 0 && !isRunning && (
                  <button
                    onClick={() => setRuns([])}
                    className="px-2 py-0.5 rounded ml-0.5 border border-vsc-border bg-transparent text-vsc-text-muted text-[10px] cursor-pointer"
                  >
                    Clear
                  </button>
                )}
              </>
            )}
          </div>
        ) : (
          <span className="text-vsc-text-faint text-[11px]">No functions yet</span>
        )}
      </div>

      {/* Execution area */}
      {selectedFn ? (
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
              const statusCls = run.status === 'error' ? 'bg-vsc-red' : run.status === 'success' ? 'bg-vsc-green' : 'bg-vsc-text-muted';

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
                      <span className="text-vsc-text-muted text-[10px]">running...</span>
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

                  {/* Result / Error for this run */}
                  {run.error && (
                    <div className="py-1.5 pr-2.5 pl-[22px]">
                      <div className="text-[10px] font-semibold text-vsc-red mb-0.5 uppercase tracking-wide">Error</div>
                      <pre className={`${codeBlockCls} border-vsc-red! text-vsc-red!`}>{run.error}</pre>
                    </div>
                  )}
                  {run.result != null && (
                    <div className="py-1.5 pr-2.5 pl-[22px]">
                      <div className="text-[10px] font-semibold text-vsc-green mb-0.5 uppercase tracking-wide">Result</div>
                      <pre className={`${codeBlockCls} border-vsc-green! text-vsc-green!`}>{run.result}</pre>
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        </div>
      ) : (
        <div className="flex-1 flex items-center justify-center text-vsc-text-faint text-xs bg-vsc-bg">
          Select a function to run
        </div>
      )}

    </>
  );
};
