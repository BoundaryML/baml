import Image from 'next/image';
import { createMetadata } from '@/app/_lib/metadata';
import { section } from '@/components/home/analytics';
import { PAGE_WRAP, PageShell } from '@/components/home/page-shell';
import { getEapEvents } from '@/lib/luma';
import SignupForm from './signup-form';

export const metadata = createMetadata({
  description:
    'Join weekly live onboarding or request a private session for your team.',
  ogTitle: 'Get started with Boundary',
  path: '/eap',
  title: 'Early Access',
});

export const revalidate = 3600;

export default async function EapPage({
  searchParams,
}: {
  searchParams: Promise<{ session?: string }>;
}) {
  const [{ session }, sessions] = await Promise.all([
    searchParams,
    getEapEvents(),
  ]);
  const initialSessionId = sessions.find(({ id }) => id === session)?.id;
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
              initialSessionId={initialSessionId}
              key={initialSessionId ?? 'signup'}
              sessions={sessions}
            />
          </div>
        </div>
      </section>
    </PageShell>
  );
}
