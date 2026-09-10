import { notFound } from 'next/navigation';
import { Activity } from '@/components/issues/activity';
import { ApproveIssue } from '@/components/issues/approve-issue';
import { IssueDetail } from '@/components/issues/issue-detail';
import { LiveUpdates } from '@/components/issues/live-updates';
import { loadIssue, loadIssueEvents } from '@/lib/db';

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
  const events = await loadIssueEvents(id, issue.dataset);
  // Approval is a live-dataset flow: eval rows are pipeline test fixtures and
  // the approve route only writes dataset=live, so never offer the form here.
  return (
    <>
      <LiveUpdates />
      {issue.dataset === 'live' && <ApproveIssue issue={issue} />}
      <IssueDetail issue={issue} />
      <Activity events={events} />
    </>
  );
}
