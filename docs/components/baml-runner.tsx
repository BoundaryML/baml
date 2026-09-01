'use client';

import { Play, RotateCcw } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import {
  runBaml,
  shouldWarmBaml,
  warmBaml,
  type RunnerResponse,
} from '@/lib/baml-runner/client';

type Props = {
  expected?: string;
  files: Record<string, string>;
  functionName?: string;
};

function milliseconds(value?: number) {
  if (value === undefined) return null;
  return value >= 1000 ? `${(value / 1000).toFixed(2)} s` : `${value.toFixed(0)} ms`;
}

export function BamlRunner({ expected, files, functionName = 'main' }: Props) {
  const root = useRef<HTMLDivElement>(null);
  const [state, setState] = useState<'idle' | 'running' | 'success' | 'error'>('idle');
  const [result, setResult] = useState<RunnerResponse>();

  useEffect(() => {
    if (!root.current || !shouldWarmBaml()) return;
    let cancelled = false;
    const observer = new IntersectionObserver(
      (entries) => {
        if (!entries.some((entry) => entry.isIntersecting)) return;
        observer.disconnect();
        window.setTimeout(() => {
          if (!cancelled) void warmBaml(files, functionName);
        }, 150);
      },
      { rootMargin: '240px' },
    );
    observer.observe(root.current);
    return () => {
      cancelled = true;
      observer.disconnect();
    };
  }, [files, functionName]);

  async function run() {
    setState('running');
    setResult(undefined);
    const response = await runBaml(files, functionName);
    setResult(response);
    setState(response.ok ? 'success' : 'error');
  }

  const source = files['baml_src/main.baml'] ?? Object.values(files)[0] ?? '';
  const matched = !expected || result?.output === expected;

  return (
    <div ref={root} className="baml-runner not-prose">
      <pre className="baml-runner-source" aria-label="Runnable BAML source">
        <code>{source}</code>
      </pre>
      <div className="baml-runner-toolbar">
        <button type="button" onClick={run} disabled={state === 'running'}>
          {state === 'success' || state === 'error' ? <RotateCcw /> : <Play />}
          {state === 'running' ? 'Running…' : state === 'idle' ? 'Run BAML' : 'Run again'}
        </button>
        <span>Runs locally in an isolated Web Worker. No API key or network access.</span>
      </div>
      {state !== 'idle' && (
        <div
          className={`baml-runner-result baml-runner-result--${state}`}
          aria-live="polite"
        >
          {state === 'running' ? (
            <span>Loading the version-pinned BAML runtime…</span>
          ) : result?.ok ? (
            <>
              <code>{result.output}</code>
              <span>{matched ? 'Output verified' : `Expected ${expected}`}</span>
            </>
          ) : (
            <span>{result?.error ?? 'The run failed.'}</span>
          )}
        </div>
      )}
      {result?.ok && result.timings && (
        <dl className="baml-runner-timings">
          <div><dt>Download</dt><dd>{milliseconds(result.timings.wasmDownloadMs)}</dd></div>
          <div><dt>Initialize</dt><dd>{milliseconds(result.timings.wasmInitializationMs)}</dd></div>
          <div><dt>Project</dt><dd>{milliseconds(result.timings.sessionInitializationMs)}</dd></div>
          <div><dt>Run</dt><dd>{milliseconds(result.timings.runMs)}</dd></div>
        </dl>
      )}
    </div>
  );
}
