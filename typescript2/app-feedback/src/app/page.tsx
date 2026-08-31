import { IssueList } from "@/components/issues/issue-list";
import { StatTiles } from "@/components/issues/stat-tiles";
import { ISSUES } from "@/lib/mock-data";

export default function Home() {
  return (
    <main className="max-w-[1400px] mx-auto px-4 py-6 space-y-6">
      <div>
        <h1 className="text-2xl font-semibold">Issues</h1>
        <p className="mt-1 text-sm text-muted-foreground">
          Every issue triaged from user feedback, and how far the pipeline has taken it: triage,
          routing, difficulty, then the agent&apos;s design pass, fix pass, the gate and the PR.
        </p>
      </div>
      <StatTiles issues={ISSUES} />
      <IssueList issues={ISSUES} />
    </main>
  );
}
