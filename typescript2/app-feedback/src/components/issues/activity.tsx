import Link from "next/link";
import type { IssueEvent } from "@/lib/db";
import { activityText, eventProposalPath } from "@/lib/activity";

export function Activity({ events }: { events: IssueEvent[] }) {
  return <section className="space-y-4">
    <h2 className="text-lg font-semibold">Activity</h2>
    {events.length === 0 ? <p className="text-sm text-muted-foreground">No activity recorded yet.</p> :
      <ol className="space-y-4 border-l pl-5">{events.map(event => {
        const proposal = eventProposalPath(event);
        return <li key={event.id} className="space-y-1">
          <time className="text-xs text-muted-foreground" dateTime={event.created_at}>{new Date(event.created_at).toLocaleString("en-US", { timeZone: "UTC" })} UTC</time>
          <p>{activityText(event)}</p>
          {proposal && <Link className="text-sm underline" href={proposal}>Approve on Slack</Link>}
        </li>;
      })}</ol>}
  </section>;
}
