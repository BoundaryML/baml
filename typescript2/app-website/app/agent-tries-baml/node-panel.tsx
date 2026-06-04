'use client';

import Link from 'next/link';

import type { Issue, LiveState } from './lib/data';
import { ago } from './lib/format';

const TONE: Record<string, string> = {
  approved: 'completed',
  closed: 'mute',
  confirmed: 'timeout',
  cursor: 'cursor',
  deduping: 'partial',
  done: 'completed',
  failed: 'failed',
  fixing: 'partial',
  open: 'partial',
  partial: 'partial',
  queued: 'mute',
  rejected: 'mute',
  running: 'ok',
  success: 'completed',
};
const ACTIVE = new Set(['running', 'deduping', 'syncing', 'building']);
const pill = (v: string) => (
  <span className={`statpill ${TONE[v] ?? ''}`}>{v}</span>
);

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

  const body = (() => {
    if (meta.table === 'tasks') {
      const data = [...s.tasks].sort(
        (a, b) =>
          (ACTIVE.has(b.status) ? 1 : 0) - (ACTIVE.has(a.status) ? 1 : 0) ||
          b.createdAt - a.createdAt,
      );
      if (data.length === 0) return <p className="mute">empty.</p>;
      return (
        <table className="runtable">
          <thead>
            <tr>
              <th>status</th>
              <th>prompt</th>
              <th className="r">age</th>
            </tr>
          </thead>
          <tbody>
            {data.map((t) => (
              <tr className="runrow" key={t._id}>
                <td>{pill(t.status)}</td>
                <td>
                  {t.reportId ? (
                    <Link href={`/agent-tries-baml/runs/${t.reportId}`}>
                      {t.prompt}
                    </Link>
                  ) : (
                    t.prompt
                  )}
                </td>
                <td className="r mono mute">
                  {ago(now - (t.claimedAt ?? t.createdAt))}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      );
    }
    if (meta.table === 'trophies') {
      if (s.runs.length === 0) return <p className="mute">empty.</p>;
      return (
        <table className="runtable">
          <thead>
            <tr>
              <th>outcome</th>
              <th>task</th>
              <th className="r">cost</th>
            </tr>
          </thead>
          <tbody>
            {s.runs.map((r) => (
              <tr className="runrow" key={r.trophyId}>
                <td>{pill(r.outcome)}</td>
                <td>
                  <Link href={`/agent-tries-baml/runs/${r.trophyId}`}>
                    {r.prompt}
                  </Link>
                </td>
                <td className="r mono">${(r.costUsd ?? 0).toFixed(2)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      );
    }
    if (issues.length === 0) return <p className="mute">empty.</p>;
    return (
      <table className="runtable">
        <thead>
          <tr>
            <th>kind</th>
            <th>title</th>
          </tr>
        </thead>
        <tbody>
          {issues.map((i) => (
            <tr className="runrow" key={i._id}>
              <td>{pill(i.kind)}</td>
              <td>{i.title}</td>
            </tr>
          ))}
        </tbody>
      </table>
    );
  })();

  return (
    <aside className="nodepanel">
      <div className="nodepanel-head">
        <div>
          <span className="nodepanel-title">{meta.title}</span>{' '}
          <span className="mono mute" style={{ fontSize: 12 }}>
            {count}
          </span>
        </div>
        <button
          aria-label="Close panel"
          className="nodepanel-x"
          onClick={onClose}
          type="button"
        >
          ×
        </button>
      </div>
      <div className="nodepanel-body">{body}</div>
      <div className="nodepanel-foot">
        <Link href={`/agent-tries-baml/db/${meta.table}`}>
          open full {meta.table} view →
        </Link>
      </div>
    </aside>
  );
}
