import Link from "next/link";
import { notFound } from "next/navigation";
import { currentUser } from "@/lib/auth";
import { runnerRequest } from "@/lib/runner";
import type { PlayRun } from "@/lib/runs";
import { CodeBlock, FormattedText } from "@/components/code";
import { LiveUpdates } from "@/components/issues/live-updates";
export const revalidate = 0;
export default async function RunPage({ params }: { params: Promise<{ id: string }> }) {
  const { id } = await params;
  if (!/^[1-9][0-9]{0,18}$/.test(id)) notFound();
  if (!await currentUser()) return <main className="p-8"><a className="underline" href="/api/auth/github">Sign in with GitHub to view this session</a></main>;
  const [run] = await runnerRequest<PlayRun[]>("run", { id });
  if (!run) notFound();
  const feedback = run.report?.feedback ?? [];
  return <main className="mx-auto max-w-5xl space-y-8 p-6"><LiveUpdates /><Link href="/runs" className="text-sm underline">All runs</Link>
    <header className="space-y-3"><h1 className="text-2xl font-semibold">Run #{run.id} · {run.play_status}</h1><p className="whitespace-pre-wrap">{run.prompt}</p><p className="text-sm text-muted-foreground">BAML {run.report?.version ?? "version being resolved"}{run.canary_sha && <> · <code>{run.canary_sha}</code></>}</p></header>
    {run.report?.summary && <FormattedText text={run.report.summary} />}{run.reason && <p role="status">{run.reason}</p>}
    <section className="space-y-4"><h2 className="text-xl font-semibold">Feedback sent ({run.feedback_ids.length})</h2>
      {!run.feedback_ids.length && <p>No feedback has been filed in this session.</p>}
      {run.feedback_ids.map(id => { const report = feedback.find(f => f.id === id); const stored = run.feedback?.find(f => f.id === id); return <article id={id} key={id} className="space-y-2 rounded border p-4"><h3 className="font-semibold">{stored?.title ?? report?.title ?? id}</h3><p className="break-all text-xs text-muted-foreground">{id} · {report?.status === "open" ? "Saved locally; delivery pending" : report?.status ?? "Filed"}</p>{(stored?.body ?? report?.description) && <FormattedText text={stored?.body ?? report?.description ?? ""} />}{stored?.issue_ids.map(issue => <Link className="mr-3 inline-block text-sm underline" key={issue} href={`/issues/${encodeURIComponent(issue)}`}>View issue</Link>)}{Object.entries(stored?.files ?? {}).map(([name, text]) => <details key={name}><summary className="cursor-pointer text-sm">{name}</summary><CodeBlock text={text} language={name.endsWith(".baml") ? "baml" : "text"} /></details>)}</article>; })}
    </section>
    <section className="space-y-4"><h2 className="text-xl font-semibold">Full session transcript</h2><p className="text-sm text-muted-foreground">Messages, tool calls, and results, including earlier turns in this session. Updates while the agent runs.</p>
      {!run.transcript?.length && <p>The agent has not produced a transcript yet.</p>}
      {run.transcript?.map((turn, i) => <article key={i} className="min-w-0 space-y-3 rounded-lg border p-4"><h3 className="text-sm font-semibold capitalize">{turn.role}</h3>{turn.content.map((block, j) => block.type === "text" ? <FormattedText key={j} text={block.text} /> : <details key={j} className="min-w-0"><summary className="cursor-pointer text-sm">{block.type === "tool_use" ? block.name ?? "Tool call" : "Tool result"}</summary><CodeBlock text={block.text} language="text" /></details>)}</article>)}
    </section>
  </main>;
}
