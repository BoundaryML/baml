import Link from "next/link";
import { notFound } from "next/navigation";
import { playRuns } from "@/lib/play-runs";
import { Turns } from "@/components/runs/turns";
import { RunSignIn } from "@/components/runs/sign-in";
import { LiveUpdates } from "@/components/issues/live-updates";

export const dynamic = "force-dynamic";
export default async function RunPage({ params }: { params: Promise<{ id: string }> }) {
  const { id } = await params;
  if (!/^[1-9][0-9]{0,18}$/.test(id)) notFound();
  let rows;
  try { rows = await playRuns(id); }
  catch { return <main className="p-8">This run is temporarily unavailable. Please try again.</main>; }
  if (rows === null) return <RunSignIn id={id} />;
  const run = rows[0];
  if (!run) notFound();
  return <main className="max-w-4xl mx-auto p-8 space-y-6"><LiveUpdates />
    <Link className="underline" href="/runs">All runs</Link>
    <h1 className="text-2xl font-semibold">Run #{run.id}</h1>
    <p className="text-sm text-muted-foreground">{run.play_status} · {run.turns} turns · {run.tokens ?? "unknown"} tokens · {run.seconds}s</p>
    {run.canary_sha && <p className="text-xs font-mono break-all">Canary compiler: {run.canary_sha}</p>}
    <section className="space-y-2"><h2 className="font-semibold">Task</h2><p className="whitespace-pre-wrap break-words">{run.prompt}</p></section>
    <section className="space-y-2"><h2 className="font-semibold">Result</h2>
      <p className="whitespace-pre-wrap break-words">{run.report?.summary ?? "No completed result yet."}</p>
      {typeof run.report?.worked === "boolean" && <p>{run.report.worked ? "Task succeeded." : "Task did not fully succeed."}</p>}
    </section>
    {run.feedback_ids.length > 0 && <section><h2 className="font-semibold">Feedback filed</h2>
      <ul className="list-disc pl-5">{run.feedback_ids.map(report => <li className="break-all" key={report}>{report}
        {(run.feedback_issues?.find(item => item.id === report)?.issue_ids ?? []).filter(issue => /^[a-zA-Z0-9-]{1,100}$/.test(issue)).map(issue =>
          <Link key={issue} className="underline ml-3" href={`/issues/${issue}`}>{issue}</Link>)}
      </li>)}</ul>
      <p className="text-sm text-muted-foreground">These reports enter the shared issue triage and shepherd approval process.</p>
    </section>}
    <section className="space-y-3"><h2 className="font-semibold">Turns</h2><Turns turns={run.transcript ?? []} /></section>
  </main>;
}
