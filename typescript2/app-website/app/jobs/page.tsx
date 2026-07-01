import Link from 'next/link';
import { createMetadata } from '@/app/_lib/metadata';
import { FooterSection } from '@/components/footer-section';
import { Navbar } from '@/components/navbar';

const BG = '#ffffff';
const INK = '#1A1612';
const MUTED = '#5C5852';
const BORDER = '#D9D3C4';
const ACCENT = '#6D28D9';
const EYEBROW = '#8A8580';
const CARD_BG = '#FBF8F1';

export const metadata = createMetadata({
  description:
    "We're a small team building the first language designed for LLMs. If that sounds interesting, we'd love to hear from you.",
  eyebrow: 'Careers',
  path: '/jobs',
  title: 'Jobs at Boundary',
});

type Role = {
  title: string;
  team: string;
  location: string;
  summary: string;
  applyHref: string;
};

const ROLES: Role[] = [
  {
    title: 'Compiler Engineer',
    team: 'Language',
    location: 'San Francisco · in-person preferred',
    summary:
      'Own slices of the BAML compiler — parser, type checker, codegen. You like beautiful error messages and have opinions about Salsa, rust-analyzer, or both.',
    applyHref: 'mailto:founders@boundaryml.com?subject=Compiler%20Engineer',
  },
  {
    title: 'Runtime / Streaming Engineer',
    team: 'Runtime',
    location: 'San Francisco · in-person preferred',
    summary:
      'Build the runtime that turns a typed BAML function into a streaming, retried, multi-provider call across Python, TypeScript, Ruby, and Go.',
    applyHref: 'mailto:founders@boundaryml.com?subject=Runtime%20Engineer',
  },
  {
    title: 'Developer Tools Engineer',
    team: 'Tooling',
    location: 'Remote (US/EU overlap) · or SF',
    summary:
      'Make BAML feel real inside an editor — LSP, VS Code extension, JetBrains plugin, the playground. The interface is the product.',
    applyHref: 'mailto:founders@boundaryml.com?subject=Developer%20Tools',
  },
  {
    title: 'Founding Product Engineer',
    team: 'Product',
    location: 'San Francisco · in-person preferred',
    summary:
      'Ship the surface area users meet first: docs, marketing site, onboarding, examples. Strong taste, strong shipping cadence, comfortable across stack.',
    applyHref:
      'mailto:founders@boundaryml.com?subject=Founding%20Product%20Engineer',
  },
  {
    title: 'Developer Relations',
    team: 'Community',
    location: 'Remote · or SF',
    summary:
      'Run the weekly "ai that works" stream, write the examples nobody can stop linking to, and make the Discord the best place to talk about LLM apps.',
    applyHref: 'mailto:founders@boundaryml.com?subject=DevRel',
  },
];

const principles = [
  {
    title: 'Compiler-quality work',
    body: 'BAML is a real language with a real compiler. The bar for correctness, error messages, and tooling matches what you would expect from rust-analyzer, not from a wrapper library.',
  },
  {
    title: 'Small team, large surface area',
    body: 'Everyone here ships across language internals, runtime, devtools, and product. If you want narrow scope and a clean ticket queue, this is not that.',
  },
  {
    title: 'Open by default',
    body: 'BAML is open source. We design in the open, debate in the open, and use the same tools our users do. The playground is the playground.',
  },
];

export default function JobsPage() {
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
        <div style={{ maxWidth: 1100, margin: '0 auto' }}>
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
            Join us
          </p>
          <h1
            style={{
              fontSize: 'clamp(2.5rem, 5vw, 4.25rem)',
              fontWeight: 600,
              lineHeight: 1.02,
              letterSpacing: '-0.03em',
              margin: '20px 0 0',
              color: INK,
              maxWidth: 900,
            }}
          >
            Build a{' '}
            <span
              style={{
                fontFamily: 'var(--font-serif)',
                fontStyle: 'italic',
                fontWeight: 500,
                color: ACCENT,
              }}
            >
              real language
            </span>{' '}
            for the agent era.
          </h1>
          <p
            style={{
              fontSize: 18,
              lineHeight: 1.6,
              color: MUTED,
              margin: '28px 0 0',
              maxWidth: 680,
            }}
          >
            We're a small team building the first language designed with LLMs in
            mind — compiler, runtime, IDE tooling, and the language itself. If
            you've ever wanted to ship across all of those at once, this is the
            place.
          </p>
        </div>
      </section>

      {/* Open roles — placeholder */}
      <section style={{ padding: '80px 48px 64px' }}>
        <div style={{ maxWidth: 1100, margin: '0 auto' }}>
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
              Open roles
            </p>
            <h2
              style={{
                fontSize: 'clamp(1.5rem, 2.5vw, 2rem)',
                fontWeight: 600,
                lineHeight: 1.15,
                letterSpacing: '-0.02em',
                margin: 0,
                color: INK,
              }}
            >
              We're hiring across the stack.
            </h2>
          </div>

          <ul
            style={{
              listStyle: 'none',
              padding: 0,
              margin: 0,
              display: 'flex',
              flexDirection: 'column',
              gap: 12,
            }}
          >
            {ROLES.map((role) => (
              <li key={role.title} style={{ margin: 0 }}>
                <Link
                  href={role.applyHref}
                  className="role-card"
                  style={{
                    display: 'grid',
                    gridTemplateColumns: 'minmax(0, 1.4fr) minmax(0, 1fr) auto',
                    alignItems: 'center',
                    gap: 24,
                    padding: '22px 24px',
                    background: BG,
                    border: `1px solid ${BORDER}`,
                    borderRadius: 10,
                    textDecoration: 'none',
                    color: INK,
                    transition:
                      'background-color 200ms ease, border-color 200ms ease, transform 200ms ease, box-shadow 200ms ease',
                  }}
                >
                  <div style={{ minWidth: 0 }}>
                    <div
                      style={{
                        display: 'flex',
                        alignItems: 'center',
                        gap: 10,
                        marginBottom: 6,
                      }}
                    >
                      <span
                        style={{
                          fontSize: 11,
                          fontWeight: 500,
                          letterSpacing: '0.14em',
                          textTransform: 'uppercase',
                          color: ACCENT,
                        }}
                      >
                        {role.team}
                      </span>
                    </div>
                    <h3
                      style={{
                        fontSize: 18,
                        fontWeight: 600,
                        letterSpacing: '-0.01em',
                        margin: 0,
                        color: INK,
                        lineHeight: 1.25,
                      }}
                    >
                      {role.title}
                    </h3>
                    <p
                      style={{
                        fontSize: 14,
                        lineHeight: 1.55,
                        color: MUTED,
                        margin: '6px 0 0',
                      }}
                    >
                      {role.summary}
                    </p>
                  </div>
                  <span
                    style={{
                      fontSize: 13,
                      color: MUTED,
                      letterSpacing: '0.01em',
                    }}
                  >
                    {role.location}
                  </span>
                  <span
                    className="role-arrow"
                    aria-hidden
                    style={{
                      display: 'inline-flex',
                      alignItems: 'center',
                      justifyContent: 'center',
                      width: 36,
                      height: 36,
                      borderRadius: '50%',
                      background: CARD_BG,
                      border: `1px solid ${BORDER}`,
                      color: INK,
                      fontSize: 14,
                      transition:
                        'background-color 200ms ease, color 200ms ease, transform 200ms ease',
                    }}
                  >
                    →
                  </span>
                </Link>
              </li>
            ))}
          </ul>

          <div
            style={{
              marginTop: 32,
              padding: '24px 28px',
              background: CARD_BG,
              border: `1px solid ${BORDER}`,
              borderRadius: 10,
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'space-between',
              gap: 16,
              flexWrap: 'wrap',
            }}
          >
            <div style={{ minWidth: 0, flex: 1 }}>
              <h3
                style={{
                  fontSize: 16,
                  fontWeight: 600,
                  margin: 0,
                  color: INK,
                  letterSpacing: '-0.01em',
                }}
              >
                Don't see a fit?
              </h3>
              <p
                style={{
                  fontSize: 14,
                  lineHeight: 1.55,
                  color: MUTED,
                  margin: '4px 0 0',
                }}
              >
                Tell us what you'd build here. We hire for trajectory, not just
                role.
              </p>
            </div>
            <Link
              href="mailto:founders@boundaryml.com?subject=Joining%20Boundary"
              style={{
                display: 'inline-flex',
                alignItems: 'center',
                padding: '10px 18px',
                borderRadius: 8,
                background: INK,
                color: '#ffffff',
                fontSize: 13,
                fontWeight: 500,
                textDecoration: 'none',
                whiteSpace: 'nowrap',
              }}
            >
              founders@boundaryml.com
            </Link>
          </div>

          <style>{`
            .role-card:hover {
              border-color: ${ACCENT};
              box-shadow: 0 14px 36px -22px rgba(109,40,217,0.32);
              transform: translateY(-2px);
            }
            .role-card:hover .role-arrow {
              background: ${ACCENT};
              color: #ffffff;
              border-color: ${ACCENT};
              transform: translateX(2px);
            }
            @media (max-width: 720px) {
              .role-card {
                grid-template-columns: 1fr !important;
              }
              .role-card .role-arrow { justify-self: start; }
            }
          `}</style>
        </div>
      </section>

      {/* How we work */}
      <section
        style={{
          padding: '64px 48px 120px',
          borderTop: `1px solid ${BORDER}`,
        }}
      >
        <div style={{ maxWidth: 1100, margin: '0 auto' }}>
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
              How we work
            </p>
            <h2
              style={{
                fontSize: 'clamp(1.5rem, 2.5vw, 2rem)',
                fontWeight: 600,
                lineHeight: 1.15,
                letterSpacing: '-0.02em',
                margin: 0,
                color: INK,
              }}
            >
              A few principles, no manifesto.
            </h2>
          </div>

          <div
            style={{
              display: 'grid',
              gridTemplateColumns:
                'repeat(auto-fill, minmax(min(100%, 320px), 1fr))',
              gap: 20,
            }}
          >
            {principles.map((p) => (
              <article
                key={p.title}
                style={{
                  border: `1px solid ${BORDER}`,
                  borderRadius: 8,
                  background: BG,
                  padding: 24,
                }}
              >
                <h3
                  style={{
                    fontSize: 17,
                    fontWeight: 600,
                    letterSpacing: '-0.01em',
                    color: INK,
                    margin: 0,
                  }}
                >
                  {p.title}
                </h3>
                <p
                  style={{
                    fontSize: 15,
                    lineHeight: 1.6,
                    color: MUTED,
                    margin: '12px 0 0',
                  }}
                >
                  {p.body}
                </p>
              </article>
            ))}
          </div>
        </div>
      </section>

      <FooterSection />
    </div>
  );
}
