'use client';

import { type FormEvent, useEffect, useState } from 'react';
import type { EapEvent, RegistrationQuestion } from '@/lib/luma';

type Status = 'error' | 'idle' | 'submitting' | 'success';

function QuestionField({ question }: { question: RegistrationQuestion }) {
  const name = `q_${question.id}`;
  const label = (
    <span className="eap-field-label">
      {question.label}
      {question.required && <b> *</b>}
    </span>
  );

  if (question.options && question.options.length > 0) {
    return (
      <label className="eap-field">
        {label}
        <select defaultValue="" name={name} required={question.required}>
          <option disabled value="">
            Select…
          </option>
          {question.options.map((option) => (
            <option key={option.label} value={option.label}>
              {option.label}
            </option>
          ))}
        </select>
      </label>
    );
  }

  return (
    <label className="eap-field">
      {label}
      <input name={name} required={question.required} type="text" />
    </label>
  );
}

export function RegisterModal({
  event,
  onClose,
}: {
  event: EapEvent;
  onClose: () => void;
}) {
  const [status, setStatus] = useState<Status>('idle');
  const [error, setError] = useState<string | null>(null);

  // Close on Escape, and lock body scroll while open.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    document.addEventListener('keydown', onKey);
    const prevOverflow = document.body.style.overflow;
    document.body.style.overflow = 'hidden';
    return () => {
      document.removeEventListener('keydown', onKey);
      document.body.style.overflow = prevOverflow;
    };
  }, [onClose]);

  const when = new Intl.DateTimeFormat('en-US', {
    day: 'numeric',
    hour: 'numeric',
    minute: '2-digit',
    month: 'short',
    timeZoneName: 'short',
    weekday: 'short',
  }).format(new Date(event.start_at));

  async function handleSubmit(e: FormEvent<HTMLFormElement>) {
    e.preventDefault();
    setStatus('submitting');
    setError(null);

    const form = new FormData(e.currentTarget);
    const answers: Record<string, string> = {};
    for (const question of event.registrationQuestions) {
      const value = form.get(`q_${question.id}`);
      if (value != null) answers[question.id] = String(value);
    }

    try {
      const response = await fetch('/api/eap/register', {
        body: JSON.stringify({
          answers,
          email: String(form.get('email') ?? ''),
          eventId: event.id,
          name: String(form.get('name') ?? ''),
        }),
        headers: { 'content-type': 'application/json' },
        method: 'POST',
      });
      const data = (await response.json()) as { error?: string; ok?: boolean };
      if (!response.ok || !data.ok) {
        setError(data.error ?? 'Something went wrong. Please try again.');
        setStatus('error');
        return;
      }
      setStatus('success');
    } catch {
      setError('We could not reach the server. Please try again.');
      setStatus('error');
    }
  }

  return (
    <div className="eap-modal-overlay">
      <button
        aria-label="Close"
        className="eap-modal-backdrop"
        onClick={onClose}
        type="button"
      />
      <div
        aria-labelledby="eap-modal-title"
        aria-modal="true"
        className="eap-modal"
        role="dialog"
      >
        <button
          aria-label="Close"
          className="eap-modal-close"
          onClick={onClose}
          type="button"
        >
          ×
        </button>

        {status === 'success' ? (
          <div className="eap-modal-success">
            <h2 className="eap-modal-title" id="eap-modal-title">
              You're registered
            </h2>
            <p className="eap-modal-sub">
              Check your inbox. Luma just sent your confirmation and the Zoom
              link for {event.name}.
            </p>
            <a
              className="eap-book"
              href={event.url}
              rel="noopener noreferrer"
              target="_blank"
            >
              View event on Luma <span className="eap-arw">→</span>
            </a>
            <button className="eap-form-alt" onClick={onClose} type="button">
              Done
            </button>
          </div>
        ) : (
          <>
            <h2 className="eap-modal-title" id="eap-modal-title">
              Register for {event.name}
            </h2>
            <p className="eap-modal-sub">{when}</p>
            <form className="eap-form" onSubmit={handleSubmit}>
              <label className="eap-field">
                <span className="eap-field-label">Name</span>
                <input autoComplete="name" name="name" type="text" />
              </label>
              <label className="eap-field">
                <span className="eap-field-label">
                  Email<b> *</b>
                </span>
                <input
                  autoComplete="email"
                  name="email"
                  required
                  type="email"
                />
              </label>
              {event.registrationQuestions.map((question) => (
                <QuestionField key={question.id} question={question} />
              ))}
              {error && <p className="eap-form-error">{error}</p>}
              <button
                className="eap-book eap-form-submit"
                disabled={status === 'submitting'}
                type="submit"
              >
                {status === 'submitting'
                  ? 'Registering…'
                  : 'Complete registration'}
              </button>
              <a
                className="eap-form-alt"
                href={event.url}
                rel="noopener noreferrer"
                target="_blank"
              >
                or register on Luma instead
              </a>
            </form>
          </>
        )}
      </div>
    </div>
  );
}
