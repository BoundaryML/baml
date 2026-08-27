'use client';

import Link from 'next/link';
import { usePathname } from 'next/navigation';
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
  const pathname = usePathname();
  useEffect(() => {
    setMounted(true);
  }, []);
  // The /learn* and /bamlcode pages are self-contained; no marketing banner.
  if (pathname?.startsWith('/learn') || pathname?.startsWith('/bamlcode')) {
    return null;
  }

  if (!mounted) {
    return (
      <div
        aria-hidden
        className="site-banner-reserved"
        style={{
          background: CARD_BG,
          borderBottom: `1px solid ${BORDER}`,
          height: RESERVED_HEIGHT,
          position: 'fixed',
          top: 0,
          width: '100%',
          zIndex: 60,
        }}
      />
    );
  }

  return (
    <div
      className="site-banner"
      style={{
        alignItems: 'center',
        background: CARD_BG,
        borderBottom: `1px solid ${BORDER}`,
        color: INK,
        display: 'flex',
        fontSize: 13,
        justifyContent: 'center',
        letterSpacing: '0.01em',
        minHeight: RESERVED_HEIGHT,
        padding: '10px 16px',
        position: 'fixed',
        top: 0,
        width: '100%',
        zIndex: 60,
      }}
    >
      <Link
        className="site-banner-cta"
        href="/eap"
        style={{
          alignItems: 'center',
          color: INK,
          display: 'flex',
          gap: 14,
          textDecoration: 'none',
        }}
      >
        <span
          style={{
            alignItems: 'center',
            background: ACCENT,
            borderRadius: 999,
            color: '#ffffff',
            display: 'inline-flex',
            fontSize: 10,
            fontWeight: 600,
            letterSpacing: '0.14em',
            padding: '2px 8px',
            textTransform: 'uppercase',
          }}
        >
          New
        </span>
        <span style={{ color: MUTED }}>
          BAML 1.0 will be GA soon! in the meantime, join a live onboarding,{' '}
          <span style={{ color: INK, fontWeight: 600 }}>every Thursday</span>
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
      </Link>
      <a
        className="site-banner-olddocs"
        href="https://docs.boundaryml.com"
        rel="noopener noreferrer"
        style={{
          color: MUTED,
          fontWeight: 500,
          position: 'absolute',
          right: 16,
          textDecoration: 'none',
          transition: 'color 200ms ease',
        }}
      >
        Old Docs
      </a>
      <style>{`
        .site-banner-cta:hover { color: ${ACCENT}; }
        .site-banner-cta:hover .site-banner-arrow { transform: translateX(4px); }
        .site-banner-olddocs:hover { color: ${ACCENT}; }
        @media (max-width: 600px) {
          .site-banner { font-size: 12px; padding: 8px 12px; }
          .site-banner-cta { gap: 10px; }
          .site-banner-olddocs { right: 12px; }
        }
      `}</style>
    </div>
  );
}
