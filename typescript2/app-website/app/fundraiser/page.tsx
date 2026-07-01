import { ArrowRight, Sparkles } from 'lucide-react';
import Link from 'next/link';
import { createMetadata } from '@/app/_lib/metadata';
import { FooterSection } from '@/components/footer-section';
import { Navbar } from '@/components/navbar';

const BG = '#FBF7ED';
const INK = '#1A1612';
const MUTED = '#5C5852';
const BORDER = '#D9D3C4';
const ACCENT = '#6D28D9';
const EYEBROW = '#8A8580';
const CARD_BG = '#FBF8F1';

const FUNDING_POST_SLUG = 'announcing-our-funding';
const FUNDING_POST_HREF = `/blog/${FUNDING_POST_SLUG}`;

export const metadata = createMetadata({
  description:
    'We raised funding to keep building BAML: a statically typed, Turing complete programming language for agents.',
  eyebrow: 'Announcement',
  path: '/fundraiser',
  title: 'BAML raised — building the language for agents',
  titleAbsolute: 'BAML raised — building the language for agents',
});

export default function FundraiserPage() {
  return (
    <div
      style={{
        background: BG,
        color: INK,
        width: '100%',
        maxWidth: 1600,
        margin: '0 auto',
        minHeight: '100vh',
      }}
    >
      <Navbar />

      {/* Hero */}
      <section
        style={{
          padding: '96px 48px 80px',
          borderBottom: `1px solid ${BORDER}`,
        }}
      >
        <div style={{ maxWidth: 1200, margin: '0 auto' }}>
          <p
            style={{
              fontSize: 13,
              fontWeight: 500,
              letterSpacing: '0.12em',
              textTransform: 'uppercase',
              color: EYEBROW,
              margin: 0,
            }}
          >
            Announcement
          </p>
          <h1
            style={{
              fontSize: 'clamp(2.5rem, 5.5vw, 4.5rem)',
              fontWeight: 600,
              lineHeight: 1.02,
              letterSpacing: '-0.03em',
              margin: '20px 0 0',
              color: INK,
              maxWidth: 980,
            }}
          >
            We raised to keep building{' '}
            <span
              style={{
                fontFamily: 'var(--font-serif)',
                fontStyle: 'italic',
                fontWeight: 500,
                color: ACCENT,
              }}
            >
              the language for agents
            </span>
            .
          </h1>
          <div style={{ marginTop: 28, maxWidth: 720 }}>
            <p
              style={{
                fontSize: 18,
                lineHeight: 1.6,
                color: MUTED,
                margin: 0,
              }}
            >
              BAML is a statically typed, Turing complete programming language
              with first class LLM functions, a custom VM, and schema aware
              parsing. New funding lets us go deeper on the compiler, the
              runtime, and the developer tooling that make agents feel like
              normal software.
            </p>
            <div
              style={{
                display: 'flex',
                gap: 12,
                marginTop: 28,
                flexWrap: 'wrap',
              }}
            >
              <Link
                href={FUNDING_POST_HREF}
                style={{
                  alignItems: 'center',
                  background: INK,
                  borderRadius: 6,
                  color: '#ffffff',
                  display: 'inline-flex',
                  fontSize: 14,
                  fontWeight: 500,
                  gap: 8,
                  padding: '12px 20px',
                  textDecoration: 'none',
                }}
              >
                Read the full announcement
                <ArrowRight size={16} />
              </Link>
              <Link
                href="/jobs"
                style={{
                  alignItems: 'center',
                  background: BG,
                  border: `1px solid ${BORDER}`,
                  borderRadius: 6,
                  color: INK,
                  display: 'inline-flex',
                  fontSize: 14,
                  fontWeight: 500,
                  gap: 8,
                  padding: '12px 20px',
                  textDecoration: 'none',
                }}
              >
                We&apos;re hiring
              </Link>
            </div>
          </div>
        </div>
      </section>

      {/* What it goes toward */}
      <section style={{ padding: '64px 48px 0' }}>
        <div style={{ maxWidth: 1200, margin: '0 auto' }}>
          <div
            style={{
              display: 'flex',
              alignItems: 'baseline',
              gap: 16,
              marginBottom: 32,
              paddingBottom: 16,
              borderBottom: `1px solid ${BORDER}`,
            }}
          >
            <p
              style={{
                fontSize: 12,
                fontWeight: 500,
                letterSpacing: '0.14em',
                textTransform: 'uppercase',
                color: EYEBROW,
                margin: 0,
              }}
            >
              Where it goes
            </p>
          </div>

          <div
            style={{
              display: 'grid',
              gridTemplateColumns:
                'repeat(auto-fill, minmax(min(100%, 320px), 1fr))',
              gap: 20,
            }}
          >
            {[
              {
                title: 'Compiler & runtime',
                body: 'Faster incremental builds, a richer type system, and a Bex VM that keeps up with how fast agents iterate.',
              },
              {
                title: 'Developer tooling',
                body: 'A playground and LSP that make .baml feel native in every editor — including the ones agents use.',
              },
              {
                title: 'Generated clients',
                body: 'First class clients across Python, TypeScript, Ruby, and Go, with the ergonomics of writing native code.',
              },
            ].map((item) => (
              <article
                key={item.title}
                style={{
                  background: CARD_BG,
                  border: `1px solid ${BORDER}`,
                  borderRadius: 8,
                  padding: 24,
                  display: 'flex',
                  flexDirection: 'column',
                  gap: 10,
                }}
              >
                <h3
                  style={{
                    fontSize: 17,
                    fontWeight: 600,
                    letterSpacing: '-0.015em',
                    color: INK,
                    margin: 0,
                    lineHeight: 1.25,
                  }}
                >
                  {item.title}
                </h3>
                <p
                  style={{
                    fontSize: 14,
                    lineHeight: 1.55,
                    color: MUTED,
                    margin: 0,
                  }}
                >
                  {item.body}
                </p>
              </article>
            ))}
          </div>
        </div>
      </section>

      {/* Read more CTA */}
      <section style={{ padding: '80px 48px 120px' }}>
        <div style={{ maxWidth: 720, margin: '0 auto', textAlign: 'center' }}>
          <p
            style={{
              fontSize: 12,
              fontWeight: 500,
              letterSpacing: '0.14em',
              textTransform: 'uppercase',
              color: EYEBROW,
              margin: 0,
            }}
          >
            The full story
          </p>
          <h2
            style={{
              fontSize: 'clamp(1.75rem, 3.5vw, 2.5rem)',
              fontWeight: 600,
              lineHeight: 1.1,
              letterSpacing: '-0.02em',
              margin: '16px 0 0',
              color: INK,
            }}
          >
            Why we raised, and what comes next.
          </h2>
          <p
            style={{
              fontSize: 17,
              lineHeight: 1.6,
              color: MUTED,
              margin: '20px auto 32px',
              maxWidth: 560,
            }}
          >
            The full announcement covers the round, our investors, and the
            roadmap — written by the founders.
          </p>
          <Link
            href={FUNDING_POST_HREF}
            style={{
              alignItems: 'center',
              background: ACCENT,
              borderRadius: 6,
              color: '#ffffff',
              display: 'inline-flex',
              fontSize: 14,
              fontWeight: 500,
              gap: 8,
              padding: '12px 20px',
              textDecoration: 'none',
            }}
          >
            <Sparkles size={16} />
            Read the funding announcement
            <ArrowRight size={16} />
          </Link>
        </div>
      </section>

      <FooterSection />
    </div>
  );
}
