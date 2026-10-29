import BoundaryBand from '@/components/boundary-band';
import Faq from '@/components/faq';
import Features from '@/components/features';
import FinalCta from '@/components/final-cta';
import Footer from '@/components/footer';
import Hero from '@/components/hero';
import Logos from '@/components/logos';
import Nav from '@/components/nav';
import Proof from '@/components/proof';
import Stack from '@/components/stack';
import Status from '@/components/status';
import Toolchain from '@/components/toolchain';
import { SectionViews } from '@/components/track';
import Walkthrough from '@/components/walkthrough';
import Why from '@/components/why';

// Section order follows one narrative: who uses it → proof it's real → why a new language → what's
// in it → how an agent drives it → where it runs → how it feeds Boundary → status → start.
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
        <Logos />
        <Proof />
        <Why />
        <Features />
        <Walkthrough />
        <Toolchain />
        <Stack />
        <BoundaryBand />
        <Faq />
        <Status />
        <FinalCta />
      </main>
      <Footer />
      <SectionViews />
    </>
  );
}
