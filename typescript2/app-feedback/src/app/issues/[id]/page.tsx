import { notFound } from "next/navigation";
import { IssueDetail } from "@/components/issues/issue-detail";
import Link from "next/link";
import { Activity } from "@/components/issues/activity";
import { LiveUpdates } from "@/components/issues/live-updates";
import { loadIssueEvents, loadIssue } from "@/lib/db";

// On demand, never prerendered at build: the data source is decided by the
// server's environment. revalidate = 0 keeps the route dynamic while the
// fetch-level cache in db.ts (REVALIDATE_S) still bounds the reads.
export const revalidate = 0;

export default async function IssuePage({ params }: { params: Promise<{ id: string }> }) {
  const { id } = await params;
  const issue = await loadIssue(id);
  if (!issue) notFound();
  const events = await loadIssueEvents(id, issue.dataset);
  const pr = "pr" in issue.status ? issue.status.pr : null;
  const number = pr?.match(/^https:\/\/github\.com\/BoundaryML\/baml\/pull\/([1-9][0-9]{0,9})$/)?.[1];
  return <><LiveUpdates /><IssueDetail issue={issue} />
    <div className="max-w-4xl mx-auto px-4 pb-10 space-y-5">
      {number && <Link className="underline" href={`/prs/${number}?dataset=${issue.dataset}`}>PR #{number} babysitter activity</Link>}
      <Activity events={events} />
    </div></>;

}
