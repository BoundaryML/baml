import { createMetadata } from '@/app/_lib/metadata';
import { Navbar } from '@/components/navbar';
import { type EapEvent, getEapEvents } from '@/lib/luma';

// Pull the live EAP onboarding sessions from Luma. Revalidated on the same
// cadence as the underlying fetch (see lib/luma.ts).
export const metadata = createMetadata({
  description: 'Book a live BAML early access onboarding session.',
  eyebrow: 'Early Access',
  path: '/eap',
  title: 'Early Access Sign Up',
});

// The public Luma calendar, so people can browse everything if nothing is listed.
const LUMA_CALENDAR_URL = 'https://luma.com/baml';

const CSS = `
.eap-wrap { margin: 0 auto; max-width: 560px; padding: 120px 24px 96px; }
.eap-h1 { font-size: clamp(36px, 6vw, 52px); font-weight: 600; letter-spacing: -0.02em; line-height: 1.05; margin: 0; }
.eap-sub { color: #6b6456; font-size: 18px; line-height: 1.6; margin: 18px 0 40px; }
.eap-sessions { display: flex; flex-direction: column; gap: 12px; }
.eap-session { display: flex; align-items: center; justify-content: space-between; gap: 16px;
  border: 1px solid #e0dac9; border-radius: 12px; padding: 18px 20px; text-decoration: none;
  color: inherit; transition: border-color 0.12s ease, background 0.12s ease; }
.eap-session:hover { border-color: #1a1a1a; background: rgba(0,0,0,0.02); }
.eap-session-left { display: flex; flex-direction: column; gap: 4px; }
.eap-session-kicker { color: #8a8372; font-size: 12px; font-weight: 500; letter-spacing: 0.04em; text-transform: uppercase; }
.eap-session-date { font-size: 19px; font-weight: 600; letter-spacing: -0.01em; }
.eap-session-meta { display: flex; align-items: center; gap: 8px; margin-top: 2px; color: #8a8372; font-size: 13px; }
.eap-badge { display: inline-flex; align-items: center; border-radius: 999px; padding: 2px 9px; font-size: 12px; font-weight: 500; }
.eap-badge--open { background: rgba(22,163,74,0.12); color: #15803d; }
.eap-badge--waitlist { background: rgba(217,119,6,0.12); color: #b45309; }
.eap-badge--closed { background: rgba(0,0,0,0.06); color: #6b6456; }
.eap-session-cta { color: #2563eb; font-size: 14px; font-weight: 500; white-space: nowrap; }
.eap-empty { border: 1px dashed #e0dac9; border-radius: 12px; padding: 28px 20px; text-align: center; color: #6b6456; }
.eap-empty a { color: #2563eb; font-weight: 500; text-decoration: none; }
.eap-footer { margin-top: 28px; font-size: 14px; color: #8a8372; }
.eap-footer a { color: #2563eb; text-decoration: none; }
`;

interface Availability {
  label: string;
  className: string;
  bookable: boolean;
}

function availabilityOf(event: EapEvent): Availability {
  if (event.waitlist_status === 'enabled') {
    return {
      bookable: true,
      className: 'eap-badge--waitlist',
      label: 'Waitlist open',
    };
  }
  if (event.registration_open === false) {
    return {
      bookable: false,
      className: 'eap-badge--closed',
      label: 'Registration closed',
    };
  }
  return { bookable: true, className: 'eap-badge--open', label: 'Open' };
}

function formatWhen(event: EapEvent): string {
  const start = new Date(event.start_at);
  const date = new Intl.DateTimeFormat('en-US', {
    day: 'numeric',
    month: 'short',
    timeZone: event.timezone,
    weekday: 'short',
  }).format(start);
  const time = new Intl.DateTimeFormat('en-US', {
    hour: 'numeric',
    minute: '2-digit',
    timeZone: event.timezone,
    timeZoneName: 'short',
  }).format(start);
  return `${date} · ${time}`;
}

export default async function EapPage() {
  const events = await getEapEvents();

  return (
    <>
      {/* eslint-disable-next-line react/no-danger */}
      <style dangerouslySetInnerHTML={{ __html: CSS }} />
      <Navbar />
      <main className="eap-wrap">
        <h1 className="eap-h1">Early Access Sign Up</h1>
        <p className="eap-sub">
          Join a live onboarding session to get started with BAML. Pick a time
          that works and book your spot below.
        </p>

        {events.length > 0 ? (
          <div className="eap-sessions">
            {events.map((event) => {
              const availability = availabilityOf(event);
              return (
                <a
                  className="eap-session"
                  href={event.url}
                  key={event.id}
                  rel="noopener noreferrer"
                  target="_blank"
                >
                  <span className="eap-session-left">
                    <span className="eap-session-kicker">{event.name}</span>
                    <span className="eap-session-date">
                      {formatWhen(event)}
                    </span>
                    <span className="eap-session-meta">
                      <span className={`eap-badge ${availability.className}`}>
                        {availability.label}
                      </span>
                      {event.goingCount !== null && (
                        <span>{event.goingCount} going</span>
                      )}
                    </span>
                  </span>
                  <span className="eap-session-cta">
                    {availability.bookable ? 'Book' : 'View'} &rarr;
                  </span>
                </a>
              );
            })}
          </div>
        ) : (
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
        )}

        <p className="eap-footer">
          See every session on the{' '}
          <a href={LUMA_CALENDAR_URL} rel="noopener noreferrer" target="_blank">
            BAML Luma calendar
          </a>
          .
        </p>
      </main>
    </>
  );
}
