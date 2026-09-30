import { LiveAgents } from '@/components/live-agents';
export default async function AgentsPage({
  searchParams,
}: {
  searchParams: Promise<{ issue_id?: string; dataset?: string }>;
}) {
  const { issue_id, dataset } = await searchParams;
  const issueId =
    issue_id && /^(?:ISSUE|GH)-[A-Za-z0-9_-]+$/.test(issue_id)
      ? issue_id
      : undefined;
  return (
    <main className="mx-auto max-w-5xl space-y-6 p-6">
      <h1 className="text-2xl font-semibold">Agent sessions</h1>
      <LiveAgents
        dataset={dataset === 'eval' ? 'eval' : 'live'}
        issueId={issueId}
      />
    </main>
  );
}
