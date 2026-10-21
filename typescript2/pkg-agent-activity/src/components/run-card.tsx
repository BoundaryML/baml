// What a run did: the command, whether it passed, which functions ran (and
// where they live), and the data it produced. For `baml run` the output is the
// function's return value, shown next to the code that produced it.

import { memo, type ReactNode } from 'react';
import type { Run } from '../engine/model';
import type { RunFrame } from '../engine/protocol';
import { formatDuration, splitPath, withOffsets } from './symbols';

export const RunCard = memo(function RunCard({
  run,
  active = false,
  compact = false,
  onFrame,
}: {
  run: Run;
  active?: boolean;
  compact?: boolean;
  onFrame?: (frame: RunFrame) => void;
}) {
  const duration = run.endedAt ? run.endedAt - run.startedAt : null;
  const frames = collapseFrames(run.frames ?? []);
  return (
    <section
      aria-label={run.command}
      className="aa-run"
      data-active={active}
      data-status={run.status}
    >
      <header className="aa-run-head">
        <RunStatusIcon status={run.status} />
        <code className="aa-run-command" title={run.command}>
          {run.command}
        </code>
        <span className="aa-run-time">
          {duration === null ? 'running' : formatMs(duration)}
        </span>
      </header>
      {!compact && frames.length > 0 && (
        <ol className="aa-run-frames">
          {frames.map((f, i) => (
            <li key={`${f.name}${i}`}>
              <button
                disabled={!onFrame}
                onClick={() => onFrame?.(f)}
                title={`${f.path}:${(f.line ?? 0) + 1}`}
                type="button"
              >
                <span
                  className="aa-run-depth"
                  style={{ width: f.depth * 12 }}
                />
                <span className="aa-run-fn">{f.name}</span>
                {f.count > 1 && (
                  <span className="aa-run-count">×{f.count}</span>
                )}
                <span className="aa-run-file">{splitPath(f.path).name}</span>
                {f.llm && (
                  <span className="aa-run-llm">
                    {f.llm.model ?? f.llm.client}
                  </span>
                )}
                {f.durationMs !== undefined && (
                  <span className="aa-run-ms">{formatMs(f.durationMs)}</span>
                )}
              </button>
            </li>
          ))}
        </ol>
      )}
      {!compact && run.output !== undefined && (
        <div className="aa-run-output">
          <div className="aa-run-label">Returned</div>
          <pre>{renderJson(run.output)}</pre>
        </div>
      )}
      {!compact && run.output === undefined && run.log && (
        <pre className="aa-run-log">
          {withOffsets(
            run.log
              .split('\n')
              .slice(-9)
              .map((line) => `${line}\n`),
          ).map(({ part: line, offset }) => (
            <span data-tone={toneOf(line)} key={offset}>
              {line}
            </span>
          ))}
        </pre>
      )}
      {!compact && run.error && <div className="aa-run-error">{run.error}</div>}
    </section>
  );
});

interface CollapsedFrame extends RunFrame {
  count: number;
  depth: number;
}

/** Merge consecutive calls to the same function (a loop) into one row with a count. */
function collapseFrames(frames: RunFrame[]): CollapsedFrame[] {
  const out: CollapsedFrame[] = [];
  frames.forEach((f, i) => {
    const prev = out.at(-1);
    if (prev && prev.name === f.name && prev.path === f.path) {
      prev.count++;
      prev.durationMs = (prev.durationMs ?? 0) + (f.durationMs ?? 0);
      return;
    }
    out.push({ ...f, count: 1, depth: i === 0 ? 0 : 1 });
  });
  return out;
}

export function RunStatusIcon({ status }: { status: Run['status'] }) {
  if (status === 'running')
    return (
      <span
        aria-label="running"
        className="aa-run-icon aa-spinner"
        role="img"
      />
    );
  if (status === 'passed')
    return (
      <svg
        aria-label="passed"
        className="aa-run-icon"
        height="14"
        role="img"
        viewBox="0 0 16 16"
        width="14"
      >
        <title>passed</title>
        <circle cx="8" cy="8" fill="var(--aa-add)" r="7" />
        <path
          d="M5 8.2l2 2 4-4.4"
          fill="none"
          stroke="var(--aa-raised)"
          strokeLinecap="round"
          strokeWidth="1.8"
        />
      </svg>
    );
  return (
    <svg
      aria-label="failed"
      className="aa-run-icon"
      height="14"
      role="img"
      viewBox="0 0 16 16"
      width="14"
    >
      <title>failed</title>
      <circle cx="8" cy="8" fill="var(--aa-del)" r="7" />
      <path
        d="M5.6 5.6l4.8 4.8M10.4 5.6l-4.8 4.8"
        stroke="var(--aa-raised)"
        strokeLinecap="round"
        strokeWidth="1.8"
      />
    </svg>
  );
}

function toneOf(line: string): 'bad' | 'good' | undefined {
  if (
    /\b(FAIL|failed|error|panicked|×)\b|^\s*×|→ expected/i.test(line) &&
    !/0 failed/.test(line)
  )
    return 'bad';
  if (/\b(passed|ok|✓)\b|^\s*✓/.test(line)) return 'good';
  return undefined;
}

export function formatMs(ms: number): string {
  return ms < 1000
    ? `${Math.round(ms)}ms`
    : ms < 60_000
      ? `${(ms / 1000).toFixed(1)}s`
      : formatDuration(ms);
}

/** Pretty JSON with keys, strings, and numbers colored; long arrays elided. */
function renderJson(value: unknown): ReactNode {
  const lines: ReactNode[] = [];
  const push = (indent: number, parts: ReactNode[]) =>
    lines.push(
      <span key={lines.length}>
        {'  '.repeat(indent)}
        {parts}
        {'\n'}
      </span>,
    );
  const scalar = (v: unknown): ReactNode => {
    if (v === null) return <span className="j-null">null</span>;
    if (typeof v === 'string')
      return <span className="j-str">{JSON.stringify(v)}</span>;
    if (typeof v === 'number' || typeof v === 'boolean')
      return <span className="j-num">{String(v)}</span>;
    return JSON.stringify(v);
  };
  const walk = (
    v: unknown,
    indent: number,
    prefix: ReactNode[],
    comma: string,
  ) => {
    if (Array.isArray(v)) {
      if (
        v.every((x) => typeof x !== 'object' || x === null) &&
        JSON.stringify(v).length < 60
      ) {
        push(indent, [
          ...prefix,
          '[',
          ...v.flatMap((x, i) => (i ? [', ', scalar(x)] : [scalar(x)])),
          `]${comma}`,
        ]);
        return;
      }
      push(indent, [...prefix, '[']);
      v.slice(0, 6).forEach((x, i) =>
        walk(x, indent + 1, [], i < Math.min(v.length, 6) - 1 ? ',' : ''),
      );
      if (v.length > 6)
        push(indent + 1, [
          <span className="j-null" key="more">{`… ${v.length - 6} more`}</span>,
        ]);
      push(indent, [`]${comma}`]);
    } else if (v && typeof v === 'object') {
      const entries = Object.entries(v as Record<string, unknown>);
      push(indent, [...prefix, '{']);
      entries.forEach(([k, x], i) =>
        walk(
          x,
          indent + 1,
          [
            <span className="j-key" key="k">
              {k}
            </span>,
            ': ',
          ],
          i < entries.length - 1 ? ',' : '',
        ),
      );
      push(indent, [`}${comma}`]);
    } else {
      push(indent, [...prefix, scalar(v), comma]);
    }
  };
  walk(value, 0, [], '');
  return lines;
}
