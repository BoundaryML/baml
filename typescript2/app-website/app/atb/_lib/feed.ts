// Feed item shape served by /api/atb/feed: agent observations (wins) and
// tracked bugs, rendered as a scrollable card feed on the front page.

export type FeedStatus = "reported" | "fixing" | "fixed" | "rejected";

export type FeedItem = {
  id: string;
  kind: "win" | "bug";
  text: string;
  detail?: string | null;
  at: number;
  // attribution
  skillRef?: string | null;
  source?: string | null;
  taskPrompt?: string | null;
  bamlVersion?: string | null;
  // links
  runId?: string | null;
  issueId?: string | null;
  // bug fields
  status?: FeedStatus;
  issueKind?: "skill" | "language";
  category?: string | null;
  evidenceCount?: number;
  brokeIn?: string | null;
  fixedIn?: string | null;
};

export type Feed = {
  generatedAt: number;
  counts: {
    wins: number;
    bugs: number;
    fixed: number;
    runs: number;
  };
  items: FeedItem[];
};

/** Collapse the issue lifecycle into the three states readers care about. */
export function feedStatus(issue: {
  status: string;
  fixSlackTs?: string | null;
}): FeedStatus | null {
  switch (issue.status) {
    case "open":
    case "confirmed":
    case "redraft":
    case "redrafting":
    case "verifying":
      return issue.fixSlackTs ? "fixing" : "reported";
    case "approved":
    case "dispatching":
    case "fixing":
    case "tocursor":
    case "prprep":
    case "pr_ready":
    case "needs_human":
      return "fixing";
    case "closed":
      return "fixed";
    case "rejected":
      return "rejected";
    default:
      return null; // queue noise (failed dedup etc.) stays out of the feed
  }
}
