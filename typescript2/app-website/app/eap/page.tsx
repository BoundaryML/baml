import type { Metadata } from 'next';

import { Navbar } from '@/components/navbar';

// Minimal static early-access page: just the onboarding-session signup links.
const baseUrl =
  process.env.NEXT_PUBLIC_BASE_URL ??
  (process.env.VERCEL_PROJECT_PRODUCTION_URL
    ? `https://${process.env.VERCEL_PROJECT_PRODUCTION_URL}`
    : 'https://boundaryml.com');

export const metadata: Metadata = {
  alternates: { canonical: `${baseUrl}/eap` },
  description: 'Sign up for a BAML early access onboarding session.',
  openGraph: {
    description: 'Sign up for a BAML early access onboarding session.',
    siteName: 'BAML',
    title: 'Early Access Sign Up',
    type: 'website',
    url: `${baseUrl}/eap`,
  },
  title: 'Early Access Sign Up | BAML',
};

const SESSIONS = [
  { date: 'June 11', href: 'https://luma.com/baml-eap-jun-11' },
  { date: 'June 18', href: 'https://luma.com/baml-eap-jun-18' },
];

const CSS = `
.eap-wrap { margin: 0 auto; max-width: 520px; padding: 120px 24px 96px; }
.eap-h1 { font-size: clamp(36px, 6vw, 52px); font-weight: 600; letter-spacing: -0.02em; line-height: 1.05; margin: 0; }
.eap-sub { color: #6b6456; font-size: 18px; line-height: 1.6; margin: 18px 0 40px; }
.eap-sessions { display: flex; flex-direction: column; gap: 12px; }
.eap-session { display: flex; align-items: center; justify-content: space-between; gap: 16px;
  border: 1px solid #e0dac9; border-radius: 12px; padding: 18px 20px; text-decoration: none;
  color: inherit; transition: border-color 0.12s ease, background 0.12s ease; }
.eap-session:hover { border-color: #1a1a1a; background: rgba(0,0,0,0.02); }
.eap-session-left { display: flex; flex-direction: column; gap: 2px; }
.eap-session-kicker { color: #8a8372; font-size: 12px; font-weight: 500; letter-spacing: 0.04em; text-transform: uppercase; }
.eap-session-date { font-size: 19px; font-weight: 600; letter-spacing: -0.01em; }
.eap-session-cta { color: #2563eb; font-size: 14px; font-weight: 500; white-space: nowrap; }
`;

export default function EapPage() {
  return (
    <>
      {/* eslint-disable-next-line react/no-danger */}
      <style dangerouslySetInnerHTML={{ __html: CSS }} />
      <Navbar />
      <main className="eap-wrap">
        <h1 className="eap-h1">Early Access Sign Up</h1>
        <p className="eap-sub">
          Join a live onboarding session to get started with BAML.
        </p>
        <div className="eap-sessions">
          {SESSIONS.map((s) => (
            <a
              key={s.href}
              className="eap-session"
              href={s.href}
              rel="noopener noreferrer"
              target="_blank"
            >
              <span className="eap-session-left">
                <span className="eap-session-kicker">Onboarding session</span>
                <span className="eap-session-date">{s.date}</span>
              </span>
              <span className="eap-session-cta">Sign up &rarr;</span>
            </a>
          ))}
        </div>
      </main>
    </>
  );
}
