import { ArrowRight, Github, MessageCircle } from 'lucide-react';
import Image from 'next/image';
import Link from 'next/link';
import { createMetadata } from '@/app/_lib/metadata';
import { FooterSection } from '@/components/footer-section';
import { Navbar } from '@/components/navbar';

export const metadata = createMetadata({
  description:
    'The small team rebuilding language and tooling for a world where AI writes most of the code.',
  ogTitle: 'Our team',
  path: '/who-are-we',
  team: true,
  title: 'Who are we',
});

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

// Bios follow one shape on purpose: a single line of background, i.e. where
// each person came from, not what they "own" here. Uniform titles plus
// pedigree-only copy reads as one team of high-caliber people, not a set of
// silos.
const teamMembers: TeamMember[] = [
  {
    bio: 'Vaibhav previously built the on-device depth and face-unlock systems for Google’s Pixel 4, and worked on Microsoft’s HoloLens.',
    image: '/profile-vbv.jpeg',
    linkedin: 'https://www.linkedin.com/in/vaigup',
    name: 'Vaibhav Gupta',
    role: 'Co-founder & CEO',
    twitter: 'https://x.com/vaicode',
  },
  {
    bio: 'Aaron previously spent seven years at Amazon, scaling EC2’s internal monitoring and building live-streaming features for Prime Video and Twitch.',
    image: '/aaronv.jpg',
    linkedin: 'https://www.linkedin.com/in/aaron-villalpando-99284576/',
    name: 'Aaron Villalpando',
    role: 'Co-founder & CTO',
    twitter: 'https://x.com/aaronvi',
  },
  {
    bio: 'Sam previously worked on user identity and Cloud Firestore at Google, then built developer tooling at Trunk, after studying CS at Vanderbilt.',
    image: '/profile-sam.png',
    linkedin: 'https://www.linkedin.com/in/sxlijin/',
    name: 'Sam Lijin',
    role: 'Engineering',
    twitter: 'https://x.com/sxlijin',
  },
  {
    bio: 'Antonio has written an ACID-compliant database, an Nginx-style reverse proxy, and a memory allocator from scratch in Rust, and teaches it all to 180K+ subscribers on YouTube.',
    image: '/antonio.jpeg',
    linkedin: 'https://www.linkedin.com/in/antoniosarosi/',
    name: 'Antonio Sarosi',
    role: 'Engineering',
    twitter: 'https://x.com/antoniosarosi',
  },
  {
    bio: 'Paulo is a former Y Combinator founder who grew his startup past $2M in ARR, and was an early BAML user before he joined.',
    image: '/testimonials/people/paulo.png',
    name: 'Paulo Rossi',
    role: 'Engineering',
  },
  {
    bio: 'Kai built ere, a Rust crate that compiles regular expressions at build time into efficient, type-checked code, and a VS Code extension with over a million downloads, after an MS in computer science.',
    image: '/kai.png',
    name: 'Kai Orita',
    role: 'Engineering',
  },
  {
    bio: 'Avery revived the defunct LEGO Universe MMO with a custom C++ server emulator in high school, then wrote a full wgpu rendering backend from scratch and contributed to the Slint compiler while finishing a degree at UPenn.',
    image: '/avery.png',
    linkedin: 'https://www.linkedin.com/in/codeshaunted/',
    name: 'Avery Townsend',
    role: 'Engineering',
    twitter: 'https://x.com/codeshaunted',
  },
  {
    bio: 'Dhilan is a computer science intern from UT Austin who built agent-tries-baml, the system BAML uses to measure and improve how well AI agents write it.',
    image: '/dhilan.png',
    name: 'Dhilan Shah',
    role: 'Engineering',
    twitter: 'https://x.com/_dhilan_shah_',
  },
];

export default function WhoAreWePage() {
  return (
    <div
      style={{
        background: BG,
        color: INK,
        margin: '0 auto',
        maxWidth: 1600,
        minHeight: '100vh',
        width: '100%',
      }}
    >
      <Navbar />

      {/* Hero: text and team photo side by side */}
      <section
        style={{
          borderBottom: `1px solid ${BORDER}`,
          padding: '88px 48px 72px',
        }}
      >
        <div className="hero-grid" style={{ margin: '0 auto', maxWidth: 1200 }}>
          <div>
            <h1
              style={{
                color: INK,
                fontSize: 'clamp(2.5rem, 5.5vw, 4.5rem)',
                fontWeight: 600,
                letterSpacing: '-0.03em',
                lineHeight: 1.02,
                margin: 0,
              }}
            >
              Our Team
            </h1>
            <div style={{ marginTop: 28 }}>
              <p
                style={{
                  color: MUTED,
                  fontSize: 18,
                  lineHeight: 1.6,
                  margin: 0,
                }}
              >
                Coding has changed more in the last two years than in the twenty
                before it. The languages, the tools, and the type systems we all
                rely on were designed for a world where humans did the writing.
                We're a small team rebuilding those foundations for a world
                where AI does most of it.
              </p>
            </div>
          </div>
          <div
            style={{
              background: CARD_BG,
              border: `1px solid ${BORDER}`,
              borderRadius: 8,
              overflow: 'hidden',
            }}
          >
            <Image
              alt="Our team"
              height={1333}
              src="/team.jpg"
              style={{
                display: 'block',
                height: '100%',
                objectFit: 'cover',
                width: '100%',
              }}
              width={2000}
            />
          </div>
        </div>
      </section>

      {/* Team grid */}
      <section style={{ padding: '64px 48px 96px' }}>
        <div style={{ margin: '0 auto', maxWidth: 1200 }}>
          <div
            style={{
              alignItems: 'baseline',
              borderBottom: `1px solid ${BORDER}`,
              display: 'flex',
              gap: 16,
              marginBottom: 32,
              paddingBottom: 16,
            }}
          >
            <p
              style={{
                color: EYEBROW,
                fontSize: 12,
                fontWeight: 500,
                letterSpacing: '0.14em',
                margin: 0,
                textTransform: 'uppercase',
              }}
            >
              People building BAML
            </p>
          </div>

          <div
            style={{
              display: 'grid',
              gap: 16,
              gridTemplateColumns:
                'repeat(auto-fill, minmax(min(100%, 380px), 1fr))',
            }}
          >
            {teamMembers.map((member) => (
              <article className="team-card" key={member.name}>
                <div className="team-photo-col">
                  <div className="team-photo">
                    {member.image ? (
                      <Image
                        alt={member.name}
                        className="team-photo-img"
                        height={800}
                        src={member.image}
                        width={640}
                      />
                    ) : (
                      <div
                        aria-label={member.name}
                        className="team-photo-fallback"
                        role="img"
                      >
                        {member.name
                          .split(' ')
                          .map((part) => part[0])
                          .join('')
                          .slice(0, 2)}
                      </div>
                    )}
                  </div>
                  {(member.linkedin || member.twitter) && (
                    <div className="team-links">
                      {member.twitter && (
                        <Link
                          aria-label={`${member.name} on X`}
                          className="team-icon"
                          href={member.twitter}
                          target="_blank"
                        >
                          <svg
                            aria-hidden
                            fill="currentColor"
                            height="14"
                            viewBox="0 0 24 24"
                            width="14"
                          >
                            <path d="M18.244 2.25h3.308l-7.227 8.26 8.502 11.24H16.17l-5.214-6.817L4.99 21.75H1.68l7.73-8.835L1.254 2.25H8.08l4.713 6.231zm-1.161 17.52h1.833L7.084 4.126H5.117z" />
                          </svg>
                        </Link>
                      )}
                      {member.linkedin && (
                        <Link
                          aria-label={`${member.name} on LinkedIn`}
                          className="team-icon team-icon-linkedin"
                          href={member.linkedin}
                          target="_blank"
                        >
                          <svg
                            aria-hidden
                            fill="currentColor"
                            height="15"
                            viewBox="0 0 24 24"
                            width="15"
                          >
                            <path d="M20.447 20.452h-3.554v-5.569c0-1.328-.027-3.037-1.852-3.037-1.853 0-2.136 1.445-2.136 2.939v5.667H9.351V9h3.414v1.561h.046c.477-.9 1.637-1.85 3.37-1.85 3.601 0 4.268 2.37 4.268 5.455v6.286zM5.337 7.433a2.062 2.062 0 01-2.063-2.065 2.063 2.063 0 112.063 2.065zm1.782 13.019H3.555V9h3.564v11.452zM22.225 0H1.771C.792 0 0 .774 0 1.729v20.542C0 23.227.792 24 1.771 24h20.451C23.2 24 24 23.227 24 22.271V1.729C24 .774 23.2 0 22.222 0h.003z" />
                          </svg>
                        </Link>
                      )}
                    </div>
                  )}
                </div>
                <div className="team-text">
                  <p className="team-role">{member.role}</p>
                  <h3 className="team-name">{member.name}</h3>
                  <p className="team-bio">{member.bio}</p>
                </div>
              </article>
            ))}
          </div>
        </div>
      </section>

      {/* Hiring CTA */}
      <section
        style={{
          borderTop: `1px solid ${BORDER}`,
          padding: '80px 48px 120px',
        }}
      >
        <div style={{ margin: '0 auto', maxWidth: 720, textAlign: 'center' }}>
          <p
            style={{
              color: EYEBROW,
              fontSize: 12,
              fontWeight: 500,
              letterSpacing: '0.14em',
              margin: 0,
              textTransform: 'uppercase',
            }}
          >
            Join us
          </p>
          <h2
            style={{
              color: INK,
              fontSize: 'clamp(1.75rem, 3.5vw, 2.5rem)',
              fontWeight: 600,
              letterSpacing: '-0.02em',
              lineHeight: 1.1,
              margin: '16px 0 0',
            }}
          >
            Work on the hard parts.
          </h2>
          <p
            style={{
              color: MUTED,
              fontSize: 17,
              lineHeight: 1.6,
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
        .hero-grid {
          display: grid;
          grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
          gap: 56px;
          align-items: center;
        }
        @media (max-width: 860px) {
          .hero-grid {
            grid-template-columns: 1fr;
            gap: 36px;
          }
        }
        .team-card {
          display: flex;
          flex-direction: row;
          gap: 18px;
          align-items: flex-start;
          background: ${CARD_BG};
          border: 1px solid ${BORDER};
          border-radius: 10px;
          padding: 20px;
          transition: box-shadow 200ms ease, transform 200ms ease;
        }
        .team-card:hover {
          box-shadow: 0 12px 36px -22px rgba(0,0,0,0.18);
          transform: translateY(-2px);
        }
        .team-photo-col {
          flex: 0 0 auto;
          display: flex;
          flex-direction: column;
          align-items: center;
          gap: 10px;
        }
        .team-photo {
          flex: 0 0 auto;
          width: 104px;
          aspect-ratio: 1 / 1;
          overflow: hidden;
          border-radius: 8px;
          border: 1px solid ${BORDER};
          background: linear-gradient(135deg, #FFFDF6 0%, #F4EEE2 100%);
        }
        .team-photo-img {
          width: 100%;
          height: 100%;
          object-fit: cover;
          display: block;
        }
        .team-photo-fallback {
          display: flex;
          align-items: center;
          justify-content: center;
          width: 100%;
          height: 100%;
          color: ${ACCENT};
          font-size: 30px;
          font-weight: 600;
          letter-spacing: -0.04em;
        }
        .team-text {
          flex: 1 1 auto;
          min-width: 0;
          display: flex;
          flex-direction: column;
        }
        .team-role {
          margin: 0;
          font-family: var(--font-geist-mono), ui-monospace, monospace;
          font-size: 11px;
          letter-spacing: 0.12em;
          text-transform: uppercase;
          color: ${EYEBROW};
        }
        .team-name {
          margin: 5px 0 0;
          font-family: var(--font-geist-sans), ui-sans-serif, system-ui, sans-serif;
          font-style: normal;
          font-weight: 600;
          font-size: 22px;
          line-height: 1.1;
          letter-spacing: -0.01em;
          color: ${INK};
        }
        .team-bio {
          margin: 10px 0 0;
          font-size: 13.5px;
          line-height: 1.5;
          color: ${MUTED};
        }
        .team-links {
          display: flex;
          gap: 8px;
          align-items: center;
          justify-content: center;
        }
        .team-icon {
          display: inline-flex;
          align-items: center;
          justify-content: center;
          width: 24px;
          height: 24px;
          color: ${MUTED};
          transition: color 200ms ease;
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
