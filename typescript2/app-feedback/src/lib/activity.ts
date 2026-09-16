import { validProposalId } from "./proposals";
import type { IssueEvent } from "./db";

/** What a pipeline step decided and why, as the runner records it
 * (tools/atb2/baml_src/store.baml, `decide`). Any event that carries these
 * three fields renders as a decision, whatever its kind. */
export interface Decision {
  step: string;
  decision: string;
  reason: string;
  evidence: Record<string, unknown>;
}

const text = (v: unknown): string | null => (typeof v === "string" && v.trim() ? v.trim() : null);

export function decisionOf(event: Pick<IssueEvent, "payload">): Decision | null {
  const step = text(event.payload.step), decision = text(event.payload.decision), reason = text(event.payload.reason);
  if (!step || !decision || !reason) return null;
  const raw = event.payload.evidence;
  const evidence = raw && typeof raw === "object" && !Array.isArray(raw) ? (raw as Record<string, unknown>) : {};
  return { step, decision, reason, evidence };
}

/** The reason recorded for each field the pipeline sets on an issue: the
 * latest decision of the step that owns it. Keys are the field names the
 * detail page shows badges for. */
export function fieldReasons(events: Pick<IssueEvent, "kind" | "payload">[]): Partial<Record<"difficulty" | "subsystem" | "shepherd" | "repro" | "verified" | "duplicate", Decision>> {
  const out: ReturnType<typeof fieldReasons> = {};
  for (const event of events) {
    const d = decisionOf(event);
    if (!d) continue;
    if (event.kind === "gauged" || d.step === "Gauge difficulty") out.difficulty = d;
    else if (d.step === "Write the ticket") out.subsystem = d;
    else if (d.step === "Assign a shepherd") out.shepherd = d;
    else if (d.step === "Build minimal repro") out.repro = d;
    else if (event.kind === "repro_verified" || d.step.startsWith("Re-verify")) out.verified = d;
    else if (d.step === "Check for duplicates") out.duplicate = d;
  }
  return out;
}

/** A kind the renderer has no sentence for, as words: "babysit_round" -> "Babysit round". */
export function humanizeKind(kind: string): string {
  const words = kind.replace(/[_-]+/g, " ").trim();
  return words ? words[0].toUpperCase() + words.slice(1) : "Update";
}

// Deliberately render known lifecycle fields only, never arbitrary event payloads
// (which can include historical agent output or CI diagnostics).
export function activityText(event: Pick<IssueEvent, "kind" | "payload">): string {
  switch (event.kind) {
    case "ingested": return "Report ingested" + (typeof event.payload.source === "string" ? ` from ${event.payload.source}.` : ".");
    case "enrich_started": return "Enrichment started: checking the report and building a repro.";
    case "decision": return typeof event.payload.step === "string" && event.payload.step.trim() ? event.payload.step.trim() : "Pipeline decision.";
    case "repro_verified": return "Repro verified on the latest nightly.";
    case "behavior_review": return "Reviewed whether the behavior is a defect.";
    case "no_issue": {
      const reasons: Record<string, string> = {
        needs_details: "No issue opened: the report needs more detail.",
        unconfirmed_repro: "No issue opened: the repro could not be confirmed on the latest nightly.",
        by_design: "No issue opened: the behavior is by design.",
        uncertain: "No issue opened yet: whether this is a defect needs a human decision.",
        "fixed on this toolchain": "No issue opened: already fixed on the latest nightly.",
      };
      return reasons[String(event.payload.reason)] ?? "No issue opened from this report.";
    }
    case "enriched": return "Enrichment completed: repro and observed behavior recorded.";
    case "organized": return "Issue organized and assigned.";
    case "gauged": return "Difficulty assessed.";
    case "comment_synced": return "New comment on the GitHub issue" + (typeof event.payload.author === "string" && /^[A-Za-z0-9-]{1,39}$/.test(event.payload.author) ? ` from @${event.payload.author}.` : ".");
    case "intuition": return "Intuition: this issue is part of a pattern across issues.";
    case "linked": return "Linked to the other issue filed from this report.";
    case "slack_notified": return "Notified you on Slack.";
    case "babysit_fix_started": return "Fixing CI and CodeRabbit feedback automatically.";
    case "issue_created": return "Issue created; automatic fix queued.";
    case "issue_shipped": return "Shipped: every repro passes on the latest nightly.";
    case "cancelled": return "Shepherd cancelled the issue: not an issue.";
    case "approved": return "Shepherd approved the issue.";
    case "fix_started": return "Creating a fix and preparing the PR.";
    case "pr_opened": return "Fixed: draft PR created; CI and CodeRabbit follow-up continues automatically.";
    case "needs_human": return "Fix creation needs a human.";
    case "fixed_dry_run": return "Dry-run fix passed; nothing pushed.";
    case "merged": return "PR merged.";
    case "babysit_started": return "Babysitter investigating CI and reviewer feedback.";
    case "babysit_proposed": return "CI or reviewer feedback needs a fix; the agent is handling it automatically.";
    case "babysit_proposal_posted": return "Fix details posted to Slack.";
    case "babysit_approved": return "Implementing the fix.";
    case "babysit_pushing": return "Fix is being scanned and pushed; CI will validate it.";
    case "babysit_round": return event.payload.result === "fixed"
      ? "Fix completed; checking the updated PR." : "Fix attempt stopped; human attention required.";
    case "babysit_result": {
      const results: Record<string, string> = {
        green: "CI has no reported failures and no unaddressed configured-reviewer feedback. Still watching for changes.",
        awaiting_approval: "Historical run paused; no automatic retry.",
        waiting_on_checks: "CI is still running; watching continues.",
        merged: "PR merged; babysitting complete.", closed: "PR closed; babysitting complete.",
        round_failed: "Babysitting failed; inspect the proposal and retry explicitly.",
        out_of_rounds: "Round limit reached; human attention required.",
        fork_refused: "Fork PR cannot be modified by this runner.", dry_run: "Dry run complete; nothing pushed.",
      };
      if (event.payload.result === "green" && typeof event.payload.workflow_changes_needed === "string") return `Green, with workflow changes needed: ${event.payload.workflow_changes_needed}`;
      return results[String(event.payload.result)] ?? "Babysitter status updated.";
    }
    default: return `${humanizeKind(event.kind)} recorded.`;
  }
}
export function eventProposalPath(event: Pick<IssueEvent, "payload">): string | null {
  const id = event.payload.proposal_id;
  return typeof id === "string" && validProposalId(id) ? `/proposals/${id}` : null;
}
