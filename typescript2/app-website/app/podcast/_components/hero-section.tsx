import { Calendar } from 'lucide-react';
import Link from 'next/link';

const INK = '#1A1612';
const MUTED = '#5C5852';
const BORDER = '#D9D3C4';
const ACCENT = '#6D28D9';
const EYEBROW = '#8A8580';

export function HeroSection() {
  return (
    <section
      style={{
        borderBottom: `1px solid ${BORDER}`,
        padding: '96px 48px 64px',
        width: '100%',
      }}
    >
      <div style={{ margin: '0 auto', maxWidth: 1200 }}>
        <p
          style={{
            color: EYEBROW,
            fontSize: 13,
            fontWeight: 500,
            letterSpacing: '0.12em',
            margin: 0,
            textTransform: 'uppercase',
          }}
        >
          Weekly sessions
        </p>
        <h1
          style={{
            color: INK,
            fontSize: 'clamp(2.5rem, 5vw, 4rem)',
            fontWeight: 600,
            letterSpacing: '-0.03em',
            lineHeight: 1.02,
            margin: '20px 0 0',
          }}
        >
          ai that{' '}
          <span
            style={{
              color: ACCENT,
              fontFamily: 'var(--font-serif)',
              fontStyle: 'italic',
              fontWeight: 500,
            }}
          >
            works
          </span>
          .
        </h1>
        <p
          style={{
            color: MUTED,
            fontSize: 18,
            lineHeight: 1.6,
            margin: '24px 0 0',
            maxWidth: 620,
          }}
        >
          A weekly conversation about getting more out of today&apos;s models
          with{' '}
          <Link href="https://www.github.com/hellovai" style={{ color: INK }}>
            @hellovai
          </Link>{' '}
          and{' '}
          <Link href="https://www.github.com/dexhorthy" style={{ color: INK }}>
            @dexhorthy
          </Link>
          . Live code, Q&A, and production AI patterns.
        </p>
        <div
          style={{
            alignItems: 'center',
            color: EYEBROW,
            display: 'flex',
            fontSize: 13,
            gap: 8,
            marginTop: 28,
          }}
        >
          <Calendar size={15} />
          <span>
            Every <strong style={{ color: INK }}>Tuesday</strong> at{' '}
            <strong style={{ color: INK }}>10 AM PST</strong>
          </span>
        </div>
      </div>
    </section>
  );
}
