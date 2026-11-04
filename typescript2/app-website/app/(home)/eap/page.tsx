import { createMetadata } from '@/app/_lib/metadata';
import { DiscordCard } from '@/components/community-cards';
import { section } from '@/components/home/analytics';
import { PAGE_WRAP, PageShell } from '@/components/home/page-shell';
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

// The early access sign-up page, in the homepage's BAML lane. It isn't linked from the nav or the
// footer.
export default async function EapPage() {
  const events = await getEapEvents();

  // Secondary CTA. Rendered inside the schedule's primary column (under the
  // featured session) so it fills the space there rather than sitting below
  // the whole list. Passed as a slot because the schedule is a client component.
  const discordCta = (
    <div className="mt-5">
      <DiscordCard />
    </div>
  );

  return (
    <PageShell>
      <section
        {...section('eap_schedule')}
        className={`eap ${PAGE_WRAP} pt-14 md:pt-20 pb-16 md:pb-24`}
      >
        <div className="max-w-[60rem]">
          <h1 className="v4-h1">Early access sign up</h1>
          <p className="mt-6 md:mt-8 max-w-[42rem] v4-body text-ink-2">
            Get started with BAML.{' '}
            <a className="eap-link" href="/explore">
              Explore on your own
            </a>
            , or join a live session to go deeper with us in real time: ask
            questions, see the latest features, and work through your actual use
            case together.{' '}
            <span className="text-muted">About 45 minutes on Zoom.</span>
          </p>

          {events.length > 0 ? (
            <EapSchedule discord={discordCta} events={events} />
          ) : (
            <div className="max-w-[440px]">
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
            </div>
          )}

          <p className="mt-12 pt-6 border-t border-rule text-[15px] text-body">
            See every session on the{' '}
            <a
              className="eap-link"
              href={LUMA_CALENDAR_URL}
              rel="noopener noreferrer"
              target="_blank"
            >
              BAML Luma calendar
            </a>
            .
          </p>
        </div>
      </section>
    </PageShell>
  );
}
