/** biome-ignore-all lint/style/useFilenamingConvention: component filename matches its export */
/**
 * Telemetry: the local observability surface for BAML executions.
 *
 * Three views over one execution, each answering a different question:
 *
 * - **Overview**: what happened, and where the time went.
 * - **Trace**: ordered by time. Contains only spans. Work that exists only
 *   in the counts appears as a chip, never as an invented row.
 * - **Timings**: ordered by call path. Renders the calling-context tree,
 *   where retained instances are pivots back into Trace.
 *
 * ## Honesty rules, enforced in pixels
 *
 * 1. An aggregate never gets a position on a time axis.
 * 2. A count never expands into rows unless every instance was retained.
 * 3. Aggregate-derived marks are dashed or faint; span-derived marks are
 *    solid, so a reader can always tell evidence from summary.
 * 4. "Not captured by policy" and "readable but not served here" are
 *    different states with different labels, because they call for
 *    different actions.
 */

import {
  AlertCircle,
  ArrowLeft,
  ArrowRight,
  BarChart3,
  Brain,
  ChevronDown,
  ChevronRight,
  ChevronUp,
  Circle,
  Flame,
  FlaskConical,
  GitBranch,
  Info,
  Loader2,
  Play,
  Search,
  Sigma,
  Terminal,
  Waypoints,
  XCircle,
  Zap,
} from 'lucide-react';
import type { FC, KeyboardEvent as ReactKeyboardEvent, ReactNode } from 'react';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { cn } from '../lib/utils';
import type { TelemetryMedia } from '../worker-protocol';
import type {
  CallKind,
  CapturedValue,
  ContextNode,
  ErrorCapture,
  Evidence,
  ExecutionRow,
  GapInfo,
  SourceKind,
  SourceRef,
  SpanNode,
  TelemetryStatus,
  ThreadLane,
  ValueAvailability,
} from './evidence';
import {
  formatClock,
  formatCount,
  formatDuration,
  formatTimeOfDay,
  functionColor,
  LLM_PURPLE,
  statusStyles,
  summarizeDurations,
} from './format';

type ViewMode = 'overview' | 'trace' | 'timings';

// ---------------------------------------------------------------------------
// Shared atoms
// ---------------------------------------------------------------------------

const StatusIcon: FC<{ status: TelemetryStatus; className?: string }> = ({
  status,
  className,
}) => {
  if (status === 'running') {
    return <Loader2 className={cn('h-3.5 w-3.5 animate-spin', className)} />;
  }
  // Cancelled reads as a failure that was not ours: same glyph, amber.
  if (status === 'failed' || status === 'cancelled') {
    return <XCircle className={cn('h-3.5 w-3.5', className)} />;
  }
  return <Circle className={cn('h-3.5 w-3.5', className)} />;
};

const SourceIcon: FC<{ sourceKind: SourceKind; className?: string }> = ({
  sourceKind,
  className,
}) => {
  const style = cn('h-3.5 w-3.5', className);
  if (sourceKind === 'test') return <FlaskConical className={style} />;
  if (sourceKind === 'playground') return <Play className={style} />;
  if (sourceKind === 'sdk') return <Zap className={style} />;
  if (sourceKind === 'unknown') return <Circle className={style} />;
  return <Terminal className={style} />;
};

/**
 * Kind glyph. A model call gets a brain rather than a spark: reviewers read
 * the spark as "magic" or "new", where the one thing worth seeing at a
 * glance is which calls reach a model.
 */
const KindGlyph: FC<{ fn: string; kind: CallKind }> = ({ fn, kind }) => {
  if (kind === 'llm') {
    return (
      <Brain className="h-3.5 w-3.5 shrink-0" style={{ color: LLM_PURPLE }} />
    );
  }
  if (kind === 'spawn') {
    return <GitBranch className="h-3.5 w-3.5 shrink-0 text-vsc-text-muted" />;
  }
  return (
    <span
      className="h-2 w-2 shrink-0 rounded-sm"
      style={{ backgroundColor: functionColor(fn, kind) }}
    />
  );
};

const SourceLink: FC<{
  source: SourceRef;
  onOpen?: (file: string, line: number | null) => void;
}> = ({ source, onOpen }) => (
  <button
    className="mt-2 truncate font-vsc-mono text-[11px] text-vsc-accent hover:underline"
    onClick={() => onOpen?.(source.file, source.line)}
    type="button"
  >
    {source.file}
    {source.line != null && `:${source.line}`}
  </button>
);

const Pill: FC<{
  children: ReactNode;
  className?: string;
  title?: string;
}> = ({ children, className, title }) => (
  <span
    className={cn(
      'inline-flex items-center gap-1 rounded-full border border-vsc-border-subtle bg-vsc-surface px-2 py-0.5 text-[11px] text-vsc-text-muted',
      className,
    )}
    title={title}
  >
    {children}
  </span>
);

const SectionHeading: FC<{ children: ReactNode }> = ({ children }) => (
  <div className="mb-1.5 text-[11px] font-semibold uppercase tracking-wide text-vsc-text-muted">
    {children}
  </div>
);

const Panel: FC<{
  title: string;
  subtitle?: string;
  action?: ReactNode;
  children: ReactNode;
  className?: string;
}> = ({ title, subtitle, action, children, className }) => (
  <section
    className={cn(
      'overflow-hidden rounded-md border border-vsc-border bg-vsc-surface',
      className,
    )}
  >
    <div className="flex items-start border-b border-vsc-border-subtle px-3 py-2">
      <div className="min-w-0 flex-1">
        <h2 className="text-[13px] font-semibold">{title}</h2>
        {subtitle && (
          <p className="mt-0.5 truncate text-[12px] text-vsc-text-faint">
            {subtitle}
          </p>
        )}
      </div>
      {action}
    </div>
    {children}
  </section>
);

/**
 * A number with the bar that gives it scale.
 *
 * Reviewers found bare boxed numbers "overly sanitized": a figure with no
 * comparison in view cannot be judged. Every stat that has a natural
 * denominator carries its share here instead.
 */
const Stat: FC<{
  label: string;
  value: string;
  hint?: string;
  fraction?: number | null;
  tone?: string;
}> = ({ label, value, hint, fraction, tone }) => (
  <div className="rounded border border-vsc-border-subtle bg-vsc-surface px-2.5 py-2">
    <div className="text-[11px] text-vsc-text-faint" title={hint}>
      {label}
    </div>
    <div className={cn('mt-0.5 font-vsc-mono text-[14px]', tone)}>{value}</div>
    {fraction != null && (
      <div className="mt-1.5 h-1 rounded bg-vsc-bg">
        <div
          className="h-full rounded bg-vsc-accent"
          style={{ width: `${Math.min(100, Math.max(0, fraction * 100))}%` }}
        />
      </div>
    )}
  </div>
);

/** Why a value is absent, in terms of what the reader could do about it. */
function describeAbsence(availability: ValueAvailability): ReactNode {
  switch (availability.state) {
    case 'notCaptured':
      return 'not captured: the capture policy did not select this call';
    case 'lost':
      return `captured but lost before it reached the store (${availability.reason})`;
    case 'available':
      // Available with no body means hydration hit a budget: the value is in
      // the store and too large to have been decoded for this panel.
      return 'stored locally, but too large to decode for this view';
    default:
      return 'not applicable to this call';
  }
}

/**
 * One captured value: its structure, and its media when it holds any.
 *
 * Media never travels with the value -- an image arrives as
 * `{"$media":"image","bytes_len":N}` -- so the bytes are fetched only when
 * this is on screen and only for the value being looked at.
 */
const CapturedValueView: FC<{
  valueRole: string;
  value: CapturedValue;
  onLoadMedia?: (cid: string) => Promise<TelemetryMedia>;
}> = ({ valueRole, value, onLoadMedia }) => {
  const [media, setMedia] = useState<TelemetryMedia | null>(null);
  const [mediaError, setMediaError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const cid = value.mediaCid;

  useEffect(() => {
    setMedia(null);
    setMediaError(null);
    if (!cid || !onLoadMedia) return;
    let live = true;
    setLoading(true);
    onLoadMedia(cid)
      .then((loaded) => {
        if (live) setMedia(loaded);
      })
      .catch((cause: unknown) => {
        if (live) {
          setMediaError(cause instanceof Error ? cause.message : String(cause));
        }
      })
      .finally(() => {
        if (live) setLoading(false);
      });
    return () => {
      live = false;
    };
  }, [cid, onLoadMedia]);

  if (value.body == null) {
    return (
      <div>
        <SectionHeading>{valueRole}</SectionHeading>
        <div className="rounded border border-dashed border-vsc-border p-2 text-[12px] text-vsc-text-faint">
          {describeAbsence(value.availability)}
        </div>
      </div>
    );
  }

  return (
    <div>
      <SectionHeading>{valueRole}</SectionHeading>
      {cid && (
        <div className="mb-1.5">
          {media?.base64 && media.kind === 'image' ? (
            <img
              alt={valueRole}
              className="max-h-72 w-auto rounded border border-vsc-border-subtle"
              src={`data:${media.mime};base64,${media.base64}`}
            />
          ) : media?.url ? (
            <a
              className="text-[12px] text-vsc-accent hover:underline"
              href={media.url}
              rel="noreferrer"
              target="_blank"
            >
              {media.url}
            </a>
          ) : loading ? (
            <div className="flex items-center gap-1.5 text-[12px] text-vsc-text-faint">
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
              loading media
            </div>
          ) : mediaError ? (
            <div className="rounded border border-dashed border-vsc-border p-2 text-[12px] text-vsc-text-faint">
              media could not be read: {mediaError}
            </div>
          ) : null}
        </div>
      )}
      <pre className="max-h-64 overflow-auto whitespace-pre-wrap rounded border border-vsc-border-subtle bg-vsc-surface p-2 font-vsc-mono text-[11px] leading-4 text-vsc-text">
        {typeof value.body === 'string'
          ? value.body
          : JSON.stringify(value.body, null, 2)}
      </pre>
    </div>
  );
};

// ---------------------------------------------------------------------------
// Shell
// ---------------------------------------------------------------------------

export interface TelemetryViewProps {
  /** Executions in this project's profile store, newest first. */
  executions: ExecutionRow[];
  /** Evidence for the open execution; null while it loads. */
  evidence: Evidence | null;
  selectedId: string | null;
  onSelect: (executionId: string | null) => void;
  loading?: boolean;
  /** Set when the project has never been run under the profiler. */
  storeMissing?: boolean;
  error?: string | null;
  onRefresh?: () => void;
  /** Signatures the project reports, keyed by function name. */
  signatures?: ReadonlyMap<string, string>;
  onOpenSource?: (file: string, line: number | null) => void;
  /** Fetches a captured value's media bytes by content id. */
  onLoadMedia?: (cid: string) => Promise<TelemetryMedia>;
}

export const TelemetryView: FC<TelemetryViewProps> = ({
  executions,
  evidence,
  selectedId,
  onSelect,
  loading,
  storeMissing,
  error,
  onRefresh,
  signatures,
  onOpenSource,
  onLoadMedia,
}) => {
  const selected = executions.find((row) => row.id === selectedId) ?? null;
  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-hidden bg-vsc-bg font-vsc text-vsc-text">
      {selected ? (
        <ExecutionDetail
          error={error}
          evidence={evidence}
          execution={selected}
          loading={loading}
          onBack={() => onSelect(null)}
          onLoadMedia={onLoadMedia}
          onOpenSource={onOpenSource}
          signatures={signatures}
        />
      ) : (
        <ExecutionsList
          error={error}
          executions={executions}
          loading={loading}
          onOpen={(row) => onSelect(row.id)}
          onRefresh={onRefresh}
          storeMissing={storeMissing}
        />
      )}
    </div>
  );
};

// ---------------------------------------------------------------------------
// Executions
// ---------------------------------------------------------------------------

const SOURCE_FILTERS: Array<{ key: SourceKind; label: string }> = [
  // The CLI entry point is `baml run`, so the filter says what was typed.
  { key: 'cli', label: 'Runs' },
  { key: 'playground', label: 'Playground' },
  { key: 'test', label: 'Tests' },
  { key: 'sdk', label: 'SDK' },
];

const ExecutionsList: FC<{
  executions: ExecutionRow[];
  loading?: boolean;
  storeMissing?: boolean;
  error?: string | null;
  onOpen: (row: ExecutionRow) => void;
  onRefresh?: () => void;
}> = ({ executions, loading, storeMissing, error, onOpen, onRefresh }) => {
  const [search, setSearch] = useState('');
  const [sourceFilter, setSourceFilter] = useState<SourceKind | 'all'>('all');
  const [failedOnly, setFailedOnly] = useState(false);

  // Only offer a filter that something can match. Origin is not recorded
  // for most executions, so a fixed row of buttons would be mostly dead
  // controls; these appear as the data gains the ability to fill them.
  const availableFilters = useMemo(() => {
    const present = new Set(executions.map((row) => row.sourceKind));
    return SOURCE_FILTERS.filter((filter) => present.has(filter.key));
  }, [executions]);

  const visible = useMemo(() => {
    const query = search.trim().toLowerCase();
    return executions.filter((row) => {
      if (sourceFilter !== 'all' && row.sourceKind !== sourceFilter) {
        return false;
      }
      // Failed means the execution itself failed. A run that succeeded while
      // handling errors is not a failure; it gets a badge, not this filter.
      if (failedOnly && row.status !== 'failed') return false;
      if (!query) return true;
      return `${row.target} ${row.entryPoint}`.toLowerCase().includes(query);
    });
  }, [executions, failedOnly, search, sourceFilter]);

  return (
    <main className="min-h-0 flex-1 overflow-auto">
      <div className="w-full px-3 py-3">
        <div className="flex flex-wrap items-end justify-between gap-3">
          <div>
            <h1 className="text-[16px] font-semibold text-vsc-text-bright">
              Executions
            </h1>
            <p className="mt-0.5 text-[12px] text-vsc-text-muted">
              Every recorded entry point, newest first. Each row is one
              execution.
            </p>
          </div>
          <div className="flex items-center gap-3">
            <div className="relative w-56 max-w-[30vw]">
              <Search className="absolute left-2 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-vsc-text-faint" />
              <input
                aria-label="Search executions"
                className="h-7 w-full rounded border border-vsc-input-border bg-vsc-input-bg pl-7 pr-2 text-[12px] text-vsc-input-fg outline-none placeholder:text-vsc-text-faint focus:border-vsc-accent"
                onChange={(event) => setSearch(event.target.value)}
                placeholder="Search functions, commands"
                value={search}
              />
            </div>
            {availableFilters.length > 0 && (
              <div className="flex items-center rounded border border-vsc-border p-0.5">
                {[
                  { key: 'all' as const, label: 'All' },
                  ...availableFilters,
                ].map(({ key, label }) => (
                  <button
                    className={cn(
                      'rounded px-2 py-1 text-[11px]',
                      sourceFilter === key
                        ? 'bg-vsc-accent/15 font-medium text-vsc-accent'
                        : 'text-vsc-text-muted hover:text-vsc-text',
                    )}
                    key={key}
                    onClick={() => setSourceFilter(key)}
                    type="button"
                  >
                    {label}
                  </button>
                ))}
              </div>
            )}
            <label className="flex cursor-pointer items-center gap-1.5 text-[11px] text-vsc-text-muted">
              <input
                checked={failedOnly}
                className="accent-vsc-accent"
                onChange={(event) => setFailedOnly(event.target.checked)}
                type="checkbox"
              />
              failed only
            </label>
            {onRefresh && (
              <button
                className="rounded px-2 py-1 text-[11px] text-vsc-text-muted hover:bg-vsc-hover hover:text-vsc-text"
                onClick={onRefresh}
                type="button"
              >
                Refresh
              </button>
            )}
          </div>
        </div>

        {error && (
          <div className="mt-3 rounded border border-vsc-red/25 bg-vsc-red/5 px-3 py-2 text-[12px] text-vsc-red">
            {error}
          </div>
        )}

        <div className="mt-3 overflow-hidden rounded-md border border-vsc-border bg-vsc-surface">
          <table className="w-full border-collapse text-left">
            <thead className="border-b border-vsc-border bg-vsc-bg text-[11px] font-semibold uppercase tracking-wide text-vsc-text-faint">
              <tr>
                <th className="w-[150px] px-3 py-2" scope="col">
                  Status
                </th>
                <th className="px-3 py-2" scope="col">
                  Execution
                </th>
                <th className="w-[120px] px-3 py-2" scope="col">
                  Started
                </th>
                <th className="w-[100px] px-3 py-2 text-right" scope="col">
                  Duration
                </th>
                <th className="w-[130px] px-3 py-2 text-right" scope="col">
                  Calls
                </th>
                <th className="w-[80px] px-3 py-2" scope="col">
                  <span className="sr-only">Open</span>
                </th>
              </tr>
            </thead>
            <tbody className="divide-y divide-vsc-border-subtle">
              {visible.map((row) => (
                <ExecutionRowView key={row.id} onOpen={onOpen} row={row} />
              ))}
            </tbody>
          </table>
          {visible.length === 0 && (
            <div className="px-4 py-10 text-center text-[12px] text-vsc-text-muted">
              {loading
                ? 'Reading the profile store'
                : storeMissing
                  ? 'Nothing has run in this project yet. The profiler writes .baml/profiles-v1 on the first run.'
                  : executions.length === 0
                    ? 'No executions recorded yet.'
                    : 'No executions match the current search and filters.'}
            </div>
          )}
        </div>
      </div>
    </main>
  );
};

const ExecutionRowView: FC<{
  row: ExecutionRow;
  onOpen: (row: ExecutionRow) => void;
}> = ({ row, onOpen }) => {
  // `errors` is the population count of errored CALLS, not of distinct
  // failures: one throw marks every frame it unwinds through, so a single
  // retried request shows up as five. Calling that "caught" also claimed an
  // interpretation the number does not carry.
  const erroredCalls =
    row.status !== 'failed' && (row.errors ?? 0) > 0 ? row.errors : null;
  const tracedPct =
    row.calls != null && row.calls > 0 && row.spanCount != null
      ? Math.round((row.spanCount / row.calls) * 100)
      : null;
  return (
    <tr
      className="group cursor-pointer hover:bg-vsc-hover"
      {...tableRowActivation(() => onOpen(row))}
    >
      <td className="px-3 py-2.5">
        <div className="flex flex-wrap items-center gap-1">
          <span
            className={cn(
              'inline-flex items-center gap-1.5 rounded border px-1.5 py-0.5 text-[11px] font-semibold uppercase tracking-wide',
              statusStyles(row.status),
            )}
          >
            <StatusIcon status={row.status} />
            {row.status}
          </span>
          {erroredCalls != null && (
            <span
              className="rounded border border-vsc-yellow/25 bg-vsc-yellow/10 px-1.5 py-0.5 text-[11px] font-medium text-vsc-yellow"
              title={`${erroredCalls} calls ended in an error and the execution still succeeded, so something handled them. One throw marks every frame it unwinds through, so this counts frames rather than distinct failures.`}
            >
              {erroredCalls} errored {erroredCalls === 1 ? 'call' : 'calls'}
            </span>
          )}
          {!row.indexComplete && (
            <span
              className="rounded border border-vsc-yellow/25 bg-vsc-yellow/10 px-1.5 py-0.5 text-[11px] font-medium text-vsc-yellow"
              title="Records were lost for this execution, so its counts are a floor rather than a total"
            >
              partial
            </span>
          )}
        </div>
      </td>
      <td className="min-w-0 px-3 py-2.5">
        <span className="block max-w-full text-left">
          <span className="block truncate font-vsc-mono text-[13px] font-semibold text-vsc-text-bright group-hover:text-vsc-accent">
            {row.target}
          </span>
          <span className="mt-0.5 flex items-center gap-1.5 truncate text-[11px] text-vsc-text-faint">
            <SourceIcon sourceKind={row.sourceKind} />
            <span
              className="truncate font-vsc-mono"
              title={
                row.entryPointIsIdentity
                  ? 'Identity of the source snapshot that ran. The store does not record the command that started it.'
                  : row.entryPoint
              }
            >
              {row.entryPoint}
            </span>
            {row.revision && (
              <>
                <span>·</span>
                <span className="truncate font-vsc-mono">{row.revision}</span>
              </>
            )}
          </span>
        </span>
      </td>
      <td className="px-3 py-2.5 font-vsc-mono text-[12px] text-vsc-text-muted">
        {row.startedMs == null ? '' : formatTimeOfDay(row.startedMs)}
      </td>
      <td className="px-3 py-2.5 text-right font-vsc-mono text-[12px] text-vsc-text-muted">
        {formatDuration(row.durationMs)}
      </td>
      <td className="px-3 py-2.5 text-right font-vsc-mono text-[12px] text-vsc-text-muted">
        {formatCount(row.calls)}
        {tracedPct != null && (
          <span
            className="ml-1.5 text-[11px] text-vsc-text-faint"
            title={`${row.spanCount?.toLocaleString()} of ${row.calls?.toLocaleString()} calls were retained as spans; the rest exist in the counts only`}
          >
            {tracedPct}% traced
          </span>
        )}
      </td>
      <td className="px-3 py-2.5 text-right">
        <button
          aria-label={`Open ${row.target}`}
          className="inline-flex items-center gap-1 rounded px-2 py-1 text-[12px] font-medium text-vsc-accent hover:bg-vsc-accent/10"
          onClick={(event) => {
            stopRowActivation(event);
            onOpen(row);
          }}
          type="button"
        >
          Open <ChevronRight className="h-3.5 w-3.5" />
        </button>
      </td>
    </tr>
  );
};

// ---------------------------------------------------------------------------
// Detail
// ---------------------------------------------------------------------------

const ExecutionDetail: FC<{
  execution: ExecutionRow;
  evidence: Evidence | null;
  loading?: boolean;
  error?: string | null;
  onBack: () => void;
  signatures?: ReadonlyMap<string, string>;
  onOpenSource?: (file: string, line: number | null) => void;
  onLoadMedia?: (cid: string) => Promise<TelemetryMedia>;
}> = ({
  execution,
  evidence,
  loading,
  error,
  onBack,
  signatures,
  onOpenSource,
  onLoadMedia,
}) => {
  const [mode, setMode] = useState<ViewMode>('overview');
  const [selectedSpanId, setSelectedSpanId] = useState<string | null>(null);
  const [selectedContextId, setSelectedContextId] = useState<string | null>(
    null,
  );

  // The two pivots. Opening a span keeps its context selected and the other
  // way round, so switching views preserves what the reader was looking at.
  const openSpan = useCallback((span: SpanNode) => {
    setSelectedSpanId(span.id);
    if (span.contextId) setSelectedContextId(span.contextId);
    setMode('trace');
  }, []);
  const openContext = useCallback((contextId: string) => {
    setSelectedContextId(contextId);
    setMode('timings');
  }, []);

  return (
    <main className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
      <div className="shrink-0 border-b border-vsc-border bg-vsc-bg">
        <div className="flex items-center gap-2 px-3 pb-1 pt-2">
          <button
            aria-label="Back to executions"
            className="mr-1 inline-flex h-6 items-center gap-1 rounded px-1.5 text-[12px] text-vsc-text-muted hover:bg-vsc-hover hover:text-vsc-text"
            onClick={onBack}
            type="button"
          >
            <ArrowLeft className="h-3.5 w-3.5" />
            Executions
          </button>
          <span className="h-4 w-px bg-vsc-border" />
          <StatusIcon
            className={statusStyles(execution.status).split(' ')[0]}
            status={execution.status}
          />
          <h1 className="truncate font-vsc-mono text-[14px] font-semibold">
            {execution.target}
          </h1>
          {/* Reviewers could not tell whether they were looking at one call
              or every call of this function. This says which. */}
          <span className="truncate text-[12px] text-vsc-text-faint">
            one execution
            {execution.startedMs != null &&
              `, started ${formatTimeOfDay(execution.startedMs)}`}
          </span>
        </div>
        <nav className="flex h-8 items-end gap-1 px-3">
          {(
            [
              [
                'overview',
                BarChart3,
                'Overview',
                'What happened and where the time went',
              ],
              ['trace', Waypoints, 'Trace', 'Retained calls, ordered by time'],
              ['timings', Flame, 'Timings', 'Every call grouped by call path'],
            ] as const
          ).map(([value, Icon, label, hint]) => (
            <button
              className={cn(
                'flex h-8 items-center gap-1.5 border-b-2 px-2 text-[13px]',
                mode === value
                  ? 'border-vsc-accent text-vsc-text'
                  : 'border-transparent text-vsc-text-muted hover:text-vsc-text',
              )}
              key={value}
              onClick={() => setMode(value)}
              title={hint}
              type="button"
            >
              <Icon className="h-3.5 w-3.5" />
              {label}
            </button>
          ))}
        </nav>
      </div>

      <div className="flex min-h-0 min-w-0 flex-1 overflow-hidden">
        <div className="min-h-0 min-w-0 flex-1 overflow-auto">
          {error ? (
            <div className="m-3 rounded border border-vsc-red/25 bg-vsc-red/5 p-3 text-[12px] text-vsc-red">
              {error}
            </div>
          ) : execution.status === 'running' ? (
            <RunningExecution execution={execution} />
          ) : !evidence ? (
            <div className="flex min-h-[320px] items-center justify-center p-6">
              <div className="flex items-center gap-2 text-[13px] text-vsc-text-muted">
                {loading && (
                  <Loader2 className="h-4 w-4 animate-spin text-vsc-accent" />
                )}
                Reading local evidence for {execution.target}
              </div>
            </div>
          ) : mode === 'overview' ? (
            <OverviewTab
              evidence={evidence}
              execution={execution}
              openContext={openContext}
              openSpan={openSpan}
            />
          ) : mode === 'trace' ? (
            <TraceTab
              evidence={evidence}
              onLoadMedia={onLoadMedia}
              onOpenSource={onOpenSource}
              openContext={openContext}
              selectedSpanId={selectedSpanId}
              setSelectedSpanId={setSelectedSpanId}
              signatures={signatures}
              startedMs={execution.startedMs}
            />
          ) : (
            <TimingsTab
              evidence={evidence}
              onOpenSource={onOpenSource}
              openSpan={openSpan}
              selectedContextId={selectedContextId}
              setSelectedContextId={setSelectedContextId}
              signatures={signatures}
            />
          )}
        </div>
      </div>
    </main>
  );
};

/**
 * A running execution, which has no evidence to show yet.
 *
 * The profiler publishes an execution's threads, calling contexts, spans,
 * and errors when its root returns. Until then the store holds the fact that
 * it started and nothing else: every counter reads zero and every relation
 * is empty. Rendering the normal panels over that shows a screen of blanks
 * and "not recorded", which reads as a broken or damaged run rather than one
 * that has not finished.
 */
const RunningExecution: FC<{ execution: ExecutionRow }> = ({ execution }) => (
  <div className="flex min-h-[320px] items-start justify-center p-6">
    <div className="w-full max-w-xl rounded-md border border-vsc-border bg-vsc-surface p-5">
      <div className="flex items-center gap-2">
        <Loader2 className="h-4 w-4 animate-spin text-vsc-accent" />
        <h2 className="text-[14px] font-semibold text-vsc-text-bright">
          This execution is still running
        </h2>
      </div>
      <p className="mt-2 text-[12px] leading-5 text-vsc-text-muted">
        Its telemetry is published when the run finishes, so there is nothing to
        show yet. This view refreshes on its own and will fill in as soon as the
        execution completes.
      </p>
      <div className="mt-3 grid grid-cols-2 gap-2">
        <Stat
          label="Started"
          value={
            execution.startedMs == null
              ? 'unknown'
              : formatTimeOfDay(execution.startedMs)
          }
        />
        <Stat label="Entry point" value={execution.entryPoint} />
      </div>
    </div>
  </div>
);

// ---------------------------------------------------------------------------
// Overview
// ---------------------------------------------------------------------------

const OverviewTab: FC<{
  execution: ExecutionRow;
  evidence: Evidence;
  openContext: (contextId: string) => void;
  openSpan: (span: SpanNode) => void;
}> = ({ execution, evidence, openContext, openSpan }) => {
  const rootTotal = Math.max(
    1,
    ...evidence.contexts
      .filter((context) => context.parentId == null)
      .map((context) => context.totalMs),
  );
  const hotContexts = evidence.contexts
    .filter((context) => !context.folded && context.parentId != null)
    .sort((left, right) => right.selfMs - left.selfMs)
    .slice(0, 5);

  // The recorded error captures are the truth about what went wrong. An
  // earlier version walked failing spans instead, which only ever saw the
  // calls capture policy happened to retain and could not name the error at
  // all. Captures carry the throw site, the whole stack, and the value.
  const errors = evidence.errors;
  const erroredSpans = evidence.spans.filter(
    (span) => span.status === 'failed',
  );
  // Call paths that recorded errors. When nothing errored was retained,
  // these are the only account of what failed: the counts cover every call,
  // so a path with errors names where they happened even with no span.
  const erroredContexts = useMemo(
    () =>
      evidence.contexts
        .filter((context) => context.errors > 0)
        .sort((left, right) => right.errors - left.errors),
    [evidence.contexts],
  );

  // `execution.errors` counts errored CALLS, not errors: one throw marks
  // every frame it unwinds through. Reporting that number as "errors" turns
  // one bug into eight, and subtracting one for the failure is not a fix.
  const totalErrors = execution.errors ?? 0;

  const wall = evidence.durationMs;
  const accounted = (evidence.cpuMs ?? 0) + (evidence.awaitMs ?? 0);

  return (
    <div className="space-y-3 px-3 py-3">
      {errors.length > 0 && (
        <ErrorsPanel
          erroredCalls={erroredSpans.length || (execution.errors ?? 0)}
          errors={errors}
          failed={execution.status === 'failed'}
          onOpenSpan={(callId) => {
            const span = evidence.spans.find((other) => other.id === callId);
            if (span) openSpan(span);
          }}
          spanIds={new Set(evidence.spans.map((span) => span.id))}
          target={execution.target}
        />
      )}

      {errors.length === 0 && execution.status === 'failed' && (
        <section className="rounded-md border border-vsc-red/30 bg-vsc-red/5 px-3 py-2.5">
          <div className="flex items-start gap-2">
            <XCircle className="mt-0.5 h-4 w-4 shrink-0 text-vsc-red" />
            <div className="min-w-0">
              <div className="text-[11px] font-semibold uppercase tracking-wide text-vsc-red">
                Execution failed
              </div>
              <div className="mt-0.5 font-vsc-mono text-[15px] font-semibold text-vsc-text-bright">
                {execution.target}
              </div>
              {/* The run failed but no capture reached the store, which is a
                  different situation from a failure we can explain. */}
              <div className="mt-1 text-[12px] text-vsc-text-muted">
                No error was captured for this execution, so there is nothing
                here to say what went wrong.
              </div>
            </div>
          </div>
        </section>
      )}

      {!evidence.indexComplete && (
        <div className="flex items-start gap-2 rounded border border-vsc-yellow/25 bg-vsc-yellow/5 px-3 py-2 text-[12px] text-vsc-yellow">
          <AlertCircle className="mt-0.5 h-3.5 w-3.5 shrink-0" />
          Records were lost for this execution, so every count below is a floor
          rather than a total.
        </div>
      )}

      <div className="grid grid-cols-2 gap-2 lg:grid-cols-4">
        <Stat
          hint="Elapsed time from the first thread starting to the execution sealing"
          label="Wall time"
          value={formatDuration(wall) || 'unknown'}
        />
        <Stat
          fraction={
            wall && evidence.cpuMs != null ? evidence.cpuMs / wall : null
          }
          hint="Time attributed to running code, summed over every call path. Self, waiting, and child time are disjoint, so nothing is counted twice."
          label="Running"
          value={
            evidence.cpuMs == null
              ? 'not recorded'
              : formatDuration(evidence.cpuMs)
          }
        />
        <Stat
          fraction={
            wall && evidence.awaitMs != null ? evidence.awaitMs / wall : null
          }
          hint="Time calls spent suspended, waiting on IO or another task"
          label="Waiting on IO"
          value={
            evidence.awaitMs == null
              ? 'not recorded'
              : formatDuration(evidence.awaitMs)
          }
        />
        <Stat
          hint={`Distinct errors captured. One throw unwinds through every frame above it, which is why ${totalErrors.toLocaleString()} calls are marked errored.`}
          label="Errors"
          tone={errors.length > 0 ? 'text-vsc-red' : undefined}
          value={
            errors.length > 0
              ? `${errors.length}`
              : totalErrors > 0
                ? 'none captured'
                : '0'
          }
        />
      </div>

      {wall != null && accounted > wall * 1.05 && (
        <p className="text-[11px] text-vsc-text-faint">
          Running and waiting time add up to more than wall time because work
          ran on {evidence.threads.length} threads at once.
        </p>
      )}

      <div className="grid gap-3 lg:grid-cols-[minmax(0,1.4fr)_minmax(280px,0.9fr)]">
        <Panel
          subtitle="self time by calling context, opens Timings"
          title="Where time went"
        >
          <div className="divide-y divide-vsc-border-subtle">
            {hotContexts.map((context) => {
              const share = Math.min(
                100,
                Math.round((context.selfMs / rootTotal) * 100),
              );
              return (
                <button
                  className="grid w-full grid-cols-[minmax(160px,1fr)_130px_80px] items-center gap-3 px-3 py-2 text-left hover:bg-vsc-hover"
                  key={context.id}
                  onClick={() => openContext(context.id)}
                  type="button"
                >
                  <div className="flex min-w-0 items-center gap-1.5">
                    <KindGlyph fn={context.fn} kind={context.kind} />
                    <div className="min-w-0">
                      <div className="truncate font-vsc-mono text-[13px] font-medium">
                        {context.fn}
                      </div>
                      <div className="truncate text-[11px] text-vsc-text-faint">
                        {context.enters.toLocaleString()} calls
                        {context.errors > 0 &&
                          `, ${context.errors.toLocaleString()} errors`}
                      </div>
                    </div>
                  </div>
                  <div>
                    <div className="mb-1 flex justify-between font-vsc-mono text-[12px]">
                      <span>{formatDuration(context.selfMs)}</span>
                      <span className="text-vsc-text-faint">{share}%</span>
                    </div>
                    <div className="h-1 rounded bg-vsc-bg">
                      <div
                        className="h-full rounded bg-vsc-accent"
                        style={{ width: `${share}%` }}
                      />
                    </div>
                  </div>
                  <span className="text-right text-[12px] text-vsc-accent">
                    Timings <ArrowRight className="ml-0.5 inline h-3 w-3" />
                  </span>
                </button>
              );
            })}
            {hotContexts.length === 0 && (
              <div className="px-3 py-6 text-center text-[12px] text-vsc-text-faint">
                No calling contexts recorded.
              </div>
            )}
          </div>
        </Panel>

        <Panel
          subtitle={
            erroredSpans.length > 0
              ? 'retained calls the throw unwound through, opens Trace'
              : 'call paths that recorded errors, opens Timings'
          }
          title="Errored calls"
        >
          {erroredSpans.length > 0 ? (
            <div className="max-h-64 divide-y divide-vsc-border-subtle overflow-auto">
              {erroredSpans.map((span) => (
                <button
                  className="flex w-full items-center gap-2 px-3 py-2 text-left hover:bg-vsc-hover"
                  key={span.id}
                  onClick={() => openSpan(span)}
                  type="button"
                >
                  <XCircle className="h-3.5 w-3.5 shrink-0 text-vsc-red" />
                  <span className="truncate font-vsc-mono text-[12px]">
                    {span.fn}
                  </span>
                  <span className="ml-auto shrink-0 font-vsc-mono text-[11px] text-vsc-text-faint">
                    +{formatDuration(span.startMs)}
                  </span>
                </button>
              ))}
            </div>
          ) : erroredContexts.length > 0 ? (
            <div className="max-h-64 overflow-auto">
              <p className="border-b border-vsc-border-subtle px-3 py-2 text-[11px] text-vsc-text-faint">
                No errored call was individually retained, so there is no span
                to open. The counts still say where the errors happened.
              </p>
              {erroredContexts.map((context) => (
                <button
                  className="flex w-full items-center gap-2 px-3 py-2 text-left hover:bg-vsc-hover"
                  key={context.id}
                  onClick={() => openContext(context.id)}
                  type="button"
                >
                  <XCircle className="h-3.5 w-3.5 shrink-0 text-vsc-red" />
                  <span className="truncate font-vsc-mono text-[12px]">
                    {context.fn}
                  </span>
                  <span className="ml-auto shrink-0 font-vsc-mono text-[11px] text-vsc-text-faint">
                    {context.errors} of {context.enters.toLocaleString()}
                  </span>
                </button>
              ))}
            </div>
          ) : (
            <div className="px-3 py-6 text-center text-[12px] text-vsc-text-faint">
              {totalErrors > 0
                ? 'Calls errored, but nothing recorded where.'
                : 'No call errored.'}
            </div>
          )}
        </Panel>
      </div>

      {evidence.threads.length > 1 && (
        <ThreadsPanel
          threads={evidence.threads}
          wallMs={Math.max(1, wall ?? rootTotal)}
        />
      )}
    </div>
  );
};

/**
 * What went wrong, from the recorded error captures.
 *
 * Two counts are deliberately kept apart. One throw unwinds through every
 * frame above it, so a single bug commonly marks a dozen calls as errored;
 * reporting that as "12 errors" makes one problem look like twelve. The
 * capture count is the number of things that went wrong, and the errored-call
 * count is how far each one travelled.
 */
const ErrorsPanel: FC<{
  errors: ErrorCapture[];
  erroredCalls: number;
  failed: boolean;
  target: string;
  spanIds: ReadonlySet<string>;
  onOpenSpan: (callId: string) => void;
}> = ({ errors, erroredCalls, failed, target, spanIds, onOpenSpan }) => (
  <section
    className={cn(
      'overflow-hidden rounded-md border',
      failed
        ? 'border-vsc-red/30 bg-vsc-red/5'
        : 'border-vsc-yellow/25 bg-vsc-yellow/5',
    )}
  >
    <div className="flex items-start gap-2 px-3 py-2.5">
      <XCircle
        className={cn(
          'mt-0.5 h-4 w-4 shrink-0',
          failed ? 'text-vsc-red' : 'text-vsc-yellow',
        )}
      />
      <div className="min-w-0 flex-1">
        <div
          className={cn(
            'text-[11px] font-semibold uppercase tracking-wide',
            failed ? 'text-vsc-red' : 'text-vsc-yellow',
          )}
        >
          {failed ? 'Execution failed' : 'Errors were raised and handled'}
        </div>
        <div className="mt-0.5 truncate font-vsc-mono text-[15px] font-semibold text-vsc-text-bright">
          {target}
        </div>
        <div className="mt-0.5 text-[12px] text-vsc-text-muted">
          {errors.length === 1 ? '1 error' : `${errors.length} errors`}
          {erroredCalls > 0 &&
            `, which failed ${erroredCalls === 1 ? '1 call' : `${erroredCalls} calls`} on the way up`}
        </div>
      </div>
    </div>
    <div className="divide-y divide-vsc-red/15 border-t border-vsc-red/15">
      {errors.map((error) => (
        <ErrorCaptureView
          error={error}
          key={error.id}
          onOpenSpan={onOpenSpan}
          spanIds={spanIds}
        />
      ))}
    </div>
  </section>
);

/** A frame outside the user's own code: builtin runtime or a provider. */
function isBuiltinFrame(fqn: string): boolean {
  return !fqn.startsWith('user.');
}

const ErrorCaptureView: FC<{
  error: ErrorCapture;
  spanIds: ReadonlySet<string>;
  onOpenSpan: (callId: string) => void;
}> = ({ error, spanIds, onOpenSpan }) => {
  const [showAllFrames, setShowAllFrames] = useState(false);
  // The user's own frames are what they can act on; the runtime and provider
  // frames below them are usually noise, so they collapse by default.
  const userFrames = error.stack.filter((frame) => !isBuiltinFrame(frame));
  const hiddenCount = error.stack.length - userFrames.length;
  const frames =
    showAllFrames || userFrames.length === 0 ? error.stack : userFrames;
  const canOpen = error.callId != null && spanIds.has(error.callId);

  return (
    <div className="px-3 py-2.5">
      <div className="flex flex-wrap items-center gap-1.5">
        <span className="font-vsc-mono text-[13px] font-semibold text-vsc-text-bright">
          {error.fn ?? 'unknown function'}
        </span>
        {error.source_location && (
          <span className="font-vsc-mono text-[11px] text-vsc-text-faint">
            {error.source_location.file}
            {error.source_location.line != null &&
              `:${error.source_location.line}`}
          </span>
        )}
        {error.kind === 'rethrow' && (
          <Pill title="Passed along from an earlier throw rather than raised here">
            rethrow
          </Pill>
        )}
        {canOpen && (
          <button
            className="ml-auto shrink-0 text-[12px] text-vsc-accent hover:underline"
            onClick={() => onOpenSpan(error.callId as string)}
            type="button"
          >
            Trace <ArrowRight className="ml-0.5 inline h-3 w-3" />
          </button>
        )}
      </div>

      {error.value != null ? (
        <pre className="mt-2 max-h-64 overflow-auto whitespace-pre-wrap rounded border border-vsc-red/20 bg-vsc-bg p-2 font-vsc-mono text-[11px] leading-4 text-vsc-text">
          {typeof error.value === 'string'
            ? error.value
            : JSON.stringify(error.value, null, 2)}
        </pre>
      ) : (
        <div className="mt-2 rounded border border-dashed border-vsc-border p-2 text-[12px] text-vsc-text-faint">
          {error.valueUnavailable?.state === 'lost'
            ? `The error value was captured but lost before it reached the store (${error.valueUnavailable.reason}).`
            : 'No error value was captured for this throw.'}
        </div>
      )}

      <div className="mt-2">
        <SectionHeading>
          Stack, root to throw
          {!error.stackComplete && ' (incomplete)'}
        </SectionHeading>
        <div className="space-y-0.5">
          {frames.map((frame, index) => (
            <div
              className="flex items-baseline gap-2 font-vsc-mono text-[12px]"
              // biome-ignore lint/suspicious/noArrayIndexKey: a stack can repeat a frame
              key={`${frame}-${index}`}
            >
              <span className="text-vsc-text-faint">at</span>
              <span
                className={cn(
                  'truncate',
                  isBuiltinFrame(frame)
                    ? 'text-vsc-text-muted'
                    : 'text-vsc-text-bright',
                )}
              >
                {frame}
              </span>
            </div>
          ))}
        </div>
        {hiddenCount > 0 && (
          <button
            className="mt-1 text-[11px] text-vsc-accent hover:underline"
            onClick={() => setShowAllFrames((current) => !current)}
            type="button"
          >
            {showAllFrames
              ? 'Hide runtime frames'
              : `Show ${hiddenCount} runtime and provider ${hiddenCount === 1 ? 'frame' : 'frames'}`}
          </button>
        )}
        {!error.stackComplete && (
          <p className="mt-1 text-[11px] text-vsc-text-faint">
            Frames were lost, so this is not the whole path from root to throw.
          </p>
        )}
      </div>
    </div>
  );
};

/**
 * Threads, drawn like a trace: each lane starts at its real offset on the
 * execution clock, indented under the thread that spawned it.
 */
const ThreadsPanel: FC<{ threads: ThreadLane[]; wallMs: number }> = ({
  threads,
  wallMs,
}) => {
  const ordered = [...threads].sort(
    (left, right) => left.firstMs - right.firstMs,
  );
  return (
    <Panel
      subtitle="every logical thread, at its real start offset"
      title="Threads"
    >
      <div className="space-y-2.5 p-3">
        {ordered.map((lane) => {
          const left = Math.min(98, (lane.firstMs / wallMs) * 100);
          const end = lane.lastMs ?? wallMs;
          const width = Math.max(
            0.8,
            Math.min(100 - left, ((end - lane.firstMs) / wallMs) * 100),
          );
          return (
            <div key={lane.id} style={{ paddingLeft: lane.spawnedBy ? 16 : 0 }}>
              <div className="mb-1 flex items-center justify-between gap-2 text-[12px]">
                <span className="flex min-w-0 items-center gap-1.5">
                  {lane.spawnedBy && (
                    <GitBranch className="h-3.5 w-3.5 shrink-0 text-vsc-text-faint" />
                  )}
                  <span className="truncate font-vsc-mono">{lane.name}</span>
                  {lane.spawnedBy?.fn && (
                    <span className="truncate text-vsc-text-faint">
                      spawned by {lane.spawnedBy.fn} at +
                      {formatDuration(lane.spawnedBy.atMs)}
                    </span>
                  )}
                  {lane.status === 'cancelled' && (
                    <XCircle className="h-3.5 w-3.5 shrink-0 text-vsc-yellow" />
                  )}
                </span>
                <span className="shrink-0 font-vsc-mono text-vsc-text-faint">
                  {formatDuration(end - lane.firstMs)}
                </span>
              </div>
              <div className="relative h-2 rounded bg-vsc-bg">
                <div
                  className="absolute inset-y-0 rounded bg-vsc-accent/45"
                  style={{ left: `${left}%`, width: `${width}%` }}
                />
              </div>
            </div>
          );
        })}
      </div>
      <div className="border-t border-vsc-border-subtle px-3 py-2 text-[11px] text-vsc-text-faint">
        Lanes share the execution clock. The bar is when a thread was alive, not
        how much of that time it spent running: the store attributes running and
        waiting time per call path, not per thread.
      </div>
    </Panel>
  );
};

// ---------------------------------------------------------------------------
// Trace
// ---------------------------------------------------------------------------

interface TraceRow {
  span: SpanNode;
  depth: number;
  /** Ancestor levels that still have a following sibling, for tree lines. */
  ancestorsWithMore: boolean[];
  lastChild: boolean;
}

function flattenTrace(spans: SpanNode[]): TraceRow[] {
  const byParent = new Map<string | null, SpanNode[]>();
  for (const span of spans) {
    const list = byParent.get(span.parentId) ?? [];
    list.push(span);
    byParent.set(span.parentId, list);
  }
  for (const list of byParent.values()) {
    list.sort((left, right) => left.startMs - right.startMs);
  }

  const rows: TraceRow[] = [];
  const walk = (
    span: SpanNode,
    depth: number,
    ancestorsWithMore: boolean[],
    lastChild: boolean,
  ) => {
    rows.push({ ancestorsWithMore, depth, lastChild, span });
    const children = byParent.get(span.id) ?? [];
    children.forEach((child, index) => {
      walk(
        child,
        depth + 1,
        [...ancestorsWithMore, !lastChild],
        index === children.length - 1,
      );
    });
  };
  const roots = byParent.get(null) ?? [];
  roots.forEach((root, index) => {
    walk(root, 0, [], index === roots.length - 1);
  });
  return rows;
}

/**
 * Tree guides for one row.
 *
 * Reviewers found the call tree hard to read as bare indentation. These are
 * the lines that make ancestry visible: a vertical rule for each ancestor
 * level that still has work below it, and an elbow into this row.
 */
const TreeGuides: FC<{
  ancestorsWithMore: boolean[];
  lastChild: boolean;
  depth: number;
}> = ({ ancestorsWithMore, lastChild, depth }) => {
  if (depth === 0) return null;
  return (
    <span aria-hidden className="flex shrink-0 self-stretch">
      {ancestorsWithMore.map((hasMore, level) => (
        <span
          className="relative w-4"
          // biome-ignore lint/suspicious/noArrayIndexKey: levels are positional
          key={level}
        >
          {hasMore && (
            <span className="absolute inset-y-0 left-1/2 w-px bg-vsc-border" />
          )}
        </span>
      ))}
      <span className="relative w-4">
        <span
          className={cn(
            'absolute left-1/2 w-px bg-vsc-border',
            lastChild ? 'top-0 h-1/2' : 'inset-y-0',
          )}
        />
        <span className="absolute left-1/2 top-1/2 h-px w-1/2 bg-vsc-border" />
      </span>
    </span>
  );
};

const TraceTab: FC<{
  evidence: Evidence;
  startedMs: number | null;
  selectedSpanId: string | null;
  setSelectedSpanId: (id: string | null) => void;
  openContext: (contextId: string) => void;
  signatures?: ReadonlyMap<string, string>;
  onOpenSource?: (file: string, line: number | null) => void;
  onLoadMedia?: (cid: string) => Promise<TelemetryMedia>;
}> = ({
  evidence,
  startedMs,
  selectedSpanId,
  setSelectedSpanId,
  openContext,
  signatures,
  onOpenSource,
  onLoadMedia,
}) => {
  const [query, setQuery] = useState('');
  const rows = useMemo(() => flattenTrace(evidence.spans), [evidence.spans]);

  const visibleRows = useMemo(() => {
    const needle = query.trim().toLowerCase();
    if (!needle) return rows;
    // Keep matching spans plus their retained ancestors, so depth stays
    // meaningful instead of collapsing every match to the left margin.
    const keep = new Set<string>();
    const byId = new Map(evidence.spans.map((span) => [span.id, span]));
    for (const span of evidence.spans) {
      if (!span.fn.toLowerCase().includes(needle)) continue;
      let current: SpanNode | undefined = span;
      while (current && !keep.has(current.id)) {
        keep.add(current.id);
        current = current.parentId ? byId.get(current.parentId) : undefined;
      }
    }
    return rows.filter((row) => keep.has(row.span.id));
  }, [evidence.spans, query, rows]);

  const totalMs = Math.max(
    evidence.durationMs ?? 0,
    ...evidence.spans.map((span) => span.startMs + (span.durationMs ?? 0)),
    1,
  );
  const selectedSpan =
    evidence.spans.find((span) => span.id === selectedSpanId) ??
    evidence.spans[0] ??
    null;
  const gapsByContext = useMemo(
    () => new Map(evidence.gaps.map((gap) => [gap.contextId, gap])),
    [evidence.gaps],
  );
  const totalGapCalls = evidence.gaps.reduce((sum, gap) => sum + gap.calls, 0);

  return (
    <div className="flex h-full min-h-[520px] min-w-0">
      <div className="flex min-w-0 flex-[1.5] flex-col border-r border-vsc-border">
        <div className="flex h-9 shrink-0 items-center gap-2 border-b border-vsc-border-subtle bg-vsc-surface px-3 text-[12px] text-vsc-text-muted">
          <Waypoints className="h-3.5 w-3.5" />
          <Pill
            className="border-vsc-accent/20 text-vsc-accent"
            title="Calls retained with exact timestamps: the rows below"
          >
            {evidence.spans.length} traced
          </Pill>
          {totalGapCalls > 0 && (
            <Pill title="Calls that exist only in the counts. Open Timings to see them grouped by call path.">
              <Sigma className="h-3 w-3" />
              {totalGapCalls.toLocaleString()} counted only
            </Pill>
          )}
          <div className="relative ml-auto w-44">
            <Search className="absolute left-1.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-vsc-text-faint" />
            <input
              aria-label="Filter spans by function name"
              className="h-6 w-full rounded border border-vsc-input-border bg-vsc-input-bg pl-6 pr-2 text-[12px] text-vsc-input-fg outline-none placeholder:text-vsc-text-faint focus:border-vsc-accent"
              onChange={(event) => setQuery(event.target.value)}
              placeholder="Filter spans"
              value={query}
            />
          </div>
        </div>

        <div className="grid shrink-0 grid-cols-[96px_80px_minmax(180px,34%)_minmax(180px,1fr)_80px] border-b border-vsc-border bg-vsc-surface px-2 py-1 text-[11px] font-semibold uppercase tracking-wide text-vsc-text-faint">
          <span>Time</span>
          <span>Thread</span>
          <span>Span</span>
          <span className="relative block h-4">
            {[0, 0.25, 0.5, 0.75, 1].map((fraction) => (
              <span
                className="absolute top-0 -translate-x-1/2 font-vsc-mono normal-case"
                key={fraction}
                style={{ left: `${fraction * 100}%` }}
              >
                {fraction === 0 ? '0' : formatDuration(totalMs * fraction)}
              </span>
            ))}
          </span>
          <span className="text-right">Duration</span>
        </div>

        <div className="min-h-0 flex-1 overflow-auto py-1">
          {visibleRows.map((row, index) => {
            const previous = visibleRows[index - 1];
            const threadLabel =
              previous?.span.threadName === row.span.threadName
                ? null
                : row.span.threadName;
            return (
              <SpanRowView
                gap={
                  row.span.contextId
                    ? (gapsByContext.get(row.span.contextId) ?? null)
                    : null
                }
                key={row.span.id}
                onOpenContext={openContext}
                onSelect={() => setSelectedSpanId(row.span.id)}
                row={row}
                selected={selectedSpan?.id === row.span.id}
                startedMs={startedMs}
                threadLabel={threadLabel}
                totalMs={totalMs}
              />
            );
          })}
          {evidence.spans.length === 0 && (
            <div className="mx-3 mt-4 rounded border border-dashed border-vsc-border px-3 py-4 text-[12px] text-vsc-text-muted">
              <Sigma className="mb-1 h-3.5 w-3.5" />
              No individual calls were retained for this execution. The
              aggregate counts are still complete: open{' '}
              <span className="font-medium text-vsc-text">Timings</span> to see
              every call path with its counts and timing.
            </div>
          )}
          {query.trim() !== '' &&
            visibleRows.length === 0 &&
            evidence.spans.length > 0 && (
              <div className="mx-3 mt-4 rounded border border-dashed border-vsc-border px-3 py-4 text-[12px] text-vsc-text-muted">
                No retained spans match "{query.trim()}".
              </div>
            )}
        </div>
      </div>
      <SpanInspector
        context={
          selectedSpan?.contextId
            ? (evidence.contexts.find(
                (context) => context.id === selectedSpan.contextId,
              ) ?? null)
            : null
        }
        evidence={evidence}
        onLoadMedia={onLoadMedia}
        onOpenContext={openContext}
        onOpenSource={onOpenSource}
        signatures={signatures}
        span={selectedSpan}
      />
    </div>
  );
};

const SpanRowView: FC<{
  row: TraceRow;
  totalMs: number;
  startedMs: number | null;
  threadLabel: string | null;
  gap: GapInfo | null;
  selected: boolean;
  onSelect: () => void;
  onOpenContext: (contextId: string) => void;
}> = ({
  row,
  totalMs,
  startedMs,
  threadLabel,
  gap,
  selected,
  onSelect,
  onOpenContext,
}) => {
  const { span } = row;
  const left = Math.min(98, (span.startMs / totalMs) * 100);
  const width = Math.max(
    0.6,
    Math.min(100 - left, ((span.durationMs ?? 0) / totalMs) * 100),
  );
  return (
    <div
      className={cn(
        'group grid h-8 cursor-pointer grid-cols-[96px_80px_minmax(180px,34%)_minmax(180px,1fr)_80px] items-stretch px-2 text-[12px] hover:bg-vsc-hover focus:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-vsc-accent',
        selected && 'bg-vsc-accent/10',
      )}
      {...rowActivation(onSelect)}
    >
      <span
        className="self-center pr-2 text-left font-vsc-mono text-[11px] text-vsc-text-faint"
        title={`+${formatDuration(span.startMs)} after the execution started`}
      >
        {startedMs == null
          ? `+${formatDuration(span.startMs)}`
          : formatClock(startedMs + span.startMs)}
      </span>
      <span className="self-center truncate pr-2 font-vsc-mono text-[11px] text-vsc-text-faint">
        {threadLabel ?? ''}
      </span>
      <div className="flex min-w-0 items-center">
        <TreeGuides
          ancestorsWithMore={row.ancestorsWithMore}
          depth={row.depth}
          lastChild={row.lastChild}
        />
        <span className="flex min-w-0 items-center gap-1.5">
          <KindGlyph fn={span.fn} kind={span.kind} />
          <span className="truncate font-vsc-mono">{span.fn}</span>
          {span.status === 'failed' && (
            <XCircle className="h-3.5 w-3.5 shrink-0 text-vsc-red" />
          )}
          {span.status === 'cancelled' && (
            <XCircle className="h-3.5 w-3.5 shrink-0 text-vsc-yellow" />
          )}
        </span>
        {/* Aggregate-only siblings on this call path: a chip, never a row,
            because they have no position in time. This pivots to Timings,
            which is not what the row does, so it keeps its own click. */}
        {gap && (
          <button
            className="ml-1.5 flex shrink-0 items-center gap-0.5 rounded border border-dashed border-vsc-border px-1 py-px text-[11px] text-vsc-text-muted hover:border-vsc-accent/40 hover:text-vsc-accent"
            onClick={(event) => {
              stopRowActivation(event);
              onOpenContext(gap.contextId);
            }}
            title={`${gap.calls.toLocaleString()} more calls took this path but were not individually retained. Click to see the aggregate in Timings.`}
            type="button"
          >
            <Sigma className="h-3 w-3" />+{gap.calls.toLocaleString()}
          </button>
        )}
      </div>
      <span className="relative my-1.5 block overflow-hidden rounded-sm bg-vsc-surface">
        <span
          className={cn(
            'absolute inset-y-0 rounded-sm opacity-85',
            span.status === 'failed' && 'ring-1 ring-inset ring-vsc-red',
          )}
          style={{
            backgroundColor: functionColor(span.fn, span.kind),
            left: `${left}%`,
            width: `${width}%`,
          }}
        />
      </span>
      <span
        className={cn(
          'self-center pr-1 text-right font-vsc-mono',
          span.status === 'failed' ? 'text-vsc-red' : 'text-vsc-text-muted',
        )}
      >
        {formatDuration(span.durationMs) || 'running'}
      </span>
    </div>
  );
};

const SpanInspector: FC<{
  span: SpanNode | null;
  context: ContextNode | null;
  evidence: Evidence;
  onOpenContext: (contextId: string) => void;
  signatures?: ReadonlyMap<string, string>;
  onOpenSource?: (file: string, line: number | null) => void;
  onLoadMedia?: (cid: string) => Promise<TelemetryMedia>;
}> = ({
  span,
  context,
  evidence,
  onOpenContext,
  signatures,
  onOpenSource,
  onLoadMedia,
}) => {
  if (!span) {
    return (
      <aside className="flex min-w-[320px] flex-1 items-center justify-center bg-vsc-bg p-6 text-center text-[12px] text-vsc-text-faint">
        Select a span to inspect its exact evidence.
      </aside>
    );
  }
  const retained = context
    ? evidence.spans.filter((other) => other.contextId === context.id).length
    : 0;
  const signature = signatures?.get(span.fn);
  return (
    <aside className="flex min-w-[320px] flex-1 flex-col overflow-auto bg-vsc-bg">
      <div className="border-b border-vsc-border bg-vsc-surface px-3 py-3">
        <div className="flex items-start gap-2">
          <span className="mt-0.5">
            <KindGlyph fn={span.fn} kind={span.kind} />
          </span>
          <div className="min-w-0 flex-1">
            <div className="truncate font-vsc-mono text-[15px] font-semibold">
              {span.fn}
            </div>
            {/* Reviewers asked to see what the function actually takes and
                returns while reading its trace. */}
            {signature && (
              <div className="mt-0.5 break-words font-vsc-mono text-[11px] text-vsc-text-muted">
                {signature}
              </div>
            )}
            <div className="mt-1 flex flex-wrap gap-1.5">
              {span.kind === 'llm' && <Pill>model call</Pill>}
              {span.threadName && <Pill>thread {span.threadName}</Pill>}
              {span.reasons.map((reason) => (
                <Pill
                  key={reason}
                  title="Why this call was individually retained"
                >
                  kept: {reason}
                </Pill>
              ))}
            </div>
          </div>
          <StatusIcon
            className={statusStyles(span.status).split(' ')[0]}
            status={span.status}
          />
        </div>
        <div className="mt-3 grid grid-cols-2 gap-2">
          <Stat label="Started" value={`+${formatDuration(span.startMs)}`} />
          <Stat
            label="Duration"
            value={formatDuration(span.durationMs) || 'running'}
          />
        </div>
        {span.source && (
          <SourceLink onOpen={onOpenSource} source={span.source} />
        )}
      </div>

      <div className="min-h-0 flex-1 space-y-3 overflow-auto p-3">
        <CapturedValueView
          onLoadMedia={onLoadMedia}
          value={span.values.args}
          valueRole="Arguments"
        />
        <CapturedValueView
          onLoadMedia={onLoadMedia}
          value={span.values.output}
          valueRole="Return"
        />
        {span.values.error.availability.state !== 'notApplicable' && (
          <CapturedValueView
            onLoadMedia={onLoadMedia}
            value={span.values.error}
            valueRole="Error"
          />
        )}
      </div>

      {context && (
        <div className="border-t border-vsc-border bg-vsc-surface px-3 py-2.5">
          <SectionHeading>Calling context</SectionHeading>
          <div className="flex items-center gap-2 text-[12px] text-vsc-text-muted">
            <Sigma className="h-3.5 w-3.5 shrink-0" />
            <span className="min-w-0 flex-1">
              This path ran{' '}
              <span className="font-vsc-mono text-vsc-text">
                {context.enters.toLocaleString()}
              </span>{' '}
              times here, {retained} retained
            </span>
            <button
              className="shrink-0 text-vsc-accent hover:underline"
              onClick={() => onOpenContext(context.id)}
              type="button"
            >
              View in Timings <ArrowRight className="ml-0.5 inline h-3 w-3" />
            </button>
          </div>
        </div>
      )}
    </aside>
  );
};

// ---------------------------------------------------------------------------
// Timings
// ---------------------------------------------------------------------------

export interface ContextTreeNode {
  context: ContextNode;
  children: ContextTreeNode[];
}

type SortKey = 'total' | 'self' | 'calls' | 'errors';

function buildContextTree(
  contexts: ContextNode[],
  compare: (left: ContextNode, right: ContextNode) => number,
): ContextTreeNode[] {
  const nodes = new Map<string, ContextTreeNode>();
  for (const context of contexts) {
    nodes.set(context.id, { children: [], context });
  }
  const roots: ContextTreeNode[] = [];
  for (const context of contexts) {
    const node = nodes.get(context.id);
    if (!node) continue;
    const parent = context.parentId ? nodes.get(context.parentId) : undefined;
    if (parent) parent.children.push(node);
    else roots.push(node);
  }
  // Folded rows always sort last: they stand for leftovers, not for work
  // that outranks a named path.
  const sortRec = (list: ContextTreeNode[]) => {
    list.sort((left, right) => {
      if (left.context.folded !== right.context.folded) {
        return left.context.folded ? 1 : -1;
      }
      return compare(left.context, right.context);
    });
    for (const node of list) sortRec(node.children);
  };
  sortRec(roots);
  return roots;
}

const TimingsTab: FC<{
  evidence: Evidence;
  selectedContextId: string | null;
  setSelectedContextId: (id: string | null) => void;
  openSpan: (span: SpanNode) => void;
  signatures?: ReadonlyMap<string, string>;
  onOpenSource?: (file: string, line: number | null) => void;
}> = ({
  evidence,
  selectedContextId,
  setSelectedContextId,
  openSpan,
  signatures,
  onOpenSource,
}) => {
  const [sortKey, setSortKey] = useState<SortKey>('total');
  const [ascending, setAscending] = useState(false);
  // The flame keeps one stable picture; the toggle only decides whether
  // waiting time counts toward width.
  const [includeAwait, setIncludeAwait] = useState(true);
  // A model call sits on several frames of client plumbing. Showing only the
  // reader's own functions is the difference between a three-row flame and a
  // ten-row one, so it is the default.
  const [userOnly, setUserOnly] = useState(true);
  const hasRuntimeFrames = useMemo(
    () =>
      evidence.contexts.some(
        (context) =>
          !context.folded &&
          context.fqn != null &&
          !context.fqn.startsWith('user.'),
      ),
    [evidence.contexts],
  );

  // Only offer the running view when some wait was actually recorded;
  // otherwise the two modes are the same picture under different names.
  const awaitKnown = evidence.contexts.some(
    (context) => context.subtreeAwaitMs > 0,
  );
  const compare = useCallback(
    (left: ContextNode, right: ContextNode) => {
      const value = (context: ContextNode): number => {
        switch (sortKey) {
          case 'calls':
            return context.enters;
          case 'errors':
            return context.errors;
          case 'self':
            return context.selfMs;
          default:
            return context.totalMs;
        }
      };
      return ascending
        ? value(left) - value(right)
        : value(right) - value(left);
    },
    [ascending, sortKey],
  );

  const tree = useMemo(
    () => buildContextTree(evidence.contexts, compare),
    [compare, evidence.contexts],
  );
  const flameTree = useMemo(
    () =>
      buildContextTree(
        evidence.contexts,
        (left, right) => right.totalMs - left.totalMs,
      ),
    [evidence.contexts],
  );
  // Elapsed, or elapsed minus every wait inside the call. Subtracting only
  // the frame's own wait would change nothing for user code, because the
  // frame that suspends is always a transport leaf far below it.
  const metric = useCallback(
    (context: ContextNode): number =>
      includeAwait
        ? context.totalMs
        : Math.max(0, context.totalMs - context.subtreeAwaitMs),
    [includeAwait],
  );
  const rootTotalMs = Math.max(
    1,
    ...evidence.contexts
      .filter((context) => context.parentId == null)
      .map((context) => context.totalMs),
  );
  const selected =
    evidence.contexts.find((context) => context.id === selectedContextId) ??
    evidence.contexts[0] ??
    null;

  // Every ancestor of the selection, so the tree can reveal a row that sits
  // under a parent the reader had collapsed.
  const revealIds = useMemo(
    () => ancestorIdsOf(evidence.contexts, selected?.id ?? null),
    [evidence.contexts, selected],
  );

  const onSort = useCallback((key: SortKey) => {
    setSortKey((current) => {
      if (current === key) {
        setAscending((direction) => !direction);
        return current;
      }
      setAscending(false);
      return key;
    });
  }, []);

  return (
    <div className="flex h-full min-h-[520px] min-w-0">
      <div className="flex min-w-0 flex-[1.5] flex-col border-r border-vsc-border">
        <div className="flex h-9 shrink-0 items-center gap-2 border-b border-vsc-border-subtle bg-vsc-surface px-3 text-[12px] text-vsc-text-muted">
          <Flame className="h-3.5 w-3.5" />
          <span>Where time went</span>
          <Pill title="These counts cover every call, retained or not">
            <Sigma className="h-3 w-3" />
            all {evidence.totalCalls.toLocaleString()} calls
          </Pill>
          <div className="ml-auto flex items-center gap-3">
            <ModeToggle
              label="Width"
              onChange={(value) => setIncludeAwait(value === 'elapsed')}
              options={[
                {
                  hint: 'Time from call to return, including time spent waiting on IO.',
                  label: 'elapsed',
                  value: 'elapsed',
                },
                {
                  disabled: !awaitKnown,
                  hint: awaitKnown
                    ? 'Elapsed time minus every wait inside the call, so what is left is time actually executing.'
                    : 'No waiting was recorded for this execution.',
                  label: 'running',
                  value: 'running',
                },
              ]}
              value={includeAwait ? 'elapsed' : 'running'}
            />
            {hasRuntimeFrames && (
              <ModeToggle
                label="Frames"
                onChange={(value) => setUserOnly(value === 'yours')}
                options={[
                  {
                    hint: 'Runtime and provider frames are replaced by the user code beneath them. Their time still shows as unfilled width under the caller.',
                    label: 'your code',
                    value: 'yours',
                  },
                  {
                    hint: 'Every frame, including runtime and provider internals.',
                    label: 'all',
                    value: 'all',
                  },
                ]}
                value={userOnly ? 'yours' : 'all'}
              />
            )}
          </div>
        </div>

        {tree.length > 0 ? (
          <>
            <div className="shrink-0 border-b border-vsc-border-subtle bg-vsc-bg p-2">
              <FlameGraph
                metric={metric}
                onSelect={setSelectedContextId}
                roots={flameTree}
                selectedId={selected?.id ?? null}
                userOnly={userOnly}
              />
            </div>
            <div className="grid shrink-0 grid-cols-[minmax(200px,1fr)_92px_72px_64px_84px_84px_72px] border-b border-vsc-border bg-vsc-surface px-2 py-1 text-[11px] font-semibold text-vsc-text-faint">
              <span className="uppercase tracking-wide">Context</span>
              {/* Reviewers read a bare "%" as a share of something that adds
                  to 100. This one nests, so children can exceed their
                  parent's share; the label and title say what it measures. */}
              <span
                className="text-right uppercase tracking-wide"
                title="Share of the execution's wall time spent in this path including its children. Nested paths overlap, so these do not add up to 100%."
              >
                % Duration
              </span>
              <SortHeader
                active={sortKey}
                ascending={ascending}
                className="justify-end"
                label="Calls"
                onSort={onSort}
                sortKey="calls"
              />
              <SortHeader
                active={sortKey}
                ascending={ascending}
                className="justify-end"
                label="Errors"
                onSort={onSort}
                sortKey="errors"
              />
              <SortHeader
                active={sortKey}
                ascending={ascending}
                className="justify-end"
                label="Total"
                onSort={onSort}
                sortKey="total"
              />
              <SortHeader
                active={sortKey}
                ascending={ascending}
                className="justify-end"
                label="Self"
                onSort={onSort}
                sortKey="self"
              />
              <span
                className="text-right uppercase tracking-wide"
                title="How many of this path's calls were individually retained as spans"
              >
                Traced
              </span>
            </div>
            <div className="min-h-0 flex-1 overflow-auto py-1">
              {tree.map((root, index) => (
                <ContextTreeRows
                  ancestorsWithMore={[]}
                  depth={0}
                  evidence={evidence}
                  key={root.context.id}
                  lastChild={index === tree.length - 1}
                  node={root}
                  onSelect={setSelectedContextId}
                  revealIds={revealIds}
                  rootTotalMs={rootTotalMs}
                  selectedId={selected?.id ?? null}
                />
              ))}
            </div>
          </>
        ) : (
          <div className="m-3 rounded border border-dashed border-vsc-border px-3 py-4 text-[12px] text-vsc-text-muted">
            No calling contexts were recorded for this execution.
          </div>
        )}
      </div>
      <ContextInspector
        context={selected}
        evidence={evidence}
        onOpenSource={onOpenSource}
        openSpan={openSpan}
        signatures={signatures}
      />
    </div>
  );
};

const SortHeader: FC<{
  label: string;
  sortKey: SortKey;
  active: SortKey;
  ascending: boolean;
  onSort: (key: SortKey) => void;
  className?: string;
}> = ({ label, sortKey, active, ascending, onSort, className }) => (
  <button
    className={cn(
      'flex items-center gap-0.5 uppercase tracking-wide',
      className,
      active === sortKey ? 'text-vsc-accent' : 'hover:text-vsc-text',
    )}
    onClick={() => onSort(sortKey)}
    title="Sorts grouped call paths. This is never execution order: that lives in Trace."
    type="button"
  >
    {label}
    {active === sortKey &&
      (ascending ? (
        <ChevronUp className="h-3 w-3" />
      ) : (
        <ChevronDown className="h-3 w-3" />
      ))}
  </button>
);

/**
 * Left-heavy flame graph. Width encodes time within the parent; the x axis
 * carries no ordering meaning, which is why nothing here is labelled with a
 * timestamp.
 */
/**
 * The ids of every ancestor of `id`.
 *
 * Selecting a context from Overview or from a Trace pivot can land on a row
 * nested under parents the reader collapsed. Those ancestors are forced open
 * so the selection is actually in the document; without it the inspector
 * describes a row that cannot be found or scrolled to.
 */
export function ancestorIdsOf(
  contexts: ContextNode[],
  id: string | null,
): ReadonlySet<string> {
  const ids = new Set<string>();
  if (id == null) return ids;
  const byId = new Map(contexts.map((context) => [context.id, context]));
  let parentId = byId.get(id)?.parentId ?? null;
  // A malformed tree must not hang the panel.
  while (parentId != null && !ids.has(parentId)) {
    ids.add(parentId);
    parentId = byId.get(parentId)?.parentId ?? null;
  }
  return ids;
}

/**
 * Make a whole row selectable.
 *
 * Rows here are grids of cells, and the obvious way to build them is to make
 * each cell that should respond its own button. That leaves every gap
 * between cells dead, so whether a click registers depends on hitting text
 * rather than on the row the reader aimed at. The row owns the click
 * instead; cells are plain markup, and only a control that does something
 * *different* stays a button and stops propagation.
 */
export function rowActivation(onActivate: () => void) {
  return {
    onClick: onActivate,
    onKeyDown: (event: ReactKeyboardEvent) => {
      if (event.key !== 'Enter' && event.key !== ' ') return;
      // Space scrolls the pane otherwise, which loses the reader's place.
      event.preventDefault();
      onActivate();
    },
    role: 'button' as const,
    tabIndex: 0,
  };
}

/**
 * The same click affordance for a real table row.
 *
 * A `<tr>` keeps its row semantics: announcing it as a button would strip
 * the column context that makes a table readable non-visually. The row is a
 * pointer convenience, and the labelled control inside it remains the
 * keyboard and screen-reader path.
 */
export function tableRowActivation(onActivate: () => void) {
  return { onClick: onActivate };
}

/** A control inside a row whose action is not the row's action. */
export function stopRowActivation(event: {
  stopPropagation: () => void;
}): void {
  event.stopPropagation();
}

/**
 * A segmented control that says what it controls and which option is live.
 *
 * The unlabelled pair this replaces read as two buttons: clicking one
 * swapped the highlight, but neither the name of the setting nor which
 * value was active was on screen. The prefix answers the first and the
 * filled segment answers the second.
 */
const ModeToggle: FC<{
  label: string;
  value: string;
  onChange: (value: string) => void;
  options: Array<{
    value: string;
    label: string;
    hint: string;
    disabled?: boolean;
  }>;
}> = ({ label, value, onChange, options }) => (
  <div className="flex items-center gap-1.5">
    <span className="text-[11px] text-vsc-text-faint">{label}</span>
    <fieldset className="flex items-center rounded border border-vsc-border p-0.5">
      {options.map((option) => {
        const active = option.value === value;
        return (
          <button
            aria-pressed={active}
            className={cn(
              'rounded px-2 py-0.5 text-[11px]',
              active
                ? 'bg-vsc-accent text-white'
                : 'text-vsc-text-muted hover:bg-vsc-hover hover:text-vsc-text',
              option.disabled && 'cursor-default opacity-40',
            )}
            disabled={option.disabled}
            key={option.value}
            onClick={() => onChange(option.value)}
            title={option.hint}
            type="button"
          >
            {option.label}
          </button>
        );
      })}
    </fieldset>
  </div>
);

/** Pixels a frame needs before its name is worth drawing. */
const FLAME_LABEL_MIN_PX = 34;

/** Pixels below which a frame is folded in with its small siblings. */
const FLAME_MIN_PX = 3;

/** One frame's slot in the laid-out flame. */
export interface FlameSlot {
  node: ContextTreeNode;
  /** Share of the parent's width, already summing to at most 1. */
  fraction: number;
}

/**
 * Lay out one node's children so their widths sum to at most the parent.
 *
 * Two things break naive layout. Spawned subtrees overlap their parent in
 * wall time, so children can sum past it; and giving every child a minimum
 * width means many small siblings overflow, which is what squashes a row
 * into unreadable slivers. So widths are normalised, and anything below the
 * visible threshold is folded into one summed sliver rather than each being
 * padded up to a size it did not earn.
 */
export function layoutChildren(
  node: ContextTreeNode,
  parentPx: number,
  metric: (context: ContextNode) => number,
): { slots: FlameSlot[]; foldedCount: number; foldedFraction: number } {
  const own = metric(node.context);
  const childSum = node.children.reduce(
    (sum, child) => sum + metric(child.context),
    0,
  );
  const scale = childSum > own && childSum > 0 ? own / childSum : 1;
  const slots: FlameSlot[] = [];
  let foldedCount = 0;
  let foldedFraction = 0;

  for (const child of node.children) {
    const fraction = own > 0 ? (metric(child.context) * scale) / own : 0;
    if (fraction * parentPx < FLAME_MIN_PX) {
      foldedCount += 1;
      foldedFraction += fraction;
      continue;
    }
    slots.push({ fraction, node: child });
  }
  return { foldedCount, foldedFraction, slots };
}

/**
 * Replace runtime frames with the user code underneath them.
 *
 * A single model call sits on seven frames of client plumbing, which is what
 * turns a three-function program into a ten-deep flame where nothing legible
 * belongs to the reader. Collapsing lifts user descendants up to their
 * nearest user ancestor; a subtree that is entirely runtime disappears, and
 * the time it used stays visible as unfilled width under its parent.
 */
export function collapseToUserFrames(
  nodes: ContextTreeNode[],
): ContextTreeNode[] {
  const out: ContextTreeNode[] = [];
  for (const node of nodes) {
    const lifted = collapseToUserFrames(node.children);
    if (isUserContext(node.context)) {
      out.push({ children: lifted, context: node.context });
    } else {
      out.push(...lifted);
    }
  }
  return out;
}

function isUserContext(context: ContextNode): boolean {
  // Folded overflow rows have no name to judge; they stand for real calls,
  // so hiding them would quietly drop work from the picture.
  if (context.folded) return true;
  return context.fqn == null || context.fqn.startsWith('user.');
}

function findNode(
  nodes: ContextTreeNode[],
  id: string,
): ContextTreeNode | null {
  for (const node of nodes) {
    if (node.context.id === id) return node;
    const found = findNode(node.children, id);
    if (found) return found;
  }
  return null;
}

/** The path from a root down to `id`, for the zoom breadcrumb. */
function pathToNode(
  nodes: ContextTreeNode[],
  id: string,
  trail: ContextNode[] = [],
): ContextNode[] | null {
  for (const node of nodes) {
    const next = [...trail, node.context];
    if (node.context.id === id) return next;
    const found = pathToNode(node.children, id, next);
    if (found) return found;
  }
  return null;
}

/**
 * Left-heavy flame graph.
 *
 * Width is time within the parent; the x axis carries no ordering, which is
 * why nothing here is labelled with a timestamp. Anything too narrow to read
 * is reached by zooming into it rather than by being drawn at a size it did
 * not earn.
 */
const FlameGraph: FC<{
  roots: ContextTreeNode[];
  selectedId: string | null;
  onSelect: (id: string) => void;
  metric: (context: ContextNode) => number;
  userOnly: boolean;
}> = ({ roots, selectedId, onSelect, metric, userOnly }) => {
  const [zoomId, setZoomId] = useState<string | null>(null);
  const [width, setWidth] = useState(900);
  const containerRef = useRef<HTMLDivElement | null>(null);

  // Label and fold thresholds are in pixels, so the layout has to know how
  // wide it actually is rather than guess from fractions alone.
  useEffect(() => {
    const element = containerRef.current;
    if (!element) return;
    const measure = () => setWidth(element.clientWidth || 900);
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  const shown = useMemo(
    () => (userOnly ? collapseToUserFrames(roots) : roots),
    [roots, userOnly],
  );
  const zoomed = zoomId ? findNode(shown, zoomId) : null;
  const trail = zoomId ? (pathToNode(shown, zoomId) ?? []) : [];
  const visibleRoots = zoomed ? [zoomed] : shown;
  const total = Math.max(1, ...visibleRoots.map((r) => metric(r.context)));

  // A zoom that survives a toggle would point at a node no longer shown.
  // biome-ignore lint/correctness/useExhaustiveDependencies: reset on toggle
  useEffect(() => {
    setZoomId(null);
  }, [userOnly]);

  return (
    <div>
      {trail.length > 0 && (
        <div className="mb-1 flex flex-wrap items-center gap-1 text-[11px]">
          <button
            className="text-vsc-accent hover:underline"
            onClick={() => setZoomId(null)}
            type="button"
          >
            all
          </button>
          {trail.map((context, index) => (
            <span className="flex items-center gap-1" key={context.id}>
              <ChevronRight className="h-3 w-3 text-vsc-text-faint" />
              {index === trail.length - 1 ? (
                <span className="font-vsc-mono text-vsc-text">
                  {context.fn}
                </span>
              ) : (
                <button
                  className="font-vsc-mono text-vsc-accent hover:underline"
                  onClick={() => setZoomId(context.id)}
                  type="button"
                >
                  {context.fn}
                </button>
              )}
            </span>
          ))}
        </div>
      )}
      <div className="space-y-px" ref={containerRef}>
        {visibleRoots.map((root) => (
          <FlameNode
            key={root.context.id}
            metric={metric}
            node={root}
            onSelect={onSelect}
            onZoom={setZoomId}
            pxWidth={(metric(root.context) / total) * width}
            selectedId={selectedId}
          />
        ))}
      </div>
    </div>
  );
};

const FlameNode: FC<{
  node: ContextTreeNode;
  /** Measured width of this frame, so thresholds can be real pixels. */
  pxWidth: number;
  selectedId: string | null;
  onSelect: (id: string) => void;
  onZoom: (id: string) => void;
  metric: (context: ContextNode) => number;
}> = ({ node, pxWidth, selectedId, onSelect, onZoom, metric }) => {
  const { context } = node;
  const { slots, foldedCount, foldedFraction } = layoutChildren(
    node,
    pxWidth,
    metric,
  );
  const hasChildren = slots.length > 0 || foldedCount > 0;

  return (
    <div className="min-w-0">
      <button
        className={cn(
          'block h-[18px] w-full overflow-hidden whitespace-nowrap rounded-[2px] px-1 text-left font-vsc-mono text-[11px] leading-[18px] text-white/95',
          context.folded && 'opacity-40',
          context.spawn && 'ring-1 ring-inset ring-white/40',
          selectedId === context.id && 'ring-2 ring-inset ring-vsc-accent',
        )}
        onClick={() => onSelect(context.id)}
        onDoubleClick={() => onZoom(context.id)}
        style={{
          backgroundColor: context.folded
            ? 'var(--vsc-text-faint, #777)'
            : functionColor(context.fn, context.kind),
        }}
        title={`${context.fn}\n${context.enters.toLocaleString()} calls, ${formatDuration(context.totalMs)} total, ${formatDuration(context.selfMs)} self\nDouble-click to zoom in`}
        type="button"
      >
        {pxWidth >= FLAME_LABEL_MIN_PX && context.fn}
      </button>
      {hasChildren && (
        <div className="mt-px flex min-w-0">
          {slots.map((slot) => (
            <div
              key={slot.node.context.id}
              // Percentages of this row sum to at most 1, so no child is
              // pushed out of the parent's width.
              style={{ flex: `0 0 ${slot.fraction * 100}%` }}
            >
              <FlameNode
                metric={metric}
                node={slot.node}
                onSelect={onSelect}
                onZoom={onZoom}
                pxWidth={slot.fraction * pxWidth}
                selectedId={selectedId}
              />
            </div>
          ))}
          {foldedCount > 0 && (
            <div style={{ flex: `0 0 ${foldedFraction * 100}%` }}>
              <div
                className="h-[18px] rounded-[2px] bg-vsc-border/60"
                title={`${foldedCount} ${foldedCount === 1 ? 'frame' : 'frames'} too narrow to draw. Double-click the parent to zoom in.`}
              />
            </div>
          )}
        </div>
      )}
    </div>
  );
};

const ContextTreeRows: FC<{
  node: ContextTreeNode;
  depth: number;
  ancestorsWithMore: boolean[];
  lastChild: boolean;
  selectedId: string | null;
  onSelect: (id: string) => void;
  evidence: Evidence;
  rootTotalMs: number;
  /** Ancestors of the selection, which stay open however they were left. */
  revealIds: ReadonlySet<string>;
}> = ({
  node,
  depth,
  ancestorsWithMore,
  lastChild,
  selectedId,
  onSelect,
  evidence,
  rootTotalMs,
  revealIds,
}) => {
  const [collapsed, setCollapsed] = useState(false);
  const { context, children } = node;
  const rowRef = useRef<HTMLDivElement | null>(null);
  const selected = selectedId === context.id;

  // Arriving from Overview or from a Trace pivot selects a row that may be
  // anywhere in a long tree. Selecting without revealing leaves the reader
  // looking at an unchanged screen, with the inspector describing something
  // they cannot find.
  useEffect(() => {
    if (!selected) return;
    rowRef.current?.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
  }, [selected]);

  // An ancestor of the selection opens even if it was collapsed by hand,
  // because the alternative is a selected row that is not in the document.
  const open = !collapsed || revealIds.has(context.id);
  const retained = evidence.spans.filter(
    (span) => span.contextId === context.id,
  ).length;
  const share = Math.min(
    100,
    Math.round((context.totalMs / rootTotalMs) * 100),
  );
  return (
    <>
      <div
        className={cn(
          'grid h-8 cursor-pointer grid-cols-[minmax(200px,1fr)_92px_72px_64px_84px_84px_72px] items-stretch px-2 text-[12px] hover:bg-vsc-hover focus:outline-none focus-visible:ring-1 focus-visible:ring-inset focus-visible:ring-vsc-accent',
          selected && 'bg-vsc-accent/10',
        )}
        ref={rowRef}
        {...rowActivation(() => onSelect(context.id))}
      >
        <div className="flex min-w-0 items-center">
          <TreeGuides
            ancestorsWithMore={ancestorsWithMore}
            depth={depth}
            lastChild={lastChild}
          />
          <button
            aria-label={`${collapsed ? 'Expand' : 'Collapse'} ${context.fn}`}
            className="mr-1 flex h-4 w-4 shrink-0 items-center justify-center"
            disabled={children.length === 0}
            onClick={(event) => {
              stopRowActivation(event);
              setCollapsed((current) => !current);
            }}
            type="button"
          >
            {children.length > 0 &&
              (open ? (
                <ChevronDown className="h-3.5 w-3.5" />
              ) : (
                <ChevronRight className="h-3.5 w-3.5" />
              ))}
          </button>
          <span className="flex min-w-0 flex-1 items-center gap-1.5 text-left">
            <KindGlyph fn={context.fn} kind={context.kind} />
            <span
              className={cn(
                'truncate font-vsc-mono',
                context.folded && 'italic text-vsc-text-faint',
              )}
            >
              {context.fn}
            </span>
            {context.spawn && (
              <span
                className="shrink-0 rounded bg-vsc-bg px-1 text-[11px] text-vsc-text-faint"
                title="Runs on its own thread, so its time overlaps its parent's"
              >
                spawned
              </span>
            )}
          </span>
        </div>
        {/* Share, with the bar that makes it comparable at a glance. */}
        <div className="flex items-center justify-end gap-1.5 pr-1">
          {!context.folded && (
            <>
              <span className="h-1 w-10 shrink-0 rounded bg-vsc-bg">
                <span
                  className="block h-full rounded bg-vsc-accent/70"
                  style={{ width: `${share}%` }}
                />
              </span>
              <span className="font-vsc-mono text-vsc-text-faint">
                {share}%
              </span>
            </>
          )}
        </div>
        <span className="self-center text-right font-vsc-mono text-vsc-text-muted">
          {context.enters.toLocaleString()}
        </span>
        <span className="self-center text-right font-vsc-mono font-semibold text-vsc-red">
          {context.errors > 0 ? context.errors.toLocaleString() : ''}
        </span>
        <span
          className={cn(
            'self-center text-right font-vsc-mono text-vsc-text-muted',
            !context.timingComplete && 'text-vsc-yellow',
          )}
          title={
            context.timingComplete
              ? undefined
              : 'Timing for this path is incomplete: a counter saturated or self time underflowed'
          }
        >
          {formatDuration(context.totalMs)}
        </span>
        <span className="self-center text-right font-vsc-mono text-vsc-text-muted">
          {formatDuration(context.selfMs)}
        </span>
        <span className="self-center text-right">
          {retained > 0 ? (
            <span
              className="rounded border border-vsc-accent/25 bg-vsc-accent/5 px-1.5 py-0.5 font-vsc-mono text-[11px] text-vsc-accent"
              title={`${retained} of ${context.enters.toLocaleString()} calls retained as spans`}
            >
              {retained}/{context.enters.toLocaleString()}
            </span>
          ) : null}
        </span>
      </div>
      {open &&
        children.map((child, index) => (
          <ContextTreeRows
            ancestorsWithMore={[...ancestorsWithMore, !lastChild]}
            depth={depth + 1}
            evidence={evidence}
            key={child.context.id}
            lastChild={index === children.length - 1}
            node={child}
            onSelect={onSelect}
            revealIds={revealIds}
            rootTotalMs={rootTotalMs}
            selectedId={selectedId}
          />
        ))}
    </>
  );
};

function contextPath(evidence: Evidence, context: ContextNode): string[] {
  const path: string[] = [];
  const seen = new Set<string>();
  let current: ContextNode | undefined = context;
  while (current && !seen.has(current.id)) {
    seen.add(current.id);
    path.unshift(current.fn);
    const parentId: string | null = current.parentId;
    current = parentId
      ? evidence.contexts.find((other) => other.id === parentId)
      : undefined;
  }
  return path;
}

const ContextInspector: FC<{
  context: ContextNode | null;
  evidence: Evidence;
  openSpan: (span: SpanNode) => void;
  signatures?: ReadonlyMap<string, string>;
  onOpenSource?: (file: string, line: number | null) => void;
}> = ({ context, evidence, openSpan, signatures, onOpenSource }) => {
  if (!context) {
    return (
      <aside className="flex min-w-[320px] flex-1 items-center justify-center bg-vsc-bg p-6 text-center text-[12px] text-vsc-text-faint">
        Select a calling context to inspect its aggregate.
      </aside>
    );
  }
  const exemplars = evidence.spans.filter(
    (span) => span.contextId === context.id,
  );
  const distribution = summarizeDurations(exemplars, context.enters);
  const path = contextPath(evidence, context);
  const signature = signatures?.get(context.fn);
  const childMs = Math.max(
    0,
    context.totalMs - context.selfMs - (context.awaitMs ?? 0),
  );

  return (
    <aside className="flex min-w-[320px] flex-1 flex-col overflow-auto bg-vsc-bg">
      <div className="border-b border-vsc-border bg-vsc-surface px-3 py-3">
        <div className="flex items-start gap-2">
          <span className="mt-0.5">
            <KindGlyph fn={context.fn} kind={context.kind} />
          </span>
          <div className="min-w-0 flex-1">
            <div className="truncate font-vsc-mono text-[15px] font-semibold">
              {context.fn}
            </div>
            {signature && (
              <div className="mt-0.5 break-words font-vsc-mono text-[11px] text-vsc-text-muted">
                {signature}
              </div>
            )}
            <div className="mt-1 truncate font-vsc-mono text-[11px] text-vsc-text-faint">
              {path.join(' → ')}
            </div>
            <div className="mt-1.5 flex flex-wrap gap-1.5">
              <Pill title="Counts and timing cover every call that took this path">
                <Sigma className="h-3 w-3" /> aggregate
              </Pill>
              {context.spawn && <Pill>spawned thread</Pill>}
              {!context.timingComplete && (
                <Pill className="border-vsc-yellow/25 text-vsc-yellow">
                  timing incomplete
                </Pill>
              )}
            </div>
          </div>
        </div>
        {context.source && (
          <SourceLink onOpen={onOpenSource} source={context.source} />
        )}
      </div>

      <div className="min-h-0 flex-1 space-y-3 overflow-auto p-3">
        <div className="grid grid-cols-2 gap-2">
          <Stat label="Calls" value={context.enters.toLocaleString()} />
          <Stat
            label="Errors"
            tone={context.errors > 0 ? 'text-vsc-red' : undefined}
            value={context.errors.toLocaleString()}
          />
        </div>

        {/* Where this path's own time went, as one bar rather than three
            disconnected boxes. Running, waiting, and time in children are
            disjoint parts of the total, so they genuinely compose. */}
        <div>
          <SectionHeading>
            Time in this path: {formatDuration(context.totalMs)}
          </SectionHeading>
          <div className="flex h-2.5 overflow-hidden rounded bg-vsc-bg">
            <span
              className="bg-vsc-accent"
              style={{
                width: `${(context.selfMs / Math.max(1, context.totalMs)) * 100}%`,
              }}
              title={`Running ${formatDuration(context.selfMs)}`}
            />
            <span
              className="bg-vsc-yellow/70"
              style={{
                width: `${((context.awaitMs ?? 0) / Math.max(1, context.totalMs)) * 100}%`,
              }}
              title={`Waiting ${formatDuration(context.awaitMs)}`}
            />
            <span
              className="bg-vsc-border"
              style={{
                width: `${(childMs / Math.max(1, context.totalMs)) * 100}%`,
              }}
              title={`In calls it made ${formatDuration(childMs)}`}
            />
          </div>
          <div className="mt-1.5 space-y-0.5 text-[11px]">
            <div className="flex items-center gap-1.5">
              <span className="h-2 w-2 rounded-sm bg-vsc-accent" />
              <span className="text-vsc-text-muted">running</span>
              <span className="ml-auto font-vsc-mono">
                {formatDuration(context.selfMs)}
              </span>
            </div>
            <div className="flex items-center gap-1.5">
              <span className="h-2 w-2 rounded-sm bg-vsc-yellow/70" />
              <span className="text-vsc-text-muted">
                waiting on IO or another task
              </span>
              <span className="ml-auto font-vsc-mono">
                {context.awaitMs == null
                  ? 'not recorded'
                  : formatDuration(context.awaitMs)}
              </span>
            </div>
            <div className="flex items-center gap-1.5">
              <span className="h-2 w-2 rounded-sm bg-vsc-border" />
              <span className="text-vsc-text-muted">in calls it made</span>
              <span className="ml-auto font-vsc-mono">
                {formatDuration(childMs)}
              </span>
            </div>
          </div>
        </div>

        <div>
          <SectionHeading>Per call</SectionHeading>
          {distribution.kind === 'population' ? (
            <div className="grid grid-cols-3 gap-2">
              <Stat label="p50" value={formatDuration(distribution.p50)} />
              <Stat label="p90" value={formatDuration(distribution.p90)} />
              <Stat label="p99" value={formatDuration(distribution.p99)} />
            </div>
          ) : distribution.kind === 'sample' ? (
            <>
              <div className="grid grid-cols-3 gap-2">
                <Stat
                  label="Fastest"
                  value={formatDuration(distribution.min)}
                />
                <Stat
                  label="Middle"
                  value={formatDuration(distribution.median)}
                />
                <Stat
                  label="Slowest"
                  value={formatDuration(distribution.max)}
                />
              </div>
              {/* Quantiles over a policy-chosen subset would describe the
                  sample and be read as the population. */}
              <p className="mt-1.5 text-[11px] text-vsc-text-faint">
                From the {distribution.retained} retained of{' '}
                {distribution.total.toLocaleString()} calls, so this is a
                sample, not a distribution over all of them.
              </p>
            </>
          ) : (
            <p className="text-[11px] text-vsc-text-faint">
              No call on this path was individually retained, so there is no
              per-call timing. The totals above still cover every call.
            </p>
          )}
        </div>

        <div>
          <SectionHeading>
            Retained calls: {exemplars.length} of{' '}
            {context.enters.toLocaleString()}
          </SectionHeading>
          {exemplars.length > 0 ? (
            <div className="space-y-1">
              {exemplars.slice(0, 20).map((span) => (
                <button
                  className="flex w-full items-center gap-2 rounded border border-vsc-border-subtle bg-vsc-surface px-2 py-1.5 text-left text-[12px] hover:border-vsc-accent/40 hover:bg-vsc-hover"
                  key={span.id}
                  onClick={() => openSpan(span)}
                  type="button"
                >
                  <StatusIcon
                    className={statusStyles(span.status).split(' ')[0]}
                    status={span.status}
                  />
                  <span className="font-vsc-mono text-vsc-text-muted">
                    +{formatDuration(span.startMs)}
                  </span>
                  <span className="font-vsc-mono">
                    {formatDuration(span.durationMs)}
                  </span>
                  <span className="ml-auto shrink-0 text-vsc-accent">
                    Trace <ArrowRight className="ml-0.5 inline h-3 w-3" />
                  </span>
                </button>
              ))}
              {exemplars.length < context.enters && (
                <div className="rounded border border-dashed border-vsc-border-subtle px-2 py-1.5 text-[11px] text-vsc-text-faint">
                  The other{' '}
                  {(context.enters - exemplars.length).toLocaleString()} calls
                  exist only in this aggregate. The counts and timing above
                  cover all of them.
                </div>
              )}
            </div>
          ) : (
            <div className="rounded border border-dashed border-vsc-border p-2 text-[12px] text-vsc-text-faint">
              No call on this path was individually retained. Counts and timing
              above still cover every call.
            </div>
          )}
        </div>

        <div className="flex items-start gap-1.5 rounded border border-vsc-border-subtle bg-vsc-surface p-2 text-[11px] text-vsc-text-faint">
          <Info className="mt-0.5 h-3.5 w-3.5 shrink-0" />
          An aggregate is flat in time: it cannot say when, in what order, or
          under which parent call these ran. Exact ordering lives in Trace, on
          retained spans only.
        </div>
      </div>
    </aside>
  );
};

export default TelemetryView;
