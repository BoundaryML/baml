import { currentUser } from "@/lib/approval-auth";
import { isShepherd, type ApprovalIssue } from "@/lib/issue-approval";
export async function ApproveIssue({ issue }: { issue: ApprovalIssue }) {
  if (issue.status.state !== "awaiting_approval") return null;
  let login: string | null;
  try { login = await currentUser(); } catch { return <p>Website approval is unavailable. You can approve in Slack.</p>; }
  if (!login) return <a href={`/auth/github?issue=${encodeURIComponent(issue.id)}`}>Sign in to approve this issue</a>;
  if (!isShepherd(login, issue)) return <p>Waiting for approval from @{issue.shepherd}.</p>;
  return <form action={`/issues/${encodeURIComponent(issue.id)}/approve`} method="post"><button type="submit" className="rounded border px-3 py-2">Approve issue</button></form>;
}
