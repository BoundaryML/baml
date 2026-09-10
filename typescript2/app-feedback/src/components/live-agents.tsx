"use client";
import Link from "next/link";
import { useEffect, useState } from "react";
import { CodeBlock, FormattedText } from "@/components/code";
type Agent = { id: string; stage: string; status: string; started_at: number; reason: string | null; issue_ids: string[]; pr_urls?: string[]; workspace: string };
type Event = { role: string; type: string; text: string; name: string | null; continuation: boolean };
export function LiveAgents({ id, issueId, pr }: { id?: string; issueId?: string; pr?: string }) {
  const [agents, setAgents] = useState<Agent[]>([]);
  const [meta, setMeta] = useState<Agent | null>(null);
  const [events, setEvents] = useState<Event[]>([]);
  const [artifact, setArtifact] = useState<{ title: string; body: string; status: string; url: string } | null>(null);
  const [error, setError] = useState("");
  useEffect(() => {
    let stopped = false; let timer: ReturnType<typeof setTimeout>; let offset = 0;
    const controller = new AbortController(); setEvents([]); setMeta(null); setArtifact(null);
    async function poll() {
      let delay = 2000;
      try {
        const query = new URLSearchParams({ id: id ?? "", offset: String(offset), issue_id: issueId ?? "", pr: pr ?? "" });
        const response = await fetch(`/api/agents?${query}`, { cache: "no-store", signal: controller.signal });
        const data = await response.json();
        if (response.status === 401) delay = 30000;
        if (!response.ok || data.error) throw new Error(data.error ?? "Unable to load transcripts");
        if (stopped) return;
        setError("");
        if (id) { setMeta(data.meta); setArtifact(data.artifact ?? null); setEvents(old => [...old, ...data.events]); offset = data.offset; if (data.more) delay = 0; }
        else setAgents(data);
      } catch (e) { if (!stopped) setError(e instanceof Error ? e.message : "Unable to load transcripts"); }
      finally { if (!stopped) timer = setTimeout(poll, delay); }
    }
    void poll(); return () => { stopped = true; controller.abort(); clearTimeout(timer); };
  }, [id, issueId, pr]);
  return <section className="space-y-4">
    {error && <p role="status" className="rounded border p-3">{error} <Link href="/api/auth/github" className="underline">Sign in</Link></p>}
    {!id && <>{!agents.length && !error && <p className="text-muted-foreground">No captured agent sessions yet.</p>}{agents.map(a => <Link key={a.id} href={`/agents/${a.id}`} className="block rounded-lg border p-4 hover:bg-accent"><div className="flex justify-between"><strong className="capitalize">{a.stage}</strong><span>{a.status}</span></div><p className="mt-1 text-sm text-muted-foreground">{new Date(a.started_at * 1000).toLocaleString()}</p>{a.pr_urls?.map(pr => <p key={pr} className="text-sm">PR #{pr.split("/").pop()}</p>)}{a.issue_ids.map(issue => <span className="mr-2 text-xs" key={issue}>{issue}</span>)}{a.reason && <p>{a.reason}</p>}</Link>)}</>}
    {artifact && <section className="space-y-3 rounded-lg border-2 border-blue-500 p-5"><h2 className="text-xl font-semibold">{artifact.title}</h2><p>{["pending", "awaiting_approval"].includes(artifact.status) ? "Historical investigation; this run is paused." : `Status: ${artifact.status}`}</p><FormattedText text={artifact.body} /><Link className="underline" href={artifact.url}>Open fix details</Link></section>}
    {id && <>{meta && <header className="space-y-2"><h1 className="text-2xl font-semibold capitalize">{meta.stage} · {meta.status}</h1>{meta.reason && <p role="status">{meta.reason}</p>}{meta.issue_ids.map(issue => <Link className="mr-3 underline" href={`/issues/${encodeURIComponent(issue)}`} key={issue}>{issue}</Link>)}</header>}<p className="text-sm text-muted-foreground">Updates every two seconds. Messages and tool results remain available after failure.</p>{!events.length && !error && <p>Waiting for the first message…</p>}{events.map((event,i) => <article key={i} className="min-w-0 space-y-2 rounded-lg border p-4"><h2 className="text-sm font-semibold capitalize">{event.role}{event.continuation ? " (continued)" : ""}</h2>{event.type === "text" ? <FormattedText text={event.text} /> : <details open={event.type === "tool_use"}><summary className="cursor-pointer">{event.name ?? "Tool result"}</summary><CodeBlock text={event.text} language="text" /></details>}</article>)}</>}
  </section>;
}
