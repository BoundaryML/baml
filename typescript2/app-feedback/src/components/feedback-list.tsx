import Link from "next/link";
import { Badge } from "@/components/ui/badge";
import { KindBadge, StatusBadge } from "@/components/issues/issue-status";
import type { FeedbackListing } from "@/lib/db";

/** How the pipeline left a report: an issue, a recorded no-issue reason, or nothing yet. */
export type Fate = "issue" | "no_issue" | "pending";

export function fateOf(report: Pick<FeedbackListing, "issues" | "no_issue">): Fate {
  if (report.issues.length) return "issue";
  return report.no_issue ? "no_issue" : "pending";
}

const FATE_LABEL: Record<Fate, string> = { issue: "became an issue", no_issue: "no issue", pending: "waiting for triage" };

function firstLine(body: string, max = 220): string {
  const text = body.replace(/\s+/g, " ").trim();
  return text.length > max ? text.slice(0, max - 1).trimEnd() + "…" : text;
}

/** Every report and what it became. Pure: the page loads, this renders. */
export function FeedbackList({ reports }: { reports: FeedbackListing[] }) {
  if (reports.length === 0) return <p className="text-sm text-muted-foreground">No reports yet.</p>;
  const counts = reports.reduce((acc, r) => { acc[fateOf(r)] += 1; return acc; }, { issue: 0, no_issue: 0, pending: 0 } as Record<Fate, number>);
  return <div className="space-y-4">
    <p className="text-xs text-muted-foreground" data-testid="fate-counts">
      {reports.length} reports · {counts.issue} {FATE_LABEL.issue} · {counts.no_issue} {FATE_LABEL.no_issue} · {counts.pending} {FATE_LABEL.pending}
    </p>
    <ul className="space-y-3">
      {reports.map((r) => <li key={`${r.dataset}:${r.id}`} className="rounded-lg border bg-card p-4 space-y-2">
        <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
          <span className="font-mono">{r.id}</span>
          <span>·</span>
          <span>{r.source}</span>
          <span>·</span>
          <time dateTime={r.received_at}>{new Date(r.received_at).toLocaleString("en-US", { timeZone: "UTC", month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" })} UTC</time>
          {r.toolchain && <><span>·</span><span>toolchain {r.toolchain}</span></>}
          {r.dataset === "eval" && <Badge variant="outline" className="font-normal">eval</Badge>}
        </div>
        <h2 className="text-base font-semibold leading-snug break-words">
          <Link href={`/feedback/${encodeURIComponent(r.id)}`} className="hover:underline underline-offset-2">{r.title}</Link>
        </h2>
        <p className="text-sm text-muted-foreground break-words">{firstLine(r.body)}</p>
        {r.issues.length > 0
          ? <ul className="space-y-1">{r.issues.map((i) => <li key={i.id} className="flex flex-wrap items-center gap-2 text-sm">
              <KindBadge kind={i.kind} /><StatusBadge issue={i} />
              <Link href={`/issues/${i.id}`} className="font-medium underline underline-offset-2">{i.title}</Link>
            </li>)}</ul>
          : <p className="text-sm">{r.no_issue
              ? <><span className="font-medium">No issue:</span> <span className="text-muted-foreground">{r.no_issue}</span></>
              : <span className="text-muted-foreground">Waiting for triage.</span>}</p>}
      </li>)}
    </ul>
  </div>;
}
