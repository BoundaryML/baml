'use client';

import { useSyncExternalStore } from 'react';
import { track } from './track';

type Theme = 'light' | 'dark';

const QUERY = '(prefers-color-scheme: light)';

/** The theme lives on <html>, not in React: read it, don't mirror it. */
function subscribe(onChange: () => void) {
  const mq = window.matchMedia(QUERY);
  const observer = new MutationObserver(onChange);
  mq.addEventListener('change', onChange);
  observer.observe(document.documentElement, {
    attributeFilter: ['data-theme'],
    attributes: true,
  });
  return () => {
    mq.removeEventListener('change', onChange);
    observer.disconnect();
  };
}

const read = (): Theme =>
  (document.documentElement.dataset.theme as Theme | undefined) ??
  (window.matchMedia(QUERY).matches ? 'light' : 'dark');

// On the server the theme isn't knowable, and guessing would mean a wrong label for a frame.
const unknown = () => null;

/**
 * Nav-sized theme switch. Which icon shows is decided in CSS (--show-dark / --show-light) rather
 * than in React, so the button looks right on the server, with no JS, and before hydration. The
 * resolved theme is only read to keep the accessible name honest.
 */
export default function ThemeToggle() {
  const theme = useSyncExternalStore(subscribe, read, unknown);

  const toggle = () => {
    const next: Theme = (theme ?? read()) === 'dark' ? 'light' : 'dark';
    document.documentElement.dataset.theme = next;
    track('theme_toggled', { theme: next });
    try {
      localStorage.setItem('baml-theme', next);
    } catch {
      // Private browsing: the choice still applies, it just won't be remembered.
    }
  };

  return (
    <button
      aria-label={
        theme
          ? `Switch to ${theme === 'dark' ? 'light' : 'dark'} theme`
          : 'Switch theme'
      }
      className="tap shrink-0 size-9 grid place-items-center rounded-[7px] border border-rule text-body hover:text-ink hover:border-body/50 transition-colors"
      onClick={toggle}
      type="button"
    >
      <svg
        aria-hidden="true"
        className="dark-only"
        fill="none"
        height="16"
        stroke="currentColor"
        strokeLinecap="round"
        strokeWidth="1.4"
        viewBox="0 0 16 16"
        width="16"
      >
        <circle cx="8" cy="8" r="3.1" />
        <path d="M8 1v1.6M8 13.4V15M15 8h-1.6M2.6 8H1M12.95 3.05l-1.13 1.13M4.18 11.82l-1.13 1.13M12.95 12.95l-1.13-1.13M4.18 4.18L3.05 3.05" />
      </svg>
      <svg
        aria-hidden="true"
        className="light-only"
        fill="none"
        height="16"
        stroke="currentColor"
        strokeLinejoin="round"
        strokeWidth="1.4"
        viewBox="0 0 16 16"
        width="16"
      >
        <path d="M13.8 9.6A6.1 6.1 0 0 1 6.4 2.2a6.1 6.1 0 1 0 7.4 7.4Z" />
      </svg>
    </button>
  );
}
