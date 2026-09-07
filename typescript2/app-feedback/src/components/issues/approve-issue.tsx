import type { Issue } from "@/lib/types";
import { SlackLink } from "@/components/slack-link";

export function ApproveIssue({ issue }: { issue: Pick<Issue, "status" | "shepherd"> }) {
  if (issue.status.state !== "awaiting_approval") return null;
  return <div className="space-y-2 rounded border p-4">
    <SlackLink>Approve on Slack</SlackLink>
    <p className="text-sm text-muted-foreground">
      {issue.shepherd ? `@${issue.shepherd}: react` : "Ask the assigned shepherd to react"} with 👍 to the issue announcement in Slack.
    </p>
  </div>;
}
