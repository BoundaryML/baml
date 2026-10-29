'use client';

import type { KeyboardEvent } from 'react';

/**
 * The ARIA tabs pattern expects the arrow keys to move between tabs and Tab to step over the whole
 * group; buttons alone give you neither. Put `onKeyDown` on the element with `role="tablist"` and
 * `tabIndex` on each tab. Used by the install panel and the walkthrough's generated-client panel.
 */
export default function useTabKeys<T extends string>(
  tabs: readonly T[],
  current: T,
  set: (t: T) => void,
) {
  return {
    onKeyDown: (e: KeyboardEvent<HTMLElement>) => {
      const step = e.key === 'ArrowRight' ? 1 : e.key === 'ArrowLeft' ? -1 : 0;
      if (!step) return;
      e.preventDefault();
      const next =
        tabs[(tabs.indexOf(current) + step + tabs.length) % tabs.length];
      set(next);
      e.currentTarget
        .querySelectorAll<HTMLElement>('[role="tab"]')
        [tabs.indexOf(next)]?.focus();
    },
    tabIndex: (t: T) => (t === current ? 0 : -1),
  };
}
