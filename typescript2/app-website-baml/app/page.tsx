import BoundaryBand from '@/components/boundary-band';
import Faq from '@/components/faq';
import Features from '@/components/features';
import FinalCta from '@/components/final-cta';
import Flow from '@/components/flow';
import Footer from '@/components/footer';
import Hero from '@/components/hero';
import Logos from '@/components/logos';
import Nav from '@/components/nav';
import Observability from '@/components/observability';
import Stack from '@/components/stack';
import { SectionViews } from '@/components/track';
import Walkthrough from '@/components/walkthrough';

// Section order follows copy/home.md: what BAML is → who uses it → why telemetry belongs in the
// language → what's in it → how to adopt it → where it runs → where the recordings go → start.
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
