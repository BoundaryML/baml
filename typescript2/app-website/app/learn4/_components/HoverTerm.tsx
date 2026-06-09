'use client';

import type { ReactNode } from 'react';

/**
 * A dotted-underline term with a CSS-only popover (hover or keyboard focus).
 * No state, no effects — the tooltip is a positioned sibling toggled by
 * :hover / :focus-visible in learn4.css.
 */
export function HoverTerm({
  children,
  tip,
}: {
  children: ReactNode;
  tip: string;
}) {
  return (
    // biome-ignore lint/a11y/noNoninteractiveTabindex: focusable so the tooltip works from the keyboard
    <span className="l4-term" tabIndex={0}>
      {children}
      <span role="tooltip" className="l4-term-tip">
        {tip}
      </span>
    </span>
  );
}
