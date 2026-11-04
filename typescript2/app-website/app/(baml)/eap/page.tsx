import Link from 'next/link';
import { createMetadata } from '@/app/_lib/metadata';
import { DiscordCta } from '@/components/discord-cta';
import { Navbar } from '@/components/navbar';
import { getEapEvents } from '@/lib/luma';
import './eap.css';
import { EapSchedule } from './eap-schedule';

// Pull the live EAP onboarding sessions from Luma. Revalidated on the same
// cadence as the underlying fetch (see lib/luma.ts).
export const metadata = createMetadata({
  description: 'Book a live BAML early access onboarding session.',
  ogTitle: 'Get BAML early',
  path: '/eap',
  title: 'Early Access Sign Up',
});

// The public Luma calendar, so people can browse everything if nothing is listed.
const LUMA_CALENDAR_URL = 'https://luma.com/baml';
export default async function EapPage() {
  const events = await getEapEvents();

  // Secondary CTA. Rendered inside the schedule's primary column (under the
  // featured session) so it fills the space there rather than sitting below
  // the whole list. Passed as a slot because the schedule is a client component.
  const discordCta = <DiscordCta />;

  return (
    <>
      <Navbar />
      <div className="eap">
        <main className="eap-wrap">
          <div className="eap-header">
            <h1 className="eap-h1">Early Access Sign Up</h1>
            <p className="eap-sub">
              Get started with BAML.{' '}
              <Link className="eap-sub-link" href="/explore">
                Explore on your own
              </Link>
              , or join a live session to go deeper with us in real time: ask
              questions, see the latest features, and work through your actual
              use case together.{' '}
              <span className="eap-sub-dim">About 45 minutes on Zoom.</span>
            </p>
          </div>

          {events.length > 0 ? (
            <EapSchedule discord={discordCta} events={events} />
          ) : (
            <>
              <div className="eap-empty">
                No sessions are scheduled right now. Browse the{' '}
                <a
                  href={LUMA_CALENDAR_URL}
                  rel="noopener noreferrer"
                  target="_blank"
                >
                  BAML calendar
                </a>{' '}
                for upcoming events.
              </div>
              {discordCta}
            </>
          )}

          <p className="eap-footer">
            See every session on the{' '}
            <a
              href={LUMA_CALENDAR_URL}
              rel="noopener noreferrer"
              target="_blank"
            >
              BAML Luma calendar
            </a>
            .
          </p>
        </main>
      </div>
    </>
  );
}
