import { ArrowLeft, ExternalLink } from 'lucide-react';
import Link from 'next/link';
import { FormattedText } from '@/components/code';
import type { IssueEvent } from '@/lib/db';
import {
  investigationPrompt,
  sourceCitations,
  ticketSections,
} from '@/lib/investigation';
import { formatSeconds, stageInfo } from '@/lib/pipeline';
import type { Comment, Issue } from '@/lib/types';
import { InvestigationPrompt } from './investigation-prompt';
import {
  DifficultyBadge,
  KindBadge,
  StatusBadge,
  SubsystemBadge,
} from './issue-status';
import { PipelineStripLabeled } from './pipeline-strip';
import { ReproTabs } from './repro-tabs';

function Section({
  title,
  children,
}: {
  title: string;
  children: React.ReactNode;
}) {
  return (
    <section>
      <h2 className="text-xs uppercase tracking-wide text-muted-foreground mb-2">
        {title}
      </h2>
      {children}
    </section>
  );
}

const COMMENT_URL =
  /^https:\/\/github\.com\/BoundaryML\/baml\/issues\/[1-9][0-9]*#issuecomment-[0-9]+$/;

function CommentCard({ comment }: { comment: Comment }) {
  const url =
    comment.source === 'github' && comment.url && COMMENT_URL.test(comment.url)
      ? comment.url
      : null;
  return (
    <div className="min-w-0 rounded-md border p-3">
      <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
        <span>@{comment.author}</span>
        <span>·</span>
        <span>
          {new Date(comment.at).toLocaleString('en-US', { timeZone: 'UTC' })}{' '}
          UTC
        </span>
        {comment.source === 'github' && (
          <span className="rounded border px-1.5 py-0.5">from GitHub</span>
        )}
        {url && (
          <a
            className="underline"
            href={url}
            rel="noopener noreferrer"
            target="_blank"
          >
            View on GitHub
          </a>
        )}
      </div>
      <FormattedText text={comment.body} />
    </div>
  );
}

const PR_URL = /^https:\/\/github\.com\/BoundaryML\/baml\/pull\/[1-9][0-9]*$/;

/** The fix phase in one card; the artifacts (design doc, run) live on /issues/[id]/fix. */
function FixSummary({ issue }: { issue: Issue }) {
  const o = issue.outcome;
  const pr =
    o?.pr && PR_URL.test(o.pr)
      ? o.pr
      : 'pr' in issue.status && issue.status.pr && PR_URL.test(issue.status.pr)
        ? issue.status.pr
        : null;
  const state = o?.running
    ? `running: ${o.running} pass`
    : o?.kind === 'fixed'
      ? 'fix pushed'
      : o?.kind === 'hard'
        ? 'design doc written, no code change'
        : o?.kind === 'agent_stopped'
          ? 'agent stopped without a result'
          : 'not started';
  return (
    <div className="rounded-lg border bg-card p-4 text-sm">
      <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-1">
        <dt className="text-muted-foreground">state</dt>
        <dd>
          {state}
          {o?.timed_out ? ' (killed at budget)' : ''}
        </dd>
        {pr && (
          <>
            <dt className="text-muted-foreground">PR</dt>
            <dd>
              <a
                className="underline underline-offset-2"
                href={pr}
                rel="noreferrer"
                target="_blank"
              >
                {pr.replace('https://github.com/', '')}
              </a>
            </dd>
          </>
        )}
        {o && (
          <>
            <dt className="text-muted-foreground">agent</dt>
            <dd className="tabular-nums">
              {o.turns} turns · {formatSeconds(o.seconds)}
            </dd>
          </>
        )}
        {o?.reason && (
          <>
            <dt className="text-muted-foreground">reason</dt>
            <dd>{o.reason}</dd>
          </>
        )}
      </dl>
      <div className="mt-3 flex flex-wrap gap-x-4 gap-y-1 text-sm">
        <Link
          className="underline underline-offset-2"
          href={`/issues/${issue.id}/fix`}
        >
          {issue.design_doc ? 'Design doc and run details' : 'Run details'}
        </Link>
        <Link
          className="underline underline-offset-2"
          href={`/agents?issue_id=${encodeURIComponent(issue.id)}`}
        >
          Agent transcripts
        </Link>
      </div>
    </div>
  );
}

export function IssueDetail({
  issue,
  siblings = [],
  events = [],
}: {
  events?: IssueEvent[];
  issue: Issue;
  siblings?: Issue[];
}) {
  const sections = ticketSections(issue.description);
  const investigation = [...events]
    .reverse()
    .find((e) => e.kind === 'investigated');
  const evidence = investigation?.payload.evidence as
    | Record<string, unknown>
    | undefined;
  const revision =
    typeof evidence?.revision === 'string' ? evidence.revision : '';
  const locations = Array.isArray(evidence?.location)
    ? evidence.location.filter((v): v is string => typeof v === 'string')
    : [];
  const stages = stageInfo(issue);
  const o = issue.outcome;
  const st = issue.status;

  return (
    <div className="max-w-4xl mx-auto px-4 py-6">
      <Link
        className="inline-flex items-center gap-1 text-sm text-muted-foreground hover:text-foreground"
        href="/?view=all"
      >
        <ArrowLeft className="h-4 w-4" /> All issues
      </Link>

      <div className="mt-3 space-y-8">
        <div className="min-w-0 space-y-8">
          <div className="min-w-0">
            <div className="flex flex-wrap items-center gap-2">
              <KindBadge kind={issue.kind} />
              <span className="font-mono text-xs text-muted-foreground">
                {issue.id}
              </span>
            </div>
            <h1 className="mt-1 text-2xl font-semibold leading-tight break-words">
              Issue: {issue.title}
            </h1>
            <div className="mt-3 flex flex-wrap items-center gap-2 text-sm text-muted-foreground">
              <StatusBadge issue={issue} />
              <SubsystemBadge subsystem={issue.subsystem} />
              <DifficultyBadge difficulty={issue.difficulty} />
              <span>
                {issue.shepherd ? `shepherd @${issue.shepherd}` : 'unassigned'}
              </span>
              <span>·</span>
              <span>BAML version: {issue.version || 'unknown'}</span>
              <span>·</span>
              <span>
                {issue.feedback_ids.length} report
                {issue.feedback_ids.length === 1 ? '' : 's'}
              </span>
              {'pr' in issue.status &&
                issue.status.pr &&
                /^https:\/\/github\.com\/BoundaryML\/baml\/pull\/[1-9][0-9]*$/.test(
                  issue.status.pr,
                ) && (
                  <>
                    <span>·</span>
                    <a
                      className="inline-flex items-center gap-1 font-medium text-foreground underline"
                      href={issue.status.pr}
                      rel="noopener noreferrer"
                      target="_blank"
                    >
                      PR #{issue.status.pr.split('/').pop()}{' '}
                      <ExternalLink className="h-3.5 w-3.5" />
                    </a>
                  </>
                )}
            </div>
          </div>
          <nav
            aria-label="Original feedback"
            className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs"
          >
            <span className="text-muted-foreground">Feedback</span>
            {issue.feedback_ids.map((id, index) => (
              <Link
                className="underline underline-offset-2"
                href={`/feedback/${encodeURIComponent(id)}`}
                key={id}
                title={id}
              >
                Report {index + 1}
              </Link>
            ))}
            <InvestigationPrompt prompt={investigationPrompt(issue)} />
          </nav>
          {siblings.length > 0 && (
            <div className="rounded-md border border-dashed p-3 text-sm">
              <span className="text-muted-foreground">
                From the same report:{' '}
              </span>
              {siblings.map((s, i) => (
                <span key={s.id}>
                  {i > 0 && ', '}
                  <Link className="underline" href={`/issues/${s.id}`}>
                    {s.title}
                  </Link>{' '}
                  <KindBadge className="align-middle" kind={s.kind} />
                </span>
              ))}
            </div>
          )}

          <Section title="Brief description">
            <FormattedText text={sections.brief} />
          </Section>
          <Section title="What's going wrong">
            <FormattedText
              citations={sourceCitations(locations, revision)}
              text={
                sections.investigation ||
                'No source investigation recorded yet.'
              }
            />
          </Section>

          <Section title={`Repros (${issue.repros.length})`}>
            {issue.repros.length === 0 ? (
              <p className="text-sm text-muted-foreground">
                No repro attached.
              </p>
            ) : (
              <ReproTabs repros={issue.repros} />
            )}
          </Section>

          <details className="border-t pt-4">
            <summary className="cursor-pointer text-sm text-muted-foreground">
              Issue details and discussion
            </summary>
            <div className="mt-4 space-y-6">
              {sections.original && <FormattedText text={sections.original} />}
              {st.state === 'rejected' && (
                <Section title="Rejected">
                  <p className="text-sm">{st.reason}</p>
                </Section>
              )}
              {st.state === 'deferred' && (
                <Section title="Deferred">
                  <p className="text-sm">{st.reason}</p>
                  {st.workaround && (
                    <p className="mt-1 text-sm text-muted-foreground">
                      Workaround: {st.workaround}
                    </p>
                  )}
                </Section>
              )}

              {issue.resolution_plan && (
                <details>
                  <summary className="cursor-pointer text-sm text-muted-foreground">
                    {issue.kind === 'feature'
                      ? 'Proposed feature'
                      : 'Resolution plan'}
                  </summary>
                  <FormattedText text={issue.resolution_plan} />
                </details>
              )}

              {(issue.design_doc || o) && (
                <Section title="Fix">
                  <FixSummary issue={issue} />
                </Section>
              )}

              <Section title={`Comments (${issue.comments.length})`}>
                {issue.comments.length === 0 ? (
                  <p className="text-sm text-muted-foreground">No comments.</p>
                ) : (
                  <div className="space-y-3">
                    {issue.comments.map((c, i) => (
                      <CommentCard comment={c} key={c.url ?? i} />
                    ))}
                  </div>
                )}
              </Section>
            </div>
          </details>
        </div>

        <details className="border-t pt-4">
          <summary className="cursor-pointer text-sm text-muted-foreground">
            Pipeline
          </summary>
          <aside className="mt-4 space-y-6">
            <div className="rounded-lg border bg-card p-4">
              <div className="text-xs text-muted-foreground mb-2">Pipeline</div>
              <PipelineStripLabeled stages={stages} />
            </div>
          </aside>
        </details>
      </div>
    </div>
  );
}
