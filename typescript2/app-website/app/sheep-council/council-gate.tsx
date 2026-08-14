'use client';

import {
  ConvexProvider,
  ConvexReactClient,
  useConvex,
  useMutation,
} from 'convex/react';
import { useEffect, useMemo, useRef, useState } from 'react';

import { api } from '@/convex/_generated/api';

type Submission = {
  address: string;
  discord: string;
  email: string;
  firstName: string;
  lastName: string;
};
type Persist = (args: Submission) => Promise<unknown>;
// Validates the password SERVER-SIDE (Convex `council.checkPassword`). The
// secret lives only in the Convex deployment env, never in the client bundle.
type Verify = (password: string) => Promise<boolean>;

// One field per "slide". Asked one at a time; you can step back to edit.
// `hint` shows a small note under the field (e.g. why we ask for an address).
const FIELDS: {
  key: keyof Submission;
  label: string;
  placeholder: string;
  type: string;
  hint?: string;
}[] = [
  {
    key: 'firstName',
    label: 'First name',
    placeholder: 'First name',
    type: 'text',
  },
  {
    key: 'lastName',
    label: 'Last name',
    placeholder: 'Last name',
    type: 'text',
  },
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
    placeholder: 'Start typing your address…',
    type: 'text',
    hint: 'so we can ship you merch! 🐑',
  },
];

// Compose a single-line address from Photon's structured properties.
function formatAddress(p: Record<string, string | undefined>): string {
  const street = [p.housenumber, p.street].filter(Boolean).join(' ') || p.name;
  const region = [p.state, p.postcode].filter(Boolean).join(' ');
  return [street, p.city, region, p.country].filter(Boolean).join(', ');
}

// Address auto-finisher backed by Photon (komoot) — a free, keyless geocoder
// with permissive CORS. Type a few characters, pick a full address from the
// list. If the request fails (offline / rate-limited), it degrades to a plain
// text input so the form is never blocked.
function AddressField({
  value,
  onChange,
  label,
  placeholder,
}: {
  value: string;
  onChange: (next: string) => void;
  label: string;
  placeholder: string;
}) {
  const [suggestions, setSuggestions] = useState<string[]>([]);
  const [open, setOpen] = useState(false);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const abortRef = useRef<AbortController | null>(null);
  const pickedRef = useRef(false);

  useEffect(
    () => () => {
      if (timerRef.current) clearTimeout(timerRef.current);
      abortRef.current?.abort();
    },
    [],
  );

  function query(q: string) {
    abortRef.current?.abort();
    const ctrl = new AbortController();
    abortRef.current = ctrl;
    fetch(
      `https://photon.komoot.io/api/?q=${encodeURIComponent(q)}&limit=5&lang=en`,
      { signal: ctrl.signal },
    )
      .then((r) => (r.ok ? r.json() : null))
      .then((data) => {
        const lines: string[] = (data?.features ?? [])
          .map((f: { properties: Record<string, string> }) =>
            formatAddress(f.properties),
          )
          .filter(Boolean);
        setSuggestions([...new Set(lines)].slice(0, 5));
        setOpen(true);
      })
      .catch(() => {
        /* aborted or offline — leave it as a plain text input */
      });
  }

  function handleChange(next: string) {
    onChange(next);
    if (timerRef.current) clearTimeout(timerRef.current);
    // Don't re-query the value we just filled in from a picked suggestion.
    if (pickedRef.current) {
      pickedRef.current = false;
      return;
    }
    if (next.trim().length < 3) {
      setSuggestions([]);
      setOpen(false);
      return;
    }
    timerRef.current = setTimeout(() => query(next.trim()), 220);
  }

  function pick(suggestion: string) {
    pickedRef.current = true;
    onChange(suggestion);
    setSuggestions([]);
    setOpen(false);
  }

  return (
    <label className="sc-label sc-slide">
      {label}
      <div className="sc-ac">
        <input
          // biome-ignore lint/a11y/noAutofocus: matches the other wizard slides
          autoFocus
          autoComplete="off"
          className="sc-input"
          onBlur={() => setTimeout(() => setOpen(false), 120)}
          onChange={(e) => handleChange(e.target.value)}
          onFocus={() => suggestions.length > 0 && setOpen(true)}
          placeholder={placeholder}
          type="text"
          value={value}
        />
        {open && suggestions.length > 0 && (
          <ul className="sc-ac-list">
            {suggestions.map((s) => (
              <li key={s}>
                <button
                  className="sc-ac-item"
                  onClick={() => pick(s)}
                  // mousedown fires before input blur — keep the pick from being lost
                  onMouseDown={(e) => e.preventDefault()}
                  type="button"
                >
                  {s}
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>
    </label>
  );
}

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
    firstName: '',
    lastName: '',
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
          firstName: form.firstName,
          lastName: form.lastName,
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
      {field.key === 'address' ? (
        <AddressField
          key={field.key}
          label={field.label}
          onChange={(v) => setForm((f) => ({ ...f, address: v }))}
          placeholder={field.placeholder}
          value={form.address}
        />
      ) : (
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
      )}

      {field.hint && (
        <p className="sc-hint sc-slide" key={`${field.key}-hint`}>
          {field.hint}
        </p>
      )}

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
