import { FeedbackList } from '@/components/feedback-list';
import { LiveUpdates } from '@/components/issues/live-updates';
import { loadFeedbackList, REVALIDATE_S } from '@/lib/db';

export const revalidate = 0;

/** Every report the pipeline has received, newest first, and what became of it. */
export default async function FeedbackIndexPage() {
  const reports = await loadFeedbackList();
  return (
    <main className="mx-auto max-w-4xl space-y-6 px-4 py-6">
      <LiveUpdates />
      <div>
        <h1 className="text-2xl font-semibold">Reports</h1>
        <p className="mt-1 text-xs text-muted-foreground">{`Live from the atb2 store, refreshed every ${REVALIDATE_S}s. Reporter identities never leave the store.`}</p>
      </div>
      <FeedbackList reports={reports} />
    </main>
  );
}
