import Link from "next/link";
import { notFound } from "next/navigation";
import { currentApprover, proposalPath } from "@/lib/approval-auth";
import { loadProposal } from "@/lib/proposals";

export const dynamic = "force-dynamic";
export default async function ProposalPage({ params }: { params: Promise<{ id: string }> }) {
  const { id } = await params;
  if (!/^[a-zA-Z0-9-]{1,80}$/.test(id)) notFound();
  let login: string | null;
  try { login = await currentApprover(); } catch { login = null; }
  if (!login) return <main className="max-w-3xl mx-auto p-8 space-y-4">
    <h1 className="text-2xl font-semibold">Review a proposed fix</h1>
    <p>Sign in as a repository maintainer to view CI diagnostics and approve this fix.</p>
    <Link className="underline" href={`/auth/github?proposal=${encodeURIComponent(id)}`}>Sign in with GitHub</Link>
    <p className="text-sm text-muted-foreground">You can also approve the proposal with a thumbs up on its Slack message.</p>
  </main>;
  const proposal = await loadProposal(id);
  if (!proposal) notFound();
  const prSafe = /^https:\/\/github\.com\/BoundaryML\/baml\/pull\/\d+$/.test(proposal.pr);
  return <main className="max-w-4xl mx-auto p-8 space-y-6">
    <div><p className="text-sm text-muted-foreground">Babysitter · {proposal.status}</p>
      <h1 className="text-2xl font-semibold">Proposed fix</h1>
      {prSafe ? <a className="underline" href={proposal.pr}>{proposal.pr}</a> : <p>{proposal.pr}</p>}
      <p className="text-sm font-mono">Based on {proposal.head}</p></div>
    <p>{proposal.summary}</p>
    <pre className="whitespace-pre-wrap break-words font-sans text-sm rounded border p-5">{proposal.plan}</pre>
    {proposal.status === "pending" ? <form action={`${proposalPath(id)}/approve`} method="POST" className="space-y-3">
      <input type="hidden" name="head" value={proposal.head} />
      <p>Approval authorizes one implementation, test run, and push for this plan on this PR head. New feedback needs another approval.</p>
      <button className="rounded bg-primary text-primary-foreground px-4 py-2" type="submit">Approve fix and push as {login}</button>
    </form> : <p>{proposal.approved_by ? `Approved by ${proposal.approved_by}. ` : ""}
      {proposal.status === "stale" ? "The PR changed. Review the replacement proposal in Slack." : "Refresh to see the latest execution status."}</p>}
  </main>;
}
