import { Github, MessageCircle } from 'lucide-react';
import Image from 'next/image';
import Link from 'next/link';
import { FooterSection } from '@/components/footer-section';
import { Navbar } from '@/components/navbar';

const BG = '#ffffff';
const INK = '#1A1612';
const MUTED = '#5C5852';
const BORDER = '#D9D3C4';
const ACCENT = '#6D28D9';
const EYEBROW = '#8A8580';
const CARD_BG = '#FBF8F1';

type TeamMember = {
  name: string;
  role: string;
  image: string;
  bio: string;
  linkedin?: string;
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
    name: 'Greg Hale',
    role: 'Engineer',
    image: '/greg.jpg',
    linkedin: 'https://www.linkedin.com/in/greg-hale-5684b1bb/',
    bio: 'Greg works at the seam between product and compiler — translating things developers want to do into things the compiler can guarantee. He cares deeply about the path from "I have an idea" to "it just works".',
  },
  {
    name: 'Chris Watts',
    role: 'Engineer',
    image: '/seawatts.png',
    linkedin: 'https://www.linkedin.com/in/seawatts',
    bio: 'Chris owns the React and Next.js integrations and most of the developer-facing surface area. If you have ever pasted a BAML schema into an editor and watched it light up correctly, that is partly him.',
  },
  {
    name: 'Anish Palakurthi',
    role: 'Intern · S24',
    image: '/profile-anish.png',
    linkedin: 'https://www.linkedin.com/in/anish-palakurthi/',
    bio: 'Anish works on the marketing site, the playground, and the thousand small things that make BAML feel like a real product. He thinks the best documentation is a well-designed example.',
  },
  {
    name: 'Rahul',
    role: 'Intern · S25',
    image: '/rahult.jpg',
    linkedin: 'https://www.linkedin.com/in/ba11b0y/',
    bio: 'Rahul focuses on the VS Code extension and the LSP — the daily-driver experience for anyone writing BAML. He is the person to ask why your editor is doing something it should not.',
  },
  {
    name: 'Egor',
    role: 'Intern · S25',
    image: '/egor.jpg',
    linkedin: 'https://www.linkedin.com/in/egor-l/',
    bio: 'Egor works on the language core and tooling pipeline — the part of BAML that turns a .baml file into something every client SDK agrees on. He likes problems where the answer has to be exactly right.',
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
            The team behind BAML
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
            Who are we?
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
              We believe structured-output LLM work needs typed prompt
              boundaries. So we're building BAML:{' '}
              <span
                style={{
                  fontFamily: 'var(--font-serif)',
                  fontStyle: 'italic',
                  fontWeight: 500,
                  color: ACCENT,
                }}
              >
                a statically-typed, expression-oriented language with
                first-class LLM functions
              </span>
              .
            </p>
            <p
              style={{
                fontSize: 18,
                lineHeight: 1.6,
                color: MUTED,
                margin: '14px 0 0',
              }}
            >
              Yes, we're that crazy.
              <span
                style={{
                  marginLeft: 8,
                  fontSize: 14,
                  color: EYEBROW,
                }}
              >
                (we literally use Notion to present slides)
              </span>
            </p>
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
              height={1000}
              src="/team.jpg"
              width={1000}
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
              The team
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
              Builders, in order of appearance.
            </h2>
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
                    <Image
                      alt={member.name}
                      height={112}
                      src={member.image}
                      width={96}
                      style={{
                        width: '100%',
                        height: '100%',
                        objectFit: 'cover',
                      }}
                    />
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

      {/* Join community */}
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
            Get involved
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
            Join the community.
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
            Ready to build type-safe AI applications? Join thousands of
            developers who are already using BAML in production.
          </p>
          <div
            style={{
              display: 'flex',
              gap: 12,
              justifyContent: 'center',
              flexWrap: 'wrap',
            }}
          >
            <Link
              href="https://github.com/boundaryml/baml"
              style={{
                display: 'inline-flex',
                alignItems: 'center',
                gap: 8,
                padding: '12px 20px',
                borderRadius: 6,
                background: INK,
                color: BG,
                fontSize: 14,
                fontWeight: 500,
                textDecoration: 'none',
                transition: 'opacity 200ms ease',
              }}
            >
              <Github size={16} />
              Star on GitHub
            </Link>
            <Link
              href="https://boundaryml.com/discord"
              style={{
                display: 'inline-flex',
                alignItems: 'center',
                gap: 8,
                padding: '12px 20px',
                borderRadius: 6,
                background: BG,
                color: INK,
                border: `1px solid ${BORDER}`,
                fontSize: 14,
                fontWeight: 500,
                textDecoration: 'none',
                transition: 'background-color 200ms ease',
              }}
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
      `}</style>
    </div>
  );
}
