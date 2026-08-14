'use client';

import { useEffect, useState } from 'react';

/**
 * Client-side solved-problem tracking. Grading runs entirely in the browser, so
 * progress lives in localStorage. A custom event keeps the index count and the
 * solve page in sync within a tab; the `storage` event syncs across tabs.
 */

const KEY = 'bamlcode:solved:v1';
const EVENT = 'bamlcode:progress';

export function getSolved(): Set<string> {
  if (typeof window === 'undefined') return new Set();
  try {
    const raw = window.localStorage.getItem(KEY);
    return new Set(raw ? (JSON.parse(raw) as string[]) : []);
  } catch {
    return new Set();
  }
}

export function markSolved(slug: string): void {
  if (typeof window === 'undefined') return;
  const solved = getSolved();
  if (solved.has(slug)) return;
  solved.add(slug);
  window.localStorage.setItem(KEY, JSON.stringify([...solved]));
  window.dispatchEvent(new Event(EVENT));
}

/** Subscribe to the solved set; re-renders when it changes. */
export function useSolved(): Set<string> {
  const [solved, setSolved] = useState<Set<string>>(() => new Set());
  useEffect(() => {
    const sync = () => setSolved(getSolved());
    sync();
    window.addEventListener(EVENT, sync);
    window.addEventListener('storage', sync);
    return () => {
      window.removeEventListener(EVENT, sync);
      window.removeEventListener('storage', sync);
    };
  }, []);
  return solved;
}
