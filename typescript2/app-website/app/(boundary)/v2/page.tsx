import type { Metadata } from 'next';
import type { ReactNode } from 'react';
import { SectionViews } from '@/components/boundary/track';
import Baml from '@/components/boundary/v2/baml';
import Footer from '@/components/boundary/v2/footer';
import ForceDark from '@/components/boundary/v2/force-dark';
import GetStarted from '@/components/boundary/v2/get-started';
import Hero from '@/components/boundary/v2/hero';
import Nav from '@/components/boundary/v2/nav';
import Toolchain from '@/components/boundary/v2/toolchain';
import Workflows from '@/components/boundary/v2/workflows';
import './v2.css';

// Version 2 of the homepage, a bolder direction to compare with /v1. Its copy is in copy/v2.md.
export const metadata: Metadata = {
  robots: { follow: true, index: false },
  title: { absolute: 'Boundary | Agentic Observability (v2)' },
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
    <div className="v2" data-theme="dark">
      <ForceDark />
      <Nav />
      <main>
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
      </main>
      <Section name="footer">
        <Footer />
      </Section>
      <SectionViews />
    </div>
  );
}
