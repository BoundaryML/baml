import Image from 'next/image';
import { notFound } from 'next/navigation';
import { createMetadata } from '@/app/_lib/metadata';
import { section } from '@/components/home/analytics';
import { PAGE_WRAP, PageShell } from '@/components/home/page-shell';
import { getEapEvents } from '@/lib/luma';
import SignupForm from '../signup-form';
import { submitSignup } from './action';

export const metadata = createMetadata({
  description:
    'Join weekly live onboarding or request an engineering session for your team.',
  ogTitle: 'Get started with Boundary',
  path: '/eap',
  title: 'Early Access',
});

export const revalidate = 3600;

export default async function EapPage({
  searchParams,
}: {
  searchParams: Promise<{ state?: string; zone?: string }>;
}) {
  if (process.env.NODE_ENV !== 'development') notFound();
  const { state, zone } = await searchParams;
  const [live] =
    state === 'join' || state === 'empty' ? [] : await getEapEvents();
  const now = Date.now();
  const sessions =
    state === 'join'
      ? [
          { endsIn: 50, id: 'preview-live', startsIn: -10 },
          { endsIn: 70, id: 'preview-soon', startsIn: 10 },
          { endsIn: 10_140, id: 'preview-later', startsIn: 10_080 },
        ].map(({ id, startsIn, endsIn }) => ({
          end_at: new Date(now + endsIn * 60_000).toISOString(),
          goingCount: null,
          id,
          name: 'Boundary onboarding preview',
          registrationQuestions: [],
          start_at: new Date(now + startsIn * 60_000).toISOString(),
          timezone: 'America/Los_Angeles',
          url: '/eap/local-session-preview?state=join',
        }))
      : !live || state === 'empty'
        ? []
        : [
            live,
            ...[7, 14].map((days) => ({
              ...live,
              end_at: new Date(
                Date.parse(live.end_at) + days * 86400000,
              ).toISOString(),
              id: 'preview-' + days,
              start_at: new Date(
                Date.parse(live.start_at) + days * 86400000,
              ).toISOString(),
            })),
          ];
  return (
    <PageShell>
      <section
        {...section('eap_request')}
        aria-labelledby="request-access"
        className="relative isolate overflow-hidden"
      >
        <div
          aria-hidden
          className="absolute inset-x-0 top-0 -z-10 h-[34rem] overflow-hidden"
        >
          <Image
            alt=""
            className="object-cover object-[center_65%] opacity-55"
            fill
            sizes="100vw"
            src="/paint/baml-dusk-field.jpg"
          />
          <div className="absolute inset-0 bg-gradient-to-b from-ground/45 via-ground/55 to-ground" />
        </div>
        <div className={`${PAGE_WRAP} py-12 md:py-20`}>
          <div className="mx-auto max-w-[52rem]">
            <SignupForm
              previewZone={
                zone === 'tokyo'
                  ? 'Asia/Tokyo'
                  : zone === 'new-york'
                    ? 'America/New_York'
                    : undefined
              }
              sessions={sessions}
              signupAction={submitSignup}
            />
          </div>
        </div>
      </section>
    </PageShell>
  );
}
