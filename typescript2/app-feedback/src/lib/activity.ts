import type { IssueEvent } from './db';

// Deliberately render known lifecycle fields only, never arbitrary event payloads
// (which can include historical agent output or CI diagnostics).
export function activityText(event: IssueEvent): string {
  switch (event.kind) {
    case 'issue_created':
      return 'Issue created; waiting for shepherd approval.';
    case 'approved':
      return 'Shepherd approved the issue.';
    case 'fix_started':
      return 'Creating a fix and preparing the PR.';
    case 'pr_opened':
      return 'Fix PR created; handed to the shared babysitter.';
    case 'needs_human':
      return 'Fix creation needs a human.';
    case 'fixed_dry_run':
      return 'Dry-run fix passed; nothing pushed.';
    case 'merged':
      return 'PR merged.';
    case 'babysit_started':
      return 'Babysitter investigating CI and reviewer feedback.';
    case 'babysit_proposed':
      return 'CI or reviewer feedback needs a fix. Review the proposal and approve its push.';
    case 'babysit_approved':
      return 'Fix approved; implementing the plan and running tests.';
    case 'babysit_pushing':
      return 'Approved fix passed tests; pushing to the PR branch.';
    case 'babysit_round':
      return event.payload.result === 'fixed'
        ? 'Fix completed; checking the updated PR.'
        : 'Fix attempt stopped; human attention required.';
    case 'babysit_result': {
      const results: Record<string, string> = {
        awaiting_approval: 'Waiting for approval of the proposed fix.',
        closed: 'PR closed; babysitting complete.',
        dry_run: 'Dry run complete; nothing pushed.',
        fork_refused: 'Fork PR cannot be modified by this runner.',
        green:
          'CI has no reported failures and no unaddressed configured-reviewer feedback. Still watching for changes.',
        merged: 'PR merged; babysitting complete.',
        out_of_rounds: 'Round limit reached; human attention required.',
        round_failed:
          'Babysitting failed; inspect the proposal and retry explicitly.',
        waiting_on_checks: 'CI is still running; watching continues.',
      };
      return (
        results[String(event.payload.result)] ?? 'Babysitter status updated.'
      );
    }
    default:
      return 'Issue status updated.';
  }
}
export function eventProposalPath(event: IssueEvent): string | null {
  const id = event.payload.proposal_id;
  return typeof id === 'string' && /^[a-zA-Z0-9-]{1,80}$/.test(id)
    ? `/proposals/${id}`
    : null;
}
