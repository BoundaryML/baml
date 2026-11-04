import type { Metadata } from 'next';
import type { ReactNode } from 'react';
import PageTransition from '@/components/boundary/page-transition';
import { SectionViews } from '@/components/boundary/track';
import Baml from '@/components/boundary/v1/baml';
import { CtaBand } from '@/components/boundary/v1/cta-band';
import Footer from '@/components/boundary/v1/footer';
import GetStarted from '@/components/boundary/v1/get-started';
import Hero from '@/components/boundary/v1/hero';
import Nav from '@/components/boundary/v1/nav';
import Toolchain from '@/components/boundary/v1/toolchain';
import Workflows from '@/components/boundary/v1/workflows';

// Version 1 of the homepage, kept so people can look back at it. Its copy and layout live in
// components/boundary/v1 and copy/v1.md, and neither should change. Work on the next version happens in
// the components that app/(home)/page.tsx uses, and in copy/home.md.
export const metadata: Metadata = {
  robots: { follow: true, index: false },
  title: { absolute: 'Boundary | Agentic Observability (v1)' },
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
      <PageTransition id={'home-v1'}>
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
