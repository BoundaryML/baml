'use client';

import Link from 'next/link';

import { DataTable, Td, Th, Tr } from '@/components/ui/data-table';
import { Dot, type DotTone } from '@/components/ui/dot';
import { InlineCode } from '@/components/ui/inline-code';
import { Pulse } from '@/components/ui/pulse';
import { StatPill } from '@/components/ui/stat-pill';

import { BottomTabs, usePolledState } from './live-dashboard';
import type { ChangelogEntry, Inflight, Issue, LiveState, WorkerRow } from './lib/data';
import { ago } from './lib/format';

// mirror of data.ts#issueStatusLabel (kept local: data.ts is server-side)
const issueLabel = (i: Pick<Issue, 'status' | 'fixSlackTs'>) =>
  i.fixSlackTs ? 'cursor' : i.status;

// heartbeat freshness → dot tone: ok <45s, warn <3m, mute beyond (stale)
function beatTone(ageMs: number): DotTone {
  if (ageMs < 45_000) return 'ok';
  if (ageMs < 180_000) return 'warn';
  return 'mute';
}

/** A single 3-up headline figure in the strip under the page header. */
function Headline({ n, label }: { n: number; label: string }) {
  return (
    <div className="flex-1 border border-border px-4 py-3">
      <div className="text-[34px] font-semibold leading-none tabular-nums">{n}</div>
      <div className="mt-1.5 font-mono text-[11px] uppercase tracking-[0.08em] text-muted-foreground">
        {label}
      </div>
    </div>
  );
}

/** Uppercase eyebrow header for a dispatched-work section. */
function SectionHead({ children }: { children: React.ReactNode }) {
  return (
    <div className="mb-2 mt-8 border-b border-border pb-1.5 font-mono text-[11px] uppercase tracking-[0.1em] text-muted-foreground">
      {children}
    </div>
  );
}

/** The live agents roster: one row per worker presence record. */
function Roster({ workers, now }: { workers: WorkerRow[]; now: number }) {
  if (workers.length === 0)
    return (
      <p className="text-muted-foreground">
        no agents reporting yet — workers appear here as they heartbeat.
      </p>
    );
  return (
    <DataTable>
      <thead>
        <tr>
          <Th> </Th>
          <Th>role</Th>
          <Th>worker</Th>
          <Th>status</Th>
          <Th>working on</Th>
          <Th align="right">since</Th>
          <Th align="right">beat</Th>
        </tr>
      </thead>
      <tbody>
        {workers.map((w) => {
          const age = now - w.lastHeartbeat;
          return (
            <Tr key={w._id}>
              <Td>
                <Dot tone={beatTone(age)} />
              </Td>
              <Td className="mono">{w.role}</Td>
              <Td className="mono text-muted-foreground">
                <span title={w.workerId}>
                  {w.workerId.length > 28 ? `${w.workerId.slice(0, 28)}…` : w.workerId}
                  {w.inferred ? ' (inferred)' : ''}
                </span>
              </Td>
              <Td>
                <StatPill status={w.status === 'busy' ? 'running' : 'completed'}>
                  {w.status}
                </StatPill>
              </Td>
              <Td cell="task">
                {w.label ? (
                  w.href ? (
                    <Link href={w.href} title={w.label}>
                      <InlineCode text={w.label} />
                    </Link>
                  ) : (
                    <InlineCode text={w.label} />
                  )
                ) : (
                  <span className="text-muted-foreground">—</span>
                )}
              </Td>
              <Td align="right" className="mono text-muted-foreground">
                {w.sinceMs != null ? ago(w.sinceMs) : ''}
              </Td>
              <Td align="right" className="mono text-muted-foreground">
                {ago(age)}
              </Td>
            </Tr>
          );
        })}
      </tbody>
    </DataTable>
  );
}

/** Bench runs currently claimed by a worker. */
function InflightSection({ inflight, now }: { inflight: Inflight[]; now: number }) {
  const tasks = inflight.filter((f) => f.kind === 'task');
  if (tasks.length === 0) return null;
  return (
    <>
      <SectionHead>bench runs in flight</SectionHead>
      <DataTable>
        <tbody>
          {tasks.map((f) => (
            <Tr key={f.id}>
              <Td cell="task">
                <Link href={`/tasks/${f.id}`} title={f.label}>
                  <InlineCode text={f.label} />
                </Link>
              </Td>
              <Td className="mono text-muted-foreground">{f.claimedBy ?? ''}</Td>
              <Td align="right" className="mono text-muted-foreground">
                {ago(f.sinceMs)}
              </Td>
            </Tr>
          ))}
        </tbody>
      </DataTable>
    </>
  );
}

/** Issues dispatched to Cursor cloud agents (status fixing / handed off). */
function CursorSection({ issues }: { issues: Issue[] }) {
  const dispatched = issues.filter((i) => ['cursor', 'fixing'].includes(issueLabel(i)));
  if (dispatched.length === 0) return null;
  return (
    <>
      <SectionHead>cursor fix agents</SectionHead>
      <DataTable>
        <tbody>
          {dispatched.map((i) => (
            <Tr key={i._id}>
              <Td cell="task">
                <Link href={`/issues/${i._id}`} title={i.title}>
                  <InlineCode text={i.title} />
                </Link>
              </Td>
              <Td className="mono text-muted-foreground">{i.kind}</Td>
              <Td align="right">
                <StatPill status="running">{issueLabel(i)}</StatPill>
              </Td>
            </Tr>
          ))}
        </tbody>
      </DataTable>
    </>
  );
}

/** Changelog entries being generated right now (or queued for it). */
function ChangelogSection({ entries, now }: { entries: ChangelogEntry[]; now: number }) {
  const active = entries.filter((e) => ['queued', 'generating'].includes(e.status));
  if (active.length === 0) return null;
  return (
    <>
      <SectionHead>changelog generation</SectionHead>
      <DataTable>
        <tbody>
          {active.map((e) => (
            <Tr key={e._id}>
              <Td cell="task">
                <Link href="/changelog">
                  <InlineCode text={`${e.version} (${e.channel})`} />
                </Link>
              </Td>
              <Td align="right">
                <StatPill status={e.status === 'generating' ? 'running' : 'partial'}>
                  {e.status}
                </StatPill>
              </Td>
              <Td align="right" className="mono text-muted-foreground">
                {ago(now - (e.updatedAt ?? e.createdAt))}
              </Td>
            </Tr>
          ))}
        </tbody>
      </DataTable>
    </>
  );
}

/**
 * The agents.boundaryml.com landing view: a live roster of every agent in the
 * monolith (presence heartbeats), the work currently dispatched (bench runs,
 * cursor fixes, changelog generation), and the runs/issues tabs below.
 * @param initial - the server-rendered LiveState used to seed live polling
 */
export default function AgentsView({ initial }: { initial: LiveState }) {
  const { s, live, setLive } = usePolledState(initial);
  const now = Date.now();
  const workers = s.workers ?? [];

  return (
    <div>
      <header className="mb-7 max-[640px]:mb-5">
        <h1 className="mb-1.5 text-[28px] font-medium tracking-[-0.01em] max-[640px]:text-[22px]">
          agents <Pulse on={live} />
        </h1>
        <p className="text-[15px] leading-[1.55] text-muted-foreground">
          Every agent running in the BAML monolith: bench workers, dedup,
          changelog generation, notion sync, and cursor fix dispatches.
        </p>
        <p className="text-[13px] text-muted-foreground">
          ${s.totals.costUsd.toFixed(2)} est ·{' '}
          <button
            className="cursor-pointer border-0 bg-transparent p-0 text-link"
            onClick={() => setLive((v) => !v)}
          >
            {live ? 'live — pause' : 'paused — resume'}
          </button>{' '}
          · {s.generatedAt}
        </p>
      </header>

      <div className="mb-7 flex gap-2.5 max-[640px]:flex-col">
        <Headline n={s.agents.online ?? 0} label="agents online" />
        <Headline n={s.agents.busy ?? 0} label="busy now" />
        <Headline n={s.agents.activeTasks ?? 0} label="active tasks" />
      </div>

      <SectionHead>roster</SectionHead>
      <Roster workers={workers} now={now} />

      <InflightSection inflight={s.inflight} now={now} />
      <CursorSection issues={s.issues} />
      <ChangelogSection entries={s.changelog ?? []} now={now} />

      <BottomTabs s={s} now={now} />
    </div>
  );
}
