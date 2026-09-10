import { FormattedText } from "@/components/code";
import Link from "next/link";
import type { IssueEvent } from "@/lib/db";
import { activityText, eventProposalPath } from "@/lib/activity";

export function Activity({ events, dataset = "live", issueId }: { events: IssueEvent[]; dataset?: "live" | "eval"; issueId?: string }) {
  return <section className="space-y-4">
    <h2 className="text-lg font-semibold">Issue timeline</h2>
    {events.length === 0 ? <p className="text-sm text-muted-foreground">No activity recorded yet.</p> :
      <ol className="space-y-4 border-l pl-5">{events.map(event => {
        const proposal = eventProposalPath(event);
        return <li key={event.id} className="space-y-1">
          <time className="text-xs text-muted-foreground" dateTime={event.created_at}>{new Date(event.created_at).toLocaleString("en-US", { timeZone: "UTC" })} UTC</time>
          <p className="font-medium">{activityText(event)}</p>
          {typeof event.payload.summary === "string" && <FormattedText text={event.payload.summary} />}
          <dl className="flex flex-wrap gap-x-4 text-sm text-muted-foreground">{["difficulty", "subsystem", "shepherd", "repros"].map(key => {
            const value = event.payload[key];
            return typeof value === "string" || typeof value === "number" ? <div key={key}><dt className="inline capitalize">{key}: </dt><dd className="inline">{String(value)}</dd></div> : null;
          })}</dl>
          {typeof event.payload.pr === "string" && /^https:\/\/github\.com\/BoundaryML\/baml\/pull\/[1-9][0-9]*$/.test(event.payload.pr) && <a className="text-sm underline" href={event.payload.pr}>View fix PR</a>}
          {issueId && ["enrich_started", "enriched", "gauged", "organized", "fix_started", "pr_opened"].includes(event.kind) && <Link className="mr-3 text-sm underline" href={`/agents?issue_id=${encodeURIComponent(issueId)}`}>View agent transcripts</Link>}
          {proposal && <Link className="text-sm underline" href={`${proposal}?dataset=${dataset}`}>View proposed fix</Link>}
        </li>;
      })}</ol>}
  </section>;
}
