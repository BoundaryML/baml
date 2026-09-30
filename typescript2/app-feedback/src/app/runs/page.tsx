import Link from 'next/link';
import { LiveUpdates } from '@/components/issues/live-updates';
import { currentUser } from '@/lib/auth';
import { runnerRequest } from '@/lib/runner';
import type { PlayRun } from '@/lib/runs';
export const revalidate = 0;
export default async function RunsPage() {
  const user = await currentUser();
  if (!user)
    return (
      <main className="mx-auto max-w-3xl space-y-4 p-8">
        <h1 className="text-2xl font-semibold">Bammy runs</h1>
        <a className="underline" href="/api/auth/github">
          Sign in with GitHub to view run transcripts and feedback
        </a>
      </main>
    );
  const runs = await runnerRequest<PlayRun[]>('runs');
  return (
    <main className="mx-auto max-w-4xl space-y-4 p-8">
      <LiveUpdates />
      <h1 className="text-2xl font-semibold">Bammy runs</h1>
      {!runs.length && (
        <p>
          No try runs yet. Mention bammy in Slack with “try” followed by a task.
        </p>
      )}
      {runs.map((run) => (
        <Link
          className="block space-y-2 rounded-lg border p-4 hover:bg-muted/50"
          href={`/runs/${run.id}`}
          key={run.id}
        >
          <div className="flex justify-between">
            <strong>Run #{run.id}</strong>
            <span>{run.play_status}</span>
          </div>
          <p className="line-clamp-3 whitespace-pre-wrap">{run.prompt}</p>
          <p className="text-sm text-muted-foreground">
            {run.report?.version ? `BAML ${run.report.version} · ` : ''}
            {run.feedback_ids.length} feedback reports ·{' '}
            {new Date(run.created_at).toLocaleString()}
          </p>
        </Link>
      ))}
    </main>
  );
}
