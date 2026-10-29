import type { ReactNode } from 'react';
import Baml from '@/components/boundary/baml';
import Faq from '@/components/boundary/faq';
import Footer from '@/components/boundary/footer';
import Hero from '@/components/boundary/hero';
import LogoWall from '@/components/boundary/logo-wall';
import Loop from '@/components/boundary/loop';
import Nav from '@/components/boundary/nav';
import PageTransition from '@/components/boundary/page-transition';
import Platform from '@/components/boundary/platform';
import { Enterprise, Pricing } from '@/components/boundary/pricing';
import { CaseStudy, Metrics, Testimonials } from '@/components/boundary/proof';
import Stack from '@/components/boundary/stack';
import { SectionViews } from '@/components/boundary/track';

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
      <PageTransition id={'home'}>
        <main className="flex-1">
          <Section name="hero">
            <Hero />
          </Section>
          <Section name="logo_wall">
            <LogoWall />
          </Section>
          <Section name="stack">
            <Stack />
          </Section>
          <Section name="loop">
            <Loop />
          </Section>
          <Section name="platform">
            <Platform />
          </Section>
          <Section name="baml">
            <Baml />
          </Section>
          <Section name="metrics">
            <Metrics />
          </Section>
          <Section name="testimonials">
            <Testimonials />
          </Section>
          <Section name="case_study">
            <CaseStudy />
          </Section>
          <Section name="pricing">
            <Pricing />
          </Section>
          <Section name="enterprise">
            <Enterprise />
          </Section>
          <Section name="faq">
            <Faq />
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
