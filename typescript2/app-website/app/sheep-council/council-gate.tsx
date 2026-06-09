'use client';

import {
  ConvexProvider,
  ConvexReactClient,
  useConvex,
  useMutation,
} from 'convex/react';
import { useMemo, useState } from 'react';

import { api } from '@/convex/_generated/api';

type Submission = { address: string; discord: string; email: string };
type Persist = (args: Submission) => Promise<unknown>;
// Validates the password SERVER-SIDE (Convex `council.checkPassword`). The
// secret lives only in the Convex deployment env, never in the client bundle.
type Verify = (password: string) => Promise<boolean>;

// One field per "slide". Asked one at a time; you can step back to edit.
const FIELDS: {
  key: keyof Submission;
  label: string;
  placeholder: string;
  type: string;
}[] = [
  { key: 'discord', label: 'Discord', placeholder: 'username', type: 'text' },
  {
    key: 'email',
    label: 'Email',
    placeholder: 'you@example.com',
    type: 'email',
  },
  {
    key: 'address',
    label: 'Address',
    placeholder: 'Mailing address',
    type: 'text',
  },
];

// Provider wrapper: builds the Convex client from the public deployment URL.
// If the URL isn't configured yet, the form still renders and "submits" — it
// just doesn't persist (so the page never crashes before Convex is wired up).
export function CouncilGate() {
  const url = process.env.NEXT_PUBLIC_CONVEX_URL;
  const client = useMemo(
    () => (url ? new ConvexReactClient(url) : null),
    [url],
  );

  if (!client) {
    return <Council persist={null} verify={null} />;
  }
  return (
    <ConvexProvider client={client}>
      <CouncilWithConvex />
    </ConvexProvider>
  );
}

function CouncilWithConvex() {
  const convex = useConvex();
  const submit = useMutation(api.council.submit);
  const verify: Verify = (password) =>
    convex.query(api.council.checkPassword, { password });
  return <Council persist={(args) => submit(args)} verify={verify} />;
}

function Council({
  persist,
  verify,
}: {
  persist: Persist | null;
  verify: Verify | null;
}) {
  const [unlocked, setUnlocked] = useState(false);
  const [pw, setPw] = useState('');
  const [error, setError] = useState(false);
  const [form, setForm] = useState<Submission>({
    address: '',
    discord: '',
    email: '',
  });
  const [submitted, setSubmitted] = useState(false);
  const [sending, setSending] = useState(false);
  const [submitError, setSubmitError] = useState(false);
  const [step, setStep] = useState(0);
  const [checking, setChecking] = useState(false);

  async function tryUnlock(e: React.FormEvent) {
    e.preventDefault();
    setChecking(true);
    setError(false);
    try {
      const ok = verify ? await verify(pw) : false;
      if (ok) {
        setUnlocked(true);
      } else {
        setError(true);
      }
    } catch {
      setError(true);
    } finally {
      setChecking(false);
    }
  }

  function goTo(i: number) {
    setStep(i);
    setSubmitError(false);
  }

  function back() {
    goTo(Math.max(0, step - 1));
  }

  // Form submit = "Next" on every step except the last, where it persists.
  async function onFormSubmit(e: React.FormEvent) {
    e.preventDefault();
    const lastIndex = FIELDS.length - 1;
    if (step < lastIndex) {
      if (form[FIELDS[step].key].trim()) goTo(step + 1);
      return;
    }
    const firstEmpty = FIELDS.findIndex((f) => !form[f.key].trim());
    if (firstEmpty !== -1) {
      goTo(firstEmpty);
      return;
    }
    setSending(true);
    setSubmitError(false);
    try {
      if (persist) {
        await persist({
          address: form.address,
          discord: form.discord,
          email: form.email,
        });
      }
      setSubmitted(true);
    } catch {
      setSubmitError(true);
    } finally {
      setSending(false);
    }
  }

  const set =
    (k: keyof Submission) =>
    (e: React.ChangeEvent<HTMLInputElement | HTMLTextAreaElement>) =>
      setForm((f) => ({ ...f, [k]: e.target.value }));

  if (!unlocked) {
    return (
      <form className="sc-card sc-form" onSubmit={tryUnlock}>
        <p className="sc-kicker">Members only</p>
        <h1 className="sc-title">The Sheep Council</h1>
        <p className="sc-sub">Speak the words to enter the chamber.</p>
        <input
          autoFocus
          className="sc-input"
          onChange={(e) => {
            setPw(e.target.value);
            setError(false);
          }}
          placeholder="Password"
          type="password"
          value={pw}
        />
        {error && (
          <p className="sc-error">The flock does not recognize those words.</p>
        )}
        <button
          className="sc-btn"
          disabled={checking || !pw.trim()}
          type="submit"
        >
          {checking ? 'Checking…' : 'Enter the council'}
        </button>
      </form>
    );
  }

  if (submitted) {
    return (
      <div className="sc-card sc-form">
        <div aria-hidden className="sc-sheep">
          🐑
        </div>
        <h1 className="sc-title">Your petition is heard.</h1>
        <p className="sc-sub">
          The council will be in touch. Welcome to the flock.
        </p>
      </div>
    );
  }

  const field = FIELDS[step];
  const isLast = step === FIELDS.length - 1;
  const currentFilled = form[field.key].trim() !== '';
  const allFilled = FIELDS.every((f) => form[f.key].trim() !== '');

  return (
    <form className="sc-card sc-form" onSubmit={onFormSubmit}>
      <p className="sc-kicker">
        Council registry · {step + 1} of {FIELDS.length}
      </p>
      <h1 className="sc-title">Join the Sheep Council</h1>

      {/* one field per slide; `key` re-runs the slide animation + autofocus */}
      <label className="sc-label sc-slide" key={field.key}>
        {field.label}
        <input
          autoFocus
          className="sc-input"
          onChange={set(field.key)}
          placeholder={field.placeholder}
          type={field.type}
          value={form[field.key]}
        />
      </label>

      {submitError && (
        <p className="sc-error">
          The clerk could not record your petition. Please try again.
        </p>
      )}

      <div className="sc-nav">
        <button
          className="sc-btn-ghost"
          disabled={step === 0}
          onClick={back}
          type="button"
        >
          ← Back
        </button>
        <span className="sc-dots">
          {FIELDS.map((f, i) => (
            <button
              aria-label={`Go to ${f.label}`}
              className={
                i === step ? 'on' : form[f.key].trim() ? 'done' : undefined
              }
              key={f.key}
              onClick={() => goTo(i)}
              type="button"
            />
          ))}
        </span>
        <button
          className="sc-btn"
          disabled={sending || (isLast ? !allFilled : !currentFilled)}
          type="submit"
        >
          {isLast ? (sending ? 'Submitting…' : 'Submit') : 'Next →'}
        </button>
      </div>
    </form>
  );
}
