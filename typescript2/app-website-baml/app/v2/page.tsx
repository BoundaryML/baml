import type { Metadata } from 'next';
import localFont from 'next/font/local';
import { SectionViews } from '@/components/track';
import BoundaryBand from '@/components/v2/boundary-band';
import Examples from '@/components/v2/examples';
import Faq from '@/components/v2/faq';
import Features from '@/components/v2/features';
import FinalCta from '@/components/v2/final-cta';
import Footer from '@/components/v2/footer';
import ForceDark from '@/components/v2/force-dark';
import Hero from '@/components/v2/hero';
import Logos from '@/components/v2/logos';
import Nav from '@/components/v2/nav';
import Observability from '@/components/v2/observability';
import Stack from '@/components/v2/stack';
import Walkthrough from '@/components/v2/walkthrough';
import './v2.css';

// Geist Pixel is BAML's voice in the brand, used here for the headline.
const pixel = localFont({
  display: 'swap',
  src: '../fonts/GeistPixel-Square.otf',
  variable: '--font-geist-pixel',
});

// Version 2 of the homepage, a bolder direction to compare with /v1. Its copy is in copy/v2.md.
export const metadata: Metadata = {
  robots: { follow: true, index: false },
  title: 'BAML | The programming language for agents (v2)',
};

export default function Home() {
  return (
    <div className={`v2 ${pixel.variable}`} data-theme="dark">
      <ForceDark />
      <Nav />
      <main id="main">
        <Hero />
        <Logos />
        <Observability />
        <Features />
        <Walkthrough />
        <Examples />
        <Stack />
        <BoundaryBand />
        <Faq />
        <FinalCta />
      </main>
      <Footer />
      <SectionViews />
    </div>
  );
}
