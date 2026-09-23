import { FormattedText } from "@/components/code";
import Link from "next/link";
import type { IssueEvent } from "@/lib/db";
import { activityText, decisionOf, eventProposalPath, type Decision } from "@/lib/activity";
import { cn } from "@/lib/utils";

const GITHUB_PR = /^https:\/\/github\.com\/BoundaryML\/baml\/pull\/[1-9][0-9]*$/;
const GITHUB_COMMENT = /^https:\/\/github\.com\/BoundaryML\/baml\/issues\/[1-9][0-9]*#issuecomment-[0-9]+$/;
const LOGIN = /^[A-Za-z0-9-]{1,39}$/;

// ---------- phases ----------

type Phase = "report" | "triage" | "investigate" | "organize" | "fix" | "verify" | "intuition" | "comments" | "other";

const PHASE_LABEL: Record<Phase, string> = {
  report: "Report",
  triage: "Triage",
  investigate: "Investigation",
  organize: "Routing",
  fix: "Fix",
  verify: "Later nightlies",
  intuition: "Intuition",
  comments: "Comments",
  other: "Other",
};

const TRIAGE_STEPS = new Set(["Assess report", "Build minimal repro", "Repair the repro", "Verify repro on the latest nightly", "Is this a defect?", "Make the repro readable", "Write the ticket", "Write the feature request", "Check for duplicates", "Split report: bug", "Split report: feature request", "Linked to the other half of the report"]);
const FIX_KINDS = new Set(["design_started", "implementation_started", "issue_created", "approved", "fix_started", "pr_opened", "needs_human", "fixed_dry_run", "merged", "babysit_started", "babysit_proposed", "babysit_proposal_posted", "babysit_approved", "babysit_pushing", "babysit_round", "babysit_result", "babysit_fix_started", "cancelled", "issue_shipped", "feedback_linked"]);

/** Events that only say a step began or a message went out; the decision that followed says more. */
const NOISE = new Set(["enrich_started", "enriched", "organized", "slack_notified"]);

function phaseOf(event: IssueEvent, decision: Decision | null): Phase {
  if (event.kind === "ingested" || event.kind === "no_issue") return "report";
  if (event.kind === "comment_synced") return "comments";
  if (event.kind === "intuition") return "intuition";
  if (["investigation_started", "investigated", "investigation_failed"].includes(event.kind)) return "investigate";
  if (event.kind === "gauged" || (decision && decision.step === "Assign a shepherd")) return "organize";
  if (decision && decision.step.startsWith("Re-verify")) return "verify";
  if (decision && TRIAGE_STEPS.has(decision.step)) return "triage";
  if (event.kind === "repro_verified" || event.kind === "behavior_review" || event.kind === "linked") return "triage";
  if (FIX_KINDS.has(event.kind)) return "fix";
  if (decision) return "triage";
  return "other";
}

// ---------- outcome mark ----------

type Tone = "ok" | "warn" | "bad" | "info";

function toneOf(event: IssueEvent, decision: Decision | null): Tone {
  const d = (decision?.decision ?? "").toLowerCase();
  if (event.kind === "no_issue" || event.kind === "needs_human" || event.kind === "cancelled") return "warn";
  if (/^not actionable|^no |uncertain|still inconclusive|could not confirm|human/.test(d)) return "warn";
  if (/by design|inconclusive/.test(d)) return "warn";
  if (/fixed|shipped|passes|already/.test(d) && !/still/.test(d)) return "ok";
  if (event.kind === "pr_opened" || event.kind === "merged" || event.kind === "issue_shipped") return "ok";
  if (event.kind === "babysit_round" && event.payload.result !== "fixed") return "bad";
  return "info";
}

const TONE_CLASS: Record<Tone, string> = {
  ok: "bg-emerald-500",
  warn: "bg-amber-500",
  bad: "bg-rose-500",
  info: "bg-slate-400 dark:bg-slate-500",
};

// ---------- time ----------

function clock(iso: string) {
  return new Date(iso).toLocaleTimeString("en-US", { timeZone: "UTC", hour: "2-digit", minute: "2-digit", hour12: false });
}
function day(iso: string) {
  return new Date(iso).toLocaleDateString("en-US", { timeZone: "UTC", month: "short", day: "numeric" });
}
function span(fromIso: string, toIso: string): string | null {
  const s = Math.round((new Date(toIso).getTime() - new Date(fromIso).getTime()) / 1000);
  if (s < 45) return null;
  if (s < 3600) return `${Math.round(s / 60)} min`;
  if (s < 86400) return `${Math.round(s / 360) / 10} h`;
  return `${Math.round(s / 8640) / 10} d`;
}

// ---------- pieces ----------

function Evidence({ evidence }: { evidence: Record<string, unknown> }) {
  const entries = Object.entries(evidence).filter(([, v]) => v !== null && v !== undefined && v !== "" && !(Array.isArray(v) && v.length === 0));
  if (!entries.length) return null;
  return <details className="mt-1.5 text-xs">
    <summary className="cursor-pointer select-none text-muted-foreground hover:text-foreground">Evidence</summary>
    <dl className="mt-1.5 grid grid-cols-[auto_minmax(0,1fr)] gap-x-3 gap-y-1 rounded-md bg-muted/40 p-2">
      {entries.map(([key, value]) => {
        const scalar = typeof value === "string" || typeof value === "number" || typeof value === "boolean";
        const long = typeof value === "string" && (value.length > 100 || value.includes("\n"));
        const list = Array.isArray(value) && value.every((v) => typeof v === "string" || typeof v === "number");
        return <div key={key} className="contents">
          <dt className="font-mono text-muted-foreground">{key.replace(/_/g, " ")}</dt>
          <dd className="min-w-0 break-words">
            {scalar && !long ? String(value)
              : list ? (value as unknown[]).map(String).join(", ")
              : <pre className="max-h-56 overflow-auto whitespace-pre-wrap font-mono">{long ? String(value) : JSON.stringify(value, null, 1)}</pre>}
          </dd>
        </div>;
      })}
    </dl>
  </details>;
}

function Row({ tone, time, title, children, trailing }: { tone: Tone; time: string; title: React.ReactNode; children?: React.ReactNode; trailing?: React.ReactNode }) {
  return <li className="relative grid min-w-0 grid-cols-[3.25rem_1rem_minmax(0,1fr)] gap-x-2 py-2">
    <time className="pt-0.5 font-mono text-[11px] tabular-nums text-muted-foreground" dateTime={time} title={new Date(time).toUTCString()}>{clock(time)}</time>
    <span className="relative flex justify-center">
      <span className={cn("mt-1.5 h-2 w-2 rounded-full ring-2 ring-background", TONE_CLASS[tone])} aria-hidden />
    </span>
    <div className="min-w-0">
      <div className="flex flex-wrap items-baseline gap-x-2">
        <span className="text-sm font-medium leading-5 break-words">{title}</span>
        {trailing}
      </div>
      {children}
    </div>
  </li>;
}

function DecisionRow({ event, decision, took }: { event: IssueEvent; decision: Decision; took: string | null }) {
  const gauge = event.kind === "gauged";
  const questions = Array.isArray(decision.evidence.unknowns) ? decision.evidence.unknowns.filter((v): v is string => typeof v === "string") : [];
  const grade = /^(Trivial|Easy|Medium|Hard)\b/.exec(decision.decision)?.[1];
  return <Row tone={toneOf(event, decision)} time={event.created_at}
    title={gauge ? "Difficulty" : decision.step}
    trailing={<>{grade && <span className="rounded-full bg-muted px-2 py-0.5 text-xs">{grade}</span>}{took && <span className="text-[11px] text-muted-foreground">{took}</span>}</>}>
    <details className="mt-1 text-sm"><summary className="cursor-pointer text-muted-foreground">{gauge ? "Assessment" : "Summary"}</summary>
      {!gauge && <p className="mt-2 font-medium">{decision.decision}</p>}
      <FormattedText text={decision.reason} />
      {gauge ? questions.length > 0 && <div className="mt-3 rounded-md bg-muted/40 p-3"><h4 className="text-xs font-medium">Open questions</h4><ul className="mt-2 list-disc space-y-1 pl-4 text-sm text-muted-foreground">{questions.map(q => <li key={q}>{q}</li>)}</ul></div> : <Evidence evidence={decision.evidence} />}
    </details>
  </Row>;
}

function PlainRow({ event, dataset, issueId }: { event: IssueEvent; dataset: "live" | "eval"; issueId?: string }) {
  const author = typeof event.payload.author === "string" && LOGIN.test(event.payload.author) ? event.payload.author : null;
  const commentUrl = typeof event.payload.url === "string" && GITHUB_COMMENT.test(event.payload.url) ? event.payload.url : null;
  const proposal = eventProposalPath(event);
  const pr = typeof event.payload.pr === "string" && GITHUB_PR.test(event.payload.pr) ? event.payload.pr : null;
  const summary = typeof event.payload.summary === "string" ? event.payload.summary : null;
  const links = <>
    {pr && <a className="text-xs underline" href={pr}>View fix PR</a>}
    {issueId && ["fix_started", "pr_opened"].includes(event.kind) && <Link className="text-xs underline" href={`/agents?issue_id=${encodeURIComponent(issueId)}`}>Agent transcript</Link>}
    {event.kind === "intuition" && <Link className="text-xs underline" href="/intuition">All intuitions</Link>}
    {proposal && <Link className="text-xs underline" href={`${proposal}?dataset=${dataset}`}>Proposed fix</Link>}
    {commentUrl && <a className="text-xs underline" href={commentUrl} target="_blank" rel="noopener noreferrer">View on GitHub</a>}
  </>;
  if (event.kind === "comment_synced") {
    return <Row tone="info" time={event.created_at} title={author ? `@${author} commented on GitHub` : "New comment on GitHub"} trailing={links}>
      {summary && <blockquote className="mt-0.5 border-l-2 pl-3 text-sm text-muted-foreground">{summary}</blockquote>}
    </Row>;
  }
  return <Row tone={toneOf(event, null)} time={event.created_at} title={activityText(event)} trailing={links}>
    {summary && <details className="mt-1 text-sm"><summary className="cursor-pointer text-muted-foreground">Summary</summary><FormattedText text={summary} /></details>}
  </Row>;
}

// ---------- the view ----------

export function Activity({ events, dataset = "live", issueId, title = "Decision trail" }: { events: IssueEvent[]; dataset?: "live" | "eval"; issueId?: string; title?: string }) {
  const ordered = [...events].sort((a, b) => a.created_at.localeCompare(b.created_at) || a.id - b.id);
  // A report can be triaged more than once (a run that aborted, a re-run):
  // only the last attempt produced the issue, so only it is shown. An
  // attempt starts at "Assess report" (or the older enrich_started).
  const starts = ordered.map((e, i) => (e.kind === "enrich_started" || decisionOf(e)?.step === "Assess report") ? i : -1).filter((i) => i >= 0);
  const attemptStarts = starts.filter((i, k) => k === 0 || i - starts[k - 1] > 1); // enrich_started immediately followed by Assess counts once
  const lastStart = attemptStarts.length > 1 ? attemptStarts[attemptStarts.length - 1] : 0;
  const earlierAttempts = attemptStarts.length > 1 ? attemptStarts.length - 1 : 0;
  const isTriage = (e: IssueEvent) => { const p = phaseOf(e, decisionOf(e)); return p === "triage" || p === "investigate" || p === "report" && e.kind !== "ingested"; };
  const afterTriage = ordered.filter((e, i) => i >= lastStart || !isTriage(e));
  // Likewise the fix phase: a fix attempt starts at fix_started; only the
  // latest one is shown, earlier stopped attempts are counted.
  const fixStarts = afterTriage.map((e, i) => e.kind === "fix_started" ? i : -1).filter((i) => i >= 0);
  const lastFix = fixStarts.length > 1 ? fixStarts[fixStarts.length - 1] : -1;
  const earlierFixes = fixStarts.length > 1 ? fixStarts.length - 1 : 0;
  const isFix = (e: IssueEvent) => phaseOf(e, decisionOf(e)) === "fix";
  const latest = lastFix < 0 ? afterTriage : afterTriage.filter((e, i) => i >= lastFix || !isFix(e));
  const shown = latest.filter((e) => e.kind !== "intuition" && (!NOISE.has(e.kind) || decisionOf(e)));
  const hidden = latest.length - shown.length;

  // group consecutive events by phase, keeping order of first appearance
  const groups: { phase: Phase; items: IssueEvent[] }[] = [];
  for (const e of shown) {
    const phase = phaseOf(e, decisionOf(e));
    const last = groups[groups.length - 1];
    if (last && last.phase === phase) last.items.push(e);
    else groups.push({ phase, items: [e] });
  }
  const first = shown[0]?.created_at, last = shown[shown.length - 1]?.created_at;
  const total = first && last ? span(first, last) : null;

  return <section className="min-w-0">
    <div className="flex flex-wrap items-baseline justify-between gap-x-4 gap-y-1">
      <h2 className="text-lg font-semibold">{title}</h2>
      {shown.length > 0 && <p className="text-xs text-muted-foreground">
        {shown.length} step{shown.length === 1 ? "" : "s"}{total ? ` over ${total}` : ""} · times UTC
        {hidden > 0 && ` · ${hidden} routine notification${hidden === 1 ? "" : "s"} folded`}
        {earlierAttempts > 0 && ` · ${earlierAttempts} earlier triage attempt${earlierAttempts === 1 ? "" : "s"} hidden`}
        {earlierFixes > 0 && ` · ${earlierFixes} earlier fix attempt${earlierFixes === 1 ? "" : "s"} hidden`}
      </p>}
    </div>
    {shown.length === 0 ? <p className="mt-2 text-sm text-muted-foreground">No activity recorded yet.</p> :
      <div className="mt-3 rounded-lg border bg-card">
        {groups.map((g, gi) => {
          const dayLabel = day(g.items[0].created_at);
          const prevDay = gi > 0 ? day(groups[gi - 1].items[groups[gi - 1].items.length - 1].created_at) : null;
          return <div key={gi} className={cn(gi > 0 && "border-t")}>
            <div className="flex items-baseline gap-2 px-3 pt-3 pb-1">
              <h3 className="text-[11px] font-semibold uppercase tracking-wide text-muted-foreground">{PHASE_LABEL[g.phase]}</h3>
              {(gi === 0 || dayLabel !== prevDay) && <span className="text-[11px] text-muted-foreground">{dayLabel}</span>}
            </div>
            <ol className="relative px-3 pb-2 before:absolute before:bottom-3 before:left-[calc(3.25rem+0.75rem+0.4375rem)] before:top-2 before:w-px before:bg-border">
              {g.items.map((event, i) => {
                const decision = decisionOf(event);
                const prev = i > 0 ? g.items[i - 1] : null;
                const took = decision && prev ? span(prev.created_at, event.created_at) : null;
                return decision
                  ? <DecisionRow key={event.id} event={event} decision={decision} took={took} />
                  : <PlainRow key={event.id} event={event} dataset={dataset} issueId={issueId} />;
              })}
            </ol>
          </div>;
        })}
      </div>}
  </section>;
}
