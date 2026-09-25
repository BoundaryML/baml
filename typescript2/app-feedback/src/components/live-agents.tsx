'use client';
import { ArrowUpRight, Clock, Terminal } from 'lucide-react';
import Link from 'next/link';
import { useEffect, useState } from 'react';
import { FormattedText } from '@/components/code';
import { assessmentIssue } from '@/lib/assessment-issue';
import { AgentResult } from './agent-result';
import { AgentTerminal } from './agent-terminal';

type Agent = {
  id: string;
  stage: string;
  status: string;
  started_at: number;
  reason: string | null;
  issue_ids: string[];
  pr_urls?: string[];
  workspace: string;
};
type Event = {
  role: string;
  type: string;
  text: string;
  name: string | null;
  continuation: boolean;
};
export function LiveAgents({
  id,
  issueId,
  pr,
  dataset = 'live',
}: {
  id?: string;
  issueId?: string;
  pr?: string;
  dataset?: 'live' | 'eval';
}) {
  const [agents, setAgents] = useState<Agent[]>([]);
  const [meta, setMeta] = useState<Agent | null>(null);
  const [events, setEvents] = useState<Event[]>([]);
  const [artifact, setArtifact] = useState<{
    title: string;
    body: string;
    status: string;
    url: string;
  } | null>(null);
  const [error, setError] = useState('');
  const [loading, setLoading] = useState(true);
  const [needsLogin, setNeedsLogin] = useState(false);
  const [filter, setFilter] = useState('all');
  useEffect(() => {
    let stopped = false;
    let timer: ReturnType<typeof setTimeout>;
    let offset = 0;
    const controller = new AbortController();
    setEvents([]);
    setMeta(null);
    setArtifact(null);
    setLoading(true);
    async function poll() {
      let delay = 2000;
      try {
        const query = new URLSearchParams({
          dataset,
          id: id ?? '',
          issue_id: issueId ?? '',
          offset: String(offset),
          pr: pr ?? '',
        });
        const response = await fetch(`/api/agents?${query}`, {
          cache: 'no-store',
          signal: controller.signal,
        });
        const data = await response.json();
        setNeedsLogin(response.status === 401);
        if (response.status === 401) delay = 30000;
        if (!response.ok || data.error)
          throw new Error(data.error ?? 'Unable to load transcripts');
        if (stopped) return;
        setError('');
        if (id) {
          setMeta(data.meta);
          setArtifact(data.artifact ?? null);
          setEvents((old) => [...old, ...data.events]);
          offset = data.offset;
          if (data.more) delay = 0;
        } else setAgents(data);
      } catch (e) {
        if (!stopped)
          setError(
            e instanceof Error ? e.message : 'Unable to load transcripts',
          );
      } finally {
        if (!stopped) {
          setLoading(false);
          timer = setTimeout(poll, delay);
        }
      }
    }
    void poll();
    return () => {
      stopped = true;
      controller.abort();
      clearTimeout(timer);
    };
  }, [id, issueId, pr, dataset]);
  const isAssessment = meta?.stage === 'gauge' || meta?.stage === 'gauging';
  const assessed = isAssessment ? assessmentIssue(events) : null;
  const issueLinks = isAssessment
    ? assessed
      ? [assessed]
      : []
    : (meta?.issue_ids ?? []).map((id) => ({ id, title: 'View issue' }));
  const shown = agents
    .filter(
      (a) =>
        filter === 'all' ||
        (filter === 'running'
          ? a.status === 'running'
          : a.status !== 'running'),
    )
    .sort((a, b) => b.started_at - a.started_at);
  return (
    <section className="space-y-5">
      {error && (
        <div
          aria-live="polite"
          className="rounded-lg border bg-muted/30 px-4 py-3 text-sm"
        >
          {error}{' '}
          {needsLogin && (
            <a className="ml-2 font-medium underline" href="/api/auth/github">
              Sign in
            </a>
          )}
        </div>
      )}
      {!id && (
        <>
          {!loading && !error && agents.length > 0 && (
            <div className="flex items-center gap-1 border-b pb-3">
              {[
                ['all', 'All sessions'],
                ['running', 'Running'],
                ['finished', 'Finished'],
              ].map(([value, label]) => (
                <button
                  className={`rounded-md px-3 py-1.5 text-xs ${filter === value ? 'bg-foreground text-background' : 'text-muted-foreground hover:bg-muted'}`}
                  key={value}
                  onClick={() => setFilter(value)}
                  type="button"
                >
                  {label}
                </button>
              ))}
              <span className="ml-auto text-xs text-muted-foreground">
                {shown.length} sessions
              </span>
            </div>
          )}
          {loading && (
            <p className="py-6 text-sm text-muted-foreground">
              Loading sessions…
            </p>
          )}
          {!loading && !shown.length && !error && (
            <p className="rounded-lg border border-dashed px-4 py-8 text-center text-sm text-muted-foreground">
              {filter === 'running' ? 'No agents running.' : 'No sessions yet.'}
            </p>
          )}
          <div className="divide-y overflow-hidden rounded-xl border empty:hidden">
            {shown.map((a) => (
              <Link
                className="group flex items-center gap-4 bg-card px-4 py-4 transition-colors hover:bg-muted/50"
                href={`/agents/${a.id}?dataset=${dataset}`}
                key={a.id}
              >
                <span className="rounded-lg border bg-muted/40 p-2.5 text-muted-foreground">
                  <Terminal size={17} />
                </span>
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-3">
                    <h3 className="text-sm font-medium">
                      {stageLabel(a.stage)}
                    </h3>
                    <SessionStatus status={a.status} />
                  </div>
                  <p className="mt-1.5 text-xs text-muted-foreground">
                    {new Date(a.started_at * 1000).toLocaleString(undefined, {
                      day: 'numeric',
                      hour: 'numeric',
                      minute: '2-digit',
                      month: 'short',
                    })}
                    {a.pr_urls?.length
                      ? ` · PR #${a.pr_urls[0].split('/').pop()}`
                      : ''}
                  </p>
                </div>
                <ArrowUpRight
                  className="text-muted-foreground transition-transform group-hover:-translate-y-0.5 group-hover:translate-x-0.5"
                  size={16}
                />
              </Link>
            ))}
          </div>
        </>
      )}
      {id && (
        <>
          <header className="flex flex-wrap items-start justify-between gap-4">
            <div>
              <div className="mb-2 flex items-center gap-2 text-xs text-muted-foreground">
                <Terminal size={14} />
                Agent session
              </div>
              <h1 className="text-2xl font-semibold tracking-tight">
                {meta ? stageLabel(meta.stage) : 'Loading session…'}
              </h1>
              {meta && (
                <p className="mt-2 flex items-center gap-1.5 text-xs text-muted-foreground">
                  <Clock size={12} />
                  {new Date(meta.started_at * 1000).toLocaleString()} ·{' '}
                  {events.length} events
                </p>
              )}
            </div>
            {meta && <SessionStatus status={meta.status} />}
          </header>
          {meta?.reason && (
            <output className="rounded-lg border border-amber-500/30 bg-amber-500/5 px-4 py-3 text-sm">
              {failureLabel(meta.reason)}
            </output>
          )}
          {issueLinks.length > 0 && (
            <nav className="flex flex-wrap gap-3 text-sm">
              {issueLinks.map((issue) => (
                <Link
                  className="inline-flex items-center gap-1 underline"
                  href={`/issues/${encodeURIComponent(issue.id)}`}
                  key={issue.id}
                >
                  {issue.title}
                  <ArrowUpRight size={13} />
                </Link>
              ))}
            </nav>
          )}
          <AgentResult events={events} />
          {artifact && (
            <section className="rounded-xl border bg-card p-5">
              <h2 className="text-sm font-semibold">{artifact.title}</h2>
              <FormattedText text={artifact.body} />
              <Link
                className="mt-3 inline-block text-sm underline"
                href={artifact.url}
              >
                View proposed fix
              </Link>
            </section>
          )}
          {!needsLogin && (
            <AgentTerminal events={events} status={meta?.status} />
          )}
        </>
      )}
    </section>
  );
}

function stageLabel(stage: string) {
  const labels: Record<string, string> = {
    babysit: 'Review PR checks',
    chat: 'Conversation',
    design: 'Plan fix',
    fix: 'Implement fix',
    gauge: 'Assess difficulty',
    gauging: 'Assess difficulty',
    investigate: 'Investigate issue',
    play: 'Try BAML',
    triage: 'Triage feedback',
  };
  return (
    labels[stage] ??
    stage.replace(/_/g, ' ').replace(/^./, (c) => c.toUpperCase())
  );
}
function failureLabel(reason: string) {
  return (
    (
      {
        agent_interrupted_or_no_result: 'The session ended without a result.',
        error_max_turns: 'The agent reached its turn limit.',
      } as Record<string, string>
    )[reason] ?? reason.replace(/_/g, ' ')
  );
}
function SessionStatus({ status }: { status: string }) {
  const color =
    status === 'running'
      ? 'bg-emerald-500'
      : status === 'completed'
        ? 'bg-slate-400'
        : 'bg-amber-500';
  return (
    <span className="inline-flex items-center gap-1.5 rounded-full border px-2.5 py-1 text-[11px] capitalize">
      <span
        className={`h-1.5 w-1.5 rounded-full ${color} ${status === 'running' ? 'animate-pulse' : ''}`}
      />
      {status === 'completed' ? 'Finished' : status}
    </span>
  );
}
