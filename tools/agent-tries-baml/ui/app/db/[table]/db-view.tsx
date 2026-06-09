'use client';

import Link from 'next/link';

import { DataTable, Td, Th, Tr } from '@/components/ui/data-table';
import { InlineCode } from '@/components/ui/inline-code';
import { Pulse } from '@/components/ui/pulse';
import { StatPill, type StatPillTone } from '@/components/ui/stat-pill';

import type { Issue, LiveState } from '../../lib/data';
import { ago } from '../../lib/format';
import { usePolledState } from '../../live-dashboard';

// raw row status → pill tone (statuses without a mapping render uncolored)
const TONE: Record<string, StatPillTone> = {
  running: 'success',
  done: 'success',
  failed: 'destructive',
  deduping: 'mute',
  comparing: 'link',
  success: 'success',
  partial: 'mute',
  open: 'mute',
  confirmed: 'mute',
  approved: 'success',
  fixing: 'mute',
  cursor: 'link',
  closed: 'mute',
  rejected: 'mute',
};
const ACTIVE = new Set(['running', 'deduping', 'syncing', 'building']);

const pill = (v: string) => <StatPill tone={TONE[v] ?? 'default'}>{v}</StatPill>;

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
const STAGE_TONE: Record<string, StatPillTone> = {
  approved: 'success',
  'to cursor': 'link',
  redraft: 'mute',
  'not started': 'mute',
  fixed: 'success',
  failed: 'destructive',
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
  if (ev.length === 0) return <span className="text-muted-foreground">—</span>;
  return ev.map((e, idx) => (
    <Link
      key={idx}
      href={`/runs/${e.trophyId}${e.call_index != null ? `?call=${e.call_index}` : ''}`}
      className="mr-2"
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
    <header className="mb-9 max-[640px]:mb-6">
      <p className="mb-1.5">
        <Link
          href="/"
          className="text-sm text-muted-foreground no-underline hover:text-link"
        >
          ← graph
        </Link>
      </p>
      <h1 className="mb-1.5 text-[28px] font-medium tracking-[-0.01em] max-[640px]:text-[22px]">
        {table} <Pulse on={live} />
      </h1>
      <p className="text-[13px] text-muted-foreground">
        {count} {table === 'issues' ? 'issues' : 'rows'} ·{' '}
        <button
          className="cursor-pointer border-0 bg-transparent p-0 text-link"
          onClick={() => setLive((v) => !v)}
        >
          {live ? 'live — pause' : 'paused — resume'}
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
        {s.issues.length === 0 ? (
          <p className="text-muted-foreground">empty.</p>
        ) : null}
        {stages.map((st) => (
          <section key={st} className="mb-[30px]">
            <h2 className="mb-[18px] flex items-baseline gap-2.5 text-[13px] font-medium uppercase tracking-[0.06em] text-muted-foreground">
              <StatPill tone={STAGE_TONE[st] ?? 'default'}>{st}</StatPill>
              <span className="mono text-[13px]">{groups[st].length}</span>
              {STAGE_BLURB[st] ? (
                <span className="font-normal">{STAGE_BLURB[st]}</span>
              ) : null}
            </h2>
            <DataTable>
              <thead>
                <tr>
                  <Th>kind</Th>
                  <Th>title</Th>
                  <Th>reports</Th>
                </tr>
              </thead>
              <tbody>
                {groups[st].map((i) => (
                  <Tr key={i._id}>
                    <Td>{pill(i.kind)}</Td>
                    <Td>
                      <InlineCode text={i.title} />
                    </Td>
                    <Td className="mono">{reportsCell(i)}</Td>
                  </Tr>
                ))}
              </tbody>
            </DataTable>
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
        <Th>status</Th>
        <Th>source</Th>
        <Th>prompt</Th>
        <Th>report</Th>
        <Th>worker</Th>
        <Th align="right">age</Th>
      </tr>
    );
    rows = data.map((t) => (
      <Tr key={t._id}>
        <Td>
          {pill(t.status)}
          {ACTIVE.has(t.status) ? <Pulse on className="ml-1.5" /> : null}
        </Td>
        <Td className="mono text-muted-foreground">{t.source}</Td>
        <Td>
          <InlineCode text={t.prompt} />
        </Td>
        <Td>
          {t.reportId ? (
            <Link href={`/runs/${t.reportId}`}>trophy →</Link>
          ) : (
            <span className="text-muted-foreground">—</span>
          )}
        </Td>
        <Td className="mono text-muted-foreground">
          {(t.claimedBy ?? '').slice(0, 16)}
        </Td>
        <Td align="right" className="mono text-muted-foreground">
          {ago(now - (t.claimedAt ?? t.createdAt))}
        </Td>
      </Tr>
    ));
  } else if (table === 'cohorts') {
    const data = [...s.cohorts].sort(
      (a, b) =>
        (a.status === 'comparing' ? 1 : 0) - (b.status === 'comparing' ? 1 : 0) ||
        b.createdAt - a.createdAt,
    );
    head = (
      <tr>
        <Th>status</Th>
        <Th>prompt</Th>
        <Th>branches</Th>
        <Th align="right">variants</Th>
        <Th>report</Th>
        <Th align="right">age</Th>
      </tr>
    );
    rows = data.map((c) => (
      <Tr key={c._id}>
        <Td>
          {pill(c.status)}
          {c.status === 'comparing' ? <Pulse on className="ml-1.5" /> : null}
        </Td>
        <Td>
          <Link href={`/cohorts/${c._id}`}>
            <InlineCode text={c.prompt} />
          </Link>
        </Td>
        <Td className="mono text-muted-foreground">
          {(c.skillRefs ?? []).join(', ')}
        </Td>
        <Td align="right" className="mono">
          {(c.memberTaskIds ?? []).length}
        </Td>
        <Td>
          {c.reportTrophyId ? (
            <Link href={`/runs/${c.reportTrophyId}`}>comparison →</Link>
          ) : (
            <span className="text-muted-foreground">—</span>
          )}
        </Td>
        <Td align="right" className="mono text-muted-foreground">
          {ago(now - c.createdAt)}
        </Td>
      </Tr>
    ));
  } else {
    head = (
      <tr>
        <Th>outcome</Th>
        <Th>task</Th>
        <Th>src</Th>
        <Th align="right">turns</Th>
        <Th align="right">api</Th>
        <Th align="right">tokens</Th>
        <Th align="right">cost</Th>
        <Th align="right">issues</Th>
      </tr>
    );
    rows = s.runs.map((r) => (
      <Tr key={r.trophyId}>
        <Td>{pill(r.outcome)}</Td>
        <Td>
          <Link href={`/runs/${r.trophyId}`}>
            <InlineCode text={r.prompt} />
          </Link>
        </Td>
        <Td className="mono text-muted-foreground">{r.source}</Td>
        <Td align="right" className="mono">{r.turns ?? '-'}</Td>
        <Td align="right" className="mono">{r.apiCalls ?? '-'}</Td>
        <Td align="right" className="mono">{r.outputTokens ?? '-'}</Td>
        <Td align="right" className="mono">${(r.costUsd ?? 0).toFixed(2)}</Td>
        <Td align="right" className="mono">{r.findings || ''}</Td>
      </Tr>
    ));
  }

  return (
    <div>
      {header(rows.length)}
      {rows.length === 0 ? (
        <p className="text-muted-foreground">empty.</p>
      ) : (
        <DataTable>
          <thead>{head}</thead>
          <tbody>{rows}</tbody>
        </DataTable>
      )}
    </div>
  );
}
