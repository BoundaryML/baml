'use client';

import Link from 'next/link';

import { DataTable, Td, Th, Tr } from '@/components/ui/data-table';
import { InlineCode } from '@/components/ui/inline-code';
import { StatPill, type StatPillTone } from '@/components/ui/stat-pill';

import type { Issue, LiveState } from './lib/data';
import { ago } from './lib/format';

// raw row status → pill tone (statuses without a mapping render uncolored)
const TONE: Record<string, StatPillTone> = {
  approved: 'success',
  closed: 'mute',
  confirmed: 'mute',
  cursor: 'link',
  deduping: 'mute',
  done: 'success',
  failed: 'destructive',
  fixing: 'mute',
  open: 'mute',
  partial: 'mute',
  queued: 'mute',
  rejected: 'mute',
  running: 'success',
  success: 'success',
};
const ACTIVE = new Set(['running', 'deduping', 'syncing', 'building']);
const pill = (v: string) => <StatPill tone={TONE[v] ?? 'default'}>{v}</StatPill>;

// Mirror of the issue lifecycle mapping used by the /db/issues view, so the
// approved / to-cursor / redraft nodes can show their own filtered slice.
function issueStage(i: Issue): string {
  if (i.fixSlackTs) return 'to cursor';
  switch (i.status) {
    case 'redraft':
    case 'redrafting':
      return 'redraft';
    case 'approved':
      return 'approved';
    case 'fixing':
      return 'to cursor';
    case 'open':
    case 'confirmed':
      return 'not started';
    default:
      return i.status;
  }
}

// Graph node id -> which table it reads and (for issue sub-nodes) which stage.
const NODE_MAP: Record<
  string,
  { title: string; table: 'tasks' | 'trophies' | 'issues'; stage?: string }
> = {
  approved: { stage: 'approved', table: 'issues', title: 'approved' },
  issues: { table: 'issues', title: 'issues' },
  redraft: { stage: 'redraft', table: 'issues', title: 'redraft' },
  tasks: { table: 'tasks', title: 'tasks' },
  tocursor: { stage: 'to cursor', table: 'issues', title: 'to cursor' },
  trophies: { table: 'trophies', title: 'trophies' },
};

/** Whether a graph node id has a data panel (i.e. is a db-backed node). */
export function nodeHasPanel(id: string): boolean {
  return id in NODE_MAP;
}

/**
 * Right-side data panel for a clicked graph node, shown in fullscreen instead of
 * navigating to the full /db view. Renders a compact live table for the node's
 * table (tasks / trophies / issues), filtered to a lifecycle stage for the
 * approved / to-cursor / redraft sub-nodes.
 * @param nodeId - the tapped graph node id
 * @param s - the live state supplying rows
 * @param onClose - dismisses the panel
 */
export default function NodePanel({
  nodeId,
  s,
  onClose,
}: {
  nodeId: string;
  s: LiveState;
  onClose: () => void;
}) {
  const meta = NODE_MAP[nodeId];
  if (!meta) return null;
  const now = Date.now();

  const issues = meta.stage
    ? s.issues.filter((i) => issueStage(i) === meta.stage)
    : s.issues;
  const count =
    meta.table === 'tasks'
      ? s.tasks.length
      : meta.table === 'trophies'
        ? s.runs.length
        : issues.length;

  const empty = <p className="text-muted-foreground">empty.</p>;
  const body = (() => {
    if (meta.table === 'tasks') {
      const data = [...s.tasks].sort(
        (a, b) =>
          (ACTIVE.has(b.status) ? 1 : 0) - (ACTIVE.has(a.status) ? 1 : 0) ||
          b.createdAt - a.createdAt,
      );
      if (data.length === 0) return empty;
      return (
        <DataTable className="text-sm">
          <thead>
            <tr>
              <Th>status</Th>
              <Th>prompt</Th>
              <Th align="right">age</Th>
            </tr>
          </thead>
          <tbody>
            {data.map((t) => (
              <Tr key={t._id}>
                <Td>{pill(t.status)}</Td>
                <Td>
                  {t.reportId ? (
                    <Link href={`/runs/${t.reportId}`}>
                      <InlineCode text={t.prompt} />
                    </Link>
                  ) : (
                    <InlineCode text={t.prompt} />
                  )}
                </Td>
                <Td align="right" className="mono text-muted-foreground">
                  {ago(now - (t.claimedAt ?? t.createdAt))}
                </Td>
              </Tr>
            ))}
          </tbody>
        </DataTable>
      );
    }
    if (meta.table === 'trophies') {
      if (s.runs.length === 0) return empty;
      return (
        <DataTable className="text-sm">
          <thead>
            <tr>
              <Th>outcome</Th>
              <Th>task</Th>
              <Th align="right">cost</Th>
            </tr>
          </thead>
          <tbody>
            {s.runs.map((r) => (
              <Tr key={r.trophyId}>
                <Td>{pill(r.outcome)}</Td>
                <Td>
                  <Link href={`/runs/${r.trophyId}`}>
                    <InlineCode text={r.prompt} />
                  </Link>
                </Td>
                <Td align="right" className="mono">
                  ${(r.costUsd ?? 0).toFixed(2)}
                </Td>
              </Tr>
            ))}
          </tbody>
        </DataTable>
      );
    }
    if (issues.length === 0) return empty;
    return (
      <DataTable className="text-sm">
        <thead>
          <tr>
            <Th>kind</Th>
            <Th>title</Th>
          </tr>
        </thead>
        <tbody>
          {issues.map((i) => (
            <Tr key={i._id}>
              <Td>{pill(i.kind)}</Td>
              <Td>
                <InlineCode text={i.title} />
              </Td>
            </Tr>
          ))}
        </tbody>
      </DataTable>
    );
  })();

  return (
    <aside className="fixed inset-y-0 right-0 z-[70] flex w-[380px] flex-col border-l border-border bg-background shadow-[-2px_0_14px_rgba(0,0,0,0.07)]">
      <div className="flex items-baseline justify-between gap-3 border-b border-border px-5 pt-[18px] pb-3">
        <div>
          <span className="text-lg font-medium tracking-[-0.01em]">
            {meta.title}
          </span>{' '}
          <span className="mono text-xs text-muted-foreground">{count}</span>
        </div>
        <button
          aria-label="Close panel"
          className="cursor-pointer border-0 bg-transparent text-[22px] leading-none text-muted-foreground hover:text-foreground"
          onClick={onClose}
          type="button"
        >
          ×
        </button>
      </div>
      <div className="flex-1 overflow-y-auto px-5 py-3">{body}</div>
      <div className="border-t border-border px-5 py-3 text-[13px]">
        <Link href={`/db/${meta.table}`}>open full {meta.table} view →</Link>
      </div>
    </aside>
  );
}
