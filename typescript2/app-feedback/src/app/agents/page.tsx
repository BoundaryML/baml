import { LiveAgents } from "@/components/live-agents";
export default async function AgentsPage({ searchParams }: { searchParams: Promise<{issue_id?:string}> }) {
  const {issue_id} = await searchParams;
  const issueId = issue_id && /^ISSUE-[A-Za-z0-9_-]+$/.test(issue_id) ? issue_id : undefined;
  return <main className="mx-auto max-w-5xl space-y-6 p-6"><h1 className="text-2xl font-semibold">Agent sessions</h1><p className="text-muted-foreground">Live transcripts from triage, gauging, design, fixes, babysitting, try runs, and chat.</p><LiveAgents issueId={issueId} /></main>;
}
