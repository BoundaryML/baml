import { IssueList } from "@/components/issues/issue-list";
import { StatTiles } from "@/components/issues/stat-tiles";
import { dataSource, loadIssues, REVALIDATE_S } from "@/lib/db";

// On demand, never prerendered at build: the data source is decided by the
// server's environment, and the fetch cache (REVALIDATE_S) bounds the reads.
export const dynamic = "force-dynamic";

export default async function Home() {
  const issues = await loadIssues();
  return (
    <main className="max-w-[1400px] mx-auto px-4 py-6 space-y-6">
      <div>
        <h1 className="text-2xl font-semibold">Issues</h1>
        <p className="mt-1 text-sm text-muted-foreground">
          Every issue triaged from user feedback, and how far the pipeline has taken it: triage,
          routing, difficulty, then the agent&apos;s design pass, fix pass, the gate and the PR.
        </p>
        <p className="mt-1 text-xs text-muted-foreground">
          {dataSource === "supabase"
            ? `Live from the atb2 store, refreshed every ${REVALIDATE_S}s.`
            : "Mock data: set FEEDBACK_SUPABASE_URL and FEEDBACK_SUPABASE_ANON_KEY to read the store."}
        </p>
      </div>
      <StatTiles issues={issues} />
      <IssueList issues={issues} />
    </main>
  );
}
