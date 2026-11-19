import { X } from 'lucide-react';
import { useId, useState } from 'react';
import { SIGNUP_TEAMMATE_EMAILS_MAX } from '@/lib/signup';
import { isSignupEmail, parseTeammateEmails } from '@/lib/signup-email';

export default function TeammateEmails({
  inputClass,
  emails,
  setEmails,
  draft,
  setDraft,
}: {
  inputClass: string;
  emails: string[];
  setEmails: (emails: string[]) => void;
  draft: string;
  setDraft: (draft: string) => void;
}) {
  const id = useId();
  const [error, setError] = useState('');

  function addEmails(value: string) {
    const candidates = parseTeammateEmails(value);
    const next = [...new Set([...emails, ...candidates])];
    if (
      !candidates.length ||
      candidates.some((email) => !isSignupEmail(email))
    ) {
      setError('Check the email addresses and try again.');
      return false;
    }
    if (next.join(', ').length > SIGNUP_TEAMMATE_EMAILS_MAX) {
      setError('This list is too long. Remove an address and try again.');
      return false;
    }
    setEmails(next);
    setDraft('');
    setError('');
    return true;
  }

  return (
    <div>
      <label className="block text-sm font-medium text-head" htmlFor={id}>
        Who else should join?{' '}
        <span className="font-normal text-muted">(optional)</span>
      </label>
      {emails.length > 0 && (
        <ul
          aria-label="Added teammate emails"
          className="mt-2 flex flex-wrap gap-2"
        >
          {emails.map((email) => (
            <li
              className="flex min-w-0 items-center gap-2 rounded-md bg-clover/10 py-1 pl-3 pr-1.5 text-sm text-head"
              key={email}
            >
              <input name="teammateEmails" type="hidden" value={email} />
              <span className="min-w-0 wrap-anywhere">{email}</span>
              <button
                aria-label={`Remove ${email}`}
                className="focus-ring flex size-6 shrink-0 items-center justify-center rounded-full text-ink-2 hover:bg-clover/20 hover:text-head"
                onClick={() =>
                  setEmails(emails.filter((item) => item !== email))
                }
                type="button"
              >
                <X aria-hidden="true" size={14} />
              </button>
            </li>
          ))}
        </ul>
      )}
      <div className="flex items-end gap-2">
        <input
          aria-describedby={error ? `${id}-help ${id}-error` : `${id}-help`}
          aria-invalid={Boolean(error)}
          autoCapitalize="none"
          className={inputClass}
          id={id}
          inputMode="email"
          maxLength={SIGNUP_TEAMMATE_EMAILS_MAX}
          name="teammateEmails"
          onChange={(event) => {
            setDraft(event.target.value);
            setError('');
          }}
          onKeyDown={(event) => {
            if (
              event.nativeEvent.isComposing ||
              (event.key !== 'Enter' && event.key !== ',') ||
              !draft.trim()
            )
              return;
            if (event.key === ',' && !isSignupEmail(draft.trim())) return;
            event.preventDefault();
            addEmails(draft);
          }}
          onPaste={(event) => {
            const value = event.clipboardData.getData('text').trim();
            if (!draft && value && /[,;\s<>]/.test(value)) {
              event.preventDefault();
              if (!addEmails(value))
                setDraft(
                  value
                    .replace(/[\r\n]+/g, ', ')
                    .slice(0, SIGNUP_TEAMMATE_EMAILS_MAX),
                );
            }
          }}
          placeholder="teammate@example.com"
          spellCheck={false}
          value={draft}
        />
        <button
          className="focus-ring shrink-0 rounded-lg px-3 py-3 text-sm font-medium text-clover hover:bg-clover/10 disabled:opacity-40"
          disabled={!draft.trim()}
          onClick={() => addEmails(draft)}
          type="button"
        >
          Add
        </button>
      </div>
      <p className="mt-1.5 text-sm text-ink-2" id={`${id}-help`}>
        Paste a list or press Enter after each address.
      </p>
      {error && (
        <p
          className="mt-1 text-sm text-red-300"
          id={`${id}-error`}
          role="alert"
        >
          {error}
        </p>
      )}
    </div>
  );
}
