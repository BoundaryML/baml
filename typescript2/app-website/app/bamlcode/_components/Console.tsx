'use client';

import { useCallback, useState } from 'react';
import type { RunView, SolveRuntime } from '../_lib/runtime';
import type { Problem } from '../_lib/types';

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
  const [output, setOutput] = useState<
    { kind: 'value' | 'error' | 'describe'; text: string } | null
  >(null);

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
          value={call}
          spellCheck={false}
          onChange={(e) => setCall(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') run();
          }}
          placeholder={`${fnName}(...)`}
        />
        <button
          type="button"
          className="bc-btn bc-btn-secondary font-mono"
          onClick={run}
          disabled={!!busy}
        >
          {busy === 'run' ? '…' : '▶ run'}
        </button>
        <button
          type="button"
          className="bc-btn bc-btn-ghost font-mono"
          onClick={describe}
          disabled={!!busy}
          title={`baml describe ${fnName}`}
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
