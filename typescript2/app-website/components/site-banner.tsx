import Link from 'next/link';

const BORDER = '#D9D3C4';
const INK = '#1A1612';
const MUTED = '#5C5852';
const ACCENT = '#6D28D9';
const CARD_BG = '#FBF8F1';

export function SiteBanner() {
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
        by <span style={{ color: INK, fontWeight: 600 }}>X</span> — building a
        real language for the agent era.
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
