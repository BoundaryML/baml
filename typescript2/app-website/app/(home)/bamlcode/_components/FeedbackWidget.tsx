'use client';

import { ConvexProvider, ConvexReactClient, useMutation } from 'convex/react';
import { type FormEvent, useEffect, useMemo, useState } from 'react';
import { api } from '@/convex/_generated/api';

/**
 * Inline, collapsible feedback form that lives at the bottom of the statement
 * column (beneath the problem description). Writes to the `bamlcodeFeedback`
 * Convex table. Name + email are optional; only feedback is required.
 */
export function FeedbackWidget({ slug }: { slug?: string }) {
  const url = process.env.NEXT_PUBLIC_CONVEX_URL;
  const client = useMemo(
    () => (url ? new ConvexReactClient(url) : null),
    [url],
  );
  // Close the client's connection when the widget unmounts (route change) so we
  // don't leak a websocket per visited problem page.
  useEffect(() => {
    return () => {
      void client?.close();
    };
  }, [client]);
  if (!client) return null;
  return (
    <ConvexProvider client={client}>
      <FeedbackForm slug={slug} />
    </ConvexProvider>
  );
}

type Status = 'idle' | 'sending' | 'done' | 'error';

/**
 * The feedback form: name, email, and feedback, sent to Convex with the
 * problem's slug.
 */
function FeedbackForm({ slug }: { slug?: string }) {
  const submit = useMutation(api.bamlcodeFeedback.submit);
  const [open, setOpen] = useState(true);
  const [name, setName] = useState('');
  const [email, setEmail] = useState('');
  const [feedback, setFeedback] = useState('');
  const [website, setWebsite] = useState(''); // honeypot
  const [status, setStatus] = useState<Status>('idle');
  const [error, setError] = useState('');

  const onSubmit = async (e: FormEvent) => {
    e.preventDefault();
    if (!feedback.trim() || status === 'sending') return;
    setStatus('sending');
    setError('');
    try {
      await submit({
        email: email.trim() || undefined,
        feedback,
        name: name.trim() || undefined,
        slug,
        website: website || undefined,
      });
      setStatus('done');
      setFeedback('');
      setName('');
      setEmail('');
      setTimeout(() => setStatus('idle'), 2500);
    } catch (err) {
      setStatus('error');
      setError(err instanceof Error ? err.message : 'Something went wrong.');
    }
  };

  return (
    <section className="bc-fbinline">
      <button
        aria-expanded={open}
        className="bc-fbinline-head"
        onClick={() => setOpen((v) => !v)}
        type="button"
      >
        <span>Feedback</span>
        <span aria-hidden className="bc-fbinline-chev">
          {open ? '▾' : '▸'}
        </span>
      </button>

      {open ? (
        <div className="bc-fbinline-body">
          <p className="bc-fb-prompt">
            Found a bug? Or annoyed about something in the language? Tell us.
          </p>

          {status === 'done' ? (
            <p className="bc-fb-thanks">Thanks for the feedback!</p>
          ) : (
            <form className="bc-fb-fields" onSubmit={onSubmit}>
              <input
                className="bc-fb-input"
                onChange={(e) => setName(e.target.value)}
                placeholder="Name (optional)"
                value={name}
              />
              <input
                className="bc-fb-input"
                onChange={(e) => setEmail(e.target.value)}
                placeholder="Email (optional)"
                type="email"
                value={email}
              />
              <textarea
                className="bc-fb-textarea"
                onChange={(e) => setFeedback(e.target.value)}
                placeholder="What's on your mind?"
                required
                rows={4}
                value={feedback}
              />
              {/* honeypot: hidden from real users */}
              <input
                aria-hidden
                autoComplete="off"
                className="bc-fb-hp"
                onChange={(e) => setWebsite(e.target.value)}
                tabIndex={-1}
                value={website}
              />
              {status === 'error' ? (
                <p className="bc-fb-error">{error}</p>
              ) : null}
              <button
                className="bc-btn bc-btn-secondary bc-fb-submit"
                disabled={status === 'sending' || !feedback.trim()}
                type="submit"
              >
                {status === 'sending' ? 'sending…' : 'Send feedback'}
              </button>
            </form>
          )}
        </div>
      ) : null}
    </section>
  );
}
