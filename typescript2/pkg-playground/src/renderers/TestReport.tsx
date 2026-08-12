/**
 * Renders a testing.TestReport as a readable test result summary.
 */

import type { FC } from 'react';
import type { ResultRendererProps } from '../result-renderers';
import { CheckCircle2, XCircle, AlertCircle, Clock } from 'lucide-react';
import { cn } from '../lib/utils';

interface RunReportShape {
  outcome?: string;
  duration_ms?: number;
}

interface TestReportShape {
  $type?: string;
  outcome?: string;
  runs?: RunReportShape[];
}

function isTestReport(value: unknown): value is TestReportShape {
  if (value == null || typeof value !== 'object') return false;
  const o = value as Record<string, unknown>;
  return typeof o.outcome === 'string';
}

const outcomeIcon: Record<
  string,
  { icon: typeof CheckCircle2; color: string; label: string }
> = {
  pass: { icon: CheckCircle2, color: 'text-green-500', label: 'Passed' },
  fail: { icon: XCircle, color: 'text-red-500', label: 'Failed' },
  error: { icon: AlertCircle, color: 'text-orange-500', label: 'Error' },
};

function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  return `${(ms / 1000).toFixed(2)}s`;
}

export const TestReportRenderer: FC<ResultRendererProps> = ({
  value,
  displayMode,
}) => {
  if (!isTestReport(value)) {
    return (
      <pre className="whitespace-pre font-vsc-mono text-xs p-3 rounded bg-vsc-bg border border-vsc-border text-vsc-text overflow-auto max-h-[400px] m-0">
        {JSON.stringify(value, null, 2)}
      </pre>
    );
  }

  if (displayMode === 'inline') {
    return (
      <span className="font-vsc-mono text-xs text-vsc-text">
        {value.outcome} ({value.runs?.length ?? 0} runs)
      </span>
    );
  }

  const info = outcomeIcon[value.outcome ?? ''] ?? outcomeIcon.error;
  const Icon = info.icon;
  const runs = value.runs ?? [];
  const totalDuration = runs.reduce((sum, r) => sum + (r.duration_ms ?? 0), 0);

  return (
    <div className="space-y-2 p-3 rounded bg-vsc-bg border border-vsc-border">
      {/* Overall outcome */}
      <div className="flex items-center gap-2">
        <Icon size={20} className={info.color} />
        <span className={cn('text-sm font-semibold', info.color)}>
          {info.label}
        </span>
        {totalDuration > 0 && (
          <span className="flex items-center gap-1 text-xs text-vsc-text-faint ml-auto">
            <Clock size={12} />
            {formatDuration(totalDuration)}
          </span>
        )}
      </div>

      {/* Individual runs */}
      {runs.length > 0 && (
        <div className="space-y-1">
          {runs.map((run, i) => {
            const runInfo = outcomeIcon[run.outcome ?? ''] ?? outcomeIcon.error;
            const RunIcon = runInfo.icon;
            return (
              <div
                key={i}
                className="flex items-center gap-2 text-xs font-vsc-mono py-0.5 px-2 rounded bg-vsc-bg-alt"
              >
                <RunIcon size={12} className={runInfo.color} />
                <span className={cn('text-[11px]', runInfo.color)}>
                  {runInfo.label}
                </span>
                {(run.duration_ms ?? 0) > 0 && (
                  <span className="text-vsc-text-faint ml-auto">
                    {formatDuration(run.duration_ms!)}
                  </span>
                )}
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
};
