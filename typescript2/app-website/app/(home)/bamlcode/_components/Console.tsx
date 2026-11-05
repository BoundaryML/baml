'use client';

import { useCallback, useState } from 'react';
import type { RunView, SolveRuntime } from '../_lib/runtime';
import type { Problem } from '../_lib/types';

/** The function name in a signature such as `function TwoSum(...)`, or ''. */
function fnNameOf(signature: string): string {
  return signature.match(/function\s+(\w+)/)?.[1] ?? '';
}

/** Terminal-style panel exposing `baml run` and `baml describe`. */
export function Console({
  runtime,
  problem,
}: {
  runtime: SolveRuntime;
  problem: Problem;
}) {
  const fnName = fnNameOf(problem.signature);
  const firstCall =
    problem.tests.find((t) => !t.hidden)?.call ??
    problem.tests[0]?.call ??
    `${fnName}()`;

  const [call, setCall] = useState(firstCall);
  const [busy, setBusy] = useState<'run' | 'describe' | null>(null);
  const [output, setOutput] = useState<{
    kind: 'value' | 'error' | 'describe';
    text: string;
  } | null>(null);

  const run = useCallback(async () => {
    if (busy) return;
    setBusy('run');
    try {
      const res: RunView = await runtime.runCall(call);
      if (res.ok) {
        setOutput({ kind: 'value', text: JSON.stringify(res.value) });
      } else {
        setOutput({ kind: 'error', text: res.error ?? 'run failed' });
      }
    } finally {
      setBusy(null);
    }
  }, [busy, call, runtime]);

  const describe = useCallback(async () => {
    if (busy) return;
    setBusy('describe');
    try {
      const text = await runtime.describe(fnName);
      setOutput({ kind: 'describe', text });
    } finally {
      setBusy(null);
    }
  }, [busy, fnName, runtime]);

  return (
    <div className="bc-console">
      <div className="bc-console-row">
        <span className="bc-console-prompt font-mono">baml run</span>
        <input
          className="bc-console-input font-mono"
          onChange={(e) => setCall(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') run();
          }}
          placeholder={`${fnName}(...)`}
          spellCheck={false}
          value={call}
        />
        <button
          className="bc-btn bc-btn-secondary"
          disabled={!!busy}
          onClick={run}
          type="button"
        >
          {busy === 'run' ? '…' : '▶ run'}
        </button>
        <button
          className="bc-btn bc-btn-ghost"
          disabled={!!busy}
          onClick={describe}
          title={`baml describe ${fnName}`}
          type="button"
        >
          {busy === 'describe' ? '…' : 'describe'}
        </button>
      </div>

      {output ? (
        <pre className={`bc-console-out bc-console-${output.kind} font-mono`}>
          {output.kind === 'value' ? `=> ${output.text}` : output.text}
        </pre>
      ) : (
        <pre className="bc-console-out bc-console-hint font-mono">
          Run your function on any input, or describe its signature. Results
          evaluate in the browser BexVM.
        </pre>
      )}
    </div>
  );
}
