import Link from 'next/link';
import { notFound } from 'next/navigation';
import { FeedbackPanel } from '@/components/feedback-panel';
import { Activity } from '@/components/issues/activity';
import { IssueActions } from '@/components/issues/issue-actions';
import { IssueDetail } from '@/components/issues/issue-detail';
import { LiveUpdates } from '@/components/issues/live-updates';
import { LiveAgents } from '@/components/live-agents';
import { currentUser } from '@/lib/auth';
import { loadIssue, loadIssueEvents, loadSiblingIssues } from '@/lib/db';

// On demand, never prerendered at build: the data source is decided by the
// server's environment. revalidate = 0 keeps the route dynamic while the
// fetch-level cache in db.ts (REVALIDATE_S) still bounds the reads.
export const revalidate = 0;

export default async function IssuePage({
  params,
}: {
  params: Promise<{ id: string }>;
}) {
  const { id } = await params;
  const issue = await loadIssue(id);
  if (!issue) notFound();
  const user = await currentUser();
  const [events, siblings] = await Promise.all([
    loadIssueEvents(id, issue.dataset, issue.feedback_ids),
    loadSiblingIssues(issue),
  ]);
  const pr = 'pr' in issue.status ? issue.status.pr : null;
  const number = pr?.match(
    /^https:\/\/github\.com\/BoundaryML\/baml\/pull\/([1-9][0-9]{0,9})$/,
  )?.[1];
  return (
    <>
      <LiveUpdates />
      <FeedbackPanel ids={issue.feedback_ids}>
        <IssueDetail events={events} issue={issue} siblings={siblings} />
      </FeedbackPanel>
      <section className="mx-auto min-w-0 max-w-4xl px-4 py-6">
        <Activity dataset={issue.dataset} events={events} issueId={issue.id} />
      </section>
      <section className="mx-auto max-w-4xl space-y-3 px-4 py-6">
        <h2 className="font-semibold">Agent sessions</h2>
        <LiveAgents dataset={issue.dataset} issueId={issue.id} />
      </section>
      <div className="mx-auto max-w-4xl px-4">
        <IssueActions dataset={issue.dataset ?? 'live'} id={id} user={user} />
        {number && (
          <Link href={`/prs/${number}?dataset=${issue.dataset}`}>
            PR #{number} babysitter activity
          </Link>
        )}
      </div>
    </>
  );
}
