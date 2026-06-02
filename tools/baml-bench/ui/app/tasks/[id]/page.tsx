import Link from "next/link";
import { notFound, redirect } from "next/navigation";

import { loadTask } from "../../lib/data";

export const dynamic = "force-dynamic";

/**
 * Server component for the "/tasks/[id]" route. Loads the task; 404s if not found,
 * and redirects to "/runs/[trophyId]" once the run has produced a trophy. Otherwise
 * renders an in-progress placeholder with the task's status, source, and baml version.
 * @param params - the route params resolving to the task id
 * @returns the in-progress task page, a redirect to the trophy, or a not-found response
 */
export default async function TaskPage({ params }: { params: Promise<{ id: string }> }) {
  const { id } = await params;
  const d = await loadTask(id);
  if (!d) notFound();
  // Once the run has produced a trophy, send straight to the full result.
  if (d.trophyId) redirect(`/runs/${d.trophyId}`);

  const t = d.task;
  return (
    <div>
      <header className="page">
        <p style={{ marginBottom: 6 }}>
          <Link href="/" className="back-link">← dashboard</Link>
        </p>
        <h1>{t.prompt}</h1>
        <p>
          <strong>{t.status}</strong>
          {t.source ? <> · {t.source}</> : null}
          {t.claimedBy ? <> · agent <span className="mono mute">{t.claimedBy.slice(0, 22)}</span></> : null}
          {d.bamlLabel ? (
            <> · baml <span className="mono">{d.bamlLabel}</span></>
          ) : t.bamlVersion ? (
            <> · baml <span className="mono">{t.bamlVersion.slice(0, 8)}</span></>
          ) : null}
        </p>
      </header>
      <p className="mute">
        This run is still in progress. The trophy (metrics, report, transcript) will appear here
        when it finishes. Refresh to check, or watch it move on the{" "}
        <Link href="/">dashboard graph</Link>.
      </p>
    </div>
  );
}
