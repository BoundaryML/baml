'use client';
import { useRouter } from 'next/navigation';
import { useState } from 'react';

export function IssueActions({
  id,
  dataset,
  user,
}: {
  id: string;
  dataset: string;
  user: string | null;
}) {
  const router = useRouter();
  const [body, setBody] = useState('');
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState('');
  const [linearUrl, setLinearUrl] = useState<string | null>(null);
  async function submit(operation: 'comment' | 'export') {
    setBusy(true);
    setMessage('');
    try {
      const response = await fetch(`/api/issues/${encodeURIComponent(id)}`, {
        body: JSON.stringify({ body, dataset, operation }),
        headers: { 'Content-Type': 'application/json' },
        method: 'POST',
      });
      const data = await response.json();
      if (!response.ok) throw new Error(data.error ?? 'Request failed.');
      if (operation === 'comment') {
        setBody('');
        setMessage('Comment added.');
        router.refresh();
      } else {
        const url = new URL(data.url);
        if (
          url.protocol !== 'https:' ||
          url.hostname !== 'linear.app' ||
          url.username ||
          url.password
        )
          throw new Error('Export returned an invalid link.');
        setLinearUrl(url.href);
        setMessage(data.updated ? 'Updated in Linear.' : 'Exported to Linear.');
      }
    } catch (error) {
      setMessage(error instanceof Error ? error.message : 'Request failed.');
    } finally {
      setBusy(false);
    }
  }
  return (
    <section className="my-6 space-y-3 rounded-lg border p-4">
      <div className="flex items-center gap-3">
        <h2 className="font-semibold">Discussion</h2>
        <button
          className="ml-auto rounded border px-3 py-2 text-sm disabled:opacity-50"
          disabled={!user || busy}
          onClick={() => submit('export')}
          type="button"
        >
          Export to Linear
        </button>
        {linearUrl && (
          <a
            className="underline"
            href={linearUrl}
            rel="noopener noreferrer"
            target="_blank"
          >
            Open in Linear
          </a>
        )}
      </div>
      {user ? (
        <form
          className="space-y-2"
          onSubmit={(event) => {
            event.preventDefault();
            void submit('comment');
          }}
        >
          <label className="block text-sm" htmlFor="comment">
            Comment as @{user}
          </label>
          <textarea
            className="min-h-24 w-full rounded border bg-transparent p-3"
            id="comment"
            maxLength={5000}
            onChange={(event) => setBody(event.target.value)}
            placeholder="Add context, reproduction steps, or a question…"
            required
            value={body}
          />
          <button
            className="rounded border px-3 py-2 text-sm disabled:opacity-50"
            disabled={busy || !body.trim()}
            type="submit"
          >
            Add comment
          </button>
        </form>
      ) : (
        <a className="text-sm underline" href="/api/auth/github">
          Sign in with GitHub to comment or export to Linear
        </a>
      )}
      {message && <output className="text-sm">{message}</output>}
    </section>
  );
}
