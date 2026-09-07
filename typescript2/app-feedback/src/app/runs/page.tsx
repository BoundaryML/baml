import Link from "next/link";
import { playRuns } from "@/lib/play-runs";
import { RunSignIn } from "@/components/runs/sign-in";
import { LiveUpdates } from "@/components/issues/live-updates";

export const dynamic = "force-dynamic";
export default async function RunsPage() {
  let runs;
  try { runs = await playRuns(); }
  catch { return <main className="p-8">Runs are temporarily unavailable. Please try again.</main>; }
  if (runs === null) return <RunSignIn />;
  return <main className="max-w-4xl mx-auto p-8 space-y-6"><LiveUpdates />
    <h1 className="text-2xl font-semibold">Bammy runs</h1>
    <p className="text-sm text-muted-foreground">Latest 50 scratch tasks from Slack.</p>
    {runs.length === 0 && <p>No runs yet. Mention Bammy with “try” and a BAML task in Slack.</p>}
    {runs.map(run => <article key={run.id} className="border rounded p-4 space-y-2">
      <Link className="font-medium underline whitespace-pre-wrap break-words" href={`/runs/${run.id}`}>{run.prompt.slice(0, 240)}</Link>
      <p className="text-sm text-muted-foreground">{run.play_status} · {run.created_at} · {run.turns} turns · {run.tokens ?? "unknown"} tokens</p>
      <p className="whitespace-pre-wrap break-words text-sm">{run.report?.summary}</p>
    </article>)}
  </main>;
}
