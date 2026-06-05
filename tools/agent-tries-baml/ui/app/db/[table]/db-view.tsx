'use client';

import Link from 'next/link';

import type { Issue, LiveState } from '../../lib/data';
import { ago } from '../../lib/format';
import { usePolledState } from '../../live-dashboard';

const TONE: Record<string, string> = {
  queued: 'mute',
  running: 'ok',
  done: 'completed',
  failed: 'failed',
  deduping: 'partial',
  success: 'completed',
  partial: 'partial',
  open: 'partial',
  confirmed: 'timeout',
  approved: 'completed',
  fixing: 'partial',
  cursor: 'cursor',
  closed: 'mute',
  rejected: 'mute',
};
const ACTIVE = new Set(['running', 'deduping', 'syncing', 'building']);

const pill = (v: string) => (
  <span className={`statpill ${TONE[v] ?? ''}`}>{v}</span>
);

// ---- issue lifecycle stages (the columns of the Notion board, mirrored here) ----
const STAGE_ORDER = [
  'approved',
  'to cursor',
  'redraft',
  'not started',
  'fixed',
  'failed',
  'closed',
  'rejected',
];
const STAGE_TONE: Record<string, string> = {
  approved: 'completed',
  'to cursor': 'cursor',
  redraft: 'partial',
  'not started': 'timeout',
  fixed: 'completed',
  failed: 'failed',
  closed: 'mute',
  rejected: 'mute',
};
const STAGE_BLURB: Record<string, string> = {
  approved: 'you approved — dispatching a Cursor fix',
  'to cursor': 'a Cursor agent is working the fix',
  redraft: 'sent back — baml-redraft is rewriting from your comments',
  'not started': 'boarded, awaiting review',
  failed: 'dispatch failed',
};

/**
 * Map an issue to its lifecycle stage, mirroring the Notion board columns:
 * a dispatched issue (fixSlackTs set) is "to cursor", otherwise derived from status.
 * @param i - the issue
 * @returns the stage label
 */
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
      return i.status; // failed, closed, rejected, …
  }
}

/**
 * The reports / evidence links cell for an issue row.
 * @param i - the issue whose evidence to render
 */
function reportsCell(i: Issue) {
  const ev = (i.evidence ?? []).filter((e) => e.trophyId);
  if (ev.length === 0) return <span className="mute">—</span>;
  return ev.map((e, idx) => (
    <Link
      key={idx}
      href={`/runs/${e.trophyId}${e.call_index != null ? `?call=${e.call_index}` : ''}`}
      style={{ marginRight: 8 }}
    >
      report{e.call_index != null ? `·c${e.call_index}` : ''}
    </Link>
  ));
}

/**
 * Client component rendering a live table view of tasks, trophies, or issues. Tasks
 * and trophies render as a single table; issues are split into lifecycle-stage
 * sections (approved / to cursor / redraft / not started / …). Polls for fresh
 * state and lets the user pause/resume live updates.
 * @param table - which table to render ("tasks", "trophies", or "issues")
 * @param initial - the server-rendered LiveState used to seed live polling
 * @returns the selected table view
 */
export default function DbView({
  table,
  initial,
}: {
  table: string;
  initial: LiveState;
}) {
  const { s, live, setLive } = usePolledState(initial);
  const now = Date.now();

  const header = (count: number) => (
    <header className="page">
      <p style={{ marginBottom: 6 }}>
        <Link href="/" className="back-link">
          ← graph
        </Link>
      </p>
      <h1>
        {table} <span className={`pulse ${live ? 'on' : ''}`} />
      </h1>
      <p className="mute" style={{ fontSize: 13 }}>
        {count} {table === 'issues' ? 'issues' : 'rows'} ·{' '}
        <button className="linkbtn" onClick={() => setLive((v) => !v)}>
          {live ? 'live ⏸' : 'paused ▶'}
        </button>
      </p>
    </header>
  );

  // ---- issues: split into lifecycle-stage sections ----
  if (table === 'issues') {
    const groups: Record<string, Issue[]> = {};
    for (const i of s.issues) (groups[issueStage(i)] ??= []).push(i);
    const stages = [
      ...STAGE_ORDER.filter((st) => groups[st]?.length),
      ...Object.keys(groups)
        .filter((st) => !STAGE_ORDER.includes(st))
        .sort(),
    ];

    return (
      <div>
        {header(s.issues.length)}
        {s.issues.length === 0 ? <p className="mute">empty.</p> : null}
        {stages.map((st) => (
          <section key={st} className="stage" style={{ marginBottom: 30 }}>
            <h2 style={{ display: 'flex', alignItems: 'baseline', gap: 10 }}>
              <span className={`statpill ${STAGE_TONE[st] ?? ''}`}>{st}</span>
              <span className="mono mute" style={{ fontSize: 13 }}>
                {groups[st].length}
              </span>
              {STAGE_BLURB[st] ? (
                <span
                  className="mute"
                  style={{ fontSize: 13, fontWeight: 400 }}
                >
                  {STAGE_BLURB[st]}
                </span>
              ) : null}
            </h2>
            <table className="runtable">
              <thead>
                <tr>
                  <th>kind</th>
                  <th>title</th>
                  <th>reports</th>
                </tr>
              </thead>
              <tbody>
                {groups[st].map((i) => (
                  <tr key={i._id} className="runrow">
                    <td>{pill(i.kind)}</td>
                    <td>{i.title}</td>
                    <td className="mono">{reportsCell(i)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </section>
        ))}
      </div>
    );
  }

  // ---- tasks / trophies: single table ----
  let head: React.ReactNode = null;
  let rows: React.ReactNode[] = [];

  if (table === 'tasks') {
    const data = [...s.tasks].sort(
      (a, b) =>
        (ACTIVE.has(b.status) ? 1 : 0) - (ACTIVE.has(a.status) ? 1 : 0) ||
        b.createdAt - a.createdAt,
    );
    head = (
      <tr>
        <th>status</th>
        <th>source</th>
        <th>prompt</th>
        <th>report</th>
        <th>worker</th>
        <th className="r">age</th>
      </tr>
    );
    rows = data.map((t) => (
      <tr key={t._id} className="runrow">
        <td>
          {pill(t.status)}
          {ACTIVE.has(t.status) ? (
            <span className="pulse on" style={{ marginLeft: 6 }} />
          ) : null}
        </td>
        <td className="mono mute">{t.source}</td>
        <td>{t.prompt}</td>
        <td>
          {t.reportId ? (
            <Link href={`/runs/${t.reportId}`}>trophy →</Link>
          ) : (
            <span className="mute">—</span>
          )}
        </td>
        <td className="mono mute">{(t.claimedBy ?? '').slice(0, 16)}</td>
        <td className="r mono mute">
          {ago(now - (t.claimedAt ?? t.createdAt))}
        </td>
      </tr>
    ));
  } else {
    head = (
      <tr>
        <th>outcome</th>
        <th>task</th>
        <th>src</th>
        <th className="r">turns</th>
        <th className="r">api</th>
        <th className="r">tokens</th>
        <th className="r">cost</th>
        <th className="r">issues</th>
      </tr>
    );
    rows = s.runs.map((r) => (
      <tr key={r.trophyId} className="runrow">
        <td>{pill(r.outcome)}</td>
        <td>
          <Link href={`/runs/${r.trophyId}`}>{r.prompt}</Link>
        </td>
        <td className="mono mute">{r.source}</td>
        <td className="r mono">{r.turns ?? '-'}</td>
        <td className="r mono">{r.apiCalls ?? '-'}</td>
        <td className="r mono">{r.outputTokens ?? '-'}</td>
        <td className="r mono">${(r.costUsd ?? 0).toFixed(2)}</td>
        <td className="r mono">{r.findings || ''}</td>
      </tr>
    ));
  }

  return (
    <div>
      {header(rows.length)}
      {rows.length === 0 ? (
        <p className="mute">empty.</p>
      ) : (
        <table className="runtable">
          <thead>{head}</thead>
          <tbody>{rows}</tbody>
        </table>
      )}
    </div>
  );
}
