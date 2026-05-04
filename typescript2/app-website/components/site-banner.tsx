'use client';

import Link from 'next/link';
import { useEffect, useState } from 'react';

const BORDER = '#D9D3C4';
const INK = '#1A1612';
const MUTED = '#5C5852';
const ACCENT = '#6D28D9';
const CARD_BG = '#FBF8F1';

// Reserve the visual height during SSR so layout does not shift when the
// banner mounts on the client. PostHog's autocapture init injects a <script>
// tag mid-tree which collides with the Link's <a> during hydration; mounting
// the Link only on the client sidesteps the mismatch.
const RESERVED_HEIGHT = 41;

export function SiteBanner() {
  const [mounted, setMounted] = useState(false);
  useEffect(() => {
    setMounted(true);
  }, []);

  if (!mounted) {
    return (
      <div
        aria-hidden
        style={{
          background: CARD_BG,
          borderBottom: `1px solid ${BORDER}`,
          height: RESERVED_HEIGHT,
          width: '100%',
        }}
      />
    );
  }

  return (
    <Link
      href="/series-a"
      className="site-banner"
      style={{
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        gap: 14,
        width: '100%',
        background: CARD_BG,
        borderBottom: `1px solid ${BORDER}`,
        padding: '10px 16px',
        textDecoration: 'none',
        color: INK,
        fontSize: 13,
        letterSpacing: '0.01em',
        transition: 'background-color 200ms ease',
      }}
    >
      <span
        style={{
          display: 'inline-flex',
          alignItems: 'center',
          padding: '2px 8px',
          fontSize: 10,
          fontWeight: 600,
          letterSpacing: '0.14em',
          textTransform: 'uppercase',
          color: '#ffffff',
          background: ACCENT,
          borderRadius: 999,
        }}
      >
        New
      </span>
      <span style={{ color: MUTED }}>
        Announcing our{' '}
        <span style={{ color: INK, fontWeight: 600 }}>$XXM Series A</span>, led
        by <span style={{ color: INK, fontWeight: 600 }}>X</span>. Building a
        language for the agent era.
      </span>
      <span
        aria-hidden
        className="site-banner-arrow"
        style={{
          color: ACCENT,
          fontWeight: 500,
          transition: 'transform 200ms ease',
        }}
      >
        →
      </span>
      <style>{`
        .site-banner:hover { background-color: #F5EFE3; }
        .site-banner:hover .site-banner-arrow { transform: translateX(3px); }
        @media (max-width: 600px) {
          .site-banner { font-size: 12px; padding: 8px 12px; gap: 10px; }
        }
      `}</style>
    </Link>
  );
}
