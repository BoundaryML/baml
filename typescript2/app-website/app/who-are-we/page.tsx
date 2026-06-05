import { ArrowRight, Github, MessageCircle } from 'lucide-react';
import Image from 'next/image';
import Link from 'next/link';
import { FooterSection } from '@/components/footer-section';
import { Navbar } from '@/components/navbar';

const BG = '#FBF7ED';
const INK = '#1A1612';
const MUTED = '#5C5852';
const BORDER = '#D9D3C4';
const ACCENT = '#6D28D9';
const EYEBROW = '#8A8580';
const CARD_BG = '#FBF8F1';

type TeamMember = {
  bio: string;
  image?: string;
  linkedin?: string;
  name: string;
  role: string;
  twitter?: string;
};

const teamMembers: TeamMember[] = [
  {
    name: 'Vaibhav Gupta',
    role: 'CEO & Co-founder',
    image: '/profile-vbv.jpeg',
    linkedin: 'https://www.linkedin.com/in/vaigup',
    bio: 'Vaibhav previously built computer vision systems at Google and Microsoft, where he kept watching production AI fail in ways the type system was supposed to prevent. He started BAML to make working with LLMs feel like working with any other typed boundary in the stack.',
  },
  {
    name: 'Aaron Villalpando',
    role: 'CTO & Co-founder',
    image: '/aaronv.jpg',
    linkedin: 'https://www.linkedin.com/in/aaron-villalpando-99284576/',
    bio: 'Aaron spent years at Amazon shipping high-throughput AI systems and learned exactly which abstractions break under real load. He thinks the most important UX decision in any AI tool is the one a developer never has to think about.',
  },
  {
    name: 'Sam Lijin',
    role: 'Engineer',
    image: '/profile-sam.png',
    linkedin: 'https://www.linkedin.com/in/sxlijin/',
    bio: 'Sam works on the BAML compiler and runtime. He has strong opinions about parser error messages and a soft spot for the kind of devtools that make a hard problem feel easy.',
  },
  {
    name: 'Antonio Sarosi',
    role: 'Engineer',
    image: '/antonio.jpeg',
    linkedin: 'https://www.linkedin.com/in/antoniosarosi/',
    bio: 'Antonio builds the language internals — type checker, codegen, the bits nobody sees but everyone relies on. He believes a language is judged by how forgiving it is at 2am.',
  },
  {
    bio: 'Paulo works with teams building production AI systems, turning hard-won implementation feedback into sharper workflows for BAML users.',
    image: '/testimonials/people/paulo.png',
    name: 'Paulo Rossi',
    role: 'Engineer',
  },
  {
    bio: 'Kai works across the product surface and developer experience, helping BAML feel direct, legible, and fast to adopt in real codebases.',
    image: '/kai.png',
    name: 'Kai',
    role: 'Engineer',
  },
  {
    bio: 'Avery works on the places where language tooling meets product polish, making BAML easier to understand, test, and ship.',
    image: '/avery.png',
    name: 'Avery',
    role: 'Engineer',
  },
  {
    name: 'Dhilan Shah',
    role: 'Engineer',
    image: '/dhilan.png',
    bio: 'Dhilan works across the stack on the parts of BAML that developers actually touch — the website, the playground, and the integrations that make the language feel approachable. He believes the fastest way to learn a tool is to try it and have it not get in your way.',
  },
];

export default function WhoAreWePage() {
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
            Hiring
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
            Our Team
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
              We built BAML to solve hard problems at the edge of production AI.
              Our work spans distributed systems, compilers, developer tools,
              and the constantly changing shape of modern AI.
            </p>
            <div className="team-cta-row" style={{ marginTop: 28 }}>
              <Link
                className="editorial-btn editorial-btn--primary"
                href="/jobs"
              >
                See open roles
                <ArrowRight size={16} />
              </Link>
            </div>
          </div>
        </div>
      </section>

      {/* Team photo */}
      <section
        style={{
          padding: '64px 48px 0',
        }}
      >
        <div style={{ maxWidth: 1200, margin: '0 auto' }}>
          <div
            style={{
              border: `1px solid ${BORDER}`,
              borderRadius: 8,
              overflow: 'hidden',
              background: CARD_BG,
            }}
          >
            <Image
              alt="Our team"
              className="w-full"
              height={1333}
              src="/team.jpg"
              width={2000}
              style={{ width: '100%', height: 'auto', display: 'block' }}
            />
          </div>
        </div>
      </section>

      {/* Team grid */}
      <section style={{ padding: '64px 48px 96px' }}>
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
              People building BAML
            </p>
          </div>

          <div
            style={{
              display: 'grid',
              gridTemplateColumns:
                'repeat(auto-fill, minmax(min(100%, 360px), 1fr))',
              gap: 20,
            }}
          >
            {teamMembers.map((member) => (
              <article
                key={member.name}
                style={{
                  background: BG,
                  border: `1px solid ${BORDER}`,
                  borderRadius: 8,
                  padding: 20,
                  display: 'flex',
                  flexDirection: 'column',
                  gap: 16,
                  transition:
                    'box-shadow 200ms ease, transform 200ms ease, background-color 200ms ease',
                }}
                className="team-card"
              >
                <div
                  style={{
                    display: 'grid',
                    gridTemplateColumns: '96px 1fr',
                    gap: 16,
                    alignItems: 'start',
                  }}
                >
                  <div
                    style={{
                      width: 96,
                      height: 112,
                      borderRadius: 6,
                      overflow: 'hidden',
                      border: `1px solid ${BORDER}`,
                      background: CARD_BG,
                      flexShrink: 0,
                    }}
                  >
                    {member.image ? (
                      <Image
                        alt={member.name}
                        height={112}
                        src={member.image}
                        style={{
                          height: '100%',
                          objectFit: 'cover',
                          width: '100%',
                        }}
                        width={96}
                      />
                    ) : (
                      <div
                        aria-label={member.name}
                        role="img"
                        style={{
                          alignItems: 'center',
                          background:
                            'linear-gradient(135deg, #FFFDF6 0%, #F4EEE2 100%)',
                          color: ACCENT,
                          display: 'flex',
                          fontSize: 28,
                          fontWeight: 600,
                          height: '100%',
                          justifyContent: 'center',
                          letterSpacing: '-0.04em',
                          width: '100%',
                        }}
                      >
                        {member.name
                          .split(' ')
                          .map((part) => part[0])
                          .join('')
                          .slice(0, 2)}
                      </div>
                    )}
                  </div>
                  <div
                    style={{
                      display: 'flex',
                      flexDirection: 'column',
                      gap: 4,
                      minWidth: 0,
                    }}
                  >
                    <h3
                      style={{
                        fontSize: 18,
                        fontWeight: 600,
                        letterSpacing: '-0.015em',
                        color: INK,
                        margin: 0,
                        lineHeight: 1.2,
                      }}
                    >
                      {member.name}
                    </h3>
                    <p
                      style={{
                        fontSize: 13,
                        fontWeight: 500,
                        color: MUTED,
                        margin: 0,
                        lineHeight: 1.3,
                      }}
                    >
                      {member.role}
                    </p>
                    <div
                      style={{
                        display: 'flex',
                        gap: 8,
                        marginTop: 8,
                        alignItems: 'center',
                      }}
                    >
                      {member.twitter && (
                        <Link
                          href={member.twitter}
                          target="_blank"
                          aria-label={`${member.name} on X`}
                          style={{
                            display: 'inline-flex',
                            alignItems: 'center',
                            justifyContent: 'center',
                            width: 24,
                            height: 24,
                            color: MUTED,
                            transition: 'color 200ms ease',
                          }}
                          className="team-icon"
                        >
                          <svg
                            width="14"
                            height="14"
                            viewBox="0 0 24 24"
                            fill="currentColor"
                            aria-hidden
                          >
                            <path d="M18.244 2.25h3.308l-7.227 8.26 8.502 11.24H16.17l-5.214-6.817L4.99 21.75H1.68l7.73-8.835L1.254 2.25H8.08l4.713 6.231zm-1.161 17.52h1.833L7.084 4.126H5.117z" />
                          </svg>
                        </Link>
                      )}
                      {member.linkedin && (
                        <Link
                          href={member.linkedin}
                          target="_blank"
                          aria-label={`${member.name} on LinkedIn`}
                          style={{
                            display: 'inline-flex',
                            alignItems: 'center',
                            justifyContent: 'center',
                            width: 24,
                            height: 24,
                            color: MUTED,
                            transition: 'color 200ms ease',
                          }}
                          className="team-icon team-icon-linkedin"
                        >
                          <svg
                            width="15"
                            height="15"
                            viewBox="0 0 24 24"
                            fill="currentColor"
                            aria-hidden
                          >
                            <path d="M20.447 20.452h-3.554v-5.569c0-1.328-.027-3.037-1.852-3.037-1.853 0-2.136 1.445-2.136 2.939v5.667H9.351V9h3.414v1.561h.046c.477-.9 1.637-1.85 3.37-1.85 3.601 0 4.268 2.37 4.268 5.455v6.286zM5.337 7.433a2.062 2.062 0 01-2.063-2.065 2.063 2.063 0 112.063 2.065zm1.782 13.019H3.555V9h3.564v11.452zM22.225 0H1.771C.792 0 0 .774 0 1.729v20.542C0 23.227.792 24 1.771 24h20.451C23.2 24 24 23.227 24 22.271V1.729C24 .774 23.2 0 22.222 0h.003z" />
                          </svg>
                        </Link>
                      )}
                    </div>
                  </div>
                </div>
                <p
                  style={{
                    fontSize: 14,
                    lineHeight: 1.55,
                    color: MUTED,
                    margin: 0,
                  }}
                >
                  {member.bio}
                </p>
              </article>
            ))}
          </div>
        </div>
      </section>

      {/* Hiring CTA */}
      <section
        style={{
          padding: '80px 48px 120px',
          borderTop: `1px solid ${BORDER}`,
        }}
      >
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
            Join us
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
            Work on the hard parts.
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
            We are looking for people who can move between product and systems,
            write clearly, debug patiently, and care about making AI software
            less fragile.
          </p>
          <div className="team-cta-row team-cta-row--center">
            <Link className="editorial-btn editorial-btn--primary" href="/jobs">
              Open roles
              <ArrowRight size={16} />
            </Link>
            <Link
              className="editorial-btn editorial-btn--dark"
              href="https://github.com/boundaryml/baml"
              rel="noreferrer"
              target="_blank"
            >
              <Github size={16} />
              Star on GitHub
            </Link>
            <Link
              className="editorial-btn"
              href="https://boundaryml.com/discord"
              rel="noreferrer"
              target="_blank"
            >
              <MessageCircle size={16} />
              Join Discord
            </Link>
          </div>
        </div>
      </section>

      <FooterSection />

      <style>{`
        .team-card:hover {
          box-shadow: 0 12px 36px -22px rgba(0,0,0,0.18);
          transform: translateY(-2px);
        }
        .team-icon:hover { color: ${ACCENT}; }
        .team-icon-linkedin:hover { color: #0A66C2; }

        .team-cta-row {
          display: flex;
          flex-wrap: wrap;
          gap: 12px;
        }
        .team-cta-row--center { justify-content: center; }
        .editorial-btn {
          align-items: center;
          background: #ffffff;
          border: 1px solid ${BORDER};
          border-radius: 999px;
          color: ${INK};
          display: inline-flex;
          font-family: inherit;
          font-size: 14px;
          font-weight: 500;
          gap: 8px;
          letter-spacing: 0.01em;
          padding: 12px 22px;
          text-decoration: none;
          transition: background-color 200ms ease, border-color 200ms ease, color 200ms ease, transform 200ms ease;
        }
        .editorial-btn:hover {
          background: #FBF8F1;
          border-color: ${ACCENT};
          color: ${ACCENT};
          transform: translateY(-1px);
        }
        .editorial-btn--primary {
          background: ${ACCENT};
          border-color: ${ACCENT};
          color: #ffffff;
        }
        .editorial-btn--primary:hover {
          background: #5B21B6;
          border-color: #5B21B6;
          color: #ffffff;
        }
        .editorial-btn--dark {
          background: ${INK};
          border-color: ${INK};
          color: #ffffff;
        }
        .editorial-btn--dark:hover {
          background: #2A211A;
          border-color: #2A211A;
          color: #ffffff;
        }
        @media (max-width: 640px) {
          .team-cta-row {
            flex-direction: column;
            align-items: stretch;
          }
          .editorial-btn {
            justify-content: center;
            width: 100%;
          }
        }
      `}</style>
    </div>
  );
}
