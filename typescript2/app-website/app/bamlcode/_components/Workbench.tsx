'use client';

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { CodeThemeProvider } from '@/app/learn2/_lib/code-theme';
import { graderCases } from '../_lib/harness';
import { Panel, PanelGroup, PanelResizeHandle } from 'react-resizable-panels';
import { markSolved } from '../_lib/progress';
import {
  type CaseResult,
  createSolveRuntime,
  describeExpr,
} from '../_lib/runtime';
import type { Problem } from '../_lib/types';
import { Console } from './Console';
import { ProblemEditor } from './ProblemEditor';

type Verdict = 'accepted' | 'rejected' | null;

/**
 * The interactive half of the solve page: Monaco editor + the BexVM grading
 * runtime + the run/submit results panel. Worker- and browser-only, so it is
 * always mounted via {@link WorkbenchLazy} (`ssr: false`).
 */
export function Workbench({ problem }: { problem: Problem }) {
  // Create the runtime once and keep the ref STABLE - never null it. Nulling it
  // and recreating on a later render (e.g. from setBusy) would tear down the
  // runtime an in-flight grade is using. Under StrictMode we simply
  // deactivate/reactivate around the simulated unmount; the shared worker lives
  // at module scope and is never torn down.
  const runtimeRef = useRef<ReturnType<typeof createSolveRuntime> | null>(null);
  if (runtimeRef.current === null) {
    runtimeRef.current = createSolveRuntime(problem, problem.starter);
  }
  const runtime = runtimeRef.current;

  useEffect(() => {
    runtime.activate();
    return () => runtime.deactivate();
  }, [runtime]);

  const allCases = useMemo(() => graderCases(problem), [problem]);
  const visibleCases = useMemo(
    () => allCases.filter((c) => !c.test.hidden),
    [allCases],
  );
  const hiddenCount = allCases.length - visibleCases.length;

  const [results, setResults] = useState<Map<number, CaseResult>>(new Map());
  const [busy, setBusy] = useState<'run' | 'submit' | null>(null);
  const [verdict, setVerdict] = useState<Verdict>(null);
  const [compileError, setCompileError] = useState<string | null>(null);
  const [inject, setInject] = useState<{ code: string; seq: number }>({
    code: problem.starter,
    seq: 0,
  });
  const [bottomTab, setBottomTab] = useState<'tests' | 'console'>('tests');

  const runCases = useCallback(
    async (mode: 'run' | 'submit') => {
      if (busy) return;
      setBusy(mode);
      setVerdict(null);
      setCompileError(null);
      // Reset prior results so a re-run visibly restarts (rows flip to running)
      // instead of appearing to do nothing.
      setResults(new Map());
      const cases = mode === 'run' ? visibleCases : allCases;
      try {
        const out = await runtime.grade(cases);
        setResults((prev) => {
          const next = new Map(prev);
          for (const r of out) next.set(r.index, r);
          return next;
        });
        // A solution that does not compile fails every case with the same
        // diagnostic; surface it so the user knows why, not just "error".
        const compileMsg =
          out.length > 0 &&
          out.every((r) => r.status === 'error' && r.errorMessage)
            ? (out[0].errorMessage ?? null)
            : null;
        setCompileError(compileMsg);
        if (mode === 'submit') {
          const accepted = out.every((r) => r.status === 'pass');
          setVerdict(accepted ? 'accepted' : 'rejected');
          if (accepted) markSolved(problem.slug);
        }
      } finally {
        setBusy(null);
      }
    },
    [busy, visibleCases, allCases, runtime, problem.slug],
  );

  const loadCode = useCallback((code: string) => {
    setResults(new Map());
    setVerdict(null);
    setInject((prev) => ({ code, seq: prev.seq + 1 }));
  }, []);

  const reset = useCallback(
    () => loadCode(problem.starter),
    [loadCode, problem.starter],
  );

  // E2E validation hook (opt-in via ?e2e=1): lets a headless browser load the
  // reference solution and trigger grading without simulating keystrokes.
  useEffect(() => {
    if (typeof window === 'undefined') return;
    const params = new URLSearchParams(window.location.search);
    if (params.get('e2e') !== '1') return;
    // Push code straight to the runtime too - headless Chrome can't reach the
    // Monaco CDN, so the editor instance may be absent; grading must not depend
    // on it.
    const load = (code: string) => {
      loadCode(code);
      runtime.updateSolution(code);
    };
    // biome-ignore lint/suspicious/noExplicitAny: test-only global
    (window as any).__bamlcode = {
      loadSolution: () => load(problem.solution),
      loadStarter: () => load(problem.starter),
      loadCustom: (code: string) => load(code),
      run: () => runCases('run'),
      submit: () => runCases('submit'),
      execFn: (call: string) => runtime.runCall(call),
      descFn: (fn: string) => runtime.describe(fn),
      descExpr: (expr: string) => describeExpr(expr),
    };
    return () => {
      // biome-ignore lint/suspicious/noExplicitAny: test-only global
      delete (window as any).__bamlcode;
    };
  }, [loadCode, runCases, runtime, problem.solution, problem.starter]);

  const visiblePassCount = visibleCases.filter(
    (c) => results.get(c.index)?.status === 'pass',
  ).length;

  return (
    <CodeThemeProvider value="dark">
      <PanelGroup
        direction="vertical"
        autoSaveId="bamlcode-workspace"
        className="bc-workspace"
      >
        <Panel defaultSize={60} minSize={22}>
          <div className="bc-editor">
            <ProblemEditor
              initialCode={problem.starter}
              inject={inject}
              runtime={runtime}
            />
          </div>
        </Panel>
        <PanelResizeHandle className="bc-handle bc-handle-h" />
        <Panel defaultSize={40} minSize={18}>
          <div className="bc-bottom">
            <div className="bc-actions">
          <button
            className="bc-btn bc-btn-ghost font-mono"
            disabled={!!busy}
            onClick={reset}
            type="button"
          >
            reset
          </button>
          <div className="bc-actions-right">
            <button
              className="bc-btn bc-btn-secondary font-mono"
              disabled={!!busy}
              onClick={() => runCases('run')}
              type="button"
            >
              {busy === 'run' ? 'running…' : '▶ Run'}
            </button>
            <button
              className="bc-btn bc-btn-primary font-mono"
              disabled={!!busy}
              onClick={() => runCases('submit')}
              type="button"
            >
              {busy === 'submit' ? 'grading…' : 'Submit'}
            </button>
          </div>
        </div>

        {compileError ? (
          <div className="bc-compile-error font-mono">
            <span className="bc-compile-error-label">Does not compile</span>
            {compileError}
          </div>
        ) : verdict ? (
          <div className={`bc-verdict bc-verdict-${verdict}`}>
            {verdict === 'accepted'
              ? '✓ Accepted. All tests passed.'
              : '✗ Wrong answer. Some tests failed.'}
          </div>
        ) : null}

        <div className="bc-results">
          <div className="bc-results-head font-mono">
            <div className="bc-tabs">
              <button
                type="button"
                className={`bc-tab ${bottomTab === 'tests' ? 'bc-tab-on' : ''}`}
                onClick={() => setBottomTab('tests')}
              >
                test cases
              </button>
              <button
                type="button"
                className={`bc-tab ${bottomTab === 'console' ? 'bc-tab-on' : ''}`}
                onClick={() => setBottomTab('console')}
              >
                console
              </button>
            </div>
            {bottomTab === 'tests' ? (
              <span className="bc-results-count">
                {visiblePassCount}/{visibleCases.length} passed
                {hiddenCount > 0 ? ` · +${hiddenCount} hidden` : ''}
              </span>
            ) : null}
          </div>

          {bottomTab === 'tests' ? (
            visibleCases.map((c) => {
              const r = results.get(c.index);
              const status = busy && !r ? 'running' : (r?.status ?? 'idle');
              return (
                <div
                  className={`bc-case bc-case-${status} font-mono`}
                  key={c.index}
                >
                  <span className="bc-case-icon">
                    {status === 'pass'
                      ? '✓'
                      : status === 'fail'
                        ? '✗'
                        : status === 'error'
                          ? '!'
                          : status === 'running'
                            ? '…'
                            : '○'}
                  </span>
                  <code className="bc-case-call">
                    {c.test.label ?? c.test.call}
                  </code>
                  <span className="bc-case-expect">→ {c.test.expected}</span>
                  {r?.status === 'error' && r.errorMessage ? (
                    <span className="bc-case-err">{r.errorMessage}</span>
                  ) : null}
                </div>
              );
            })
          ) : (
            <Console runtime={runtime} problem={problem} />
          )}
            </div>
          </div>
        </Panel>
      </PanelGroup>
    </CodeThemeProvider>
  );
}
