import Link from "next/link";
import { IssueList } from "@/components/issues/issue-list";
import { LiveUpdates } from "@/components/issues/live-updates";
import { currentUser } from "@/lib/auth";
import { loadIssues, loadPendingReports } from "@/lib/db";

export const revalidate = 0;
export default async function Home({ searchParams }: { searchParams: Promise<{ view?: string }> }) {
  const [issues, user, query] = await Promise.all([loadIssues(), currentUser(), searchParams]);
  const mine = Boolean(user) && query.view !== "all";
  const linked = new Set(issues.flatMap(issue => issue.feedback_ids.map(id => `${issue.dataset ?? "live"}:${id}`)));
  const reports = mine ? [] : (await loadPendingReports()).filter(report => !linked.has(`${report.dataset}:${report.id}`));
  const shown = mine ? issues.filter(issue => issue.shepherd?.replace(/^@/, "").toLowerCase() === user!.toLowerCase()) : issues;
  return <main className="max-w-[1400px] mx-auto px-4 py-6 space-y-6">
    <LiveUpdates />
    <div className="flex items-center justify-between">
      <h1 className="text-2xl font-semibold">{mine ? "Your issues" : "All Confirmed Feedback Requests"}</h1>
      {user && <Link className="text-sm underline" href={mine ? "/?view=all" : "/"}>{mine ? "All Confirmed Feedback Requests" : "Your issues"}</Link>}
    </div>
    <IssueList issues={shown} pendingReports={reports} compact={mine} />
  </main>;
}
