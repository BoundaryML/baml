import type { Metadata } from 'next';
import type { ReactNode } from 'react';
import Baml from '@/components/boundary/baml';
import Footer from '@/components/boundary/footer';
import GetStarted from '@/components/boundary/get-started';
import Hero from '@/components/boundary/hero';
import Nav from '@/components/boundary/nav';
import { CtaBand } from '@/components/boundary/page-parts';
import PageTransition from '@/components/boundary/page-transition';
import Toolchain from '@/components/boundary/toolchain';
import { SectionViews } from '@/components/boundary/track';
import Workflows from '@/components/boundary/workflows';

// Version 3 of the homepage, kept as it was when /v4 (app/(home)/page.tsx) became `/`. Its copy is in
// copy/v3.md.
export const metadata: Metadata = {
  robots: { follow: true, index: false },
  title: { absolute: 'Boundary | Agentic Observability (v3)' },
};

/** Names a homepage section for the `boundary_section_viewed` event, without adding a box to the layout. */
function Section({ name, children }: { name: string; children: ReactNode }) {
  return (
    <div className="contents" data-section={name}>
      {children}
    </div>
  );
}

export default function Home() {
  return (
    <>
      <Nav />
      <PageTransition id={'v3'}>
        <main className="flex-1">
          <Section name="hero">
            <Hero />
          </Section>
          <Section name="workflows">
            <Workflows />
          </Section>
          <Section name="baml">
            <Baml />
          </Section>
          <Section name="get_started">
            <GetStarted />
          </Section>
          <Section name="toolchain">
            <Toolchain />
          </Section>
          <Section name="cta">
            <CtaBand />
          </Section>
        </main>
      </PageTransition>
      <Section name="footer">
        <Footer />
      </Section>
      <SectionViews />
    </>
  );
}
