import { createMetadata } from '@/app/_lib/metadata';
import { FooterSection } from '@/components/footer-section';
import { Navbar } from '@/components/navbar';
import PledgeSection from './_components/PledgeSection';
import WarEpilogue from './_components/WarEpilogue';
import WarScene from './_components/WarScene';
import './war-on-slop.css';

export const metadata = createMetadata({
  description:
    'A standing legion against the tide of AI slop. Kill slop with clean constructs at the language level.',
  ogTitle: 'fight slop with slop',
  path: '/fight-slop-with-slop',
  title: 'War on Slop',
});

export default function Page() {
  return (
    <>
      <Navbar />
      <main className="war-on-slop w-full overflow-x-clip">
        <section className="relative z-10 px-6 pt-20 text-center sm:pt-28">
          <h1 className="text-4xl font-bold tracking-tight text-wos-ink sm:text-6xl">
            fight slop with slop
          </h1>
          <p className="tweet-font mt-3 text-sm text-wos-accent">
            by{' '}
            <a
              className="hover:underline"
              href="https://x.com/boundaryml"
              rel="noopener noreferrer"
              target="_blank"
            >
              @boundaryml
            </a>
          </p>
        </section>

        <WarScene />

        <WarEpilogue />

        <PledgeSection />
      </main>
      <FooterSection />
    </>
  );
}
