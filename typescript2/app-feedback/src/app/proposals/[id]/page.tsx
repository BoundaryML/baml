import { FormattedText } from "@/components/code";
import { notFound } from "next/navigation";
import { SlackLink } from "@/components/slack-link";
import { LiveUpdates } from "@/components/issues/live-updates";
import { loadProposalEvents } from "@/lib/db";
import { proposalFromEvents, validProposalId } from "@/lib/proposals";

export const dynamic = "force-dynamic";

export default async function ProposalPage({ params, searchParams }: {
  params: Promise<{ id: string }>; searchParams: Promise<{ dataset?: string }>;
}) {
  const { id } = await params;
  if (!validProposalId(id)) notFound();
  const dataset = (await searchParams).dataset === "eval" ? "eval" : "live";
  const events = await loadProposalEvents(id, dataset);
  if (events.length === 0) notFound();
  const proposal = proposalFromEvents(events);
  return <main className="max-w-3xl mx-auto p-8 space-y-6">
    <LiveUpdates />
    <header className="space-y-2">
      <h1 className="text-2xl font-semibold">Proposed fix</h1>
      <p className="text-sm text-muted-foreground">{dataset} · {proposal?.pushing ? "Approved fix push started; waiting for CI" : proposal?.approved ? "Approved for implementation" : "Check Slack for current approval status"}</p>
      {proposal?.head && <p className="text-xs text-muted-foreground break-all">Proposed for commit {proposal.head}</p>}
    </header>
    {proposal ? <div className="rounded-lg border p-5"><FormattedText text={proposal.fix} /></div>
      : <p>This older proposal does not have a published fix summary. Review it in Slack.</p>}
    <section className="space-y-2">
      <h2 className="text-lg font-semibold">Approve on Slack</h2>
      <p>Like the proposal message with 👍. An authorized shepherd’s approval permits one implementation and push, followed by PR CI. If the PR or feedback changes, a new approval is required.</p>
      {proposal?.slackUrl ? <a className="underline" href={proposal.slackUrl}>Open proposal in Slack</a> : <SlackLink>Open Slack</SlackLink>}
    </section>
  </main>;
}
