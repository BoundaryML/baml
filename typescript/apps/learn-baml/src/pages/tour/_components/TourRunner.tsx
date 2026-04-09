import React, { useCallback, useEffect, useMemo, useState } from 'react';
import { tourReplExamples } from '../_data/tourReplData';

type ActiveTab = 'prompt' | 'request' | 'output';

interface ReplRequestShape {
  method: string;
  url: string;
  headers: Record<string, string>;
  body: string;
}

interface ReplUsageShape {
  inputTokens: number | null;
  outputTokens: number | null;
  cachedInputTokens: number | null;
}

interface ReplSuccess {
  ok: true;
  promptPreview: string | null;
  request: ReplRequestShape | null;
  output: unknown;
  rawOutput: string | null;
  provider: string | null;
  clientName: string | null;
  timingMs: number | null;
  usage: ReplUsageShape | null;
  note?: string;
}

interface ReplFailure {
  ok: false;
  stage: 'validation' | 'compile' | 'request' | 'execution' | 'unknown';
  error: string;
  promptPreview: string | null;
  request: ReplRequestShape | null;
}

type ReplResult = ReplSuccess | ReplFailure;

interface TourRunnerProps {
  exampleKey: string;
}

function pretty(value: unknown): string {
  if (typeof value === 'string') {
    return value;
  }
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value);
  }
}

export default function TourRunner({ exampleKey }: TourRunnerProps) {
  const example = tourReplExamples[exampleKey];

  const [activeTab, setActiveTab] = useState<ActiveTab>('prompt');
  const [code, setCode] = useState(example?.code ?? '');
  const [argsText, setArgsText] = useState(example?.args ?? '{}');
  const [result, setResult] = useState<ReplResult | null>(null);
  const [isRunning, setIsRunning] = useState(false);
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    if (!example) {
      return;
    }
    setCode(example.code);
    setArgsText(example.args);
    setResult(null);
    setActiveTab('prompt');
    setCopied(false);
  }, [exampleKey, example]);

  const hasEdited = useMemo(() => {
    if (!example) {
      return false;
    }
    return code.trim() !== example.code.trim() || argsText.trim() !== example.args.trim();
  }, [argsText, code, example]);

  const missionComplete = Boolean(result && result.ok && hasEdited);

  const handleRun = useCallback(async () => {
    if (!example || isRunning) {
      return;
    }

    let parsedArgs: Record<string, unknown>;
    try {
      parsedArgs = JSON.parse(argsText) as Record<string, unknown>;
    } catch (error) {
      setResult({
        ok: false,
        stage: 'validation',
        error: `args.json must be valid JSON. ${(error as Error).message}`,
        promptPreview: null,
        request: null,
      });
      setActiveTab('output');
      return;
    }

    setIsRunning(true);

    try {
      const response = await fetch('/api/tour/repl', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({
          code,
          functionName: example.functionName,
          args: parsedArgs,
        }),
      });

      let payload: ReplResult;
      try {
        payload = (await response.json()) as ReplResult;
      } catch {
        payload = {
          ok: false,
          stage: 'unknown',
          error: `Unexpected response (${response.status}) from REPL endpoint.`,
          promptPreview: null,
          request: null,
        };
      }

      if (!response.ok && payload.ok) {
        setResult({
          ok: false,
          stage: 'unknown',
          error: `Request failed with status ${response.status}.`,
          promptPreview: payload.promptPreview,
          request: payload.request,
        });
        setActiveTab('output');
        return;
      }

      setResult(payload);
      if (payload.ok) {
        setActiveTab('output');
      } else if (payload.promptPreview) {
        setActiveTab('prompt');
      } else {
        setActiveTab('output');
      }
    } catch (error) {
      setResult({
        ok: false,
        stage: 'unknown',
        error: `REPL request failed: ${(error as Error).message}`,
        promptPreview: null,
        request: null,
      });
      setActiveTab('output');
    } finally {
      setIsRunning(false);
    }
  }, [argsText, code, example, isRunning]);

  const handleCopy = useCallback(() => {
    const text = `${code}\n\n// args.json\n${argsText}`;
    navigator.clipboard.writeText(text);
    setCopied(true);
    setTimeout(() => setCopied(false), 1800);
  }, [argsText, code]);

  const handleReset = useCallback(() => {
    if (!example) {
      return;
    }
    setCode(example.code);
    setArgsText(example.args);
    setResult(null);
    setActiveTab('prompt');
  }, [example]);

  const onEditorKeyDown = useCallback(
    (event: React.KeyboardEvent<HTMLTextAreaElement>) => {
      if ((event.metaKey || event.ctrlKey) && event.key === 'Enter') {
        event.preventDefault();
        void handleRun();
      }
    },
    [handleRun]
  );

  if (!example) {
    return <div className="tour-placeholder">Example not found: {exampleKey}</div>;
  }

  const promptPreview = result?.promptPreview;
  const request = result?.request;

  return (
    <div style={{ display: 'flex', flexDirection: 'column', flex: 1 }}>
      <div className="tour-runner">
        <div className="tour-panel">
          <div className="tour-panel-header" style={{ display: 'flex', justifyContent: 'space-between' }}>
            <span>Interactive REPL</span>
            <div style={{ display: 'flex', gap: '0.5rem' }}>
              <button onClick={handleReset} style={{ cursor: 'pointer' }}>Reset</button>
              <button
                onClick={handleCopy}
                style={{
                  background: 'none',
                  border: 'none',
                  cursor: 'pointer',
                  color: copied ? 'var(--ifm-color-success)' : 'inherit',
                }}
              >
                {copied ? '✓ Copied' : 'Copy'}
              </button>
            </div>
          </div>
          <div className="tour-panel-content tour-editor-stack">
            <div className="tour-editor-label">main.baml</div>
            <textarea
              className="tour-code-editor"
              value={code}
              onChange={event => setCode(event.target.value)}
              onKeyDown={onEditorKeyDown}
              spellCheck={false}
              aria-label="BAML source editor"
            />

            <div className="tour-editor-label">args.json</div>
            <textarea
              className="tour-code-editor tour-code-editor--args"
              value={argsText}
              onChange={event => setArgsText(event.target.value)}
              onKeyDown={onEditorKeyDown}
              spellCheck={false}
              aria-label="Function args JSON editor"
            />
          </div>
        </div>

        <div className="tour-divider" />

        <div className="tour-panel">
          <div className="tour-panel-header" style={{ display: 'flex', justifyContent: 'space-between' }}>
            <div style={{ display: 'flex', gap: '1rem' }}>
              <button
                onClick={() => setActiveTab('prompt')}
                style={{
                  background: 'none',
                  border: 'none',
                  cursor: 'pointer',
                  fontWeight: activeTab === 'prompt' ? 600 : 400,
                  borderBottom: activeTab === 'prompt' ? '2px solid var(--ifm-color-primary)' : 'none',
                }}
              >
                Prompt Preview
              </button>
              <button
                onClick={() => setActiveTab('request')}
                style={{
                  background: 'none',
                  border: 'none',
                  cursor: 'pointer',
                  fontWeight: activeTab === 'request' ? 600 : 400,
                  borderBottom: activeTab === 'request' ? '2px solid var(--ifm-color-primary)' : 'none',
                }}
              >
                HTTP Request
              </button>
              <button
                onClick={() => setActiveTab('output')}
                style={{
                  background: 'none',
                  border: 'none',
                  cursor: 'pointer',
                  fontWeight: activeTab === 'output' ? 600 : 400,
                  borderBottom: activeTab === 'output' ? '2px solid var(--ifm-color-primary)' : 'none',
                }}
              >
                Output
              </button>
            </div>

            <div style={{ display: 'flex', gap: '0.5rem' }}>
              <button
                onClick={() => {
                  setResult(null);
                  setActiveTab('prompt');
                }}
                style={{ cursor: 'pointer' }}
              >
                Clear
              </button>
              <button
                onClick={() => void handleRun()}
                disabled={isRunning}
                className="button button--primary button--sm"
              >
                {isRunning ? 'Running...' : '▶ Run (Cmd/Ctrl+Enter)'}
              </button>
            </div>
          </div>

          <div className="tour-panel-content">
            {!result && (
              <div className="tour-placeholder">
                Click <strong>Run</strong> to execute <code>{example.functionName}</code> with your current code and args.
              </div>
            )}

            {result && activeTab === 'prompt' && (
              promptPreview ? (
                <pre className="tour-code">{promptPreview}</pre>
              ) : (
                <div className="tour-placeholder">Prompt preview unavailable for this provider/request.</div>
              )
            )}

            {result && activeTab === 'request' && (
              request ? (
                <div className="tour-request-wrap">
                  <div className="tour-request-meta">
                    <span>{request.method} {request.url}</span>
                  </div>
                  <pre className="tour-code">{pretty(request.headers)}</pre>
                  <pre className="tour-code">{request.body}</pre>
                </div>
              ) : (
                <div className="tour-placeholder">No request available.</div>
              )
            )}

            {result && activeTab === 'output' && (
              result.ok ? (
                <div className="tour-request-wrap">
                  <div className="tour-request-meta">
                    <span>{result.clientName ?? 'unknown client'} via {result.provider ?? 'unknown provider'}</span>
                    {result.timingMs != null ? <span>{result.timingMs.toFixed(0)} ms</span> : null}
                  </div>
                  <pre className="tour-code" style={{ color: 'var(--ifm-color-success)' }}>{pretty(result.output)}</pre>
                  {result.rawOutput ? (
                    <>
                      <div className="tour-editor-label">Raw LLM Response</div>
                      <pre className="tour-code">{result.rawOutput}</pre>
                    </>
                  ) : null}
                </div>
              ) : (
                <div className="tour-error-box">
                  <strong>Run failed at {result.stage}</strong>
                  <pre className="tour-code">{result.error}</pre>
                </div>
              )
            )}
          </div>
        </div>
      </div>

      {(example.challenge || example.tryIt) && (
        <div
          className={`tour-challenge ${missionComplete ? 'tour-challenge--complete' : ''}`}
          style={{
            margin: '0 1rem 1rem',
            padding: '0.75rem 1rem',
            borderRadius: '8px',
            fontSize: '0.875rem',
            borderLeft: `3px solid ${missionComplete ? 'var(--ifm-color-success)' : 'var(--ifm-color-primary)'}`,
          }}
        >
          {example.challenge ? (
            <div>
              <strong>Mission:</strong> {example.challenge}
            </div>
          ) : null}
          {example.tryIt ? <div>{example.tryIt}</div> : null}
          {missionComplete ? <div><strong>Completed:</strong> You changed code/args and executed successfully.</div> : null}
        </div>
      )}
    </div>
  );
}
