import { notFound } from "next/navigation";
import { SlackLink } from "@/components/slack-link";

export default async function ProposalPage({ params }: { params: Promise<{ id: string }> }) {
  const { id } = await params;
  if (!/^[a-zA-Z0-9-]{1,80}$/.test(id)) notFound();
  return <main className="max-w-3xl mx-auto p-8 space-y-4">
    <h1 className="text-2xl font-semibold">Approve on Slack</h1>
    <p>Review the proposed fix in the PR’s Slack thread. An authorized shepherd can react with 👍 to the proposal message to approve one implementation, test run, and push.</p>
    <SlackLink>Open Slack</SlackLink>
    <p className="text-sm text-muted-foreground">Proposals and CI diagnostics stay private. This website cannot approve or push changes.</p>
  </main>;
}
