'use client';

import { FormEvent, useState } from 'react';
import { useMutation } from 'convex/react';
import { api } from '../../convex/_generated/api';

const MAX_DESC = 280;

export default function PledgeForm() {
  const submit = useMutation(api.submissions.submit);
  const [name, setName] = useState('');
  const [email, setEmail] = useState('');
  const [description, setDescription] = useState('');
  const [website, setWebsite] = useState(''); // honeypot
  const [status, setStatus] = useState<'idle' | 'sending' | 'done' | 'error'>('idle');
  const [error, setError] = useState('');

  async function onSubmit(e: FormEvent) {
    e.preventDefault();
    if (status === 'sending') return;
    setStatus('sending');
    setError('');
    try {
      await submit({ name, email, description, website });
      setStatus('done');
      setName('');
      setEmail('');
      setDescription('');
    } catch (err) {
      setStatus('error');
      setError(err instanceof Error ? err.message : 'Something went wrong.');
    }
  }

  const inputClass =
    'w-full border-0 border-b border-ink/25 bg-transparent px-0 py-3 text-[17px] text-ink outline-none transition placeholder:text-ink-2/70 focus:border-accent';

  if (status === 'done') {
    return (
      <div className="tweet-font bg-transparent py-2 text-left">
        <p className="text-lg font-bold text-ink">Enlisted. ⚔️</p>
        <p className="mt-1 text-ink-2">
          Your pledge is on the wall below. Welcome to the war on slop.
        </p>
        <button
          onClick={() => setStatus('idle')}
          className="mt-4 text-sm font-bold text-accent underline underline-offset-4 hover:text-accent-deep"
        >
          Add another
        </button>
      </div>
    );
  }

  return (
    <form
      onSubmit={onSubmit}
      className="tweet-font w-full bg-transparent"
    >
      <div className="grid gap-5 sm:grid-cols-2">
        <label htmlFor="pledge-name">
          <span className="sr-only">Name</span>
          <input
            id="pledge-name"
            name="name"
            autoComplete="name"
            className={inputClass}
            placeholder="Name"
            value={name}
            onChange={(e) => setName(e.target.value)}
            maxLength={80}
            required
          />
        </label>
        <label htmlFor="pledge-email">
          <span className="sr-only">Email</span>
          <input
            id="pledge-email"
            name="email"
            autoComplete="email"
            className={inputClass}
            type="email"
            placeholder="Email"
            value={email}
            onChange={(e) => setEmail(e.target.value)}
            maxLength={200}
            required
          />
        </label>
      </div>
      <div className="mt-5">
        <label htmlFor="pledge-description">
          <span className="sr-only">How are you fighting slop with slop?</span>
          <textarea
            id="pledge-description"
            name="description"
            className={`${inputClass} min-h-[128px] resize-none`}
            placeholder="How are you fighting slop with slop?"
            value={description}
            onChange={(e) => setDescription(e.target.value.slice(0, MAX_DESC))}
            maxLength={MAX_DESC}
            required
          />
        </label>
        <div className="mt-1 text-right text-xs text-ink-2/60">
          {description.length}/{MAX_DESC}
        </div>
      </div>

      {/* honeypot — hidden from real users */}
      <input
        type="text"
        name="website"
        tabIndex={-1}
        autoComplete="off"
        value={website}
        onChange={(e) => setWebsite(e.target.value)}
        className="hidden"
        aria-hidden="true"
      />

      {status === 'error' && <p className="mt-3 text-sm text-red-700">{error}</p>}

      <div className="mt-2 flex justify-end">
        <button
          type="submit"
          disabled={status === 'sending'}
          className="group inline-flex items-center gap-2 bg-transparent py-1 text-[15px] font-bold text-accent transition hover:text-accent-deep disabled:opacity-60"
        >
          {status === 'sending' ? 'Sharing' : 'Share'}
          <svg
            aria-hidden="true"
            viewBox="0 0 24 24"
            className={`h-4 w-4 fill-none stroke-current stroke-2 ${status === 'sending' ? 'animate-paper-plane-send' : 'transition group-hover:translate-x-0.5 group-hover:-translate-y-0.5'}`}
          >
            <path d="M3.5 11.5 20 4l-7.5 16-2-7-7-1.5Z" />
            <path d="m10.5 13 4-4" />
          </svg>
        </button>
      </div>
    </form>
  );
}
