import Link from 'next/link';
import { notFound } from 'next/navigation';
import { CodeBlock, FormattedText } from '@/components/code';
import { LiveUpdates } from '@/components/issues/live-updates';
import { currentUser } from '@/lib/auth';
import { runnerRequest } from '@/lib/runner';
import type { PlayRun } from '@/lib/runs';
export const revalidate = 0;
export default async function RunPage({
  params,
}: {
  params: Promise<{ id: string }>;
}) {
  const { id } = await params;
  if (!/^[1-9][0-9]{0,18}$/.test(id)) notFound();
  if (!(await currentUser()))
    return (
      <main className="p-8">
        <a className="underline" href="/api/auth/github">
          Sign in with GitHub to view this session
        </a>
      </main>
    );
  const [run] = await runnerRequest<PlayRun[]>('run', { id });
  if (!run) notFound();
  const feedback = run.report?.feedback ?? [];
  return (
    <main className="mx-auto max-w-5xl space-y-8 p-6">
      <LiveUpdates />
      <Link className="text-sm underline" href="/runs">
        All runs
      </Link>
      <header className="space-y-3">
        <h1 className="text-2xl font-semibold">
          Run #{run.id} · {run.play_status}
        </h1>
        <p className="whitespace-pre-wrap">{run.prompt}</p>
        <p className="text-sm text-muted-foreground">
          BAML {run.report?.version ?? 'version being resolved'}
          {run.canary_sha && (
            <>
              {' '}
              · <code>{run.canary_sha}</code>
            </>
          )}
        </p>
      </header>
      {run.report?.summary && <FormattedText text={run.report.summary} />}
      {run.reason && <output>{run.reason}</output>}
      <section className="space-y-4">
        <h2 className="text-xl font-semibold">
          Feedback sent ({run.feedback_ids.length})
        </h2>
        {!run.feedback_ids.length && (
          <p>No feedback has been filed in this session.</p>
        )}
        {run.feedback_ids.map((id) => {
          const report = feedback.find((f) => f.id === id);
          const stored = run.feedback?.find((f) => f.id === id);
          return (
            <article className="space-y-2 rounded border p-4" id={id} key={id}>
              <h3 className="font-semibold">
                {stored?.title ?? report?.title ?? id}
              </h3>
              <p className="break-all text-xs text-muted-foreground">
                {id} ·{' '}
                {report?.status === 'open'
                  ? 'Saved locally; delivery pending'
                  : (report?.status ?? 'Filed')}
              </p>
              {(stored?.body ?? report?.description) && (
                <FormattedText
                  text={stored?.body ?? report?.description ?? ''}
                />
              )}
              {stored?.issue_ids.map((issue) => (
                <Link
                  className="mr-3 inline-block text-sm underline"
                  href={`/issues/${encodeURIComponent(issue)}`}
                  key={issue}
                >
                  View issue
                </Link>
              ))}
              {Object.entries(stored?.files ?? {}).map(([name, text]) => (
                <details key={name}>
                  <summary className="cursor-pointer text-sm">{name}</summary>
                  <CodeBlock
                    language={name.endsWith('.baml') ? 'baml' : 'text'}
                    text={text}
                  />
                </details>
              ))}
            </article>
          );
        })}
      </section>
      <section className="space-y-4">
        <h2 className="text-xl font-semibold">Full session transcript</h2>
        <p className="text-sm text-muted-foreground">
          Messages, tool calls, and results, including earlier turns in this
          session. Updates while the agent runs.
        </p>
        {!run.transcript?.length && (
          <p>The agent has not produced a transcript yet.</p>
        )}
        {run.transcript?.map((turn, i) => (
          // biome-ignore lint/suspicious/noArrayIndexKey: Transcript turns are append-only and have no persisted identifiers.
          <article className="min-w-0 space-y-3 rounded-lg border p-4" key={i}>
            <h3 className="text-sm font-semibold capitalize">{turn.role}</h3>
            {turn.content.map((block, j) =>
              block.type === 'text' ? (
                // biome-ignore lint/suspicious/noArrayIndexKey: Blocks are append-only within their transcript turn.
                <FormattedText key={j} text={block.text} />
              ) : (
                // biome-ignore lint/suspicious/noArrayIndexKey: Blocks are append-only within their transcript turn.
                <details className="min-w-0" key={j}>
                  <summary className="cursor-pointer text-sm">
                    {block.type === 'tool_use'
                      ? (block.name ?? 'Tool call')
                      : 'Tool result'}
                  </summary>
                  <CodeBlock language="text" text={block.text} />
                </details>
              ),
            )}
          </article>
        ))}
      </section>
    </main>
  );
}
