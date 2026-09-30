import { ArrowLeft } from 'lucide-react';
import Link from 'next/link';
import { notFound } from 'next/navigation';
import { FormattedText } from '@/components/code';
import { KindBadge, StatusBadge } from '@/components/issues/issue-status';
import { loadFeedback, loadIssuesById, loadNoIssueReason } from '@/lib/db';

export const revalidate = 0;

const GITHUB_ISSUE =
  /https:\/\/github\.com\/BoundaryML\/baml\/issues\/[1-9][0-9]*/;

/** One report as the public view has it: what the reporter wrote, and the issues it became. */
export default async function FeedbackPage({
  params,
}: {
  params: Promise<{ id: string }>;
}) {
  const { id } = await params;
  const report = await loadFeedback(id);
  if (!report) notFound();
  const [issues, noIssue] = await Promise.all([
    loadIssuesById(report.issue_ids),
    report.issue_ids.length
      ? Promise.resolve(null)
      : loadNoIssueReason(id, report.dataset),
  ]);
  const github = report.body.match(GITHUB_ISSUE)?.[0] ?? null;
  return (
    <main className="mx-auto max-w-4xl space-y-6 px-4 py-6">
      <Link
        className="inline-flex items-center gap-1 text-sm text-muted-foreground hover:text-foreground"
        href={issues[0] ? `/issues/${issues[0].id}` : '/feedback'}
      >
        <ArrowLeft className="h-4 w-4" />{' '}
        {issues[0] ? 'Back to the issue' : 'All reports'}
      </Link>
      <header className="space-y-2">
        <div className="font-mono text-xs text-muted-foreground">
          {report.id}
        </div>
        <h1 className="text-2xl font-semibold leading-tight break-words">
          {report.title}
        </h1>
        <p className="text-sm text-muted-foreground">
          Report from {report.source} · received{' '}
          {new Date(report.received_at).toLocaleString('en-US', {
            timeZone: 'UTC',
          })}{' '}
          UTC
          {report.toolchain && ` · reporter's toolchain ${report.toolchain}`}
          {github && (
            <>
              {' '}
              ·{' '}
              <a
                className="underline"
                href={github}
                rel="noreferrer"
                target="_blank"
              >
                GitHub issue
              </a>
            </>
          )}
        </p>
      </header>
      <section>
        <h2 className="mb-2 text-xs uppercase tracking-wide text-muted-foreground">
          Became
        </h2>
        {issues.length === 0 ? (
          <p className="text-sm text-muted-foreground">
            {noIssue ? (
              <>No issue: {noIssue}.</>
            ) : (
              'No issue yet: this report is untriaged, or was held for a human.'
            )}
          </p>
        ) : (
          <ul className="space-y-2">
            {issues.map((i) => (
              <li
                className="flex flex-wrap items-center gap-2 rounded-md border p-3 text-sm"
                key={i.id}
              >
                <KindBadge kind={i.kind} />
                <StatusBadge issue={i} />
                <Link
                  className="font-medium underline underline-offset-2"
                  href={`/issues/${i.id}`}
                >
                  {i.title}
                </Link>
              </li>
            ))}
          </ul>
        )}
      </section>
      <section>
        <h2 className="mb-2 text-xs uppercase tracking-wide text-muted-foreground">
          Report as written
        </h2>
        <div className="rounded-lg border bg-card p-4">
          <FormattedText text={report.body} />
        </div>
      </section>
    </main>
  );
}
