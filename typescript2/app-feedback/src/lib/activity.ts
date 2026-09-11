import { validProposalId } from "./proposals";
import type { IssueEvent } from "./db";

// Deliberately render known lifecycle fields only, never arbitrary event payloads
// (which can include historical agent output or CI diagnostics).
export function activityText(event: IssueEvent): string {
  switch (event.kind) {
    case "repro_verified": return "Repro checked against current canary and nightly.";
    case "ingested": return "Report ingested" + (typeof event.payload.source === "string" ? ` from ${event.payload.source}.` : ".");
    case "enrich_started": return "Enrichment started: checking the report and building a repro.";
    case "enriched": return "Enrichment completed: repro and observed behavior recorded.";
    case "organized": return "Issue organized and assigned.";
    case "gauged": return "Difficulty assessed.";
    case "slack_notified": return "Notified you on Slack.";
    case "babysit_fix_started": return "Fixing CI and CodeRabbit feedback automatically.";
    case "issue_created": return "Issue created; automatic fix queued.";
    case "cancelled": return "Shepherd cancelled the issue: not an issue.";
    case "approved": return "Shepherd approved the issue.";
    case "fix_started": return "Creating a fix and preparing the PR.";
    case "pr_opened": return "Initial draft fix PR created; the shepherd owns review and merge.";
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
    default: return "Issue status updated.";
  }
}
export function eventProposalPath(event: IssueEvent): string | null {
  const id = event.payload.proposal_id;
  return typeof id === "string" && validProposalId(id) ? `/proposals/${id}` : null;
}
