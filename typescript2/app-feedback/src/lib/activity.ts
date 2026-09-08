import { validProposalId } from "./proposals";
import type { IssueEvent } from "./db";

// Deliberately render known lifecycle fields only, never arbitrary event payloads
// (which can include historical agent output or CI diagnostics).
export function activityText(event: IssueEvent): string {
  switch (event.kind) {
    case "issue_created": return "Issue created; waiting for shepherd approval.";
    case "approved": return "Shepherd approved the issue.";
    case "fix_started": return "Creating a fix and preparing the PR.";
    case "pr_opened": return "Fix PR created; handed to the shared babysitter.";
    case "needs_human": return "Fix creation needs a human.";
    case "fixed_dry_run": return "Dry-run fix passed; nothing pushed.";
    case "merged": return "PR merged.";
    case "babysit_started": return "Babysitter investigating CI and reviewer feedback.";
    case "babysit_proposed": return "CI or reviewer feedback needs a fix. View the proposed fix, then approve on Slack.";
    case "babysit_proposal_posted": return "Proposal posted to Slack for approval.";
    case "babysit_approved": return "Fix approved; implementing the plan.";
    case "babysit_pushing": return "Approved fix is being scanned and pushed; CI will validate it.";
    case "babysit_round": return event.payload.result === "fixed"
      ? "Fix completed; checking the updated PR." : "Fix attempt stopped; human attention required.";
    case "babysit_result": {
      const results: Record<string, string> = {
        green: "CI has no reported failures and no unaddressed configured-reviewer feedback. Still watching for changes.",
        awaiting_approval: "Waiting for approval of the proposed fix.",
        waiting_on_checks: "CI is still running; watching continues.",
        merged: "PR merged; babysitting complete.", closed: "PR closed; babysitting complete.",
        round_failed: "Babysitting failed; inspect the proposal and retry explicitly.",
        out_of_rounds: "Round limit reached; human attention required.",
        fork_refused: "Fork PR cannot be modified by this runner.", dry_run: "Dry run complete; nothing pushed.",
      };
      return results[String(event.payload.result)] ?? "Babysitter status updated.";
    }
    default: return "Issue status updated.";
  }
}
export function eventProposalPath(event: IssueEvent): string | null {
  const id = event.payload.proposal_id;
  return typeof id === "string" && validProposalId(id) ? `/proposals/${id}` : null;
}
