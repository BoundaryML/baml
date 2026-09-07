import { ApproveIssue } from "@/components/issues/approve-issue";
import { notFound } from "next/navigation";
import { IssueDetail } from "@/components/issues/issue-detail";
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
  return <><LiveUpdates /><ApproveIssue issue={issue} /><IssueDetail issue={issue} /><Activity events={events} /></>;
}
