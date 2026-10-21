'use client';

import { useTheme } from 'next-themes';
import { useCallback } from 'react';

export function ThemeToggle() {
  const { resolvedTheme, setTheme } = useTheme();
  const toggleTheme = useCallback(() => {
    setTheme(resolvedTheme === 'dark' ? 'light' : 'dark');
  }, [resolvedTheme, setTheme]);

  return (
    <button
      aria-label="Toggle theme"
      className="docs-focus-ring group/toggle inline-flex size-8 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
      onClick={toggleTheme}
      type="button"
    >
      <svg
        aria-hidden="true"
        className="size-[18px]"
        fill="none"
        height="24"
        stroke="currentColor"
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth="2"
        viewBox="0 0 24 24"
        width="24"
        xmlns="http://www.w3.org/2000/svg"
      >
        <path d="M12 12m-9 0a9 9 0 1 0 18 0a9 9 0 1 0 -18 0" />
        <path d="M12 3v18" />
        <path d="m12 9 4.65-4.65" />
        <path d="m12 14.3 7.37-7.37" />
        <path d="m12 19.6 8.85-8.85" />
      </svg>
      <span className="sr-only">Toggle theme</span>
    </button>
  );
}
