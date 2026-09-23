import Link from "next/link";
import { notFound } from "next/navigation";
import { ArrowLeft, ExternalLink } from "lucide-react";
import { FormattedText } from "@/components/code";
import { KindBadge } from "@/components/issues/issue-status";
import { loadIssue } from "@/lib/db";
import { formatSeconds } from "@/lib/pipeline";

export const revalidate = 0;

const PR_URL = /^https:\/\/github\.com\/BoundaryML\/baml\/pull\/[1-9][0-9]*$/;

/** The fix phase's artifacts, off the issue page: the run and the design doc. */
export default async function FixPage({ params }: { params: Promise<{ id: string }> }) {
  const { id } = await params;
  const issue = await loadIssue(id);
  if (!issue) notFound();
  const o = issue.outcome;
  const pr = o?.pr && PR_URL.test(o.pr) ? o.pr : null;
  return (
    <main className="mx-auto max-w-4xl space-y-8 px-4 py-6">
      <Link href={`/issues/${issue.id}`} className="inline-flex items-center gap-1 text-sm text-muted-foreground hover:text-foreground">
        <ArrowLeft className="h-4 w-4" /> Back to the issue
      </Link>
      <header className="space-y-2">
        <div className="flex flex-wrap items-center gap-2"><KindBadge kind={issue.kind} /><span className="font-mono text-xs text-muted-foreground">{issue.id}</span></div>
        <h1 className="text-2xl font-semibold leading-tight break-words">Fix: {issue.title}</h1>
      </header>

      <section>
        <h2 className="mb-2 text-xs uppercase tracking-wide text-muted-foreground">Run</h2>
        {!o ? <p className="text-sm text-muted-foreground">No fix run yet.</p> :
          <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-1 rounded-lg border bg-card p-4 text-sm">
            <dt className="text-muted-foreground">outcome</dt><dd className="font-mono text-xs">{o.running ? `running (${o.running})` : o.kind}</dd>
            <dt className="text-muted-foreground">turns</dt><dd className="tabular-nums">{o.turns}</dd>
            <dt className="text-muted-foreground">time</dt><dd className="tabular-nums">{formatSeconds(o.seconds)}{o.timed_out && <span className="text-stage-failed"> (killed at budget)</span>}</dd>
            {o.reason && <><dt className="text-muted-foreground">reason</dt><dd>{o.reason}</dd></>}
            {o.branch && <><dt className="text-muted-foreground">branch</dt><dd className="font-mono text-xs break-all">{o.branch}</dd></>}
            {pr && <><dt className="text-muted-foreground">PR</dt><dd><a href={pr} target="_blank" rel="noreferrer" className="inline-flex items-center gap-1 underline underline-offset-2">{pr.replace("https://github.com/", "")} <ExternalLink className="h-3 w-3" /></a></dd></>}
          </dl>}
        <p className="mt-2 text-sm"><Link className="underline underline-offset-2" href={`/agents?issue_id=${encodeURIComponent(issue.id)}`}>Agent transcripts</Link></p>
      </section>


      {issue.design_doc && (
        <section>
          <h2 className="mb-2 text-xs uppercase tracking-wide text-muted-foreground">Design doc (agent, read-only pass)</h2>
          <div className="rounded-lg border bg-card p-4"><FormattedText text={issue.design_doc} /></div>
        </section>
      )}
    </main>
  );
}
