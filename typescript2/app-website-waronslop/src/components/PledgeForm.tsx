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
    'w-full rounded-lg border border-black/15 bg-white/70 px-3 py-2 text-[15px] text-black outline-none transition-colors focus:border-black/40';

  if (status === 'done') {
    return (
      <div className="rounded-2xl border border-black/10 bg-white/70 p-6 text-center">
        <p className="text-lg font-bold text-black">Enlisted. ⚔️</p>
        <p className="mt-1 text-black/60">
          Your pledge is on the wall below. Welcome to the war on slop.
        </p>
        <button
          onClick={() => setStatus('idle')}
          className="mt-4 text-sm font-bold text-black/50 underline underline-offset-4 hover:text-black"
        >
          Add another
        </button>
      </div>
    );
  }

  return (
    <form
      onSubmit={onSubmit}
      className="tweet-font space-y-3 rounded-2xl border border-black/10 bg-white/60 p-5"
    >
      <div className="grid gap-3 sm:grid-cols-2">
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
      <div>
        <label htmlFor="pledge-description">
          <span className="sr-only">How are you fighting slop with slop?</span>
          <textarea
            id="pledge-description"
            name="description"
            className={`${inputClass} min-h-[88px] resize-none`}
            placeholder="How are you fighting slop with slop?"
            value={description}
            onChange={(e) => setDescription(e.target.value.slice(0, MAX_DESC))}
            maxLength={MAX_DESC}
            required
          />
        </label>
        <div className="mt-1 text-right text-xs text-black/35">
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

      {status === 'error' && <p className="text-sm text-red-700">{error}</p>}

      <button
        type="submit"
        disabled={status === 'sending'}
        className="w-full rounded-lg bg-black px-4 py-2.5 text-[15px] font-bold text-white transition-opacity hover:opacity-90 disabled:opacity-50"
      >
        {status === 'sending' ? 'Sharing…' : 'Share'}
      </button>
    </form>
  );
}
