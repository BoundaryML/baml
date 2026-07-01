import type { Metadata } from 'next';
import Link from 'next/link';
import { Navbar } from '@/components/navbar';

export const metadata: Metadata = {
  description: 'Real projects and examples built with BAML.',
  title: 'Built with BAML',
};

const INK = '#1A1612';
const MUTED = '#5C5852';
const EYEBROW = '#8A8580';
const ACCENT = '#6D28D9';
const BG = '#FBF7ED';

// Placeholder until the examples gallery is ready.
export default function BuiltWithBamlPage() {
  return (
    <div style={{ background: BG, color: INK, minHeight: '100vh' }}>
      <Navbar />
      <section
        style={{ padding: '112px 48px 120px', maxWidth: 880, margin: '0 auto' }}
      >
        <p
          style={{
            fontFamily: 'var(--font-geist-mono), ui-monospace, monospace',
            fontSize: 12,
            letterSpacing: '0.14em',
            textTransform: 'uppercase',
            color: EYEBROW,
            margin: 0,
          }}
        >
          Built with BAML
        </p>
        <h1
          style={{
            fontSize: 'clamp(2.2rem, 5vw, 3.6rem)',
            fontWeight: 600,
            letterSpacing: '-0.03em',
            lineHeight: 1.05,
            margin: '16px 0 0',
          }}
        >
          A gallery is on the way.
        </h1>
        <p
          style={{
            fontSize: 18,
            lineHeight: 1.6,
            color: MUTED,
            margin: '20px 0 0',
            maxWidth: 620,
          }}
        >
          We're collecting real projects built with BAML to show what the
          language looks like in practice. Check back soon. In the meantime, you
          can dig into how it works.
        </p>
        <p style={{ marginTop: 28 }}>
          <Link
            href="/explore"
            style={{ color: ACCENT, textDecoration: 'none', fontWeight: 550 }}
          >
            {'Explore BAML →'}
          </Link>
        </p>
      </section>
    </div>
  );
}
