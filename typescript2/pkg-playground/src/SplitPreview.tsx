import type { ChangeEvent, FC } from 'react';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { version, hotReloadTestString } from '@b/bridge_wasm';
import type { BamlWasmRuntime } from '@b/bridge_wasm';
import { encodeCallArgs, decodeCallResult } from '@b/pkg-proto';
import { usePlayground } from './PlaygroundProvider';
import {
  getRuntime, isGenCurrent, setInitialCode,
  subscribeFetchLogs,
  subscribeEnvRequests, resolveEnvRequest,
  subscribeEnvVars, setEnvVar, deleteEnvVar,
} from './wasmRuntime';
import type { FetchLogEntry, EnvVarRequest } from './wasmRuntime';

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface DiagnosticEntry {
  severity: 'error' | 'warning' | 'info';
  message: string;
}

// ---------------------------------------------------------------------------
// Design tokens
// ---------------------------------------------------------------------------

const mono = '"SF Mono", "Fira Code", Consolas, "Liberation Mono", monospace';
const sans = '-apple-system, BlinkMacSystemFont, "Segoe UI", Helvetica, Arial, sans-serif';

const c = {
  bg: '#0d1117',
  surface: '#161b22',
  surfaceHover: '#1c2333',
  border: '#30363d',
  borderSubtle: '#21262d',
  text: '#c9d1d9',
  textBright: '#e6edf3',
  textMuted: '#8b949e',
  textFaint: '#484f58',
  accent: '#58a6ff',
  accentSubtle: 'rgba(56,139,253,0.15)',
  green: '#3fb950',
  red: '#f85149',
  yellow: '#d29922',
  yellowSubtle: 'rgba(210,153,34,0.15)',
};

function tryFormatJson(str: string): string {
  try { return JSON.stringify(JSON.parse(str), null, 2); } catch { return str; }
}

const codeBlock = {
  margin: 0, whiteSpace: 'pre-wrap' as const, wordBreak: 'break-all' as const,
  fontFamily: mono, fontSize: 12, lineHeight: 1.5,
  padding: '8px 10px', borderRadius: 6,
  background: c.bg, border: `1px solid ${c.border}`,
  color: c.text, overflow: 'auto' as const, maxHeight: 200,
};

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export const SplitPreview: FC = () => {
  const { code, setCode } = usePlayground();
  const runtimeRef = useRef<BamlWasmRuntime | null>(null);
  const genRef = useRef<number>(-1);
  const [functionNames, setFunctionNames] = useState<string[]>([]);
  const [isReady, setReady] = useState(false);
  const [diags, setDiags] = useState<DiagnosticEntry[]>([]);
  const [engineStale, setEngineStale] = useState(false);
  const [hotReloadTestStr, setHotReloadTestStr] = useState<string | null>(null);

  const [selectedFn, setSelectedFn] = useState<string | null>(null);
  const [argsJson, setArgsJson] = useState('{}');
  const [result, setResult] = useState<string | null>(null);
  const [resultError, setResultError] = useState<string | null>(null);
  const [isRunning, setIsRunning] = useState(false);

  const [fetchLogs, setFetchLogs] = useState<FetchLogEntry[]>([]);
  const [expandedLogId, setExpandedLogId] = useState<number | null>(null);

  const [envRequests, setEnvRequests] = useState<EnvVarRequest[]>([]);
  const [envVars, setEnvVarsState] = useState<Record<string, string>>({});
  const [envInputs, setEnvInputs] = useState<Record<number, string>>({});
  const [newEnvKey, setNewEnvKey] = useState('');
  const [newEnvValue, setNewEnvValue] = useState('');

  useEffect(() => subscribeFetchLogs(setFetchLogs), []);
  useEffect(() => subscribeEnvRequests(setEnvRequests), []);
  useEffect(() => subscribeEnvVars(setEnvVarsState), []);

  useEffect(() => {
    let metaTag: HTMLMetaElement | null = null;
    let cancelled = false;
    getRuntime()
      .then(() => {
        if (cancelled) return;
        try {
          const ver = version();
          metaTag = document.createElement('meta');
          metaTag.name = 'baml-version';
          metaTag.content = ver;
          document.head.appendChild(metaTag);
        } catch {}
        try { setHotReloadTestStr(hotReloadTestString()); } catch {}
      })
      .catch(() => {});
    return () => {
      cancelled = true;
      if (metaTag?.parentNode) metaTag.parentNode.removeChild(metaTag);
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    setInitialCode(code);
    getRuntime()
      .then(({ rt, gen }) => {
        if (cancelled) return;
        runtimeRef.current = rt;
        genRef.current = gen;
        setFunctionNames(rt.functionNames());
        setDiags(JSON.parse(rt.diagnostics()) as DiagnosticEntry[]);
        setEngineStale(!rt.engineIsCurrent());
        setReady(true);
      })
      .catch((cause: unknown) => {
        if (cancelled) return;
        setDiags([{ severity: 'error', message: cause instanceof Error ? cause.message : String(cause) }]);
      });
    return () => { cancelled = true; };
  }, []);

  useEffect(() => {
    const rt = runtimeRef.current;
    const gen = genRef.current;
    if (!isReady || !rt || !isGenCurrent(gen)) return;
    rt.setSource(code);
    const stale = !rt.engineIsCurrent();
    setEngineStale(stale);
    const names = rt.functionNames();
    setFunctionNames(names);
    setDiags(JSON.parse(rt.diagnostics()) as DiagnosticEntry[]);
    if (selectedFn && !names.includes(selectedFn)) setSelectedFn(null);
  }, [code, isReady, selectedFn]);

  const onChange = useMemo(
    () => (e: ChangeEvent<HTMLTextAreaElement>) => setCode(e.target.value),
    [setCode],
  );

  const onRunFunction = useCallback(async () => {
    const rt = runtimeRef.current;
    const gen = genRef.current;
    if (!rt || !selectedFn || !isGenCurrent(gen) || isRunning) return;
    setIsRunning(true);
    setResult(null);
    setResultError(null);
    try {
      const parsed = JSON.parse(argsJson);
      if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) {
        throw new Error('Arguments must be a JSON object, e.g. {"arr": [3,1,2]}');
      }
      const argsProto = encodeCallArgs(parsed as Record<string, unknown>);
      const resultBytes = await rt.callFunction(selectedFn, argsProto);
      if (!isGenCurrent(gen)) { setResultError('Runtime was disposed during execution'); return; }
      const decoded = decodeCallResult(resultBytes);
      setResult(JSON.stringify(decoded, null, 2));
    } catch (e) {
      setResultError(e instanceof Error ? e.message : String(e));
    } finally {
      setIsRunning(false);
    }
  }, [selectedFn, argsJson, isRunning]);

  const errors = diags.filter((d) => d.severity === 'error');
  const warnings = diags.filter((d) => d.severity === 'warning');
  const hasErrors = errors.length > 0;
  const envCount = Object.keys(envVars).length;

  return (
    <div style={{ fontFamily: sans, color: c.text, display: 'flex', flexDirection: 'column', gap: 6 }}>
      {hotReloadTestStr && <span data-testid="hot-reload-test" style={{ display: 'none' }}>{hotReloadTestStr}</span>}

      {/* ── Env var request banner (above main panel) ── */}
      {envRequests.length > 0 && (
        <div style={{ background: c.yellowSubtle, border: `1px solid ${c.yellow}44`, borderRadius: 6, overflow: 'hidden' }}>
          {envRequests.map((req) => (
            <div key={req.id} style={{ display: 'flex', alignItems: 'center', gap: 6, padding: '5px 10px' }}>
              <span style={{ fontSize: 10, color: c.yellow, fontWeight: 600 }}>ENV</span>
              <code style={{ fontFamily: mono, fontSize: 11, color: c.yellow }}>{req.variable}</code>
              <input
                type="password"
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
                style={{
                  flex: 1, padding: '3px 6px', borderRadius: 3,
                  border: `1px solid ${c.border}`, background: c.bg,
                  fontFamily: mono, fontSize: 11, color: c.text, outline: 'none',
                }}
              />
              <button
                onClick={() => { resolveEnvRequest(req.id, envInputs[req.id] ?? ''); setEnvInputs((prev) => { const { [req.id]: _, ...rest } = prev; return rest; }); }}
                style={{ padding: '3px 8px', borderRadius: 3, border: 'none', background: c.yellow, color: c.bg, fontWeight: 600, fontSize: 10, cursor: 'pointer' }}
              >
                Set
              </button>
              <button
                onClick={() => { resolveEnvRequest(req.id, undefined); setEnvInputs((prev) => { const { [req.id]: _, ...rest } = prev; return rest; }); }}
                style={{ padding: '3px 6px', borderRadius: 3, border: `1px solid ${c.border}`, background: 'none', color: c.textMuted, fontSize: 10, cursor: 'pointer' }}
              >
                Skip
              </button>
            </div>
          ))}
        </div>
      )}

      {/* ════════ Main panel ════════ */}
      <div style={{ background: c.bg, borderRadius: 8, border: `1px solid ${c.border}`, overflow: 'hidden', display: 'flex', minHeight: 500 }}>

        {/* ──── Left: Editor ──── */}
        <div style={{ flex: '0 0 55%', display: 'flex', flexDirection: 'column', borderRight: `1px solid ${c.border}` }}>
          <textarea
            spellCheck={false}
            value={code}
            onChange={onChange}
            style={{
              flex: 1, padding: 12,
              fontFamily: mono, fontSize: 13, lineHeight: 1.6,
              background: c.bg, color: c.text,
              border: 'none', outline: 'none', resize: 'none',
            }}
          />
        </div>

        {/* ──── Right: Functions + Execution ──── */}
        <div style={{ flex: 1, display: 'flex', flexDirection: 'column', minWidth: 0, overflow: 'hidden' }}>

          {/* Functions + Run */}
          <div style={{ padding: '8px 10px', borderBottom: `1px solid ${c.border}`, background: c.surface, flexShrink: 0 }}>
            {functionNames.length > 0 ? (
              <div style={{ display: 'flex', flexWrap: 'wrap', alignItems: 'center', gap: 4 }}>
                {functionNames.map((name) => {
                  const sel = selectedFn === name;
                  return (
                    <button
                      key={name}
                      onClick={() => setSelectedFn(sel ? null : name)}
                      style={{
                        padding: '2px 8px', borderRadius: 4,
                        fontFamily: mono, fontSize: 11, cursor: 'pointer',
                        background: sel ? c.accent : 'transparent',
                        color: sel ? '#fff' : c.accent,
                        border: sel ? `1px solid ${c.accent}` : `1px solid ${c.accentSubtle}`,
                        fontWeight: sel ? 600 : 400,
                      }}
                    >
                      {name}()
                    </button>
                  );
                })}
                {selectedFn && (
                  <button
                    disabled={hasErrors || isRunning}
                    onClick={onRunFunction}
                    style={{
                      padding: '2px 12px', borderRadius: 4, border: 'none', marginLeft: 4,
                      background: hasErrors || isRunning ? c.textFaint : c.green,
                      color: hasErrors || isRunning ? c.textMuted : '#fff',
                      fontWeight: 600, fontSize: 11, cursor: hasErrors || isRunning ? 'not-allowed' : 'pointer',
                    }}
                  >
                    {isRunning ? 'Running...' : 'Run'}
                  </button>
                )}
                {errors.length > 0 && <span style={{ fontSize: 10, color: c.red, marginLeft: 2 }}>{errors.length} error{errors.length !== 1 ? 's' : ''}</span>}
                {warnings.length > 0 && <span style={{ fontSize: 10, color: c.yellow, marginLeft: 2 }}>{warnings.length} warning{warnings.length !== 1 ? 's' : ''}</span>}
              </div>
            ) : (
              <span style={{ color: c.textFaint, fontSize: 11 }}>No functions yet</span>
            )}
            {engineStale && errors.length === 0 && (
              <div style={{ fontFamily: mono, fontSize: 10, color: c.yellow, marginTop: 4 }}>
                Engine stale — using last successful build
              </div>
            )}
            {diags.length > 0 && (
              <div style={{
                fontFamily: mono, fontSize: 10, lineHeight: 1.5, marginTop: 4,
                ...(errors.length > 0 ? { padding: '4px 8px', borderRadius: 4, background: `${c.red}11`, border: `1px solid ${c.red}33` } : {}),
              }}>
                {diags.map((d, i) => (
                  <div key={i} style={{ color: d.severity === 'error' ? c.red : d.severity === 'warning' ? c.yellow : c.accent, whiteSpace: 'pre-wrap', wordBreak: 'break-word' }}>
                    {d.message}
                  </div>
                ))}
              </div>
            )}
          </div>

          {/* Execution */}
          {selectedFn ? (
            <div style={{ flex: 1, display: 'flex', flexDirection: 'column', minHeight: 0 }}>
              {/* Args */}
              <div style={{ display: 'flex', alignItems: 'center', borderBottom: `1px solid ${c.border}`, flexShrink: 0 }}>
                <span style={{ padding: '4px 8px', fontSize: 10, color: c.textFaint, fontFamily: mono, background: c.surface, borderRight: `1px solid ${c.border}`, alignSelf: 'stretch', display: 'flex', alignItems: 'center' }}>args</span>
                <input
                  spellCheck={false}
                  value={argsJson}
                  onChange={(e: ChangeEvent<HTMLInputElement>) => setArgsJson(e.target.value)}
                  style={{
                    flex: 1, padding: '4px 8px',
                    fontFamily: mono, fontSize: 12,
                    background: c.bg, color: c.text,
                    border: 'none', outline: 'none',
                  }}
                  placeholder='{"key": "value"}'
                />
              </div>

              {/* Output (scrollable) */}
              <div style={{ flex: 1, overflow: 'auto', fontFamily: mono, fontSize: 12 }}>
                {fetchLogs.map((log) => {
                  const isExp = expandedLogId === log.id;
                  const sc = log.status === null ? c.textMuted
                    : log.status >= 200 && log.status < 300 ? c.green
                    : log.status === 0 ? c.red : c.yellow;
                  return (
                    <div key={`n-${log.id}`}>
                      <div
                        onClick={() => setExpandedLogId(isExp ? null : log.id)}
                        style={{
                          display: 'flex', alignItems: 'center', gap: 6,
                          padding: '4px 10px', cursor: 'pointer',
                          background: c.surface,
                          borderBottom: `1px solid ${c.borderSubtle}`,
                        }}
                      >
                        <span style={{ color: sc, fontWeight: 600, fontSize: 11 }}>{log.status ?? '...'}</span>
                        <span style={{ color: c.textFaint, fontSize: 10 }}>{log.method}</span>
                        <span style={{ color: c.text, flex: 1, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', fontSize: 11 }}>{log.url}</span>
                        {log.durationMs != null && <span style={{ color: c.textFaint, fontSize: 10 }}>{log.durationMs}ms</span>}
                        <span style={{ color: c.textFaint, fontSize: 9 }}>{isExp ? '\u25B4' : '\u25BE'}</span>
                      </div>
                      {isExp && (
                        <div style={{ padding: '8px 10px', display: 'flex', flexDirection: 'column', gap: 8, borderBottom: `1px solid ${c.border}` }}>
                          {log.error && <pre style={{ ...codeBlock, borderColor: `${c.red}44`, color: c.red }}>{log.error}</pre>}
                          <div>
                            <div style={{ fontSize: 10, fontWeight: 600, color: c.textMuted, marginBottom: 3, textTransform: 'uppercase', letterSpacing: '0.05em' }}>Request Headers</div>
                            <pre style={codeBlock}>{JSON.stringify(log.requestHeaders, null, 2)}</pre>
                          </div>
                          {log.requestBody && (
                            <div>
                              <div style={{ fontSize: 10, fontWeight: 600, color: c.textMuted, marginBottom: 3, textTransform: 'uppercase', letterSpacing: '0.05em' }}>Request Body</div>
                              <pre style={codeBlock}>{tryFormatJson(log.requestBody)}</pre>
                            </div>
                          )}
                          {log.responseBody != null && (
                            <div>
                              <div style={{ fontSize: 10, fontWeight: 600, color: c.textMuted, marginBottom: 3, textTransform: 'uppercase', letterSpacing: '0.05em' }}>Response Body</div>
                              <pre style={codeBlock}>{tryFormatJson(log.responseBody)}</pre>
                            </div>
                          )}
                        </div>
                      )}
                    </div>
                  );
                })}

                {/* Result */}
                {(isRunning || result !== null || resultError !== null) && (
                  <div style={{ padding: '8px 10px' }}>
                    {isRunning && <span style={{ color: c.textMuted, fontSize: 11 }}>Executing...</span>}
                    {resultError && (
                      <>
                        <div style={{ fontSize: 10, fontWeight: 600, color: c.red, marginBottom: 3, textTransform: 'uppercase', letterSpacing: '0.05em' }}>Error</div>
                        <pre style={{ ...codeBlock, borderColor: `${c.red}44`, color: c.red }}>{resultError}</pre>
                      </>
                    )}
                    {result != null && !resultError && (
                      <>
                        <div style={{ fontSize: 10, fontWeight: 600, color: c.green, marginBottom: 3, textTransform: 'uppercase', letterSpacing: '0.05em' }}>Result</div>
                        <pre style={{ ...codeBlock, borderColor: `${c.green}44`, color: c.green }}>{result}</pre>
                      </>
                    )}
                  </div>
                )}

                {fetchLogs.length === 0 && result === null && resultError === null && !isRunning && (
                  <div style={{ padding: '20px 10px', textAlign: 'center', color: c.textFaint, fontSize: 11 }}>
                    Press Run to execute {selectedFn}()
                  </div>
                )}
              </div>
            </div>
          ) : (
            <div style={{ flex: 1, display: 'flex', alignItems: 'center', justifyContent: 'center', color: c.textFaint, fontSize: 12 }}>
              Select a function to run
            </div>
          )}
        </div>
      </div>

      {/* ════════ Env vars (below main panel) ════════ */}
      {envCount > 0 && (
        <div style={{ display: 'flex', alignItems: 'center', gap: 5, padding: '0 2px', flexWrap: 'wrap' }}>
          <span style={{ fontSize: 10, color: c.textFaint, fontFamily: mono }}>ENV</span>
          {Object.keys(envVars).map((key) => (
            <span key={key} style={{
              fontSize: 10, fontFamily: mono, color: c.textMuted,
              background: c.surface, padding: '1px 6px', borderRadius: 3,
              border: `1px solid ${c.borderSubtle}`,
              display: 'inline-flex', alignItems: 'center', gap: 3,
            }}>
              {key}
              <span onClick={() => deleteEnvVar(key)} style={{ cursor: 'pointer', color: c.textFaint, lineHeight: 1 }}>&times;</span>
            </span>
          ))}
          <input placeholder="KEY" value={newEnvKey} onChange={(e) => setNewEnvKey(e.target.value)}
            style={{ width: 60, padding: '1px 5px', borderRadius: 3, border: `1px solid ${c.borderSubtle}`, background: c.surface, color: c.text, fontFamily: mono, fontSize: 10, outline: 'none' }} />
          <input type="password" placeholder="value" value={newEnvValue} onChange={(e) => setNewEnvValue(e.target.value)}
            onKeyDown={(e) => { if (e.key === 'Enter' && newEnvKey.trim()) { setEnvVar(newEnvKey.trim(), newEnvValue); setNewEnvKey(''); setNewEnvValue(''); } }}
            style={{ width: 90, padding: '1px 5px', borderRadius: 3, border: `1px solid ${c.borderSubtle}`, background: c.surface, color: c.text, fontFamily: mono, fontSize: 10, outline: 'none' }} />
          <button disabled={!newEnvKey.trim()}
            onClick={() => { if (newEnvKey.trim()) { setEnvVar(newEnvKey.trim(), newEnvValue); setNewEnvKey(''); setNewEnvValue(''); } }}
            style={{ padding: '1px 6px', borderRadius: 3, border: 'none', fontSize: 10, fontWeight: 600, cursor: newEnvKey.trim() ? 'pointer' : 'default', background: newEnvKey.trim() ? c.green : c.textFaint, color: newEnvKey.trim() ? '#fff' : c.textMuted }}>
            +
          </button>
        </div>
      )}
      {envCount === 0 && (
        <div style={{ display: 'flex', alignItems: 'center', gap: 5, padding: '0 2px' }}>
          <span style={{ fontSize: 10, color: c.textFaint, fontFamily: mono }}>ENV</span>
          <input placeholder="KEY" value={newEnvKey} onChange={(e) => setNewEnvKey(e.target.value)}
            style={{ width: 60, padding: '1px 5px', borderRadius: 3, border: `1px solid ${c.borderSubtle}`, background: c.surface, color: c.text, fontFamily: mono, fontSize: 10, outline: 'none' }} />
          <input type="password" placeholder="value" value={newEnvValue} onChange={(e) => setNewEnvValue(e.target.value)}
            onKeyDown={(e) => { if (e.key === 'Enter' && newEnvKey.trim()) { setEnvVar(newEnvKey.trim(), newEnvValue); setNewEnvKey(''); setNewEnvValue(''); } }}
            style={{ width: 90, padding: '1px 5px', borderRadius: 3, border: `1px solid ${c.borderSubtle}`, background: c.surface, color: c.text, fontFamily: mono, fontSize: 10, outline: 'none' }} />
          <button disabled={!newEnvKey.trim()}
            onClick={() => { if (newEnvKey.trim()) { setEnvVar(newEnvKey.trim(), newEnvValue); setNewEnvKey(''); setNewEnvValue(''); } }}
            style={{ padding: '1px 6px', borderRadius: 3, border: 'none', fontSize: 10, fontWeight: 600, cursor: newEnvKey.trim() ? 'pointer' : 'default', background: newEnvKey.trim() ? c.green : c.textFaint, color: newEnvKey.trim() ? '#fff' : c.textMuted }}>
            +
          </button>
        </div>
      )}
    </div>
  );
};
