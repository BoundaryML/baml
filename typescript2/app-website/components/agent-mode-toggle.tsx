'use client';

import Link from 'next/link';
import { usePathname } from 'next/navigation';

const MONO =
  'ui-monospace, SFMono-Regular, "JetBrains Mono", Menlo, Consolas, monospace';
const ACCENT = '#7C3AED';

export function AgentModeToggle({ className }: { className?: string }) {
  const pathname = usePathname();
  const isAgent = pathname === '/agent';

  const base: React.CSSProperties = {
    fontFamily: MONO,
    fontSize: 11,
    letterSpacing: '0.06em',
    padding: '4px 10px',
    borderRadius: 6,
    textTransform: 'lowercase',
    transition: 'color 150ms, background-color 150ms',
  };

  return (
    <div
      className={className}
      style={{
        display: 'inline-flex',
        alignItems: 'center',
        gap: 2,
        border: '1px solid #D9D3C4',
        borderRadius: 8,
        padding: 2,
        background: 'rgba(255,255,255,0.4)',
      }}
      aria-label="Agent mode toggle"
    >
      <Link
        href="/?from=toggle"
        prefetch
        style={{
          ...base,
          background: isAgent ? 'transparent' : ACCENT,
          color: isAgent ? '#5C5852' : '#fff',
        }}
      >
        human
      </Link>
      <Link
        href="/agent?from=toggle"
        prefetch
        style={{
          ...base,
          background: isAgent ? ACCENT : 'transparent',
          color: isAgent ? '#fff' : '#5C5852',
        }}
      >
        agent
      </Link>
    </div>
  );
}
