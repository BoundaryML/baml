import type { Metadata } from 'next';
import { SectionViews } from '@/components/track';
import BoundaryBand from '@/components/v1/boundary-band';
import Faq from '@/components/v1/faq';
import Features from '@/components/v1/features';
import FinalCta from '@/components/v1/final-cta';
import Flow from '@/components/v1/flow';
import Footer from '@/components/v1/footer';
import Hero from '@/components/v1/hero';
import Logos from '@/components/v1/logos';
import Nav from '@/components/v1/nav';
import Observability from '@/components/v1/observability';
import Stack from '@/components/v1/stack';
import Walkthrough from '@/components/v1/walkthrough';

// Version 1 of the homepage, kept so people can look back at it. Its copy and layout live in
// components/v1 and copy/v1.md, and neither should change. Work on the next version happens in the
// components that app/page.tsx uses, and in copy/home.md.
export const metadata: Metadata = {
  robots: { follow: true, index: false },
  title: 'BAML | The programming language for agents (v1)',
};

export default function Home() {
  return (
    <>
      <a
        className="sr-only focus:not-sr-only focus:fixed focus:top-3 focus:left-3 focus:z-50 focus:px-4 focus:py-2 focus:rounded-md focus:bg-accent focus:text-on-accent"
        href="#main"
      >
        Skip to content
      </a>
      <Nav />
      <main className="flex-1" id="main">
        <Hero />
        <Flow />
        <Logos />
        <Observability />
        <Features />
        <Walkthrough />
        <Stack />
        <BoundaryBand />
        <Faq />
        <FinalCta />
      </main>
      <Footer />
      <SectionViews />
    </>
  );
}
