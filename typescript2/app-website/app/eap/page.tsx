import Link from 'next/link';
import { createMetadata } from '@/app/_lib/metadata';
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
const DISCORD_URL = 'https://boundaryml.com/discord';

const DISCORD_PATH =
  'M20.317 4.3698a19.7913 19.7913 0 00-4.8851-1.5152.0741.0741 0 00-.0785.0371c-.211.3753-.4447.8648-.6083 1.2495-1.8447-.2762-3.68-.2762-5.4868 0-.1636-.3933-.4058-.8742-.6177-1.2495a.077.077 0 00-.0785-.037 19.7363 19.7363 0 00-4.8852 1.515.0699.0699 0 00-.0321.0277C.5334 9.0458-.319 13.5799.0992 18.0578a.0824.0824 0 00.0312.0561c2.0528 1.5076 4.0413 2.4228 5.9929 3.0294a.0777.0777 0 00.0842-.0276c.4616-.6304.8731-1.2952 1.226-1.9942a.076.076 0 00-.0416-.1057c-.6528-.2476-1.2743-.5495-1.8722-.8923a.077.077 0 01-.0076-.1277c.1258-.0943.2517-.1923.3718-.2914a.0743.0743 0 01.0776-.0105c3.9278 1.7933 8.18 1.7933 12.0614 0a.0739.0739 0 01.0785.0095c.1202.099.246.1981.3728.2924a.077.077 0 01-.0066.1276 12.2986 12.2986 0 01-1.873.8914.0766.0766 0 00-.0407.1067c.3604.698.7719 1.3628 1.225 1.9932a.076.076 0 00.0842.0286c1.961-.6067 3.9495-1.5219 6.0023-3.0294a.077.077 0 00.0313-.0552c.5004-5.177-.8382-9.6739-3.5485-13.6604a.061.061 0 00-.0312-.0286zM8.02 15.3312c-1.1825 0-2.1569-1.0857-2.1569-2.419 0-1.3332.9555-2.4189 2.157-2.4189 1.2108 0 2.1757 1.0952 2.1568 2.419 0 1.3332-.9555 2.4189-2.1569 2.4189zm7.9748 0c-1.1825 0-2.1569-1.0857-2.1569-2.419 0-1.3332.9554-2.4189 2.1569-2.4189 1.2108 0 2.1757 1.0952 2.1568 2.419 0 1.3332-.946 2.4189-2.1568 2.4189Z';

export default async function EapPage() {
  const events = await getEapEvents();

  // Secondary CTA. Rendered inside the schedule's primary column (under the
  // featured session) so it fills the space there rather than sitting below
  // the whole list. Passed as a slot because the schedule is a client component.
  const discordCta = (
    <section className="eap-cta-band">
      <div className="eap-cta-head">
        <svg aria-hidden="true" className="eap-cta-ico" viewBox="0 0 24 24">
          <path d={DISCORD_PATH} fill="currentColor" />
        </svg>
        <div className="eap-cta-title">Questions or feedback?</div>
      </div>
      <p className="eap-cta-sub">
        Join the BAML Discord. We're around to help you get set up, and we read
        every bit of feedback.
      </p>
      <a
        className="eap-discord-btn"
        href={DISCORD_URL}
        rel="noopener noreferrer"
        target="_blank"
      >
        <svg aria-hidden="true" viewBox="0 0 24 24">
          <path d={DISCORD_PATH} />
        </svg>
        Join the Discord
      </a>
    </section>
  );

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
